//! Thread Manager operations over the process execution owner.
//!
//! ABI adapters retain output-pointer and register handling. This service
//! creates no task registry, snapshots, or independent scheduling state.

use crate::execution_kernel::ExecutionTaskState;
use crate::guest_call::{ExecutionTaskId, SharedGuestCallStack, ThreadStorage};
use crate::guest_procedure::GuestIsa;

pub(crate) const DEFAULT_COOPERATIVE_THREAD_STACK_SIZE: u32 = 32 * 1024;

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

    // Thread Manager (1999), pp. 50–55. Size is a signed Macintosh Size;
    // stack minima reflect the corresponding adapter's initial ABI frame.
    pub(crate) fn stack_size(isa: GuestIsa, style: u32, requested: u32) -> Result<u32, i16> {
        let size = if requested == 0 {
            DEFAULT_COOPERATIVE_THREAD_STACK_SIZE
        } else {
            requested
        };
        let minimum = match isa {
            GuestIsa::M68k => 8,
            GuestIsa::PowerPc => 256,
        };
        if style != 1 || size < minimum || size > i32::MAX as u32 {
            Err(-50)
        } else {
            Ok(size)
        }
    }

    /// Prepare every allocation before publishing any pool entry. On failure,
    /// return all reserved storage to the ABI allocator for rollback.
    /// Thread Manager (1999), p. 51 requires all-or-none pool creation.
    pub(crate) fn create_pool(
        &self,
        isa: GuestIsa,
        style: u32,
        count: i16,
        requested: u32,
        mut allocate: impl FnMut(u32) -> Option<ThreadStorage>,
    ) -> Result<(), (i16, Vec<ThreadStorage>)> {
        let size = Self::stack_size(isa, style, requested).map_err(|error| (error, Vec::new()))?;
        if count < 0 {
            return Err((-50, Vec::new()));
        }
        let mut prepared = Vec::new();
        for _ in 0..count {
            let Some(mut storage) = allocate(size) else {
                return Err((-108, prepared));
            };
            storage.result_destination = 0;
            prepared.push(storage);
            if storage.stack_base == 0
                || storage.stack_limit.checked_sub(storage.stack_base) != Some(size)
            {
                return Err((-108, prepared));
            }
        }
        self.execution.publish_thread_pool(isa, prepared);
        Ok(())
    }

    pub(crate) fn free_count(
        &self,
        isa: GuestIsa,
        style: u32,
        minimum_size: u32,
    ) -> Result<u16, i16> {
        if style != 1 || minimum_size > i32::MAX as u32 {
            return Err(-50);
        }
        u16::try_from(self.execution.thread_pool_count(isa, minimum_size)).map_err(|_| -617)
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

    // Thread task references identify the process, not an execution thread.
    // The current runtime hosts one process per service. Preserve the existing
    // process-local opaque token while process construction is migrated.
    // Inside Macintosh: Thread Manager (1999), pp. 46, 73–76.
    pub(crate) fn task_reference(&self) -> u32 {
        ExecutionTaskId::APPLICATION.thread_id()
    }

    pub(crate) fn state_given_task(&self, reference: u32, thread: u32) -> Result<u16, i16> {
        if reference != self.task_reference() {
            return Err(THREAD_PROTOCOL_ERR);
        }
        self.state(thread)
    }

    pub(crate) fn ready_given_task(&self, reference: u32, thread: u32) -> i16 {
        if reference != self.task_reference() {
            return THREAD_PROTOCOL_ERR;
        }
        let task = ExecutionTaskId::from_thread_id(self.resolve_thread(thread));
        match self.execution.scheduling_state(task) {
            None => THREAD_NOT_FOUND_ERR,
            Some(ExecutionTaskState::Stopped) => {
                if self
                    .execution
                    .set_scheduling_state(task, ExecutionTaskState::Ready)
                {
                    0
                } else {
                    THREAD_PROTOCOL_ERR
                }
            }
            Some(_) => THREAD_PROTOCOL_ERR,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_pool_preparation_preserves_existing_entries_and_returns_every_reserved_stack() {
        for isa in [GuestIsa::M68k, GuestIsa::PowerPc] {
            let execution = SharedGuestCallStack::default();
            let manager = ThreadManager::new(&execution);
            let storage = |base| ThreadStorage {
                stack_base: base,
                stack_limit: base + 1024,
                result_destination: 42,
                managed_pointer: isa == GuestIsa::PowerPc,
            };
            manager
                .create_pool(isa, 1, 1, 1024, |_| Some(storage(0x1000)))
                .unwrap();
            let mut allocations = 0;
            let failure = manager
                .create_pool(isa, 1, 3, 1024, |_| {
                    allocations += 1;
                    if allocations == 3 {
                        None
                    } else {
                        Some(storage(0x2000 + allocations * 1024))
                    }
                })
                .unwrap_err();
            assert_eq!(failure.0, -108);
            assert_eq!(failure.1.len(), 2);
            assert!(failure.1.iter().all(|stack| stack.result_destination == 0));
            assert_eq!(manager.free_count(isa, 1, 0), Ok(1));
            assert_eq!(manager.free_count(isa, 1, 2048), Ok(0));
            assert!(manager
                .create_pool(isa, 0, 1, 1024, |_| panic!(
                    "invalid style must not allocate"
                ))
                .is_err());
            assert!(manager
                .create_pool(isa, 1, -1, 1024, |_| panic!(
                    "negative count must not allocate"
                ))
                .is_err());
            assert_eq!(execution.create_task().unwrap().thread_id(), 3);
        }
    }
}
