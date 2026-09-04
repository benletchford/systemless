//! Process-owned guest-procedure continuation frames.
//!
//! Mixed Mode resolves a universal procedure pointer, constructs the target
//! architecture's calling sequence, and returns through a switch frame owned
//! by the calling process. Inside Macintosh: PowerPC System Software (1994),
//! pp. 1-15--1-17 and 2-4--2-12. Keeping that continuation above either CPU
//! adapter lets nested 68k and native PowerPC callbacks share one LIFO owner
//! while each adapter remains responsible for its architectural registers and
//! ABI frame.

#[cfg(test)]
pub(crate) use crate::execution_kernel::MAX_POWERPC_GUEST_ARGUMENTS;
pub(crate) use crate::execution_kernel::{
    CallId, ContinuationPhase, ExecutionTaskId, GuestCallArguments, GuestCallContinuation,
    GuestCallEffect, GuestCallRequest, GuestCallReturnPolicy, GuestCallTarget, M68kCallRequest,
    M68kRegisterState, M68kResultSource, M68kResultTarget, M68kResume, PendingM68kExecution,
    PendingPowerPcExecution, PowerPcArguments, PowerPcReturnState,
};
use crate::execution_kernel::{
    ExecutionContextBank, ExecutionTaskContextBank, ExecutionTaskEffect, ExecutionTaskState,
};
use crate::guest_procedure::GuestIsa;
use ppc::{PpcCpu, PpcImportAction, PpcNativeReturnGpr3};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Saved 68K state for one cooperative Thread Manager thread.
///
/// Cooperative switches occur only inside `_ThreadDispatch`, so the HLE can
/// preserve the complete caller-visible register file without involving a
/// host thread. New threads inherit the creator's register world (notably A5)
/// and receive a private guest stack.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CooperativeThread {
    pub(crate) d_regs: [u32; 8],
    pub(crate) a_regs: [u32; 8],
    pub(crate) pc: u32,
    pub(crate) ccr: u8,
    /// `void **threadResult` the entry proc's return value is stored to.
    pub(crate) result_destination: u32,
    /// Lowest address of the private guest stack, or 0 for the
    /// application thread, which keeps the process stack.
    pub(crate) stack_base: u32,
    /// Address one past the top of the private guest stack.
    pub(crate) stack_limit: u32,
    /// `SetThreadSwitcher` switch-in proc and its `switchProcParam`.
    pub(crate) switch_in: (u32, u32),
    /// `SetThreadSwitcher` switch-out proc and its `switchProcParam`.
    pub(crate) switch_out: (u32, u32),
    /// `SetThreadTerminator` proc and its `terminationProcParam`.
    pub(crate) terminator: (u32, u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M68kCallOrigin {
    return_pc: u32,
    final_sp: u32,
    result: Option<M68kResultTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PowerPcCallOrigin {
    return_pc: u32,
    final_pc: u32,
    restore_rtoc: u32,
    return_gpr3: GuestCallReturnPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M68kExecution {
    entry: u32,
    initial_sp: u32,
    return_pc: u32,
    final_sp: u32,
    registers: M68kRegisterState,
    result: Option<M68kResultSource>,
    started: bool,
}

#[derive(Clone, Debug)]
struct PowerPcExecution {
    arguments: PowerPcArguments,
    return_pc: Option<u32>,
    completed: Option<PowerPcReturnState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuestCallOrigin {
    M68k(M68kCallOrigin),
    PowerPc(PowerPcCallOrigin),
}

#[derive(Clone, Debug)]
struct GuestCallFrame {
    target: GuestCallTarget,
    origin: GuestCallOrigin,
    m68k_execution: Option<M68kExecution>,
    powerpc_execution: Option<PowerPcExecution>,
}

impl From<PpcNativeReturnGpr3> for GuestCallReturnPolicy {
    fn from(policy: PpcNativeReturnGpr3) -> Self {
        match policy {
            PpcNativeReturnGpr3::Preserve => Self::Preserve,
            PpcNativeReturnGpr3::Mask(mask) => Self::Mask(mask),
            PpcNativeReturnGpr3::Set(value) => Self::Set(value),
            PpcNativeReturnGpr3::ZeroOrSet { zero, nonzero } => Self::ZeroOrSet { zero, nonzero },
            PpcNativeReturnGpr3::CrBit(bit_index) => Self::CrBit(bit_index),
            PpcNativeReturnGpr3::XerCa => Self::XerCa,
            PpcNativeReturnGpr3::XerOv => Self::XerOv,
        }
    }
}

impl From<GuestCallReturnPolicy> for PpcNativeReturnGpr3 {
    fn from(policy: GuestCallReturnPolicy) -> Self {
        match policy {
            GuestCallReturnPolicy::Preserve => Self::Preserve,
            GuestCallReturnPolicy::Mask(mask) => Self::Mask(mask),
            GuestCallReturnPolicy::Set(value) => Self::Set(value),
            GuestCallReturnPolicy::ZeroOrSet { zero, nonzero } => Self::ZeroOrSet { zero, nonzero },
            GuestCallReturnPolicy::CrBit(bit_index) => Self::CrBit(bit_index),
            GuestCallReturnPolicy::XerCa => Self::XerCa,
            GuestCallReturnPolicy::XerOv => Self::XerOv,
        }
    }
}

/// Compatibility aliases for the CPU-free semantic store. The concrete frame
/// map below remains responsible for parked CPU contexts until the runner is
/// migrated to consume continuation effects directly.
pub(crate) type ContinuationStore =
    crate::execution_kernel::ContinuationStore<GuestCallRequest, GuestCallContinuation>;

impl GuestCallEffect {
    /// Convert the semantic effect to the PPC CPU's import ABI.
    ///
    /// Only a native PowerPC target can be represented by `CallNative`; a
    /// cross-ISA effect remains in the process continuation owner and returns
    /// `None` here for the caller to schedule through its other adapter.
    pub(crate) fn into_ppc_import_action(self) -> Option<PpcImportAction> {
        let Self::CallGuest {
            request,
            continuation:
                GuestCallContinuation::ReturnToPowerPc {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                },
        } = self
        else {
            return None;
        };
        if request.target.isa != GuestIsa::PowerPc
            || !matches!(request.arguments, GuestCallArguments::None)
        {
            return None;
        }
        Some(PpcImportAction::CallNative {
            entry: request.target.entry,
            rtoc: request.target.rtoc,
            return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3: return_gpr3.into(),
        })
    }

    /// Decode the PPC ABI adapter action back into its neutral effect.
    #[cfg(test)]
    pub(crate) fn from_ppc_import_action(action: PpcImportAction) -> Option<Self> {
        Self::from_ppc_import_action_for_task(ExecutionTaskId::APPLICATION, action)
    }

    pub(crate) fn from_ppc_import_action_for_task(
        task: ExecutionTaskId,
        action: PpcImportAction,
    ) -> Option<Self> {
        let PpcImportAction::CallNative {
            entry,
            rtoc,
            return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3,
        } = action
        else {
            return None;
        };
        Some(Self::call_guest(
            GuestCallRequest::for_task(
                task,
                GuestCallTarget {
                    isa: GuestIsa::PowerPc,
                    entry,
                    rtoc,
                },
            ),
            GuestCallContinuation::to_powerpc(
                return_pc,
                final_pc,
                restore_rtoc,
                GuestCallReturnPolicy::from(return_gpr3),
            ),
        ))
    }

    fn into_frame(self) -> Option<GuestCallFrame> {
        let Self::CallGuest {
            request,
            continuation,
        } = self;
        let target = request.target;
        match (target.isa, request.arguments, continuation) {
            (
                GuestIsa::M68k,
                GuestCallArguments::None,
                GuestCallContinuation::ReturnToM68k {
                    return_pc,
                    final_sp,
                    result,
                },
            ) => Some(GuestCallFrame {
                target,
                origin: GuestCallOrigin::M68k(M68kCallOrigin {
                    return_pc,
                    final_sp,
                    result,
                }),
                m68k_execution: None,
                powerpc_execution: None,
            }),
            (
                GuestIsa::PowerPc,
                GuestCallArguments::PowerPc(arguments),
                GuestCallContinuation::ReturnToM68k {
                    return_pc,
                    final_sp,
                    result,
                },
            ) => Some(GuestCallFrame {
                target,
                origin: GuestCallOrigin::M68k(M68kCallOrigin {
                    return_pc,
                    final_sp,
                    result,
                }),
                m68k_execution: None,
                powerpc_execution: Some(PowerPcExecution {
                    arguments,
                    return_pc: None,
                    completed: None,
                }),
            }),
            (
                GuestIsa::PowerPc,
                GuestCallArguments::None,
                GuestCallContinuation::ReturnToPowerPc {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                },
            ) => Some(GuestCallFrame {
                target,
                origin: GuestCallOrigin::PowerPc(PowerPcCallOrigin {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                }),
                m68k_execution: None,
                powerpc_execution: None,
            }),
            (
                GuestIsa::M68k,
                GuestCallArguments::M68k(request),
                GuestCallContinuation::ReturnToPowerPc {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                },
            ) => Some(GuestCallFrame {
                target,
                origin: GuestCallOrigin::PowerPc(PowerPcCallOrigin {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                }),
                m68k_execution: Some(M68kExecution {
                    entry: request.entry,
                    initial_sp: request.initial_sp,
                    return_pc,
                    final_sp: request.final_sp,
                    registers: request.registers,
                    result: request.result,
                    started: false,
                }),
                powerpc_execution: None,
            }),
            _ => None,
        }
    }
}

/// Stable diagnostic spelling for the PPC ABI adapter's action vocabulary.
///
/// Keeping this formatter beside the adapter conversion means loader tracing
/// does not need to inspect the architecture-specific native-call variant.
pub(crate) fn format_ppc_import_action(action: &PpcImportAction) -> String {
    match action {
        PpcImportAction::Return(value) => format!("return(${:08X})", value),
        PpcImportAction::ReturnPreserve => "return-preserve".to_string(),
        PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles) => {
            format!("return-preserve+{}cycles", extra_cycles)
        }
        PpcImportAction::ReturnWithExtraCycles(value, extra_cycles) => {
            format!("return(${:08X})+{}cycles", value, extra_cycles)
        }
        PpcImportAction::Continue => "continue".to_string(),
        PpcImportAction::Yield(cycles) => format!("yield({cycles}cycles)"),
        PpcImportAction::CallNative {
            entry,
            rtoc,
            return_pc,
            final_pc,
            restore_rtoc,
            ..
        } => format!(
            "call-native(entry=${:08X},rtoc=${:08X},return_pc=${:08X},final_pc=${:08X},restore_rtoc=${:08X})",
            entry, rtoc, return_pc, final_pc, restore_rtoc
        ),
        PpcImportAction::RaiseException(exception) => format!("exception({:?})", exception),
        PpcImportAction::Halt => "halt".to_string(),
    }
}

impl PartialEq for GuestCallFrame {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.origin == other.origin
            && self.m68k_execution == other.m68k_execution
            && match (&self.powerpc_execution, &other.powerpc_execution) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.arguments == right.arguments
                        && left.return_pc == right.return_pc
                        && left.completed == right.completed
                }
                _ => false,
            }
    }
}

