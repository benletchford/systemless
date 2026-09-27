//! Collection Manager (`ColMgrLib`) PowerPC import adapter.

use super::*;
use crate::collection_manager::{
    CollectionItem, COLLECTION_INDEX_RANGE_ERR, COLLECTION_ITEM_NOT_FOUND_ERR,
    COLLECTION_VERSION_ERR,
};

pub(super) struct PpcCollectionDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) collections: &'a SharedProcessCollectionManager,
    pub(super) callback_stack: &'a mut Vec<PpcCollectionCallbackState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PpcCollectionCallbackKind {
    Flatten {
        collection: u32,
    },
    Exception {
        collection: u32,
        error: i16,
    },
    Unflatten {
        collection: u32,
        bytes: Vec<u8>,
        phase: PpcUnflattenPhase,
        remaining_items: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PpcUnflattenPhase {
    Header,
    ItemHeader,
    ItemData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcCollectionCallbackState {
    import_pc: u32,
    final_pc: u32,
    restore_rtoc: u32,
    target: PpcCallbackTarget,
    refcon: u32,
    data_ptr: u32,
    requested_size: u32,
    kind: PpcCollectionCallbackKind,
}

fn ppc_collection_call_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    state: &PpcCollectionCallbackState,
) -> Option<PpcImportAction> {
    let arguments: Vec<u32> = match state.kind {
        PpcCollectionCallbackKind::Exception { collection, error } => {
            vec![collection, error as i32 as u32]
        }
        _ => vec![state.requested_size, state.data_ptr, state.refcon],
    };
    install_powerpc_call_arguments(cpu, memory, &arguments)?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: state.target.entry,
            rtoc: state.target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            cpu.pc,
            state.restore_rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}

fn ppc_collection_allocate_callback_buffer(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    size: u32,
) -> Option<u32> {
    let allocation_size = size.max(1);
    let pointer = ppc_heap_alloc(memory, heap_cursor, heap_limit, allocation_size, true);
    (pointer != 0).then_some(pointer)
}

fn ppc_collection_finish_callback(
    cpu: &mut PpcCpu,
    state: PpcCollectionCallbackState,
    error: i16,
) -> PpcImportAction {
    cpu.lr = state.final_pc;
    cpu.gpr[2] = state.restore_rtoc;
    PpcImportAction::Return(ppc_i16_result(error))
}

fn ppc_collection_resume_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    collections: &SharedProcessCollectionManager,
    callback_stack: &mut Vec<PpcCollectionCallbackState>,
) -> Option<PpcImportAction> {
    if !(cpu.lr == cpu.pc
        && callback_stack
            .last()
            .is_some_and(|state| state.import_pc == cpu.pc))
    {
        return None;
    }
    let callback_error = cpu.gpr[3] as u16 as i16;
    if matches!(
        callback_stack.last().unwrap().kind,
        PpcCollectionCallbackKind::Exception { .. }
    ) {
        let state = callback_stack.pop().unwrap();
        return Some(ppc_collection_finish_callback(cpu, state, callback_error));
    }
    if callback_error != 0 {
        let state = callback_stack.pop().unwrap();
        return Some(ppc_collection_finish_callback_error(
            cpu,
            memory,
            collections,
            callback_stack,
            state,
            callback_error,
        ));
    }

    let state = callback_stack.last_mut().unwrap();
    match &mut state.kind {
        PpcCollectionCallbackKind::Exception { .. } => {
            unreachable!("exception callbacks are completed before resuming collection work")
        }
        PpcCollectionCallbackKind::Flatten { .. } => {
            let state = callback_stack.pop().unwrap();
            Some(ppc_collection_finish_callback(cpu, state, 0))
        }
        PpcCollectionCallbackKind::Unflatten {
            collection,
            bytes,
            phase,
            remaining_items,
        } => {
            let Some(block) = read_guest_bytes(memory, state.data_ptr, state.requested_size) else {
                let state = callback_stack.pop().unwrap();
                return Some(ppc_collection_finish_callback_error(
                    cpu,
                    memory,
                    collections,
                    callback_stack,
                    state,
                    PPC_PARAM_ERR,
                ));
            };
            bytes.extend_from_slice(&block);
            let next_size = match *phase {
                PpcUnflattenPhase::Header => {
                    if block.len() != 12
                        || block[..4] != *b"cltn"
                        || block[4..8] != 1u32.to_be_bytes()
                    {
                        let state = callback_stack.pop().unwrap();
                        return Some(ppc_collection_finish_callback_error(
                            cpu,
                            memory,
                            collections,
                            callback_stack,
                            state,
                            COLLECTION_VERSION_ERR,
                        ));
                    } else {
                        *remaining_items = u32::from_be_bytes(block[8..12].try_into().unwrap());
                        if *remaining_items == 0 {
                            0
                        } else {
                            *phase = PpcUnflattenPhase::ItemHeader;
                            16
                        }
                    }
                }
                PpcUnflattenPhase::ItemHeader => {
                    let size = block
                        .get(12..16)
                        .and_then(|value| value.try_into().ok())
                        .map(u32::from_be_bytes)
                        .unwrap_or(0);
                    if size == 0 {
                        *remaining_items = remaining_items.saturating_sub(1);
                        if *remaining_items == 0 {
                            0
                        } else {
                            16
                        }
                    } else {
                        *phase = PpcUnflattenPhase::ItemData;
                        size
                    }
                }
                PpcUnflattenPhase::ItemData => {
                    *remaining_items = remaining_items.saturating_sub(1);
                    if *remaining_items == 0 {
                        0
                    } else {
                        *phase = PpcUnflattenPhase::ItemHeader;
                        16
                    }
                }
            };
            if next_size == 0 {
                let error = collections.with_mut(|manager| manager.unflatten(*collection, bytes));
                let state = callback_stack.pop().unwrap();
                if error == 0 {
                    Some(ppc_collection_finish_callback(cpu, state, 0))
                } else {
                    Some(ppc_collection_finish_callback_error(
                        cpu,
                        memory,
                        collections,
                        callback_stack,
                        state,
                        error,
                    ))
                }
            } else {
                let Some(data_ptr) = ppc_collection_allocate_callback_buffer(
                    memory,
                    heap_cursor,
                    heap_limit,
                    next_size,
                ) else {
                    let state = callback_stack.pop().unwrap();
                    return Some(ppc_collection_finish_callback_error(
                        cpu,
                        memory,
                        collections,
                        callback_stack,
                        state,
                        PPC_MEM_FULL_ERR,
                    ));
                };
                let state = callback_stack.last_mut().unwrap();
                state.data_ptr = data_ptr;
                state.requested_size = next_size;
                match ppc_collection_call_callback(cpu, memory, state) {
                    Some(action) => Some(action),
                    None => {
                        let state = callback_stack.pop().unwrap();
                        Some(ppc_collection_finish_callback_error(
                            cpu,
                            memory,
                            collections,
                            callback_stack,
                            state,
                            PPC_PARAM_ERR,
                        ))
                    }
                }
            }
        }
    }
}

