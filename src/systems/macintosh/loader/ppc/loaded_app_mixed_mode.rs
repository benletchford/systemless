//! PowerPC loaded application mixed mode 68k caller activation methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    /// Park the current native context and enter a PowerPC routine selected by
    /// a 68k RoutineDescriptor. The new ABI frame protects the parked caller's
    /// linkage and parameter areas while the shared continuation owns return.
    pub(crate) fn activate_powerpc_from_m68k(
        &mut self,
        caller: &mut crate::cpu::M68kCpu,
    ) -> Option<()> {
        let pending = self
            .toolbox_startup
            .execution
            .calls()
            .pending_powerpc_from_m68k()?;
        let parameter_slots = pending
            .arguments
            .as_slice()
            .len()
            .max(PPC_NATIVE_PARAMETER_GPR_COUNT);
        let parameter_bytes = u32::try_from(parameter_slots).ok()?.checked_mul(4)?;
        let required = PPC_PARAMETER_AREA_OFFSET.checked_add(parameter_bytes)?;
        let frame_size = required.max(PPC_INITIAL_STACK_FRAME_SIZE).checked_add(15)? & !15;
        let caller_sp = self.cpu.gpr[1];
        let callback_sp = caller_sp.checked_sub(frame_size)? & !15;
        // PowerPC System Software, 1-44–1-49: linkage and parameter areas
        // belong to the caller's grow-down stack, including worker stacks.
        let (stack_base, stack_limit) =
            self.toolbox_startup.execution.calls().native_stack_bounds(
                self.stack_base,
                self.stack_base.checked_add(self.stack_size)?,
            )?;
        if callback_sp < stack_base
            || caller_sp > stack_limit
            || !ppc_memory_can_write_bytes(&mut self.memory, callback_sp, frame_size)
            || !ppc_zero_guest_bytes(&mut self.memory, callback_sp, frame_size)
        {
            return None;
        }
        self.memory
            .write_u32_be(callback_sp + PPC_LINKAGE_BACK_CHAIN_OFFSET, caller_sp)?;
        self.memory
            .write_u32_be(callback_sp + PPC_LINKAGE_SAVED_CR_OFFSET, self.cpu.cr)?;
        self.memory
            .write_u32_be(callback_sp + PPC_LINKAGE_SAVED_LR_OFFSET, self.cpu.lr)?;
        self.memory
            .write_u32_be(callback_sp + PPC_LINKAGE_SAVED_RTOC_OFFSET, self.cpu.gpr[2])?;

        let pending = self
            .toolbox_startup
            .execution
            .calls()
            .activate_powerpc_with_classic_caller(
                &mut self.cpu,
                caller,
                PPC_GUEST_CALL_RETURN_PC,
            )?;
        self.cpu.gpr[1] = callback_sp;
        self.cpu.pc = pending.target.entry;
        self.cpu.lr = PPC_GUEST_CALL_RETURN_PC;
        self.cpu.gpr[2] = pending.target.rtoc;
        install_powerpc_call_arguments(
            &mut self.cpu,
            &mut self.memory,
            pending.arguments.as_slice(),
        )
    }
}
