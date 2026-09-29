//! PowerPC loaded application memory, heap partition, and native zone methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    /// Return the fixed base of the process-owned native heap.
    pub fn heap_base(&self) -> u32 {
        self.process_memory_manager
            .0
            .borrow()
            .native_heap_state()
            .map_or(PPC_HEAP_BASE, |heap| heap.heap_base)
    }

    /// Return the next address in the process-owned native heap.
    pub fn heap_cursor(&self) -> u32 {
        self.process_memory_manager.heap_cursor(PPC_HEAP_BASE)
    }

    /// Return the upper bound of the process-owned native heap.
    pub fn heap_limit(&self) -> u32 {
        self.process_memory_manager.heap_limit(self.stack_base)
    }

    /// Return the process-owned application heap limit used by
    /// `GetApplLimit`, distinct from the native heap mapping ceiling.
    pub fn application_heap_limit(&self) -> u32 {
        self.process_memory_manager
            .application_heap_limit(self.stack_base)
    }

    /// Return the last process-owned native Memory Manager error.
    pub fn last_mem_error(&self) -> i16 {
        self.process_memory_manager.last_mem_error()
    }

    /// Report whether `MaxApplZone` has expanded the process-owned native heap.
    pub fn heap_maximized(&self) -> bool {
        self.process_memory_manager
            .0
            .borrow()
            .native_heap_state()
            .is_some_and(|heap| heap.heap_maximized)
    }

    /// Return the number of process-owned master-pointer growth requests.
    pub fn master_pointer_blocks_requested(&self) -> u32 {
        self.process_memory_manager
            .0
            .borrow()
            .native_heap_state()
            .map_or(0, |heap| heap.master_pointer_blocks_requested)
    }

    /// Return the process-owned native handle states for this adapter's handles.
    pub fn handle_states(&self) -> Vec<PpcHandleStateRecord> {
        let memory_manager = self.process_memory_manager.0.borrow();
        memory_manager
            .native_handle_records()
            .iter()
            .map(|record| memory_manager.native_handle_state(record.handle))
            .collect()
    }

    /// Return the process-owned native relocatable allocation records.
    pub fn handles(&self) -> Vec<PpcHandleRecord> {
        self.process_memory_manager.handles()
    }

    /// Return the process-owned native pointer allocation records.
    pub fn ptrs(&self) -> Vec<PpcPtrRecord> {
        self.process_memory_manager
            .0
            .borrow()
            .native_allocator_snapshot()
            .map_or_else(Vec::new, |allocator| allocator.ptrs)
    }

    /// Return the process-owned native pointer free list.
    pub fn free_ptr_blocks(&self) -> Vec<PpcPtrRecord> {
        self.process_memory_manager
            .0
            .borrow()
            .native_allocator_snapshot()
            .map_or_else(Vec::new, |allocator| allocator.free_ptr_blocks)
    }

    /// Return the process-owned native handle free list.
    pub fn free_handle_blocks(&self) -> Vec<PpcHandleRecord> {
        self.process_memory_manager
            .0
            .borrow()
            .native_allocator_snapshot()
            .map_or_else(Vec::new, |allocator| allocator.free_handle_blocks)
    }

    #[cfg(test)]
    pub(crate) fn replace_handle_states(&self, handle_states: Vec<PpcHandleStateRecord>) {
        let mut memory_manager = self.process_memory_manager.0.borrow_mut();
        let handles = memory_manager.native_handle_records().to_vec();
        memory_manager.register_native_handle_records(handles.iter().map(|record| {
            (
                *record,
                ppc_process_handle_state_bits(&handle_states, record.handle),
            )
        }));
        for state in handle_states {
            memory_manager.set_native_handle_state(state);
        }
    }

    #[cfg(test)]
    pub(crate) fn replace_ptr_allocator_records(
        &self,
        ptrs: Vec<PpcPtrRecord>,
        free_ptr_blocks: Vec<PpcPtrRecord>,
    ) {
        let mut memory_manager = self.process_memory_manager.0.borrow_mut();
        if memory_manager.has_native_allocator() {
            memory_manager.mutate_native_allocator(|allocator| {
                allocator.ptrs = ptrs;
                allocator.free_ptr_blocks = free_ptr_blocks;
            });
        } else {
            memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor: self.heap_cursor(),
                    heap_limit: self.stack_base,
                    last_mem_error: 0,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &ptrs,
                &free_ptr_blocks,
                &[],
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn set_heap_cursor(&self, heap_cursor: u32) {
        let mut memory_manager = self.process_memory_manager.0.borrow_mut();
        if memory_manager.has_native_allocator() {
            memory_manager.mutate_native_allocator(|allocator| {
                allocator.heap.heap_cursor = heap_cursor;
            });
        } else {
            memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor,
                    heap_limit: self.stack_base,
                    last_mem_error: 0,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn with_ptr_allocator_records<R>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut Vec<PpcPtrRecord>, &mut Vec<PpcPtrRecord>) -> R,
    ) -> R {
        let mut ptrs = self.ptrs();
        let mut free_ptr_blocks = self.free_ptr_blocks();
        let result = f(self, &mut ptrs, &mut free_ptr_blocks);
        self.replace_ptr_allocator_records(ptrs, free_ptr_blocks);
        result
    }

    #[cfg(test)]
    pub(crate) fn set_heap_limit(&self, heap_limit: u32) {
        let mut memory_manager = self.process_memory_manager.0.borrow_mut();
        if memory_manager.has_native_allocator() {
            memory_manager.mutate_native_allocator(|allocator| {
                allocator.heap.heap_limit = heap_limit;
            });
        } else {
            memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor: self.heap_cursor(),
                    heap_limit,
                    last_mem_error: 0,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn set_last_mem_error(&self, last_mem_error: i16) {
        let mut memory_manager = self.process_memory_manager.0.borrow_mut();
        if memory_manager.has_native_allocator() {
            memory_manager.mutate_native_allocator(|allocator| {
                allocator.heap.last_mem_error = last_mem_error;
            });
        } else {
            memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor: self.heap_cursor(),
                    heap_limit: self.stack_base,
                    last_mem_error,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
        }
    }

    pub(crate) fn refresh_process_native_zone(
        &mut self,
        heap: ProcessNativeHeapState,
        allocation_limit: u32,
    ) {
        ppc_update_zone_free_bytes(&mut self.memory, heap.heap_cursor, allocation_limit);
    }

    pub(crate) fn prepare_shared_system_reservation(&mut self, base: u32, len: u32) -> bool {
        if self.memory.has_readonly_allocation_exclusion(base, len) {
            return true;
        }
        let start = u64::from(base);
        let Some(end) = start.checked_add(u64::from(len)) else {
            return false;
        };
        if len == 0 || end > (1u64 << 32) {
            return false;
        }

        fn mapped_overlaps(memory: &mut PpcSectionMem, start: u64, end: u64) -> bool {
            if start >= end {
                return false;
            }
            memory.mapping_overlaps(
                u32::try_from(start).expect("nonempty guest range starts below 2^32"),
                u32::try_from(end - start).expect("subrange fits reservation length"),
            )
        }

        // The public low-level loader has no runner reservation to exclude.
        // Preserve that source-compatible path by attaching late only when
        // every byte outside the future bump-heap tail is genuinely unmapped.
        let future_start = u64::from(self.heap_cursor());
        let future_end = u64::from(self.heap_limit());
        let before_end = end.min(future_start);
        let after_start = start.max(future_end);
        if mapped_overlaps(&mut self.memory, start, before_end)
            || mapped_overlaps(&mut self.memory, after_start, end)
        {
            return false;
        }
        self.memory
            .add_readonly_allocation_exclusion(base, len)
            .is_some()
    }

    /// Grow the native allocation budget for a Finder-style preferred partition.
    /// The sparse native address space keeps its fixed stack and Toolbox mappings;
    /// those addresses must never become available to the expanding allocator.
    /// Inside Macintosh: Processes (1994), pp. 1-3 and 2-18.
    pub(crate) fn grow_application_partition(&mut self, partition_size: u32) {
        // The application's data section counts against the partition even
        // though it is mapped outside the native heap; launch-time CFM code
        // and container copies in the heap do not. See
        // `PpcLaunchPartitionStorage` and `ppc_exempt_fragment_from_partition`.
        let storage = self.launch_partition_storage;
        let requested_heap = partition_size
            .saturating_sub(self.stack_size)
            .saturating_sub(storage.application_data)
            .saturating_add(storage.outside_partition)
            & !(PPC_HEAP_ALIGNMENT - 1);
        let heap_base = self.heap_base();
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] partition {partition_size} bytes: stack={} application data={} CFM code and containers outside={} heap={requested_heap}",
                self.stack_size, storage.application_data, storage.outside_partition
            );
        }
        if requested_heap <= ppc_heap_free_capacity(&self.memory, heap_base, self.heap_limit()).0 {
            return;
        }
        let fixed_end = PPC_DSP_BACK_SCREEN_BASE + ppc_main_screen_buffer_size();
        let fixed_len = fixed_end - self.stack_base;
        self.memory
            .add_readonly_allocation_exclusion(self.stack_base, fixed_len)
            .expect("native fixed mappings have a valid address range");
        let mut limit = heap_base.saturating_add(requested_heap);
        // Include each reserved gap in the address ceiling without counting its
        // bytes as application memory (including the shared system reservation).
        loop {
            let available = ppc_heap_free_capacity(&self.memory, heap_base, limit).0;
            if available >= requested_heap {
                break;
            }
            let Some(next) = limit.checked_add(requested_heap - available) else {
                return;
            };
            limit = next;
        }
        {
            let mut manager = self.process_memory_manager.0.borrow_mut();
            manager.native_mut().grow_native_heap_limit(limit);
            manager.set_application_heap_limit(limit);
        }
        let _ = self
            .memory
            .write_u32_be(crate::memory::globals::addr::APPL_LIMIT, limit);
        let _ = self.memory.write_u32_be(PPC_APPLICATION_ZONE, limit);
        let _ = self.memory.write_u32_be(PPC_SYSTEM_ZONE, limit);
        let cursor = self.heap_cursor();
        ppc_update_zone_free_bytes(&mut self.memory, cursor, limit);
    }

    /// Reserve system-service allocations before the application can consume
    /// its native partition. Display services may run after no contiguous
    /// application space remains.
    pub(crate) fn reserve_ppc_system_storage(&mut self) {
        let mut cursor = self.heap_cursor();
        let limit = self.heap_limit();
        let mut manager = self.process_memory_manager.0.borrow_mut();
        // Keep the host-owned Toolbox arena as a pinned block near the top of
        // the partition. MaxBlock then describes the largest *contiguous*
        // application block, while a separate tail remains available for
        // later guest handles and resources. This matters for games that
        // allocate nearly all of MaxBlock as an internal memory pool.
        const RESOURCE_TAIL_SIZE: u32 = 2 * 1024 * 1024;
        let descriptor_pool = limit
            .checked_sub(RESOURCE_TAIL_SIZE + PPC_SYSTEM_ALLOCATION_POOL_SIZE)
            .filter(|&base| {
                base > cursor.saturating_add(PPC_SYSTEM_ALLOCATION_POOL_SIZE)
                    && self
                        .memory
                        .readonly_allocation_overlap_end(base, PPC_SYSTEM_ALLOCATION_POOL_SIZE)
                        .is_none()
            })
            .map(|base| {
                self.memory
                    .add_readonly_allocation_exclusion(base, PPC_SYSTEM_ALLOCATION_POOL_SIZE)
                    .expect("system arena fits the native heap");
                if !ppc_memory_can_write_bytes(
                    &mut self.memory,
                    base,
                    PPC_SYSTEM_ALLOCATION_POOL_SIZE,
                ) {
                    self.memory
                        .add_region(base, vec![0; PPC_SYSTEM_ALLOCATION_POOL_SIZE as usize]);
                }
                base
            })
            .unwrap_or_else(|| {
                ppc_process_heap_alloc(
                    &mut manager,
                    &mut self.memory,
                    &mut cursor,
                    PPC_SYSTEM_ALLOCATION_POOL_SIZE,
                    true,
                )
            });
        self.toolbox_startup
            .system_allocations
            .reserve(descriptor_pool);
        ppc_update_zone_free_bytes(&mut self.memory, cursor, limit);
    }
}
