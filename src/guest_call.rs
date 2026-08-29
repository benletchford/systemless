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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PowerPcCallOrigin {
    return_pc: u32,
    final_pc: u32,
    restore_rtoc: u32,
    return_gpr3: PpcNativeReturnGpr3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuestCallOrigin {
    M68k(M68kCallOrigin),
    PowerPc(PowerPcCallOrigin),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GuestCallFrame {
    target: GuestCallTarget,
    origin: GuestCallOrigin,
}

/// One process's nested guest-procedure continuation stack.
///
/// Ordinary `Clone` creates an independent process snapshot. The runner uses
/// [`Self::shared_handle`] explicitly when it attaches two CPU adapters to the
/// same live process.
#[derive(Debug, Default)]
pub(crate) struct SharedGuestCallStack(Rc<RefCell<Vec<GuestCallFrame>>>);

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
        let pending = std::mem::take(&mut *self.0.borrow_mut());
        debug_assert!(
            pending.is_empty() || process_calls.is_empty(),
            "cannot attach two active guest-procedure continuation stacks"
        );
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
            }),
        });
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
            let frames = self.0.borrow();
            let Some(frame) = frames.last().copied() else {
                return false;
            };
            let GuestCallOrigin::PowerPc(origin) = frame.origin else {
                return false;
            };
            if frame.target.isa != GuestIsa::PowerPc || cpu.pc != origin.return_pc {
                return false;
            }
            frame
        };
        self.0.borrow_mut().pop();

        let GuestCallOrigin::PowerPc(origin) = frame.origin else {
            unreachable!();
        };
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
        true
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
