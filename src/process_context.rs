//! Process-scoped state shared by classic and native CPU adapters.

use crate::event_queue::EventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::memory::bus::SharedRamRegion;
use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
struct ProcessMemoryRegion {
    base: u32,
    bytes: SharedRamRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHandleRecord {
    pub handle: u32,
    pub ptr: u32,
    pub size: u32,
    pub capacity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessPtrRecord {
    pub ptr: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHandleStateRecord {
    pub handle: u32,
    pub locked: bool,
    pub high_locked: bool,
    pub no_purge: bool,
    pub resource: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessNativeHeapState {
    pub(crate) heap_base: u32,
    pub(crate) heap_cursor: u32,
    pub(crate) heap_limit: u32,
    pub(crate) last_mem_error: i16,
    pub(crate) heap_maximized: bool,
    pub(crate) master_pointer_blocks_requested: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessNativeAllocatorState {
    pub(crate) heap: ProcessNativeHeapState,
    pub(crate) ptrs: Vec<ProcessPtrRecord>,
    pub(crate) free_ptr_blocks: Vec<ProcessPtrRecord>,
    pub(crate) free_handle_blocks: Vec<ProcessHandleRecord>,
}

/// Architecture-neutral Memory Manager metadata for one Macintosh process.
///
/// Guest addresses, rather than CPU adapter records, identify relocatable
/// blocks. Keeping the reverse master-pointer index and handle state here
/// gives 68K traps and native imports one canonical registry as allocation
/// itself moves behind this process-level boundary. Inside Macintosh: Memory
/// (1992), pp. 2-12, 2-40--2-41.
#[derive(Debug, Default)]
pub(crate) struct ProcessMemoryManager {
    ptr_to_handle: HashMap<u32, u32>,
    handle_state_bits: HashMap<u32, u8>,
    native_handle_ptrs: HashSet<u32>,
    native_handles: HashSet<u32>,
    native_allocations: HashMap<u32, ProcessHandleRecord>,
    native_allocator: Option<ProcessNativeAllocatorState>,
    native_allocator_dirty: bool,
}

impl ProcessMemoryManager {
    const NATIVE_HEAP_ALIGNMENT: u32 = 16;
    const MEM_FULL_ERR: i16 = -108;
    const NIL_HANDLE_ERR: i16 = -109;
    const NO_ERR: i16 = 0;

    pub(crate) fn merge_metadata(
        &mut self,
        ptr_to_handle: HashMap<u32, u32>,
        handle_state_bits: HashMap<u32, u8>,
    ) {
        self.ptr_to_handle.extend(ptr_to_handle);
        self.handle_state_bits.extend(handle_state_bits);
    }

    pub(crate) fn adopt_metadata(
        &mut self,
        ptr_to_handle: &mut HashMap<u32, u32>,
        handle_state_bits: &mut HashMap<u32, u8>,
    ) {
        assert!(
            self.ptr_to_handle.is_empty() || ptr_to_handle.is_empty(),
            "cannot attach two active pointer-to-handle registries"
        );
        assert!(
            self.handle_state_bits.is_empty() || handle_state_bits.is_empty(),
            "cannot attach two active handle-state registries"
        );
        if self.ptr_to_handle.is_empty() {
            std::mem::swap(&mut self.ptr_to_handle, ptr_to_handle);
        }
        if self.handle_state_bits.is_empty() {
            std::mem::swap(&mut self.handle_state_bits, handle_state_bits);
        }
    }

    #[cfg(test)]
    pub(crate) fn metadata_mut(
        &mut self,
    ) -> (&mut HashMap<u32, u32>, &mut HashMap<u32, u8>) {
        (&mut self.ptr_to_handle, &mut self.handle_state_bits)
    }

    pub(crate) fn take_metadata(&mut self) -> (HashMap<u32, u32>, HashMap<u32, u8>) {
        (
            std::mem::take(&mut self.ptr_to_handle),
            std::mem::take(&mut self.handle_state_bits),
        )
    }

    pub(crate) fn register_native_handle_records(
        &mut self,
        handles: impl IntoIterator<Item = (ProcessHandleRecord, u8)>,
    ) {
        for ptr in self.native_handle_ptrs.drain() {
            self.ptr_to_handle.remove(&ptr);
        }
        for handle in self.native_handles.drain() {
            self.handle_state_bits.remove(&handle);
        }
        self.native_allocations.clear();
        for (record, state) in handles {
            let ProcessHandleRecord { handle, ptr, .. } = record;
            if handle != 0 && ptr != 0 {
                self.ptr_to_handle.insert(ptr, handle);
                self.native_handle_ptrs.insert(ptr);
                self.handle_state_bits.insert(handle, state);
                self.native_handles.insert(handle);
                self.native_allocations.insert(handle, record);
            }
        }
    }

    pub(crate) fn state_for_handle(&self, handle: u32) -> Option<u8> {
        self.handle_state_bits.get(&handle).copied()
    }

    pub(crate) fn native_allocation(&self, handle: u32) -> Option<ProcessHandleRecord> {
        self.native_allocations.get(&handle).copied()
    }

    fn native_allocation_size(size: u32) -> Option<u32> {
        Some(
            size.checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
                & !(Self::NATIVE_HEAP_ALIGNMENT - 1),
        )
        .map(|size| size.max(Self::NATIVE_HEAP_ALIGNMENT))
    }

    fn native_allocation_bounds(
        bus: &MacMemoryBus,
        heap_cursor: u32,
        heap_limit: u32,
        aligned_size: u32,
    ) -> Option<(u32, u32)> {
        let mut ptr = heap_cursor
            .checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
            & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        loop {
            let next = ptr.checked_add(aligned_size)?;
            if next >= heap_limit {
                return None;
            }
            let Some(reserved_end) =
                bus.foreign_readonly_allocation_overlap_end(ptr, aligned_size)
            else {
                return Some((ptr, next));
            };
            ptr = reserved_end
                .checked_add(Self::NATIVE_HEAP_ALIGNMENT - 1)?
                & !(Self::NATIVE_HEAP_ALIGNMENT - 1);
        }
    }

    fn set_native_mem_error(&mut self, error: i16) {
        if let Some(allocator) = &mut self.native_allocator {
            allocator.heap.last_mem_error = error;
            self.native_allocator_dirty = true;
        }
    }

    /// Replace a native relocatable block while its process address space is
    /// attached to the serialized 68K adapter.
    ///
    /// A handle remains stable while its master pointer may change when the
    /// block grows. Inside Macintosh: Memory (1992), pp. 1-18--1-19 and
    /// 2-40--2-41.
    pub(crate) fn replace_native_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        expected_ptr: u32,
        bytes: &[u8],
    ) -> Result<(u32, u32), i16> {
        let Some(record) = self.native_allocations.get(&handle).copied() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        };
        let current_ptr = bus.read_long(handle);
        if current_ptr == 0 || current_ptr != expected_ptr || record.ptr != current_ptr {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }
        let Ok(size) = u32::try_from(bytes.len()) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let Some(new_aligned) = Self::native_allocation_size(size) else {
            self.set_native_mem_error(Self::MEM_FULL_ERR);
            return Err(Self::MEM_FULL_ERR);
        };
        let Some(allocator) = self.native_allocator.as_ref() else {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        };

        let mut new_ptr = record.ptr;
        let mut new_cursor = allocator.heap.heap_cursor;
        let mut new_capacity = record.capacity;
        if size > record.capacity {
            let Some(old_aligned) = Self::native_allocation_size(record.capacity) else {
                self.set_native_mem_error(Self::MEM_FULL_ERR);
                return Err(Self::MEM_FULL_ERR);
            };
            let can_extend_last = record.ptr.checked_add(old_aligned)
                == Some(allocator.heap.heap_cursor)
                && Self::native_allocation_bounds(
                    bus,
                    record.ptr,
                    allocator.heap.heap_limit,
                    new_aligned,
                )
                .is_some_and(|(ptr, _)| ptr == record.ptr);
            if can_extend_last {
                new_cursor = record.ptr + new_aligned;
            } else {
                let Some((ptr, next)) = Self::native_allocation_bounds(
                    bus,
                    allocator.heap.heap_cursor,
                    allocator.heap.heap_limit,
                    new_aligned,
                ) else {
                    self.set_native_mem_error(Self::MEM_FULL_ERR);
                    return Err(Self::MEM_FULL_ERR);
                };
                new_ptr = ptr;
                new_cursor = next;
            }
            new_capacity = size;
        }

        if bus.write_foreign_bytes(new_ptr, bytes).is_none()
            || (new_ptr != current_ptr
                && bus
                    .write_foreign_bytes(handle, &new_ptr.to_be_bytes())
                    .is_none())
        {
            self.set_native_mem_error(Self::NIL_HANDLE_ERR);
            return Err(Self::NIL_HANDLE_ERR);
        }

        let updated = ProcessHandleRecord {
            handle,
            ptr: new_ptr,
            size,
            capacity: new_capacity,
        };
        self.native_allocations.insert(handle, updated);
        self.native_handle_ptrs.remove(&current_ptr);
        self.native_handle_ptrs.insert(new_ptr);
        let allocator = self
            .native_allocator
            .as_mut()
            .expect("native allocator remains registered");
        allocator.heap.heap_cursor = new_cursor;
        allocator.heap.last_mem_error = Self::NO_ERR;
        self.native_allocator_dirty = true;
        Ok((current_ptr, new_ptr))
    }

    pub(crate) fn publish_native_allocator(
        &mut self,
        heap: ProcessNativeHeapState,
        ptrs: &[ProcessPtrRecord],
        free_ptr_blocks: &[ProcessPtrRecord],
        free_handle_blocks: &[ProcessHandleRecord],
    ) {
        let allocator = self
            .native_allocator
            .get_or_insert_with(|| ProcessNativeAllocatorState {
                heap,
                ptrs: Vec::new(),
                free_ptr_blocks: Vec::new(),
                free_handle_blocks: Vec::new(),
            });
        allocator.heap = heap;
        if allocator.ptrs != ptrs {
            allocator.ptrs.clear();
            allocator.ptrs.extend_from_slice(ptrs);
        }
        if allocator.free_ptr_blocks != free_ptr_blocks {
            allocator.free_ptr_blocks.clear();
            allocator.free_ptr_blocks.extend_from_slice(free_ptr_blocks);
        }
        if allocator.free_handle_blocks != free_handle_blocks {
            allocator.free_handle_blocks.clear();
            allocator
                .free_handle_blocks
                .extend_from_slice(free_handle_blocks);
        }
        self.native_allocator_dirty = false;
    }

    pub(crate) fn native_allocator_update(&self) -> Option<ProcessNativeAllocatorState> {
        self.native_allocator_dirty
            .then(|| self.native_allocator.clone())
            .flatten()
    }

    #[cfg(test)]
    pub(crate) fn native_allocator(&self) -> Option<&ProcessNativeAllocatorState> {
        self.native_allocator.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn set_native_allocation(&mut self, record: ProcessHandleRecord) {
        self.native_allocations.insert(record.handle, record);
    }

    #[cfg(test)]
    pub(crate) fn mutate_native_allocator(
        &mut self,
        mutation: impl FnOnce(&mut ProcessNativeAllocatorState),
    ) {
        mutation(
            self.native_allocator
                .as_mut()
                .expect("native allocator registered"),
        );
        self.native_allocator_dirty = true;
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.ptr_to_handle.get(&ptr).copied()
    }

    #[cfg(test)]
    pub(crate) fn handle_state(&self, handle: u32) -> u8 {
        self.handle_state_bits.get(&handle).copied().unwrap_or(0)
    }
}

/// Canonical owner for state that belongs to one emulated process rather than
/// to either of its CPU ABI adapters.
///
/// `FixtureRunner` owns this context and serializes all adapter access through
/// its mutable borrow.
#[derive(Debug, Default)]
pub(crate) struct ProcessContext {
    memory: Vec<ProcessMemoryRegion>,
    memory_manager: ProcessMemoryManager,
    event_queue: EventQueue,
    menu_tracking: Option<ProcessMenuTrackingState>,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
}

impl ProcessContext {
    pub(crate) fn memory_manager_mut(&mut self) -> &mut ProcessMemoryManager {
        &mut self.memory_manager
    }

    #[cfg(test)]
    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.memory_manager.handle_for_ptr(ptr)
    }

    pub(crate) fn adopt_memory_manager_metadata(
        &mut self,
        ptr_to_handle: &mut HashMap<u32, u32>,
        handle_state_bits: &mut HashMap<u32, u8>,
    ) {
        self.memory_manager
            .adopt_metadata(ptr_to_handle, handle_state_bits);
    }

    /// Install a canonical process-memory allocation and attach a CPU
    /// address-space adapter to it.
    ///
    /// Repeated attachment is allowed for another adapter (or a relaunched
    /// native fragment), but each range must either match an existing region
    /// exactly or remain disjoint from every region already owned here.
    pub(crate) fn attach_memory(
        &mut self,
        base: u32,
        bytes: SharedRamRegion,
        adapter: &mut GuestAddressSpace,
    ) {
        let len = bytes.len();
        let memory_index = self
            .memory
            .iter()
            .position(|memory| memory.base == base && memory.bytes.len() == len)
            .unwrap_or_else(|| {
                let start = u64::from(base);
                let end = start.saturating_add(len as u64);
                assert!(
                    self.memory.iter().all(|memory| {
                        let memory_start = u64::from(memory.base);
                        let memory_end = memory_start.saturating_add(memory.bytes.len() as u64);
                        end <= memory_start || memory_end <= start
                    }),
                    "cannot overlap process memory regions"
                );
                self.memory.push(ProcessMemoryRegion { base, bytes });
                self.memory.len() - 1
            });

        let memory = &self.memory[memory_index];
        // SAFETY: `ProcessContext` and all attached CPU adapters are private
        // children of one runner. Every execution entry point requires an
        // exclusive mutable runner borrow, so adapter access is serialized.
        unsafe {
            adapter.add_shared_region(memory.base, memory.bytes.clone());
        }
    }

    #[cfg(test)]
    pub(crate) fn memory_ranges(&self) -> Vec<(u32, usize)> {
        self.memory
            .iter()
            .map(|memory| (memory.base, memory.bytes.len()))
            .collect()
    }

    pub(crate) fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }

    pub(crate) fn event_queue_mut(&mut self) -> &mut EventQueue {
        &mut self.event_queue
    }

    pub(crate) fn menu_tracking(&self) -> Option<&ProcessMenuTrackingState> {
        self.menu_tracking.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn menu_tracking_mut(&mut self) -> Option<&mut ProcessMenuTrackingState> {
        self.menu_tracking.as_mut()
    }

    #[cfg(test)]
    pub(crate) fn take_menu_tracking(&mut self) -> Option<ProcessMenuTrackingState> {
        self.menu_tracking.take()
    }

    #[cfg(test)]
    pub(crate) fn set_menu_tracking(&mut self, state: Option<ProcessMenuTrackingState>) {
        self.menu_tracking = state;
    }

    #[cfg(test)]
    pub(crate) fn menu_tracking_slot_mut(&mut self) -> &mut Option<ProcessMenuTrackingState> {
        &mut self.menu_tracking
    }

    /// Transfer detached adapter state into the canonical process owner.
    pub(crate) fn adopt_menu_tracking(&mut self, adapter: &mut Option<ProcessMenuTrackingState>) {
        assert!(
            adapter.is_none() || self.menu_tracking.is_none(),
            "cannot attach two active Menu Manager continuations"
        );
        if self.menu_tracking.is_none() {
            self.menu_tracking = adapter.take();
        }
    }

    /// Borrow the process state temporarily installed in an active CPU adapter.
    pub(crate) fn event_queue_and_menu_tracking_mut(
        &mut self,
    ) -> (&mut EventQueue, &mut Option<ProcessMenuTrackingState>) {
        (&mut self.event_queue, &mut self.menu_tracking)
    }

    pub(crate) fn event_queue_menu_tracking_and_memory_manager_mut(
        &mut self,
    ) -> (
        &mut EventQueue,
        &mut Option<ProcessMenuTrackingState>,
        &mut ProcessMemoryManager,
    ) {
        (
            &mut self.event_queue,
            &mut self.menu_tracking,
            &mut self.memory_manager,
        )
    }

    pub(crate) fn attach_native_menu_selection(&self, adapter: &mut SharedNativeMenuSelection) {
        adapter.attach_to(&self.pending_native_menu_selection);
    }

    pub(crate) fn attach_guest_calls(&self, adapter: &mut SharedGuestCallStack) {
        adapter.attach_to(&self.guest_calls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_queue::QueuedEvent;
    use crate::guest_call::GuestCallTarget;
    use crate::guest_procedure::GuestIsa;
    use crate::memory::{MacMemoryBus, MemoryBus};
    use ppc::PpcMemory;

    #[test]
    fn process_context_owns_the_memory_mapping_for_cpu_adapters() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(0x2000);
        bus.write_long(0x100, 0x1234_5678);
        let region = bus.shared_ram_region(0, 0x1000).unwrap();
        let mut native = GuestAddressSpace::new();

        context.attach_memory(0, region, &mut native);

        assert_eq!(context.memory_ranges(), vec![(0, 0x1000)]);
        assert_eq!(native.read_u32_be(0x100), Some(0x1234_5678));
        native.write_u32_be(0x100, 0x89ab_cdef).unwrap();
        assert_eq!(bus.read_long(0x100), 0x89ab_cdef);
    }

    #[test]
    fn process_context_owns_multiple_regions_and_clones_detach_from_all_of_them() {
        let mut context = ProcessContext::default();
        let mut bus = MacMemoryBus::new(0x5000);
        bus.write_long(0x100, 0x1122_3344);
        bus.write_long(0x3100, 0x5566_7788);
        let low = bus.shared_ram_region(0, 0x1000).unwrap();
        let high = bus.shared_ram_region(0x3000, 0x1000).unwrap();
        let mut native = GuestAddressSpace::new();

        context.attach_memory(0, low, &mut native);
        context.attach_memory(0x3000, high, &mut native);
        assert_eq!(
            context.memory_ranges(),
            vec![(0, 0x1000), (0x3000, 0x1000)]
        );

        let mut detached = native.clone();
        native.write_u32_be(0x100, 0x99aa_bbcc).unwrap();
        native.write_u32_be(0x3100, 0xddee_ff00).unwrap();
        assert_eq!(bus.read_long(0x100), 0x99aa_bbcc);
        assert_eq!(bus.read_long(0x3100), 0xddee_ff00);
        assert_eq!(detached.read_u32_be(0x100), Some(0x1122_3344));
        assert_eq!(detached.read_u32_be(0x3100), Some(0x5566_7788));

        detached.write_u32_be(0x100, 0x0102_0304).unwrap();
        detached.write_u32_be(0x3100, 0x0506_0708).unwrap();
        assert_eq!(bus.read_long(0x100), 0x99aa_bbcc);
        assert_eq!(bus.read_long(0x3100), 0xddee_ff00);
    }

    #[test]
    fn process_context_owns_canonical_event_queue() {
        let mut context = ProcessContext::default();
        assert!(context.event_queue().is_empty());
        context.event_queue_mut().push_back(QueuedEvent {
            what: 1,
            message: 0x1234,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
        assert_eq!(context.event_queue().len(), 1);
        assert_eq!(context.event_queue().front().unwrap().message, 0x1234);
    }

    #[test]
    fn process_context_owns_canonical_menu_tracking() {
        let mut context = ProcessContext::default();
        assert!(context.menu_tracking().is_none());

        let tracking = crate::menu_manager::test_process_menu_tracking(0x0012_3456);
        context.set_menu_tracking(Some(tracking));
        assert_eq!(
            context.menu_tracking().map(|t| t.menu_handle),
            Some(0x0012_3456)
        );

        if let Some(t) = context.menu_tracking_mut() {
            t.highlighted_item = 3;
        }
        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x0012_3456, 3))
        );

        let taken = context.take_menu_tracking();
        assert_eq!(taken.map(|t| t.menu_handle), Some(0x0012_3456));
        assert!(context.menu_tracking().is_none());

        let (queue, menu) = context.event_queue_and_menu_tracking_mut();
        queue.push_back(QueuedEvent {
            what: 2,
            message: 0x5678,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        *menu = Some(crate::menu_manager::test_process_menu_tracking(0x0065_4321));
        assert_eq!(context.event_queue().len(), 1);
        assert_eq!(
            context.menu_tracking().map(|t| t.menu_handle),
            Some(0x0065_4321)
        );
    }

    #[test]
    fn adapters_transfer_pending_state_and_share_one_process_owner() {
        let context = ProcessContext::default();

        let mut classic_selection = SharedNativeMenuSelection::default();
        assert!(classic_selection.stage((128, 2)));
        let mut native_selection = SharedNativeMenuSelection::default();
        context.attach_native_menu_selection(&mut classic_selection);
        context.attach_native_menu_selection(&mut native_selection);
        assert_eq!(native_selection.take(), Some((128, 2)));
        assert!(classic_selection.is_none());

        let mut classic_calls = SharedGuestCallStack::default();
        classic_calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );
        let mut native_calls = SharedGuestCallStack::default();
        context.attach_guest_calls(&mut classic_calls);
        context.attach_guest_calls(&mut native_calls);
        assert_eq!(native_calls.len(), 1);
        assert!(native_calls.complete_m68k(0x2002, 0x3000));
        assert!(classic_calls.is_empty());
    }

    #[test]
    #[should_panic(expected = "cannot attach two active Menu Manager continuations")]
    fn adopting_two_active_menu_continuations_is_always_rejected() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x1000,
        )));
        let mut second = Some(crate::menu_manager::test_process_menu_tracking(0x2000));
        context.adopt_menu_tracking(&mut second);
    }

