//! Collection Manager (`_CollectionMgr`, `$ABF6`) 68K adapter.

use crate::collection_manager::{
    CollectionItem, COLLECTION_INDEX_RANGE_ERR, COLLECTION_ITEM_NOT_FOUND_ERR,
};
use crate::cpu::{CpuOps, Register};
use crate::guest_call::{GuestCallTarget, M68kResultTarget, PowerPcArguments};
use crate::guest_procedure::{resolve_guest_procedure, GuestIsa, GuestProcedure};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::Result;

use super::dispatch::TrapDispatcher;

#[derive(Clone, Debug)]
pub(crate) enum CollectionCallbackKind {
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
        phase: CollectionUnflattenPhase,
        remaining_items: u32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CollectionUnflattenPhase {
    Header,
    ItemHeader,
    ItemData,
}

#[derive(Clone, Debug)]
pub(crate) struct CollectionCallbackState {
    pub(crate) return_pc: u32,
    pub(crate) result_sp: u32,
    pub(crate) procedure: GuestProcedure,
    pub(crate) refcon: u32,
    pub(crate) data_ptr: u32,
    pub(crate) requested_size: u32,
    pub(crate) kind: CollectionCallbackKind,
}

impl TrapDispatcher {
    fn collection_callback_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.collection_callback_trampoline == 0 {
            let address = bus.alloc(8);
            bus.write_word(address, 0x303C); // MOVE.W #$70FE,D0
            bus.write_word(address + 2, 0x70FE);
            bus.write_word(address + 4, 0xABF6); // _CollectionMgr
            self.collection_callback_trampoline = address;
        }
        self.collection_callback_trampoline
    }