impl Eq for GuestCallFrame {}

#[derive(Debug)]
struct ExecutionTaskCalls {
    /// Authoritative task/order/phase state. The frame map below is only the
    /// temporary CPU-adapter projection keyed by this store's CallId.
    kernel: ContinuationStore,
    frames: HashMap<CallId, GuestCallFrame>,
    powerpc_contexts: ExecutionContextBank<Box<PpcCpu>>,
    cooperative_contexts: ExecutionTaskContextBank<CooperativeThread>,
}

impl Clone for ExecutionTaskCalls {
    fn clone(&self) -> Self {
        Self {
            kernel: self.kernel.clone(),
            frames: self.frames.clone(),
            powerpc_contexts: self.powerpc_contexts.clone(),
            cooperative_contexts: self.cooperative_contexts.clone(),
        }
    }
}

impl PartialEq for ExecutionTaskCalls {
    fn eq(&self, other: &Self) -> bool {
        self.kernel == other.kernel
            && self.frames == other.frames
            && self.powerpc_contexts.same_slots(&other.powerpc_contexts)
            && self.cooperative_contexts == other.cooperative_contexts
    }
}

impl Eq for ExecutionTaskCalls {}

impl Default for ExecutionTaskCalls {
    fn default() -> Self {
        Self {
            kernel: ContinuationStore::default(),
            frames: HashMap::new(),
            powerpc_contexts: ExecutionContextBank::default(),
            cooperative_contexts: ExecutionTaskContextBank::default(),
        }
    }
}

impl ExecutionTaskCalls {
    fn is_pristine(&self) -> bool {
        self.kernel.is_pristine() && self.cooperative_contexts.is_empty()
    }
}

/// Task-indexed guest-procedure continuation stacks for one process.
///
/// Both CPU adapters share this owner, but every Thread Manager task has an
/// independent LIFO stack. Switching tasks changes which stack subsequent
/// Mixed Mode operations address; it cannot expose another task's suspended
/// call. Ordinary `Clone` still creates an independent process snapshot.
#[derive(Debug, Default)]
pub(crate) struct SharedGuestCallStack(Rc<RefCell<ExecutionTaskCalls>>);

impl PartialEq for SharedGuestCallStack {
    fn eq(&self, other: &Self) -> bool {
        *self.0.borrow() == *other.0.borrow()
    }
}

impl Eq for SharedGuestCallStack {}

impl Clone for SharedGuestCallStack {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }
}

