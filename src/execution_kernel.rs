//! CPU-free execution continuation state.
//!
//! This module owns the semantic identity and lifecycle of a guest call. It
//! deliberately knows nothing about either CPU's registers, parked context,
//! import actions, or ABI. Those details remain at the compatibility edges
//! until the execution runner can ask this store to schedule an adapter.
//!
//! A continuation is owned by an execution task and is consumed in LIFO order
//! within that task. Every mutating transition validates the whole request
//! before changing the store, so a stale task or call ID cannot accidentally
//! consume a different continuation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Stable execution-task identity used by the continuation owner.
///
/// Thread Manager IDs are already stable and process-local, so cooperative
/// tasks use their guest-visible ID directly. The application task is ID 2,
/// matching `kApplicationThreadID` from Threads.h. Inside Macintosh:
/// Processes (1994), pp. 4-4--4-6.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionTaskId(u32);

impl ExecutionTaskId {
    pub(crate) const APPLICATION: Self = Self(2);

    pub(crate) const fn from_thread_id(thread_id: u32) -> Self {
        Self(thread_id)
    }
}

/// A request type that identifies the task which owns its continuation.
///
/// The kernel does not know the request's ABI or payload. Each semantic edge
/// supplies this one task identity projection so `submit` can reject stale
/// task metadata instead of rewriting it.
pub(crate) trait TaskOwned {
    fn task(&self) -> ExecutionTaskId;
}

/// Opaque, monotonically allocated identity for one submitted continuation.
///
/// IDs are process-local and are never reused by a live store. Keeping the
/// value opaque prevents an adapter from manufacturing a continuation token
/// that belongs to another call or task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CallId(u64);

/// Lifecycle phase for one continuation.
///
/// `Pending` is the submitted-but-not-yet-started state, `Active` marks the
/// adapter slice currently executing the call, and `Completed` retains the
/// result until the owner retires the frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContinuationPhase {
    Pending,
    Active,
    Completed,
}

/// A CPU-free snapshot of a continuation and its lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContinuationState<R: Copy, C: Copy> {
    call_id: CallId,
    task: ExecutionTaskId,
    request: R,
    continuation: C,
    phase: ContinuationPhase,
    result: Option<u32>,
}

impl<R: Copy, C: Copy> ContinuationState<R, C> {
    pub(crate) const fn call_id(self) -> CallId {
        self.call_id
    }

    #[cfg(test)]
    pub(crate) const fn request(self) -> R {
        self.request
    }

    pub(crate) const fn phase(self) -> ContinuationPhase {
        self.phase
    }

    #[cfg(test)]
    pub(crate) const fn result(self) -> Option<u32> {
        self.result
    }
}

/// Why a transactional continuation operation was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContinuationError {
    /// A request's embedded owner disagreed with the task supplied to submit.
    TaskMismatch {
        expected: ExecutionTaskId,
        actual: ExecutionTaskId,
    },
    /// A task other than the selected execution task attempted to advance a
    /// continuation. Task switches must be explicit before execution can be
    /// resumed.
    TaskNotCurrent {
        current: ExecutionTaskId,
        requested: ExecutionTaskId,
    },
    /// The requested call is not the top continuation for its owner task.
    CallIdMismatch {
        task: ExecutionTaskId,
        expected: Option<CallId>,
        actual: CallId,
    },
    /// The call exists, but its lifecycle does not permit the requested
    /// transition.
    InvalidPhase {
        call_id: CallId,
        actual: ContinuationPhase,
        expected: ContinuationPhase,
    },
    /// A task with suspended continuations cannot be retired.
    RetirementRefused {
        task: ExecutionTaskId,
        depth: usize,
        current: bool,
    },
    /// The monotonic ID namespace is exhausted. This is practically
    /// unreachable, but retaining it makes submission transactional even at
    /// the numeric boundary.
    CallIdExhausted,
}

/// A task-indexed, CPU-free continuation store for one Macintosh process.
///
/// `Clone` snapshots the state into an independent store. `shared_handle`
/// explicitly opts into sharing the live state, matching the compatibility
/// stack's attach/share distinction while making ownership visible to callers.
#[derive(Debug)]
pub(crate) struct ContinuationStore<R: Copy, C: Copy>(Rc<RefCell<StoreState<R, C>>>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoreState<R: Copy, C: Copy> {
    current_task: ExecutionTaskId,
    next_call_id: u64,
    stacks: HashMap<ExecutionTaskId, Vec<ContinuationState<R, C>>>,
}

impl<R: Copy, C: Copy> Default for StoreState<R, C> {
    fn default() -> Self {
        Self {
            current_task: ExecutionTaskId::APPLICATION,
            next_call_id: 1,
            stacks: HashMap::from([(ExecutionTaskId::APPLICATION, Vec::new())]),
        }
    }
}

impl<R: Copy, C: Copy> Default for ContinuationStore<R, C> {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(StoreState::default())))
    }
}

