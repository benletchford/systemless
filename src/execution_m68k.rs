//! Concrete 68K execution ownership and Mixed Mode transitions.
//!
//! This owner is deliberately not Clone: live CPU caches and memory bindings
//! must move with their context. The task store remains the process authority.

use crate::cpu::{M68kCpu, Register};
use crate::guest_call::{PendingM68kExecution, SharedGuestCallStack};
use crate::memory::GuestAddressSpace as PpcSectionMem;
use ppc::PpcMemory;

/// Stack-local Pascal MDEF lowering shared by classic and native callers.
/// Code lives above the callback SP, so nested calls cannot overwrite it.
/// Macintosh Toolbox Essentials (1992), pp. 3-148--3-151.
pub(crate) struct M68kMenuDefinitionFrame {
    pub(crate) entry: u32,
    pub(crate) image: [u8; 60],
}

impl M68kMenuDefinitionFrame {
    /// Emit an internal A-line return boundary inside the live callback frame.
    /// Execution matches its PC/SP before guest trap routing; the retained
    /// operation supplies the caller return instead of repeating guest entry.
    pub(crate) fn trap_return(&mut self, opcode: u16) -> u32 {
        self.image[52..54].copy_from_slice(&opcode.to_be_bytes());
        self.entry + 52
    }

    pub(crate) const RESERVATION: u32 = 80;
    pub(crate) const STACK_PREFIX: u32 = 58;

