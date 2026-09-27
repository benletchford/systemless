//! PowerPC Heap, Handle, and Memory Manager implementation.

use super::*;

#[cfg(test)]
pub(crate) fn ppc_process_handle_state_bits(
    handle_states: &[PpcHandleStateRecord],
    handle: u32,
) -> u8 {
    let state = handle_states.iter().find(|state| state.handle == handle);
    let mut bits = 0u8;
    if state.is_some_and(|state| state.locked) {
        bits |= 0x80;
    }
    if state.is_none_or(|state| !state.no_purge) {
        bits |= 0x40;
    }
    if state.is_some_and(|state| state.resource) {
        bits |= 0x20;
    }
    bits
}

pub(crate) fn ppc_apply_process_native_allocator(
    memory_manager: &ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) {
    let Some(heap) = memory_manager.native_heap_state() else {
        return;
    };
    *heap_cursor = heap.heap_cursor;
    *last_mem_error = heap.last_mem_error;
    // The zone's free-byte projection follows the guest-visible application
    // boundary, while the native heap record retains the physical mapping
    // ceiling for stack protection and loader validation. Inside Macintosh:
    // Memory (1992), pp. 2-83--2-85.
    let allocation_limit = memory_manager.native_allocation_limit(heap.heap_limit);
    ppc_update_zone_free_bytes(memory, *heap_cursor, allocation_limit);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispose_process_native_handle(
    memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    _heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
) -> bool {
    let disposed = memory_manager
        .dispose_native_handle(memory, handle)
        .is_some();
    ppc_apply_process_native_allocator(memory_manager, memory, heap_cursor, last_mem_error);
    if disposed {
        handles.retain(|record| record.handle != handle);
    }
    disposed
}

pub(crate) fn ppc_apply_process_native_handle(
    memory_manager: &ProcessNativeMemoryManager,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
) {
    let Some(updated) = memory_manager.native_allocation(handle) else {
        return;
    };
    if let Some(record) = handles.iter_mut().find(|record| record.handle == handle) {
        *record = updated;
    } else {
        handles.push(updated);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_apply_process_native_resource_handle(
    memory_manager: &ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
) {
    if let Some(heap) = memory_manager.native_heap_state() {
        *heap_cursor = heap.heap_cursor;
        *last_mem_error = heap.last_mem_error;
        let allocation_limit = memory_manager.native_allocation_limit(heap.heap_limit);
        ppc_update_zone_free_bytes(memory, *heap_cursor, allocation_limit);
    }
    ppc_apply_process_native_handle(memory_manager, handles, handle);
}

pub(crate) fn ppc_dispose_tracked_handle(
    handle: u32,
    memory: &mut PpcSectionMem,
    handles: &mut Vec<PpcHandleRecord>,
    handle_states: &mut Vec<PpcHandleStateRecord>,
) -> bool {
    if handle == 0 {
        return false;
    }
    if let Some(index) = handles.iter().position(|record| record.handle == handle) {
        let record = handles.remove(index);
        let _ = memory.write_u32_be(record.handle, 0);
        ppc_forget_handle_state(handle, handle_states);
        true
    } else {
        false
    }
}

pub(crate) fn ppc_is_valid_handle(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    handle: u32,
) -> bool {
    if handle == 0 {
        return false;
    }
    if handles.iter().any(|record| record.handle == handle) {
        return true;
    }
    let Some(ptr) = memory.read_u32_be(handle) else {
        return false;
    };
    ptr != 0 && memory.read_u8(ptr).is_some()
}

pub(crate) fn ppc_forget_handle_state(handle: u32, handle_states: &mut Vec<PpcHandleStateRecord>) {
    handle_states.retain(|record| record.handle != handle);
}

pub(crate) fn ppc_alloc_handle_with_bytes(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    bytes: &[u8],
) -> u32 {
    let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let handle = ppc_alloc_handle(memory, heap_cursor, heap_limit, handles, size, false);
    if handle == 0 {
        return 0;
    }
    let Some(ptr) = memory.read_u32_be(handle) else {
        return 0;
    };
    if memory.write_bytes(ptr, bytes).is_none() {
        return 0;
    }
    handle
}

pub(crate) fn ppc_alloc_recyclable_handle_with_bytes(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
    bytes: &[u8],
) -> u32 {
    let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let handle = ppc_alloc_recyclable_handle(
        memory,
        heap_cursor,
        heap_limit,
        handles,
        free_handle_blocks,
        size,
        false,
    );
    if handle == 0 {
        return 0;
    }
    let Some(ptr) = memory.read_u32_be(handle) else {
        return 0;
    };
    if memory.write_bytes(ptr, bytes).is_none() {
        return 0;
    }
    handle
}

pub(crate) fn ppc_alloc_recyclable_handle(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
    size: u32,
    clear: bool,
) -> u32 {
    let required = match ppc_allocation_size(size) {
        Some(required) => required,
        None => return 0,
    };
    let reusable_index = free_handle_blocks
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let capacity = ppc_allocation_size(record.capacity)?;
            (capacity >= required).then_some((index, capacity))
        })
        .min_by_key(|(_, capacity)| *capacity)
        .map(|(index, _)| index);
    let handle = if let Some(index) = reusable_index {
        let mut record = free_handle_blocks.swap_remove(index);
        record.size = size;
        if memory.write_u32_be(record.handle, record.ptr).is_none() {
            return 0;
        }
        let handle = record.handle;
        handles.push(record);
        handle
    } else {
        ppc_alloc_handle(memory, heap_cursor, heap_limit, handles, size, clear)
    };
    if handle == 0 {
        return 0;
    }
    if clear {
        let Some(ptr) = memory.read_u32_be(handle) else {
            return 0;
        };
        for offset in 0..required {
            if memory.write_u8(ptr + offset, 0).is_none() {
                return 0;
            }
        }
    }
    handle
}

pub(crate) fn ppc_alloc_handle(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    size: u32,
    clear: bool,
) -> u32 {
    if !ppc_heap_can_alloc_sequence(memory, *heap_cursor, heap_limit, &[4, size]) {
        return 0;
    }
    let handle = ppc_heap_alloc(memory, heap_cursor, heap_limit, 4, true);
    if handle == 0 {
        return 0;
    }
    let ptr = ppc_heap_alloc(memory, heap_cursor, heap_limit, size, clear);
    if ptr == 0 || memory.write_u32_be(handle, ptr).is_none() {
        return 0;
    }
    handles.push(PpcHandleRecord {
        handle,
        ptr,
        size,
        capacity: size,
    });
    handle
}

pub(crate) fn ppc_set_handle_size(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut [PpcHandleRecord],
    handle: u32,
    size: u32,
) -> i16 {
    let Some(record) = handles.iter_mut().find(|record| record.handle == handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    if size <= record.capacity {
        record.size = size;
        return PPC_NO_ERR;
    }

    let Some(old_aligned) = ppc_allocation_size(record.size) else {
        return PPC_PARAM_ERR;
    };
    let Some(new_aligned) = ppc_allocation_size(size) else {
        return PPC_MEM_FULL_ERR;
    };
    if let Some(old_end) = record.ptr.checked_add(old_aligned) {
        if old_end == *heap_cursor {
            if let Some((resize_ptr, new_end)) =
                ppc_heap_allocation_bounds(memory, record.ptr, heap_limit, new_aligned)
            {
                if resize_ptr == record.ptr {
                    if new_end > old_end && memory.read_u8(old_end).is_none() {
                        let Ok(growth) = usize::try_from(new_end - old_end) else {
                            return PPC_MEM_FULL_ERR;
                        };
                        memory.add_region(old_end, vec![0; growth]);
                    }
                    for addr in old_end..new_end {
                        if memory.write_u8(addr, 0).is_none() {
                            return PPC_PARAM_ERR;
                        }
                    }
                    *heap_cursor = new_end;
                    record.size = size;
                    record.capacity = size;
                    return PPC_NO_ERR;
                }
            }
        }
    }

    let new_ptr = ppc_heap_alloc(memory, heap_cursor, heap_limit, size, true);
    if new_ptr == 0 {
        return PPC_MEM_FULL_ERR;
    }
    let copy_len = record.size.min(size);
    for offset in 0..copy_len {
        let Some(byte) = memory.read_u8(record.ptr + offset) else {
            return PPC_PARAM_ERR;
        };
        if memory.write_u8(new_ptr + offset, byte).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    if memory.write_u32_be(handle, new_ptr).is_none() {
        return PPC_PARAM_ERR;
    }
    record.ptr = new_ptr;
    record.size = size;
    record.capacity = size;
    PPC_NO_ERR
}

pub(crate) fn ppc_handle_resize_allocation_size(
    memory: &PpcSectionMem,
    record: PpcHandleRecord,
    heap_cursor: u32,
    heap_limit: u32,
    size: u32,
) -> Option<u32> {
    if size <= record.capacity {
        return Some(0);
    }
    let old_aligned = ppc_allocation_size(record.size)?;
    let new_aligned = ppc_allocation_size(size)?;
    if record.ptr.checked_add(old_aligned) == Some(heap_cursor)
        && ppc_heap_allocation_bounds(memory, record.ptr, heap_limit, new_aligned)
            .is_some_and(|(new_ptr, _)| new_ptr == record.ptr)
    {
        new_aligned.checked_sub(old_aligned)
    } else {
        Some(new_aligned)
    }
}

pub(crate) fn ppc_handle_bytes(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    handle: u32,
) -> Option<Vec<u8>> {
    let record = handles.iter().find(|record| record.handle == handle)?;
    if record.size != 0 {
        record.ptr.checked_add(record.size - 1)?;
    }
    let mut bytes = vec![0; usize::try_from(record.size).ok()?];
    memory.read_bytes_into(record.ptr, &mut bytes)?;
    Some(bytes)
}

pub(crate) fn ppc_block_move(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) {
    let source_ptr = cpu.gpr[3];
    let dest_ptr = cpu.gpr[4];
    let byte_count = cpu.gpr[5] as usize;
    let mut bytes = vec![0; byte_count];
    if memory.read_bytes_into(source_ptr, &mut bytes).is_none() {
        return;
    }
    let mut pixels = crate::memory::SavedPixels::from(bytes);
    let presentation = memory.presentation();
    presentation.capture_detail(&mut pixels, 0, source_ptr, byte_count);
    if memory.write_bytes(dest_ptr, &pixels).is_some() {
        presentation.restore_detail(&pixels, 0, dest_ptr, byte_count);
    }
}

#[cfg(test)]
pub(crate) fn ppc_hand_to_hand(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
) -> i16 {
    let handle_variable = cpu.gpr[3];
    if handle_variable == 0 || !ppc_memory_can_write_bytes(memory, handle_variable, 4) {
        return PPC_PARAM_ERR;
    }
    let Some(source_handle) = memory.read_u32_be(handle_variable) else {
        return PPC_PARAM_ERR;
    };
    let Some(source) = handles
        .iter()
        .find(|record| record.handle == source_handle)
        .copied()
        .or_else(|| ppc_system_handle_record(memory, source_handle))
    else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(bytes) = ppc_memory_read_bytes(memory, source.ptr, source.size) else {
        return PPC_PARAM_ERR;
    };
    let copy = ppc_alloc_recyclable_handle_with_bytes(
        memory,
        heap_cursor,
        heap_limit,
        handles,
        free_handle_blocks,
        &bytes,
    );
    if copy == 0 {
        return PPC_MEM_FULL_ERR;
    }
    // Inside Macintosh: Memory (1992), pp. 2-62--2-63: HandToHand replaces
    // the caller's Handle variable with an unlocked, unpurgeable data copy.
    if memory.write_u32_be(handle_variable, copy).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] HandToHand variable=${handle_variable:08X} source=${source_handle:08X} size={} -> ${copy:08X}",
            source.size
        );
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_system_handle_record(
    memory: &mut PpcSectionMem,
    handle: u32,
) -> Option<PpcHandleRecord> {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-47 and 4-83:
    // a GDevice's indexed PixMap owns its ColorTable through a Handle. The
    // main device is Toolbox-owned rather than application-heap-owned, but
    // Memory Manager routines such as HandToHand must still accept it.
    let size = match handle {
        PPC_MAIN_CTABLE_HANDLE => PPC_MAIN_CTABLE_SIZE,
        _ => return None,
    };
    let ptr = memory.read_u32_be(handle)?;
    ppc_memory_read_bytes(memory, ptr, size)?;
    Some(PpcHandleRecord {
        handle,
        ptr,
        size,
        capacity: size,
    })
}

pub(crate) fn ppc_heap_alloc(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    size: u32,
    clear: bool,
) -> u32 {
    let Some(aligned) = ppc_allocation_size(size) else {
        return 0;
    };
    let Some((ptr, next)) = ppc_heap_allocation_bounds(memory, *heap_cursor, heap_limit, aligned)
    else {
        return 0;
    };
    if ppc_memory_can_read_bytes(memory, ptr, aligned) {
        if clear {
            for offset in 0..aligned {
                let _ = memory.write_u8(ptr + offset, 0);
            }
        }
    } else {
        memory.add_region(ptr, vec![0u8; aligned as usize]);
    }
    *heap_cursor = next;
    ppc_update_zone_free_bytes(memory, next, heap_limit);
    ptr
}

pub(crate) fn ppc_process_heap_alloc(
    memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    size: u32,
    clear: bool,
) -> u32 {
    let ptr = memory_manager.reserve_native_bytes(memory, size, clear);
    if let Some(heap) = memory_manager.native_heap_state() {
        *heap_cursor = heap.heap_cursor;
        let allocation_limit = memory_manager.native_allocation_limit(heap.heap_limit);
        ppc_update_zone_free_bytes(memory, heap.heap_cursor, allocation_limit);
    }
    ptr
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_process_alloc_handle_with_bytes(
    memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    bytes: &[u8],
) -> u32 {
    let handle = memory_manager.copy_bytes_to_new_native_handle(memory, bytes);
    ppc_apply_process_native_resource_handle(
        memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        handle,
    );
    handle
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_process_alloc_handle(
    memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    size: u32,
    clear: bool,
) -> u32 {
    let handle = memory_manager.new_native_handle(memory, size, clear);
    ppc_apply_process_native_resource_handle(
        memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        handle,
    );
    handle
}

pub(crate) struct PpcProcessAllocatorView<'a> {
    pub(crate) memory_manager: &'a mut ProcessNativeMemoryManager,
}

impl PpcProcessAllocatorView<'_> {
    pub(crate) fn reserve_bytes(
        &mut self,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        size: u32,
        clear: bool,
    ) -> u32 {
        ppc_process_heap_alloc(self.memory_manager, memory, heap_cursor, size, clear)
    }

    pub(crate) fn allocate_handle(
        &mut self,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
        handles: &mut Vec<PpcHandleRecord>,
        size: u32,
        clear: bool,
    ) -> u32 {
        ppc_process_alloc_handle(
            self.memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            size,
            clear,
        )
    }

    pub(crate) fn allocate_handle_with_bytes(
        &mut self,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
        handles: &mut Vec<PpcHandleRecord>,
        bytes: &[u8],
    ) -> u32 {
        ppc_process_alloc_handle_with_bytes(
            self.memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            bytes,
        )
    }

    pub(crate) fn resize_handle(
        &mut self,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
        handles: &mut Vec<PpcHandleRecord>,
        handle: u32,
        size: u32,
    ) -> i16 {
        let result = self
            .memory_manager
            .set_native_handle_size(memory, handle, size);
        ppc_apply_process_native_resource_handle(
            self.memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            handle,
        );
        result
    }

    pub(crate) fn dispose_handle(
        &mut self,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        heap_limit: u32,
        last_mem_error: &mut i16,
        handles: &mut Vec<PpcHandleRecord>,
        handle: u32,
    ) -> bool {
        ppc_dispose_process_native_handle(
            self.memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        )
    }
}

pub(crate) fn ppc_allocator_view_reserve_bytes(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    size: u32,
    clear: bool,
) -> u32 {
    if let Some(allocator) = allocator {
        allocator.reserve_bytes(memory, heap_cursor, size, clear)
    } else {
        ppc_heap_alloc(memory, heap_cursor, heap_limit, size, clear)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_allocator_view_allocate_handle(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    size: u32,
    clear: bool,
) -> u32 {
    if let Some(allocator) = allocator {
        allocator.allocate_handle(memory, heap_cursor, last_mem_error, handles, size, clear)
    } else {
        ppc_alloc_handle(memory, heap_cursor, heap_limit, handles, size, clear)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_allocator_view_allocate_handle_with_bytes(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    bytes: &[u8],
) -> u32 {
    if let Some(allocator) = allocator {
        allocator.allocate_handle_with_bytes(memory, heap_cursor, last_mem_error, handles, bytes)
    } else {
        ppc_alloc_handle_with_bytes(memory, heap_cursor, heap_limit, handles, bytes)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_allocator_view_resize_handle(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
    size: u32,
) -> i16 {
    if let Some(allocator) = allocator {
        allocator.resize_handle(memory, heap_cursor, last_mem_error, handles, handle, size)
    } else {
        ppc_set_handle_size(memory, heap_cursor, heap_limit, handles, handle, size)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_allocator_view_dispose_handle(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    handle: u32,
) -> bool {
    if let Some(allocator) = allocator {
        allocator.dispose_handle(
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        )
    } else {
        ppc_dispose_tracked_handle(
            handle,
            memory,
            handles,
            legacy_handle_states.expect("legacy handle disposal requires handle state"),
        )
    }
}

pub(crate) fn ppc_heap_allocation_bounds(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
    aligned_size: u32,
) -> Option<(u32, u32)> {
    // The native bump heap shares one guest address space with runner-owned
    // system code. Treat both its staged exclusion and live read-only mapping
    // as a reserved hole so a successful Memory Manager call never returns
    // storage whose writes the Trap Manager topology must reject.
    ppc_aligned_heap_allocation_bounds(
        memory,
        heap_cursor,
        heap_limit,
        aligned_size,
        PPC_HEAP_ALIGNMENT,
    )
}

pub(crate) fn ppc_aligned_heap_allocation_bounds(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
    size: u32,
    alignment: u32,
) -> Option<(u32, u32)> {
    let mut ptr = align_up(heap_cursor, alignment).ok()?;
    loop {
        let next = ptr.checked_add(size)?;
        if next >= heap_limit {
            return None;
        }
        let Some(reserved_end) = memory.readonly_allocation_overlap_end(ptr, size) else {
            return Some((ptr, next));
        };
        ptr = align_up(reserved_end, alignment).ok()?;
    }
}

pub(crate) fn ppc_seed_zone_header(
    memory: &mut PpcSectionMem,
    zone: u32,
    heap_base: u32,
    heap_limit: u32,
    more_masters: u16,
) {
    // Inside Macintosh: Memory (1992), pp. 2-19--2-21, specifies the classic
    // Zone record through heapData. Universal Interfaces MacMemory.h refines
    // the former maxNRel field at byte 30 into heapType: k32BitHeap is bit 0
    // and kNewStyleHeap (the PowerPC Modern Memory Manager) is bit 1.
    let free_bytes = ppc_heap_free_capacity(memory, heap_base, heap_limit).0;
    let _ = memory.write_u32_be(zone, heap_limit); // bkLim
    let _ = memory.write_u32_be(zone + 4, 0); // purgePtr
    let _ = memory.write_u32_be(zone + 8, 0); // hFstFree
    let _ = memory.write_u32_be(zone + 12, free_bytes); // zcbFree
    let _ = memory.write_u32_be(zone + 16, 0); // gzProc
    let _ = memory.write_u16_be(zone + 20, more_masters); // moreMast
    let _ = memory.write_u16_be(zone + 22, 0); // flags
    let _ = memory.write_u16_be(zone + 24, 0); // cntRel
    let _ = memory.write_u16_be(zone + 26, 0); // maxRel
    let _ = memory.write_u16_be(zone + 28, 0); // cntNRel
    let _ = memory.write_u8(
        zone + PPC_ZONE_HEAP_TYPE_OFFSET,
        PPC_ZONE_32_BIT_HEAP | PPC_ZONE_NEW_STYLE_HEAP,
    );
    let _ = memory.write_u8(zone + PPC_ZONE_HEAP_TYPE_OFFSET + 1, 0);
    let _ = memory.write_u16_be(zone + 32, 0); // cntEmpty
    let _ = memory.write_u16_be(zone + 34, 0); // cntHandles
    let _ = memory.write_u32_be(zone + 36, free_bytes); // minCBFree
    let _ = memory.write_u32_be(zone + 40, 0); // purgeProc
    let _ = memory.write_u32_be(zone + 44, 0); // sparePtr
    let _ = memory.write_u32_be(zone + 48, heap_base); // allocPtr
    let _ = memory.write_u16_be(zone + 52, 0); // heapData
}

pub(crate) fn ppc_update_zone_free_bytes(
    memory: &mut PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
) {
    let free_bytes = ppc_heap_free_capacity(memory, heap_cursor, heap_limit).0;
    let _ = memory.write_u32_be(PPC_APPLICATION_ZONE + 12, free_bytes);
    let _ = memory.write_u32_be(PPC_SYSTEM_ZONE + 12, free_bytes);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_allocator_view_replace_handle_bytes(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
    bytes: &[u8],
) -> i16 {
    let Ok(size) = u32::try_from(bytes.len()) else {
        return PPC_MEM_FULL_ERR;
    };
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
        size,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
        return PPC_NIL_HANDLE_ERR;
    };
    if memory.write_bytes(ptr, bytes).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

#[cfg(test)]
pub(crate) fn ppc_alloc_ptr(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    ptrs: &mut Vec<PpcPtrRecord>,
    free_ptr_blocks: &mut Vec<PpcPtrRecord>,
    size: u32,
    clear: bool,
) -> u32 {
    let required = match ppc_allocation_size(size) {
        Some(required) => required,
        None => return 0,
    };
    let reusable_index = free_ptr_blocks
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let capacity = ppc_allocation_size(record.size)?;
            (capacity >= required).then_some((index, capacity))
        })
        .min_by_key(|(_, capacity)| *capacity)
        .map(|(index, _)| index);
    let ptr = if let Some(index) = reusable_index {
        free_ptr_blocks.swap_remove(index).ptr
    } else {
        ppc_heap_alloc(memory, heap_cursor, heap_limit, size, false)
    };
    if ptr == 0 {
        return 0;
    }
    if clear {
        for offset in 0..required {
            if memory.write_u8(ptr + offset, 0).is_none() {
                return 0;
            }
        }
    }
    // Inside Macintosh: Memory (1992), pp. 2-42 and 2-44: DisposePtr
    // returns a nonrelocatable block to the application heap for later use.
    ptrs.push(PpcPtrRecord { ptr, size });
    ptr
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_legacy_memory_utility(
    operation: PpcLegacyMemoryUtilityOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &[PpcHandleRecord],
) -> Option<PpcImportAction> {
    let free_ptr_blocks = process_memory_manager.native_free_ptr_blocks();
    // Inside Macintosh: Operating System Utilities (1994), pp. 3-28--3-29,
    // and Memory (1992), pp. 2-42--2-83, define these routines independently
    // of any application. Keeping their implementations in one dispatcher is
    // only an ABI grouping; behavior is selected solely by the imported API.
    match operation {
        PpcLegacyMemoryUtilityOperation::BitNot => Some(PpcImportAction::Return(!cpu.gpr[3])),
        PpcLegacyMemoryUtilityOperation::BitSet | PpcLegacyMemoryUtilityOperation::BitClear => {
            let bit_number = cpu.gpr[4];
            let byte_addr = cpu.gpr[3].checked_add(bit_number / 8)?;
            let mask = 0x80u8 >> (bit_number & 7);
            let old = memory.read_u8(byte_addr)?;
            let value = if operation == PpcLegacyMemoryUtilityOperation::BitSet {
                old | mask
            } else {
                old & !mask
            };
            memory.write_u8(byte_addr, value)?;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyMemoryUtilityOperation::FixToExtended => {
            let fixed = cpu.gpr[3] as i32;
            cpu.fpr[1] = (f64::from(fixed) / 65_536.0).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyMemoryUtilityOperation::GetMyZone => {
            Some(PpcImportAction::Return(PPC_APPLICATION_ZONE))
        }
        PpcLegacyMemoryUtilityOperation::HandleZone => {
            let handle = cpu.gpr[3];
            let valid = handle == 0 || handles.iter().any(|record| record.handle == handle);
            *last_mem_error = if valid { PPC_NO_ERR } else { PPC_MEM_WZ_ERR };
            Some(PpcImportAction::Return(if valid {
                PPC_APPLICATION_ZONE
            } else {
                0
            }))
        }
        PpcLegacyMemoryUtilityOperation::LockMemory
        | PpcLegacyMemoryUtilityOperation::UnlockMemory => {
            // The native heap is resident host memory, so a valid mapped range
            // is already immovable for the lifetime requested by the caller.
            let start = cpu.gpr[3];
            let len = cpu.gpr[4];
            let mapped = len == 0
                || (start != 0
                    && memory.read_u8(start).is_some()
                    && start
                        .checked_add(len - 1)
                        .and_then(|end| memory.read_u8(end))
                        .is_some());
            let result = if mapped { PPC_NO_ERR } else { PPC_PARAM_ERR };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyMemoryUtilityOperation::MaxBlock => Some(PpcImportAction::Return(
            ppc_largest_free_ptr_block(memory, *heap_cursor, heap_limit, free_ptr_blocks),
        )),
        PpcLegacyMemoryUtilityOperation::PurgeSpace => {
            let total = ppc_heap_free_capacity(memory, *heap_cursor, heap_limit)
                .0
                .saturating_add(
                    free_ptr_blocks
                        .iter()
                        .filter(|block| ppc_free_ptr_block_fits_limit(block, heap_limit))
                        .fold(0u32, |sum, block| sum.saturating_add(block.size)),
                );
            let contiguous =
                ppc_largest_free_ptr_block(memory, *heap_cursor, heap_limit, free_ptr_blocks);
            if cpu.gpr[3] != 0 {
                memory.write_u32_be(cpu.gpr[3], total)?;
            }
            if cpu.gpr[4] != 0 {
                memory.write_u32_be(cpu.gpr[4], contiguous)?;
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyMemoryUtilityOperation::ReserveMem => {
            let requested = cpu.gpr[3];
            let available =
                ppc_largest_free_ptr_block(memory, *heap_cursor, heap_limit, free_ptr_blocks);
            *last_mem_error = if requested <= available {
                PPC_NO_ERR
            } else {
                PPC_MEM_FULL_ERR
            };
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyMemoryUtilityOperation::SetGrowZone => {
            memory.write_u32_be(PPC_APPLICATION_ZONE + 16, cpu.gpr[3])?;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyMemoryUtilityOperation::StackSpace => Some(PpcImportAction::Return(
            cpu.gpr[1].saturating_sub(process_memory_manager.application_heap_limit(heap_limit)),
        )),
        PpcLegacyMemoryUtilityOperation::TempFreeMem => Some(PpcImportAction::Return(
            ppc_heap_free_capacity(memory, *heap_cursor, heap_limit).0,
        )),
    }
}

pub(crate) fn ppc_largest_free_ptr_block(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
    free_ptr_blocks: &[PpcPtrRecord],
) -> u32 {
    free_ptr_blocks
        .iter()
        .filter(|record| ppc_free_ptr_block_fits_limit(record, heap_limit))
        .map(|record| record.size)
        .max()
        .unwrap_or(0)
        .max(ppc_heap_free_capacity(memory, heap_cursor, heap_limit).1)
}

pub(crate) fn ppc_free_ptr_block_fits_limit(record: &ProcessPtrRecord, heap_limit: u32) -> bool {
    ppc_allocation_size(record.size)
        .and_then(|capacity| record.ptr.checked_add(capacity))
        .is_some_and(|end| end <= heap_limit)
}

pub(crate) fn ppc_memory_can_write_bytes(memory: &mut PpcSectionMem, addr: u32, len: u32) -> bool {
    memory.preflight_writable_range(addr, len)
}

pub(crate) fn ppc_init_zone_header(
    memory: &mut PpcSectionMem,
    start: u32,
    limit: u32,
    more_masters_raw: u16,
    grow_zone: u32,
) {
    const ZONE_HEADER_SIZE: u32 = 52;

    let Some(header_end) = start.checked_add(ZONE_HEADER_SIZE) else {
        return;
    };
    if limit < header_end || !ppc_memory_can_write_bytes(memory, start, ZONE_HEADER_SIZE) {
        return;
    }

    let more_masters = i16::from_be_bytes(more_masters_raw.to_be_bytes()).max(0) as u32;
    let free_bytes = limit
        .saturating_sub(start)
        .saturating_sub(72 + (4 * more_masters));
    let first_master_ptr = if more_masters == 0 {
        0
    } else {
        start.wrapping_add(60)
    };

    let _ = memory.write_u32_be(start, limit); // bkLim
    let _ = memory.write_u32_be(start + 4, 0); // purgePtr
    let _ = memory.write_u32_be(start + 8, first_master_ptr); // hFstFree
    let _ = memory.write_u32_be(start + 12, free_bytes); // zcbFree
    let _ = memory.write_u32_be(start + 16, grow_zone); // gzProc
    let _ = memory.write_u16_be(start + 20, more_masters_raw); // moreMast
    let _ = memory.write_u16_be(start + 22, 0); // flags
    let _ = memory.write_u16_be(start + 24, 0); // cntRel
    let _ = memory.write_u16_be(start + 26, 0); // maxRel
    let _ = memory.write_u16_be(start + 28, 0); // cntNRel
    let _ = memory.write_u16_be(start + 30, 0); // maxNRel
    let _ = memory.write_u16_be(start + 32, 0); // cntEmpty
    let _ = memory.write_u16_be(start + 34, 0); // cntHandles
    let _ = memory.write_u32_be(start + 36, free_bytes); // minCBFree
    let _ = memory.write_u32_be(start + 40, 0); // purgeProc
    let _ = memory.write_u32_be(start + 44, 0); // sparePtr
    let _ = memory.write_u32_be(start + 48, start + ZONE_HEADER_SIZE); // allocPtr
    let _ = memory.write_u32_be(PPC_THE_ZONE_ADDR, start);
}

pub(crate) fn ppc_memory_can_read_bytes(memory: &mut PpcSectionMem, addr: u32, len: u32) -> bool {
    for offset in 0..len {
        let Some(byte_addr) = addr.checked_add(offset) else {
            return false;
        };
        if memory.read_u8(byte_addr).is_none() {
            return false;
        }
    }
    true
}

pub(crate) fn ppc_memory_read_bytes(
    memory: &mut PpcSectionMem,
    addr: u32,
    len: u32,
) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(usize::try_from(len).ok()?);
    for offset in 0..len {
        bytes.push(memory.read_u8(addr.checked_add(offset)?)?);
    }
    Some(bytes)
}

pub(crate) fn ppc_optional_output_can_write(
    memory: &mut PpcSectionMem,
    addr: u32,
    len: u32,
) -> bool {
    addr == 0 || ppc_memory_can_write_bytes(memory, addr, len)
}

pub(crate) fn ppc_optional_pstring_output_can_write(
    memory: &mut PpcSectionMem,
    addr: u32,
    bytes: &[u8],
) -> bool {
    let len = bytes.len().min(255) as u32;
    addr == 0 || ppc_memory_can_write_bytes(memory, addr, len + 1)
}

pub(crate) fn ppc_heap_can_alloc_sequence(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
    sizes: &[u32],
) -> bool {
    let mut simulated_cursor = heap_cursor;
    for size in sizes {
        let Some(aligned_size) = ppc_allocation_size(*size) else {
            return false;
        };
        let Some((_, next)) =
            ppc_heap_allocation_bounds(memory, simulated_cursor, heap_limit, aligned_size)
        else {
            return false;
        };
        simulated_cursor = next;
    }
    true
}

pub(crate) fn ppc_heap_can_alloc_repeated(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
    size: u32,
    count: u32,
) -> bool {
    let Some(aligned_size) = ppc_allocation_size(size) else {
        return false;
    };
    let mut simulated_cursor = heap_cursor;
    for _ in 0..count {
        let Some((_, next)) =
            ppc_heap_allocation_bounds(memory, simulated_cursor, heap_limit, aligned_size)
        else {
            return false;
        };
        simulated_cursor = next;
    }
    true
}

#[cfg(test)]
pub(crate) fn ppc_heap_allocation_sequence_size(sizes: &[u32]) -> Option<u32> {
    sizes.iter().try_fold(0u32, |total_size, size| {
        total_size.checked_add(ppc_allocation_size(*size)?)
    })
}

pub(crate) fn ppc_allocation_size(size: u32) -> Option<u32> {
    // The native Power Mac Memory Manager returns NewPtr/NewHandle storage on
    // 16-byte boundaries. Native renderers depend on this stronger guarantee:
    // for example, packed mask blitters derive their starting bit from the low
    // address bits of a PixMap's pixel storage.
    Some(size.checked_add(PPC_HEAP_ALIGNMENT - 1)? & !(PPC_HEAP_ALIGNMENT - 1))
        .map(|size| size.max(PPC_HEAP_ALIGNMENT))
}

pub(crate) fn ppc_heap_free_capacity(
    memory: &PpcSectionMem,
    heap_cursor: u32,
    heap_limit: u32,
) -> (u32, u32) {
    memory.readonly_allocation_available_bytes(heap_cursor, heap_limit)
}
