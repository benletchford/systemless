//! CPU backend built on the [`m68k`] crate.
//!
//! [`M68kCpu`] initializes the canonical guest CPU model and exposes both
//! precise single-instruction stepping and JIT-enabled native batch execution.
//! WebAssembly builds retain the same batch API through m68k's portable trace
//! executor.

use crate::memory::MacMemoryBus;

pub use m68k::HleHandler;

/// 68k CPU register identifiers: eight data registers (D0–D7), eight
/// address registers (A0–A7, with A7 selecting the active stack pointer),
/// and the program counter.
#[derive(Debug, Clone, Copy)]
pub enum Register {
    D0,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    A0,
    A1,
    A2,
    A3,
    A4,
    A5,
    A6,
    A7,
    PC,
}

/// Outcome of a single 68k instruction step. Returned by
/// [`FixtureRunner::step`](crate::runner::FixtureRunner::step).
pub enum StepResult {
    /// The instruction completed normally and execution may continue.
    Ok,
    /// The CPU executed `STOP`, or the wrapper normalized an unsupported
    /// terminal condition to a stopped result.
    Stopped,
    /// The instruction was an A-line trap; the carried value is the trap word
    /// for the dispatcher.
    Aline(u16),
}

/// Register interface used by the trap dispatcher.
///
/// Keeping handlers generic over this subset lets them operate on both the
/// production [`M68kCpu`] backend and test doubles without exposing the full
/// m68k architectural state.
pub trait CpuOps {
    /// Read the current value of a single register.
    fn read_reg(&self, reg: Register) -> u32;
    /// Write a value into a single register.
    fn write_reg(&mut self, reg: Register, value: u32);
    /// Read the condition code register (CCR) byte.
    fn get_ccr(&self) -> u8;
    /// Set the condition code register (CCR).
    ///
    /// The low five bits are X(4), N(3), Z(2), V(1), and C(0).
    fn set_ccr(&mut self, ccr: u8);
}

/// Systemless wrapper around [`m68k::CpuCore`].
///
/// Construction selects the CPU type from
/// [`crate::machine_profile::REFERENCE_MACHINE_PROFILE`]. Callers with a
/// specialized embedding may subsequently reconfigure the public core.
pub struct M68kCpu {
    /// Complete m68k CPU state and execution API.
    ///
    /// This is public for diagnostics and specialized embedding beyond the
    /// register-only [`CpuOps`] interface.
    pub core: m68k::CpuCore,
}

impl M68kCpu {
    /// Create a CPU configured for the canonical Systemless machine profile.
    pub fn new() -> Self {
        let mut core = m68k::CpuCore::new();
        core.set_cpu_type(crate::machine_profile::REFERENCE_MACHINE_PROFILE.cpu_type());
        Self { core }
    }

    /// Read one data, address, or program-counter register.
    #[inline]
    pub fn read_reg(&self, reg: Register) -> u32 {
        match reg {
            Register::D0 => self.core.d(0),
            Register::D1 => self.core.d(1),
            Register::D2 => self.core.d(2),
            Register::D3 => self.core.d(3),
            Register::D4 => self.core.d(4),
            Register::D5 => self.core.d(5),
            Register::D6 => self.core.d(6),
            Register::D7 => self.core.d(7),
            Register::A0 => self.core.a(0),
            Register::A1 => self.core.a(1),
            Register::A2 => self.core.a(2),
            Register::A3 => self.core.a(3),
            Register::A4 => self.core.a(4),
            Register::A5 => self.core.a(5),
            Register::A6 => self.core.a(6),
            Register::A7 => self.core.a(7),
            Register::PC => self.core.pc,
        }
    }

