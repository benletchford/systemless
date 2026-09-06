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

pub(crate) trait NewThreadCreationEdge {
    /// Validate and prepare ABI-local state before storage selection. Edge
    /// operations are sequential, and no borrow may survive guest execution.
    fn preflight(&mut self, size: u32) -> Result<(), i16>;

    fn allocate_fresh(&mut self, size: u32) -> Result<ThreadStorage, i16>;

    /// Publish ABI state and the execution task together. `None` means the
    /// execution owner refused publication without consuming a task identity.
    fn prepare_and_publish(
        &mut self,
        execution: &SharedGuestCallStack,
        storage: ThreadStorage,
        suspended: bool,
    ) -> Result<Option<ExecutionTaskId>, i16>;

    fn release_fresh(&mut self, storage: ThreadStorage);

    fn finish_publication_attempt(&mut self);
}

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

    /// Apply the common NewThread policy while leaving ABI frame construction,
    /// allocation, and output publication at the calling edge.
    pub(crate) fn create_thread<E: NewThreadCreationEdge>(
        &self,
        isa: GuestIsa,
        style: u32,
        requested_size: u32,
        options: u32,
        edge: &mut E,
    ) -> Result<ExecutionTaskId, i16> {
        let size = Self::stack_size(isa, style, requested_size)?;
        edge.preflight(size)?;

        let pooled = self.execution.request_thread_stack(isa, size, options)?;
        let (storage, came_from_pool) = match pooled {
            Some(storage) => (storage, true),
            None => (edge.allocate_fresh(size)?, false),
        };
        let result = match edge.prepare_and_publish(self.execution, storage, options & 1 != 0) {
            Ok(Some(task)) => Ok(task),
            Ok(None) => Err(-108),
            Err(error) => Err(error),
        };
        if result.is_err() {
            if came_from_pool {
                self.execution.recycle_thread_stack(isa, storage);
            } else {
                edge.release_fresh(storage);
            }
        }
        edge.finish_publication_attempt();
        result
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

    // Thread Manager (1999), pp. 17–18 and 61: workers have private
    // allocations; the main thread uses the expandable application stack.
    pub(crate) fn stack_space(
        &self,
        thread: u32,
        live_isa: GuestIsa,
        live_sp: u32,
        application_limit: impl FnOnce(GuestIsa) -> u32,
    ) -> Result<u32, i16> {
        let task = ExecutionTaskId::from_thread_id(self.resolve_thread(thread));
        let storage = self
            .execution
            .thread_storage(task)
            .ok_or(THREAD_NOT_FOUND_ERR)?;
        let (isa, sp) = self
            .execution
            .thread_stack_pointer(task, live_isa, live_sp)
            .ok_or(THREAD_PROTOCOL_ERR)?;
        let base = if task == ExecutionTaskId::APPLICATION {
            let application_limit = application_limit(isa);
            if application_limit == 0 {
                return Err(THREAD_PROTOCOL_ERR);
            }
            application_limit
        } else {
            if storage.stack_base == 0
                || storage.stack_limit < storage.stack_base
                || sp > storage.stack_limit
            {
                return Err(THREAD_PROTOCOL_ERR);
            }
            storage.stack_base
        };
        Ok(sp.saturating_sub(base))
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
    use crate::guest_call::CooperativeThread;

    #[derive(Clone, Copy)]
    enum PublicationResult {
        Accept,
        Refuse,
        Error(i16),
    }

    struct RecordingNewThreadEdge {
        preflight_error: Option<i16>,
        allocation: Result<ThreadStorage, i16>,
        publication: PublicationResult,
        preflight_sizes: Vec<u32>,
        allocated_sizes: Vec<u32>,
        published_storage: Vec<ThreadStorage>,
        suspended: Vec<bool>,
        released: Vec<ThreadStorage>,
        finish_count: usize,
    }

    impl RecordingNewThreadEdge {
        fn accepting(storage: ThreadStorage) -> Self {
            Self {
                preflight_error: None,
                allocation: Ok(storage),
                publication: PublicationResult::Accept,
                preflight_sizes: Vec::new(),
                allocated_sizes: Vec::new(),
                published_storage: Vec::new(),
                suspended: Vec::new(),
                released: Vec::new(),
                finish_count: 0,
            }
        }
    }

    impl NewThreadCreationEdge for RecordingNewThreadEdge {
        fn preflight(&mut self, size: u32) -> Result<(), i16> {
            self.preflight_sizes.push(size);
            self.preflight_error.map_or(Ok(()), Err)
        }

        fn allocate_fresh(&mut self, size: u32) -> Result<ThreadStorage, i16> {
            self.allocated_sizes.push(size);
            self.allocation
        }

        fn prepare_and_publish(
            &mut self,
            execution: &SharedGuestCallStack,
            storage: ThreadStorage,
            suspended: bool,
        ) -> Result<Option<ExecutionTaskId>, i16> {
            self.published_storage.push(storage);
            self.suspended.push(suspended);
            match self.publication {
                PublicationResult::Accept => Ok(execution.create_classic_thread(
                    CooperativeThread::default(),
                    storage,
                    suspended,
                    |_| true,
                )),
                PublicationResult::Refuse => Ok(execution.create_classic_thread(
                    CooperativeThread::default(),
                    storage,
                    suspended,
                    |_| false,
                )),
                PublicationResult::Error(error) => Err(error),
            }
        }

        fn release_fresh(&mut self, storage: ThreadStorage) {
            self.released.push(storage);
        }

        fn finish_publication_attempt(&mut self) {
            self.finish_count += 1;
        }
    }

    fn storage(base: u32, size: u32) -> ThreadStorage {
        ThreadStorage {
            stack_base: base,
            stack_limit: base + size,
            result_destination: base + 4,
            managed_pointer: false,
        }
    }

    #[test]
    fn new_thread_creation_validates_before_selecting_or_allocating_storage() {
        let execution = SharedGuestCallStack::default();
        let manager = ThreadManager::new(&execution);
        let mut edge = RecordingNewThreadEdge::accepting(storage(0x1000, 1024));

        assert_eq!(
            manager.create_thread(GuestIsa::M68k, 0, 1024, 4, &mut edge),
            Err(-50)
        );
        assert!(edge.preflight_sizes.is_empty());
        assert!(edge.allocated_sizes.is_empty());

        edge.preflight_error = Some(-37);
        assert_eq!(
            manager.create_thread(GuestIsa::M68k, 1, 1024, 4, &mut edge),
            Err(-37)
        );
        assert_eq!(edge.preflight_sizes, [1024]);
        assert!(edge.allocated_sizes.is_empty());
        assert_eq!(edge.finish_count, 0);

        edge.preflight_error = None;
        assert_eq!(
            manager.create_thread(GuestIsa::M68k, 1, 1024, 2, &mut edge),
            Err(-617)
        );
        assert!(edge.allocated_sizes.is_empty());
        assert_eq!(edge.finish_count, 0);

        edge.allocation = Err(-108);
        assert_eq!(
            manager.create_thread(GuestIsa::M68k, 1, 1024, 4, &mut edge),
            Err(-108)
        );
        assert_eq!(edge.allocated_sizes, [1024]);
        assert!(edge.published_storage.is_empty());
        assert!(edge.released.is_empty());
        assert_eq!(edge.finish_count, 0);

        edge.allocation = Ok(storage(0x1000, 1024));
        let task = manager
            .create_thread(GuestIsa::M68k, 1, 1024, 4, &mut edge)
            .unwrap();
        assert_eq!(task.thread_id(), 3);
    }

    #[test]
    fn new_thread_creation_uses_and_recycles_pool_without_fresh_allocation() {
        let execution = SharedGuestCallStack::default();
        let manager = ThreadManager::new(&execution);
        let pooled = storage(0x2000, 1024);
        execution.publish_thread_pool(GuestIsa::M68k, vec![pooled]);
        let mut edge = RecordingNewThreadEdge::accepting(storage(0x8000, 1024));
        edge.publication = PublicationResult::Refuse;

        assert_eq!(
            manager.create_thread(GuestIsa::M68k, 1, 1024, 3, &mut edge),
            Err(-108)
        );
        assert!(edge.allocated_sizes.is_empty());
        assert!(edge.released.is_empty());
        assert_eq!(edge.finish_count, 1);
        assert_eq!(manager.free_count(GuestIsa::M68k, 1, 1024), Ok(1));

        edge.publication = PublicationResult::Accept;
        let task = manager
            .create_thread(GuestIsa::M68k, 1, 1024, 3, &mut edge)
            .unwrap();
        assert_eq!(task.thread_id(), 3);
        assert_eq!(edge.published_storage[0], pooled);
        assert_eq!(
            edge.published_storage[1],
            ThreadStorage {
                result_destination: 0,
                ..pooled
            }
        );
        assert!(edge.allocated_sizes.is_empty());
        assert_eq!(
            execution.scheduling_state(task),
            Some(ExecutionTaskState::Stopped)
        );
    }

    #[test]
    fn new_thread_creation_releases_fresh_storage_on_every_publication_failure() {
        for publication in [PublicationResult::Error(-50), PublicationResult::Refuse] {
            let execution = SharedGuestCallStack::default();
            let manager = ThreadManager::new(&execution);
            let fresh = storage(0x3000, 1024);
            let mut edge = RecordingNewThreadEdge::accepting(fresh);
            edge.publication = publication;

            let expected = match publication {
                PublicationResult::Error(error) => error,
                PublicationResult::Refuse => -108,
                PublicationResult::Accept => unreachable!(),
            };
            assert_eq!(
                manager.create_thread(GuestIsa::M68k, 1, 1024, 4, &mut edge),
                Err(expected)
            );
            assert_eq!(edge.allocated_sizes, [1024]);
            assert_eq!(edge.released, [fresh]);
            assert_eq!(edge.finish_count, 1);

            edge.publication = PublicationResult::Accept;
            let task = manager
                .create_thread(GuestIsa::M68k, 1, 1024, 4, &mut edge)
                .unwrap();
            assert_eq!(task.thread_id(), 3);
            assert_eq!(
                execution.scheduling_state(task),
                Some(ExecutionTaskState::Ready)
            );
        }
    }

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