impl<R: Copy, C: Copy> Clone for ContinuationStore<R, C> {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }
}

impl<R: Copy + Eq, C: Copy + Eq> PartialEq for ContinuationStore<R, C> {
    fn eq(&self, other: &Self) -> bool {
        *self.0.borrow() == *other.0.borrow()
    }
}

impl<R: Copy + Eq, C: Copy + Eq> Eq for ContinuationStore<R, C> {}

impl<R: Copy + TaskOwned, C: Copy> ContinuationStore<R, C> {
    /// Return a handle sharing this store's live state.
    #[cfg(test)]
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    /// Current execution task selected for continuation transitions.
    pub(crate) fn current_task(&self) -> ExecutionTaskId {
        self.0.borrow().current_task
    }

    /// Select the task subsequent activation, completion, and retirement may
    /// consume. Selecting a task creates an empty stack if necessary.
    pub(crate) fn switch_to_task(&self, task: ExecutionTaskId) {
        let mut state = self.0.borrow_mut();
        state.stacks.entry(task).or_default();
        state.current_task = task;
    }

    /// Submit a continuation to `task` and allocate its explicit call ID.
    ///
    /// The request already carries an owner task. A disagreement is rejected
    /// instead of being rewritten to the currently selected task, which keeps
    /// stale ABI data from silently crossing a cooperative task boundary.
    pub(crate) fn submit(
        &self,
        task: ExecutionTaskId,
        request: R,
        continuation: C,
    ) -> Result<CallId, ContinuationError> {
        let mut state = self.0.borrow_mut();
        if request.task() != task {
            return Err(ContinuationError::TaskMismatch {
                expected: request.task(),
                actual: task,
            });
        }
        let Some(next_call_id) = state.next_call_id.checked_add(1) else {
            return Err(ContinuationError::CallIdExhausted);
        };
        let call_id = CallId(state.next_call_id);
        state.next_call_id = next_call_id;
        state
            .stacks
            .entry(task)
            .or_default()
            .push(ContinuationState {
                call_id,
                task,
                request,
                continuation,
                phase: ContinuationPhase::Pending,
                result: None,
            });
        Ok(call_id)
    }

    /// Mark the top continuation active after validating owner task, call ID,
    /// and phase. A failed validation leaves every field unchanged.
    pub(crate) fn activate(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<ContinuationState<R, C>, ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_transition(&state, task, call_id, ContinuationPhase::Pending)?;
        let frame = state
            .stacks
            .get_mut(&task)
            .and_then(|stack| stack.last_mut())
            .expect("validated continuation must remain present");
        frame.phase = ContinuationPhase::Active;
        Ok(*frame)
    }