    #[test]
    #[should_panic(expected = "cannot attach two pending native menu selections")]
    fn attaching_two_pending_native_selections_is_always_rejected() {
        let context = ProcessContext::default();
        let mut first = SharedNativeMenuSelection::default();
        let mut second = SharedNativeMenuSelection::default();
        first.stage((128, 1));
        second.stage((129, 2));
        context.attach_native_menu_selection(&mut first);
        context.attach_native_menu_selection(&mut second);
    }

    #[test]
    #[should_panic(expected = "cannot attach two active guest-procedure continuation stacks")]
    fn attaching_two_active_guest_call_stacks_is_always_rejected() {
        fn begin_call(calls: &SharedGuestCallStack, entry: u32) {
            calls.begin_m68k(
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry,
                    rtoc: 0,
                },
                entry + 2,
                0x3000,
            );
        }

        let context = ProcessContext::default();
        let mut first = SharedGuestCallStack::default();
        let mut second = SharedGuestCallStack::default();
        begin_call(&first, 0x1000);
        begin_call(&second, 0x2000);
        context.attach_guest_calls(&mut first);
        context.attach_guest_calls(&mut second);
    }

    #[test]
    fn process_memory_manager_relocates_native_handle_immediately_through_68k_bus() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 16,
            },
            0,
        )]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
        let replacement = vec![0x5a; 48];
        let relocated = manager
            .replace_native_handle_bytes(&mut bus, handle, old_ptr, &replacement)
            .unwrap();

        assert_eq!(relocated, (old_ptr, heap_cursor));
        assert_eq!(bus.read_long(handle), heap_cursor);
        assert_eq!(bus.read_bytes(heap_cursor, replacement.len()), replacement);
        assert_eq!(
            manager.native_allocation(handle),
            Some(ProcessHandleRecord {
                handle,
                ptr: heap_cursor,
                size: 48,
                capacity: 48,
            })
        );
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.heap_cursor),
            Some(heap_cursor + 48)
        );
    }

    #[test]
    fn process_memory_manager_preserves_native_handle_when_growth_exhausts_heap() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x100]);
        native.write_u32_be(handle, old_ptr).unwrap();
        native.write_bytes(old_ptr, b"original").unwrap();

        let mut manager = ProcessMemoryManager::default();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: heap_cursor,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let original = ProcessHandleRecord {
            handle,
            ptr: old_ptr,
            size: 8,
            capacity: 16,
        };
        manager.register_native_handle_records([(original, 0)]);

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = unsafe { native.shared_view() };
        unsafe { bus.attach_guest_address_space(shared) };
        assert_eq!(
            manager.replace_native_handle_bytes(&mut bus, handle, old_ptr, &[0x5a; 48]),
            Err(ProcessMemoryManager::MEM_FULL_ERR)
        );
        assert_eq!(bus.read_long(handle), old_ptr);
        assert_eq!(bus.read_bytes(old_ptr, 8), b"original");
        assert_eq!(manager.native_allocation(handle), Some(original));
        assert_eq!(
            manager
                .native_allocator_update()
                .map(|allocator| allocator.heap.last_mem_error),
            Some(ProcessMemoryManager::MEM_FULL_ERR)
        );
    }

    #[test]
    fn native_handle_registration_tracks_relocation_without_discarding_classic_handles() {
        let mut manager = ProcessMemoryManager::default();
        manager.merge_metadata(HashMap::from([(0x2200, 0x1100)]), HashMap::new());

        manager.register_native_handle_records([
            (
                ProcessHandleRecord {
                    handle: 0x3300,
                    ptr: 0x4400,
                    size: 16,
                    capacity: 32,
                },
                0x80,
            ),
            (
                ProcessHandleRecord {
                    handle: 0x5500,
                    ptr: 0x6600,
                    size: 48,
                    capacity: 64,
                },
                0x40,
            ),
        ]);
        assert_eq!(manager.handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(manager.handle_for_ptr(0x4400), Some(0x3300));
        assert_eq!(manager.handle_for_ptr(0x6600), Some(0x5500));
        assert_eq!(manager.handle_state(0x3300), 0x80);
        assert_eq!(manager.handle_state(0x5500), 0x40);
        assert_eq!(manager.native_allocation(0x3300).unwrap().size, 16);

        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle: 0x3300,
                ptr: 0x7700,
                size: 80,
                capacity: 96,
            },
            0xc0,
        )]);
        assert_eq!(manager.handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(manager.handle_for_ptr(0x4400), None);
        assert_eq!(manager.handle_for_ptr(0x6600), None);
        assert_eq!(manager.handle_for_ptr(0x7700), Some(0x3300));
        assert_eq!(manager.handle_state(0x3300), 0xc0);
        assert_eq!(manager.handle_state(0x5500), 0);
        assert_eq!(manager.native_allocation(0x3300).unwrap().size, 80);
        assert_eq!(manager.native_allocation(0x5500), None);
    }
}
