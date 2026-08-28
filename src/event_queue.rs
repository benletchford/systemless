//! Architecture-neutral ownership for the Operating System event queue.

use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// A queued Mac event (mouseDown, mouseUp, keyDown, etc.).
///
/// The Operating System Event Manager owns one queue, and `EventRecord.where`
/// uses global coordinates. Inside Macintosh Volume I, I-244 and I-259.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedEvent {
    /// Event type (1=mouseDown, 2=mouseUp, 3=keyDown, etc.).
    pub what: u16,
    /// Event message (key code for key events, window ptr for activate, etc.).
    pub message: u32,
    /// Mouse location at event time, in global Macintosh coordinates.
    pub where_v: i16,
    pub where_h: i16,
    /// Modifier flags.
    pub modifiers: u16,
}

/// One serialized event queue that can be attached to both CPU adapters.
///
/// Ordinary construction owns a private queue. The runner may explicitly
/// attach a second adapter to the same allocation once both adapters are its
/// private children and all access is serialized through a mutable runner
/// borrow.
#[derive(Debug, Default)]
pub(crate) struct SharedEventQueue(Rc<UnsafeCell<VecDeque<QueuedEvent>>>);

impl Clone for SharedEventQueue {
    fn clone(&self) -> Self {
        // A cloned runtime is a snapshot, not another live CPU adapter.
        Self(Rc::new(UnsafeCell::new(self.deref().clone())))
    }
}

impl SharedEventQueue {
    /// Attach another CPU adapter to this queue without copying it.
    ///
    /// # Safety
    ///
    /// Every handle sharing this allocation must remain under one owner that
    /// serializes access. No queue reference may remain live while another
    /// handle reads or mutates the allocation.
    pub(crate) unsafe fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl Deref for SharedEventQueue {
    type Target = VecDeque<QueuedEvent>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: shared handles can only be created under the serialized
        // ownership contract documented by `shared_handle`.
        unsafe { &*self.0.get() }
    }
}

impl DerefMut for SharedEventQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: shared handles can only be created under the serialized
        // ownership contract documented by `shared_handle`.
        unsafe { &mut *self.0.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_handles_have_immediate_bidirectional_visibility() {
        let mut first = SharedEventQueue::default();
        // SAFETY: this test accesses the handles strictly in sequence.
        let mut second = unsafe { first.shared_handle() };
        first.push_back(QueuedEvent {
            what: 3,
            message: 0x1122_3344,
            where_v: 10,
            where_h: 20,
            modifiers: 0x0100,
        });
        assert_eq!(second.front().map(|event| event.message), Some(0x1122_3344));
        second.pop_front();
        assert!(first.is_empty());
    }

    #[test]
    fn clone_is_a_detached_snapshot() {
        let mut live = SharedEventQueue::default();
        live.push_back(QueuedEvent {
            what: 3,
            message: 0x1122_3344,
            where_v: 10,
            where_h: 20,
            modifiers: 0x0100,
        });

        let mut snapshot = live.clone();
        snapshot.front_mut().unwrap().message = 0x5566_7788;

        assert_eq!(live.front().unwrap().message, 0x1122_3344);
        assert_eq!(snapshot.front().unwrap().message, 0x5566_7788);
    }
}
