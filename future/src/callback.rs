//! Same-ISA Pascal Boolean(CInfoPBPtr) callbacks for classic Standard File.
//! Caller CPU is suspended whole by Runtime; this only marshals a guest frame.
use crate::{
    cpu::{Architecture, Cpu, RETURN_GATE},
    memory::{GuestPtr, Memory},
    Error, Result,
};
pub(crate) enum BooleanResult {
    Stack(GuestPtr),
    Register,
}
impl BooleanResult {
    pub fn read(&self, memory: &Memory, register: u32) -> Result<bool> {
        Ok(match self {
            Self::Stack(p) => memory.read(*p, 1)?[0] != 0,
            Self::Register => register as u8 != 0,
        })
    }
}
fn resolve(caller: &Cpu, memory: &Memory, routine: u32, info: u32) -> Result<u32> {
    Ok(if memory.u16(GuestPtr(routine))? == 0xaafe {
        let rd = memory.read(GuestPtr(routine), 32)?;
        let isa = if caller.architecture() == Architecture::M68k {
            0
        } else {
            1
        };
        if rd[2] != 7
            || rd[3..12].iter().any(|b| *b != 0)
            || rd[12..16] != info.to_be_bytes()
            || rd[16] != 0
            || rd[17] != isa
            || !matches!(u16::from_be_bytes(rd[18..20].try_into().unwrap()), 0 | 4)
            || rd[24..32].iter().any(|b| *b != 0)
        {
            return Err(Error::Cpu("unsupported callback RoutineDescriptor (requires one prepared absolute same-ISA routine)".into()));
        }
        u32::from_be_bytes(rd[20..24].try_into().unwrap())
    } else {
        if caller.architecture() == Architecture::PowerPc {
            return Err(Error::Cpu(
                "PowerPC callback requires a RoutineDescriptor".into(),
            ));
        }
        routine
    })
}
pub(crate) fn file_filter(
    caller: &Cpu,
    memory: &mut Memory,
    routine: u32,
    argument: GuestPtr,
) -> Result<(Cpu, BooleanResult)> {
    let bad = || Error::Cpu("invalid Standard File callback frame".into());
    let routine = resolve(caller, memory, routine, 0xd0)?;
    match caller {
        Cpu::M68k(c) => {
            memory.fetch(GuestPtr(routine), 2, 2)?;
            let sp = c.dar[15]
                .checked_sub(10)
                .filter(|s| s.is_multiple_of(2))
                .ok_or_else(bad)?;
            memory.validate_write(GuestPtr(sp), 10)?;
            let mut frame = [0; 10];
            frame[..4].copy_from_slice(&RETURN_GATE.to_be_bytes());
            frame[4..8].copy_from_slice(&argument.0.to_be_bytes());
            let mut callee = Cpu::new(Architecture::M68k, routine, sp);
            if let Cpu::M68k(next) = &mut callee {
                next.dar = c.dar;
                next.set_sr(c.get_sr());
                next.dar[15] = sp;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok((callee, BooleanResult::Stack(GuestPtr(sp + 8))))
        }
        Cpu::PowerPc(c) => {
            let descriptor = memory.read(GuestPtr(routine), 8)?;
            let entry = u32::from_be_bytes(descriptor[..4].try_into().unwrap());
            let toc = u32::from_be_bytes(descriptor[4..].try_into().unwrap());
            memory.fetch(GuestPtr(entry), 4, 4)?;
            let sp = crate::lifecycle::Lifecycle::validate_nested(caller, memory)?;
            let mut frame = [0; 64];
            frame[..4].copy_from_slice(&c.gpr[1].to_be_bytes());
            let mut callee = Cpu::new(Architecture::PowerPc, entry, sp);
            if let Cpu::PowerPc(next) = &mut callee {
                next.gpr = c.gpr;
                next.gpr[1] = sp;
                next.gpr[2] = toc;
                next.gpr[3] = argument.0;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok((callee, BooleanResult::Register))
        }
    }
}

/// Pascal Boolean(DialogPtr, EventRecord*, Integer*), same-ISA prepared UPP.
pub(crate) fn modal_filter(
    caller: &Cpu,
    memory: &mut Memory,
    routine: u32,
    arguments: [u32; 3],
) -> Result<(Cpu, BooleanResult)> {
    let routine = resolve(caller, memory, routine, 0xfd0)?;
    match caller {
        Cpu::M68k(c) => {
            memory.fetch(GuestPtr(routine), 2, 2)?;
            let sp = c.dar[15]
                .checked_sub(18)
                .filter(|s| s.is_multiple_of(2))
                .ok_or_else(|| Error::Cpu("invalid modal callback stack".into()))?;
            memory.validate_write(GuestPtr(sp), 18)?;
            let mut frame = Vec::new();
            frame.extend(RETURN_GATE.to_be_bytes());
            for arg in arguments.into_iter().rev() {
                frame.extend(arg.to_be_bytes());
            }
            frame.extend([0; 2]);
            let mut callee = Cpu::new(Architecture::M68k, routine, sp);
            if let Cpu::M68k(next) = &mut callee {
                next.dar = c.dar;
                next.set_sr(c.get_sr());
                next.dar[15] = sp;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok((callee, BooleanResult::Stack(GuestPtr(sp + 16))))
        }
        Cpu::PowerPc(c) => {
            let descriptor = memory.read(GuestPtr(routine), 8)?;
            let entry = u32::from_be_bytes(descriptor[..4].try_into().unwrap());
            let toc = u32::from_be_bytes(descriptor[4..].try_into().unwrap());
            memory.fetch(GuestPtr(entry), 4, 4)?;
            let sp = crate::lifecycle::Lifecycle::validate_nested(caller, memory)?;
            let mut frame = [0; 64];
            frame[..4].copy_from_slice(&c.gpr[1].to_be_bytes());
            let mut callee = Cpu::new(Architecture::PowerPc, entry, sp);
            if let Cpu::PowerPc(next) = &mut callee {
                next.gpr = c.gpr;
                next.gpr[1] = sp;
                next.gpr[2] = toc;
                next.gpr[3..6].copy_from_slice(&arguments);
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok((callee, BooleanResult::Register))
        }
    }
}

/// Pascal void(SndChannelPtr, SndCommandPtr). The caller's whole CPU is retained
/// by Runtime; this adapter never runs from a native audio callback.
pub(crate) fn sound(caller: &Cpu, memory: &mut Memory, routine: u32, channel: u32) -> Result<Cpu> {
    let routine = resolve(caller, memory, routine, 0x3c0)?;
    let command = channel
        .checked_add(20)
        .ok_or_else(|| Error::Cpu("sound callback argument overflow".into()))?;
    match caller {
        Cpu::M68k(c) => {
            memory.fetch(GuestPtr(routine), 2, 2)?;
            let sp = c.dar[15]
                .checked_sub(12)
                .filter(|s| s.is_multiple_of(2))
                .ok_or_else(|| Error::Cpu("invalid sound callback stack".into()))?;
            memory.validate_write(GuestPtr(sp), 12)?;
            let mut frame = Vec::new();
            frame.extend(RETURN_GATE.to_be_bytes());
            frame.extend(command.to_be_bytes());
            frame.extend(channel.to_be_bytes());
            let mut callee = Cpu::new(Architecture::M68k, routine, sp);
            if let Cpu::M68k(next) = &mut callee {
                next.dar = c.dar;
                next.set_sr(c.get_sr());
                next.dar[15] = sp;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok(callee)
        }
        Cpu::PowerPc(c) => {
            let descriptor = memory.read(GuestPtr(routine), 8)?;
            let entry = u32::from_be_bytes(descriptor[..4].try_into().unwrap());
            let toc = u32::from_be_bytes(descriptor[4..].try_into().unwrap());
            memory.fetch(GuestPtr(entry), 4, 4)?;
            let sp = crate::lifecycle::Lifecycle::validate_nested(caller, memory)?;
            let mut frame = [0; 64];
            frame[..4].copy_from_slice(&c.gpr[1].to_be_bytes());
            let mut callee = Cpu::new(Architecture::PowerPc, entry, sp);
            if let Cpu::PowerPc(next) = &mut callee {
                next.gpr = c.gpr;
                next.gpr[1] = sp;
                next.gpr[2] = toc;
                next.gpr[3] = channel;
                next.gpr[4] = command;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok(callee)
        }
    }
}

/// TimerProc: A1 on 68K; one task pointer argument through a prepared PPC UPP.
pub(crate) fn timer(
    caller: &Cpu,
    memory: &mut Memory,
    routine: u32,
    task: GuestPtr,
) -> Result<Cpu> {
    task_callback(caller, memory, routine, task, false)
}
pub(crate) fn retrace(
    caller: &Cpu,
    memory: &mut Memory,
    routine: u32,
    task: GuestPtr,
) -> Result<Cpu> {
    task_callback(caller, memory, routine, task, true)
}
fn task_callback(
    caller: &Cpu,
    memory: &mut Memory,
    routine: u32,
    task: GuestPtr,
    vbl: bool,
) -> Result<Cpu> {
    let routine = resolve(caller, memory, routine, if vbl { 0x9802 } else { 0xb802 })?;
    match caller {
        Cpu::M68k(c) => {
            memory.fetch(GuestPtr(routine), 2, 2)?;
            let sp = c.dar[15]
                .checked_sub(4)
                .filter(|s| s.is_multiple_of(2))
                .ok_or_else(|| Error::Cpu("invalid task callback stack".into()))?;
            memory.validate_write(GuestPtr(sp), 4)?;
            let mut callee = Cpu::new(Architecture::M68k, routine, sp);
            if let Cpu::M68k(next) = &mut callee {
                next.dar = c.dar;
                next.set_sr(c.get_sr());
                next.dar[15] = sp;
                next.dar[if vbl { 8 } else { 9 }] = task.0;
                if vbl {
                    next.set_sr((c.get_sr() & !0x0700) | 0x0100);
                }
            }
            memory.write(GuestPtr(sp), &RETURN_GATE.to_be_bytes())?;
            Ok(callee)
        }
        Cpu::PowerPc(c) => {
            let descriptor = memory.read(GuestPtr(routine), 8)?;
            let entry = u32::from_be_bytes(descriptor[..4].try_into().unwrap());
            let toc = u32::from_be_bytes(descriptor[4..].try_into().unwrap());
            memory.fetch(GuestPtr(entry), 4, 4)?;
            let sp = crate::lifecycle::Lifecycle::validate_nested(caller, memory)?;
            let mut frame = [0; 64];
            frame[..4].copy_from_slice(&c.gpr[1].to_be_bytes());
            let mut callee = Cpu::new(Architecture::PowerPc, entry, sp);
            if let Cpu::PowerPc(next) = &mut callee {
                next.gpr = c.gpr;
                next.gpr[1] = sp;
                next.gpr[2] = toc;
                next.gpr[3] = task.0;
            }
            memory.write(GuestPtr(sp), &frame)?;
            Ok(callee)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cpu::Outcome, memory::Permissions};
    #[test]
    fn pascal_boolean_frame_passes_pointer_preserves_a5_and_returns_a_stack_byte() {
        let mut m = Memory::default();
        // move.l 4(sp),a0; move.b #1,8(sp); move.l a5,(a0);
        // move.l (sp)+,a1; addq.l #4,sp; jmp (a1)
        let words: [u16; 10] = [
            0x206f, 4, 0x1f7c, 1, 8, 0x208d, 0x225f, 0x588f, 0x4ed1, 0x4e71,
        ];
        m.map(
            GuestPtr(0x9000),
            words.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
        m.map(GuestPtr(0x1000), vec![0xa5; 0x100], Permissions::RW)
            .unwrap();
        let mut caller = Cpu::new(Architecture::M68k, 0x1234, 0x1080);
        if let Cpu::M68k(c) = &mut caller {
            c.dar[13] = 0xfeed1234;
        }
        let (mut callee, result) = file_filter(&caller, &mut m, 0x9000, GuestPtr(0x1000)).unwrap();
        for _ in 0..6 {
            assert!(matches!(callee.step(&mut m).unwrap(), Outcome::Continue));
        }
        assert!(matches!(callee.step(&mut m).unwrap(), Outcome::Return));
        assert!(result.read(&m, 0).unwrap());
        assert_eq!(m.u32(GuestPtr(0x1000)).unwrap(), 0xfeed1234);
        assert_eq!(caller.pc(), 0x1234);
        if let Cpu::M68k(c) = caller {
            assert_eq!(c.dar[15], 0x1080);
        }
    }
    #[test]
    fn invalid_descriptors_and_stack_frames_do_not_mutate_guest_memory() {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0xa5; 0x100], Permissions::RW)
            .unwrap();
        let mut rd = [0; 32];
        rd[..4].copy_from_slice(&[0xaa, 0xfe, 7, 0]);
        rd[12..16].copy_from_slice(&0xd0u32.to_be_bytes());
        rd[17] = 1;
        rd[20..24].copy_from_slice(&0x9000u32.to_be_bytes());
        m.map(GuestPtr(0x8000), rd.to_vec(), Permissions::RW)
            .unwrap();
        m.map(GuestPtr(0x9000), vec![0x4e, 0x80, 0, 0x20], Permissions::RX)
            .unwrap();
        m.map(
            GuestPtr(0xa000),
            [0x9000u32, 0x1234]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            Permissions::RX,
        )
        .unwrap();
        let caller = Cpu::new(Architecture::PowerPc, 0x1234, 0x1080);
        for (at, byte) in [
            (2, 8),
            (3, 1),
            (10, 1),
            (15, 0xd1),
            (17, 0),
            (19, 2),
            (24, 1),
            (31, 1),
        ] {
            let mut bad = rd;
            bad[at] = byte;
            m.write(GuestPtr(0x8000), &bad).unwrap();
            assert!(file_filter(&caller, &mut m, 0x8000, GuestPtr(0x1000)).is_err());
            assert_eq!(m.read(GuestPtr(0x1000), 0x100).unwrap(), [0xa5; 0x100]);
        }
        rd[20..24].copy_from_slice(&0xa000u32.to_be_bytes());
        m.write(GuestPtr(0x8000), &rd).unwrap();
        let bad_stack = Cpu::new(Architecture::PowerPc, 0, 0x1081);
        assert!(file_filter(&bad_stack, &mut m, 0x8000, GuestPtr(0x1000)).is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 0x100).unwrap(), [0xa5; 0x100]);
        let (callee, result) = file_filter(&caller, &mut m, 0x8000, GuestPtr(0x1000)).unwrap();
        if let Cpu::PowerPc(c) = callee {
            assert_eq!(c.gpr[1], 0x1040);
            assert_eq!(c.gpr[2], 0x1234);
            assert_eq!(c.gpr[3], 0x1000);
            assert_eq!(c.lr, RETURN_GATE);
        }
        assert!(result.read(&m, 0x8000_0001).unwrap());
        assert!(!result.read(&m, 0x8000_0100).unwrap());
    }
}
