//! Process-owned guest-procedure continuation frames.
//!
//! Mixed Mode resolves a universal procedure pointer, constructs the target
//! architecture's calling sequence, and returns through a switch frame owned
//! by the calling process. Inside Macintosh: PowerPC System Software (1994),
//! pp. 1-15--1-17 and 2-4--2-12. Keeping that continuation above either CPU
//! adapter lets nested 68k and native PowerPC callbacks share one LIFO owner
//! while each adapter remains responsible for its architectural registers and
//! ABI frame.

use crate::cfm::{CfmLoadId, CfmLoadOperation, CfmOperation};
use crate::cpu::{CpuOps, M68kCpu, M68kExtendedContext, Register};
#[cfg(test)]
pub(crate) use crate::execution_kernel::MAX_POWERPC_GUEST_ARGUMENTS;
pub(crate) use crate::execution_kernel::{
    CallId, ContinuationPhase, ExecutionTaskId, GuestCallArguments, GuestCallContinuation,
    GuestCallEffect, GuestCallRequest, GuestCallReturnPolicy, GuestCallTarget, M68kCallRequest,
    M68kRegisterState, M68kResultSource, M68kResultTarget, M68kResume, PendingM68kExecution,
    PendingPowerPcExecution, PowerPcArguments, PowerPcReturnState,
};
use crate::execution_kernel::{
    ExecutionContextBank, ExecutionRoute, ExecutionTaskContextBank, ExecutionTaskState,
    NativeAvailability,
};
use crate::guest_procedure::GuestIsa;
use crate::memory::GuestAddressSpace;
use crate::process_context::{ProcessNativeMemoryManager, SharedProcessValue};
use ppc::{PpcCpu, PpcImportAction, PpcMemory, PpcNativeReturnGpr3};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Install a saved register context while retaining the furthest native
/// execution time. Task switches, mixed-mode returns and interrupt returns
/// share this policy; restoring registers must not rewind the time base.
pub(crate) fn restore_powerpc_context(cpu: &mut PpcCpu, context: PpcCpu) {
    let time_base = cpu.time_base().max(context.time_base());
    *cpu = context;
    cpu.set_time_base(time_base);
}

/// Exclusive, synchronous preparation of the PowerPC word-argument ABI.
/// No guest execution or mapping change may intervene before installation.
struct PreparedPowerPcCallArguments<'a> {
    cpu: &'a mut PpcCpu,
    memory: &'a mut GuestAddressSpace,
    values: &'a [u32],
    parameter_start: u32,
}

impl<'a> PreparedPowerPcCallArguments<'a> {
    fn prepare(
        cpu: &'a mut PpcCpu,
        memory: &'a mut GuestAddressSpace,
        values: &'a [u32],
    ) -> Option<Self> {
        // Inside Macintosh: PowerPC System Software (1994), pp. 1-45--1-50:
        // parameter words follow the 24-byte linkage area. Keep the existing
        // eight-word minimum so native variable-argument callees can spill r3--r10.
        let parameter_start = cpu.gpr[1].checked_add(24)?;
        let parameter_len = u32::try_from(values.len().max(8)).ok()?.checked_mul(4)?;
        if !memory.preflight_writable_range(parameter_start, parameter_len) {
            return None;
        }
        Some(Self {
            cpu,
            memory,
            values,
            parameter_start,
        })
    }

    fn install(self) {
        for slot in 0..self.values.len().max(8) {
            let value = self.values.get(slot).copied().unwrap_or(0);
            self.memory
                .write_u32_be(self.parameter_start + slot as u32 * 4, value)
                .expect("preflighted native parameter area remains writable");
            if slot < 8 {
                self.cpu.gpr[3 + slot] = value;
            }
        }
    }
}

pub(crate) fn install_powerpc_call_arguments(
    cpu: &mut PpcCpu,
    memory: &mut GuestAddressSpace,
    values: &[u32],
) -> Option<()> {
    PreparedPowerPcCallArguments::prepare(cpu, memory, values)?.install();
    Some(())
}

/// Saved 68K state for one cooperative Thread Manager thread.
///
/// The execution owner preserves the caller-visible register file across
/// classic switches and native task handoffs without involving a host thread.
/// New threads inherit the creator's register world (notably A5) and receive
/// a private guest stack.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CooperativeThread {
    pub(crate) d_regs: [u32; 8],
    pub(crate) a_regs: [u32; 8],
    pub(crate) pc: u32,
    pub(crate) ccr: u8,
    pub(crate) extended: Option<M68kExtendedContext>,
    /// `SetThreadSwitcher` switch-in proc and its `switchProcParam`.
    pub(crate) switch_in: (u32, u32),
    /// `SetThreadSwitcher` switch-out proc and its `switchProcParam`.
    pub(crate) switch_out: (u32, u32),
    /// `SetThreadTerminator` proc and its `terminationProcParam`.
    pub(crate) terminator: (u32, u32),
}

impl CooperativeThread {
    pub(crate) fn capture<C: CpuOps>(cpu: &C) -> CooperativeThread {
        let d_regs = [
            cpu.read_reg(Register::D0),
            cpu.read_reg(Register::D1),
            cpu.read_reg(Register::D2),
            cpu.read_reg(Register::D3),
            cpu.read_reg(Register::D4),
            cpu.read_reg(Register::D5),
            cpu.read_reg(Register::D6),
            cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            cpu.read_reg(Register::A0),
            cpu.read_reg(Register::A1),
            cpu.read_reg(Register::A2),
            cpu.read_reg(Register::A3),
            cpu.read_reg(Register::A4),
            cpu.read_reg(Register::A5),
            cpu.read_reg(Register::A6),
            cpu.read_reg(Register::A7),
        ];
        CooperativeThread {
            d_regs,
            a_regs,
            pc: cpu.read_reg(Register::PC),
            ccr: cpu.get_ccr(),
            extended: cpu.capture_extended_context(),
            switch_in: (0, 0),
            switch_out: (0, 0),
            terminator: (0, 0),
        }
    }

    pub(crate) fn save_registers<C: CpuOps>(&mut self, cpu: &C) {
        *self = Self {
            switch_in: self.switch_in,
            switch_out: self.switch_out,
            terminator: self.terminator,
            ..Self::capture(cpu)
        };
    }

    pub(crate) fn install<C: CpuOps>(&self, cpu: &mut C) {
        if let Some(context) = &self.extended {
            cpu.restore_extended_context(context);
        }
        let d_registers = [
            Register::D0,
            Register::D1,
            Register::D2,
            Register::D3,
            Register::D4,
            Register::D5,
            Register::D6,
            Register::D7,
        ];
        let a_registers = [
            Register::A0,
            Register::A1,
            Register::A2,
            Register::A3,
            Register::A4,
            Register::A5,
            Register::A6,
            Register::A7,
        ];
        for (register, value) in d_registers.into_iter().zip(self.d_regs) {
            cpu.write_reg(register, value);
        }
        for (register, value) in a_registers.into_iter().zip(self.a_regs) {
            cpu.write_reg(register, value);
        }
        cpu.write_reg(Register::PC, self.pc);
        // ABI results may update CCR after the extended snapshot was captured.
        cpu.set_ccr(self.ccr);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M68kCallOrigin {
    return_pc: u32,
    final_sp: u32,
    /// Keep callback storage above SP until execution captures its result.
    parked_sp: Option<u32>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManagerContinuation {
    Cfm(CfmOperation),
    Menu(crate::menu_manager::MenuDefinitionOperation),
}

#[derive(Clone, Debug)]
struct GuestCallFrame {
    target: GuestCallTarget,
    origin: GuestCallOrigin,
    native_scratch: Option<u32>,
    operation: Option<ManagerContinuation>,
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
                    parked_sp: None,
                    result,
                }),
                native_scratch: None,
                operation: None,
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
                    parked_sp: None,
                    result,
                }),
                native_scratch: None,
                operation: None,
                m68k_execution: None,
                powerpc_execution: Some(PowerPcExecution {
                    arguments,
                    return_pc: None,
                    completed: None,
                }),
            }),
            (
                GuestIsa::PowerPc,
                GuestCallArguments::None | GuestCallArguments::PowerPc(_),
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
                native_scratch: None,
                operation: None,
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
                native_scratch: None,
                operation: None,
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
            && self.native_scratch == other.native_scratch
            && self.operation == other.operation
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

/// Full native engine state is retained, including the CPU's private import
/// continuations. The active CPU remains with its engine between task switches.
#[derive(Clone, Debug)]
pub(crate) struct NativeThreadContext {
    pub(crate) cpu: Box<PpcCpu>,
}

/// Process-owned storage provenance is independent of a task's current ISA.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ThreadStorage {
    pub(crate) result_destination: u32,
    pub(crate) stack_base: u32,
    pub(crate) stack_limit: u32,
    /// Managed pointers are released through the process Memory Manager;
    /// reserved guest stacks use the classic allocator when not recycled.
    pub(crate) managed_pointer: bool,
}

#[derive(Clone, Debug)]
enum TaskResumeContext {
    Classic(CooperativeThread),
    Native(Box<PpcCpu>),
}

impl TaskResumeContext {
    fn stack_pointer(&self) -> u32 {
        match self {
            Self::Classic(context) => context.a_regs[7],
            Self::Native(cpu) => cpu.gpr[1],
        }
    }

    fn isa(&self) -> GuestIsa {
        match self {
            Self::Classic(_) => GuestIsa::M68k,
            Self::Native(_) => GuestIsa::PowerPc,
        }
    }
}

/// Identity of one retained Menu Manager operation, independent of callback depth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuOperationId(u64);

/// Caller return placement belongs to execution, not to menu sizing policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuBarCallOrigin {
    M68k {
        stack_pointer: u32,
        return_address: u32,
    },
    PowerPc {
        return_address: u32,
    },
}