    /// Complete the active top continuation with an optional neutral result.
    /// The result is retained until [`Self::retire`] consumes the frame.
    pub(crate) fn complete(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
        result: Option<u32>,
    ) -> Result<ContinuationState<R, C>, ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_transition(&state, task, call_id, ContinuationPhase::Active)?;
        let frame = state
            .stacks
            .get_mut(&task)
            .and_then(|stack| stack.last_mut())
            .expect("validated continuation must remain present");
        frame.phase = ContinuationPhase::Completed;
        frame.result = result;
        Ok(*frame)
    }

    /// Retire a completed top continuation after validating its exact ID.
    ///
    /// Retirement is separate from completion so a scheduler can observe the
    /// result before the frame leaves the task's LIFO stack.
    pub(crate) fn retire(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<ContinuationState<R, C>, ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_transition(&state, task, call_id, ContinuationPhase::Completed)?;
        Ok(state
            .stacks
            .get_mut(&task)
            .and_then(Vec::pop)
            .expect("validated continuation must remain present"))
    }

    /// Withdraw a still-pending continuation while an adapter setup operation
    /// rolls back. This is intentionally narrower than [`Self::retire`]: an
    /// active or completed call must remain visible until its normal return.
    pub(crate) fn cancel_pending(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<ContinuationState<R, C>, ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_transition(&state, task, call_id, ContinuationPhase::Pending)?;
        Ok(state
            .stacks
            .get_mut(&task)
            .and_then(Vec::pop)
            .expect("validated continuation must remain present"))
    }

    /// Retire an execution task only after its continuation stack is empty.
    ///
    /// A selected task is also refused, even when empty, because removing its
    /// stack would leave the task cursor dangling. Switch to a surviving task
    /// first, then call this method.
    pub(crate) fn retire_task(&self, task: ExecutionTaskId) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        let depth = state.stacks.get(&task).map_or(0, Vec::len);
        let current = state.current_task == task;
        if current || depth != 0 {
            return Err(ContinuationError::RetirementRefused {
                task,
                depth,
                current,
            });
        }
        state.stacks.remove(&task);
        Ok(())
    }

    /// Return the current top continuation for `task`, without changing its
    /// phase or stack.
    pub(crate) fn peek(&self, task: ExecutionTaskId) -> Option<ContinuationState<R, C>> {
        self.0
            .borrow()
            .stacks
            .get(&task)
            .and_then(|stack| stack.last().copied())
    }

    pub(crate) fn task_depth(&self, task: ExecutionTaskId) -> usize {
        self.0.borrow().stacks.get(&task).map_or(0, Vec::len)
    }

    pub(crate) fn depth(&self) -> usize {
        self.task_depth(self.current_task())
    }

    pub(crate) fn len(&self) -> usize {
        self.0.borrow().stacks.values().map(Vec::len).sum()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn task_is_empty(&self, task: ExecutionTaskId) -> bool {
        self.task_depth(task) == 0
    }

    /// Snapshot one task's stack in bottom-to-top order. The semantic layer
    /// exposes values rather than internal borrows so an ABI adapter can
    /// inspect nesting without holding the store across a transition.
    pub(crate) fn task_states(&self, task: ExecutionTaskId) -> Vec<ContinuationState<R, C>> {
        self.0
            .borrow()
            .stacks
            .get(&task)
            .map_or_else(Vec::new, |stack| stack.clone())
    }

    fn validate_transition(
        state: &StoreState<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        expected_phase: ContinuationPhase,
    ) -> Result<(), ContinuationError> {
        if let Some(owner) = Self::owner_of(state, call_id) {
            if owner != task {
                return Err(ContinuationError::TaskMismatch {
                    expected: owner,
                    actual: task,
                });
            }
        }
        if state.current_task != task {
            return Err(ContinuationError::TaskNotCurrent {
                current: state.current_task,
                requested: task,
            });
        }
        let stack = state.stacks.get(&task);
        let top = stack.and_then(|stack| stack.last());
        if top.map(|frame| frame.call_id()) != Some(call_id) {
            return Err(ContinuationError::CallIdMismatch {
                task,
                expected: top.map(|frame| frame.call_id()),
                actual: call_id,
            });
        }
        let phase = top
            .expect("the top continuation was validated with the call ID")
            .phase;
        if phase != expected_phase {
            return Err(ContinuationError::InvalidPhase {
                call_id,
                actual: phase,
                expected: expected_phase,
            });
        }
        Ok(())
    }

    fn owner_of(state: &StoreState<R, C>, call_id: CallId) -> Option<ExecutionTaskId> {
        state.stacks.iter().find_map(|(task, stack)| {
            stack
                .iter()
                .any(|frame| frame.call_id == call_id)
                .then_some(*task)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Request {
        task: ExecutionTaskId,
        entry: u32,
    }

    impl TaskOwned for Request {
        fn task(&self) -> ExecutionTaskId {
            self.task
        }
    }

    type Store = ContinuationStore<Request, u32>;

    fn request(task: ExecutionTaskId, entry: u32) -> Request {
        Request { task, entry }
    }

    fn submit(store: &Store, task: ExecutionTaskId, entry: u32) -> CallId {
        store.submit(task, request(task, entry), entry + 1).unwrap()
    }

    #[test]
    fn same_task_continuations_are_lifo_and_phase_ordered() {
        let store = Store::default();
        let task = ExecutionTaskId::APPLICATION;
        let outer = submit(&store, task, 0x1000);
        let inner = submit(&store, task, 0x3000);

        assert_eq!(store.peek(task).unwrap().call_id(), inner);
        assert!(matches!(
            store.activate(task, outer),
            Err(ContinuationError::CallIdMismatch {
                expected: Some(actual),
                actual: requested,
                ..
            }) if actual == inner && requested == outer
        ));
        assert_eq!(
            store.peek(task).unwrap().phase(),
            ContinuationPhase::Pending
        );

        assert_eq!(
            store.activate(task, inner).unwrap().phase(),
            ContinuationPhase::Active
        );
        assert_eq!(
            store.complete(task, inner, Some(0x55)).unwrap(),
            ContinuationState {
                call_id: inner,
                task,
                request: request(task, 0x3000),
                continuation: 0x3001,
                phase: ContinuationPhase::Completed,
                result: Some(0x55),
            }
        );
        assert_eq!(store.retire(task, inner).unwrap().call_id(), inner);
        assert_eq!(store.peek(task).unwrap().call_id(), outer);
    }

    #[test]
    fn task_stacks_are_independent_across_explicit_switches() {
        let store = Store::default();
        let application = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(7);
        let app_call = submit(&store, application, 0x1000);
        let worker_call = submit(&store, worker, 0x3000);

        store.switch_to_task(worker);
        assert_eq!(store.depth(), 1);
        store.activate(worker, worker_call).unwrap();
        store.complete(worker, worker_call, None).unwrap();
        store.retire(worker, worker_call).unwrap();

        store.switch_to_task(application);
        assert_eq!(store.depth(), 1);
        assert_eq!(store.peek(application).unwrap().call_id(), app_call);
        store.activate(application, app_call).unwrap();
        store.complete(application, app_call, Some(1)).unwrap();
        store.retire(application, app_call).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn task_mismatch_is_rejected_without_rewriting_the_request() {
        let store = Store::default();
        let application = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(9);

        assert_eq!(
            store.submit(application, request(worker, 0x1000), 0x1001),
            Err(ContinuationError::TaskMismatch {
                expected: worker,
                actual: application,
            })
        );
        assert!(store.is_empty());

        let call_id = submit(&store, application, 0x3000);
        let before = store.clone();
        assert!(matches!(
            store.activate(worker, call_id),
            Err(ContinuationError::TaskMismatch {
                expected,
                actual,
            }) if expected == application && actual == worker
        ));
        assert_eq!(store, before);
        assert_eq!(store.peek(application).unwrap().request().task, application);
    }

    #[test]
    fn call_id_mismatch_is_transactional() {
        let store = Store::default();
        let task = ExecutionTaskId::APPLICATION;
        let first = submit(&store, task, 0x1000);
        let second = submit(&store, task, 0x3000);
        let before = store.clone();

        assert!(matches!(
            store.complete(task, first, Some(9)),
            Err(ContinuationError::CallIdMismatch {
                expected: Some(expected),
                actual,
                ..
            }) if expected == second && actual == first
        ));
        assert_eq!(store, before);
        assert_eq!(
            store.peek(task).unwrap().phase(),
            ContinuationPhase::Pending
        );
        assert_eq!(store.peek(task).unwrap().result(), None);
    }

    #[test]
    fn retirement_refuses_current_or_nonempty_tasks() {
        let store = Store::default();
        let application = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(11);
        let call_id = submit(&store, application, 0x1000);
        let before = store.clone();
        assert!(matches!(
            store.retire_task(application),
            Err(ContinuationError::RetirementRefused {
                task,
                depth: 1,
                current: true,
            }) if task == application
        ));
        assert_eq!(store, before);

        store.switch_to_task(worker);
        let before = store.clone();
        assert!(matches!(
            store.retire_task(application),
            Err(ContinuationError::RetirementRefused {
                task,
                depth: 1,
                current: false,
            }) if task == application
        ));
        assert_eq!(store, before);

        store.switch_to_task(application);
        store.activate(application, call_id).unwrap();
        store.complete(application, call_id, None).unwrap();
        store.retire(application, call_id).unwrap();
        store.switch_to_task(worker);
        assert_eq!(store.retire_task(application), Ok(()));
        assert!(store.task_is_empty(application));
    }

    #[test]
    fn clone_is_detached_while_shared_handle_observes_live_state() {
        let store = Store::default();
        let detached = store.clone();
        let shared = store.shared_handle();
        let task = ExecutionTaskId::APPLICATION;
        let call_id = submit(&store, task, 0x1000);

        assert!(detached.is_empty());
        assert_eq!(shared.peek(task).unwrap().call_id(), call_id);
        shared.activate(task, call_id).unwrap();
        assert_eq!(store.peek(task).unwrap().phase(), ContinuationPhase::Active);
    }
}
