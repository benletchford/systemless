//! Process-scoped state shared by classic and native CPU adapters.

use crate::event_queue::EventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::memory::bus::SharedRamRegion;
use crate::memory::GuestAddressSpace;
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};

#[derive(Debug)]
struct ProcessMemoryRegion {
    base: u32,
    bytes: SharedRamRegion,
}

/// A deferred byte replacement for a relocatable guest handle.
///
/// This channel lets a serialized 68k execution context request a handle resize
/// and byte update within the native process address space without giving the
/// 68k dispatcher direct allocator ownership over the parked PowerPC heap.
/// Inside Macintosh: Memory (1992), pp. 2-40--2-41.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingHandleByteReplacement {
    pub(crate) handle: u32,
    pub(crate) expected_ptr: u32,
    pub(crate) replacement: Vec<u8>,
}

/// Canonical owner for state that belongs to one emulated process rather than
/// to either of its CPU ABI adapters.
///
/// `FixtureRunner` owns this context and serializes all adapter access through
/// its mutable borrow.
#[derive(Debug, Default)]
pub(crate) struct ProcessContext {
    memory: Vec<ProcessMemoryRegion>,
    event_queue: EventQueue,
    menu_tracking: Option<ProcessMenuTrackingState>,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
    pending_memory_effects: Vec<PendingHandleByteReplacement>,
}

impl ProcessContext {
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

    #[cfg(test)]
    pub(crate) fn pending_memory_effects(&self) -> &[PendingHandleByteReplacement] {
        &self.pending_memory_effects
    }

    #[cfg(test)]
    pub(crate) fn pending_memory_effects_mut(&mut self) -> &mut Vec<PendingHandleByteReplacement> {
        &mut self.pending_memory_effects
    }

    pub(crate) fn take_pending_memory_effects(&mut self) -> Vec<PendingHandleByteReplacement> {
        std::mem::take(&mut self.pending_memory_effects)
    }

    pub(crate) fn has_pending_memory_effects(&self) -> bool {
        !self.pending_memory_effects.is_empty()
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

    pub(crate) fn event_queue_menu_tracking_and_memory_effects_mut(
        &mut self,
    ) -> (
        &mut EventQueue,
        &mut Option<ProcessMenuTrackingState>,
        &mut Vec<PendingHandleByteReplacement>,
    ) {
        (
            &mut self.event_queue,
            &mut self.menu_tracking,
            &mut self.pending_memory_effects,
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
    fn process_context_owns_canonical_pending_memory_effects() {
        let mut context = ProcessContext::default();
        assert!(!context.has_pending_memory_effects());
        assert!(context.pending_memory_effects().is_empty());

        context.pending_memory_effects_mut().push(PendingHandleByteReplacement {
            handle: 0x1000,
            expected_ptr: 0x2000,
            replacement: vec![1, 2, 3, 4],
        });
        assert!(context.has_pending_memory_effects());
        assert_eq!(context.pending_memory_effects().len(), 1);
        assert_eq!(context.pending_memory_effects()[0].handle, 0x1000);

        let taken = context.take_pending_memory_effects();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].handle, 0x1000);
        assert_eq!(taken[0].expected_ptr, 0x2000);
        assert_eq!(taken[0].replacement, vec![1, 2, 3, 4]);
        assert!(!context.has_pending_memory_effects());

        let (queue, menu, effects) = context.event_queue_menu_tracking_and_memory_effects_mut();
        queue.push_back(QueuedEvent {
            what: 1,
            message: 0x1234,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        *menu = Some(crate::menu_manager::test_process_menu_tracking(0x3000));
        effects.push(PendingHandleByteReplacement {
            handle: 0x4000,
            expected_ptr: 0x5000,
            replacement: vec![9, 8, 7],
        });
        assert_eq!(context.event_queue().len(), 1);
        assert!(context.menu_tracking().is_some());
        assert_eq!(context.pending_memory_effects().len(), 1);
    }
}