    fn collection_call_guest<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) -> bool {
        let Some(state) = self.collection_callback_stack.last().cloned() else {
            return false;
        };
        let trampoline = self.collection_callback_trampoline(bus);
        match state.procedure.isa {
            GuestIsa::M68k => {
                let frame = match state.kind {
                    CollectionCallbackKind::Exception { collection, error } => {
                        let frame = state.result_sp.wrapping_sub(10);
                        bus.write_long(frame, trampoline);
                        bus.write_word(frame + 4, error as u16);
                        bus.write_long(frame + 6, collection);
                        frame
                    }
                    _ => {
                        let frame = state.result_sp.wrapping_sub(16);
                        bus.write_long(frame, trampoline);
                        bus.write_long(frame + 4, state.refcon);
                        bus.write_long(frame + 8, state.data_ptr);
                        bus.write_long(frame + 12, state.requested_size);
                        frame
                    }
                };
                cpu.write_reg(Register::A7, frame);
                cpu.write_reg(Register::PC, state.procedure.entry);
                true
            }
            GuestIsa::PowerPc => {
                let arguments = match state.kind {
                    CollectionCallbackKind::Exception { collection, error } => {
                        PowerPcArguments::from_slice(&[collection, error as i32 as u32])
                    }
                    _ => PowerPcArguments::from_slice(&[
                        state.requested_size,
                        state.data_ptr,
                        state.refcon,
                    ]),
                }
                .expect("Collection callback arguments fit the native ABI");
                self.guest_calls.begin_m68k_to_powerpc(
                    GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: state.procedure.entry,
                        rtoc: state.procedure.rtoc,
                    },
                    arguments,
                    trampoline,
                    state.result_sp,
                    Some(M68kResultTarget::Memory {
                        address: state.result_sp,
                        size: 2,
                    }),
                )
            }
        }
    }

    fn collection_finish_callback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        error: i16,
    ) -> Result<()> {
        let state = self.collection_callback_stack.pop().unwrap();
        if state.data_ptr != 0 {
            self.dispose_process_ptr(bus, state.data_ptr);
        }
        bus.write_word(state.result_sp, error as u16);
        cpu.write_reg(Register::A7, state.result_sp);
        cpu.write_reg(Register::D0, error as i32 as u32);
        cpu.write_reg(Register::PC, state.return_pc);
        Ok(())
    }

    fn collection_callback_collection(kind: &CollectionCallbackKind) -> u32 {
        match *kind {
            CollectionCallbackKind::Flatten { collection }
            | CollectionCallbackKind::Exception { collection, .. }
            | CollectionCallbackKind::Unflatten { collection, .. } => collection,
        }
    }

    fn collection_return_error<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        collection: u32,
        error: i16,
        result_sp: u32,
        return_pc: u32,
    ) -> Result<()> {
        let exception_proc = self
            .collections
            .with_ref(|manager| manager.exception_proc(collection));
        let procedure = (error != 0 && exception_proc != 0)
            .then(|| {
                resolve_guest_procedure(
                    bus,
                    exception_proc,
                    0,
                    None,
                    GuestIsa::M68k,
                    GuestIsa::M68k,
                )
            })
            .flatten();
        if let Some(procedure) = procedure {
            self.collection_callback_stack
                .push(CollectionCallbackState {
                    return_pc,
                    result_sp,
                    procedure,
                    refcon: 0,
                    data_ptr: 0,
                    requested_size: 0,
                    kind: CollectionCallbackKind::Exception { collection, error },
                });
            if self.collection_call_guest(cpu, bus) {
                return Ok(());
            }
            self.collection_callback_stack.pop();
        }
        bus.write_word(result_sp, error as u16);
        cpu.write_reg(Register::A7, result_sp);
        cpu.write_reg(Register::D0, error as i32 as u32);
        cpu.write_reg(Register::PC, return_pc);
        Ok(())
    }

    fn collection_finish_callback_error<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        error: i16,
    ) -> Result<()> {
        let state = self.collection_callback_stack.pop().unwrap();
        let collection = Self::collection_callback_collection(&state.kind);
        if state.data_ptr != 0 {
            self.dispose_process_ptr(bus, state.data_ptr);
        }
        self.collection_return_error(
            cpu,
            bus,
            collection,
            error,
            state.result_sp,
            state.return_pc,
        )
    }

    fn collection_resume_callback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Result<()> {
        let result_sp = cpu.read_reg(Register::A7);
        let callback_error = bus.read_word(result_sp) as i16;
        if callback_error != 0 {
            if matches!(
                self.collection_callback_stack.last().unwrap().kind,
                CollectionCallbackKind::Exception { .. }
            ) {
                return self.collection_finish_callback(cpu, bus, callback_error);
            }
            return self.collection_finish_callback_error(cpu, bus, callback_error);
        }
        if matches!(
            self.collection_callback_stack.last().unwrap().kind,
            CollectionCallbackKind::Flatten { .. } | CollectionCallbackKind::Exception { .. }
        ) {
            return self.collection_finish_callback(cpu, bus, 0);
        }
        let (next_size, old_data_ptr, finished, invalid) = {
            let state = self.collection_callback_stack.last_mut().unwrap();
            let CollectionCallbackKind::Unflatten {
                collection,
                bytes,
                phase,
                remaining_items,
            } = &mut state.kind
            else {
                unreachable!()
            };
            let block = Self::collection_read_bytes(bus, state.data_ptr, state.requested_size);
            bytes.extend_from_slice(&block);
            let mut invalid = false;
            let next_size = match *phase {
                CollectionUnflattenPhase::Header => {
                    if block.len() != 12
                        || block[..4] != *b"cltn"
                        || block[4..8] != 1u32.to_be_bytes()
                    {
                        invalid = true;
                        0
                    } else {
                        *remaining_items = u32::from_be_bytes(block[8..12].try_into().unwrap());
                        if *remaining_items == 0 {
                            0
                        } else {
                            *phase = CollectionUnflattenPhase::ItemHeader;
                            16
                        }
                    }
                }
                CollectionUnflattenPhase::ItemHeader => {
                    let size = u32::from_be_bytes(block[12..16].try_into().unwrap());
                    if size == 0 {
                        *remaining_items = remaining_items.saturating_sub(1);
                        if *remaining_items == 0 {
                            0
                        } else {
                            16
                        }
                    } else {
                        *phase = CollectionUnflattenPhase::ItemData;
                        size
                    }
                }
                CollectionUnflattenPhase::ItemData => {
                    *remaining_items = remaining_items.saturating_sub(1);
                    if *remaining_items == 0 {
                        0
                    } else {
                        *phase = CollectionUnflattenPhase::ItemHeader;
                        16
                    }
                }
            };
            let finished = (next_size == 0 && !invalid).then(|| (*collection, bytes.clone()));
            (next_size, state.data_ptr, finished, invalid)
        };
        if invalid {
            return self.collection_finish_callback_error(cpu, bus, -5753);
        }
        if let Some((collection, bytes)) = finished {
            let error = self
                .collections
                .with_mut(|manager| manager.unflatten(collection, &bytes));
            return if error == 0 {
                self.collection_finish_callback(cpu, bus, 0)
            } else {
                self.collection_finish_callback_error(cpu, bus, error)
            };
        }
        self.dispose_process_ptr(bus, old_data_ptr);
        let data_ptr = self.new_process_classic_ptr(bus, next_size);
        if data_ptr == 0 {
            return self.collection_finish_callback_error(cpu, bus, -108);
        }
        let state = self.collection_callback_stack.last_mut().unwrap();
        state.data_ptr = data_ptr;
        state.requested_size = next_size;
        if self.collection_call_guest(cpu, bus) {
            Ok(())
        } else {
            self.collection_finish_callback_error(cpu, bus, -108)
        }
    }

    fn collection_begin_callback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        procedure_pointer: u32,
        refcon: u32,
        result_sp: u32,
        data: Option<&[u8]>,
        requested_size: u32,
        kind: CollectionCallbackKind,
    ) -> Result<()> {
        let procedure = match resolve_guest_procedure(
            bus,
            procedure_pointer,
            0,
            None,
            GuestIsa::M68k,
            GuestIsa::M68k,
        ) {
            Some(procedure) => procedure,
            _ => {
                let collection = Self::collection_callback_collection(&kind);
                return self.collection_return_error(
                    cpu,
                    bus,
                    collection,
                    -50,
                    result_sp,
                    cpu.read_reg(Register::PC),
                );
            }
        };
        let data_ptr = self.new_process_classic_ptr(bus, requested_size.max(1));
        if data_ptr == 0 {
            let collection = Self::collection_callback_collection(&kind);
            return self.collection_return_error(
                cpu,
                bus,
                collection,
                -108,
                result_sp,
                cpu.read_reg(Register::PC),
            );
        }
        if let Some(data) = data {
            for (offset, byte) in data.iter().enumerate() {
                bus.write_byte(data_ptr + offset as u32, *byte);
            }
        }
        self.collection_callback_stack
            .push(CollectionCallbackState {
                return_pc: cpu.read_reg(Register::PC),
                result_sp,
                procedure,
                refcon,
                data_ptr,
                requested_size,
                kind,
            });
        if self.collection_call_guest(cpu, bus) {
            Ok(())
        } else {
            self.collection_finish_callback(cpu, bus, -108)
        }
    }

    fn collection_finish_error<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        collection: u32,
        argument_bytes: u32,
        error: i16,
    ) -> Result<()> {
        let result_sp = cpu.read_reg(Register::A7) + argument_bytes;
        self.collection_return_error(
            cpu,
            bus,
            collection,
            error,
            result_sp,
            cpu.read_reg(Register::PC),
        )
    }
    fn collection_finish<C: CpuOps>(
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        argument_bytes: u32,
        result: Option<(u32, usize)>,
    ) -> Result<()> {
        let sp = cpu.read_reg(Register::A7);
        if let Some((value, size)) = result {
            match size {
                1 => bus.write_byte(sp + argument_bytes, value as u8),
                2 => bus.write_word(sp + argument_bytes, value as u16),
                4 => bus.write_long(sp + argument_bytes, value),
                _ => unreachable!(),
            }
            cpu.write_reg(Register::D0, value);
        }
        cpu.write_reg(Register::A7, sp + argument_bytes);
        Ok(())
    }

    fn collection_read_bytes(bus: &MacMemoryBus, address: u32, size: u32) -> Vec<u8> {
        (0..size)
            .map(|offset| bus.read_byte(address + offset))
            .collect()
    }

    fn collection_handle_bytes(&self, bus: &MacMemoryBus, handle: u32) -> Option<Vec<u8>> {
        if handle == 0 {
            return None;
        }
        let manager = self.process_memory_manager();
        let size = manager.borrow().process_handle_size(bus, handle)?;
        let pointer = bus.read_long(handle);
        (pointer != 0 || size == 0).then(|| Self::collection_read_bytes(bus, pointer, size))
    }

    fn collection_replace_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        bytes: &[u8],
    ) -> i16 {
        if handle == 0 {
            return -109;
        }
        let manager = self.process_memory_manager();
        let mut manager = manager.borrow_mut();
        manager.attach_classic_memory_bus(bus);
        manager.replace_process_handle_bytes(bus, handle, bytes)
    }

    fn collection_copy_item(
        bus: &mut MacMemoryBus,
        item: Option<CollectionItem>,
        item_size_ptr: u32,
        item_data: u32,
        missing_error: i16,
    ) -> i16 {
        let Some(item) = item else {
            return missing_error;
        };
        let actual_size = item.data.len() as u32;
        let requested_size = if item_size_ptr == 0 {
            actual_size
        } else {
            bus.read_long(item_size_ptr)
        };
        if item_data != 0 {
            for (offset, byte) in item
                .data
                .iter()
                .take(requested_size.min(actual_size) as usize)
                .enumerate()
            {
                bus.write_byte(item_data + offset as u32, *byte);
            }
        }
        if item_size_ptr != 0 {
            bus.write_long(item_size_ptr, actual_size);
        }
        0
    }

    pub(crate) fn dispatch_collection<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        if !is_tool || trap_num != 0x3F6 {
            return None;
        }
        let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
        if selector == 0x70FE && !self.collection_callback_stack.is_empty() {
            return Some(self.collection_resume_callback(cpu, bus));
        }
        if !(0x7000..=0x7026).contains(&selector) {
            cpu.write_reg(Register::D0, (-50i32) as u32);
            return Some(Ok(()));
        }
        let routine = selector - 0x7000;
        let sp = cpu.read_reg(Register::A7);
        let result = match routine {
            0x00 => {
                let collection = self
                    .collections
                    .with_mut(|manager| manager.new_collection());
                Self::collection_finish(cpu, bus, 0, Some((collection, 4)))
            }
            0x01 => {
                self.collections
                    .with_mut(|manager| manager.dispose(bus.read_long(sp)));
                Self::collection_finish(cpu, bus, 4, None)
            }
            0x02 => {
                let collection = self
                    .collections
                    .with_mut(|manager| manager.clone_reference(bus.read_long(sp)));
                Self::collection_finish(cpu, bus, 4, Some((collection, 4)))
            }
            0x03 => {
                let owners = self
                    .collections
                    .with_ref(|manager| manager.owner_count(bus.read_long(sp)) as u32);
                Self::collection_finish(cpu, bus, 4, Some((owners, 4)))
            }
            0x04 => {
                let collection = self.collections.with_mut(|manager| {
                    manager.copy_collection(bus.read_long(sp + 4), bus.read_long(sp))
                });
                Self::collection_finish(cpu, bus, 8, Some((collection, 4)))
            }
            0x05 => {
                let attributes = self
                    .collections
                    .with_ref(|manager| manager.default_attributes(bus.read_long(sp)));
                Self::collection_finish(cpu, bus, 4, Some((attributes, 4)))
            }
            0x06 => {
                self.collections.with_mut(|manager| {
                    manager.set_default_attributes(
                        bus.read_long(sp + 8),
                        bus.read_long(sp + 4),
                        bus.read_long(sp),
                    )
                });
                Self::collection_finish(cpu, bus, 12, None)
            }
            0x07 => {
                let count = self
                    .collections
                    .with_ref(|manager| manager.item_count(bus.read_long(sp)) as u32);
                Self::collection_finish(cpu, bus, 4, Some((count, 4)))
            }
            0x08 => {
                let size = bus.read_long(sp + 4);
                let pointer = bus.read_long(sp);
                let data = if size == 0 || pointer == 0 {
                    Vec::new()
                } else {
                    Self::collection_read_bytes(bus, pointer, size)
                };
                let error = self.collections.with_mut(|manager| {
                    manager.add_item(
                        bus.read_long(sp + 16),
                        bus.read_long(sp + 12),
                        bus.read_long(sp + 8) as i32,
                        data,
                    )
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 16), 20, error)
            }
            0x09 => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .item_by_key(
                            bus.read_long(sp + 16),
                            bus.read_long(sp + 12),
                            bus.read_long(sp + 8) as i32,
                        )
                        .map(|(_, item)| item.clone())
                });
                let error = Self::collection_copy_item(
                    bus,
                    item,
                    bus.read_long(sp + 4),
                    bus.read_long(sp),
                    COLLECTION_ITEM_NOT_FOUND_ERR,
                );
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 16), 20, error)
            }
            0x0A => {
                let error = self.collections.with_mut(|manager| {
                    manager.remove_key(
                        bus.read_long(sp + 8),
                        bus.read_long(sp + 4),
                        bus.read_long(sp) as i32,
                    )
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 8), 12, error)
            }
            0x0B => {
                let error = self.collections.with_mut(|manager| {
                    manager.set_key_attributes(
                        bus.read_long(sp + 16),
                        bus.read_long(sp + 12),
                        bus.read_long(sp + 8) as i32,
                        bus.read_long(sp + 4),
                        bus.read_long(sp),
                    )
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 16), 20, error)
            }
            0x0C => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .item_by_key(
                            bus.read_long(sp + 20),
                            bus.read_long(sp + 16),
                            bus.read_long(sp + 12) as i32,
                        )
                        .map(|(index, item)| (index, item.clone()))
                });
                let error = if let Some((index, item)) = item {
                    for (pointer, value) in [
                        (bus.read_long(sp + 8), index as u32),
                        (bus.read_long(sp + 4), item.data.len() as u32),
                        (bus.read_long(sp), item.attributes),
                    ] {
                        if pointer != 0 {
                            bus.write_long(pointer, value);
                        }
                    }
                    0
                } else {
                    COLLECTION_ITEM_NOT_FOUND_ERR
                };
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 20), 24, error)
            }
            0x0D => {
                let size = bus.read_long(sp + 4);
                let pointer = bus.read_long(sp);
                let data = if size == 0 || pointer == 0 {
                    Vec::new()
                } else {
                    Self::collection_read_bytes(bus, pointer, size)
                };
                let error = self.collections.with_mut(|manager| {
                    manager.replace_indexed(
                        bus.read_long(sp + 12),
                        bus.read_long(sp + 8) as i32,
                        data,
                    )
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 12), 16, error)
            }
            0x0E => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .item_by_index(bus.read_long(sp + 12), bus.read_long(sp + 8) as i32)
                        .cloned()
                });
                let error = Self::collection_copy_item(
                    bus,
                    item,
                    bus.read_long(sp + 4),
                    bus.read_long(sp),
                    COLLECTION_INDEX_RANGE_ERR,
                );
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 12), 16, error)
            }
            0x0F => {
                let error = self.collections.with_mut(|manager| {
                    manager.remove_indexed(bus.read_long(sp + 4), bus.read_long(sp) as i32)
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 4), 8, error)
            }
            0x10 => {
                let error = self.collections.with_mut(|manager| {
                    manager.set_indexed_attributes(
                        bus.read_long(sp + 12),
                        bus.read_long(sp + 8) as i32,
                        bus.read_long(sp + 4),
                        bus.read_long(sp),
                    )
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 12), 16, error)
            }
            0x11 => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .item_by_index(bus.read_long(sp + 20), bus.read_long(sp + 16) as i32)
                        .cloned()
                });
                let error = if let Some(item) = item {
                    for (pointer, value) in [
                        (bus.read_long(sp + 12), item.tag),
                        (bus.read_long(sp + 8), item.id as u32),
                        (bus.read_long(sp + 4), item.data.len() as u32),
                        (bus.read_long(sp), item.attributes),
                    ] {
                        if pointer != 0 {
                            bus.write_long(pointer, value);
                        }
                    }
                    0
                } else {
                    COLLECTION_INDEX_RANGE_ERR
                };
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 20), 24, error)
            }
            0x12 => {
                let exists = self.collections.with_ref(|manager| {
                    manager.tag_exists(bus.read_long(sp + 4), bus.read_long(sp))
                });
                Self::collection_finish(cpu, bus, 8, Some((u32::from(exists), 1)))
            }
            0x13 => {
                let count = self
                    .collections
                    .with_ref(|manager| manager.tags(bus.read_long(sp)).len() as u32);
                Self::collection_finish(cpu, bus, 4, Some((count, 4)))
            }
            0x14 => {
                let tag = self.collections.with_ref(|manager| {
                    usize::try_from(bus.read_long(sp + 4) as i32 - 1)
                        .ok()
                        .and_then(|index| manager.tags(bus.read_long(sp + 8)).get(index).copied())
                });
                let error = tag.map_or(COLLECTION_INDEX_RANGE_ERR, |tag| {
                    let pointer = bus.read_long(sp);
                    if pointer != 0 {
                        bus.write_long(pointer, tag);
                    }
                    0
                });
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 8), 12, error)
            }
            0x15 => {
                let count = self.collections.with_ref(|manager| {
                    manager.tagged_count(bus.read_long(sp + 4), bus.read_long(sp)) as u32
                });
                Self::collection_finish(cpu, bus, 8, Some((count, 4)))
            }
            0x16 => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .tagged_item(
                            bus.read_long(sp + 16),
                            bus.read_long(sp + 12),
                            bus.read_long(sp + 8) as i32,
                        )
                        .map(|(_, item)| item.clone())
                });
                let error = Self::collection_copy_item(
                    bus,
                    item,
                    bus.read_long(sp + 4),
                    bus.read_long(sp),
                    COLLECTION_INDEX_RANGE_ERR,
                );
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 16), 20, error)
            }
            0x17 => {
                let item = self.collections.with_ref(|manager| {
                    manager
                        .tagged_item(
                            bus.read_long(sp + 24),
                            bus.read_long(sp + 20),
                            bus.read_long(sp + 16) as i32,
                        )
                        .map(|(index, item)| (index, item.clone()))
                });
                let error = if let Some((index, item)) = item {
                    for (pointer, value) in [
                        (bus.read_long(sp + 12), item.id as u32),
                        (bus.read_long(sp + 8), index as u32),
                        (bus.read_long(sp + 4), item.data.len() as u32),
                        (bus.read_long(sp), item.attributes),
                    ] {
                        if pointer != 0 {
                            bus.write_long(pointer, value);
                        }
                    }
                    0
                } else {
                    COLLECTION_INDEX_RANGE_ERR
                };
                self.collection_finish_error(cpu, bus, bus.read_long(sp + 24), 28, error)
            }
            0x18 => {
                self.collections.with_mut(|manager| {
                    manager.purge_matching(
                        bus.read_long(sp + 8),
                        bus.read_long(sp + 4),
                        bus.read_long(sp),
                    )
                });
                Self::collection_finish(cpu, bus, 12, None)
            }
            0x19 => {
                self.collections.with_mut(|manager| {
                    manager.purge_tag(bus.read_long(sp + 4), bus.read_long(sp))
                });
                Self::collection_finish(cpu, bus, 8, None)
            }
            0x1A => {
                self.collections
                    .with_mut(|manager| manager.empty(bus.read_long(sp)));
                Self::collection_finish(cpu, bus, 4, None)
            }
            0x1B | 0x1C => {
                let (argument_bytes, collection, procedure, refcon, filter) = if routine == 0x1B {
                    (
                        12,
                        bus.read_long(sp + 8),
                        bus.read_long(sp + 4),
                        bus.read_long(sp),
                        None,
                    )
                } else {
                    (
                        20,
                        bus.read_long(sp + 16),
                        bus.read_long(sp + 12),
                        bus.read_long(sp + 8),
                        Some((bus.read_long(sp + 4), bus.read_long(sp))),
                    )
                };
                let bytes = self
                    .collections
                    .with_ref(|manager| manager.flatten(collection, filter));
                if let Some(bytes) = bytes {
                    self.collection_begin_callback(
                        cpu,
                        bus,
                        procedure,
                        refcon,
                        sp + argument_bytes,
                        Some(&bytes),
                        bytes.len() as u32,
                        CollectionCallbackKind::Flatten { collection },
                    )
                } else {
                    Self::collection_finish(
                        cpu,
                        bus,
                        argument_bytes,
                        Some(((-50i16) as u16 as u32, 2)),
                    )
                }
            }
            0x1D => self.collection_begin_callback(
                cpu,
                bus,
                bus.read_long(sp + 4),
                bus.read_long(sp),
                sp + 12,
                None,
                12,
                CollectionCallbackKind::Unflatten {
                    collection: bus.read_long(sp + 8),
                    bytes: Vec::new(),
                    phase: CollectionUnflattenPhase::Header,
                    remaining_items: 0,
                },
            ),
            0x1E => {
                let proc = self
                    .collections
                    .with_ref(|manager| manager.exception_proc(bus.read_long(sp)));
                Self::collection_finish(cpu, bus, 4, Some((proc, 4)))
            }
            0x1F => {
                self.collections.with_mut(|manager| {
                    manager.set_exception_proc(bus.read_long(sp + 4), bus.read_long(sp))
                });
                Self::collection_finish(cpu, bus, 8, None)
            }
            0x20 => {
                let id = bus.read_word(sp) as i16;
                let resource = self
                    .find_or_load_resource_any(bus, *b"cltn", id)
                    .map(|(_, pointer)| pointer);
                let collection = resource.and_then(|pointer| {
                    let size = bus.get_alloc_size(pointer)? as u32;
                    let bytes = Self::collection_read_bytes(bus, pointer, size);
                    self.collections
                        .with_mut(|manager| manager.from_resource(&bytes))
                });
                Self::collection_finish(cpu, bus, 2, Some((collection.unwrap_or(0), 4)))
            }
            0x21 | 0x22 => {
                let handle = bus.read_long(sp);
                let collection = bus.read_long(sp + 12);
                let tag = bus.read_long(sp + 8);
                let id = bus.read_long(sp + 4) as i32;
                let error = if routine == 0x21 {
                    let bytes = self.collection_handle_bytes(bus, handle);
                    bytes.map_or(-109, |bytes| {
                        self.collections
                            .with_mut(|manager| manager.add_item(collection, tag, id, bytes))
                    })
                } else {
                    let bytes = self.collections.with_ref(|manager| {
                        manager
                            .item_by_key(collection, tag, id)
                            .map(|(_, item)| item.data.clone())
                    });
                    bytes.map_or(COLLECTION_ITEM_NOT_FOUND_ERR, |bytes| {
                        if handle == 0 {
                            0
                        } else {
                            self.collection_replace_handle_bytes(bus, handle, &bytes)
                        }
                    })
                };
                self.collection_finish_error(cpu, bus, collection, 16, error)
            }
            0x23 | 0x24 => {
                let handle = bus.read_long(sp);
                let index = bus.read_long(sp + 4) as i32;
                let collection = bus.read_long(sp + 8);
                let error = if routine == 0x23 {
                    self.collection_handle_bytes(bus, handle)
                        .map_or(-109, |bytes| {
                            self.collections.with_mut(|manager| {
                                manager.replace_indexed(collection, index, bytes)
                            })
                        })
                } else {
                    let bytes = self.collections.with_ref(|manager| {
                        manager
                            .item_by_index(collection, index)
                            .map(|item| item.data.clone())
                    });
                    bytes.map_or(COLLECTION_INDEX_RANGE_ERR, |bytes| {
                        self.collection_replace_handle_bytes(bus, handle, &bytes)
                    })
                };
                self.collection_finish_error(cpu, bus, collection, 12, error)
            }
            0x25 | 0x26 => {
                let handle = bus.read_long(sp);
                let collection = bus.read_long(sp + 4);
                let error = if routine == 0x25 {
                    self.collections
                        .with_ref(|manager| manager.flatten(collection, None))
                        .map_or(-50, |bytes| {
                            self.collection_replace_handle_bytes(bus, handle, &bytes)
                        })
                } else {
                    self.collection_handle_bytes(bus, handle)
                        .map_or(-109, |bytes| {
                            self.collections
                                .with_mut(|manager| manager.unflatten(collection, &bytes))
                        })
                };
                self.collection_finish_error(cpu, bus, collection, 8, error)
            }
            _ => unreachable!(),
        };
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::test_helpers::{setup, TEST_SP};

    fn call_collection(
        dispatcher: &mut TrapDispatcher,
        cpu: &mut impl CpuOps,
        bus: &mut MacMemoryBus,
        selector: u16,
    ) {
        cpu.write_reg(Register::D0, selector as u32);
        dispatcher
            .dispatch_collection(true, 0x3f6, cpu, bus)
            .expect("Collection Manager owns _CollectionMgr")
            .expect("Collection Manager selector succeeds");
    }

    #[test]
    fn classic_abi_adds_and_retrieves_item_data() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7000);
        let collection = bus.read_long(TEST_SP);
        assert_ne!(collection, 0);

        let tag = u32::from_be_bytes(*b"test");
        let source = 0x120000;
        bus.write_bytes(source, b"data");
        cpu.write_reg(Register::A7, TEST_SP + 0x40);
        let sp = TEST_SP + 0x40;
        bus.write_long(sp, source);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, (-7i32) as u32);
        bus.write_long(sp + 12, tag);
        bus.write_long(sp + 16, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7008);
        assert_eq!(bus.read_word(sp + 20), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 20);

        let destination = 0x120100;
        let size = 0x120200;
        bus.write_long(size, 16);
        cpu.write_reg(Register::A7, TEST_SP + 0x80);
        let sp = TEST_SP + 0x80;
        bus.write_long(sp, destination);
        bus.write_long(sp + 4, size);
        bus.write_long(sp + 8, (-7i32) as u32);
        bus.write_long(sp + 12, tag);
        bus.write_long(sp + 16, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7009);
        assert_eq!(bus.read_word(sp + 20), 0);
        assert_eq!(bus.read_long(size), 4);
        assert_eq!(bus.read_bytes(destination, 4), b"data");
    }

    #[test]
    fn classic_abi_uses_one_based_tag_positions_and_byte_booleans() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let collection = dispatcher
            .collections
            .with_mut(|manager| manager.new_collection());
        let first_tag = u32::from_be_bytes(*b"aaaa");
        dispatcher.collections.with_mut(|manager| {
            assert_eq!(manager.add_item(collection, first_tag, 1, Vec::new()), 0);
        });

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, first_tag);
        bus.write_long(TEST_SP + 4, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7012);
        assert_eq!(bus.read_byte(TEST_SP + 8), 1);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let tag_out = 0x120000;
        cpu.write_reg(Register::A7, TEST_SP + 0x40);
        let sp = TEST_SP + 0x40;
        bus.write_long(sp, tag_out);
        bus.write_long(sp + 4, 1);
        bus.write_long(sp + 8, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7014);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(bus.read_long(tag_out), first_tag);
    }

    #[test]
    fn classic_exception_proc_can_rewrite_collection_errors() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let collection = dispatcher
            .collections
            .with_mut(|manager| manager.new_collection());
        let exception_proc = 0x120000;
        dispatcher.collections.with_mut(|manager| {
            manager.set_exception_proc(collection, exception_proc);
        });

        let return_pc = 0x234000;
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(TEST_SP, 99);
        bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"none"));
        bus.write_long(TEST_SP + 8, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x700a);

        assert_eq!(cpu.read_reg(Register::PC), exception_proc);
        let callback_sp = cpu.read_reg(Register::A7);
        assert_eq!(
            bus.read_word(callback_sp + 4) as i16,
            COLLECTION_ITEM_NOT_FOUND_ERR
        );
        assert_eq!(bus.read_long(callback_sp + 6), collection);

        bus.write_word(TEST_SP + 12, 0);
        cpu.write_reg(Register::A7, TEST_SP + 12);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x70fe);
        assert_eq!(bus.read_word(TEST_SP + 12), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
    }

    #[test]
    fn classic_handle_variants_and_flatten_round_trip_share_memory_manager_handles() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let collection = dispatcher
            .collections
            .with_mut(|manager| manager.new_collection());
        let tag = u32::from_be_bytes(*b"data");
        dispatcher.collections.with_mut(|manager| {
            assert_eq!(manager.add_item(collection, tag, 3, b"handle".to_vec()), 0);
        });
        let (item_handle, _) = dispatcher
            .new_process_classic_handle(&mut bus, 0)
            .expect("item handle allocation");

        bus.write_long(TEST_SP, item_handle);
        bus.write_long(TEST_SP + 4, 3);
        bus.write_long(TEST_SP + 8, tag);
        bus.write_long(TEST_SP + 12, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7022);
        assert_eq!(bus.read_word(TEST_SP + 16), 0);
        let item_pointer = bus.read_long(item_handle);
        assert_eq!(bus.read_bytes(item_pointer, 6), b"handle");

        let (flattened_handle, _) = dispatcher
            .new_process_classic_handle(&mut bus, 0)
            .expect("flattened handle allocation");
        cpu.write_reg(Register::A7, TEST_SP + 0x40);
        let sp = TEST_SP + 0x40;
        bus.write_long(sp, flattened_handle);
        bus.write_long(sp + 4, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7025);
        assert_eq!(bus.read_word(sp + 8), 0);

        let target = dispatcher
            .collections
            .with_mut(|manager| manager.new_collection());
        cpu.write_reg(Register::A7, TEST_SP + 0x80);
        let sp = TEST_SP + 0x80;
        bus.write_long(sp, flattened_handle);
        bus.write_long(sp + 4, target);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x7026);
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(
            dispatcher.collections.with_ref(|manager| {
                manager
                    .item_by_key(target, tag, 3)
                    .map(|(_, item)| item.data.clone())
            }),
            Some(b"handle".to_vec())
        );
    }

    #[test]
    fn classic_flatten_propagates_callback_failure() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let collection = dispatcher
            .collections
            .with_mut(|manager| manager.new_collection());
        dispatcher.collections.with_mut(|manager| {
            assert_eq!(manager.add_item(collection, 1, 1, vec![1]), 0);
        });
        let flatten_proc = 0x120000;
        let return_pc = 0x234000;
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(TEST_SP, 0xfeed_beef);
        bus.write_long(TEST_SP + 4, flatten_proc);
        bus.write_long(TEST_SP + 8, collection);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x701b);

        assert_eq!(cpu.read_reg(Register::PC), flatten_proc);
        let callback_sp = cpu.read_reg(Register::A7);
        assert_ne!(bus.read_long(callback_sp + 8), 0);
        assert_eq!(bus.read_long(callback_sp + 4), 0xfeed_beef);

        bus.write_word(TEST_SP + 12, (-123i16) as u16);
        cpu.write_reg(Register::A7, TEST_SP + 12);
        call_collection(&mut dispatcher, &mut cpu, &mut bus, 0x70fe);
        assert_eq!(bus.read_word(TEST_SP + 12) as i16, -123);
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
    }
}
