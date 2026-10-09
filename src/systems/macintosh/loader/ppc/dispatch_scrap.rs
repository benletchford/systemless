//! Typed Scrap Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcScrapDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) scrap: &'a mut PpcScrapState,
}

pub(super) fn dispatch_scrap_import(
    context: PpcScrapDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcScrapDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        scrap,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InfoScrap => {
            // Inside Macintosh I, I-457: ScrapStuff is a 16-byte record.
            let ptr = scrap.desktop.ensure_stuff_ptr(|| {
                process_memory_manager.new_native_ptr(memory, crate::scrap_manager::SCRAP_STUFF_RECORD_SIZE as u32, true)
            });
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if ptr == 0 {
                return Some(PpcImportAction::Return(0));
            }
            let summary = scrap.desktop.summary();
            let handle = if summary.in_memory {
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                let handle = scrap.desktop.ensure_handle(|| {
                    allocator.allocate_handle(memory, heap_cursor, last_mem_error, handles, 0, true)
                });
                if handle != 0 && summary.handle_dirty {
                    let bytes = scrap.desktop.serialized_entries();
                    if allocator.resize_handle(
                        memory,
                        heap_cursor,
                        last_mem_error,
                        handles,
                        handle,
                        bytes.len() as u32,
                    ) == PPC_NO_ERR
                    {
                        let data_ptr = memory.read_u32_be(handle).unwrap_or(0);
                        if bytes.is_empty() || memory.write_bytes(data_ptr, &bytes).is_some() {
                            scrap.desktop.mark_handle_clean();
                        }
                    }
                }
                handle
            } else {
                0
            };
            let record = crate::scrap_manager::evaluate_scrap_stuff_record(
                summary.serialized_size,
                handle,
                summary.count as u16,
                summary.in_memory,
            );
            let _ = memory.write_u32_be(ptr, record.scrap_size);
            let _ = memory.write_u32_be(ptr + 4, record.scrap_handle);
            let _ = memory.write_u16_be(ptr + 8, record.scrap_count);
            let _ = memory.write_u16_be(ptr + 10, record.scrap_state as u16);
            let _ = memory.write_u32_be(ptr + 12, record.scrap_name);
            Some(PpcImportAction::Return(ptr))
        }
        PpcImportDispatcherTarget::GetScrap => {
            // Inside Macintosh: More Macintosh Toolbox (1993), pp. 2-38--2-40:
            // return the first matching flavor, resize the destination handle,
            // and report its offset in the ordered desktop scrap.
            let params = crate::scrap_manager::evaluate_get_scrap_parameters(
                cpu.gpr[3],
                cpu.gpr[4].to_be_bytes(),
                cpu.gpr[5],
            );
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            Some(PpcImportAction::Return(ppc_get_scrap(
                &params,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                scrap,
            )))
        }
        PpcImportDispatcherTarget::PutScrap => {
            // More Macintosh Toolbox (1993), pp. 2-35--2-37: successive calls
            // add ordered flavors; a repeated type remains a later occurrence.
            let params = crate::scrap_manager::evaluate_put_scrap_parameters(
                cpu.gpr[3] as i32,
                cpu.gpr[4].to_be_bytes(),
                cpu.gpr[5],
            );
            let result = if !scrap.desktop.summary().initialized {
                crate::scrap_manager::NO_SCRAP_ERR
            } else if !params.is_valid() {
                crate::scrap_manager::PARAM_ERR
            } else if let Some(bytes) =
                ppc_memory_read_bytes(memory, params.source_ptr, params.length as u32)
            {
                scrap.desktop.append_entry(params.flavor_type, bytes);
                crate::scrap_manager::NO_ERR
            } else {
                crate::scrap_manager::PARAM_ERR
            };
            Some(PpcImportAction::Return((i32::from(result)) as u32))
        }
        PpcImportDispatcherTarget::ZeroScrap => {
            let _action = crate::scrap_manager::evaluate_zero_scrap();
            scrap.desktop.zero();
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::LoadScrap => {
            // The process-owned desktop scrap remains resident in HLE, so an
            // explicit disk-to-memory synchronization is already satisfied.
            let _action = crate::scrap_manager::evaluate_load_scrap();
            scrap.desktop.load();
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::UnloadScrap => {
            // UnloadScrap (_UnlodeScrap, $A9FA)
            // Writes the desk scrap to disk and releases resident memory.
            // FUNCTION UnloadScrap: LONGINT;
            // Inside Macintosh Volume I (1985), p. I-458.
            let _action = crate::scrap_manager::evaluate_unload_scrap();
            let result = match scrap.desktop.unload() {
                Err(()) => u32::MAX,
                Ok(Some(handle)) => {
                    let _ = ppc_dispose_process_native_handle(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        handle,
                    );
                    0
                }
                Ok(None) => 0,
            };
            Some(PpcImportAction::Return(result))
        }
        _ => None,
    }
}

fn ppc_get_scrap(
    params: &crate::scrap_manager::GetScrapParameters,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    scrap: &PpcScrapState,
) -> u32 {
    let Some(flavor) = scrap.desktop.flavor(params.flavor_type) else {
        if params.offset_ptr != 0 {
            let _ = memory.write_u32_be(params.offset_ptr, 0);
        }
        return (i32::from(crate::scrap_manager::NO_TYPE_ERR)) as u32;
    };
    if params.offset_ptr != 0 {
        let _ = memory.write_u32_be(params.offset_ptr, flavor.payload_offset);
    }
    if !params.is_query_only() {
        let result = ppc_allocator_view_resize_handle(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            params.destination_handle,
            flavor.data.len() as u32,
        );
        if result != PPC_NO_ERR {
            return (i32::from(result)) as u32;
        }
        if !flavor.data.is_empty() {
            let Some(ptr) = memory.read_u32_be(params.destination_handle).filter(|ptr| *ptr != 0) else {
                return (i32::from(crate::scrap_manager::PARAM_ERR)) as u32;
            };
            if memory.write_bytes(ptr, &flavor.data).is_none() {
                return (i32::from(crate::scrap_manager::PARAM_ERR)) as u32;
            }
        }
    }
    flavor.data.len() as u32
}