    /// Write one data, address, or program-counter register.
    #[inline]
    pub fn write_reg(&mut self, reg: Register, value: u32) {
        match reg {
            Register::D0 => self.core.set_d(0, value),
            Register::D1 => self.core.set_d(1, value),
            Register::D2 => self.core.set_d(2, value),
            Register::D3 => self.core.set_d(3, value),
            Register::D4 => self.core.set_d(4, value),
            Register::D5 => self.core.set_d(5, value),
            Register::D6 => self.core.set_d(6, value),
            Register::D7 => self.core.set_d(7, value),
            Register::A0 => self.core.set_a(0, value),
            Register::A1 => self.core.set_a(1, value),
            Register::A2 => self.core.set_a(2, value),
            Register::A3 => self.core.set_a(3, value),
            Register::A4 => self.core.set_a(4, value),
            Register::A5 => self.core.set_a(5, value),
            Register::A6 => self.core.set_a(6, value),
            Register::A7 => self.core.set_a(7, value),
            Register::PC => self.core.pc = value,
        }
    }

    /// Return whether the CPU is stopped by a `STOP` instruction.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.core.is_stopped()
    }

    /// Reset the CPU from the initial stack-pointer and program-counter vectors.
    pub fn reset(&mut self, bus: &mut MacMemoryBus) {
        self.core.reset(bus);
    }

    /// Execute at most `max_instructions` through m68k's throughput path.
    ///
    /// Native builds enable Cranelift compilation for eligible hot traces;
    /// WebAssembly executes the same traces through the portable executor.
    /// Execution returns on the first event the runner must handle: a trap,
    /// stopped CPU, watched PC, or exhausted instruction budget.
    ///
    /// See [`m68k::CpuCore::run_batch`] for the exit/accounting
    /// contract; the trapping instruction is *not* included in
    /// `instructions` and `core.ppc` holds its address.
    #[inline]
    pub fn run_batch(
        &mut self,
        bus: &mut MacMemoryBus,
        max_instructions: u32,
        watch_pcs: &[u32],
    ) -> m68k::BatchResult {
        self.core.run_batch(bus, max_instructions, watch_pcs)
    }

    /// Execute one instruction through m68k's precise stepping path.
    ///
    /// Systemless surfaces completed instructions, A-line traps, and STOP
    /// directly. F-line exits retain the legacy no-op behavior and return
    /// [`StepResult::Ok`]; illegal instructions, `TRAP`, and `BKPT` exits are
    /// reported and converted to [`StepResult::Stopped`].
    #[inline]
    pub fn step(&mut self, bus: &mut MacMemoryBus) -> StepResult {
        match self.core.step(bus) {
            m68k::StepResult::Ok { .. } => StepResult::Ok,
            m68k::StepResult::AlineTrap { opcode } => StepResult::Aline(opcode),
            m68k::StepResult::FlineTrap { .. } => StepResult::Ok,
            m68k::StepResult::Stopped => {
                eprintln!("[CPU] StepResult::Stopped");
                StepResult::Stopped
            }
            m68k::StepResult::IllegalInstruction { opcode } => {
                eprintln!(
                    "[CPU] IllegalInstruction: ${:04X} at PC=${:08X}",
                    opcode, self.core.pc
                );
                StepResult::Stopped
            }
            m68k::StepResult::TrapInstruction { trap_num } => {
                eprintln!("[CPU] TrapInstruction: #{}", trap_num);
                StepResult::Stopped
            }
            m68k::StepResult::Breakpoint { bp_num } => {
                eprintln!("[CPU] Breakpoint: #{}", bp_num);
                StepResult::Stopped
            }
        }
    }
}

impl Default for M68kCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuOps for M68kCpu {
    #[inline]
    fn read_reg(&self, reg: Register) -> u32 {
        M68kCpu::read_reg(self, reg)
    }
    #[inline]
    fn write_reg(&mut self, reg: Register, value: u32) {
        M68kCpu::write_reg(self, reg, value)
    }
    #[inline]
    fn get_ccr(&self) -> u8 {
        self.core.get_ccr()
    }
    #[inline]
    fn set_ccr(&mut self, ccr: u8) {
        self.core.set_ccr(ccr);
    }
}

#[cfg(test)]
mod tests {
    use super::{M68kCpu, Register, StepResult};
    use crate::memory::{MacMemoryBus, MemoryBus};

