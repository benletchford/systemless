//! Process-owned guest-procedure continuation frames.
//!
//! Mixed Mode resolves a universal procedure pointer, constructs the target
//! architecture's calling sequence, and returns through a switch frame owned
//! by the calling process. Inside Macintosh: PowerPC System Software (1994),
//! pp. 1-15--1-17 and 2-4--2-12. Keeping that continuation above either CPU
//! adapter lets nested 68k and native PowerPC callbacks share one LIFO owner
//! while each adapter remains responsible for its architectural registers and
//! ABI frame.

use crate::guest_procedure::GuestIsa;
use ppc::{PpcCpu, PpcImportAction, PpcNativeReturnGpr3};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GuestCallTarget {
    pub(crate) isa: GuestIsa,
    pub(crate) entry: u32,
    pub(crate) rtoc: u32,
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
    return_gpr3: PpcNativeReturnGpr3,
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
    parked_cpu: Option<Box<PpcCpu>>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingM68kExecution {
    pub(crate) entry: u32,
    pub(crate) initial_sp: u32,
    pub(crate) return_pc: u32,
    pub(crate) final_sp: u32,
    pub(crate) registers: M68kRegisterState,
    pub(crate) result: Option<M68kResultSource>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerPcReturnState {
    pub(crate) gpr3: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingPowerPcExecution {
    pub(crate) target: GuestCallTarget,
    pub(crate) arguments: PowerPcArguments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M68kResume {
    pub(crate) return_pc: u32,
    pub(crate) final_sp: u32,
    pub(crate) result: Option<M68kResultTarget>,
    pub(crate) powerpc: PowerPcReturnState,
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
                        && left.parked_cpu.is_some() == right.parked_cpu.is_some()
                        && left.completed == right.completed
                }
                _ => false,
            }
    }
}

impl Eq for GuestCallFrame {}

/// One process's nested guest-procedure continuation stack.
///
/// Ordinary `Clone` creates an independent process snapshot. The runner's
/// process context explicitly attaches both CPU adapters to the same live
/// allocation.
#[derive(Debug, Default)]
pub(crate) struct SharedGuestCallStack(Rc<RefCell<Vec<GuestCallFrame>>>);

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
    pub(crate) fn shared_handle(&self) -> Self {
        Self(Rc::clone(&self.0))
    }

    pub(crate) fn attach_to(&mut self, process_calls: &Self) {
        if Rc::ptr_eq(&self.0, &process_calls.0) {
            return;
        }
        assert!(
            self.is_empty() || process_calls.is_empty(),
            "cannot attach two active guest-procedure continuation stacks"
        );
        let pending = std::mem::take(&mut *self.0.borrow_mut());
        self.0 = Rc::clone(&process_calls.0);
        self.0.borrow_mut().extend(pending);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub(crate) fn begin_m68k(&self, target: GuestCallTarget, return_pc: u32, final_sp: u32) {
        debug_assert_eq!(target.isa, GuestIsa::M68k);
        self.0.borrow_mut().push(GuestCallFrame {
            target,
            origin: GuestCallOrigin::M68k(M68kCallOrigin {
                return_pc,
                final_sp,
                result: None,
            }),
            m68k_execution: None,
            powerpc_execution: None,
        });
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
        self.0.borrow_mut().push(GuestCallFrame {
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
                parked_cpu: None,
                completed: None,
            }),
        });
        true
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
        return_gpr3: PpcNativeReturnGpr3,
    ) -> bool {
        debug_assert_eq!(target.isa, GuestIsa::M68k);
        self.0.borrow_mut().push(GuestCallFrame {
            target,
            origin: GuestCallOrigin::PowerPc(PowerPcCallOrigin {
                return_pc,
                final_pc,
                restore_rtoc,
                return_gpr3,
            }),
            m68k_execution: Some(M68kExecution {
                entry,
                initial_sp,
                return_pc,
                final_sp,
                registers,
                result,
                started: false,
            }),
            powerpc_execution: None,
        });
        true
    }

    pub(crate) fn pending_powerpc_from_m68k(&self) -> Option<PendingPowerPcExecution> {
        let frames = self.0.borrow();
        let frame = frames.last()?;
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
        let mut frames = self.0.borrow_mut();
        let frame = frames.last_mut()?;
        let GuestCallOrigin::M68k(_) = frame.origin else {
            return None;
        };
        let execution = frame.powerpc_execution.as_mut()?;
        if execution.return_pc.is_some() || execution.completed.is_some() {
            return None;
        }
        execution.parked_cpu = Some(Box::new(cpu.clone()));
        execution.return_pc = Some(return_pc);
        Some(PendingPowerPcExecution {
            target: frame.target,
            arguments: execution.arguments,
        })
    }

    pub(crate) fn has_powerpc_from_m68k(&self) -> bool {
        self.0.borrow().last().is_some_and(|frame| {
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
    pub(crate) fn suspended_m68k_context_depth(&self) -> usize {
        self.0
            .borrow()
            .iter()
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
        let mut frames = self.0.borrow_mut();
        let Some(frame) = frames.last_mut() else {
            return false;
        };
        let GuestCallOrigin::M68k(_) = frame.origin else {
            return false;
        };
        let Some(execution) = frame.powerpc_execution.as_mut() else {
            return false;
        };
        if execution.completed.is_some() || execution.return_pc != Some(cpu.pc) {
            return false;
        }
        let Some(parked_cpu) = execution.parked_cpu.take() else {
            return false;
        };
        let result = PowerPcReturnState { gpr3: cpu.gpr[3] };
        let elapsed_time_base = cpu.time_base();
        *cpu = *parked_cpu;
        cpu.set_time_base(elapsed_time_base);
        execution.completed = Some(result);
        true
    }

    pub(crate) fn take_m68k_resume(&self) -> Option<M68kResume> {
        let mut frames = self.0.borrow_mut();
        let frame = frames.last()?;
        let GuestCallOrigin::M68k(origin) = frame.origin else {
            return None;
        };
        let powerpc = frame.powerpc_execution.as_ref()?.completed?;
        frames.pop();
        Some(M68kResume {
            return_pc: origin.return_pc,
            final_sp: origin.final_sp,
            result: origin.result,
            powerpc,
        })
    }

    /// Return the top cross-ISA 68k interval and mark its CPU context active.
    pub(crate) fn activate_m68k(&self) -> Option<PendingM68kExecution> {
        let mut frames = self.0.borrow_mut();
        let execution = frames.last_mut()?.m68k_execution.as_mut()?;
        let pending = PendingM68kExecution {
            entry: execution.entry,
            initial_sp: execution.initial_sp,
            return_pc: execution.return_pc,
            final_sp: execution.final_sp,
            registers: execution.registers,
            result: execution.result,
        };
        execution.started = true;
        Some(pending)
    }

    pub(crate) fn active_m68k(&self) -> Option<PendingM68kExecution> {
        let frames = self.0.borrow();
        let execution = frames.last()?.m68k_execution?;
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
        self.0
            .borrow()
            .last()
            .is_some_and(|frame| frame.m68k_execution.is_some())
    }

    /// Move the continuation embedded in a CPU action into the process stack
    /// and arrange the next native PowerPC context directly.
    pub(crate) fn externalize_powerpc_action(
        &self,
        cpu: &mut PpcCpu,
        action: PpcImportAction,
    ) -> PpcImportAction {
        let PpcImportAction::CallNative {
            entry,
            rtoc,
            return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3,
        } = action
        else {
            return action;
        };

        let target = GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry,
            rtoc,
        };
        self.0.borrow_mut().push(GuestCallFrame {
            target,
            origin: GuestCallOrigin::PowerPc(PowerPcCallOrigin {
                return_pc,
                final_pc,
                restore_rtoc,
                return_gpr3,
            }),
            m68k_execution: None,
            powerpc_execution: None,
        });
        cpu.pc = target.entry;
        cpu.lr = return_pc;
        cpu.gpr[2] = target.rtoc;
        PpcImportAction::Continue
    }

    /// Complete the top native frame only when the CPU reached its exact
    /// synthetic return import. A frame belonging to 68k remains untouched.
    pub(crate) fn complete_powerpc(&self, cpu: &mut PpcCpu) -> bool {
        let frame = {
            let mut frames = self.0.borrow_mut();
            let Some(frame) = frames.last() else {
                return false;
            };
            let GuestCallOrigin::PowerPc(origin) = frame.origin else {
                return false;
            };
            if frame.target.isa != GuestIsa::PowerPc || cpu.pc != origin.return_pc {
                return false;
            }
            frames.pop().expect("verified PowerPC frame")
        };

        let GuestCallOrigin::PowerPc(origin) = frame.origin else {
            unreachable!();
        };
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
        let frame = {
            let mut frames = self.0.borrow_mut();
            let Some(frame) = frames.last() else {
                return false;
            };
            let GuestCallOrigin::PowerPc(_) = frame.origin else {
                return false;
            };
            let Some(execution) = frame.m68k_execution else {
                return false;
            };
            if !execution.started
                || post_call_pc != execution.return_pc
                || final_sp != execution.final_sp
                || execution.result.is_some() != result.is_some()
            {
                return false;
            }
            frames.pop().expect("verified 68k execution frame")
        };

        let GuestCallOrigin::PowerPc(origin) = frame.origin else {
            unreachable!();
        };
        if let Some(result) = result {
            cpu.gpr[3] = result;
        }
        Self::apply_powerpc_return(cpu, origin);
        true
    }

    fn apply_powerpc_return(cpu: &mut PpcCpu, origin: PowerPcCallOrigin) {
        match origin.return_gpr3 {
            PpcNativeReturnGpr3::Preserve => {}
            PpcNativeReturnGpr3::Mask(mask) => cpu.gpr[3] &= mask,
            PpcNativeReturnGpr3::Set(value) => cpu.gpr[3] = value,
            PpcNativeReturnGpr3::ZeroOrSet { zero, nonzero } => {
                cpu.gpr[3] = if cpu.gpr[3] == 0 { zero } else { nonzero };
            }
            PpcNativeReturnGpr3::CrBit(bit_index) => {
                cpu.gpr[3] = u32::from(cpu.cr_bit(bit_index));
            }
            PpcNativeReturnGpr3::XerCa => cpu.gpr[3] = u32::from(cpu.xer_ca()),
            PpcNativeReturnGpr3::XerOv => cpu.gpr[3] = u32::from(cpu.xer_ov()),
        }
        cpu.gpr[2] = origin.restore_rtoc;
        cpu.lr = origin.final_pc;
        cpu.pc = origin.final_pc;
    }

    /// Complete the top classic frame only after its trampoline restored the
    /// exact caller PC and stack pointer. A native frame remains untouched.
    pub(crate) fn complete_m68k(&self, post_trap_pc: u32, final_sp: u32) -> bool {
        let should_pop = self.0.borrow().last().is_some_and(|frame| {
            frame.target.isa == GuestIsa::M68k
                && matches!(
                    frame.origin,
                    GuestCallOrigin::M68k(origin)
                        if origin.return_pc.wrapping_add(2) == post_trap_pc
                            && origin.final_sp == final_sp
                )
        });
        if should_pop {
            self.0.borrow_mut().pop();
        }
        should_pop
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
        PpcImportAction::CallNative {
            entry,
            rtoc: entry + 0x100,
            return_pc: RETURN_PC,
            final_pc,
            restore_rtoc: final_pc + 0x100,
            return_gpr3,
        }
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