impl MenuBarCallOrigin {
    fn isa(self) -> GuestIsa {
        match self {
            Self::M68k { .. } => GuestIsa::M68k,
            Self::PowerPc { .. } => GuestIsa::PowerPc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuBarBuildResume {
    Size(u32),
    Waiting,
    Complete {
        result: u32,
        origin: MenuBarCallOrigin,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MenuOperation {
    Build {
        origin: MenuBarCallOrigin,
        build: crate::menu_manager::MenuBarBuild<u32>,
    },
    Tracking(Box<MenuTrackingContext>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MenuCall {
    id: MenuOperationId,
    task: ExecutionTaskId,
    parent: Option<CallId>,
    operation: MenuOperation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MenuCalls {
    calls: Vec<MenuCall>,
    next_id: u64,
}

/// The original caller's return boundary, independent of menu presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuTrackingOrigin {
    M68k {
        stack_pointer: u32,
        return_address: u32,
    },
    PowerPc {
        stack_pointer: u32,
        return_address: u32,
    },
}

impl MenuTrackingOrigin {
    pub(crate) fn isa(self) -> GuestIsa {
        match self {
            Self::M68k { .. } => GuestIsa::M68k,
            Self::PowerPc { .. } => GuestIsa::PowerPc,
        }
    }
    pub(crate) fn return_address(self) -> u32 {
        match self {
            Self::M68k { return_address, .. } | Self::PowerPc { return_address, .. } => {
                return_address
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuTrackingCall {
    pub(crate) request: crate::menu_manager::MenuTrackingRequest,
    pub(crate) origin: MenuTrackingOrigin,
}

impl MenuTrackingCall {
    pub(crate) fn popup_request(self) -> Option<crate::menu_manager::PopupMenuRequest> {
        match self.request {
            crate::menu_manager::MenuTrackingRequest::PopUp(request) => Some(request),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MenuTrackingContext {
    pub(crate) tracking: Option<crate::menu_manager::ProcessMenuTrackingState>,
    pub(crate) definition: Option<crate::menu_manager::MenuDefinitionTracking>,
    pub(crate) call: Option<MenuTrackingCall>,
    pub(crate) native_port: Option<(u32, u32)>,
    pub(crate) classic_port: Option<crate::trap::dispatch::PortStateSnapshot>,
}

impl MenuTrackingContext {
    pub(crate) fn caller_isa(&self) -> Option<GuestIsa> {
        self.call.map(|call| call.origin.isa())
    }
    pub(crate) fn native_menu(&self) -> Option<MenuTrackingCall> {
        self.call.filter(|call| {
            call.origin.isa() == GuestIsa::PowerPc
                && matches!(
                    call.request,
                    crate::menu_manager::MenuTrackingRequest::MenuSelect { .. }
                )
        })
    }
    pub(crate) fn native_popup(&self) -> Option<MenuTrackingCall> {
        self.call
            .filter(|call| call.origin.isa() == GuestIsa::PowerPc && call.popup_request().is_some())
    }
    pub(crate) fn clear_native_menu(&mut self) {
        if self.native_menu().is_some() {
            self.call = None;
        }
    }
    pub(crate) fn clear_native_popup(&mut self) {
        if self.native_popup().is_some() {
            self.call = None;
        }
    }
    pub(crate) fn classic_stack(&self) -> u32 {
        match self.call.map(|call| call.origin) {
            Some(MenuTrackingOrigin::M68k { stack_pointer, .. }) => stack_pointer,
            _ => panic!("classic menu call must be prepared before tracking"),
        }
    }
    fn is_idle(&self) -> bool {
        self.tracking.is_none()
            && self.definition.is_none()
            && self.native_port.is_none()
            && self.classic_port.is_none()
    }
}

/// A serialized view of execution-owned menu roots. Panes and return state
/// stay with their root while a nested guest call runs. It uses the existing
/// process-value access contract; no reference spans guest execution.
#[derive(Debug)]
pub(crate) struct SharedMenuTracking {
    calls: SharedProcessValue<MenuCalls>,
    execution: SharedGuestCallStack,
    empty: MenuTrackingContext,
}

/// One synchronous ABI entry owns cleanup of an operation that became idle.
/// The handle borrows no state while the adapter prepares guest execution.
pub(crate) struct MenuTrackingEntry {
    tracking: SharedMenuTracking,
    id: MenuOperationId,
}
impl Drop for MenuTrackingEntry {
    fn drop(&mut self) {
        self.tracking.finish_if_idle(self.id);
    }
}

impl Default for SharedMenuTracking {
    fn default() -> Self {
        SharedGuestCallStack::default().menu_tracking_view()
    }
}
impl Clone for SharedMenuTracking {
    fn clone(&self) -> Self {
        self.execution.clone().menu_tracking_view()
    }
}
impl PartialEq for SharedMenuTracking {
    fn eq(&self, other: &Self) -> bool {
        self.calls == other.calls
    }
}
impl Eq for SharedMenuTracking {}
impl PartialEq<Option<crate::menu_manager::ProcessMenuTrackingState>> for SharedMenuTracking {
    fn eq(&self, other: &Option<crate::menu_manager::ProcessMenuTrackingState>) -> bool {
        &**self == other
    }
}
impl std::ops::Deref for SharedMenuTracking {
    type Target = Option<crate::menu_manager::ProcessMenuTrackingState>;
    fn deref(&self) -> &Self::Target {
        &self.context().tracking
    }
}
impl std::ops::DerefMut for SharedMenuTracking {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context_mut().tracking
    }
}
impl SharedMenuTracking {
    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        self.calls.ptr_eq(&other.calls)
    }
    pub(crate) fn bind_execution(&mut self, calls: &SharedGuestCallStack) {
        let view = calls.menu_tracking_view();
        if self.calls.ptr_eq(&view.calls) {
            self.execution = calls.shared_handle();
            return;
        }
        // Derived adapter clones hold detached but equal snapshots of their
        // view and execution owner. Reconnect that view before execution;
        // adopting conflicting populated owners remains forbidden.
        if self.calls == view.calls {
            *self = view;
        } else {
            self.attach_to(&view);
        }
    }
    fn active_index(&self) -> Option<usize> {
        let task = self.execution.current_task();
        self.calls.calls.iter().rposition(|call| {
            call.task == task && matches!(call.operation, MenuOperation::Tracking(_))
        })
    }
    pub(crate) fn context(&self) -> &MenuTrackingContext {
        self.active_index()
            .and_then(|index| match &self.calls.calls[index].operation {
                MenuOperation::Tracking(context) => Some(&**context),
                _ => None,
            })
            .unwrap_or(&self.empty)
    }
    pub(crate) fn existing_context_mut(&mut self) -> Option<&mut MenuTrackingContext> {
        let index = self.active_index()?;
        match &mut self.calls.calls[index].operation {
            MenuOperation::Tracking(context) => Some(context),
            _ => None,
        }
    }
    pub(crate) fn as_mut(&mut self) -> Option<&mut crate::menu_manager::ProcessMenuTrackingState> {
        self.existing_context_mut()?.tracking.as_mut()
    }
    pub(crate) fn take(&mut self) -> Option<crate::menu_manager::ProcessMenuTrackingState> {
        self.existing_context_mut()?.tracking.take()
    }
    pub(crate) fn context_mut(&mut self) -> &mut MenuTrackingContext {
        let index = match self.active_index() {
            Some(index) => index,
            None => {
                self.begin();
                self.active_index().unwrap()
            }
        };
        match &mut self.calls.calls[index].operation {
            MenuOperation::Tracking(context) => context,
            _ => unreachable!(),
        }
    }
    pub(crate) fn enter_new_call(&mut self, call: MenuTrackingCall) -> MenuTrackingEntry {
        let task = self.execution.current_task();
        let parent = self
            .execution
            .0
            .borrow()
            .kernel
            .peek(task)
            .map(|call| call.call_id());
        let id = self.push_tracking(task, parent);
        self.context_mut().call = Some(call);
        self.scope(id)
    }

    pub(crate) fn ready_call(&self, isa: GuestIsa) -> Option<MenuTrackingCall> {
        if !self.execution.current_task_is_running() {
            return None;
        }
        let task = self.execution.current_task();
        let parent = self
            .execution
            .0
            .borrow()
            .kernel
            .peek(task)
            .map(|call| call.call_id());
        let root = &self.calls.calls[self.active_index()?];
        if root.parent != parent {
            return None;
        }
        let MenuOperation::Tracking(context) = &root.operation else {
            return None;
        };
        context
            .call
            .filter(|call| call.origin.isa() == isa && !context.is_idle())
    }

    pub(crate) fn resume_call(
        &mut self,
        isa: GuestIsa,
    ) -> Option<(MenuTrackingCall, MenuTrackingEntry)> {
        let call = self.ready_call(isa)?;
        let id = self.calls.calls[self.active_index()?].id;
        Some((call, self.scope(id)))
    }

    fn scope(&self, id: MenuOperationId) -> MenuTrackingEntry {
        MenuTrackingEntry {
            id,
            tracking: Self {
                calls: self.calls.shared_handle(),
                execution: self.execution.shared_handle(),
                empty: MenuTrackingContext::default(),
            },
        }
    }

    pub(crate) fn enter_call(&mut self, call: MenuTrackingCall) -> MenuTrackingEntry {
        let entry = self.enter();
        self.context_mut().call.get_or_insert(call);
        entry
    }
    pub(crate) fn enter(&mut self) -> MenuTrackingEntry {
        let id = self.begin();
        self.scope(id)
    }

    pub(crate) fn entry_id(&self) -> Option<MenuOperationId> {
        let task = self.execution.current_task();
        let parent = self
            .execution
            .0
            .borrow()
            .kernel
            .peek(task)
            .map(|call| call.call_id());
        let call = &self.calls.calls[self.active_index()?];
        (call.parent == parent).then_some(call.id)
    }
    pub(crate) fn bind_completion(
        &mut self,
        id: MenuOperationId,
        invocation: crate::menu_manager::MenuDefinitionInvocation,
        completion: crate::menu_manager::MenuDefinitionCompletion,
    ) {
        let task = self.execution.current_task();
        let Some(call) = self
            .calls
            .calls
            .iter_mut()
            .find(|call| call.id == id && call.task == task)
        else {
            return;
        };
        let MenuOperation::Tracking(context) = &mut call.operation else {
            return;
        };
        let definition = context
            .tracking
            .as_mut()
            .and_then(|tracking| tracking.active_definition_mut())
            .or(context.definition.as_mut());
        if let Some(definition) = definition {
            definition.bind_completion(invocation, completion);
        }
    }
    pub(crate) fn begin(&mut self) -> MenuOperationId {
        let task = self.execution.current_task();
        let parent = self
            .execution
            .0
            .borrow()
            .kernel
            .peek(task)
            .map(|call| call.call_id());
        // Cancellation can empty a root outside its original ABI entry.
        // A subsequent entry starts a fresh operation and cannot inherit its caller.
        self.calls.calls.retain(|call| {
            call.task != task || call.parent != parent
                || !matches!(&call.operation, MenuOperation::Tracking(context) if context.is_idle())
        });
        if let Some(call) = self.calls.calls.iter().rev().find(|call| {
            call.task == task
                && call.parent == parent
                && matches!(call.operation, MenuOperation::Tracking(_))
        }) {
            return call.id;
        }
        self.push_tracking(task, parent)
    }
    fn push_tracking(&mut self, task: ExecutionTaskId, parent: Option<CallId>) -> MenuOperationId {
        let id = MenuOperationId(self.calls.next_id);
        self.calls.next_id = self
            .calls
            .next_id
            .checked_add(1)
            .expect("menu operation identity exhausted");
        self.calls.calls.push(MenuCall {
            id,
            task,
            parent,
            operation: MenuOperation::Tracking(Box::default()),
        });
        id
    }
    pub(crate) fn finish_if_idle(&mut self, id: MenuOperationId) {
        self.calls.calls.retain(|call| {
            call.id != id
                || !matches!(&call.operation, MenuOperation::Tracking(context) if context.is_idle())
        });
    }
    pub(crate) fn attach_to(&mut self, other: &Self) {
        assert!(
            self.calls.ptr_eq(&other.calls)
                || self.calls.calls.is_empty()
                || other.calls.calls.is_empty(),
            "cannot attach two active Menu Manager continuations"
        );
        self.calls
            .attach_to(&other.calls, |calls| calls.calls.is_empty());
        self.execution = other.execution.shared_handle();
    }
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<crate::menu_manager::ProcessMenuTrackingState> {
        (**self).clone()
    }
}

#[derive(Debug)]
struct ExecutionTaskCalls {
    /// Authoritative task/order/phase state. The frame map below is only the
    /// temporary CPU-adapter projection keyed by this store's CallId.
    kernel: ContinuationStore,
    frames: HashMap<CallId, GuestCallFrame>,
    /// Manager roots outlive each individual guest callback and retain the
    /// invoking task, enclosing call and original ABI return placement.
    menu_calls: SharedProcessValue<MenuCalls>,
    powerpc_contexts: ExecutionContextBank<Box<PpcCpu>>,
    m68k_contexts: Rc<RefCell<ExecutionContextBank<M68kCpu>>>,
    cooperative_contexts: ExecutionTaskContextBank<CooperativeThread>,
    native_threads: ExecutionTaskContextBank<NativeThreadContext>,
    thread_storage: ExecutionTaskContextBank<ThreadStorage>,
    thread_pool: Vec<(GuestIsa, ThreadStorage)>,
    native_cpu_task: Option<ExecutionTaskId>,
    handoff: Option<(ExecutionTaskId, TaskResumeContext)>,
}

impl Clone for ExecutionTaskCalls {
    fn clone(&self) -> Self {
        assert!(
            self.m68k_contexts.borrow().is_empty(),
            "cannot clone an execution owner while a non-cloneable 68K engine is parked"
        );
        Self {
            kernel: self.kernel.clone(),
            frames: self.frames.clone(),
            menu_calls: self.menu_calls.clone(),
            powerpc_contexts: self.powerpc_contexts.clone(),
            m68k_contexts: Rc::new(RefCell::new(ExecutionContextBank::default())),
            cooperative_contexts: self.cooperative_contexts.clone(),
            native_threads: self.native_threads.clone(),
            thread_storage: self.thread_storage.clone(),
            thread_pool: self.thread_pool.clone(),
            native_cpu_task: self.native_cpu_task,
            handoff: self.handoff.clone(),
        }
    }
}

impl PartialEq for ExecutionTaskCalls {
    fn eq(&self, other: &Self) -> bool {
        self.kernel == other.kernel
            && self.frames == other.frames
            && self.menu_calls == other.menu_calls
            && self.powerpc_contexts.same_slots(&other.powerpc_contexts)
            && self
                .m68k_contexts
                .borrow()
                .same_slots(&other.m68k_contexts.borrow())
            && self.cooperative_contexts == other.cooperative_contexts
            && self.native_threads.same_tasks(&other.native_threads)
            && self.thread_storage == other.thread_storage
            && self.thread_pool == other.thread_pool
            && self.native_cpu_task == other.native_cpu_task
            && self
                .handoff
                .as_ref()
                .map(|(task, context)| (*task, context.isa()))
                == other
                    .handoff
                    .as_ref()
                    .map(|(task, context)| (*task, context.isa()))
    }
}

impl Eq for ExecutionTaskCalls {}

impl Default for ExecutionTaskCalls {
    fn default() -> Self {
        Self {
            kernel: ContinuationStore::default(),
            frames: HashMap::new(),
            menu_calls: SharedProcessValue::default(),
            powerpc_contexts: ExecutionContextBank::default(),
            m68k_contexts: Rc::new(RefCell::new(ExecutionContextBank::default())),
            cooperative_contexts: ExecutionTaskContextBank::default(),
            native_threads: ExecutionTaskContextBank::default(),
            thread_storage: ExecutionTaskContextBank::default(),
            thread_pool: Vec::new(),
            native_cpu_task: None,
            handoff: None,
        }
    }
}

impl ExecutionTaskCalls {
    fn saved_context(&self, task: ExecutionTaskId) -> Option<TaskResumeContext> {
        let isa = self
            .kernel
            .peek(task)
            .and_then(|call| {
                let frame = self.frames.get(&call.call_id())?;
                Some(
                    if call.phase() == crate::execution_kernel::ContinuationPhase::Active {
                        frame.target.isa
                    } else {
                        match frame.origin {
                            GuestCallOrigin::M68k(_) => GuestIsa::M68k,
                            GuestCallOrigin::PowerPc(_) => GuestIsa::PowerPc,
                        }
                    },
                )
            })
            .unwrap_or_else(|| {
                if self.kernel.task_entry_isa(task) == Some(GuestIsa::PowerPc)
                    || (self.native_threads.get(task).is_some()
                        && self.cooperative_contexts.get(task).is_none())
                {
                    GuestIsa::PowerPc
                } else {
                    GuestIsa::M68k
                }
            });
        match isa {
            GuestIsa::M68k => self
                .cooperative_contexts
                .get(task)
                .cloned()
                .map(TaskResumeContext::Classic),
            GuestIsa::PowerPc => self
                .native_threads
                .get(task)
                .map(|saved| TaskResumeContext::Native(saved.cpu.clone())),
        }
    }

    fn change_thread_state(
        &mut self,
        thread: u32,
        new_state: u16,
        suggested: u32,
        end_critical: bool,
        commit: impl FnOnce() -> bool,
    ) -> Result<Option<(ExecutionTaskId, TaskResumeContext)>, i16> {
        use crate::thread_manager::{THREAD_NOT_FOUND_ERR, THREAD_PROTOCOL_ERR};
        let task = if thread <= 1 {
            self.kernel.current_task()
        } else {
            ExecutionTaskId::from_thread_id(thread)
        };
        if self.kernel.scheduling_state(task).is_none() {
            return Err(THREAD_NOT_FOUND_ERR);
        }
        let requested = match new_state {
            0 => ExecutionTaskState::Ready,
            1 => ExecutionTaskState::Stopped,
            2 => ExecutionTaskState::Running,
            _ => return Err(THREAD_PROTOCOL_ERR),
        };
        if self.handoff.is_some() {
            return Err(THREAD_PROTOCOL_ERR);
        }
        // Capture only the selected candidate before the kernel's atomic commit.
        let candidate = self
            .kernel
            .next_ready_task_after_critical(
                (suggested > 1).then(|| ExecutionTaskId::from_thread_id(suggested)),
                end_critical,
            )
            .and_then(|next| self.saved_context(next).map(|context| (next, context)));
        let mut successor = None;
        self.kernel
            .change_thread_state_with(
                task,
                requested,
                (suggested > 1).then(|| ExecutionTaskId::from_thread_id(suggested)),
                end_critical,
                |next| {
                    if let Some(next) = next {
                        let Some((task, context)) = candidate else {
                            return false;
                        };
                        if task != next {
                            return false;
                        }
                        successor = Some((next, context));
                    }
                    commit()
                },
            )
            .ok_or(THREAD_PROTOCOL_ERR)?;
        Ok(successor)
    }

    fn create_thread(
        &mut self,
        context: TaskResumeContext,
        storage: ThreadStorage,
        suspended: bool,
        commit: impl FnOnce(ExecutionTaskId) -> bool,
    ) -> Option<ExecutionTaskId> {
        let task = self.kernel.create_task_with(commit).ok()?;
        self.kernel.bind_task_entry_isa(task, context.isa());
        match context {
            TaskResumeContext::Classic(context) => {
                self.cooperative_contexts.insert(task, context);
            }
            TaskResumeContext::Native(cpu) => {
                self.native_threads
                    .insert(task, NativeThreadContext { cpu });
            }
        }
        self.thread_storage.insert(task, storage);
        if !suspended {
            assert!(self
                .kernel
                .set_scheduling_state(task, ExecutionTaskState::Ready));
        }
        Some(task)
    }

    fn retire_thread(
        &mut self,
        task: ExecutionTaskId,
        successor: Option<ExecutionTaskId>,
        recycle: bool,
        commit: impl FnOnce(&ThreadStorage) -> bool,
    ) -> Option<(ThreadStorage, Option<(ExecutionTaskId, TaskResumeContext)>)> {
        // Thread Manager (1999), p. 60: DisposeThread may never retire
        // the application thread, even while a worker is running.
        if task == ExecutionTaskId::APPLICATION {
            return None;
        }
        if self.handoff.is_some() {
            return None;
        }
        if self.cooperative_contexts.get(task).is_none() && self.native_threads.get(task).is_none()
        {
            return None;
        }
        let storage = self.thread_storage.get(task).copied().unwrap_or_default();
        let pooled_isa = if recycle && storage.stack_base != 0 {
            Some(self.kernel.task_entry_isa(task)?)
        } else {
            None
        };
        let next = match successor {
            Some(next) => Some((next, self.saved_context(next)?)),
            None => None,
        };
        self.kernel
            .retire_task_with(task, successor, || commit(&storage))
            .ok()?;
        self.cooperative_contexts.remove(task);
        self.native_threads.remove(task);
        self.thread_storage.remove(task);
        self.menu_calls.calls.retain(|call| call.task != task);
        if let Some(isa) = pooled_isa {
            self.thread_pool.push((
                isa,
                ThreadStorage {
                    result_destination: 0,
                    ..storage
                },
            ));
        }
        if self.native_cpu_task == Some(task) {
            self.native_cpu_task = None;
        }
        Some((storage, next))
    }

    fn save_native_cpu(&mut self, task: ExecutionTaskId, cpu: &PpcCpu) {
        if self.kernel.scheduling_state(task).is_none() {
            return;
        }
        let mut context = self
            .native_threads
            .get(task)
            .cloned()
            .unwrap_or(NativeThreadContext {
                cpu: Box::new(cpu.clone()),
            });
        context.cpu = Box::new(cpu.clone());
        self.native_threads.insert(task, context);
    }

    fn install_native_successor(
        &mut self,
        task: ExecutionTaskId,
        context: TaskResumeContext,
        cpu: &mut PpcCpu,
    ) {
        match context {
            TaskResumeContext::Classic(context) => {
                self.handoff = Some((task, TaskResumeContext::Classic(context)));
            }
            TaskResumeContext::Native(next) => {
                restore_powerpc_context(cpu, *next);
                self.native_cpu_task = Some(task);
            }
        }
    }

    fn is_pristine(&self) -> bool {
        self.kernel.is_pristine()
            && self.menu_calls.calls.is_empty()
            && self.menu_calls.next_id == 0
            && self.m68k_contexts.borrow().is_empty()
            && self.cooperative_contexts.is_empty()
            && self.native_threads.is_empty()
            && self.thread_storage.is_empty()
            && self.thread_pool.is_empty()
            && self.native_cpu_task.is_none()
            && self.handoff.is_none()
    }
}

/// Task-indexed guest-procedure continuation stacks for one process.
///
/// Both CPU adapters share this owner, but every Thread Manager task has an
/// independent LIFO stack. Switching tasks changes which stack subsequent
/// Mixed Mode operations address; it cannot expose another task's suspended
/// call. Ordinary `Clone` creates an independent process snapshot only when
/// no non-cloneable 68K engine is parked; `shared_handle` preserves custody.
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
        let mut pending = std::mem::take(&mut *self.0.borrow_mut());
        self.0 = Rc::clone(&process_calls.0);
        if !pending.is_pristine() {
            pending
                .menu_calls
                .attach_to(&self.0.borrow().menu_calls, |calls| calls.calls.is_empty());
            *self.0.borrow_mut() = pending;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        let tasks = self.0.borrow();
        tasks.kernel.is_empty() && tasks.menu_calls.calls.iter().all(|call| matches!(&call.operation, MenuOperation::Tracking(context) if context.is_idle()))
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
    /// Borrowing this process-owned bank never spans guest execution. Each
    /// transition selects the bank from its execution owner instead of keeping
    /// a separate bank handle on the CPU adapter.
    fn classic_contexts(&self) -> Rc<RefCell<ExecutionContextBank<M68kCpu>>> {
        Rc::clone(&self.0.borrow().m68k_contexts)
    }

    pub(crate) fn has_parked_m68k_contexts(&self) -> bool {
        !self.classic_contexts().borrow().is_empty()
    }

    #[cfg(test)]
    pub(crate) fn m68k_context_bank(&self) -> Rc<RefCell<ExecutionContextBank<M68kCpu>>> {
        self.classic_contexts()
    }

    /// The enclosing guest call distinguishes a nested Toolbox entry from
    /// resumption after its own MDEF has retired. Another task cannot observe
    /// or consume this operation, even when it uses the same import or trap.
    pub(crate) fn menu_tracking_view(&self) -> SharedMenuTracking {
        let tasks = self.0.borrow();
        SharedMenuTracking {
            calls: tasks.menu_calls.shared_handle(),
            execution: self.shared_handle(),
            empty: MenuTrackingContext::default(),
        }
    }

    pub(crate) fn menu_bar_build(&self) -> Option<MenuOperationId> {
        let tasks = self.0.borrow();
        let task = tasks.kernel.current_task();
        let parent = tasks.kernel.peek(task).map(|call| call.call_id());
        tasks
            .menu_calls
            .calls
            .iter()
            .rev()
            .find(|call| {
                call.task == task
                    && call.parent == parent
                    && matches!(call.operation, MenuOperation::Build { .. })
            })
            .map(|call| call.id)
    }
    pub(crate) fn ready_menu_bar_build(&self, isa: GuestIsa) -> Option<MenuOperationId> {
        let id = self.menu_bar_build()?;
        let tasks = self.0.borrow();
        tasks
            .menu_calls
            .calls
            .iter()
            .find(|call| call.id == id)
            .and_then(|call| match &call.operation {
                MenuOperation::Build { origin, build }
                    if origin.isa() == isa && build.callback_ready() =>
                {
                    Some(id)
                }
                _ => None,
            })
    }
    #[cfg(test)]
    pub(crate) fn has_menu_bar_builds(&self) -> bool {
        let tasks = self.0.borrow();
        tasks.menu_calls.calls.iter().any(|call| {
            call.task == tasks.kernel.current_task()
                && matches!(call.operation, MenuOperation::Build { .. })
        })
    }
    pub(crate) fn begin_menu_bar_build(
        &self,
        build: crate::menu_manager::MenuBarBuild<u32>,
        origin: MenuBarCallOrigin,
    ) -> Option<MenuOperationId> {
        if self.menu_bar_build().is_some() {
            return None;
        }
        let mut tasks = self.0.borrow_mut();
        let task = tasks.kernel.current_task();
        let parent = tasks.kernel.peek(task).map(|call| call.call_id());
        let next = tasks.menu_calls.next_id.checked_add(1)?;
        let id = MenuOperationId(tasks.menu_calls.next_id);
        tasks.menu_calls.next_id = next;
        tasks.menu_calls.calls.push(MenuCall {
            id,
            task,
            parent,
            operation: MenuOperation::Build { origin, build },
        });
        Some(id)
    }
    pub(crate) fn advance_menu_bar_build(&self, isa: GuestIsa) -> Option<MenuBarBuildResume> {
        let id = self.menu_bar_build()?;
        let mut tasks = self.0.borrow_mut();
        let index = tasks
            .menu_calls
            .calls
            .iter()
            .position(|call| call.id == id)?;
        let MenuOperation::Build { origin, build } = &mut tasks.menu_calls.calls[index].operation
        else {
            return None;
        };
        if origin.isa() != isa {
            return None;
        }
        match build.next_step() {
            Some(crate::menu_manager::MenuBarBuildStep::Size(handle)) => {
                Some(MenuBarBuildResume::Size(handle))
            }
            Some(crate::menu_manager::MenuBarBuildStep::Complete(result)) => {
                let origin = *origin;
                tasks.menu_calls.calls.remove(index);
                Some(MenuBarBuildResume::Complete { result, origin })
            }
            None => Some(MenuBarBuildResume::Waiting),
        }
    }
    pub(crate) fn bind_menu_bar_build_completion(
        &self,
        id: MenuOperationId,
        handle: u32,
        completion: crate::menu_manager::MenuDefinitionCompletion,
    ) {
        let mut tasks = self.0.borrow_mut();
        let task = tasks.kernel.current_task();
        if let Some(call) = tasks
            .menu_calls
            .calls
            .iter_mut()
            .find(|call| call.id == id && call.task == task)
        {
            if let MenuOperation::Build { build, .. } = &mut call.operation {
                build.bind_completion(handle, completion);
            }
        }
    }

    pub(crate) fn current_task(&self) -> ExecutionTaskId {
        self.0.borrow().kernel.current_task()
    }

    /// Fixtures may import an explicit identity into the owner's namespace.
    #[cfg(test)]
    pub(crate) fn register_task(&self, task: ExecutionTaskId) -> bool {
        self.0.borrow().kernel.register_task(task).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn switch_to_task(&self, task: ExecutionTaskId) -> bool {
        self.0.borrow().kernel.switch_to_task(task).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn create_task(&self) -> Option<ExecutionTaskId> {
        self.0.borrow().kernel.create_task().ok()
    }

    pub(crate) fn bind_task_entry_isa(&self, task: ExecutionTaskId, isa: GuestIsa) -> bool {
        self.0.borrow().kernel.bind_task_entry_isa(task, isa)
    }

    /// Select work using task ownership, never the numeric application ID.
    /// Availability is a current observation; no engine or manager is borrowed.
    pub(crate) fn execution_route(&self, native: NativeAvailability) -> ExecutionRoute {
        let task = self.current_task();
        if self.scheduling_state(task) != Some(ExecutionTaskState::Running) {
            return ExecutionRoute::Blocked;
        }
        let entry = self.0.borrow().kernel.task_entry_isa(task);
        let native_call = self.has_powerpc_from_m68k();
        if entry == Some(GuestIsa::PowerPc) || native_call || self.has_m68k_execution() {
            if native.application {
                ExecutionRoute::NativeApplication
            } else if native.companion {
                ExecutionRoute::NativeCompanion
            } else if native.staged_companion && native_call {
                ExecutionRoute::PrepareCompanion
            } else {
                ExecutionRoute::Blocked
            }
        } else {
            ExecutionRoute::Classic
        }
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
        tasks.native_threads.remove(task);
        tasks.thread_storage.remove(task);
        tasks.menu_calls.calls.retain(|call| call.task != task);
        true
    }

    /// Commit result delivery and retirement while the execution owner keeps
    /// both task identities and their adapter snapshots stable.
    pub(crate) fn retire_cooperative_context(
        &self,
        task: ExecutionTaskId,
        successor: Option<ExecutionTaskId>,
        recycle: bool,
        commit: impl FnOnce(&ThreadStorage) -> bool,
    ) -> Option<(ThreadStorage, Option<CooperativeThread>)> {
        let mut tasks = self.0.borrow_mut();
        let (finished, next) = tasks.retire_thread(task, successor, recycle, commit)?;
        let classic = match next {
            Some((_, TaskResumeContext::Classic(context))) => Some(context),
            Some((next, context)) => {
                tasks.handoff = Some((next, context));
                None
            }
            None => None,
        };
        Some((finished, classic))
    }

    pub(crate) fn thread_storage(&self, task: ExecutionTaskId) -> Option<ThreadStorage> {
        let tasks = self.0.borrow();
        tasks.kernel.scheduling_state(task)?;
        Some(tasks.thread_storage.get(task).copied().unwrap_or_default())
    }

    /// Observe the thread's original stack without exposing a parked engine.
    /// Classic reentry uses a bridge stack; the oldest parked classic caller
    /// remains the owner of the thread allocation. Native reentry continues
    /// on its native stack, so prefer its live or suspended active context.
    pub(crate) fn thread_stack_pointer(
        &self,
        task: ExecutionTaskId,
        live_isa: GuestIsa,
        live_sp: u32,
    ) -> Option<(GuestIsa, u32)> {
        let tasks = self.0.borrow();
        tasks.kernel.scheduling_state(task)?;
        let calls = tasks.kernel.task_states(task);
        let entry = tasks.kernel.bound_task_entry_isa(task).or_else(|| {
            if task != ExecutionTaskId::APPLICATION {
                return None;
            }
            // Detached legacy adapters may lack launch metadata. Observe the
            // oldest call or a unique saved engine without initializing them
            // as a side effect of an otherwise read-only query.
            if let Some(call) = calls.first() {
                return Some(match tasks.frames.get(&call.call_id())?.origin {
                    GuestCallOrigin::M68k(_) => GuestIsa::M68k,
                    GuestCallOrigin::PowerPc(_) => GuestIsa::PowerPc,
                });
            }
            match (
                tasks.cooperative_contexts.get(task),
                tasks.native_threads.get(task),
            ) {
                (Some(_), None) => Some(GuestIsa::M68k),
                (None, Some(_)) => Some(GuestIsa::PowerPc),
                (None, None) if task == tasks.kernel.current_task() => Some(live_isa),
                _ => None,
            }
        })?;
        if entry == GuestIsa::M68k {
            let bank = tasks.m68k_contexts.borrow();
            for call in &calls {
                if matches!(
                    tasks.frames.get(&call.call_id())?.origin,
                    GuestCallOrigin::M68k(_)
                ) {
                    if let Some(cpu) = bank.get(task, call.call_id()) {
                        return Some((entry, cpu.core.a(7)));
                    }
                    if call.phase() != ContinuationPhase::Pending {
                        return None;
                    }
                }
            }
        }
        if let Some((owner, context)) = &tasks.handoff {
            if *owner == task && context.isa() == entry {
                return Some((entry, context.stack_pointer()));
            }
        }
        if task == tasks.kernel.current_task() && live_isa == entry {
            return Some((entry, live_sp));
        }
        if task != tasks.kernel.current_task() {
            if let Some(context) = tasks.saved_context(task) {
                if context.isa() == entry {
                    return Some((entry, context.stack_pointer()));
                }
            }
        }
        if entry == GuestIsa::PowerPc {
            for call in calls.iter().rev() {
                if matches!(
                    tasks.frames.get(&call.call_id())?.origin,
                    GuestCallOrigin::PowerPc(_)
                ) {
                    if let Some(cpu) = tasks.powerpc_contexts.get(task, call.call_id()) {
                        return Some((entry, cpu.gpr[1]));
                    }
                }
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn set_thread_storage(&self, task: ExecutionTaskId, storage: ThreadStorage) -> bool {
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.scheduling_state(task).is_none() {
            return false;
        }
        tasks.thread_storage.insert(task, storage);
        true
    }

    #[cfg(test)]
    pub(crate) fn take_classic_thread_stack(&self, size: u32) -> Option<(u32, u32)> {
        self.request_classic_thread_stack(size, 2).ok().flatten()
    }

    pub(crate) fn request_classic_thread_stack(
        &self,
        size: u32,
        options: u32,
    ) -> Result<Option<(u32, u32)>, i16> {
        self.request_thread_stack(GuestIsa::M68k, size, options)
            .map(|storage| storage.map(|storage| (storage.stack_base, storage.stack_limit)))
    }

    /// Select storage without publishing a task. Thread Manager (1999),
    /// pp. 48, 57–58: same-ISA pool use, best fit, exact match and -617 refusal.
    pub(crate) fn request_thread_stack(
        &self,
        isa: GuestIsa,
        size: u32,
        options: u32,
    ) -> Result<Option<ThreadStorage>, i16> {
        if options & 2 == 0 {
            return Ok(None);
        }
        let mut tasks = self.0.borrow_mut();
        let index = tasks
            .thread_pool
            .iter()
            .enumerate()
            .filter_map(|(index, (entry_isa, storage))| {
                let available = storage.stack_limit.checked_sub(storage.stack_base)?;
                (*entry_isa == isa
                    && storage.stack_base != 0
                    && available >= size
                    && (options & 16 == 0 || available == size))
                    .then_some((index, available))
            })
            .min_by_key(|&(_, available)| available)
            .map(|(index, _)| index);
        match index {
            Some(index) => Ok(Some(tasks.thread_pool.swap_remove(index).1)),
            None if options & 4 != 0 => Ok(None),
            None => Err(-617),
        }
    }

    pub(crate) fn recycle_thread_stack(&self, isa: GuestIsa, storage: ThreadStorage) {
        self.0.borrow_mut().thread_pool.push((
            isa,
            ThreadStorage {
                result_destination: 0,
                ..storage
            },
        ));
    }

    pub(crate) fn recycle_classic_thread_stack(&self, stack: (u32, u32)) {
        self.recycle_thread_stack(
            GuestIsa::M68k,
            ThreadStorage {
                stack_base: stack.0,
                stack_limit: stack.1,
                ..ThreadStorage::default()
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn classic_thread_pool_count(&self, minimum_size: u32) -> usize {
        self.thread_pool_count(GuestIsa::M68k, minimum_size)
    }

    pub(crate) fn thread_pool_count(&self, isa: GuestIsa, minimum_size: u32) -> usize {
        self.0
            .borrow()
            .thread_pool
            .iter()
            .filter(|(entry_isa, storage)| {
                *entry_isa == isa
                    && storage.stack_limit.saturating_sub(storage.stack_base) >= minimum_size
            })
            .count()
    }

    pub(crate) fn publish_thread_pool(&self, isa: GuestIsa, storage: Vec<ThreadStorage>) {
        self.0
            .borrow_mut()
            .thread_pool
            .extend(storage.into_iter().map(|storage| (isa, storage)));
    }

    pub(crate) fn switch_from_classic(
        &self,
        next: ExecutionTaskId,
    ) -> Option<Option<CooperativeThread>> {
        let mut tasks = self.0.borrow_mut();
        if tasks.handoff.is_some() {
            return None;
        }
        let context = tasks.saved_context(next)?;
        tasks.kernel.switch_to_task(next).ok()?;
        match context {
            TaskResumeContext::Classic(context) => Some(Some(context)),
            context => {
                tasks.handoff = Some((next, context));
                Some(None)
            }
        }
    }

    pub(crate) fn take_classic_task_handoff(&self) -> Option<CooperativeThread> {
        let mut tasks = self.0.borrow_mut();
        let Some((task, TaskResumeContext::Classic(_))) = tasks.handoff.as_ref() else {
            return None;
        };
        if *task != tasks.kernel.current_task() {
            return None;
        }
        match tasks.handoff.take()? {
            (_, TaskResumeContext::Classic(context)) => Some(context),
            _ => unreachable!(),
        }
    }

    pub(crate) fn start_native_engine(&self) {
        let mut tasks = self.0.borrow_mut();
        assert!(
            tasks.handoff.is_none(),
            "cannot launch across a pending task handoff"
        );
        tasks.native_cpu_task = Some(tasks.kernel.current_task());
    }

    /// Select bounds for the installed native engine, which can still belong
    /// to a suspended task while a classic task owns the scheduling cursor.
    pub(crate) fn native_stack_bounds(
        &self,
        application_base: u32,
        application_limit: u32,
    ) -> Option<(u32, u32)> {
        let tasks = self.0.borrow();
        if let Some(task) = tasks.native_cpu_task {
            if task != ExecutionTaskId::APPLICATION
                && tasks.kernel.task_entry_isa(task) == Some(GuestIsa::PowerPc)
            {
                let storage = tasks.thread_storage.get(task)?;
                return Some((storage.stack_base, storage.stack_limit));
            }
        }
        Some((application_base, application_limit))
    }

    pub(crate) fn has_classic_task_handoff(&self) -> bool {
        matches!(
            self.0.borrow().handoff,
            Some((_, TaskResumeContext::Classic(_)))
        )
    }

    /// Preserve the previous native owner before replacing its engine. A
    /// classic task may execute while this native CPU is suspended in a callback.
    pub(crate) fn prepare_native_task(&self, cpu: &mut PpcCpu) -> bool {
        let mut tasks = self.0.borrow_mut();
        let current = tasks.kernel.current_task();
        if tasks.kernel.scheduling_state(current) != Some(ExecutionTaskState::Running) {
            return false;
        }
        if tasks
            .handoff
            .as_ref()
            .is_some_and(|(task, _)| *task != current)
        {
            return false;
        }
        if matches!(tasks.handoff, Some((_, TaskResumeContext::Classic(_)))) {
            return false;
        }
        let pending_native = matches!(tasks.handoff, Some((_, TaskResumeContext::Native(_))));
        let next = if pending_native {
            match tasks.handoff.take().unwrap().1 {
                TaskResumeContext::Native(cpu) => Some(cpu),
                _ => unreachable!(),
            }
        } else if tasks.native_cpu_task != Some(current) {
            tasks
                .native_threads
                .get(current)
                .map(|context| context.cpu.clone())
        } else {
            None
        };
        if tasks.native_cpu_task != Some(current) {
            if let Some(previous) = tasks.native_cpu_task {
                tasks.save_native_cpu(previous, cpu);
            }
        }
        if let Some(next) = next {
            restore_powerpc_context(cpu, *next);
        }
        tasks.native_cpu_task = Some(current);
        true
    }

    pub(crate) fn has_pending_task_handoff(&self) -> bool {
        self.0.borrow().handoff.is_some()
    }

    pub(crate) fn has_live_workers(&self) -> bool {
        self.0.borrow().kernel.has_live_workers()
    }

    pub(crate) fn create_classic_thread(
        &self,
        context: CooperativeThread,
        storage: ThreadStorage,
        suspended: bool,
        commit: impl FnOnce(ExecutionTaskId) -> bool,
    ) -> Option<ExecutionTaskId> {
        self.0.borrow_mut().create_thread(
            TaskResumeContext::Classic(context),
            storage,
            suspended,
            commit,
        )
    }

    pub(crate) fn create_native_thread(
        &self,
        context: NativeThreadContext,
        storage: ThreadStorage,
        suspended: bool,
        commit: impl FnOnce(ExecutionTaskId) -> bool,
    ) -> Option<ExecutionTaskId> {
        self.0.borrow_mut().create_thread(
            TaskResumeContext::Native(context.cpu),
            storage,
            suspended,
            commit,
        )
    }

    /// Scheduling resumes only after a wake operation marks a task ready and
    /// its saved engine context can be installed. A stopped cursor owns no work.
    pub(crate) fn resume_ready_task(&self) -> bool {
        let mut tasks = self.0.borrow_mut();
        if tasks.handoff.is_some() || tasks.kernel.critical_depth() != 0 {
            return false;
        }
        let current = tasks.kernel.current_task();
        let next = match tasks.kernel.scheduling_state(current) {
            Some(ExecutionTaskState::Running) => return false,
            Some(ExecutionTaskState::Ready) => current,
            _ => match tasks.kernel.next_ready_task(None) {
                Some(next) => next,
                None => return false,
            },
        };
        let Some(context) = tasks.saved_context(next) else {
            return false;
        };
        if tasks.kernel.switch_to_task(next).is_err() {
            return false;
        }
        tasks.handoff = Some((next, context));
        true
    }

    pub(crate) fn current_task_is_running(&self) -> bool {
        let tasks = self.0.borrow();
        tasks.kernel.scheduling_state(tasks.kernel.current_task())
            == Some(ExecutionTaskState::Running)
    }

    pub(crate) fn set_native_thread_state(
        &self,
        cpu: &mut PpcCpu,
        thread: u32,
        new_state: u16,
        suggested: u32,
        end_critical: bool,
    ) -> Result<bool, i16> {
        let mut tasks = self.0.borrow_mut();
        let current = tasks.kernel.current_task();
        let successor =
            tasks.change_thread_state(thread, new_state, suggested, end_critical, || true)?;
        if successor.is_none()
            && tasks.kernel.scheduling_state(current) != Some(ExecutionTaskState::Stopped)
        {
            return Ok(false);
        }
        let mut outgoing = cpu.clone();
        outgoing.pc = outgoing.lr;
        outgoing.gpr[3] = 0;
        tasks.save_native_cpu(current, &outgoing);
        *cpu = outgoing;
        tasks.native_cpu_task = Some(current);
        if let Some((next, context)) = successor {
            tasks.install_native_successor(next, context, cpu);
        }
        Ok(true)
    }

    pub(crate) fn set_classic_thread_state(
        &self,
        thread: u32,
        new_state: u16,
        suggested: u32,
        end_critical: bool,
        outgoing: CooperativeThread,
        commit: impl FnOnce() -> bool,
    ) -> Result<bool, i16> {
        let mut tasks = self.0.borrow_mut();
        let current = tasks.kernel.current_task();
        let successor =
            tasks.change_thread_state(thread, new_state, suggested, end_critical, commit)?;
        if successor.is_none()
            && tasks.kernel.scheduling_state(current) != Some(ExecutionTaskState::Stopped)
        {
            return Ok(false);
        }
        tasks.cooperative_contexts.insert(current, outgoing);
        tasks.handoff = successor;
        Ok(true)
    }

    /// Native ABI edge supplies the live CPU; the owner validates the next
    /// snapshot before saving the return and committing the selected task.
    pub(crate) fn yield_native_thread(
        &self,
        cpu: &mut PpcCpu,
        suggested: u32,
    ) -> Result<bool, i16> {
        use crate::thread_manager::THREAD_PROTOCOL_ERR;
        let mut tasks = self.0.borrow_mut();
        if tasks.kernel.critical_depth() != 0 || tasks.handoff.is_some() {
            return Err(THREAD_PROTOCOL_ERR);
        }
        let current = tasks.kernel.current_task();
        let Some(next) = tasks
            .kernel
            .next_ready_task((suggested > 1).then(|| ExecutionTaskId::from_thread_id(suggested)))
        else {
            return Ok(false);
        };
        let next_context = tasks.saved_context(next).ok_or(THREAD_PROTOCOL_ERR)?;
        let mut outgoing = cpu.clone();
        outgoing.pc = outgoing.lr;
        outgoing.gpr[3] = 0;
        tasks
            .kernel
            .switch_to_task(next)
            .map_err(|_| THREAD_PROTOCOL_ERR)?;
        tasks.save_native_cpu(current, &outgoing);
        *cpu = outgoing;
        tasks.native_cpu_task = Some(current);
        tasks.install_native_successor(next, next_context, cpu);
        Ok(true)
    }

    pub(crate) fn retire_native_thread(
        &self,
        task: ExecutionTaskId,
        cpu: &mut PpcCpu,
        recycle: bool,
        commit: impl FnOnce(&ThreadStorage) -> bool,
    ) -> Option<ThreadStorage> {
        let mut tasks = self.0.borrow_mut();
        let successor = if task == tasks.kernel.current_task() {
            Some(tasks.kernel.next_ready_task(None)?)
        } else {
            None
        };
        let (finished, successor) = tasks.retire_thread(task, successor, recycle, commit)?;
        if let Some((next, context)) = successor {
            tasks.install_native_successor(next, context, cpu);
        }
        Some(finished)
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

    #[cfg(test)]
    pub(crate) fn begin_m68k(
        &self,
        target: GuestCallTarget,
        return_pc: u32,
        final_sp: u32,
    ) -> bool {
        self.begin_m68k_with_operation(target, return_pc, final_sp, None, None)
    }

    pub(crate) fn begin_m68k_with_operation(
        &self,
        target: GuestCallTarget,
        return_pc: u32,
        final_sp: u32,
        parked_sp: Option<u32>,
        operation: Option<ManagerContinuation>,
    ) -> bool {
        if target.isa != GuestIsa::M68k {
            return false;
        }
        let Some(id) = self.submit_effect(GuestCallEffect::call_guest(
            GuestCallRequest::for_task(self.current_task(), target),
            GuestCallContinuation::to_m68k(return_pc, final_sp, None),
        )) else {
            return false;
        };
        let mut tasks = self.0.borrow_mut();
        let frame = tasks.frames.get_mut(&id).expect("submitted classic call");
        let GuestCallOrigin::M68k(origin) = &mut frame.origin else {
            unreachable!()
        };
        origin.parked_sp = parked_sp;
        frame.operation = operation;
        true
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

    /// Prepare the emulated 68K interval for a native caller. Activation
    /// retains its complete native context; completion restores that context
    /// before applying the documented return state.
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

    /// Adopt prepared emulated ABI storage into one exact task-owned call.
    pub(crate) fn begin_m68k_operation(
        &self,
        effect: GuestCallEffect,
        scratch: u32,
        operation: ManagerContinuation,
    ) -> bool {
        if effect.request().task != self.current_task()
            || effect.request().target.isa != GuestIsa::M68k
        {
            return false;
        }
        let Some(id) = self.submit_effect(effect) else {
            return false;
        };
        let mut tasks = self.0.borrow_mut();
        let frame = tasks.frames.get_mut(&id).expect("new call owns its frame");
        frame.native_scratch = Some(scratch);
        frame.operation = Some(operation);
        true
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

    #[cfg(test)]
    pub(crate) fn activate_powerpc_from_m68k(
        &self,
        cpu: &mut PpcCpu,
        return_pc: u32,
    ) -> Option<PendingPowerPcExecution> {
        self.activate_powerpc_transition(cpu, None, return_pc)
    }

    pub(crate) fn activate_powerpc_with_classic_caller(
        &self,
        cpu: &mut PpcCpu,
        caller: &mut M68kCpu,
        return_pc: u32,
    ) -> Option<PendingPowerPcExecution> {
        self.activate_powerpc_transition(cpu, Some(caller), return_pc)
    }

    fn activate_powerpc_transition(
        &self,
        cpu: &mut PpcCpu,
        caller: Option<&mut M68kCpu>,
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
        if let Some(caller) = caller {
            let bank = Rc::clone(&tasks.m68k_contexts);
            tasks
                .powerpc_contexts
                .park_pair_while_activating(
                    &mut bank.borrow_mut(),
                    &kernel,
                    task,
                    call_id,
                    Box::new(cpu.clone()),
                    caller,
                )
                .ok()?;
        } else {
            tasks
                .powerpc_contexts
                .park_while_activating(&kernel, task, call_id, Box::new(cpu.clone()))
                .ok()?;
        }
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
        restore_powerpc_context(cpu, *parked_cpu);
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
    pub(crate) fn commit_m68k_resume(
        &self,
        apply: impl FnOnce(M68kResume, Option<&mut M68kCpu>) -> bool,
    ) -> Option<Option<M68kCpu>> {
        let bank = self.classic_contexts();
        let mut bank = bank.borrow_mut();
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
        self.activate_m68k_in_bank(
            &mut ExecutionContextBank::<()>::default(),
            &mut (),
            None,
            None,
        )
    }

    pub(crate) fn activate_m68k_parking(
        &self,
        installed: &mut M68kCpu,
        native: &PpcCpu,
    ) -> Option<PendingM68kExecution> {
        let caller = self.suspended_m68k_context_owner().map(|(_, call)| call);
        let bank = self.classic_contexts();
        let pending =
            self.activate_m68k_in_bank(&mut bank.borrow_mut(), installed, caller, Some(native));
        pending
    }

    fn activate_m68k_in_bank<T: Default>(
        &self,
        bank: &mut ExecutionContextBank<T>,
        installed: &mut T,
        caller: Option<CallId>,
        native: Option<&PpcCpu>,
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
        if let Some(native) = native {
            let kernel = tasks.kernel.shared_handle();
            bank.activate_parking_caller_with_context(
                &kernel,
                task,
                call_id,
                caller,
                installed,
                Some((&mut tasks.powerpc_contexts, Box::new(native.clone()))),
            )
            .ok()?;
        } else {
            bank.activate_parking_caller(&tasks.kernel, task, call_id, caller, installed)
                .ok()?;
        }
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

    /// Activate a native call without discarding its task or logical arguments.
    /// The entire parameter area must be writable before any call state or
    /// architectural state changes. No guest execution intervenes in commit.
    #[cfg(test)]
    pub(crate) fn activate_powerpc_effect(
        &self,
        cpu: &mut PpcCpu,
        memory: &mut GuestAddressSpace,
        effect: GuestCallEffect,
    ) -> bool {
        self.activate_powerpc_effect_with_scratch(cpu, memory, effect, None, None)
    }

    /// Transfer a tracked temporary allocation to this exact call on success.
    /// On refusal, its producer retains responsibility for releasing it.
    pub(crate) fn activate_powerpc_effect_with_scratch(
        &self,
        cpu: &mut PpcCpu,
        memory: &mut GuestAddressSpace,
        effect: GuestCallEffect,
        scratch: Option<u32>,
        cfm_load: Option<CfmLoadOperation>,
    ) -> bool {
        self.activate_powerpc_effect_with_operation(
            cpu,
            memory,
            effect,
            scratch,
            cfm_load.map(|load| ManagerContinuation::Cfm(CfmOperation::Load(load))),
        )
    }

    pub(crate) fn activate_powerpc_effect_with_operation(
        &self,
        cpu: &mut PpcCpu,
        memory: &mut GuestAddressSpace,
        effect: GuestCallEffect,
        scratch: Option<u32>,
        operation: Option<ManagerContinuation>,
    ) -> bool {
        let GuestCallEffect::CallGuest {
            request,
            continuation,
        } = effect;
        let GuestCallContinuation::ReturnToPowerPc { return_pc, .. } = continuation else {
            return false;
        };
        let GuestCallArguments::PowerPc(arguments) = request.arguments else {
            return false;
        };
        if request.task != self.current_task()
            || request.target.isa != GuestIsa::PowerPc
            || request.target.entry == 0
        {
            return false;
        }
        let Some(prepared) =
            PreparedPowerPcCallArguments::prepare(cpu, memory, arguments.as_slice())
        else {
            return false;
        };
        let Some(call_id) = self.submit_effect(effect) else {
            return false;
        };
        self.0
            .borrow_mut()
            .frames
            .get_mut(&call_id)
            .expect("submitted call has its architectural frame")
            .native_scratch = scratch;
        self.0
            .borrow_mut()
            .frames
            .get_mut(&call_id)
            .expect("submitted call has its manager continuation")
            .operation = operation;
        // Submission just installed this task's pending top frame. This
        // synchronous transition cannot be displaced by another guest call.
        self.0
            .borrow()
            .kernel
            .activate(request.task, call_id)
            .expect("newly submitted native call remains pending");
        prepared.install();
        cpu.pc = request.target.entry;
        cpu.gpr[2] = request.target.rtoc;
        cpu.lr = return_pc;
        true
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
    #[cfg(test)]
    pub(crate) fn complete_powerpc(&self, cpu: &mut PpcCpu) -> bool {
        self.complete_powerpc_inner(cpu, None, None)
    }

    #[cfg(test)]
    pub(crate) fn complete_powerpc_releasing_scratch(
        &self,
        cpu: &mut PpcCpu,
        memory_manager: &mut ProcessNativeMemoryManager,
    ) -> bool {
        self.complete_powerpc_inner(cpu, Some(memory_manager), None)
    }

    pub(crate) fn is_cfm_load_pending(&self, id: CfmLoadId) -> bool {
        self.0.borrow().frames.values().any(|frame| {
            frame.operation.as_ref().is_some_and(
                |operation| matches!(operation, ManagerContinuation::Cfm(cfm) if cfm.id() == id),
            )
        })
    }

    pub(crate) fn is_resource_preparation_pending(&self, record: u32) -> bool {
        self.0.borrow().frames.values().any(|frame| {
            matches!(frame.operation.as_ref(),
            Some(ManagerContinuation::Cfm(CfmOperation::Resource(call))) if call.preparation.record == record)
        })
    }

    #[cfg(test)]
    pub(crate) fn complete_powerpc_resuming_load(
        &self,
        cpu: &mut PpcCpu,
        memory_manager: &mut ProcessNativeMemoryManager,
        mut resume: impl FnMut(CfmLoadOperation, u32) -> u32,
    ) -> bool {
        self.complete_powerpc_resuming_operation(cpu, memory_manager, |operation, result| {
            match operation {
                ManagerContinuation::Cfm(CfmOperation::Load(load)) => resume(load, result),
                _ => {
                    panic!("load-only fixture received resource operation")
                }
            }
        })
    }

    pub(crate) fn complete_powerpc_resuming_operation(
        &self,
        cpu: &mut PpcCpu,
        memory_manager: &mut ProcessNativeMemoryManager,
        mut resume: impl FnMut(ManagerContinuation, u32) -> u32,
    ) -> bool {
        self.complete_powerpc_inner(cpu, Some(memory_manager), Some(&mut resume))
    }

    fn complete_powerpc_inner(
        &self,
        cpu: &mut PpcCpu,
        memory_manager: Option<&mut ProcessNativeMemoryManager>,
        resume: Option<&mut dyn FnMut(ManagerContinuation, u32) -> u32>,
    ) -> bool {
        let (task, call_id, origin, scratch, operation) = {
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
            if let Some(scratch) = frame.native_scratch {
                if !memory_manager.as_deref().is_some_and(|manager| {
                    manager
                        .native_ptr_records()
                        .iter()
                        .any(|record| record.ptr == scratch)
                }) {
                    return false;
                }
            }
            if frame.operation.is_some() && resume.is_none() {
                return false;
            }
            (
                task,
                semantic.call_id(),
                origin,
                frame.native_scratch,
                frame.operation.clone(),
            )
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
        // Transfer the finished operation out of execution custody before its
        // service resumes. The enclosing frame stays live, and no execution
        // store borrow may span the semantic consumer.
        drop(tasks);
        if let Some(operation) = operation {
            cpu.gpr[3] =
                resume.expect("manager return requires its semantic consumer")(operation, cpu.gpr[3]);
        }
        if let Some(scratch) = scratch {
            memory_manager
                .expect("scratch return requires its memory manager")
                .release_native_scratch(scratch);
        }
        Self::apply_powerpc_return(cpu, origin);
        true
    }

    /// Complete an emulated 68k interval for its parked native caller.
    #[cfg(test)]
    pub(crate) fn complete_m68k_for_powerpc(
        &self,
        post_call_pc: u32,
        final_sp: u32,
        result: Option<u32>,
        cpu: &mut PpcCpu,
    ) -> bool {
        self.complete_m68k_for_powerpc_inner(post_call_pc, final_sp, result, cpu, None)
    }

    pub(crate) fn complete_m68k_operation_for_powerpc(
        &self,
        post_call_pc: u32,
        final_sp: u32,
        result: Option<u32>,
        cpu: &mut PpcCpu,
        memory: &mut GuestAddressSpace,
        manager: &mut ProcessNativeMemoryManager,
    ) -> bool {
        self.complete_m68k_for_powerpc_inner(
            post_call_pc,
            final_sp,
            result,
            cpu,
            Some((memory, manager)),
        )
    }

    fn complete_m68k_for_powerpc_inner(
        &self,
        post_call_pc: u32,
        final_sp: u32,
        result: Option<u32>,
        cpu: &mut PpcCpu,
        services: Option<(&mut GuestAddressSpace, &mut ProcessNativeMemoryManager)>,
    ) -> bool {
        let (task, call_id, origin, scratch, operation) = {
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
            if (frame.native_scratch.is_some() || frame.operation.is_some()) && services.is_none() {
                return false;
            }
            if let Some(scratch) = frame.native_scratch {
                if !services.as_ref().is_some_and(|(_, manager)| {
                    manager
                        .native_ptr_records()
                        .iter()
                        .any(|record| record.ptr == scratch)
                }) {
                    return false;
                }
            }
            if matches!(frame.operation, Some(ManagerContinuation::Cfm(_))) {
                return false;
            }
            (
                task,
                semantic.call_id(),
                origin,
                frame.native_scratch,
                frame.operation.clone(),
            )
        };
        let mut tasks = self.0.borrow_mut();
        let native = if tasks.powerpc_contexts.contains(task, call_id) {
            let kernel = tasks.kernel.shared_handle();
            let Ok((context, _)) = tasks
                .powerpc_contexts
                .take_while_completing(&kernel, task, call_id, result)
            else {
                return false;
            };
            Some(context)
        } else {
            #[cfg(not(test))]
            return false;
            #[cfg(test)]
            {
                // Register-only unit fixtures have no native engine to retain.
                if tasks.kernel.complete(task, call_id, result).is_err() {
                    return false;
                }
                None
            }
        };
        let _ = tasks
            .kernel
            .retire(task, call_id)
            .expect("completed 68k continuation must retire transactionally");
        tasks
            .frames
            .remove(&call_id)
            .expect("semantic continuation must have an adapter frame");
        drop(tasks);
        if let Some((memory, manager)) = services {
            if let Some(ManagerContinuation::Menu(operation)) = operation {
                operation.complete(memory);
            }
            if let Some(scratch) = scratch {
                manager.release_native_scratch(scratch);
            }
        }
        if let Some(native) = native {
            restore_powerpc_context(cpu, *native);
        }
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
    #[cfg(test)]
    pub(crate) fn complete_m68k(&self, post_trap_pc: u32, final_sp: u32) -> bool {
        self.complete_m68k_with_operation(post_trap_pc, final_sp, |_| panic!("unhandled manager completion")).is_some()
    }

    pub(crate) fn complete_m68k_with_operation(
        &self,
        post_trap_pc: u32,
        final_sp: u32,
        mut resume: impl FnMut(ManagerContinuation),
    ) -> Option<u32> {
        let (task, call_id, caller_sp) = {
            let tasks = self.0.borrow();
            let task = tasks.kernel.current_task();
            let Some(semantic) = tasks.kernel.peek(task) else {
                return None;
            };
            let Some(frame) = tasks.frames.get(&semantic.call_id()) else {
                return None;
            };
            let GuestCallOrigin::M68k(origin) = frame.origin else {
                return None;
            };
            if frame.target.isa != GuestIsa::M68k
                || origin.return_pc.wrapping_add(2) != post_trap_pc
                || origin.parked_sp.unwrap_or(origin.final_sp) != final_sp
            {
                return None;
            }
            (task, semantic.call_id(), origin.final_sp)
        };
        let mut tasks = self.0.borrow_mut();
        let phase = tasks
            .kernel
            .peek(task)
            .expect("semantic continuation must have an adapter frame")
            .phase();
        if phase == ContinuationPhase::Pending && tasks.kernel.activate(task, call_id).is_err() {
            return None;
        }
        if tasks.kernel.complete(task, call_id, None).is_err() {
            return None;
        }
        let _ = tasks
            .kernel
            .retire(task, call_id)
            .expect("completed 68k continuation must retire transactionally");
        let frame = tasks
            .frames
            .remove(&call_id)
            .expect("semantic continuation must have an adapter frame");
        drop(tasks);
        if let Some(operation) = frame.operation {
            resume(operation);
        }
        Some(caller_sp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::GuestAddressSpace;
    use ppc::PpcRunResult;

    const RETURN_PC: u32 = 0x01f0_4000;

    #[test]
    fn menu_bar_operations_scope_nested_calls_and_preserve_each_return_abi() {
        use crate::menu_manager::{
            MenuBarBuild, MenuDefinitionCompletion, MenuDefinitionOperation, MenuDefinitionResult,
        };
        let calls = SharedGuestCallStack::default();
        let outer_origin = MenuBarCallOrigin::M68k {
            stack_pointer: 0x3000,
            return_address: 0x2002,
        };
        let outer = calls
            .begin_menu_bar_build(MenuBarBuild::new(111, vec![5]), outer_origin)
            .unwrap();
        assert_eq!(calls.advance_menu_bar_build(GuestIsa::PowerPc), None);
        assert_eq!(calls.menu_bar_build(), Some(outer));
        assert_eq!(
            calls.advance_menu_bar_build(GuestIsa::M68k),
            Some(MenuBarBuildResume::Size(5))
        );
        assert!(!calls.is_empty());
        assert!(!calls.is_pristine());
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0
            },
            0x2000,
            0x3000
        ));
        assert_eq!(calls.menu_bar_build(), None);
        assert_eq!(calls.ready_menu_bar_build(GuestIsa::M68k), None);
        let completion = MenuDefinitionCompletion::pending();
        calls.bind_menu_bar_build_completion(outer, 5, completion.clone());
        let inner_origin = MenuBarCallOrigin::PowerPc {
            return_address: 0x4000,
        };
        let inner = calls
            .begin_menu_bar_build(MenuBarBuild::new(222, vec![]), inner_origin)
            .unwrap();
        assert_ne!(inner, outer);
        assert_eq!(
            calls.advance_menu_bar_build(GuestIsa::PowerPc),
            Some(MenuBarBuildResume::Complete {
                result: 222,
                origin: inner_origin
            })
        );
        assert!(calls.complete_m68k(0x2002, 0x3000));
        assert_eq!(calls.menu_bar_build(), Some(outer));
        assert_eq!(
            calls.advance_menu_bar_build(GuestIsa::M68k),
            Some(MenuBarBuildResume::Waiting)
        );
        MenuDefinitionOperation {
            scratch: 0,
            completion,
        }
        .complete_result(Ok(MenuDefinitionResult {
            menu_rect: (0, 0, 0, 0),
            which_item: 0,
        }));
        assert_eq!(calls.ready_menu_bar_build(GuestIsa::PowerPc), None);
        assert_eq!(calls.ready_menu_bar_build(GuestIsa::M68k), Some(outer));
        assert_eq!(
            calls.advance_menu_bar_build(GuestIsa::M68k),
            Some(MenuBarBuildResume::Complete {
                result: 111,
                origin: outer_origin
            })
        );
        assert_eq!(calls.advance_menu_bar_build(GuestIsa::M68k), None);
        assert_eq!(calls.ready_menu_bar_build(GuestIsa::M68k), None);
        assert!(calls.is_empty());
    }

    #[test]
    fn fresh_menu_entries_and_resumption_use_exact_call_ownership() {
        use crate::menu_manager::{test_process_menu_tracking, MenuTrackingRequest};
        let calls = SharedGuestCallStack::default();
        let mut tracking = calls.menu_tracking_view();
        let original = MenuTrackingCall {
            request: MenuTrackingRequest::MenuSelect { initial_point: 12 },
            origin: MenuTrackingOrigin::M68k {
                stack_pointer: 0x4000,
                return_address: 0x5000,
            },
        };
        let outer = tracking.enter_new_call(original);
        *tracking = Some(test_process_menu_tracking(111));
        assert_eq!(tracking.ready_call(GuestIsa::M68k), Some(original));
        assert_eq!(tracking.ready_call(GuestIsa::PowerPc), None);
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0
            },
            0x2000,
            0x3000
        ));
        assert_eq!(
            tracking.ready_call(GuestIsa::M68k),
            None,
            "a live callback still owns execution"
        );
        assert!(calls.complete_m68k(0x2002, 0x3000));
        assert_eq!(tracking.ready_call(GuestIsa::M68k), Some(original));
        let inner = tracking.enter_new_call(original);
        assert_ne!(
            inner.id, outer.id,
            "a fresh public entry cannot resume the prior operation"
        );
        *tracking = Some(test_process_menu_tracking(222));
        *tracking = None;
        drop(inner);
        assert_eq!(tracking.as_ref().unwrap().menu_handle, 111);
        let (_, resumed) = tracking.resume_call(GuestIsa::M68k).unwrap();
        assert_eq!(resumed.id, outer.id);
        *tracking = None;
        drop(resumed);
        drop(outer);
        assert!(calls.is_empty());
    }

    #[test]
    fn tracking_reentry_preserves_the_original_request_and_return_boundary() {
        use crate::menu_manager::{test_process_menu_tracking, MenuTrackingRequest};
        for origin in [
            MenuTrackingOrigin::M68k {
                stack_pointer: 0x1000,
                return_address: 0x2000,
            },
            MenuTrackingOrigin::PowerPc {
                stack_pointer: 0x3000,
                return_address: 0x4000,
            },
        ] {
            let calls = SharedGuestCallStack::default();
            let mut tracking = calls.menu_tracking_view();
            let original = MenuTrackingCall {
                request: MenuTrackingRequest::MenuSelect {
                    initial_point: 0x1234_5678,
                },
                origin,
            };
            let entry = tracking.enter_call(original);
            *tracking = Some(test_process_menu_tracking(111));
            let reentry = tracking.enter_call(MenuTrackingCall {
                request: MenuTrackingRequest::MenuSelect { initial_point: 0 },
                origin: MenuTrackingOrigin::PowerPc {
                    stack_pointer: 0x5000,
                    return_address: 0x6000,
                },
            });
            assert_eq!(entry.id, reentry.id);
            assert_eq!(tracking.context().call, Some(original));
            assert_eq!(tracking.context().caller_isa(), Some(origin.isa()));
            drop(reentry);
            assert_eq!(tracking.context().call, Some(original));
            *tracking = None;
            drop(entry);
            assert!(calls.is_empty());
            assert_eq!(tracking.context().call, None);
            let cancelled = tracking.enter_call(original);
            *tracking = Some(test_process_menu_tracking(222));
            *tracking = None;
            let replacement = MenuTrackingCall {
                request: MenuTrackingRequest::MenuSelect { initial_point: 9 },
                origin: MenuTrackingOrigin::PowerPc {
                    stack_pointer: 0x7000,
                    return_address: 0x8000,
                },
            };
            let fresh = tracking.enter_call(replacement);
            assert_ne!(cancelled.id, fresh.id);
            drop(cancelled);
            assert_eq!(tracking.context().call, Some(replacement));
            drop(fresh);
            let idle = tracking.enter_call(original);
            drop(idle);
            assert!(
                calls.is_empty(),
                "an immediate return cannot retain an idle call record"
            );
            assert_eq!(tracking.context().call, None);
        }
    }

    #[test]
    fn menu_tracking_roots_preserve_parent_state_and_exact_receipts() {
        use crate::menu_manager::{
            test_process_menu_tracking, MenuDefinitionCompletion, MenuDefinitionMessage,
            MenuDefinitionOperation, MenuDefinitionResult, MenuDefinitionTracking,
        };
        let calls = SharedGuestCallStack::default();
        let mut tracking = calls.menu_tracking_view();
        let outer = tracking.enter();
        let outer_id = outer.id;
        *tracking = Some(test_process_menu_tracking(111));
        tracking.context_mut().call = Some(MenuTrackingCall {
            request: crate::menu_manager::MenuTrackingRequest::MenuSelect { initial_point: 12 },
            origin: MenuTrackingOrigin::PowerPc {
                stack_pointer: 0x1000,
                return_address: 0x1234,
            },
        });
        tracking.context_mut().native_port = Some((0x2000, 0x3000));
        tracking.context_mut().definition = Some(MenuDefinitionTracking::begin_draw(111, (1, 2, 3, 4)));
        let invocation = tracking
            .context()
            .definition
            .as_ref()
            .unwrap()
            .pending_invocation()
            .unwrap();
        let completion = MenuDefinitionCompletion::pending();
        assert_eq!(tracking.entry_id(), Some(outer_id));
        assert!(!calls.is_empty());
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0
            },
            0x4000,
            0x5000
        ));
        assert_eq!(
            tracking.entry_id(),
            None,
            "a nested guest entry cannot bind the parent's pending message"
        );
        tracking.bind_completion(outer_id, invocation, completion.clone());
        let inner = tracking.enter();
        assert_ne!(inner.id, outer_id);
        assert!(tracking.is_none());
        assert_eq!(tracking.context().native_port, None);
        *tracking = Some(test_process_menu_tracking(222));
        tracking.context_mut().definition = Some(MenuDefinitionTracking::begin_draw(111, (1, 2, 3, 4)));
        MenuDefinitionOperation {
            scratch: 0,
            completion,
        }
        .complete_result(Ok(MenuDefinitionResult {
            menu_rect: (5, 6, 7, 8),
            which_item: 2,
        }));
        assert_eq!(
            tracking
                .context_mut()
                .definition
                .as_mut()
                .unwrap()
                .complete_callback(),
            Ok(None),
            "identical messages do not share receipts"
        );
        *tracking.context_mut() = MenuTrackingContext::default();
        drop(inner);
        assert_eq!(tracking.as_ref().unwrap().menu_handle, 111);
        assert_eq!(tracking.context().native_port, Some((0x2000, 0x3000)));
        assert_eq!(
            tracking
                .context()
                .native_menu()
                .unwrap()
                .origin
                .return_address(),
            0x1234
        );
        assert!(calls.complete_m68k(0x4002, 0x5000));
        assert_eq!(tracking.entry_id(), Some(outer_id));
        assert_eq!(
            tracking
                .context_mut()
                .definition
                .as_mut()
                .unwrap()
                .complete_callback(),
            Ok(Some(MenuDefinitionMessage::Draw))
        );
        assert_eq!(
            tracking.context().definition.as_ref().unwrap().which_item(),
            2
        );
        *tracking.context_mut() = MenuTrackingContext::default();
        drop(outer);
        assert!(calls.is_empty());
        assert!(calls.0.borrow().menu_calls.calls.is_empty());
    }

    #[test]
    fn empty_menu_queries_do_not_create_root_operations() {
        let calls = SharedGuestCallStack::default();
        let mut view = calls.menu_tracking_view();
        assert!(view.as_mut().is_none());
        assert!(view.take().is_none());
        assert!(view.existing_context_mut().is_none());
        assert!(calls.is_pristine());
        let other = SharedGuestCallStack::default();
        view.attach_to(&other.menu_tracking_view());
        assert!(other.is_pristine());
    }

    #[test]
    fn cloned_menu_view_reconnects_to_its_detached_execution_owner() {
        let calls = SharedGuestCallStack::default();
        let mut view = calls.menu_tracking_view();
        let entry = view.enter();
        *view = Some(crate::menu_manager::test_process_menu_tracking(111));
        let cloned_calls = calls.clone();
        let mut cloned_view = view.clone();
        assert!(!view.ptr_eq(&cloned_view));
        cloned_view.bind_execution(&cloned_calls);
        cloned_view.as_mut().unwrap().highlighted_item = 3;
        assert_eq!(
            cloned_calls
                .menu_tracking_view()
                .as_ref()
                .unwrap()
                .highlighted_item,
            3
        );
        assert_eq!(view.as_ref().unwrap().highlighted_item, 1);
        *cloned_view = None;
        cloned_view.finish_if_idle(entry.id);
        assert!(cloned_calls.is_empty());
        assert!(!calls.is_empty());
        *view = None;
        drop(entry);
    }

    #[test]
    fn menu_tracking_roots_follow_tasks_and_retire_owned_state() {
        let calls = SharedGuestCallStack::default();
        let mut tracking = calls.menu_tracking_view();
        let main = tracking.enter();
        *tracking = Some(crate::menu_manager::test_process_menu_tracking(111));
        let worker = ExecutionTaskId::from_thread_id(7);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        assert!(tracking.is_none());
        let owned = tracking.enter();
        assert_ne!(main.id, owned.id);
        *tracking = Some(crate::menu_manager::test_process_menu_tracking(222));
        tracking.context_mut().call = Some(MenuTrackingCall {
            request: crate::menu_manager::MenuTrackingRequest::PopUp(
                crate::menu_manager::PopupMenuRequest {
                    menu_handle: 222,
                    anchor: (20, 30),
                    requested_item: 2,
                },
            ),
            origin: MenuTrackingOrigin::PowerPc {
                stack_pointer: 0x4000,
                return_address: 0x5000,
            },
        });
        tracking.context_mut().native_port = Some((0x6000, 0x7000));
        assert!(calls.switch_to_task(ExecutionTaskId::APPLICATION));
        assert_eq!(tracking.as_ref().unwrap().menu_handle, 111);
        assert_eq!(tracking.context().native_popup(), None);
        assert!(calls.remove_task(worker));
        drop(owned);
        assert_eq!(calls.0.borrow().menu_calls.calls.len(), 1);
        assert_eq!(tracking.entry_id(), Some(main.id));
        *tracking = None;
        drop(main);
        assert!(calls.is_empty());
        assert!(calls.0.borrow().menu_calls.calls.is_empty());
        assert!(
            !calls.is_pristine(),
            "retired operation identities are not reused"
        );
    }

    #[test]
    fn menu_bar_operations_follow_tasks_and_retirement() {
        use crate::menu_manager::MenuBarBuild;
        let calls = SharedGuestCallStack::default();
        let origin = MenuBarCallOrigin::PowerPc {
            return_address: 0x1000,
        };
        let main = calls
            .begin_menu_bar_build(MenuBarBuild::new(111, vec![]), origin)
            .unwrap();
        let worker = ExecutionTaskId::from_thread_id(7);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        assert_eq!(calls.menu_bar_build(), None);
        let owned = calls
            .begin_menu_bar_build(MenuBarBuild::new(222, vec![]), origin)
            .unwrap();
        assert_ne!(main, owned);
        assert!(calls.switch_to_task(ExecutionTaskId::APPLICATION));
        assert_eq!(calls.menu_bar_build(), Some(main));
        assert!(calls.remove_task(worker));
        assert_eq!(calls.0.borrow().menu_calls.calls.len(), 1);
        assert_eq!(
            calls.advance_menu_bar_build(GuestIsa::PowerPc),
            Some(MenuBarBuildResume::Complete {
                result: 111,
                origin
            })
        );
        assert!(calls.is_empty());
    }

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
    fn classic_parked_engines_follow_the_process_owner_and_refuse_snapshot_duplication() {
        let calls = SharedGuestCallStack::default();
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0
            },
            0x2000,
            0x3000
        ));
        let (call, _) = calls.top_frame().unwrap();
        let bank = calls.m68k_context_bank();
        let mut cpu = M68kCpu::new();
        cpu.core.set_a(7, 0x7654);
        cpu.core.set_d(6, 0xabcdef);
        assert!(calls
            .park_context(
                &mut bank.borrow_mut(),
                ExecutionTaskId::APPLICATION,
                call,
                cpu
            )
            .is_ok());
        let shared = calls.shared_handle();
        assert!(Rc::ptr_eq(&bank, &shared.m68k_context_bank()));
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| calls.clone())).is_err());
        assert!(bank.borrow().contains(ExecutionTaskId::APPLICATION, call));
        let mut adopting = SharedGuestCallStack::default();
        adopting.attach_to(&calls);
        assert!(Rc::ptr_eq(&bank, &adopting.m68k_context_bank()));
        assert!(!adopting.is_pristine());
        let restored = bank
            .borrow_mut()
            .take(&calls.0.borrow().kernel, ExecutionTaskId::APPLICATION, call)
            .unwrap();
        assert_eq!(restored.core.a(7), 0x7654);
        assert_eq!(restored.core.d(6), 0xabcdef);
        assert!(adopting.m68k_context_bank().borrow().is_empty());
        let snapshot = calls.clone();
        assert!(!Rc::ptr_eq(&bank, &snapshot.m68k_context_bank()));
        assert_eq!(calls, snapshot);
    }

    #[test]
    fn thread_disposal_refuses_the_application_before_committing_either_abi_result() {
        let calls = SharedGuestCallStack::default();
        assert!(calls
            .save_cooperative_context(ExecutionTaskId::APPLICATION, CooperativeThread::default()));
        let worker = calls
            .create_classic_thread(
                CooperativeThread::default(),
                ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();
        for recycle in [false, true] {
            assert!(calls
                .retire_cooperative_context(
                    ExecutionTaskId::APPLICATION,
                    Some(worker),
                    recycle,
                    |_| panic!("application disposal must not write its result")
                )
                .is_none());
            assert!(calls
                .retire_native_thread(
                    ExecutionTaskId::APPLICATION,
                    &mut PpcCpu::new(),
                    recycle,
                    |_| panic!("application disposal must not write its result")
                )
                .is_none());
            assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
            assert_eq!(calls.next_ready_task(None), Some(worker));
        }
        assert!(calls.switch_to_task(worker));
        assert!(calls
            .retire_cooperative_context(ExecutionTaskId::APPLICATION, None, false, |_| panic!(
                "a worker must not dispose its application"
            ))
            .is_none());
        assert_eq!(calls.current_task(), worker);
        assert!(calls
            .cooperative_context(ExecutionTaskId::APPLICATION)
            .is_some());
    }

    #[test]
    fn thread_recycled_storage_retains_entry_isa_and_is_unavailable_until_retirement_commits() {
        let calls = SharedGuestCallStack::default();
        let classic_storage = ThreadStorage {
            result_destination: 0x8000,
            stack_base: 0x1000,
            stack_limit: 0x1800,
            managed_pointer: false,
        };
        let classic = calls
            .create_classic_thread(CooperativeThread::default(), classic_storage, true, |_| {
                true
            })
            .unwrap();
        let native = calls
            .create_native_thread(
                NativeThreadContext {
                    cpu: Box::new(PpcCpu::new()),
                },
                ThreadStorage {
                    stack_base: 0x2000,
                    stack_limit: 0x3000,
                    managed_pointer: true,
                    ..classic_storage
                },
                true,
                |_| true,
            )
            .unwrap();
        assert!(calls
            .retire_cooperative_context(classic, None, true, |_| false)
            .is_none());
        assert_eq!(
            calls.request_thread_stack(GuestIsa::M68k, 1024, 2),
            Err(-617)
        );
        assert_eq!(calls.thread_storage(classic), Some(classic_storage));
        assert!(calls
            .retire_cooperative_context(classic, None, true, |_| true)
            .is_some());
        assert_eq!(
            calls.request_thread_stack(GuestIsa::PowerPc, 1024, 2),
            Err(-617)
        );
        assert!(calls
            .retire_native_thread(native, &mut PpcCpu::new(), true, |_| true)
            .is_some());
        assert_eq!(
            calls.request_thread_stack(GuestIsa::PowerPc, 1024, 2 | 16),
            Err(-617)
        );
        let pooled_native = calls
            .request_thread_stack(GuestIsa::PowerPc, 4096, 2 | 16)
            .unwrap()
            .unwrap();
        assert_eq!(pooled_native.stack_base, 0x2000);
        assert!(pooled_native.managed_pointer);
        assert_eq!(pooled_native.result_destination, 0);
        let pooled_classic = calls
            .request_thread_stack(GuestIsa::M68k, 1024, 2)
            .unwrap()
            .unwrap();
        assert_eq!(pooled_classic.stack_base, classic_storage.stack_base);
        assert!(!pooled_classic.managed_pointer);
        assert_eq!(pooled_classic.result_destination, 0);
        assert_eq!(
            calls.request_thread_stack(GuestIsa::M68k, 1024, 2),
            Err(-617)
        );
    }

    #[test]
    fn native_retirement_hands_off_to_classic_without_losing_the_native_caller() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        cpu.pc = 0x1234;
        cpu.gpr[20] = 0x1122_3344;
        calls.start_native_engine();
        assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::PowerPc));
        let classic = calls.create_task().unwrap();
        let mut context = CooperativeThread::default();
        context.pc = 0x5678;
        assert!(calls.save_cooperative_context(classic, context));
        assert!(calls.set_scheduling_state(classic, ExecutionTaskState::Ready));
        let native = calls
            .create_native_thread(
                NativeThreadContext {
                    cpu: Box::new(PpcCpu::new()),
                },
                crate::guest_call::ThreadStorage {
                    result_destination: 0,
                    stack_base: 0,
                    stack_limit: 0,
                    managed_pointer: true,
                },
                false,
                |_| true,
            )
            .unwrap();
        assert!(calls.switch_to_task(native));
        assert!(calls.prepare_native_task(&mut cpu));
        assert!(calls
            .retire_native_thread(native, &mut cpu, false, |_| true)
            .is_some());
        assert_eq!(calls.current_task(), classic);
        assert!(calls.has_classic_task_handoff());
        let blocked_pc = cpu.pc;
        assert!(!calls.prepare_native_task(&mut cpu));
        assert_eq!(cpu.pc, blocked_pc);
        assert!(calls
            .switch_from_classic(ExecutionTaskId::APPLICATION)
            .is_none());
        assert_eq!(calls.take_classic_task_handoff().unwrap().pc, 0x5678);
        assert!(calls
            .switch_from_classic(ExecutionTaskId::APPLICATION)
            .unwrap()
            .is_none());
        assert!(calls.prepare_native_task(&mut cpu));
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.gpr[20], 0x1122_3344);
        assert!(!calls.has_pending_task_handoff());
    }

    #[test]
    fn native_yield_preflights_context_and_critical_state_before_saving_return() {
        let calls = SharedGuestCallStack::default();
        let mut cpu = PpcCpu::new();
        cpu.pc = 0x1234;
        cpu.lr = 0x4560;
        cpu.gpr[3] = 99;
        let worker = calls
            .create_native_thread(
                NativeThreadContext {
                    cpu: Box::new(PpcCpu::new()),
                },
                crate::guest_call::ThreadStorage {
                    result_destination: 0,
                    stack_base: 0,
                    stack_limit: 0,
                    managed_pointer: true,
                },
                false,
                |_| true,
            )
            .unwrap();
        calls.begin_critical();
        assert_eq!(
            calls.yield_native_thread(&mut cpu, worker.thread_id()),
            Err(-619)
        );
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.gpr[3], 99);
        assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
        assert!(calls.end_critical());
        let classic = calls.create_task().unwrap();
        assert!(calls.set_scheduling_state(classic, ExecutionTaskState::Ready));
        assert_eq!(
            calls.yield_native_thread(&mut cpu, classic.thread_id()),
            Err(-619)
        );
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.gpr[3], 99);
        assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
        assert!(calls
            .yield_native_thread(&mut cpu, worker.thread_id())
            .unwrap());
        assert_eq!(calls.current_task(), worker);
        cpu.lr = 0x9000;
        assert!(calls
            .yield_native_thread(&mut cpu, ExecutionTaskId::APPLICATION.thread_id())
            .unwrap());
        assert_eq!(cpu.pc, 0x4560);
        assert_eq!(cpu.gpr[3], 0);
    }

    #[test]
    fn execution_routes_follow_task_entry_and_pending_work_without_consuming_it() {
        let calls = SharedGuestCallStack::default();
        let application = NativeAvailability {
            application: true,
            ..Default::default()
        };
        assert_eq!(calls.execution_route(application), ExecutionRoute::Classic);
        assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::PowerPc));
        assert_eq!(
            calls.execution_route(application),
            ExecutionRoute::NativeApplication
        );
        let worker = ExecutionTaskId::from_thread_id(7);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        assert_eq!(calls.execution_route(application), ExecutionRoute::Classic);
        assert!(calls.begin_m68k_to_powerpc(
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: 0x1000,
                rtoc: 0
            },
            PowerPcArguments::from_slice(&[]).unwrap(),
            0x2000,
            0x3000,
            None
        ));
        let before = calls.clone();
        assert!(!calls.bind_task_entry_isa(worker, GuestIsa::PowerPc));
        for (availability, expected) in [
            (application, ExecutionRoute::NativeApplication),
            (
                NativeAvailability {
                    companion: true,
                    ..Default::default()
                },
                ExecutionRoute::NativeCompanion,
            ),
            (
                NativeAvailability {
                    staged_companion: true,
                    ..Default::default()
                },
                ExecutionRoute::PrepareCompanion,
            ),
            (NativeAvailability::default(), ExecutionRoute::Blocked),
        ] {
            assert_eq!(calls.execution_route(availability), expected);
            assert_eq!(calls, before);
        }
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Stopped));
        assert_eq!(calls.execution_route(application), ExecutionRoute::Blocked);
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
            .retire_cooperative_context(worker, None, false, |_| {
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
    fn native_arguments_preserve_word_layout_for_empty_short_and_spilled_calls() {
        for count in [0usize, 1, 8, 9, 16] {
            let mut cpu = PpcCpu::new();
            cpu.gpr.fill(0xfeed_beef);
            cpu.gpr[1] = 0x8000;
            let before = cpu.gpr;
            let mut memory = GuestAddressSpace::new();
            memory.add_region(0x8000, vec![0xa5; 128]);
            let values: Vec<_> = (0..count as u32).map(|i| 0x1234_0000 + i).collect();
            assert!(install_powerpc_call_arguments(&mut cpu, &mut memory, &values).is_some());
            assert_eq!(&cpu.gpr[..3], &before[..3]);
            assert_eq!(&cpu.gpr[11..], &before[11..]);
            for slot in 0..count.max(8) {
                let expected = values.get(slot).copied().unwrap_or(0);
                assert_eq!(memory.read_u32_be(0x8018 + slot as u32 * 4), Some(expected));
                if slot < 8 {
                    assert_eq!(cpu.gpr[3 + slot], expected);
                }
            }
            assert_eq!(memory.read_u32_be(0x8014), Some(0xa5a5_a5a5));
            assert_eq!(
                memory.read_u32_be(0x8018 + count.max(8) as u32 * 4),
                Some(0xa5a5_a5a5)
            );
        }
    }

    #[test]
    fn native_effect_preflights_all_arguments_and_preserves_nested_calls_on_refusal() {
        for failure in 0..5 {
            let calls = SharedGuestCallStack::default();
            let worker = calls.create_task().unwrap();
            assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
            assert!(calls.switch_to_task(worker));
            let mut cpu = PpcCpu::new();
            calls.externalize_powerpc_action(
                &mut cpu,
                native_action(0x1000, 0x2000, PpcNativeReturnGpr3::Preserve),
            );
            cpu.gpr[1] = if failure == 3 { u32::MAX - 8 } else { 0x8000 };
            cpu.gpr[3..11].fill(0xfeed);
            let arguments = PowerPcArguments::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();
            let mut request = GuestCallRequest::for_task(
                worker,
                GuestCallTarget {
                    isa: GuestIsa::PowerPc,
                    entry: 0x3000,
                    rtoc: 0x3100,
                },
            )
            .with_powerpc_arguments(arguments);
            if failure == 2 {
                request.task = ExecutionTaskId::APPLICATION;
            }
            if failure == 4 {
                request.target.entry = 0;
            }
            let continuation = GuestCallContinuation::to_powerpc(
                RETURN_PC,
                cpu.pc,
                cpu.gpr[2],
                PpcNativeReturnGpr3::Mask(0xff),
            );
            let effect = GuestCallEffect::call_guest(request, continuation);
            let mut memory = GuestAddressSpace::new();
            memory.add_region(0x8018, vec![0xa5; 32]);
            if failure == 1 {
                memory.add_readonly_region(0x8038, vec![0xa5; 4]);
            } else if failure != 0 {
                memory.add_region(0x8038, vec![0xa5; 4]);
            }
            let before = calls.clone();
            let registers = cpu.gpr;
            let control = (cpu.pc, cpu.lr);
            assert!(
                !calls.activate_powerpc_effect(&mut cpu, &mut memory, effect),
                "case {failure}"
            );
            assert_eq!(calls, before);
            assert_eq!(cpu.gpr, registers);
            assert_eq!((cpu.pc, cpu.lr), control);
            for offset in 0..32 {
                assert_eq!(memory.read_u8(0x8018 + offset), Some(0xa5));
            }

            // Retry under the same worker, retaining the outer continuation.
            let mut memory = GuestAddressSpace::new();
            memory.add_region(0x8000, vec![0xa5; 128]);
            cpu.gpr[1] = 0x8000;
            request.task = worker;
            request.target.entry = 0x3000;
            assert!(calls.activate_powerpc_effect(
                &mut cpu,
                &mut memory,
                GuestCallEffect::call_guest(request, continuation)
            ));
            assert_eq!(&cpu.gpr[3..11], &[1, 2, 3, 4, 5, 6, 7, 8]);
            assert_eq!(memory.read_u32_be(0x8038), Some(9));
            assert_eq!((cpu.pc, cpu.lr, cpu.gpr[2]), (0x3000, RETURN_PC, 0x3100));
            assert_eq!(calls.task_depth(worker), 2);
            assert_eq!(calls.task_depth(ExecutionTaskId::APPLICATION), 0);
            cpu.pc = RETURN_PC;
            cpu.gpr[3] = 0x1234;
            assert!(calls.complete_powerpc(&mut cpu));
            assert_eq!((cpu.pc, cpu.gpr[2], cpu.gpr[3]), (0x1000, 0x1100, 0x34));
            assert_eq!(calls.task_depth(worker), 1);
            cpu.pc = RETURN_PC;
            assert!(calls.complete_powerpc(&mut cpu));
            assert_eq!(cpu.pc, 0x2000);
            assert!(calls.is_empty());
        }
    }

    #[test]
    fn cfm_resumption_observes_retired_initializer_and_live_enclosing_call() {
        for result in [0, 1] {
            let calls = SharedGuestCallStack::default();
            let worker = calls.create_task().unwrap();
            assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
            assert!(calls.switch_to_task(worker));
            let mut memory = GuestAddressSpace::new();
            memory.add_region(0x8000, vec![0; 128]);
            let mut manager = ProcessNativeMemoryManager::default();
            let mut cpu = PpcCpu::new();
            cpu.gpr[1] = 0x8000;
            let operation = |id| CfmLoadOperation {
                id: CfmLoadId(id),
                main_address: 0,
                outputs: crate::cfm::CfmLoadOutputs {
                    connection: 0,
                    main_address: 0,
                    error_name: 0,
                },
                created_connection: true,
            };
            for id in [1, 2] {
                let request = GuestCallRequest::for_task(
                    worker,
                    GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: 0x1000 * id,
                        rtoc: 0x1100 * id,
                    },
                )
                .with_powerpc_arguments(PowerPcArguments::from_slice(&[]).unwrap());
                let effect = GuestCallEffect::call_guest(
                    request,
                    GuestCallContinuation::to_powerpc(
                        RETURN_PC,
                        0x3000 * id,
                        0x3100 * id,
                        PpcNativeReturnGpr3::Preserve,
                    ),
                );
                assert!(calls.activate_powerpc_effect_with_scratch(
                    &mut cpu,
                    &mut memory,
                    effect,
                    None,
                    Some(operation(id))
                ));
            }
            let inner = calls.top_frame().unwrap().0;
            let mut consumed = 0;
            let mut resume = |op: CfmLoadOperation, value| {
                consumed += 1;
                assert_eq!(op, operation(2));
                assert_eq!(value, result);
                assert_eq!(calls.current_task(), worker);
                assert!(!calls.is_cfm_load_pending(CfmLoadId(2)));
                assert!(calls.is_cfm_load_pending(CfmLoadId(1)));
                assert_ne!(calls.top_frame().unwrap().0, inner);
                0xABCD
            };
            assert!(!calls.complete_powerpc_resuming_load(&mut cpu, &mut manager, &mut resume));
            cpu.pc = RETURN_PC;
            cpu.gpr[3] = result;
            assert!(calls.complete_powerpc_resuming_load(&mut cpu, &mut manager, &mut resume));
            assert!(!calls.complete_powerpc_resuming_load(&mut cpu, &mut manager, &mut resume));
            assert_eq!(consumed, 1);
            assert_eq!((cpu.pc, cpu.gpr[2], cpu.gpr[3]), (0x6000, 0x6200, 0xABCD));
            cpu.pc = RETURN_PC;
            assert!(
                calls.complete_powerpc_resuming_load(&mut cpu, &mut manager, |op, _| {
                    assert_eq!(op, operation(1));
                    assert!(calls.is_empty());
                    0
                })
            );
            assert!(calls.is_empty());
        }
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
    fn initial_native_transition_owns_its_classic_caller_and_refuses_duplicate_contexts() {
        for occupied in [false, true] {
            let calls = SharedGuestCallStack::default();
            assert!(calls.begin_m68k_to_powerpc(
                GuestCallTarget {
                    isa: GuestIsa::PowerPc,
                    entry: 0x1000,
                    rtoc: 0x2000
                },
                PowerPcArguments::from_slice(&[]).unwrap(),
                0x3000,
                0x4000,
                None
            ));
            let (call, _) = calls.top_frame().unwrap();
            let bank = calls.m68k_context_bank();
            if occupied {
                assert!(calls
                    .park_context(
                        &mut bank.borrow_mut(),
                        ExecutionTaskId::APPLICATION,
                        call,
                        M68kCpu::new()
                    )
                    .is_ok());
            }
            let mut classic = M68kCpu::new();
            classic.core.set_a(7, 0x9876);
            classic.core.set_d(6, 0xabcdef);
            let mut native = PpcCpu::new();
            native.pc = 0x5000;
            native.gpr[3] = 99;
            let activated =
                calls.activate_powerpc_with_classic_caller(&mut native, &mut classic, RETURN_PC);
            if occupied {
                assert!(activated.is_none());
                assert_eq!(classic.core.a(7), 0x9876);
                assert_eq!(classic.core.d(6), 0xabcdef);
                assert_eq!(native.pc, 0x5000);
                assert_eq!(native.gpr[3], 99);
                assert!(calls.pending_powerpc_from_m68k().is_some());
                assert!(calls.0.borrow().powerpc_contexts.is_empty());
                assert_eq!(bank.borrow().len(), 1);
                continue;
            }
            assert!(activated.is_some());
            assert!(bank.borrow().contains(ExecutionTaskId::APPLICATION, call));
            assert!(calls
                .0
                .borrow()
                .powerpc_contexts
                .contains(ExecutionTaskId::APPLICATION, call));
            classic.core.set_a(7, 0x1111);
            assert!(calls
                .activate_powerpc_with_classic_caller(&mut native, &mut classic, RETURN_PC)
                .is_none());
            assert_eq!(classic.core.a(7), 0x1111);
            native.pc = RETURN_PC;
            assert!(calls.complete_powerpc_for_m68k(&mut native));
            let restored = calls
                .commit_m68k_resume(|_, context| {
                    let context = context.expect("the initial classic caller must be parked");
                    assert_eq!(context.core.a(7), 0x9876);
                    assert_eq!(context.core.d(6), 0xabcdef);
                    true
                })
                .unwrap()
                .unwrap();
            assert_eq!(restored.core.a(7), 0x9876);
            assert!(bank.borrow().is_empty());
            assert!(calls.is_empty());
        }
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
    fn thread_stack_space_uses_original_callers_through_nested_isa_transitions() {
        use crate::thread_manager::ThreadManager;
        for entry in [GuestIsa::M68k, GuestIsa::PowerPc] {
            let calls = SharedGuestCallStack::default();
            assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, entry));
            let mut classic = M68kCpu::new();
            classic.core.set_a(7, 0x9000);
            let mut native = PpcCpu::new();
            native.gpr[1] = 0x9000;
            let enter_native =
                |calls: &SharedGuestCallStack, classic: &mut M68kCpu, native: &mut PpcCpu| {
                    assert!(calls.begin_m68k_to_powerpc(
                        GuestCallTarget {
                            isa: GuestIsa::PowerPc,
                            entry: 0x1000,
                            rtoc: 0x2000
                        },
                        PowerPcArguments::from_slice(&[]).unwrap(),
                        0x3000,
                        0x6004,
                        None
                    ));
                    calls
                        .activate_powerpc_with_classic_caller(native, classic, RETURN_PC)
                        .unwrap();
                };
            let enter_classic =
                |calls: &SharedGuestCallStack, classic: &mut M68kCpu, native: &PpcCpu| {
                    assert!(calls.begin_powerpc_to_m68k(
                        GuestCallTarget {
                            isa: GuestIsa::M68k,
                            entry: 0x5000,
                            rtoc: 0
                        },
                        0x5000,
                        0x6000,
                        0x7000,
                        0x6004,
                        M68kRegisterState::default(),
                        None,
                        0x8000,
                        0x2000,
                        PpcNativeReturnGpr3::Preserve
                    ));
                    calls.activate_m68k_parking(classic, native).unwrap();
                    classic.core.set_a(7, 0x6000);
                };
            if entry == GuestIsa::M68k {
                enter_native(&calls, &mut classic, &mut native);
            }
            enter_classic(&calls, &mut classic, &native);
            let manager = ThreadManager::new(&calls);
            assert_eq!(
                manager.stack_space(1, GuestIsa::M68k, 0x6000, |_| 0x8000),
                Ok(0x1000)
            );
            enter_native(&calls, &mut classic, &mut native);
            native.gpr[1] = 0x8800;
            let expected = if entry == GuestIsa::M68k {
                0x1000
            } else {
                0x800
            };
            assert_eq!(
                manager.stack_space(1, GuestIsa::PowerPc, native.gpr[1], |_| 0x8000),
                Ok(expected)
            );
            enter_classic(&calls, &mut classic, &native);
            let depth = calls.len();
            assert_eq!(
                manager.stack_space(1, GuestIsa::M68k, 0x6000, |_| 0x8000),
                Ok(expected)
            );
            let other = calls.create_task().unwrap();
            assert!(calls.set_scheduling_state(other, ExecutionTaskState::Ready));
            assert!(calls.switch_to_task(other));
            assert_eq!(
                manager.stack_space(2, GuestIsa::M68k, 0x1234, |_| 0x8000),
                Ok(expected)
            );
            assert_eq!(calls.len(), depth);
        }
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
    fn classic_callback_activation_retains_native_context_until_a_valid_return() {
        for occupied in [false, true] {
            let calls = SharedGuestCallStack::default();
            assert!(calls.begin_powerpc_to_m68k(
                GuestCallTarget {
                    isa: GuestIsa::M68k,
                    entry: 0x2000,
                    rtoc: 0
                },
                0x2000,
                0x3000,
                0x4000,
                0x3004,
                M68kRegisterState::default(),
                Some(M68kResultSource::Data(0)),
                0x5000,
                0x6000,
                PpcNativeReturnGpr3::Preserve
            ));
            let (call, _) = calls.top_frame().unwrap();
            let mut native = PpcCpu::new();
            native.gpr[1] = 0x9000;
            native.gpr[20] = 0xabcdef;
            native.fpr[20] = 0x400921fb54442d18;
            native.cr = 0x12345678;
            native.set_time_base(7);
            if occupied {
                let kernel = calls.0.borrow().kernel.shared_handle();
                assert!(calls
                    .0
                    .borrow_mut()
                    .powerpc_contexts
                    .park(
                        &kernel,
                        ExecutionTaskId::APPLICATION,
                        call,
                        Box::new(native.clone())
                    )
                    .is_ok());
            }
            let mut classic = M68kCpu::new();
            classic.core.set_d(6, 0x7777);
            let activated = calls.activate_m68k_parking(&mut classic, &native);
            if occupied {
                assert!(activated.is_none());
                assert_eq!(classic.core.d(6), 0x7777);
                assert!(calls.active_m68k().is_none());
                continue;
            }
            assert!(activated.is_some());
            assert!(calls
                .0
                .borrow()
                .powerpc_contexts
                .contains(ExecutionTaskId::APPLICATION, call));
            native.gpr[1] = 0x1111;
            native.gpr[20] = 0;
            native.fpr[20] = 0;
            native.cr = 0;
            native.set_time_base(44);
            assert!(!calls.complete_m68k_for_powerpc(0x4000, 0x3000, Some(42), &mut native));
            assert_eq!(native.gpr[1], 0x1111);
            assert!(calls
                .0
                .borrow()
                .powerpc_contexts
                .contains(ExecutionTaskId::APPLICATION, call));
            assert!(calls.complete_m68k_for_powerpc(0x4000, 0x3004, Some(42), &mut native));
            assert_eq!(
                (native.gpr[1], native.gpr[2], native.gpr[3]),
                (0x9000, 0x6000, 42)
            );
            assert_eq!(native.gpr[20], 0xabcdef);
            assert_eq!(native.fpr[20], 0x400921fb54442d18);
            assert_eq!(native.cr, 0x12345678);
            assert_eq!(native.time_base(), 44);
            assert!(calls.0.borrow().powerpc_contexts.is_empty());
            assert!(calls.is_empty());
        }
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