fn ppc_collection_error_action(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    collections: &SharedProcessCollectionManager,
    callback_stack: &mut Vec<PpcCollectionCallbackState>,
    collection: u32,
    error: i16,
) -> PpcImportAction {
    if error == 0 {
        return PpcImportAction::Return(0);
    }
    let exception_proc = collections.with_ref(|manager| manager.exception_proc(collection));
    let Some(target) = (exception_proc != 0)
        .then(|| ppc_resolve_callback_target(memory, exception_proc, cpu.gpr[2], None))
        .flatten()
    else {
        return PpcImportAction::Return(ppc_i16_result(error));
    };
    callback_stack.push(PpcCollectionCallbackState {
        import_pc: cpu.pc,
        final_pc: cpu.lr,
        restore_rtoc: cpu.gpr[2],
        target,
        refcon: 0,
        data_ptr: 0,
        requested_size: 0,
        kind: PpcCollectionCallbackKind::Exception { collection, error },
    });
    match ppc_collection_call_callback(cpu, memory, callback_stack.last().unwrap()) {
        Some(action) => action,
        None => {
            callback_stack.pop();
            PpcImportAction::Return(ppc_i16_result(error))
        }
    }
}

fn ppc_collection_finish_callback_error(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    collections: &SharedProcessCollectionManager,
    callback_stack: &mut Vec<PpcCollectionCallbackState>,
    state: PpcCollectionCallbackState,
    error: i16,
) -> PpcImportAction {
    let collection = match state.kind {
        PpcCollectionCallbackKind::Flatten { collection }
        | PpcCollectionCallbackKind::Exception { collection, .. }
        | PpcCollectionCallbackKind::Unflatten { collection, .. } => collection,
    };
    cpu.lr = state.final_pc;
    cpu.gpr[2] = state.restore_rtoc;
    ppc_collection_error_action(cpu, memory, collections, callback_stack, collection, error)
}

