//! Thread Manager operations over the process execution owner.
//!
//! ABI adapters retain output-pointer and register handling. This service
//! creates no task registry, snapshots, or independent scheduling state.

use crate::execution_kernel::ExecutionTaskState;
use crate::guest_call::{ExecutionTaskId, SharedGuestCallStack};

// Inside Macintosh: Thread Manager (1999), p. 101.
pub(crate) const THREAD_NOT_FOUND_ERR: i16 = -618;
pub(crate) const THREAD_PROTOCOL_ERR: i16 = -619;

pub(crate) struct ThreadManager<'a> {
    execution: &'a SharedGuestCallStack,
}

impl<'a> ThreadManager<'a> {
    pub(crate) fn new(execution: &'a SharedGuestCallStack) -> Self {
        Self { execution }
    }

    // MacGetCurrentThread / GetCurrentThread, Thread Manager (1999), p. 62.
    pub(crate) fn current_thread(&self) -> u32 {
        self.execution.current_task().thread_id()
    }

    pub(crate) fn resolve_thread(&self, thread: u32) -> u32 {
        if thread <= 1 {
            self.current_thread()
        } else {
            thread
        }
    }

    // GetThreadState, Thread Manager (1999), pp. 45, 63.
    pub(crate) fn state(&self, thread: u32) -> Result<u16, i16> {
        self.execution
            .scheduling_state(ExecutionTaskId::from_thread_id(self.resolve_thread(thread)))
            .map(|state| match state {
                ExecutionTaskState::Ready => 0,
                ExecutionTaskState::Stopped => 1,
                ExecutionTaskState::Running => 2,
            })
            .ok_or(THREAD_NOT_FOUND_ERR)
    }

    // ThreadBeginCritical / ThreadEndCritical, Thread Manager (1999), pp. 69–70.
    pub(crate) fn begin_critical(&self) -> i16 {
        self.execution.begin_critical();
        0
    }

    pub(crate) fn end_critical(&self) -> i16 {
        if self.execution.end_critical() {
            0
        } else {
            THREAD_PROTOCOL_ERR
        }
    }
}
