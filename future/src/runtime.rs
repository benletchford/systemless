use crate::{
    cpu::{Architecture, Call, Cpu, Outcome, RETURN_GATE},
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Budget,
    Exit(u32),
    Returned(u32),
}
#[derive(Default, Debug)]
pub struct Services {
    pub accumulator: u32,
    pub calls: Vec<Call>,
}
// The whole CPU is suspended, retaining even engine-private FPU/control state.
struct Continuation {
    caller: Cpu,
}
pub struct Runtime {
    pub memory: Memory,
    pub services: Services,
    pub graphics: Option<Graphics>,
    pub cpu: Cpu,
    pub instructions: u64,
    pub abi: crate::abi::Abi,
    pub process: crate::process::Process,
    continuations: Vec<Continuation>,
    terminal: Option<Stop>,
    pub(crate) lifecycle: Option<crate::lifecycle::Lifecycle>,
    faulted: bool,
}
impl Runtime {
    pub fn new(memory: Memory, cpu: Cpu, graphics: Option<Graphics>) -> Self {
        Self {
            memory,
            cpu,
            graphics,
            services: Services::default(),
            instructions: 0,
            abi: crate::abi::Abi::fixtures(),
            process: crate::process::Process::default(),
            continuations: Vec::new(),
            terminal: None,
            lifecycle: None,
            faulted: false,
        }
    }
    pub fn advance(&mut self, budget: u64) -> Result<Stop> {
        if self.faulted {
            return Err(Error::Cpu("runtime stopped after earlier fault".into()));
        }
        if let Some(stop) = self.terminal {
            return Ok(stop);
        }
        let result = self.advance_inner(budget);
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    fn advance_inner(&mut self, budget: u64) -> Result<Stop> {
        for _ in 0..budget {
            self.instructions = self
                .instructions
                .checked_add(1)
                .ok_or_else(|| Error::Cpu("instruction clock overflow".into()))?;
            if !self.abi.fixture && self.instructions.is_multiple_of(1000) {
                let ticks = self.memory.u32(GuestPtr(0x16a))?.wrapping_add(1);
                self.memory.write(GuestPtr(0x16a), &ticks.to_be_bytes())?;
            }
            match self.cpu.step_with_abi(&mut self.memory, &self.abi)? {
                Outcome::Continue => {}
                Outcome::Return => {
                    let value = self.cpu.result();
                    if let Some(frame) = self.continuations.pop() {
                        self.cpu = frame.caller;
                        self.cpu.set_result(value);
                    } else {
                        let stop = if let Some(lifecycle) = &mut self.lifecycle {
                            lifecycle.returned(&mut self.cpu)?
                        } else {
                            Some(Stop::Returned(value))
                        };
                        if let Some(stop) = stop {
                            self.terminal = Some(stop);
                            return Ok(stop);
                        }
                    }
                }
                Outcome::Macintosh(call, reply) => {
                    self.services.calls.push(call);
                    match call {
                        Call::Ticks => {
                            let value = self.tick_count()?;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::Exit(v) => {
                            let stop = if let Some(lifecycle) = &mut self.lifecycle {
                                lifecycle.exit(&mut self.cpu, v)?
                            } else {
                                Some(Stop::Exit(v))
                            };
                            self.continuations.clear();
                            if let Some(stop) = stop {
                                self.terminal = Some(stop);
                                return Ok(stop);
                            }
                        }
                        Call::NewHandle { size, clear } => {
                            let v = self.process.new_handle(&mut self.memory, size, clear)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                v,
                                self.process.mem_error,
                            )?;
                        }
                        Call::DisposeHandle(h) => {
                            self.process.dispose_handle(&mut self.memory, h)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                0,
                                self.process.mem_error,
                            )?;
                        }
                        Call::HandleSize(h) => {
                            let v = self.process.handle_size(&self.memory, h)?;
                            reply.apply(
                                &mut self.cpu,
                                &mut self.memory,
                                v,
                                self.process.mem_error,
                            )?;
                        }
                        Call::GetResource { kind, id } => {
                            let v = self.process.get_resource(&mut self.memory, kind, id)?;
                            reply.apply(&mut self.cpu, &mut self.memory, v, 0)?;
                        }
                        Call::MemError => {
                            let value = self.memory.u16(GuestPtr(0x220))? as i16 as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        Call::ResError => {
                            let value = self.memory.u16(GuestPtr(0xa60))? as i16 as i32 as u32;
                            reply.apply(&mut self.cpu, &mut self.memory, value, 0)?;
                        }
                        _ => return Err(Error::Cpu("invalid Macintosh service route".into())),
                    }
                    // Guest globals are the ABI-visible error authorities. Process
                    // fields carry operation outputs only, never readback authority.
                    if matches!(
                        call,
                        Call::NewHandle { .. }
                            | Call::DisposeHandle(_)
                            | Call::HandleSize(_)
                            | Call::GetResource { .. }
                    ) {
                        self.memory
                            .write(GuestPtr(0x220), &self.process.mem_error.to_be_bytes())?;
                    }
                    if matches!(call, Call::GetResource { .. }) {
                        self.memory
                            .write(GuestPtr(0xa60), &self.process.res_error.to_be_bytes())?;
                    }
                }
                Outcome::Service(call) => {
                    self.services.calls.push(call);
                    match call {
                        Call::NewHandle { .. }
                        | Call::DisposeHandle(_)
                        | Call::HandleSize(_)
                        | Call::GetResource { .. }
                        | Call::MemError
                        | Call::ResError => {
                            return Err(Error::Cpu("Macintosh service on fixture route".into()))
                        }
                        Call::Ticks => {
                            let value = self.tick_count()?;
                            self.cpu.set_result(value);
                        }
                        Call::Add(v) => {
                            self.services.accumulator = self.services.accumulator.wrapping_add(v);
                            self.cpu.set_result(self.services.accumulator);
                        }
                        Call::Draw(ptr) => {
                            // Private fixture command record, all fields are big-endian.
                            let b: [u8; 10] = self.memory.read(ptr, 10)?.try_into().unwrap();
                            let coord = |i| i32::from(i16::from_be_bytes([b[i], b[i + 1]]));
                            let rect = Rect::new(coord(0), coord(2), coord(4), coord(6));
                            let gpu = self
                                .graphics
                                .as_ref()
                                .ok_or_else(|| Error::Graphics("drawing requires a GPU".into()))?;
                            let mode = match b[9] {
                                0 => Transfer::Copy,
                                1 => Transfer::Or,
                                2 => Transfer::Xor,
                                3 => Transfer::Bic,
                                v => return Err(Error::Service(v.into())),
                            };
                            gpu.execute(
                                &mut self.memory,
                                Draw::Fill {
                                    rect,
                                    clip: Rect::new(0, 0, gpu.width as i32, gpu.height as i32),
                                    pattern: [255; 8],
                                    foreground: b[8],
                                    background: 0,
                                    mode,
                                },
                            )?;
                            self.cpu.set_result(0);
                        }
                        Call::Callback(ptr) => self.enter_callback(ptr)?,
                        Call::Exit(v) => {
                            let stop = Stop::Exit(v);
                            self.terminal = Some(stop);
                            return Ok(stop);
                        }
                    }
                }
            }
        }
        Ok(Stop::Budget)
    }
    // Deterministic scheduling policy: 1000 retired boundaries per 60 Hz tick.
    // Host wall time never enters guest time; future hosts pace execution budgets.
    fn tick_count(&self) -> Result<u32> {
        if self.abi.fixture {
            Ok((self.instructions / 1000) as u32)
        } else {
            self.memory.u32(GuestPtr(0x16a))
        }
    }
    fn enter_callback(&mut self, descriptor: GuestPtr) -> Result<()> {
        if self.continuations.len() >= 32 {
            return Err(Error::Cpu("callback depth exceeded".into()));
        }
        // Internal descriptor: ISA (0/1), entry, TOC. Not Apple ProcInfo.
        let bytes = self.memory.read(descriptor, 12)?;
        let word = |i| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
        let arch = match word(0) {
            0 => Architecture::M68k,
            1 => Architecture::PowerPc,
            v => return Err(Error::Fixture(format!("callback ISA {v}"))),
        };
        let entry = word(4);
        let toc = word(8);
        self.memory.fetch(
            GuestPtr(entry),
            if arch == Architecture::M68k { 2 } else { 4 },
            if arch == Architecture::M68k { 2 } else { 4 },
        )?;
        let base = 0x100000 + self.continuations.len() as u32 * 0x2000;
        // Stack mappings are retained and reused at each nesting depth.
        if self.memory.read(GuestPtr(base), 1).is_err() {
            self.memory
                .map(GuestPtr(base), vec![0; 0x2000], Permissions::RW)?;
        }
        let mut callee = Cpu::new(arch, entry, base + 0x1ff0);
        match &mut callee {
            Cpu::M68k(c) => self
                .memory
                .write(GuestPtr(c.dar[15]), &RETURN_GATE.to_be_bytes())?,
            Cpu::PowerPc(c) => c.gpr[2] = toc,
        }
        let caller = std::mem::replace(&mut self.cpu, callee);
        self.continuations.push(Continuation { caller });
        Ok(())
    }
}