    #[test]
    fn custom_zero_divide_handler_receives_68020_format_two_frame() {
        let mut cpu = M68kCpu::new();
        let mut bus = MacMemoryBus::new(0x400000);
        let prog = 0x0010_0000;
        let handler = 0x0010_0100;
        let initial_sp = 0x003F_FFC0;

        // DIVSL.L D1,D2:D0; NOP
        bus.write_word(prog, 0x4C41);
        bus.write_word(prog + 2, 0x0C02);
        bus.write_word(prog + 4, 0x4E71);
        bus.write_long(0x0014, handler);

        // A classic format-2 skip handler: discard SR/PC/format, advance
        // the saved instruction address by the four-byte DIVSL, then RTS.
        bus.write_word(handler, 0x508F); // ADDQ.L #8,A7
        bus.write_word(handler + 2, 0x5897); // ADDQ.L #4,(A7)
        bus.write_word(handler + 4, 0x4E75); // RTS

        cpu.write_reg(Register::PC, prog);
        cpu.write_reg(Register::A7, initial_sp);
        cpu.write_reg(Register::D0, 100);
        cpu.write_reg(Register::D1, 0);
        cpu.write_reg(Register::D2, 0);

        let first = cpu.run_batch(&mut bus, 1, &[]);
        assert_eq!(first.instructions, 1);
        assert_eq!(cpu.read_reg(Register::PC), handler);

        let second = cpu.run_batch(&mut bus, 4, &[]);
        assert_eq!(second.instructions, 4);
        assert_eq!(cpu.read_reg(Register::PC), prog + 6);
        assert_eq!(cpu.read_reg(Register::A7), initial_sp);
    }

    #[test]
    fn cmp_word_address_indirect_sets_lt_and_branches() {
        let mut cpu = M68kCpu::new();
        let mut bus = MacMemoryBus::new(0x400000);

        cpu.write_reg(Register::PC, 0x001000);
        cpu.write_reg(Register::A2, 0x002000);
        cpu.write_reg(Register::D1, 56);

        // CMP.W (A2),D1 ; BLT +2 ; NOP ; NOP(target)
        bus.write_word(0x001000, 0xB252);
        bus.write_word(0x001002, 0x6D02);
        bus.write_word(0x001004, 0x4E71);
        bus.write_word(0x001006, 0x4E71);
        bus.write_word(0x002000, 202);

        match cpu.step(&mut bus) {
            StepResult::Ok => {}
            _ => panic!("CMP.W should execute normally"),
        }
        assert_eq!(
            cpu.core.get_ccr() & 0x0F,
            0x09,
            "CMP.W 56 - 202 should set N and C for a signed less-than result"
        );

        match cpu.step(&mut bus) {
            StepResult::Ok => {}
            _ => panic!("BLT should execute normally"),
        }
        assert_eq!(
            cpu.read_reg(Register::PC),
            0x001006,
            "BLT should branch when D1 is less than the word at (A2)"
        );
    }

    #[test]
    fn addq_cmp_blt_loop_reaches_count_limit() {
        let mut cpu = M68kCpu::new();
        let mut bus = MacMemoryBus::new(0x400000);
        let base = 0x003000u32;
        let count_ptr = 0x002000u32;

        cpu.write_reg(Register::PC, 0x001000);
        cpu.write_reg(Register::A2, count_ptr);
        cpu.write_reg(Register::A4, base);

        // CLR.W D1
        // loop: ADDQ.W #1,D1 ; ADDQ.W #6,A4 ; CMP.W (A2),D1 ; BLT.S loop
        // MOVE.L A4,A0
        bus.write_word(0x001000, 0x4241);
        bus.write_word(0x001002, 0x5241);
        bus.write_word(0x001004, 0x5C4C);
        bus.write_word(0x001006, 0xB252);
        bus.write_word(0x001008, 0x6DF8);
        bus.write_word(0x00100A, 0x204C);
        bus.write_word(count_ptr, 202);

        for _ in 0..1000 {
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                _ => panic!("loop program should execute normally"),
            }
            if cpu.read_reg(Register::PC) == 0x00100C {
                break;
            }
        }

        assert_eq!(cpu.read_reg(Register::D1) & 0xFFFF, 202);
        assert_eq!(
            cpu.read_reg(Register::A4),
            base + 202 * 6,
            "A4 should advance by six bytes for every loop iteration until D1 reaches count"
        );
        assert_eq!(cpu.read_reg(Register::A0), base + 202 * 6);
    }
}