fn read_guest_bytes(memory: &mut PpcSectionMem, address: u32, size: u32) -> Option<Vec<u8>> {
    (0..size)
        .map(|offset| memory.read_u8(address + offset))
        .collect()
}

fn handle_bytes(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    handle: u32,
) -> Option<Vec<u8>> {
    let record = handles.iter().find(|record| record.handle == handle)?;
    read_guest_bytes(memory, record.ptr, record.size)
}

fn replace_handle_bytes(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut [PpcHandleRecord],
    handle: u32,
    bytes: &[u8],
) -> i16 {
    let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let error = ppc_set_handle_size(memory, heap_cursor, heap_limit, handles, handle, size);
    if error != 0 {
        *last_mem_error = error;
        return error;
    }
    let Some(record) = handles.iter().find(|record| record.handle == handle) else {
        *last_mem_error = PPC_NIL_HANDLE_ERR;
        return PPC_NIL_HANDLE_ERR;
    };
    let error = if memory.write_bytes(record.ptr, bytes).is_none() {
        PPC_PARAM_ERR
    } else {
        0
    };
    *last_mem_error = error;
    error
}

fn copy_item_to_guest(
    memory: &mut PpcSectionMem,
    item: Option<CollectionItem>,
    item_size_ptr: u32,
    item_data: u32,
    missing_error: i16,
) -> i16 {
    let Some(item) = item else {
        return missing_error;
    };
    let actual_size = u32::try_from(item.data.len()).unwrap_or(u32::MAX);
    let requested_size = if item_size_ptr == 0 {
        actual_size
    } else {
        memory.read_u32_be(item_size_ptr).unwrap_or(0)
    };
    if item_data != 0 {
        let copy_size = requested_size.min(actual_size) as usize;
        if memory
            .write_bytes(item_data, &item.data[..copy_size])
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    if item_size_ptr != 0 && memory.write_u32_be(item_size_ptr, actual_size).is_none() {
        return PPC_PARAM_ERR;
    }
    0
}

pub(super) fn dispatch_collection_import(
    context: PpcCollectionDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcImportDispatcherTarget::Collection(operation) = context.binding.dispatcher_target else {
        return None;
    };
    let PpcCollectionDispatchContext {
        cpu,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        collections,
        callback_stack,
        ..
    } = context;

    if let Some(action) = ppc_collection_resume_callback(
        cpu,
        memory,
        heap_cursor,
        heap_limit,
        collections,
        callback_stack,
    ) {
        return Some(action);
    }
    let argument_registers = cpu.gpr;
    let arg = |index: usize| argument_registers[3 + index];
    macro_rules! return_error {
        ($error:expr $(,)?) => {{
            let error = $error;
            ppc_collection_error_action(cpu, memory, collections, callback_stack, arg(0), error)
        }};
    }

    Some(match operation {
        PpcCollectionOperation::Version => PpcImportAction::Return(0x0100_0000),
        PpcCollectionOperation::New => {
            PpcImportAction::Return(collections.with_mut(|manager| manager.new_collection()))
        }
        PpcCollectionOperation::Dispose => {
            collections.with_mut(|manager| manager.dispose(arg(0)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::Clone => {
            PpcImportAction::Return(collections.with_mut(|manager| manager.clone_reference(arg(0))))
        }
        PpcCollectionOperation::CountOwners => PpcImportAction::Return(
            collections.with_ref(|manager| manager.owner_count(arg(0)) as u32),
        ),
        PpcCollectionOperation::Copy => PpcImportAction::Return(
            collections.with_mut(|manager| manager.copy_collection(arg(0), arg(1))),
        ),
        PpcCollectionOperation::GetDefaultAttributes => PpcImportAction::Return(
            collections.with_ref(|manager| manager.default_attributes(arg(0))),
        ),
        PpcCollectionOperation::SetDefaultAttributes => {
            collections.with_mut(|manager| manager.set_default_attributes(arg(0), arg(1), arg(2)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::CountItems => PpcImportAction::Return(
            collections.with_ref(|manager| manager.item_count(arg(0)) as u32),
        ),
        PpcCollectionOperation::AddItem => {
            let size = arg(3);
            let data = if size == 0 || arg(4) == 0 {
                Some(Vec::new())
            } else {
                read_guest_bytes(memory, arg(4), size)
            };
            return_error!(data.map_or(PPC_PARAM_ERR, |data| {
                collections
                    .with_mut(|manager| manager.add_item(arg(0), arg(1), arg(2) as i32, data))
            }))
        }
        PpcCollectionOperation::GetItem => {
            let item = collections.with_ref(|manager| {
                manager
                    .item_by_key(arg(0), arg(1), arg(2) as i32)
                    .map(|(_, item)| item.clone())
            });
            return_error!(copy_item_to_guest(
                memory,
                item,
                arg(3),
                arg(4),
                COLLECTION_ITEM_NOT_FOUND_ERR,
            ))
        }
        PpcCollectionOperation::RemoveItem => return_error!(
            collections.with_mut(|manager| manager.remove_key(arg(0), arg(1), arg(2) as i32)),
        ),
        PpcCollectionOperation::SetItemInfo => return_error!(collections.with_mut(|manager| {
            manager.set_key_attributes(arg(0), arg(1), arg(2) as i32, arg(3), arg(4))
        })),
        PpcCollectionOperation::GetItemInfo => {
            let item = collections.with_ref(|manager| {
                manager
                    .item_by_key(arg(0), arg(1), arg(2) as i32)
                    .map(|(index, item)| (index, item.clone()))
            });
            let Some((index, item)) = item else {
                return Some(return_error!(COLLECTION_ITEM_NOT_FOUND_ERR));
            };
            for (pointer, value) in [
                (arg(3), index as u32),
                (arg(4), item.data.len() as u32),
                (arg(5), item.attributes),
            ] {
                if pointer != 0 && memory.write_u32_be(pointer, value).is_none() {
                    return Some(return_error!(PPC_PARAM_ERR));
                }
            }
            return_error!(0)
        }
        PpcCollectionOperation::ReplaceIndexedItem => {
            let data = if arg(2) == 0 || arg(3) == 0 {
                Some(Vec::new())
            } else {
                read_guest_bytes(memory, arg(3), arg(2))
            };
            return_error!(data.map_or(PPC_PARAM_ERR, |data| {
                collections.with_mut(|manager| manager.replace_indexed(arg(0), arg(1) as i32, data))
            }))
        }
        PpcCollectionOperation::GetIndexedItem => {
            let item = collections
                .with_ref(|manager| manager.item_by_index(arg(0), arg(1) as i32).cloned());
            return_error!(copy_item_to_guest(
                memory,
                item,
                arg(2),
                arg(3),
                COLLECTION_INDEX_RANGE_ERR,
            ))
        }
        PpcCollectionOperation::RemoveIndexedItem => return_error!(
            collections.with_mut(|manager| manager.remove_indexed(arg(0), arg(1) as i32)),
        ),
        PpcCollectionOperation::SetIndexedItemInfo => {
            return_error!(collections.with_mut(|manager| {
                manager.set_indexed_attributes(arg(0), arg(1) as i32, arg(2), arg(3))
            }))
        }
        PpcCollectionOperation::GetIndexedItemInfo => {
            let item = collections
                .with_ref(|manager| manager.item_by_index(arg(0), arg(1) as i32).cloned());
            let Some(item) = item else {
                return Some(return_error!(COLLECTION_INDEX_RANGE_ERR));
            };
            for (pointer, value) in [
                (arg(2), item.tag),
                (arg(3), item.id as u32),
                (arg(4), item.data.len() as u32),
                (arg(5), item.attributes),
            ] {
                if pointer != 0 && memory.write_u32_be(pointer, value).is_none() {
                    return Some(return_error!(PPC_PARAM_ERR));
                }
            }
            return_error!(0)
        }
        PpcCollectionOperation::TagExists => PpcImportAction::Return(u32::from(
            collections.with_ref(|manager| manager.tag_exists(arg(0), arg(1))),
        )),
        PpcCollectionOperation::CountTags => PpcImportAction::Return(
            collections.with_ref(|manager| manager.tags(arg(0)).len() as u32),
        ),
        PpcCollectionOperation::GetIndexedTag => {
            let tag = collections.with_ref(|manager| {
                usize::try_from(arg(1) as i32 - 1)
                    .ok()
                    .and_then(|index| manager.tags(arg(0)).get(index).copied())
            });
            match tag {
                Some(tag) if arg(2) != 0 && memory.write_u32_be(arg(2), tag).is_some() => {
                    return_error!(0)
                }
                Some(_) if arg(2) == 0 => return_error!(0),
                Some(_) => return_error!(PPC_PARAM_ERR),
                None => return_error!(COLLECTION_INDEX_RANGE_ERR),
            }
        }
        PpcCollectionOperation::CountTaggedItems => PpcImportAction::Return(
            collections.with_ref(|manager| manager.tagged_count(arg(0), arg(1)) as u32),
        ),
        PpcCollectionOperation::GetTaggedItem => {
            let item = collections.with_ref(|manager| {
                manager
                    .tagged_item(arg(0), arg(1), arg(2) as i32)
                    .map(|(_, item)| item.clone())
            });
            return_error!(copy_item_to_guest(
                memory,
                item,
                arg(3),
                arg(4),
                COLLECTION_INDEX_RANGE_ERR,
            ))
        }
        PpcCollectionOperation::GetTaggedItemInfo => {
            let item = collections.with_ref(|manager| {
                manager
                    .tagged_item(arg(0), arg(1), arg(2) as i32)
                    .map(|(index, item)| (index, item.clone()))
            });
            let Some((index, item)) = item else {
                return Some(return_error!(COLLECTION_INDEX_RANGE_ERR));
            };
            for (pointer, value) in [
                (arg(3), item.id as u32),
                (arg(4), index as u32),
                (arg(5), item.data.len() as u32),
                (arg(6), item.attributes),
            ] {
                if pointer != 0 && memory.write_u32_be(pointer, value).is_none() {
                    return Some(return_error!(PPC_PARAM_ERR));
                }
            }
            return_error!(0)
        }
        PpcCollectionOperation::Purge => {
            collections.with_mut(|manager| manager.purge_matching(arg(0), arg(1), arg(2)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::PurgeTag => {
            collections.with_mut(|manager| manager.purge_tag(arg(0), arg(1)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::Empty => {
            collections.with_mut(|manager| manager.empty(arg(0)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::GetExceptionProc => {
            PpcImportAction::Return(collections.with_ref(|manager| manager.exception_proc(arg(0))))
        }
        PpcCollectionOperation::SetExceptionProc => {
            collections.with_mut(|manager| manager.set_exception_proc(arg(0), arg(1)));
            PpcImportAction::ReturnPreserve
        }
        PpcCollectionOperation::AddItemHandle => {
            let bytes = handle_bytes(memory, handles, arg(3));
            return_error!(bytes.map_or(PPC_NIL_HANDLE_ERR, |bytes| {
                collections
                    .with_mut(|manager| manager.add_item(arg(0), arg(1), arg(2) as i32, bytes))
            }))
        }
        PpcCollectionOperation::GetItemHandle => {
            let item = collections.with_ref(|manager| {
                manager
                    .item_by_key(arg(0), arg(1), arg(2) as i32)
                    .map(|(_, item)| item.data.clone())
            });
            return_error!(item.map_or(COLLECTION_ITEM_NOT_FOUND_ERR, |bytes| {
                if arg(3) == 0 {
                    0
                } else {
                    replace_handle_bytes(
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        arg(3),
                        &bytes,
                    )
                }
            }))
        }
        PpcCollectionOperation::ReplaceIndexedItemHandle => {
            let bytes = handle_bytes(memory, handles, arg(2));
            return_error!(bytes.map_or(PPC_NIL_HANDLE_ERR, |bytes| {
                collections
                    .with_mut(|manager| manager.replace_indexed(arg(0), arg(1) as i32, bytes))
            }))
        }
        PpcCollectionOperation::GetIndexedItemHandle => {
            let item = collections.with_ref(|manager| {
                manager
                    .item_by_index(arg(0), arg(1) as i32)
                    .map(|item| item.data.clone())
            });
            return_error!(item.map_or(COLLECTION_INDEX_RANGE_ERR, |bytes| {
                replace_handle_bytes(
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    arg(2),
                    &bytes,
                )
            }))
        }
        PpcCollectionOperation::FlattenToHandle => {
            let bytes = collections.with_ref(|manager| manager.flatten(arg(0), None));
            return_error!(bytes.map_or(PPC_PARAM_ERR, |bytes| {
                replace_handle_bytes(
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    arg(1),
                    &bytes,
                )
            }))
        }
        PpcCollectionOperation::UnflattenFromHandle => {
            let bytes = handle_bytes(memory, handles, arg(1));
            return_error!(bytes.map_or(PPC_NIL_HANDLE_ERR, |bytes| {
                collections.with_mut(|manager| manager.unflatten(arg(0), &bytes))
            }))
        }
        PpcCollectionOperation::Flatten | PpcCollectionOperation::FlattenPartial => {
            let filter =
                (operation == PpcCollectionOperation::FlattenPartial).then(|| (arg(3), arg(4)));
            let bytes = collections.with_ref(|manager| manager.flatten(arg(0), filter));
            let Some(bytes) = bytes else {
                return Some(return_error!(PPC_PARAM_ERR));
            };
            let Some(target) = ppc_resolve_callback_target(memory, arg(1), cpu.gpr[2], None) else {
                return Some(return_error!(PPC_PARAM_ERR));
            };
            let Some(data_ptr) = ppc_collection_allocate_callback_buffer(
                memory,
                heap_cursor,
                heap_limit,
                bytes.len() as u32,
            ) else {
                return Some(return_error!(PPC_MEM_FULL_ERR));
            };
            if memory.write_bytes(data_ptr, &bytes).is_none() {
                return Some(return_error!(PPC_PARAM_ERR));
            }
            callback_stack.push(PpcCollectionCallbackState {
                import_pc: cpu.pc,
                final_pc: cpu.lr,
                restore_rtoc: cpu.gpr[2],
                target,
                refcon: arg(2),
                data_ptr,
                requested_size: bytes.len() as u32,
                kind: PpcCollectionCallbackKind::Flatten { collection: arg(0) },
            });
            match ppc_collection_call_callback(cpu, memory, callback_stack.last().unwrap()) {
                Some(action) => action,
                None => {
                    let state = callback_stack.pop().unwrap();
                    ppc_collection_finish_callback_error(
                        cpu,
                        memory,
                        collections,
                        callback_stack,
                        state,
                        PPC_PARAM_ERR,
                    )
                }
            }
        }
        PpcCollectionOperation::Unflatten => {
            let Some(target) = ppc_resolve_callback_target(memory, arg(1), cpu.gpr[2], None) else {
                return Some(return_error!(PPC_PARAM_ERR));
            };
            let Some(data_ptr) =
                ppc_collection_allocate_callback_buffer(memory, heap_cursor, heap_limit, 12)
            else {
                return Some(return_error!(PPC_MEM_FULL_ERR));
            };
            callback_stack.push(PpcCollectionCallbackState {
                import_pc: cpu.pc,
                final_pc: cpu.lr,
                restore_rtoc: cpu.gpr[2],
                target,
                refcon: arg(2),
                data_ptr,
                requested_size: 12,
                kind: PpcCollectionCallbackKind::Unflatten {
                    collection: arg(0),
                    bytes: Vec::new(),
                    phase: PpcUnflattenPhase::Header,
                    remaining_items: 0,
                },
            });
            match ppc_collection_call_callback(cpu, memory, callback_stack.last().unwrap()) {
                Some(action) => action,
                None => {
                    let state = callback_stack.pop().unwrap();
                    ppc_collection_finish_callback_error(
                        cpu,
                        memory,
                        collections,
                        callback_stack,
                        state,
                        PPC_PARAM_ERR,
                    )
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colmgrlib_exports_map_to_every_collection_operation() {
        let mappings = [
            (
                "getCollectionMgrLibVersion",
                PpcCollectionOperation::Version,
            ),
            ("NewCollection", PpcCollectionOperation::New),
            ("DisposeCollection", PpcCollectionOperation::Dispose),
            ("CloneCollection", PpcCollectionOperation::Clone),
            ("CountCollectionOwners", PpcCollectionOperation::CountOwners),
            ("CopyCollection", PpcCollectionOperation::Copy),
            (
                "GetCollectionDefaultAttributes",
                PpcCollectionOperation::GetDefaultAttributes,
            ),
            (
                "SetCollectionDefaultAttributes",
                PpcCollectionOperation::SetDefaultAttributes,
            ),
            ("CountCollectionItems", PpcCollectionOperation::CountItems),
            ("AddCollectionItem", PpcCollectionOperation::AddItem),
            ("GetCollectionItem", PpcCollectionOperation::GetItem),
            ("RemoveCollectionItem", PpcCollectionOperation::RemoveItem),
            ("SetCollectionItemInfo", PpcCollectionOperation::SetItemInfo),
            ("GetCollectionItemInfo", PpcCollectionOperation::GetItemInfo),
            (
                "ReplaceIndexedCollectionItem",
                PpcCollectionOperation::ReplaceIndexedItem,
            ),
            (
                "GetIndexedCollectionItem",
                PpcCollectionOperation::GetIndexedItem,
            ),
            (
                "RemoveIndexedCollectionItem",
                PpcCollectionOperation::RemoveIndexedItem,
            ),
            (
                "SetIndexedCollectionItemInfo",
                PpcCollectionOperation::SetIndexedItemInfo,
            ),
            (
                "GetIndexedCollectionItemInfo",
                PpcCollectionOperation::GetIndexedItemInfo,
            ),
            ("CollectionTagExists", PpcCollectionOperation::TagExists),
            ("CountCollectionTags", PpcCollectionOperation::CountTags),
            (
                "GetIndexedCollectionTag",
                PpcCollectionOperation::GetIndexedTag,
            ),
            (
                "CountTaggedCollectionItems",
                PpcCollectionOperation::CountTaggedItems,
            ),
            (
                "GetTaggedCollectionItem",
                PpcCollectionOperation::GetTaggedItem,
            ),
            (
                "GetTaggedCollectionItemInfo",
                PpcCollectionOperation::GetTaggedItemInfo,
            ),
            ("PurgeCollection", PpcCollectionOperation::Purge),
            ("PurgeCollectionTag", PpcCollectionOperation::PurgeTag),
            ("EmptyCollection", PpcCollectionOperation::Empty),
            ("FlattenCollection", PpcCollectionOperation::Flatten),
            (
                "FlattenPartialCollection",
                PpcCollectionOperation::FlattenPartial,
            ),
            ("UnflattenCollection", PpcCollectionOperation::Unflatten),
            (
                "GetCollectionExceptionProc",
                PpcCollectionOperation::GetExceptionProc,
            ),
            (
                "SetCollectionExceptionProc",
                PpcCollectionOperation::SetExceptionProc,
            ),
            (
                "AddCollectionItemHdl",
                PpcCollectionOperation::AddItemHandle,
            ),
            (
                "GetCollectionItemHdl",
                PpcCollectionOperation::GetItemHandle,
            ),
            (
                "ReplaceIndexedCollectionItemHdl",
                PpcCollectionOperation::ReplaceIndexedItemHandle,
            ),
            (
                "GetIndexedCollectionItemHdl",
                PpcCollectionOperation::GetIndexedItemHandle,
            ),
            (
                "FlattenCollectionToHdl",
                PpcCollectionOperation::FlattenToHandle,
            ),
            (
                "UnflattenCollectionFromHdl",
                PpcCollectionOperation::UnflattenFromHandle,
            ),
        ];

        assert_eq!(mappings.len(), 39);
        for (symbol, operation) in mappings {
            assert_eq!(
                dispatcher_target_for_import("ColMgrLib", symbol),
                PpcImportDispatcherTarget::Collection(operation),
                "unexpected ColMgrLib mapping for {symbol}"
            );
        }
        assert_eq!(
            dispatcher_target_for_import("ColMgrLib", "NotACollectionRoutine"),
            PpcImportDispatcherTarget::Unsupported
        );
    }
}