    pub(crate) fn new(
        call: crate::menu_manager::MenuDefinitionCall,
        target: u32,
        final_sp: u32,
        release_stack: bool,
    ) -> Option<Self> {
        let entry = final_sp.checked_sub(Self::RESERVATION)?;
        entry.checked_sub(Self::STACK_PREFIX)?;
        if entry & 1 != 0 {
            return None;
        }
        let mut image = [0; 60];
        for (offset, value) in [
            (0, 0x48e7u16),
            (2, 0xf0f0), // MOVEM D0-D3/A0-A3,-(SP)
            (4, 0x3f3c),
            (6, call.message as i16 as u16),
            (8, 0x2f3c),
            (14, 0x2f3c),
            (20, 0x2f3c),
            (26, 0x2f3c),
            (32, 0x4eb9), // JSR target
            (38, 0x2e7c), // restore saved-register SP
            (44, 0x4cdf),
            (46, 0x0f0f),
            // RTD changes PC and SP in one instruction. An interrupt must
            // never see SP above code which the foreground will still fetch.
            (48, 0x4e74),
            (50, if release_stack { Self::RESERVATION as u16 } else { 0 }),
        ] {
            image[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        for (offset, value) in [
            (10, call.menu_handle),
            (16, call.menu_rect),
            (22, call.hit_point),
            (28, call.which_item),
            (34, target),
            (40, entry - 36),
        ] {
            image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        Some(Self { entry, image })
    }
}

/// Stack-local no-argument callback frame. Saved registers and code remain
/// below the caller SP until execution retires the exact internal return.
/// MenuHook's Pascal no-argument contract: Macintosh Toolbox Essentials
/// (1992), p. 3-116.
pub(crate) struct M68kMenuHookFrame {
    pub(crate) entry: u32,
    pub(crate) image: [u8; 32],
}

impl M68kMenuHookFrame {
    pub(crate) fn new(target: u32, caller_sp: u32) -> Option<Self> {
        let entry = caller_sp.checked_sub(48)?;
        let saved_sp = entry.checked_sub(66)?;
        if entry & 1 != 0 {
            return None;
        }
        let mut image = [0; 32];
        for (offset, word) in [
            (0, 0x40e7u16), // MOVE SR,-(SP)
            (2, 0x48e7),
            (4, 0xfffe),  // MOVEM all D/A except SP
            (6, 0x4eb9),  // JSR target
            (12, 0x2e7c), // MOVEA.L #saved-register SP,A7
            (18, 0x4cdf),
            (20, 0x7fff), // restore all D/A except SP
            (22, 0x46df), // MOVE (SP)+,SR
            (24, 0x4e74),
            (26, 0), // RTD into the internal boundary
            (28, 0xa93d),
            (30, 0x4e71),
        ] {
            image[offset..offset + 2].copy_from_slice(&word.to_be_bytes());
        }
        image[8..12].copy_from_slice(&target.to_be_bytes());
        image[14..18].copy_from_slice(&saved_sp.to_be_bytes());
        Some(Self { entry, image })
    }
}

/// Consume a manager result while its stack reservation is still live, then
/// restore the caller ABI before trap/vector lookup can invoke more guest code.
pub(crate) fn complete_classic_manager_return<C: crate::cpu::CpuOps>(
    calls: &SharedGuestCallStack,
    cpu: &mut C,
    bus: &crate::memory::MacMemoryBus,
) -> bool {
    use crate::memory::MemoryBus;
    let restored_sp = calls.complete_m68k_with_operation(
        cpu.read_reg(Register::PC),
        cpu.read_reg(Register::A7),
        |operation| {
            let crate::guest_call::ManagerContinuation::Menu(operation) = operation else {
                panic!("classic manager return has no completion consumer");
            };
            let result = if bus.is_guest_address_mapped(operation.scratch, 10) {
                <[u8; 10]>::try_from(bus.read_bytes(operation.scratch, 10))
                    .map(crate::menu_manager::MenuDefinitionInvocation::decode_result)
                    .map_err(|_| ())
            } else {
                Err(())
            };
            operation.complete_result(result);
        },
    );
    if let Some(sp) = restored_sp {
        cpu.write_reg(Register::A7, sp);
        true
    } else {
        false
    }
}

pub(crate) struct M68kExecution {
    pub(crate) cpu: M68kCpu,
    calls: SharedGuestCallStack,
}

impl M68kExecution {
    pub(crate) fn new(calls: &SharedGuestCallStack) -> Self {
        Self {
            cpu: M68kCpu::new(),
            calls: calls.shared_handle(),
        }
    }

    pub(crate) fn complete_manager_return(&mut self, bus: &crate::memory::MacMemoryBus) -> bool {
        complete_classic_manager_return(&self.calls, &mut self.cpu, bus)
    }

    pub(crate) fn apply_task_handoff(&mut self) {
        if let Some(context) = self.calls.take_classic_task_handoff() {
            context.install(&mut self.cpu);
        }
    }

    /// A launch cannot discard contexts still associated with live calls.
    pub(crate) fn can_relaunch(&self) -> bool {
        self.calls.current_task_is_running()
            && !self.calls.has_parked_m68k_contexts()
            && self.calls.is_empty()
            && !self.calls.has_live_workers()
            && !self.calls.has_pending_task_handoff()
    }

    /// Validate and park the outgoing context before installing the callback.
    pub(crate) fn activate_pending(
        &mut self,
        native: &ppc::PpcCpu,
    ) -> Option<PendingM68kExecution> {
        if let Some(active) = self.calls.active_m68k() {
            return Some(active);
        }
        let pending = self.calls.activate_m68k_parking(&mut self.cpu, native)?;
        for (index, value) in pending.registers.data.into_iter().enumerate() {
            self.cpu.core.set_d(index, value);
        }
        for (index, value) in pending.registers.address.into_iter().enumerate() {
            self.cpu.core.set_a(index, value);
        }
        self.cpu.write_reg(Register::A7, pending.initial_sp);
        self.cpu.write_reg(Register::PC, pending.entry);
        Some(pending)
    }

    /// Apply a completed native result to its actual caller, then restore it.
    pub(crate) fn resume_after_powerpc(&mut self, memory: &mut PpcSectionMem) -> bool {
        let Some(parked) = self.calls.commit_m68k_resume(|resume, parked| {
            let cpu = parked.unwrap_or(&mut self.cpu);
            if !Self::apply_m68k_resume_result(cpu, memory, resume) {
                return false;
            }
            cpu.write_reg(Register::PC, resume.return_pc);
            cpu.write_reg(Register::A7, resume.final_sp);
            true
        }) else {
            return false;
        };
        if let Some(parked) = parked {
            self.cpu = parked;
        }
        true
    }

    pub(crate) fn apply_m68k_resume_result(
        cpu: &mut M68kCpu,
        memory: &mut PpcSectionMem,
        resume: crate::guest_call::M68kResume,
    ) -> bool {
        use crate::guest_call::M68kResultTarget;

        let mask_value = |value: u32, size: u8| match size {
            1 => Some(value & 0xff),
            2 => Some(value & 0xffff),
            4 => Some(value),
            _ => None,
        };
        match resume.result {
            None => true,
            Some(M68kResultTarget::Data { index, size }) if index < 8 => {
                mask_value(resume.powerpc.gpr3, size).is_some_and(|value| {
                    cpu.core.set_d(usize::from(index), value);
                    true
                })
            }
            Some(M68kResultTarget::Address { index, size }) if index < 8 => {
                mask_value(resume.powerpc.gpr3, size).is_some_and(|value| {
                    cpu.core.set_a(usize::from(index), value);
                    true
                })
            }
            Some(M68kResultTarget::Ccr { mask }) => {
                // Native return values always arrive in R3; Mixed Mode then
                // copies that value to the ProcInfo-selected 68k destination,
                // including a CCR bit. Inside Macintosh: PowerPC System
                // Software (1994), pp. 2-10--2-12.
                let set = resume.powerpc.gpr3 != 0;
                let ccr = (cpu.core.get_ccr() & !mask) | if set { mask } else { 0 };
                cpu.core.set_ccr(ccr);
                true
            }
            Some(M68kResultTarget::Memory { address, size }) => {
                mask_value(resume.powerpc.gpr3, size).is_some_and(|value| match size {
                    1 => memory.write_u8(address, value as u8).is_some(),
                    2 => memory.write_u16_be(address, value as u16).is_some(),
                    4 => memory.write_u32_be(address, value).is_some(),
                    _ => false,
                })
            }
            Some(M68kResultTarget::SpecialCase { selector, scratch }) => {
                Self::apply_m68k_special_case_result(
                    cpu,
                    memory,
                    selector,
                    scratch,
                    resume.powerpc.gpr3,
                )
            }
            _ => false,
        }
    }

    pub(crate) fn apply_m68k_special_case_result(
        cpu: &mut M68kCpu,
        memory: &mut PpcSectionMem,
        selector: u8,
        scratch: u32,
        native_result: u32,
    ) -> bool {
        use crate::mixed_mode::special_case;

        let set_data_word = |cpu: &mut crate::cpu::M68kCpu, index: usize, value: u32| {
            let preserved = cpu.core.d(index) & 0xffff_0000;
            cpu.core.set_d(index, preserved | (value & 0xffff));
        };
        let set_z = |cpu: &mut crate::cpu::M68kCpu, value: bool| {
            let ccr = (cpu.core.get_ccr() & !0x04) | if value { 0x04 } else { 0 };
            cpu.core.set_ccr(ccr);
        };

        match u32::from(selector) {
            special_case::EOL_HOOK
            | special_case::PROTOCOL_HANDLER
            | special_case::SOCKET_LISTENER => {
                set_z(cpu, native_result & 0xff != 0);
                true
            }
            special_case::WIDTH_HOOK | special_case::NWIDTH_HOOK => {
                set_data_word(cpu, 1, native_result);
                true
            }
            special_case::HIT_TEST_HOOK => {
                let Some(pixel_width) = memory.read_u16_be(scratch) else {
                    return false;
                };
                let Some(char_offset) = scratch
                    .checked_add(2)
                    .and_then(|address| memory.read_u16_be(address))
                else {
                    return false;
                };
                let Some(pixel_in_char) = scratch
                    .checked_add(4)
                    .and_then(|address| memory.read_u8(address))
                else {
                    return false;
                };
                cpu.core
                    .set_d(0, ((native_result & 0xff) << 16) | u32::from(pixel_width));
                set_data_word(cpu, 1, u32::from(char_offset));
                set_data_word(cpu, 2, u32::from(pixel_in_char));
                true
            }
            special_case::TE_FIND_WORD => {
                let Some(word_start) = memory.read_u16_be(scratch) else {
                    return false;
                };
                let Some(word_end) = scratch
                    .checked_add(2)
                    .and_then(|address| memory.read_u16_be(address))
                else {
                    return false;
                };
                set_data_word(cpu, 0, u32::from(word_start));
                set_data_word(cpu, 1, u32::from(word_end));
                true
            }
            special_case::TE_RECALC => {
                let Some(line_start) = memory.read_u16_be(scratch) else {
                    return false;
                };
                let Some(first_char) = scratch
                    .checked_add(2)
                    .and_then(|address| memory.read_u16_be(address))
                else {
                    return false;
                };
                let Some(last_char) = scratch
                    .checked_add(4)
                    .and_then(|address| memory.read_u16_be(address))
                else {
                    return false;
                };
                set_data_word(cpu, 2, u32::from(line_start));
                set_data_word(cpu, 3, u32::from(first_char));
                set_data_word(cpu, 4, u32::from(last_char));
                true
            }
            special_case::TE_DO_TEXT => {
                let Some(current_graf_port) = memory.read_u32_be(scratch) else {
                    return false;
                };
                let Some(char_position) = scratch
                    .checked_add(4)
                    .and_then(|address| memory.read_u16_be(address))
                else {
                    return false;
                };
                cpu.core.set_a(0, current_graf_port);
                set_data_word(cpu, 0, u32::from(char_position));
                true
            }
            special_case::MBAR_HOOK => {
                set_data_word(cpu, 0, native_result);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn complete_pending(
        &mut self,
        memory: &mut PpcSectionMem,
        native: &mut ppc::PpcCpu,
        pending: crate::guest_call::PendingM68kExecution,
        manager: &mut crate::process_context::ProcessNativeMemoryManager,
    ) -> bool {
        use crate::guest_call::M68kResultSource;

        let result = match pending.result {
            None => None,
            Some(M68kResultSource::Data(index)) => Some(self.cpu.core.d(usize::from(index))),
            Some(M68kResultSource::Address(index)) => Some(self.cpu.core.a(usize::from(index))),
            Some(M68kResultSource::Memory { address, size }) => {
                let value = match size {
                    1 => memory.read_u8(address).map(u32::from),
                    2 => memory.read_u16_be(address).map(u32::from),
                    4 => memory.read_u32_be(address),
                    _ => None,
                };
                let Some(value) = value else {
                    return false;
                };
                Some(value)
            }
            Some(M68kResultSource::SpecialCase {
                selector,
                arguments,
                stack_result,
            }) => {
                let Ok(value) = self.complete_m68k_special_case_result(
                    memory,
                    selector,
                    arguments,
                    stack_result,
                ) else {
                    return false;
                };
                value
            }
        };
        self.calls.complete_m68k_operation_for_powerpc(
            pending.return_pc,
            self.cpu.read_reg(Register::A7),
            result,
            native,
            memory,
            manager,
        )
    }

    pub(crate) fn complete_m68k_special_case_result(
        &mut self,
        memory: &mut PpcSectionMem,
        selector: u8,
        arguments: crate::guest_call::PowerPcArguments,
        stack_result: Option<u32>,
    ) -> std::result::Result<Option<u32>, ()> {
        use crate::mixed_mode::special_case;

        let arguments = arguments.as_slice();
        let proc_info = crate::mixed_mode::proc_info::SPECIAL_CASE
            | (u32::from(selector) << special_case::SELECTOR_PHASE);
        let signature = crate::mixed_mode::native_special_case_signature(proc_info).ok_or(())?;
        if arguments.len() != signature.argument_count {
            return Err(());
        }
        let z = u32::from(self.cpu.core.get_ccr() & 0x04 != 0);
        match u32::from(selector) {
            // A void callback has no native return value. Preserve the
            // caller's PPC R3 rather than manufacturing zero and treating it
            // as a result to copy back through the cross-ISA frame.
            special_case::HIGH_HOOK | special_case::DRAW_HOOK => Ok(None),
            special_case::EOL_HOOK
            | special_case::PROTOCOL_HANDLER
            | special_case::SOCKET_LISTENER => Ok(Some(z)),
            special_case::WIDTH_HOOK | special_case::NWIDTH_HOOK => {
                Ok(Some(self.cpu.core.d(1) & 0xffff))
            }
            special_case::HIT_TEST_HOOK => {
                if ![(arguments[6], 2), (arguments[7], 2), (arguments[8], 1)]
                    .into_iter()
                    .all(|(address, size)| memory.preflight_writable_range(address, size))
                {
                    return Err(());
                }
                memory
                    .write_u16_be(arguments[6], self.cpu.core.d(0) as u16)
                    .ok_or(())?;
                memory
                    .write_u16_be(arguments[7], self.cpu.core.d(1) as u16)
                    .ok_or(())?;
                memory
                    .write_u8(arguments[8], self.cpu.core.d(2) as u8)
                    .ok_or(())?;
                Ok(Some((self.cpu.core.d(0) >> 16) & 0xff))
            }
            special_case::TE_FIND_WORD => {
                if ![(arguments[4], 2), (arguments[5], 2)]
                    .into_iter()
                    .all(|(address, size)| memory.preflight_writable_range(address, size))
                {
                    return Err(());
                }
                memory
                    .write_u16_be(arguments[4], self.cpu.core.d(0) as u16)
                    .ok_or(())?;
                memory
                    .write_u16_be(arguments[5], self.cpu.core.d(1) as u16)
                    .ok_or(())?;
                Ok(None)
            }
            special_case::TE_RECALC => {
                if !arguments[2..5]
                    .iter()
                    .copied()
                    .all(|address| memory.preflight_writable_range(address, 2))
                {
                    return Err(());
                }
                for (argument, register) in arguments[2..5].iter().copied().zip(2..=4) {
                    memory
                        .write_u16_be(argument, self.cpu.core.d(register) as u16)
                        .ok_or(())?;
                }
                Ok(None)
            }
            special_case::TE_DO_TEXT => {
                if ![(arguments[4], 4), (arguments[5], 2)]
                    .into_iter()
                    .all(|(address, size)| memory.preflight_writable_range(address, size))
                {
                    return Err(());
                }
                memory
                    .write_u32_be(arguments[4], self.cpu.core.a(0))
                    .ok_or(())?;
                memory
                    .write_u16_be(arguments[5], self.cpu.core.d(0) as u16)
                    .ok_or(())?;
                Ok(None)
            }
            special_case::GNE_FILTER_PROC => {
                let result = memory.read_u16_be(stack_result.ok_or(())?).ok_or(())?;
                if !memory.preflight_writable_range(arguments[1], 1) {
                    return Err(());
                }
                memory.write_u8(arguments[1], result as u8).ok_or(())?;
                Ok(None)
            }
            special_case::MBAR_HOOK => Ok(Some(self.cpu.core.d(0) & 0xffff)),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_call::GuestCallTarget;
    use crate::guest_procedure::GuestIsa;

    #[test]
    fn native_task_handoff_restores_classic_status_fpu_and_frame_state() {
        use crate::cpu::{CpuOps, StepResult};
        use crate::guest_call::{CooperativeThread, ExecutionTaskId, ThreadStorage};
        use crate::memory::{MacMemoryBus, MemoryBus};

        for just_reset in [false, true] {
            let calls = SharedGuestCallStack::default();
            calls.start_native_engine();
            assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::PowerPc));
            let mut engine = M68kExecution::new(&calls);
            engine.cpu.core.set_sr(0x250a);
            engine.cpu.core.fpr = std::array::from_fn(|i| m68k::fpu::FloatX80 {
                mantissa: 0x8000_0000_0000_0021 + i as u64,
                sign_exp: 0xffff,
            });
            engine.cpu.core.fpcr = 0x20;
            engine.cpu.core.fpsr = 0x0800_0000;
            engine.cpu.core.fpiar = 0x0010_1000;
            engine.cpu.core.fpu_just_reset = just_reset;
            engine.cpu.write_reg(Register::PC, 0x0010_0000);
            engine.cpu.write_reg(Register::A0, 0x0010_2000);
            engine.cpu.write_reg(Register::A7, 0x0010_8000);
            let mut saved = CooperativeThread::capture(&engine.cpu);
            // A result may update CCR after the extended snapshot was taken.
            saved.ccr = 0x11;
            let worker = calls
                .create_classic_thread(
                    saved.clone(),
                    ThreadStorage {
                        stack_base: 0x0010_4000,
                        stack_limit: 0x0010_9000,
                        ..Default::default()
                    },
                    false,
                    |_| true,
                )
                .unwrap();

            engine.cpu.core.set_sr(0);
            engine.cpu.core.fpr.fill(Default::default());
            engine.cpu.core.fpcr = 0;
            engine.cpu.core.fpsr = 0;
            engine.cpu.core.fpiar = 0;
            engine.cpu.core.fpu_just_reset = !just_reset;
            engine.cpu.write_reg(Register::A7, 0x0010_3000);
            let untouched = CooperativeThread::capture(&engine.cpu);
            let mut native = ppc::PpcCpu::new();
            native.lr = 0x1234;
            assert_eq!(
                calls.yield_native_thread(&mut native, worker.thread_id()),
                Ok(true)
            );
            assert_eq!(CooperativeThread::capture(&engine.cpu), untouched);
            assert!(calls.has_classic_task_handoff());
            engine.apply_task_handoff();
            assert!(!calls.has_classic_task_handoff());
            assert_eq!(engine.cpu.core.get_sr(), 0x2511);
            assert_eq!(engine.cpu.core.a(7), saved.a_regs[7]);
            let mut expected = saved.extended.unwrap();
            expected.sr = 0x2511;
            assert_eq!(engine.cpu.capture_extended_context(), Some(expected));

            // Check the frame through a guest FSAVE instruction, not just
            // the implementation's null/idle bookkeeping bit.
            let mut bus = MacMemoryBus::new(0x400000);
            bus.write_word(0x0010_0000, 0xf310); // FSAVE (A0)
            bus.write_long(0x0010_2000, 0xdead_beef);
            assert!(matches!(engine.cpu.step(&mut bus), StepResult::Ok));
            assert_eq!(bus.read_long(0x0010_2000) == 0, just_reset);
            let after = CooperativeThread::capture(&engine.cpu);
            engine.apply_task_handoff();
            assert_eq!(CooperativeThread::capture(&engine.cpu), after);
        }
    }

    #[test]
    fn relaunch_waits_for_the_bound_process_calls_to_finish() {
        let calls = SharedGuestCallStack::default();
        let mut engine = M68kExecution::new(&calls);
        engine.cpu.core.set_d(0, 42);
        assert!(engine.can_relaunch());
        assert!(calls.begin_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000
        ));
        assert!(!engine.can_relaunch());
        assert_eq!(engine.cpu.core.d(0), 42);
        assert!(calls.complete_m68k(0x2002, 0x3000));
        assert!(engine.can_relaunch());
        assert_eq!(engine.cpu.core.d(0), 42);
    }
}
