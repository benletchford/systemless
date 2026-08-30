//! Process-scoped state shared by classic and native CPU adapters.

use crate::event_queue::SharedEventQueue;
use crate::guest_call::SharedGuestCallStack;
use crate::menu_manager::{SharedMenuTracking, SharedNativeMenuSelection};

/// Canonical owner for state that belongs to one emulated process rather than
/// to either of its CPU ABI adapters.
///
/// `FixtureRunner` owns this context and serializes all adapter access through
/// its mutable borrow. Adapters may still start detached for focused tests,
/// then transfer any pending state when they attach here.
#[derive(Debug, Default)]
pub(crate) struct ProcessContext {
    event_queue: SharedEventQueue,
    menu_tracking: SharedMenuTracking,
    pending_native_menu_selection: SharedNativeMenuSelection,
    guest_calls: SharedGuestCallStack,
}

impl ProcessContext {
    /// # Safety
    ///
    /// The caller must keep the context and adapter under one owner that
    /// serializes all access to the attached handles.
    pub(crate) unsafe fn attach_event_queue(&self, adapter: &mut SharedEventQueue) {
        unsafe { adapter.attach_to(&self.event_queue) };
    }

    /// # Safety
    ///
    /// The caller must keep the context and adapter under one owner that
    /// serializes all access to the attached handles.
    pub(crate) unsafe fn attach_menu_tracking(&self, adapter: &mut SharedMenuTracking) {
        unsafe { adapter.attach_to(&self.menu_tracking) };
    }

    pub(crate) fn attach_native_menu_selection(
        &self,
        adapter: &mut SharedNativeMenuSelection,
    ) {
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

    #[test]
    fn adapters_transfer_pending_state_and_share_one_process_owner() {
        let context = ProcessContext::default();
        let mut classic_events = SharedEventQueue::default();
        classic_events.push_back(QueuedEvent {
            what: 1,
            message: 0x1122_3344,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
        let mut native_events = SharedEventQueue::default();
        // SAFETY: the test accesses both adapters strictly in sequence.
        unsafe {
            context.attach_event_queue(&mut classic_events);
            context.attach_event_queue(&mut native_events);
        }
        assert_eq!(native_events.front().map(|event| event.message), Some(0x1122_3344));
        native_events.pop_front();
        assert!(classic_events.is_empty());

        let mut classic_tracking = SharedMenuTracking::default();
        *classic_tracking = Some(crate::menu_manager::test_process_menu_tracking(0x0012_3456));
        let mut native_tracking = SharedMenuTracking::default();
        // SAFETY: the test accesses both adapters strictly in sequence.
        unsafe {
            context.attach_menu_tracking(&mut classic_tracking);
            context.attach_menu_tracking(&mut native_tracking);
        }
        native_tracking.as_mut().unwrap().highlighted_item = 4;
        assert_eq!(
            classic_tracking
                .as_ref()
                .map(|tracking| (tracking.menu_handle, tracking.highlighted_item)),
            Some((0x0012_3456, 4))
        );

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
    fn attaching_two_active_menu_continuations_is_always_rejected() {
        let context = ProcessContext::default();
        let mut first = SharedMenuTracking::default();
        *first = Some(crate::menu_manager::test_process_menu_tracking(0x1000));
        let mut second = SharedMenuTracking::default();
        *second = Some(crate::menu_manager::test_process_menu_tracking(0x2000));
        // SAFETY: the test accesses attached handles strictly in sequence.
        unsafe {
            context.attach_menu_tracking(&mut first);
            context.attach_menu_tracking(&mut second);
        }
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
}