impl SharedGuestCallStack {
    pub(crate) fn is_pristine(&self) -> bool {
        self.0.borrow().is_pristine()
    }

    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    pub(crate) fn attach_to(&mut self, process_calls: &Self) {
        if Rc::ptr_eq(&self.0, &process_calls.0) {
            return;
        }
        assert!(
            self.is_pristine() || process_calls.is_pristine(),
            "cannot attach two initialized execution owners"
        );
        let pending = std::mem::take(&mut *self.0.borrow_mut());
        self.0 = Rc::clone(&process_calls.0);
        if !pending.is_pristine() {
            *self.0.borrow_mut() = pending;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().kernel.is_empty()
    }

    pub(crate) fn depth(&self) -> usize {
        self.0.borrow().kernel.depth()
    }

    /// Return the task whose continuation stack is currently active.
    ///
    /// The execution runner uses this only to associate architecture-specific
    /// parked contexts with the same cooperative task.  The task owner itself
    /// remains process-wide; this is a view of its current cursor, not a second
    /// owner.  Inside Macintosh: Processes (1994), pp. 4-4--4-6.
    pub(crate) fn current_task(&self) -> ExecutionTaskId {
        self.0.borrow().kernel.current_task()
    }

    /// Select the continuation owner for subsequent guest execution.
    pub(crate) fn register_task(&self, task: ExecutionTaskId) -> bool {
        self.apply_task_effect(ExecutionTaskEffect::Register(task))
    }

    pub(crate) fn switch_to_task(&self, task: ExecutionTaskId) -> bool {
        self.apply_task_effect(ExecutionTaskEffect::SwitchTo(task))
    }

    pub(crate) fn scheduling_state(&self, task: ExecutionTaskId) -> Option<ExecutionTaskState> {
        self.0.borrow().kernel.scheduling_state(task)
    }

    pub(crate) fn set_scheduling_state(
        &self,
        task: ExecutionTaskId,
        state: ExecutionTaskState,
    ) -> bool {
        self.0.borrow().kernel.set_scheduling_state(task, state)
    }

    pub(crate) fn set_state_ending_critical(
        &self,
        task: ExecutionTaskId,
        state: ExecutionTaskState,
    ) -> bool {
        self.0
            .borrow()
            .kernel
            .set_state_ending_critical(task, state)
    }

    pub(crate) fn next_ready_task(
        &self,
        suggested: Option<ExecutionTaskId>,
    ) -> Option<ExecutionTaskId> {
        self.0.borrow().kernel.next_ready_task(suggested)
    }

    #[cfg(test)]
    pub(crate) fn critical_depth(&self) -> u32 {
        self.0.borrow().kernel.critical_depth()
    }
    pub(crate) fn begin_critical(&self) {
        self.0.borrow().kernel.begin_critical();
    }
    pub(crate) fn end_critical(&self) -> bool {
        self.0.borrow().kernel.end_critical()
    }

    fn apply_task_effect(&self, effect: ExecutionTaskEffect) -> bool {
        self.0.borrow().kernel.apply_task_effect(effect).is_ok()
    }

    /// Drop a retired task's empty continuation stack.
    ///
    /// A non-empty stack denotes suspended execution and cannot be discarded.
    #[cfg(test)]
    pub(crate) fn remove_task(&self, task: ExecutionTaskId) -> bool {
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.retire_task(task).is_err() {
            return false;
        }
        tasks.cooperative_contexts.remove(task);
        true
    }

    /// Commit result delivery and retirement while the execution owner keeps
    /// both task identities and their adapter snapshots stable.
    pub(crate) fn retire_cooperative_context(
        &self,
        task: ExecutionTaskId,
        successor: Option<ExecutionTaskId>,
        commit: impl FnOnce(&CooperativeThread) -> bool,
    ) -> Option<(CooperativeThread, Option<CooperativeThread>)> {
        let mut tasks = self.0.borrow_mut();
        let finished = tasks.cooperative_contexts.get(task)?.clone();
        let next = match successor {
            Some(next) => Some(tasks.cooperative_contexts.get(next)?.clone()),
            None => None,
        };
        tasks
            .kernel
            .retire_task_with(task, successor, || commit(&finished))
            .ok()?;
        tasks.cooperative_contexts.remove(task);
        Some((finished, next))
    }

    pub(crate) fn cooperative_context(&self, task: ExecutionTaskId) -> Option<CooperativeThread> {
        self.0.borrow().cooperative_contexts.get(task).cloned()
    }

    /// Only registered tasks can retain adapter snapshots. No borrowed CPU
    /// context escapes the execution owner across a guest call or task switch.
    pub(crate) fn save_cooperative_context(
        &self,
        task: ExecutionTaskId,
        context: CooperativeThread,
    ) -> bool {
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.scheduling_state(task).is_none() {
            return false;
        }
        tasks.cooperative_contexts.insert(task, context);
        true
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.borrow().kernel.len()
    }

    #[cfg(test)]
    pub(crate) fn task_depth(&self, task: ExecutionTaskId) -> usize {
        self.0.borrow().kernel.task_depth(task)
    }

    /// Park concrete adapter state against an exact live continuation.
    #[cfg(test)]
    pub(crate) fn park_context<T>(
        &self,
        bank: &mut ExecutionContextBank<T>,
        task: ExecutionTaskId,
        call_id: CallId,
        context: T,
    ) -> Result<(), T> {
        let tasks = self.0.borrow();
        bank.park(&tasks.kernel, task, call_id, context)
            .map_err(|(_, context)| context)
    }

    /// Return the active 68K-origin switch frame below the top 68K callback.
    ///
    /// This exact identity replaces the runner's old nesting-depth inference.
    /// A repeated sibling callback sees the same owner token and reuses its
    /// already parked caller; a deeper callback sees the newer token.
    pub(crate) fn suspended_m68k_context_owner(&self) -> Option<(ExecutionTaskId, CallId)> {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        tasks
            .kernel
            .task_states(task)
            .into_iter()
            .rev()
            .filter_map(|semantic| {
                let frame = tasks.frames.get(&semantic.call_id())?;
                let execution = frame.powerpc_execution.as_ref()?;
                (matches!(frame.origin, GuestCallOrigin::M68k(_))
                    && execution.return_pc.is_some()
                    && execution.completed.is_none())
                .then_some((task, semantic.call_id()))
            })
            .next()
    }

    /// Return the exact continuation whose completed native result is ready
    /// to resume a 68K caller.
    pub(crate) fn pending_m68k_resume_owner(&self) -> Option<(ExecutionTaskId, CallId)> {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        let semantic = tasks.kernel.peek(task)?;
        let frame = tasks.frames.get(&semantic.call_id())?;
        (matches!(frame.origin, GuestCallOrigin::M68k(_))
            && frame
                .powerpc_execution
                .as_ref()
                .and_then(|execution| execution.completed)
                .is_some())
        .then_some((task, semantic.call_id()))
    }

    fn top_frame(&self) -> Option<(CallId, GuestCallFrame)> {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        let semantic = tasks.kernel.peek(task)?;
        let frame = tasks.frames.get(&semantic.call_id())?;
        Some((semantic.call_id(), frame.clone()))
    }

    fn submit_effect(&self, effect: GuestCallEffect) -> Option<CallId> {
        let GuestCallEffect::CallGuest {
            request,
            continuation,
        } = effect;
        let Some(frame) = GuestCallEffect::call_guest(request, continuation).into_frame() else {
            return None;
        };
        let task = self.current_task();
        let mut tasks = self.0.borrow_mut();
        let call_id = tasks.kernel.submit(task, request, continuation).ok()?;
        tasks.frames.insert(call_id, frame);
        Some(call_id)
    }

    fn push_effect(&self, effect: GuestCallEffect) -> bool {
        self.submit_effect(effect).is_some()
    }

    pub(crate) fn begin_m68k(
        &self,
        target: GuestCallTarget,
        return_pc: u32,
        final_sp: u32,
    ) -> bool {
        debug_assert_eq!(target.isa, GuestIsa::M68k);
        self.push_effect(GuestCallEffect::call_guest(
            GuestCallRequest::for_task(self.current_task(), target),
            GuestCallContinuation::to_m68k(return_pc, final_sp, None),
        ))
    }

    pub(crate) fn begin_m68k_to_powerpc(
        &self,
        target: GuestCallTarget,
        arguments: PowerPcArguments,
        return_pc: u32,
        final_sp: u32,
        result: Option<M68kResultTarget>,
    ) -> bool {
        if self.has_powerpc_from_m68k() {
            return false;
        }
        debug_assert_eq!(target.isa, GuestIsa::PowerPc);
        self.push_effect(GuestCallEffect::call_guest(
            GuestCallRequest::for_task(self.current_task(), target)
                .with_powerpc_arguments(arguments),
            GuestCallContinuation::to_m68k(return_pc, final_sp, result),
        ))
    }

    /// Park a native caller and retain the emulated 68k execution interval
    /// that will satisfy it. The caller's PowerPC registers remain in its CPU
    /// context; only the documented return state is applied at completion.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_powerpc_to_m68k(
        &self,
        target: GuestCallTarget,
        entry: u32,
        initial_sp: u32,
        return_pc: u32,
        final_sp: u32,
        registers: M68kRegisterState,
        result: Option<M68kResultSource>,
        final_pc: u32,
        restore_rtoc: u32,
        return_gpr3: impl Into<GuestCallReturnPolicy>,
    ) -> bool {
        debug_assert_eq!(target.isa, GuestIsa::M68k);
        self.push_effect(GuestCallEffect::call_guest(
            GuestCallRequest::for_task(self.current_task(), target).with_m68k_request(
                M68kCallRequest {
                    entry,
                    initial_sp,
                    final_sp,
                    registers,
                    result,
                },
            ),
            GuestCallContinuation::to_powerpc(return_pc, final_pc, restore_rtoc, return_gpr3),
        ))
    }

