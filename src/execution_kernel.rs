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
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use crate::guest_procedure::GuestIsa;

/// Stable execution-task identity used by the continuation owner.
///
/// Thread Manager IDs are already stable and process-local, so cooperative
/// tasks use their guest-visible ID directly. The application task is ID 2,
/// matching `kApplicationThreadID` from Threads.h. Inside Macintosh:
/// Thread Manager (1999), pp. 47--48.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionTaskId(u32);

impl ExecutionTaskId {
    pub(crate) const APPLICATION: Self = Self(2);

    pub(crate) const fn from_thread_id(thread_id: u32) -> Self {
        Self(thread_id)
    }

    pub(crate) const fn thread_id(self) -> u32 {
        self.0
    }
}

/// Scheduling state, independent of either engine's saved registers.
/// Inside Macintosh: Thread Manager (1999), pp. 45, 67–70.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionTaskState {
    Ready,
    Stopped,
    Running,
}

/// Available native coordinator contexts, observed without borrowing a CPU.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NativeAvailability {
    pub(crate) application: bool,
    pub(crate) companion: bool,
    pub(crate) staged_companion: bool,
}

/// Which execution coordinator can advance the selected task. A native
/// session can itself advance a classic callback before resuming native code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionRoute {
    Classic,
    NativeApplication,
    NativeCompanion,
    PrepareCompanion,
    Blocked,
}

/// A request type that identifies the task which owns its continuation.
///
/// The kernel does not know the request's ABI or payload. Each semantic edge
/// supplies this one task identity projection so `submit` can reject stale
/// task metadata instead of rewriting it.
pub(crate) trait TaskOwned {
    fn task(&self) -> ExecutionTaskId;
}

/// Architecture-neutral destination of one guest procedure invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GuestCallTarget {
    pub(crate) isa: GuestIsa,
    pub(crate) entry: u32,
    pub(crate) rtoc: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M68kRegisterState {
    pub(crate) data: [u32; 8],
    pub(crate) address: [u32; 7],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M68kResultSource {
    Data(u8),
    Address(u8),
    Memory {
        address: u32,
        size: u8,
    },
    SpecialCase {
        selector: u8,
        arguments: PowerPcArguments,
        stack_result: Option<u32>,
    },
}

pub(crate) const MAX_POWERPC_GUEST_ARGUMENTS: usize = 13;

/// Bounded, copyable native argument list carried by a semantic call request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerPcArguments {
    values: [u32; MAX_POWERPC_GUEST_ARGUMENTS],
    len: u8,
}

impl PowerPcArguments {
    pub(crate) fn from_slice(values: &[u32]) -> Option<Self> {
        if values.len() > MAX_POWERPC_GUEST_ARGUMENTS {
            return None;
        }
        let mut arguments = Self {
            values: [0; MAX_POWERPC_GUEST_ARGUMENTS],
            len: u8::try_from(values.len()).ok()?,
        };
        arguments.values[..values.len()].copy_from_slice(values);
        Some(arguments)
    }

    pub(crate) fn as_slice(&self) -> &[u32] {
        &self.values[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M68kResultTarget {
    Data { index: u8, size: u8 },
    Address { index: u8, size: u8 },
    Ccr { mask: u8 },
    Memory { address: u32, size: u8 },
    SpecialCase { selector: u8, scratch: u32 },
}

/// Architecture-neutral policy for placing a guest callback's result in the
/// caller's result slot.
///
/// The architecture adapter translates this policy to its ABI vocabulary.
/// Inside Macintosh: PowerPC System Software (1994), pp. 2-12--2-16.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestCallReturnPolicy {
    Preserve,
    Mask(u32),
    Set(u32),
    ZeroOrSet { zero: u32, nonzero: u32 },
    CrBit(u8),
    XerCa,
    XerOv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerPcReturnState {
    pub(crate) gpr3: u32,
}

/// The architecture-neutral payload supplied to a guest call.
///
/// A call into native PowerPC code receives its values through the PPC ABI
/// registers/parameter area, while a call into 68K code needs the emulated
/// stack/register interval. Both are represented here so the process-owned
/// continuation stack does not have to infer an ABI from a CPU action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M68kCallRequest {
    pub(crate) entry: u32,
    pub(crate) initial_sp: u32,
    pub(crate) final_sp: u32,
    pub(crate) registers: M68kRegisterState,
    pub(crate) result: Option<M68kResultSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestCallArguments {
    None,
    PowerPc(PowerPcArguments),
    M68k(M68kCallRequest),
}

/// One architecture-neutral guest procedure request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GuestCallRequest {
    pub(crate) task: ExecutionTaskId,
    pub(crate) target: GuestCallTarget,
    pub(crate) arguments: GuestCallArguments,
}

impl GuestCallRequest {
    pub(crate) fn new(target: GuestCallTarget) -> Self {
        Self::for_task(ExecutionTaskId::APPLICATION, target)
    }

    pub(crate) fn for_task(task: ExecutionTaskId, target: GuestCallTarget) -> Self {
        Self {
            task,
            target,
            arguments: GuestCallArguments::None,
        }
    }

    pub(crate) fn with_powerpc_arguments(mut self, arguments: PowerPcArguments) -> Self {
        self.arguments = GuestCallArguments::PowerPc(arguments);
        self
    }

    pub(crate) fn with_m68k_request(mut self, request: M68kCallRequest) -> Self {
        self.arguments = GuestCallArguments::M68k(request);
        self
    }
}

impl TaskOwned for GuestCallRequest {
    fn task(&self) -> ExecutionTaskId {
        self.task
    }
}

/// Typed resumption metadata for one guest call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestCallContinuation {
    ReturnToM68k {
        return_pc: u32,
        final_sp: u32,
        result: Option<M68kResultTarget>,
    },
    ReturnToPowerPc {
        return_pc: u32,
        final_pc: u32,
        restore_rtoc: u32,
        return_gpr3: GuestCallReturnPolicy,
    },
}

impl GuestCallContinuation {
    pub(crate) const fn to_m68k(
        return_pc: u32,
        final_sp: u32,
        result: Option<M68kResultTarget>,
    ) -> Self {
        Self::ReturnToM68k {
            return_pc,
            final_sp,
            result,
        }
    }

