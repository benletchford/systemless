//! A single prepared application fragment's CFM lifecycle. Library closures and
//! dependency ordering will extend this process-owned state, not CPU engines.
use crate::{
    cpu::{Architecture, Cpu},
    runtime::Stop,
    Error, Result,
};

#[derive(Clone, Copy)]
pub(crate) struct Routine {
    pub entry: u32,
    pub toc: u32,
}
enum Phase {
    Initializing(Cpu),
    Running,
    Terminating(Stop),
}
pub(crate) struct Lifecycle {
    phase: Phase,
    termination: Option<Routine>,
    stack: u32,
}
impl Lifecycle {
    pub fn start(
        cpu: &mut Cpu,
        init: Option<Routine>,
        termination: Option<Routine>,
        block: u32,
    ) -> Self {
        let stack = match cpu {
            Cpu::PowerPc(c) => c.gpr[1],
            _ => unreachable!("PEF requires PPC"),
        };
        let phase = if let Some(init) = init {
            let mut callee = Self::callee(init, stack);
            if let Cpu::PowerPc(c) = &mut callee {
                c.gpr[3] = block;
            }
            Phase::Initializing(std::mem::replace(cpu, callee))
        } else {
            Phase::Running
        };
        Self {
            phase,
            termination,
            stack,
        }
    }
    pub(crate) fn validate_nested(cpu: &Cpu, memory: &crate::memory::Memory) -> Result<u32> {
        let Cpu::PowerPc(c) = cpu else {
            return Err(Error::Cpu(
                "PPC CFM init/term from 68K requires Mixed Mode".into(),
            ));
        };
        let stack = c.gpr[1]
            .checked_sub(64)
            .filter(|sp| sp.is_multiple_of(16))
            .ok_or_else(|| Error::Cpu("invalid CFM caller stack".into()))?;
        memory.validate_write(crate::memory::GuestPtr(stack), 64)?;
        Ok(stack)
    }
    pub(crate) fn nested(
        cpu: &Cpu,
        memory: &mut crate::memory::Memory,
        routine: Routine,
        argument: Option<u32>,
    ) -> Result<Cpu> {
        let stack = Self::validate_nested(cpu, memory)?;
        let mut frame = [0; 64];
        frame[..4].copy_from_slice(&(stack + 64).to_be_bytes());
        memory.write(crate::memory::GuestPtr(stack), &frame)?;
        let mut callee = Self::callee(routine, stack);
        if let (Cpu::PowerPc(c), Some(argument)) = (&mut callee, argument) {
            c.gpr[3] = argument;
        }
        Ok(callee)
    }
    fn callee(routine: Routine, stack: u32) -> Cpu {
        let mut cpu = Cpu::new(Architecture::PowerPc, routine.entry, stack);
        if let Cpu::PowerPc(c) = &mut cpu {
            c.gpr[2] = routine.toc;
        }
        cpu
    }
    pub fn returned(&mut self, cpu: &mut Cpu) -> Result<Option<Stop>> {
        match std::mem::replace(&mut self.phase, Phase::Running) {
            Phase::Initializing(main) => {
                let error = cpu.result() as i16;
                if error != 0 {
                    return Err(Error::Load(format!(
                        "PEF initializer returned {error} (cfragInitFunctionErr)"
                    )));
                }
                *cpu = main;
                Ok(None)
            }
            Phase::Running => self.finish(cpu, Stop::Returned(cpu.result())),
            Phase::Terminating(stop) => Ok(Some(stop)),
        }
    }
    pub fn exit(&mut self, cpu: &mut Cpu, value: u32) -> Result<Option<Stop>> {
        if !matches!(self.phase, Phase::Running) {
            return Err(Error::Cpu(
                "ExitToShell during fragment initialization/termination".into(),
            ));
        }
        self.finish(cpu, Stop::Exit(value))
    }
    fn finish(&mut self, cpu: &mut Cpu, stop: Stop) -> Result<Option<Stop>> {
        if let Some(term) = self.termination.take() {
            *cpu = Self::callee(term, self.stack);
            self.phase = Phase::Terminating(stop);
            Ok(None)
        } else {
            Ok(Some(stop))
        }
    }
}