    pub(crate) fn pending_powerpc_from_m68k(&self) -> Option<PendingPowerPcExecution> {
        let (_, frame) = self.top_frame()?;
        let GuestCallOrigin::M68k(_) = frame.origin else {
            return None;
        };
        let execution = frame.powerpc_execution.as_ref()?;
        (execution.return_pc.is_none() && execution.completed.is_none()).then_some(
            PendingPowerPcExecution {
                target: frame.target,
                arguments: execution.arguments,
            },
        )
    }

    pub(crate) fn activate_powerpc_from_m68k(
        &self,
        cpu: &mut PpcCpu,
        return_pc: u32,
    ) -> Option<PendingPowerPcExecution> {
        let (task, call_id, target, arguments) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let semantic = tasks.kernel.peek(task)?;
            let frame = tasks.frames.get(&semantic.call_id())?;
            let GuestCallOrigin::M68k(_) = frame.origin else {
                return None;
            };
            let execution = frame.powerpc_execution.as_ref()?;
            if execution.return_pc.is_some() || execution.completed.is_some() {
                return None;
            }
            (task, semantic.call_id(), frame.target, execution.arguments)
        };
        let mut tasks = self.0.borrow_mut();
        let kernel = tasks.kernel.shared_handle();
        tasks
            .powerpc_contexts
            .park_while_activating(&kernel, task, call_id, Box::new(cpu.clone()))
            .ok()?;
        let frame = tasks
            .frames
            .get_mut(&call_id)
            .expect("semantic continuation must have an adapter frame");
        let execution = frame
            .powerpc_execution
            .as_mut()
            .expect("validated PowerPC transition must have an execution payload");
        execution.return_pc = Some(return_pc);
        Some(PendingPowerPcExecution { target, arguments })
    }

    pub(crate) fn has_powerpc_from_m68k(&self) -> bool {
        self.top_frame().is_some_and(|(_, frame)| {
            matches!(frame.origin, GuestCallOrigin::M68k(_)) && frame.powerpc_execution.is_some()
        })
    }

    /// Return the number of live 68k callers suspended in native PowerPC.
    ///
    /// The scheduler uses this depth to pair each nested PowerPC-to-68k call
    /// with the 68k CPU context parked by its enclosing switch frame. Mixed
    /// Mode links switch frames in LIFO order and preserves the emulated 68k
    /// context across every mode switch. Inside Macintosh: PowerPC System
    /// Software (1994), pp. 2-9--2-13.
    #[cfg(test)]
    pub(crate) fn suspended_m68k_context_depth(&self) -> usize {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        tasks
            .kernel
            .task_states(task)
            .iter()
            .filter_map(|semantic| tasks.frames.get(&semantic.call_id()))
            .filter(|frame| {
                matches!(frame.origin, GuestCallOrigin::M68k(_))
                    && frame
                        .powerpc_execution
                        .as_ref()
                        .is_some_and(|execution| execution.return_pc.is_some())
            })
            .count()
    }

    pub(crate) fn complete_powerpc_for_m68k(&self, cpu: &mut PpcCpu) -> bool {
        let (task, call_id) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let semantic = match tasks.kernel.peek(task) {
                Some(semantic) => semantic,
                None => return false,
            };
            let frame = match tasks.frames.get(&semantic.call_id()) {
                Some(frame) => frame,
                None => return false,
            };
            let GuestCallOrigin::M68k(_) = frame.origin else {
                return false;
            };
            let Some(execution) = frame.powerpc_execution.as_ref() else {
                return false;
            };
            if execution.completed.is_some() || execution.return_pc != Some(cpu.pc) {
                return false;
            }
            if !tasks.powerpc_contexts.contains(task, semantic.call_id()) {
                return false;
            }
            (task, semantic.call_id())
        };
        let result = PowerPcReturnState { gpr3: cpu.gpr[3] };
        let elapsed_time_base = cpu.time_base();
        let mut tasks = self.0.borrow_mut();
        let kernel = tasks.kernel.shared_handle();
        let Ok((parked_cpu, _)) =
            tasks
                .powerpc_contexts
                .take_while_completing(&kernel, task, call_id, Some(result.gpr3))
        else {
            return false;
        };
        let frame = tasks
            .frames
            .get_mut(&call_id)
            .expect("semantic continuation must have an adapter frame");
        let execution = frame
            .powerpc_execution
            .as_mut()
            .expect("validated PowerPC transition must have an execution payload");
        execution.completed = Some(result);
        *cpu = *parked_cpu;
        cpu.set_time_base(elapsed_time_base);
        true
    }

    /// Inspect the completed 68K-origin continuation without retiring it.
    ///
    /// Result placement belongs to the runner's 68K adapter and can fail for
    /// an unmapped/read-only destination. Keeping inspection separate from
    /// retirement lets that adapter retry the same completion after a failed
    /// application. Inside Macintosh: PowerPC System Software (1994),
    /// pp. 2-10--2-12.
    pub(crate) fn peek_m68k_resume(&self) -> Option<M68kResume> {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        let semantic = tasks.kernel.peek(task)?;
        let frame = tasks.frames.get(&semantic.call_id())?;
        let GuestCallOrigin::M68k(origin) = frame.origin else {
            return None;
        };
        let powerpc = frame.powerpc_execution.as_ref()?.completed?;
        Some(M68kResume {
            return_pc: origin.return_pc,
            final_sp: origin.final_sp,
            result: origin.result,
            powerpc,
        })
    }

    /// Retire the completed 68K-origin continuation after its result has been
    /// applied by the runner. No architectural state is changed here, so a
    /// failed result application can leave the frame available for retry.
    #[cfg(test)]
    pub(crate) fn retire_m68k_resume(&self) -> bool {
        let (task, call_id) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let Some(semantic) = tasks.kernel.peek(task) else {
                return false;
            };
            let Some(frame) = tasks.frames.get(&semantic.call_id()) else {
                return false;
            };
            if !matches!(frame.origin, GuestCallOrigin::M68k(_))
                || frame
                    .powerpc_execution
                    .as_ref()
                    .and_then(|execution| execution.completed)
                    .is_none()
            {
                return false;
            }
            (task, semantic.call_id())
        };
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.retire(task, call_id).is_err() {
            return false;
        }
        tasks.frames.remove(&call_id).is_some()
    }

    /// Commit the ABI result and restore the exact caller under one validated
    /// retirement boundary. The adapter closure must leave state unchanged
    /// when it rejects a result and must not execute guest code.
    pub(crate) fn commit_m68k_resume<T>(
        &self,
        bank: &mut ExecutionContextBank<T>,
        apply: impl FnOnce(M68kResume, Option<&mut T>) -> bool,
    ) -> Option<Option<T>> {
        let resume = self.peek_m68k_resume()?;
        let (task, call_id) = self.pending_m68k_resume_owner()?;
        let mut tasks = self.0.borrow_mut();
        let context = bank
            .retire_with_context(&tasks.kernel, task, call_id, |context| {
                apply(resume, context)
            })
            .ok()?;
        tasks
            .frames
            .remove(&call_id)
            .expect("validated resume frame");
        Some(context)
    }

    /// Take and retire a completed 68K-origin continuation.
    ///
    /// New runner code should prefer [`Self::peek_m68k_resume`] followed by
    /// [`Self::retire_m68k_resume`] so result placement remains retryable.
    #[cfg(test)]
    pub(crate) fn take_m68k_resume(&self) -> Option<M68kResume> {
        let resume = self.peek_m68k_resume()?;
        self.retire_m68k_resume().then_some(resume)
    }

    /// Return the top cross-ISA 68k interval and mark its CPU context active.
    #[cfg(test)]
    pub(crate) fn activate_m68k(&self) -> Option<PendingM68kExecution> {
        self.activate_m68k_in_bank(&mut ExecutionContextBank::<()>::default(), &mut (), None)
    }

    pub(crate) fn activate_m68k_parking<T: Default>(
        &self,
        bank: &mut ExecutionContextBank<T>,
        installed: &mut T,
    ) -> Option<PendingM68kExecution> {
        let caller = self.suspended_m68k_context_owner().map(|(_, call)| call);
        self.activate_m68k_in_bank(bank, installed, caller)
    }

    fn activate_m68k_in_bank<T: Default>(
        &self,
        bank: &mut ExecutionContextBank<T>,
        installed: &mut T,
        caller: Option<CallId>,
    ) -> Option<PendingM68kExecution> {
        let (task, call_id, pending, started) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let semantic = tasks.kernel.peek(task)?;
            let frame = tasks.frames.get(&semantic.call_id())?;
            let execution = frame.m68k_execution.as_ref()?;
            let pending = PendingM68kExecution {
                entry: execution.entry,
                initial_sp: execution.initial_sp,
                return_pc: execution.return_pc,
                final_sp: execution.final_sp,
                registers: execution.registers,
                result: execution.result,
            };
            (task, semantic.call_id(), pending, execution.started)
        };
        if started {
            return Some(pending);
        }
        let mut tasks = self.0.borrow_mut();
        bank.activate_parking_caller(&tasks.kernel, task, call_id, caller, installed)
            .ok()?;
        let frame = tasks
            .frames
            .get_mut(&call_id)
            .expect("semantic continuation must have an adapter frame");
        frame
            .m68k_execution
            .as_mut()
            .expect("validated 68k transition must have an execution payload")
            .started = true;
        Some(pending)
    }

    pub(crate) fn active_m68k(&self) -> Option<PendingM68kExecution> {
        let (_, frame) = self.top_frame()?;
        let execution = frame.m68k_execution?;
        execution.started.then_some(PendingM68kExecution {
            entry: execution.entry,
            initial_sp: execution.initial_sp,
            return_pc: execution.return_pc,
            final_sp: execution.final_sp,
            registers: execution.registers,
            result: execution.result,
        })
    }

    pub(crate) fn has_m68k_execution(&self) -> bool {
        self.top_frame()
            .is_some_and(|(_, frame)| frame.m68k_execution.is_some())
    }

    /// Move the continuation embedded in a CPU action into the process stack
    /// and arrange the next native PowerPC context directly.
    pub(crate) fn externalize_powerpc_action(
        &self,
        cpu: &mut PpcCpu,
        action: PpcImportAction,
    ) -> PpcImportAction {
        let Some(effect) =
            GuestCallEffect::from_ppc_import_action_for_task(self.current_task(), action)
        else {
            return action;
        };
        let Some(call_id) = self.submit_effect(effect) else {
            return action;
        };
        let task = self.current_task();
        {
            let tasks = self.0.borrow_mut();
            if tasks.kernel.activate(task, call_id).is_err() {
                let _ = tasks.kernel.cancel_pending(task, call_id);
                drop(tasks);
                self.0.borrow_mut().frames.remove(&call_id);
                return action;
            }
        }
        let target = effect.request().target;
        cpu.pc = target.entry;
        let GuestCallEffect::CallGuest { continuation, .. } = effect;
        let GuestCallContinuation::ReturnToPowerPc { return_pc, .. } = continuation else {
            unreachable!("PPC action conversion always has a PowerPC continuation");
        };
        cpu.lr = return_pc;
        cpu.gpr[2] = target.rtoc;
        PpcImportAction::Continue
    }

    /// Complete the top native frame only when the CPU reached its exact
    /// synthetic return import. A frame belonging to 68k remains untouched.
    pub(crate) fn complete_powerpc(&self, cpu: &mut PpcCpu) -> bool {
        let (task, call_id, origin) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let semantic = match tasks.kernel.peek(task) {
                Some(semantic) => semantic,
                None => return false,
            };
            let frame = match tasks.frames.get(&semantic.call_id()) {
                Some(frame) => frame,
                None => return false,
            };
            let GuestCallOrigin::PowerPc(origin) = frame.origin else {
                return false;
            };
            if frame.target.isa != GuestIsa::PowerPc || cpu.pc != origin.return_pc {
                return false;
            }
            (task, semantic.call_id(), origin)
        };
        let mut tasks = self.0.borrow_mut();
        if tasks
            .kernel
            .complete(task, call_id, Some(cpu.gpr[3]))
            .is_err()
        {
            return false;
        }
        let _ = tasks
            .kernel
            .retire(task, call_id)
            .expect("completed native continuation must retire transactionally");
        tasks
            .frames
            .remove(&call_id)
            .expect("semantic continuation must have an adapter frame");
        Self::apply_powerpc_return(cpu, origin);
        true
    }

    /// Complete an emulated 68k interval for its parked native caller.
    pub(crate) fn complete_m68k_for_powerpc(
        &self,
        post_call_pc: u32,
        final_sp: u32,
        result: Option<u32>,
        cpu: &mut PpcCpu,
    ) -> bool {
        let (task, call_id, origin) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let semantic = match tasks.kernel.peek(task) {
                Some(semantic) => semantic,
                None => return false,
            };
            let frame = match tasks.frames.get(&semantic.call_id()) {
                Some(frame) => frame,
                None => return false,
            };
            let GuestCallOrigin::PowerPc(origin) = frame.origin else {
                return false;
            };
            let Some(execution) = frame.m68k_execution else {
                return false;
            };
            let result_required = match execution.result {
                None => Some(false),
                Some(M68kResultSource::SpecialCase { selector, .. }) => {
                    let proc_info = crate::mixed_mode::proc_info::SPECIAL_CASE
                        | (u32::from(selector) << crate::mixed_mode::special_case::SELECTOR_PHASE);
                    crate::mixed_mode::native_special_case_signature(proc_info).map(|signature| {
                        signature.result != crate::mixed_mode::NativeSpecialCaseResult::Void
                    })
                }
                Some(_) => Some(true),
            };
            if !execution.started
                || post_call_pc != execution.return_pc
                || final_sp != execution.final_sp
                // Special-case callbacks can have output-layout side effects
                // while still being native void procedures. Match the decoded
                // ABI exactly: void has no value, every other result source
                // must supply one, and an invalid selector cannot complete.
                || result_required.is_none()
                || result.is_some() != result_required.unwrap_or(false)
            {
                return false;
            }
            (task, semantic.call_id(), origin)
        };
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.complete(task, call_id, result).is_err() {
            return false;
        }
        let _ = tasks
            .kernel
            .retire(task, call_id)
            .expect("completed 68k continuation must retire transactionally");
        tasks
            .frames
            .remove(&call_id)
            .expect("semantic continuation must have an adapter frame");
        if let Some(result) = result {
            cpu.gpr[3] = result;
        }
        Self::apply_powerpc_return(cpu, origin);
        true
    }

    fn apply_powerpc_return(cpu: &mut PpcCpu, origin: PowerPcCallOrigin) {
        match origin.return_gpr3 {
            GuestCallReturnPolicy::Preserve => {}
            GuestCallReturnPolicy::Mask(mask) => cpu.gpr[3] &= mask,
            GuestCallReturnPolicy::Set(value) => cpu.gpr[3] = value,
            GuestCallReturnPolicy::ZeroOrSet { zero, nonzero } => {
                cpu.gpr[3] = if cpu.gpr[3] == 0 { zero } else { nonzero };
            }
            GuestCallReturnPolicy::CrBit(bit_index) => {
                cpu.gpr[3] = u32::from(cpu.cr_bit(bit_index));
            }
            GuestCallReturnPolicy::XerCa => cpu.gpr[3] = u32::from(cpu.xer_ca()),
            GuestCallReturnPolicy::XerOv => cpu.gpr[3] = u32::from(cpu.xer_ov()),
        }
        cpu.gpr[2] = origin.restore_rtoc;
        cpu.lr = origin.final_pc;
        cpu.pc = origin.final_pc;
    }

    /// Complete the top classic frame only after its trampoline restored the
    /// exact caller PC and stack pointer. A native frame remains untouched.
    pub(crate) fn complete_m68k(&self, post_trap_pc: u32, final_sp: u32) -> bool {
        let (task, call_id) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let Some(semantic) = tasks.kernel.peek(task) else {
                return false;
            };
            let Some(frame) = tasks.frames.get(&semantic.call_id()) else {
                return false;
            };
            if frame.target.isa != GuestIsa::M68k
                || !matches!(
                    frame.origin,
                    GuestCallOrigin::M68k(origin)
                        if origin.return_pc.wrapping_add(2) == post_trap_pc
                            && origin.final_sp == final_sp
                )
            {
                return false;
            }
            (task, semantic.call_id())
        };
        let mut tasks = self.0.borrow_mut();
        let phase = tasks
            .kernel
            .peek(task)
            .expect("semantic continuation must have an adapter frame")
            .phase();
        if phase == ContinuationPhase::Pending && tasks.kernel.activate(task, call_id).is_err() {
            return false;
        }
        if tasks.kernel.complete(task, call_id, None).is_err() {
            return false;
        }
        let _ = tasks
            .kernel
            .retire(task, call_id)
            .expect("completed 68k continuation must retire transactionally");
        tasks
            .frames
            .remove(&call_id)
            .expect("semantic continuation must have an adapter frame");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::GuestAddressSpace;
    use ppc::PpcRunResult;

    const RETURN_PC: u32 = 0x01f0_4000;

    fn native_action(
        entry: u32,
        final_pc: u32,
        return_gpr3: PpcNativeReturnGpr3,
    ) -> PpcImportAction {
        GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry,
                rtoc: entry + 0x100,
            }),
            GuestCallContinuation::to_powerpc(RETURN_PC, final_pc, final_pc + 0x100, return_gpr3),
        )
        .into_ppc_import_action()
        .expect("native PowerPC request should adapt to CallNative")
    }

    #[test]
    fn explicit_shared_handles_share_live_frames_while_clone_is_detached() {
        let calls = SharedGuestCallStack::default();
        let shared = calls.shared_handle();
        let detached = calls.clone();
        calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(shared.len(), 1);
        assert!(detached.is_empty());
        assert!(shared.complete_m68k(0x2002, 0x3000));
        assert!(calls.is_empty());
    }

    #[test]
    fn attachment_preserves_idle_task_snapshots_and_refuses_two_initialized_owners() {
        let process = SharedGuestCallStack::default();
        let mut adapter = SharedGuestCallStack::default();
        let task = ExecutionTaskId::APPLICATION;
        let mut context = CooperativeThread::default();
        context.pc = 0x1234;
        assert!(adapter.save_cooperative_context(task, context.clone()));
        adapter.attach_to(&process);
        assert_eq!(process.cooperative_context(task), Some(context.clone()));
        let mut empty = SharedGuestCallStack::default();
        empty.attach_to(&process);
        assert_eq!(empty.cooperative_context(task), Some(context.clone()));
        let mut other = SharedGuestCallStack::default();
        let mut conflicting = context.clone();
        conflicting.pc = 0x5678;
        assert!(other.save_cooperative_context(task, conflicting.clone()));
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || other.attach_to(&process)
        ))
        .is_err());
        assert_eq!(process.cooperative_context(task), Some(context));
        assert_eq!(other.cooperative_context(task), Some(conflicting));
    }

    #[test]
    fn cooperative_snapshots_share_live_ownership_but_clone_and_retire_with_the_task() {
        let calls = SharedGuestCallStack::default();
        let worker = ExecutionTaskId::from_thread_id(3);
        let mut context = CooperativeThread::default();
        context.d_regs[0] = 42;
        assert!(!calls.save_cooperative_context(worker, context.clone()));
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.save_cooperative_context(worker, context.clone()));
        assert!(calls.switch_to_task(worker));
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0
            },
            0x2000,
            0x3000
        ));
        assert!(calls.switch_to_task(ExecutionTaskId::APPLICATION));
        assert!(!calls.remove_task(worker));
        assert!(calls
            .retire_cooperative_context(worker, None, |_| {
                panic!("pending continuations must reject retirement before result delivery")
            })
            .is_none());
        assert_eq!(calls.cooperative_context(worker), Some(context.clone()));
        let detached = calls.clone();
        let shared = calls.shared_handle();
        context.d_regs[0] = 99;
        assert!(calls.save_cooperative_context(worker, context.clone()));
        assert_eq!(shared.cooperative_context(worker), Some(context.clone()));
        assert_eq!(detached.cooperative_context(worker).unwrap().d_regs[0], 42);
        assert!(calls.switch_to_task(worker));
        assert!(calls.complete_m68k(0x2002, 0x3000));
        assert!(calls.switch_to_task(ExecutionTaskId::APPLICATION));
        assert!(calls.remove_task(worker));
        assert!(shared.cooperative_context(worker).is_none());
        assert!(!calls.save_cooperative_context(worker, context));
        assert!(detached.cooperative_context(worker).is_some());
    }

    #[test]
    fn stale_effect_task_is_rejected_without_rewriting_or_allocating_a_call() {
        let calls = SharedGuestCallStack::default();
        let effect = GuestCallEffect::call_guest(
            GuestCallRequest::for_task(
                ExecutionTaskId::APPLICATION,
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry: 0x1000,
                    rtoc: 0,
                },
            ),
            GuestCallContinuation::to_m68k(0x2000, 0x3000, None),
        );
        assert!(calls.register_task(ExecutionTaskId::from_thread_id(7)));
        assert!(calls.set_scheduling_state(
            ExecutionTaskId::from_thread_id(7),
            ExecutionTaskState::Ready
        ));
        assert!(calls.switch_to_task(ExecutionTaskId::from_thread_id(7)));
        let before = calls.clone();
        assert!(!calls.push_effect(effect));
        assert_eq!(calls, before);
        calls.switch_to_task(ExecutionTaskId::APPLICATION);
        assert!(calls.push_effect(effect));
        assert!(calls.complete_m68k(0x2002, 0x3000));
    }

    #[test]
    fn execution_tasks_keep_independent_continuation_stacks() {
        let calls = SharedGuestCallStack::default();
        calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );

        let worker = ExecutionTaskId::from_thread_id(7);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        calls.switch_to_task(worker);
        assert_eq!(calls.depth(), 0);
        calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x4000,
                rtoc: 0,
            },
            0x5000,
            0x6000,
        );
        assert!(!calls.remove_task(worker));
        assert!(calls.complete_m68k(0x5002, 0x6000));

        calls.switch_to_task(ExecutionTaskId::APPLICATION);
        assert_eq!(calls.depth(), 1);
        assert!(calls.complete_m68k(0x2002, 0x3000));
        assert!(calls.remove_task(worker));
        assert!(calls.is_empty());
    }

    #[test]
    fn global_stack_queries_include_suspended_tasks() {
        let calls = SharedGuestCallStack::default();
        let worker = ExecutionTaskId::from_thread_id(7);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        calls.switch_to_task(worker);
        calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x4000,
                rtoc: 0,
            },
            0x5000,
            0x6000,
        );

        calls.switch_to_task(ExecutionTaskId::APPLICATION);
        assert_eq!(calls.depth(), 0, "the active task has no continuation");
        assert_eq!(calls.len(), 1, "the process still has a suspended frame");
        assert!(
            !calls.is_empty(),
            "suspended tasks keep the owner non-empty"
        );
    }

    #[test]
    fn attachment_preserves_pending_frames_when_the_process_owner_is_empty() {
        let process_calls = SharedGuestCallStack::default();
        let mut adapter_calls = SharedGuestCallStack::default();
        adapter_calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );

        adapter_calls.attach_to(&process_calls);

        assert_eq!(process_calls.len(), 1);
        assert!(adapter_calls.complete_m68k(0x2002, 0x3000));
        assert!(process_calls.is_empty());
    }

    #[test]
    fn powerpc_action_is_externalized_and_restored_by_the_process_owner() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        let action = calls.externalize_powerpc_action(
            &mut cpu,
            native_action(0x1000, 0x2000, PpcNativeReturnGpr3::Mask(0xff)),
        );

        assert_eq!(action, PpcImportAction::Continue);
        assert_eq!((cpu.pc, cpu.lr, cpu.gpr[2]), (0x1000, RETURN_PC, 0x1100));
        assert_eq!(calls.len(), 1);

        cpu.pc = RETURN_PC;
        cpu.gpr[3] = 0x1234;
        assert!(calls.complete_powerpc(&mut cpu));
        assert_eq!((cpu.pc, cpu.lr, cpu.gpr[2]), (0x2000, 0x2000, 0x2100));
        assert_eq!(cpu.gpr[3], 0x34);
        assert!(calls.is_empty());
    }

    #[test]
    fn powerpc_continuation_survives_across_cpu_execution_slices() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1000, 0x4e80_0020u32.to_be_bytes().to_vec());
        memory.add_region(RETURN_PC, vec![0; 4]);
        calls.externalize_powerpc_action(
            &mut cpu,
            native_action(0x1000, 0x2000, PpcNativeReturnGpr3::Preserve),
        );

        assert_eq!(
            cpu.run_with_imports(&mut memory, 1, 0, RETURN_PC, 1, |_, _, _| {
                PpcImportAction::Halt
            }),
            PpcRunResult::CycleLimit { cycles: 1 }
        );
        assert_eq!(cpu.pc, RETURN_PC);
        assert_eq!(calls.len(), 1);

        assert_eq!(
            cpu.run_with_imports(&mut memory, 1, 0, RETURN_PC, 1, |_, cpu, _| {
                assert!(calls.complete_powerpc(cpu));
                PpcImportAction::Continue
            }),
            PpcRunResult::CycleLimit { cycles: 1 }
        );
        assert_eq!(cpu.pc, 0x2000);
        assert!(calls.is_empty());
    }

    #[test]
    fn nested_powerpc_calls_complete_in_lifo_order() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        calls.externalize_powerpc_action(
            &mut cpu,
            native_action(0x1000, 0x2000, PpcNativeReturnGpr3::Preserve),
        );
        calls.externalize_powerpc_action(
            &mut cpu,
            native_action(0x3000, 0x4000, PpcNativeReturnGpr3::Set(7)),
        );

        cpu.pc = RETURN_PC;
        assert!(calls.complete_powerpc(&mut cpu));
        assert_eq!((cpu.pc, cpu.gpr[3]), (0x4000, 7));
        assert_eq!(calls.len(), 1);

        cpu.pc = RETURN_PC;
        assert!(calls.complete_powerpc(&mut cpu));
        assert_eq!(cpu.pc, 0x2000);
        assert!(calls.is_empty());
    }

    #[test]
    fn reverse_transition_parks_and_restores_the_native_context_and_elapsed_time() {
        let calls = SharedGuestCallStack::default();
        let arguments = PowerPcArguments::from_slice(&[1, 2, 3]).unwrap();
        assert!(calls.begin_m68k_to_powerpc(
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: 0x1000,
                rtoc: 0x2000,
            },
            arguments,
            0x3000,
            0x4000,
            Some(M68kResultTarget::Data { index: 2, size: 2 }),
        ));

        let mut cpu = PpcCpu::new();
        cpu.pc = 0x5000;
        cpu.lr = 0x6000;
        cpu.gpr[1] = 0x7000;
        cpu.gpr[2] = 0x8000;
        cpu.gpr[3] = 0x9000;
        cpu.set_time_base(10);
        assert_eq!(
            calls.activate_powerpc_from_m68k(&mut cpu, RETURN_PC),
            Some(PendingPowerPcExecution {
                target: GuestCallTarget {
                    isa: GuestIsa::PowerPc,
                    entry: 0x1000,
                    rtoc: 0x2000,
                },
                arguments,
            })
        );
        assert!(calls
            .activate_powerpc_from_m68k(&mut cpu, RETURN_PC)
            .is_none());

        cpu.pc = RETURN_PC;
        cpu.lr = 0xaaaa;
        cpu.gpr[1] = 0xbbbb;
        cpu.gpr[2] = 0xcccc;
        cpu.gpr[3] = 0x1234_5678;
        cpu.set_time_base(50);
        assert!(!calls.complete_powerpc(&mut cpu));
        assert!(calls.complete_powerpc_for_m68k(&mut cpu));

        assert_eq!((cpu.pc, cpu.lr), (0x5000, 0x6000));
        assert_eq!(
            (cpu.gpr[1], cpu.gpr[2], cpu.gpr[3]),
            (0x7000, 0x8000, 0x9000)
        );
        assert_eq!(cpu.time_base(), 50);
        let resume = calls.take_m68k_resume().unwrap();
        assert_eq!((resume.return_pc, resume.final_sp), (0x3000, 0x4000));
        assert_eq!(resume.powerpc.gpr3, 0x1234_5678);
        assert!(calls.is_empty());
    }

    #[test]
    fn deeper_cross_isa_transition_completes_in_lifo_order() {
        let calls = SharedGuestCallStack::default();
        assert!(calls.begin_m68k_to_powerpc(
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: 0x1000,
                rtoc: 0x2000,
            },
            PowerPcArguments::from_slice(&[]).unwrap(),
            0x3000,
            0x4000,
            None,
        ));
        assert_eq!(calls.suspended_m68k_context_depth(), 0);
        let mut cpu = PpcCpu::new();
        assert!(calls
            .activate_powerpc_from_m68k(&mut cpu, RETURN_PC)
            .is_some());
        assert_eq!(calls.suspended_m68k_context_depth(), 1);

        assert!(calls.begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x5000,
                rtoc: 0,
            },
            0x5000,
            0x6000,
            0x7000,
            0x6004,
            M68kRegisterState::default(),
            None,
            0x8000,
            0x9000,
            PpcNativeReturnGpr3::Preserve,
        ));
        assert_eq!(calls.len(), 2);
        assert!(!calls.has_powerpc_from_m68k());
        assert!(calls.has_m68k_execution());
        assert!(calls.activate_m68k().is_some());

        assert!(calls.begin_m68k_to_powerpc(
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: 0xa000,
                rtoc: 0xb000,
            },
            PowerPcArguments::from_slice(&[0xc000]).unwrap(),
            0xd000,
            0xe000,
            None,
        ));
        assert!(calls
            .activate_powerpc_from_m68k(&mut cpu, RETURN_PC + 4)
            .is_some());
        assert_eq!(calls.suspended_m68k_context_depth(), 2);
        assert!(calls.begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0xf000,
                rtoc: 0,
            },
            0xf000,
            0x1_0000,
            0x1_1000,
            0x1_0004,
            M68kRegisterState::default(),
            None,
            0x1_2000,
            0x1_3000,
            PpcNativeReturnGpr3::Preserve,
        ));
        assert!(calls.activate_m68k().is_some());
        assert!(!calls.complete_m68k_for_powerpc(0x7000, 0x6004, None, &mut cpu));
        assert!(calls.complete_m68k_for_powerpc(0x1_1000, 0x1_0004, None, &mut cpu));
        assert_eq!(calls.suspended_m68k_context_depth(), 2);

        cpu.pc = RETURN_PC + 4;
        assert!(calls.complete_powerpc_for_m68k(&mut cpu));
        let nested_resume = calls.take_m68k_resume().unwrap();
        assert_eq!(
            (nested_resume.return_pc, nested_resume.final_sp),
            (0xd000, 0xe000)
        );
        assert_eq!(calls.suspended_m68k_context_depth(), 1);

        assert!(!calls.complete_m68k_for_powerpc(0x7004, 0x6004, None, &mut cpu));
        assert!(calls.complete_m68k_for_powerpc(0x7000, 0x6004, None, &mut cpu));
        assert_eq!(calls.len(), 1);
        assert!(calls.has_powerpc_from_m68k());
        assert_eq!(calls.suspended_m68k_context_depth(), 1);

        cpu.pc = RETURN_PC;
        cpu.gpr[3] = 0x1234_5678;
        assert!(calls.complete_powerpc_for_m68k(&mut cpu));
        let resume = calls.take_m68k_resume().unwrap();
        assert_eq!((resume.return_pc, resume.final_sp), (0x3000, 0x4000));
        assert_eq!(resume.powerpc.gpr3, 0x1234_5678);
        assert_eq!(calls.suspended_m68k_context_depth(), 0);
        assert!(calls.is_empty());
    }

    #[test]
    fn cross_isa_frame_activates_once_and_restores_its_native_caller() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        cpu.gpr[2] = 0xaaaa_0000;
        assert!(calls.begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
            0x4000,
            0x3004,
            M68kRegisterState::default(),
            None,
            0x5000,
            0x6000,
            PpcNativeReturnGpr3::Preserve,
        ));

        assert!(calls.active_m68k().is_none());
        assert!(calls.has_m68k_execution());
        assert_eq!(
            calls.activate_m68k(),
            Some(PendingM68kExecution {
                entry: 0x2000,
                initial_sp: 0x3000,
                return_pc: 0x4000,
                final_sp: 0x3004,
                registers: M68kRegisterState::default(),
                result: None,
            })
        );
        assert_eq!(calls.active_m68k(), calls.activate_m68k());
        assert!(!calls.complete_m68k_for_powerpc(0x4000, 0x3000, None, &mut cpu));
        assert!(calls.complete_m68k_for_powerpc(0x4000, 0x3004, None, &mut cpu));
        assert_eq!((cpu.pc, cpu.lr, cpu.gpr[2]), (0x5000, 0x5000, 0x6000));
        assert!(calls.is_empty());
    }

    #[test]
    fn m68k_completion_requires_a_result_exactly_when_the_callback_abi_does() {
        use crate::mixed_mode::special_case;

        let mut cpu = PpcCpu::new();
        for (selector, supplied_result) in [
            (special_case::HIGH_HOOK as u8, None),
            (special_case::EOL_HOOK as u8, Some(1)),
        ] {
            let calls = SharedGuestCallStack::default();
            assert!(calls.begin_powerpc_to_m68k(
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry: 0x1000,
                    rtoc: 0,
                },
                0x2000,
                0x3000,
                0x4000,
                0x3004,
                M68kRegisterState::default(),
                Some(M68kResultSource::SpecialCase {
                    selector,
                    arguments: PowerPcArguments::from_slice(&[]).unwrap(),
                    stack_result: None,
                }),
                0x5000,
                0x6000,
                PpcNativeReturnGpr3::Preserve,
            ));
            assert!(calls.activate_m68k().is_some());

            let wrong_result = supplied_result.map_or(Some(1), |_| None);
            assert!(!calls.complete_m68k_for_powerpc(0x4000, 0x3004, wrong_result, &mut cpu));
            assert!(calls.complete_m68k_for_powerpc(0x4000, 0x3004, supplied_result, &mut cpu,));
        }
    }

    #[test]
    fn powerpc_completion_preserves_every_native_result_policy() {
        for (policy, input, expected) in [
            (PpcNativeReturnGpr3::Preserve, 0x1234, 0x1234),
            (PpcNativeReturnGpr3::Mask(0xff), 0x1234, 0x34),
            (PpcNativeReturnGpr3::Set(9), 0x1234, 9),
            (
                PpcNativeReturnGpr3::ZeroOrSet {
                    zero: 10,
                    nonzero: 11,
                },
                0,
                10,
            ),
            (
                PpcNativeReturnGpr3::ZeroOrSet {
                    zero: 10,
                    nonzero: 11,
                },
                1,
                11,
            ),
            (PpcNativeReturnGpr3::CrBit(2), 0, 1),
            (PpcNativeReturnGpr3::XerCa, 0, 1),
            (PpcNativeReturnGpr3::XerOv, 0, 1),
        ] {
            let calls = SharedGuestCallStack::default();
            let mut cpu = PpcCpu::new();
            cpu.set_cr_bit(2, true);
            cpu.set_xer_ca(true);
            cpu.xer |= 1 << 30;
            calls.externalize_powerpc_action(&mut cpu, native_action(0x1000, 0x2000, policy));
            cpu.pc = RETURN_PC;
            cpu.gpr[3] = input;

            assert!(calls.complete_powerpc(&mut cpu));
            assert_eq!(cpu.gpr[3], expected, "policy {policy:?}");
        }
    }

    #[test]
    fn completion_never_consumes_the_other_architectures_frame() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );
        cpu.pc = RETURN_PC;
        assert!(!calls.complete_powerpc(&mut cpu));
        assert_eq!(calls.len(), 1);
        assert!(calls.complete_m68k(0x2002, 0x3000));

        calls.externalize_powerpc_action(
            &mut cpu,
            native_action(0x4000, 0x5000, PpcNativeReturnGpr3::Preserve),
        );
        assert!(!calls.complete_m68k(0x5000, 0x6000));
        assert_eq!(calls.len(), 1);
    }
}