    pub(crate) fn to_powerpc(
        return_pc: u32,
        final_pc: u32,
        restore_rtoc: u32,
        return_gpr3: impl Into<GuestCallReturnPolicy>,
    ) -> Self {
        Self::ReturnToPowerPc {
            return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3: return_gpr3.into(),
        }
    }
}

/// Semantic guest-execution effect emitted by either ABI edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestCallEffect {
    CallGuest {
        request: GuestCallRequest,
        continuation: GuestCallContinuation,
    },
}

impl GuestCallEffect {
    pub(crate) const fn call_guest(
        request: GuestCallRequest,
        continuation: GuestCallContinuation,
    ) -> Self {
        Self::CallGuest {
            request,
            continuation,
        }
    }

    pub(crate) fn request(self) -> GuestCallRequest {
        match self {
            Self::CallGuest { request, .. } => request,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingPowerPcExecution {
    pub(crate) target: GuestCallTarget,
    pub(crate) arguments: PowerPcArguments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingM68kExecution {
    pub(crate) entry: u32,
    pub(crate) initial_sp: u32,
    pub(crate) return_pc: u32,
    pub(crate) final_sp: u32,
    pub(crate) registers: M68kRegisterState,
    pub(crate) result: Option<M68kResultSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M68kResume {
    pub(crate) return_pc: u32,
    pub(crate) final_sp: u32,
    pub(crate) result: Option<M68kResultTarget>,
    pub(crate) powerpc: PowerPcReturnState,
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
    TaskIdsExhausted,
    TaskUnavailable {
        task: ExecutionTaskId,
    },
    /// The adapter rejected its prepared return without changing live state.
    CommitRefused {
        call_id: CallId,
    },
    TaskCommitRefused {
        task: ExecutionTaskId,
    },
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
    /// A concrete adapter context is still parked against the call. The
    /// context must be restored before the semantic continuation can leave
    /// the kernel.
    ContextAttached {
        call_id: CallId,
        count: usize,
    },
    /// A task with suspended continuations cannot be retired.
    RetirementRefused {
        task: ExecutionTaskId,
        depth: usize,
        contexts: usize,
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
    next_task_id: Option<u32>,
    stacks: HashMap<ExecutionTaskId, Vec<ContinuationState<R, C>>>,
    attached_contexts: HashMap<CallId, usize>,
    retired_tasks: HashSet<ExecutionTaskId>,
    task_states: HashMap<ExecutionTaskId, ExecutionTaskState>,
    task_entry_isas: HashMap<ExecutionTaskId, GuestIsa>,
    ready: VecDeque<ExecutionTaskId>,
    critical_depth: u32,
}

impl<R: Copy, C: Copy> Default for StoreState<R, C> {
    fn default() -> Self {
        Self {
            current_task: ExecutionTaskId::APPLICATION,
            next_call_id: 1,
            next_task_id: Some(3),
            stacks: HashMap::from([(ExecutionTaskId::APPLICATION, Vec::new())]),
            attached_contexts: HashMap::new(),
            retired_tasks: HashSet::new(),
            task_states: HashMap::from([(
                ExecutionTaskId::APPLICATION,
                ExecutionTaskState::Running,
            )]),
            task_entry_isas: HashMap::new(),
            ready: VecDeque::new(),
            critical_depth: 0,
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
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    /// Current execution task selected for continuation transitions.
    pub(crate) fn current_task(&self) -> ExecutionTaskId {
        self.0.borrow().current_task
    }

    /// Construction-time adoption is safe only before this execution owner
    /// has acquired any task, call identity, or scheduling history.
    pub(crate) fn is_pristine(&self) -> bool {
        let state = self.0.borrow();
        state.current_task == ExecutionTaskId::APPLICATION
            && state.next_call_id == 1
            && state.next_task_id == Some(3)
            && state.stacks.len() == 1
            && state
                .stacks
                .get(&ExecutionTaskId::APPLICATION)
                .is_some_and(Vec::is_empty)
            && state.attached_contexts.is_empty()
            && state.retired_tasks.is_empty()
            && state.task_entry_isas.is_empty()
            && state.task_states.get(&ExecutionTaskId::APPLICATION)
                == Some(&ExecutionTaskState::Running)
            && state.ready.is_empty()
            && state.critical_depth == 0
    }

    /// Allocate from the same monotonic namespace used by explicit registration.
    /// Exhaustion never wraps into aliases, the application task or retired IDs.
    pub(crate) fn create_task(&self) -> Result<ExecutionTaskId, ContinuationError> {
        self.create_task_with(|_| true)
    }

    /// The synchronous commit must not reenter this owner or change state on
    /// failure. Publish guest output only after preparation, before registration.
    pub(crate) fn create_task_with(
        &self,
        commit: impl FnOnce(ExecutionTaskId) -> bool,
    ) -> Result<ExecutionTaskId, ContinuationError> {
        let id = self
            .0
            .borrow()
            .next_task_id
            .ok_or(ContinuationError::TaskIdsExhausted)?;
        let task = ExecutionTaskId::from_thread_id(id);
        if !commit(task) {
            return Err(ContinuationError::TaskCommitRefused { task });
        }
        self.register_task(task)?;
        Ok(task)
    }

    /// Register a new task exactly once for this process lifetime.
    pub(crate) fn register_task(&self, task: ExecutionTaskId) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        if state.stacks.contains_key(&task) || state.retired_tasks.contains(&task) {
            return Err(ContinuationError::TaskUnavailable { task });
        }
        state.stacks.insert(task, Vec::new());
        state.task_states.insert(task, ExecutionTaskState::Stopped);
        state.task_entry_isas.insert(task, GuestIsa::M68k);
        if state
            .next_task_id
            .is_some_and(|next| task.thread_id() >= next)
        {
            state.next_task_id = task.thread_id().checked_add(1);
        }
        Ok(())
    }

    /// Select an existing task without manufacturing or resurrecting it.
    pub(crate) fn switch_to_task(&self, task: ExecutionTaskId) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        if !state.stacks.contains_key(&task) {
            return Err(ContinuationError::TaskUnavailable { task });
        }
        if state.current_task != task {
            if state.task_states.get(&task) != Some(&ExecutionTaskState::Ready) {
                return Err(ContinuationError::TaskUnavailable { task });
            }
            if state.critical_depth != 0 {
                return Err(ContinuationError::TaskUnavailable { task });
            }
            let previous = state.current_task;
            if state.task_states.get(&previous) == Some(&ExecutionTaskState::Running) {
                state
                    .task_states
                    .insert(previous, ExecutionTaskState::Ready);
                if !state.ready.contains(&previous) {
                    state.ready.push_back(previous);
                }
            }
        }
        state.ready.retain(|queued| *queued != task);
        state.task_states.insert(task, ExecutionTaskState::Running);
        state.current_task = task;
        Ok(())
    }

    /// Initial application binding may accompany adopted pending calls. Once
    /// bound, an entry architecture changes only when that task has no calls.
    pub(crate) fn bind_task_entry_isa(&self, task: ExecutionTaskId, isa: GuestIsa) -> bool {
        let mut state = self.0.borrow_mut();
        let Some(stack) = state.stacks.get(&task) else {
            return false;
        };
        if state
            .task_entry_isas
            .get(&task)
            .is_some_and(|old| *old != isa)
            && !stack.is_empty()
        {
            return false;
        }
        state.task_entry_isas.insert(task, isa);
        true
    }

    pub(crate) fn task_entry_isa(&self, task: ExecutionTaskId) -> Option<GuestIsa> {
        let state = self.0.borrow();
        state.stacks.contains_key(&task).then(|| {
            state
                .task_entry_isas
                .get(&task)
                .copied()
                .unwrap_or(GuestIsa::M68k)
        })
    }

    pub(crate) fn has_live_workers(&self) -> bool {
        self.0
            .borrow()
            .task_states
            .keys()
            .any(|task| *task != ExecutionTaskId::APPLICATION)
    }

    pub(crate) fn scheduling_state(&self, task: ExecutionTaskId) -> Option<ExecutionTaskState> {
        self.0.borrow().task_states.get(&task).copied()
    }

    pub(crate) fn set_scheduling_state(
        &self,
        task: ExecutionTaskId,
        requested: ExecutionTaskState,
    ) -> bool {
        self.change_scheduling_state(task, requested, false)
    }

    /// SetThreadStateEndCritical is one state transition, including failures.
    /// Inside Macintosh: Thread Manager (1999), pp. 71--72.
    pub(crate) fn set_state_ending_critical(
        &self,
        task: ExecutionTaskId,
        requested: ExecutionTaskState,
    ) -> bool {
        self.change_scheduling_state(task, requested, true)
    }

    fn change_scheduling_state(
        &self,
        task: ExecutionTaskId,
        requested: ExecutionTaskState,
        end_critical: bool,
    ) -> bool {
        let mut state = self.0.borrow_mut();
        let depth = if end_critical {
            let Some(depth) = state.critical_depth.checked_sub(1) else {
                return false;
            };
            depth
        } else {
            state.critical_depth
        };
        if !state.stacks.contains_key(&task)
            || (requested == ExecutionTaskState::Running && task != state.current_task)
            || (task == state.current_task
                && requested != ExecutionTaskState::Running
                && depth != 0)
        {
            return false;
        }
        state.critical_depth = depth;
        state.ready.retain(|queued| *queued != task);
        state.task_states.insert(task, requested);
        if requested == ExecutionTaskState::Ready {
            state.ready.push_back(task);
        }
        true
    }

    /// Selection is non-destructive: the adapter must first validate that it
    /// can install the successor. Only the committed switch removes it.
    pub(crate) fn next_ready_task(
        &self,
        suggested: Option<ExecutionTaskId>,
    ) -> Option<ExecutionTaskId> {
        let state = self.0.borrow();
        if state.critical_depth != 0 {
            return None;
        }
        let eligible = |task: ExecutionTaskId| {
            task != state.current_task
                && state.task_states.get(&task) == Some(&ExecutionTaskState::Ready)
        };
        suggested
            .filter(|task| eligible(*task))
            .or_else(|| state.ready.iter().copied().find(|task| eligible(*task)))
    }

    pub(crate) fn critical_depth(&self) -> u32 {
        self.0.borrow().critical_depth
    }

    pub(crate) fn begin_critical(&self) {
        let mut state = self.0.borrow_mut();
        state.critical_depth = state.critical_depth.saturating_add(1);
    }

    pub(crate) fn end_critical(&self) -> bool {
        let mut state = self.0.borrow_mut();
        let Some(depth) = state.critical_depth.checked_sub(1) else {
            return false;
        };
        state.critical_depth = depth;
        true
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
        if !state.stacks.contains_key(&task) {
            return Err(ContinuationError::TaskUnavailable { task });
        }
        let Some(next_call_id) = state.next_call_id.checked_add(1) else {
            return Err(ContinuationError::CallIdExhausted);
        };
        let call_id = CallId(state.next_call_id);
        state.next_call_id = next_call_id;
        state
            .stacks
            .get_mut(&task)
            .expect("registered task")
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

    fn activate_attaching_context(
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
        let frame = *frame;
        *state.attached_contexts.entry(call_id).or_default() += 1;
        Ok(frame)
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

    fn complete_detaching_context(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
        result: Option<u32>,
    ) -> Result<ContinuationState<R, C>, ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_transition(&state, task, call_id, ContinuationPhase::Active)?;
        let Some(context_count) = state.attached_contexts.get(&call_id).copied() else {
            return Err(ContinuationError::ContextAttached { call_id, count: 0 });
        };
        let frame = state
            .stacks
            .get_mut(&task)
            .and_then(|stack| stack.last_mut())
            .expect("validated continuation must remain present");
        frame.phase = ContinuationPhase::Completed;
        frame.result = result;
        let frame = *frame;
        if context_count == 1 {
            state.attached_contexts.remove(&call_id);
        } else {
            state.attached_contexts.insert(call_id, context_count - 1);
        }
        Ok(frame)
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
        if let Some(count) = state.attached_contexts.get(&call_id).copied() {
            return Err(ContinuationError::ContextAttached { call_id, count });
        }
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
        if let Some(count) = state.attached_contexts.get(&call_id).copied() {
            return Err(ContinuationError::ContextAttached { call_id, count });
        }
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
    #[cfg(test)]
    pub(crate) fn retire_task(&self, task: ExecutionTaskId) -> Result<(), ContinuationError> {
        self.retire_task_with(task, None, || true)
    }

    /// Validate retirement and optional replacement before committing external
    /// result storage. The callback must be synchronous, non-reentrant and
    /// leave external state unchanged on failure; it cannot execute guest code.
    pub(crate) fn retire_task_with(
        &self,
        task: ExecutionTaskId,
        successor: Option<ExecutionTaskId>,
        commit: impl FnOnce() -> bool,
    ) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        let Some(stack) = state.stacks.get(&task) else {
            return Err(ContinuationError::TaskUnavailable { task });
        };
        let depth = stack.len();
        let contexts = state
            .stacks
            .get(&task)
            .into_iter()
            .flatten()
            .map(|continuation| {
                state
                    .attached_contexts
                    .get(&continuation.call_id)
                    .copied()
                    .unwrap_or(0)
            })
            .sum();
        let current = state.current_task == task;
        if (current && successor.is_none()) || depth != 0 || contexts != 0 {
            return Err(ContinuationError::RetirementRefused {
                task,
                depth,
                contexts,
                current,
            });
        }
        if let Some(next) = successor {
            if !current
                || next == task
                || !state.stacks.contains_key(&next)
                || state.task_states.get(&next) != Some(&ExecutionTaskState::Ready)
                || state.critical_depth != 0
            {
                return Err(ContinuationError::TaskUnavailable { task: next });
            }
        }
        if !commit() {
            return Err(ContinuationError::TaskCommitRefused { task });
        }
        if let Some(next) = successor {
            state.current_task = next;
            state.task_states.insert(next, ExecutionTaskState::Running);
            state.ready.retain(|queued| *queued != next);
        }
        state.stacks.remove(&task);
        state.retired_tasks.insert(task);
        state.task_states.remove(&task);
        state.task_entry_isas.remove(&task);
        state.ready.retain(|queued| *queued != task);
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

    #[cfg(test)]
    pub(crate) fn task_is_empty(&self, task: ExecutionTaskId) -> bool {
        let state = self.0.borrow();
        state.stacks.get(&task).is_none_or(Vec::is_empty)
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

    #[cfg(test)]
    fn attach_context(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_context_owner(&state, task, call_id)?;
        *state.attached_contexts.entry(call_id).or_default() += 1;
        Ok(())
    }

    #[cfg(test)]
    fn detach_context(
        &self,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<(), ContinuationError> {
        let mut state = self.0.borrow_mut();
        Self::validate_context_owner(&state, task, call_id)?;
        let Some(count) = state.attached_contexts.get_mut(&call_id) else {
            return Err(ContinuationError::ContextAttached { call_id, count: 0 });
        };
        *count -= 1;
        if *count == 0 {
            state.attached_contexts.remove(&call_id);
        }
        Ok(())
    }

    fn validate_context_owner(
        state: &StoreState<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<(), ContinuationError> {
        let Some(owner) = Self::owner_of(state, call_id) else {
            return Err(ContinuationError::CallIdMismatch {
                task,
                expected: state
                    .stacks
                    .get(&task)
                    .and_then(|stack| stack.last())
                    .map(|frame| frame.call_id()),
                actual: call_id,
            });
        };
        if owner != task {
            return Err(ContinuationError::TaskMismatch {
                expected: owner,
                actual: task,
            });
        }
        if state.current_task != task {
            return Err(ContinuationError::TaskNotCurrent {
                current: state.current_task,
                requested: task,
            });
        }
        Ok(())
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

/// Concrete adapter state parked against an exact semantic continuation.
///
/// The bank is generic so the execution kernel remains CPU-free. It owns the
/// `(task, call)` association and registers every parked value with the
/// continuation store, while an ISA adapter supplies the concrete context.
/// Mixed Mode switch frames preserve nonvolatile state until the matching
/// return crosses the mode boundary; they are linked in exact LIFO order.
/// Inside Macintosh: PowerPC System Software (1994), pp. 2-9--2-13.
#[derive(Clone, Debug)]
pub(crate) struct ExecutionContextBank<T> {
    by_call: HashMap<(ExecutionTaskId, CallId), T>,
}

/// Process-task snapshots keyed by the same stable identities as
/// continuations and parked ISA contexts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionTaskContextBank<T> {
    by_task: HashMap<ExecutionTaskId, T>,
}

impl<T> Default for ExecutionContextBank<T> {
    fn default() -> Self {
        Self {
            by_call: HashMap::new(),
        }
    }
}

impl<T> Default for ExecutionTaskContextBank<T> {
    fn default() -> Self {
        Self {
            by_task: HashMap::new(),
        }
    }
}

impl<T> ExecutionTaskContextBank<T> {
    pub(crate) fn same_tasks<U>(&self, other: &ExecutionTaskContextBank<U>) -> bool {
        self.by_task.len() == other.by_task.len()
            && self
                .by_task
                .keys()
                .all(|task| other.by_task.contains_key(task))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_task.is_empty()
    }

    pub(crate) fn get(&self, task: ExecutionTaskId) -> Option<&T> {
        self.by_task.get(&task)
    }

    pub(crate) fn insert(&mut self, task: ExecutionTaskId, context: T) -> Option<T> {
        self.by_task.insert(task, context)
    }

    pub(crate) fn remove(&mut self, task: ExecutionTaskId) -> Option<T> {
        self.by_task.remove(&task)
    }
}

impl<T> ExecutionContextBank<T> {
    pub(crate) fn contains(&self, task: ExecutionTaskId, call_id: CallId) -> bool {
        self.by_call.contains_key(&(task, call_id))
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.by_call.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_call.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn task_len(&self, task: ExecutionTaskId) -> usize {
        self.by_call
            .keys()
            .filter(|(owner, _)| *owner == task)
            .count()
    }

    #[cfg(test)]
    pub(crate) fn park<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        context: T,
    ) -> Result<(), (ContinuationError, T)> {
        if self.contains(task, call_id) {
            return Err((
                ContinuationError::ContextAttached { call_id, count: 1 },
                context,
            ));
        }
        if let Err(error) = kernel.attach_context(task, call_id) {
            return Err((error, context));
        }
        let replaced = self.by_call.insert((task, call_id), context);
        debug_assert!(replaced.is_none());
        Ok(())
    }

    pub(crate) fn park_while_activating<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        context: T,
    ) -> Result<ContinuationState<R, C>, (ContinuationError, T)> {
        if self.contains(task, call_id) {
            return Err((
                ContinuationError::ContextAttached { call_id, count: 1 },
                context,
            ));
        }
        let semantic = match kernel.activate_attaching_context(task, call_id) {
            Ok(semantic) => semantic,
            Err(error) => return Err((error, context)),
        };
        let replaced = self.by_call.insert((task, call_id), context);
        debug_assert!(replaced.is_none());
        Ok(semantic)
    }

    #[cfg(test)]
    pub(crate) fn take<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
    ) -> Result<T, ContinuationError> {
        if !self.contains(task, call_id) {
            return Err(ContinuationError::ContextAttached { call_id, count: 0 });
        }
        kernel.detach_context(task, call_id)?;
        Ok(self
            .by_call
            .remove(&(task, call_id))
            .expect("validated context must remain parked"))
    }

    pub(crate) fn take_while_completing<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        result: Option<u32>,
    ) -> Result<(T, ContinuationState<R, C>), ContinuationError> {
        if !self.contains(task, call_id) {
            return Err(ContinuationError::ContextAttached { call_id, count: 0 });
        }
        let semantic = kernel.complete_detaching_context(task, call_id, result)?;
        let context = self
            .by_call
            .remove(&(task, call_id))
            .expect("validated context must remain parked");
        Ok((context, semantic))
    }

    /// Park the enclosing caller and activate its nested call as one change.
    /// All identity/phase checks precede replacement of the installed engine.
    pub(crate) fn activate_parking_caller<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        caller: Option<CallId>,
        installed: &mut T,
    ) -> Result<(), ContinuationError>
    where
        T: Default,
    {
        let mut state = kernel.0.borrow_mut();
        ContinuationStore::validate_transition(&state, task, call_id, ContinuationPhase::Pending)?;
        if let Some(caller) = caller {
            ContinuationStore::validate_context_owner(&state, task, caller)?;
            if caller == call_id {
                return Err(ContinuationError::CallIdMismatch {
                    task,
                    expected: None,
                    actual: caller,
                });
            }
            if self.contains(task, caller) {
                // Sibling callbacks reuse the enclosing caller's saved state.
                *installed = T::default();
            } else {
                self.by_call
                    .insert((task, caller), std::mem::take(installed));
                *state.attached_contexts.entry(caller).or_default() += 1;
            }
        }
        state
            .stacks
            .get_mut(&task)
            .and_then(|stack| stack.last_mut())
            .expect("validated continuation")
            .phase = ContinuationPhase::Active;
        Ok(())
    }

    /// Validate the complete return boundary before allowing adapter writes.
    /// The closure must validate all fallible ABI work before mutating state;
    /// it cannot execute guest code or reenter this store. Once it succeeds,
    /// removal of the exact context and continuation cannot fail.
    pub(crate) fn retire_with_context<R: Copy + TaskOwned, C: Copy>(
        &mut self,
        kernel: &ContinuationStore<R, C>,
        task: ExecutionTaskId,
        call_id: CallId,
        apply: impl FnOnce(Option<&mut T>) -> bool,
    ) -> Result<Option<T>, ContinuationError> {
        let mut state = kernel.0.borrow_mut();
        ContinuationStore::validate_transition(
            &state,
            task,
            call_id,
            ContinuationPhase::Completed,
        )?;
        let expected = usize::from(self.contains(task, call_id));
        let attached = state.attached_contexts.get(&call_id).copied().unwrap_or(0);
        if attached != expected {
            return Err(ContinuationError::ContextAttached {
                call_id,
                count: attached,
            });
        }
        if !apply(self.by_call.get_mut(&(task, call_id))) {
            return Err(ContinuationError::CommitRefused { call_id });
        }
        state.attached_contexts.remove(&call_id);
        state.stacks.get_mut(&task).expect("validated task").pop();
        Ok(self.by_call.remove(&(task, call_id)))
    }

    pub(crate) fn same_slots<U>(&self, other: &ExecutionContextBank<U>) -> bool {
        self.by_call.len() == other.by_call.len()
            && self
                .by_call
                .keys()
                .all(|key| other.by_call.contains_key(key))
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
    fn scheduler_selection_is_stable_until_a_switch_and_stopped_tasks_are_removed() {
        let store = Store::default();
        let app = ExecutionTaskId::APPLICATION;
        let first = ExecutionTaskId::from_thread_id(3);
        let second = ExecutionTaskId::from_thread_id(4);
        for task in [first, second] {
            store.register_task(task).unwrap();
            assert!(store.set_scheduling_state(task, ExecutionTaskState::Ready));
        }
        assert_eq!(store.next_ready_task(None), Some(first));
        let before = store.clone();
        assert_eq!(store.next_ready_task(Some(second)), Some(second));
        assert_eq!(
            store, before,
            "selection must not consume a context the adapter may reject"
        );
        assert!(!store.set_scheduling_state(first, ExecutionTaskState::Running));
        assert_eq!(store, before);
        assert!(store.set_scheduling_state(first, ExecutionTaskState::Stopped));
        assert_eq!(store.next_ready_task(Some(first)), Some(second));
        store.switch_to_task(second).unwrap();
        assert_eq!(
            store.scheduling_state(second),
            Some(ExecutionTaskState::Running)
        );
        assert_eq!(store.scheduling_state(app), Some(ExecutionTaskState::Ready));
        assert_eq!(store.next_ready_task(None), Some(app));
        store.retire_task(first).unwrap();
        assert!(!store.set_scheduling_state(first, ExecutionTaskState::Ready));
        assert_eq!(store.next_ready_task(None), Some(app));
    }

    #[test]
    fn combined_state_and_critical_exit_rejects_without_partial_unmasking() {
        let store = Store::default();
        let app = ExecutionTaskId::APPLICATION;
        store.begin_critical();
        let before = store.clone();
        assert!(!store.set_state_ending_critical(
            ExecutionTaskId::from_thread_id(999),
            ExecutionTaskState::Ready
        ));
        assert_eq!(store, before);
        store.begin_critical();
        let nested = store.clone();
        assert!(!store.set_state_ending_critical(app, ExecutionTaskState::Stopped));
        assert_eq!(store, nested);
        assert!(store.end_critical());
        assert!(store.set_state_ending_critical(app, ExecutionTaskState::Stopped));
        assert_eq!(store.critical_depth(), 0);
        assert_eq!(
            store.scheduling_state(app),
            Some(ExecutionTaskState::Stopped)
        );
    }

    #[test]
    fn nested_critical_sections_keep_task_cursor_and_schedule_unchanged() {
        let store = Store::default();
        let app = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(3);
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        store.begin_critical();
        store.begin_critical();
        let before = store.clone();
        assert_eq!(store.next_ready_task(Some(worker)), None);
        assert!(store.switch_to_task(worker).is_err());
        assert!(!store.set_scheduling_state(app, ExecutionTaskState::Stopped));
        assert_eq!(store, before);
        assert!(store.end_critical());
        assert_eq!(store.next_ready_task(None), None);
        assert!(store.end_critical());
        assert_eq!(store.next_ready_task(None), Some(worker));
        assert!(!store.end_critical());
        assert_eq!(store.critical_depth(), 0);
    }

    #[test]
    fn refused_task_creation_does_not_consume_identity_or_publish_state() {
        let store = Store::default();
        let before = store.clone();
        assert!(store.create_task_with(|_| false).is_err());
        assert_eq!(store, before);
        assert_eq!(store.create_task().unwrap().thread_id(), 3);
    }

    #[test]
    fn task_allocation_shares_registration_and_never_reuses_retired_ids() {
        let store = Store::default();
        let first = store.create_task().unwrap();
        assert_eq!(first.thread_id(), 3);
        let imported = ExecutionTaskId::from_thread_id(40);
        store.register_task(imported).unwrap();
        store.retire_task(imported).unwrap();
        store.retire_task(first).unwrap();
        let next = store.create_task().unwrap();
        assert_eq!(next.thread_id(), 41);
        assert_eq!(
            store.scheduling_state(next),
            Some(ExecutionTaskState::Stopped)
        );
        assert_eq!(store.task_entry_isa(next), Some(GuestIsa::M68k));
        assert!(!store.is_pristine());
    }

    #[test]
    fn exhausted_task_namespace_is_unchanged_and_does_not_wrap() {
        let store = Store::default();
        store
            .register_task(ExecutionTaskId::from_thread_id(u32::MAX - 1))
            .unwrap();
        assert_eq!(store.create_task().unwrap().thread_id(), u32::MAX);
        let before = store.clone();
        assert_eq!(
            store.create_task(),
            Err(ContinuationError::TaskIdsExhausted)
        );
        assert_eq!(store, before);
        store
            .retire_task(ExecutionTaskId::from_thread_id(u32::MAX))
            .unwrap();
        assert_eq!(
            store.create_task(),
            Err(ContinuationError::TaskIdsExhausted)
        );
    }

    #[test]
    fn task_entry_architecture_retires_with_its_task() {
        let store = Store::default();
        let worker = ExecutionTaskId::from_thread_id(9);
        assert_eq!(store.task_entry_isa(worker), None);
        assert!(!store.bind_task_entry_isa(worker, GuestIsa::PowerPc));
        store.register_task(worker).unwrap();
        assert_eq!(store.task_entry_isa(worker), Some(GuestIsa::M68k));
        assert!(store.bind_task_entry_isa(worker, GuestIsa::PowerPc));
        assert_eq!(store.task_entry_isa(worker), Some(GuestIsa::PowerPc));
        store.retire_task(worker).unwrap();
        assert_eq!(store.task_entry_isa(worker), None);
        assert!(!store.bind_task_entry_isa(worker, GuestIsa::M68k));
    }

    #[test]
    fn task_ids_cannot_be_created_by_submission_or_resurrected_after_retirement() {
        let store = Store::default();
        let worker = ExecutionTaskId::from_thread_id(77);
        let snapshot = store.clone();
        assert!(store.switch_to_task(worker).is_err());
        assert!(store.submit(worker, request(worker, 0x1000), 0).is_err());
        assert_eq!(store, snapshot);
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(store.register_task(worker).is_err());
        store.retire_task(worker).unwrap();
        let retired = store.clone();
        assert!(store.register_task(worker).is_err());
        assert!(store.switch_to_task(worker).is_err());
        assert!(store.submit(worker, request(worker, 0x1000), 0).is_err());
        assert!(store.retire_task(worker).is_err());
        assert_eq!(store, retired);
    }

    #[test]
    fn nested_activation_validates_before_replacing_the_installed_caller() {
        let store = Store::default();
        let task = ExecutionTaskId::APPLICATION;
        let outer = submit(&store, task, 0x1000);
        store.activate(task, outer).unwrap();
        let inner = submit(&store, task, 0x2000);
        let mut bank = ExecutionContextBank::default();
        let mut installed = 42_u32;
        let before = store.clone();
        assert!(bank
            .activate_parking_caller(&store, task, inner, Some(inner), &mut installed)
            .is_err());
        assert_eq!(installed, 42);
        assert_eq!(store, before);
        assert!(bank.is_empty());
        bank.activate_parking_caller(&store, task, inner, Some(outer), &mut installed)
            .unwrap();
        assert_eq!(installed, 0);
        assert_eq!(store.peek(task).unwrap().phase(), ContinuationPhase::Active);
        installed = 99;
        assert!(bank
            .activate_parking_caller(&store, task, inner, Some(outer), &mut installed)
            .is_err());
        assert_eq!(installed, 99);
        store.complete(task, inner, None).unwrap();
        store.retire(task, inner).unwrap();
        store.complete(task, outer, None).unwrap();
        assert_eq!(
            bank.retire_with_context(&store, task, outer, |_| true),
            Ok(Some(42))
        );
    }

    #[test]
    fn return_commit_validates_contexts_before_adapter_writes_and_retries() {
        let store = Store::default();
        let task = ExecutionTaskId::APPLICATION;
        let call = submit(&store, task, 0x1000);
        let mut bank = ExecutionContextBank::default();
        bank.park(&store, task, call, 42_u32).unwrap();
        // Pending calls cannot expose the parked caller to result writes.
        assert!(bank
            .retire_with_context(&store, task, call, |_| panic!("not completed"))
            .is_err());
        store.activate(task, call).unwrap();
        store.complete(task, call, Some(7)).unwrap();
        let snapshot = store.clone();
        let mut wrong_bank = ExecutionContextBank::<u32>::default();
        assert!(wrong_bank
            .retire_with_context(&store, task, call, |_| panic!("missing context"))
            .is_err());
        assert_eq!(store, snapshot);
        assert_eq!(
            bank.retire_with_context(&store, task, call, |context| {
                assert_eq!(context.as_deref(), Some(&42));
                false
            }),
            Err(ContinuationError::CommitRefused { call_id: call })
        );
        assert_eq!(store, snapshot);
        assert_eq!(
            bank.retire_with_context(&store, task, call, |context| {
                *context.unwrap() = 7;
                true
            }),
            Ok(Some(7))
        );
        assert!(store.is_empty());
        assert!(bank.is_empty());
        assert!(bank
            .retire_with_context(&store, task, call, |_| panic!("stale return"))
            .is_err());
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
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        let app_call = submit(&store, application, 0x1000);
        let worker_call = submit(&store, worker, 0x3000);

        store.switch_to_task(worker).unwrap();
        assert_eq!(store.depth(), 1);
        store.activate(worker, worker_call).unwrap();
        store.complete(worker, worker_call, None).unwrap();
        store.retire(worker, worker_call).unwrap();

        store.switch_to_task(application).unwrap();
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
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));

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
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        let call_id = submit(&store, application, 0x1000);
        let before = store.clone();
        assert!(matches!(
            store.retire_task(application),
            Err(ContinuationError::RetirementRefused {
                task,
                depth: 1,
                contexts: 0,
                current: true,
            }) if task == application
        ));
        assert_eq!(store, before);

        store.switch_to_task(worker).unwrap();
        let before = store.clone();
        assert!(matches!(
            store.retire_task(application),
            Err(ContinuationError::RetirementRefused {
                task,
                depth: 1,
                contexts: 0,
                current: false,
            }) if task == application
        ));
        assert_eq!(store, before);

        store.switch_to_task(application).unwrap();
        store.activate(application, call_id).unwrap();
        store.complete(application, call_id, None).unwrap();
        store.retire(application, call_id).unwrap();
        store.switch_to_task(worker).unwrap();
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

    #[test]
    fn concrete_contexts_are_keyed_by_exact_task_and_call() {
        let store = Store::default();
        let application = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(13);
        store.register_task(worker).unwrap();
        assert!(store.set_scheduling_state(worker, ExecutionTaskState::Ready));
        let application_call = submit(&store, application, 0x1000);
        let worker_call = submit(&store, worker, 0x2000);
        let mut contexts = ExecutionContextBank::default();

        assert_eq!(
            contexts.park(&store, application, application_call, 0xaaaa),
            Ok(())
        );
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts.task_len(application), 1);
        assert_eq!(contexts.task_len(worker), 0);

        store.switch_to_task(worker).unwrap();
        assert_eq!(contexts.park(&store, worker, worker_call, 0xbbbb), Ok(()));
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts.task_len(application), 1);
        assert_eq!(contexts.task_len(worker), 1);
        assert_eq!(contexts.take(&store, worker, worker_call), Ok(0xbbbb));
        assert!(!contexts.contains(worker, worker_call));

        store.switch_to_task(application).unwrap();
        assert_eq!(
            contexts.take(&store, application, application_call),
            Ok(0xaaaa)
        );
        assert!(contexts.is_empty());
    }

    #[test]
    fn attached_context_blocks_call_retirement_until_exact_restore() {
        let store = Store::default();
        let task = ExecutionTaskId::APPLICATION;
        let call_id = submit(&store, task, 0x1000);
        store.activate(task, call_id).unwrap();
        let mut contexts = ExecutionContextBank::default();
        contexts.park(&store, task, call_id, 7).unwrap();
        store.complete(task, call_id, None).unwrap();

        assert_eq!(
            store.retire(task, call_id),
            Err(ContinuationError::ContextAttached { call_id, count: 1 })
        );
        assert_eq!(contexts.take(&store, task, call_id), Ok(7));
        assert_eq!(store.retire(task, call_id).unwrap().call_id(), call_id);
        assert!(store.is_empty());
    }
}
