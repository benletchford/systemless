//! MPW non-AltiVec PowerPC setjmp.h ABI: 64 longwords in guest memory.
//! The buffer remains authoritative, including after a guest copies it.
use crate::{
    cpu::Cpu,
    memory::{GuestPtr, Memory},
    Error, Result,
};

pub(crate) fn save(cpu: &mut Cpu, memory: &mut Memory, buffer: GuestPtr) -> Result<()> {
    let Cpu::PowerPc(c) = cpu else {
        return Err(Error::Cpu("PPC jump ABI on 68K".into()));
    };
    // Read reserved bytes first, then commit one checked write. Faults cannot
    // partially update the buffer or the return register.
    let mut bytes: [u8; 256] = memory.read(buffer, 256)?.try_into().unwrap();
    for (offset, value) in [(0, c.lr), (4, c.cr), (8, c.gpr[1]), (12, c.gpr[2])] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
    for (i, value) in c.gpr[13..32].iter().enumerate() {
        bytes[20 + i * 4..24 + i * 4].copy_from_slice(&value.to_be_bytes());
    }
    for (i, value) in c.fpr[14..32].iter().enumerate() {
        bytes[96 + i * 8..104 + i * 8].copy_from_slice(&value.to_be_bytes());
    }
    bytes[240..248].copy_from_slice(&u64::from(c.fpscr).to_be_bytes());
    memory.write(buffer, &bytes)?;
    c.gpr[3] = 0;
    Ok(())
}

pub(crate) fn restore(cpu: &mut Cpu, memory: &Memory, buffer: GuestPtr, value: u32) -> Result<()> {
    let Cpu::PowerPc(c) = cpu else {
        return Err(Error::Cpu("PPC jump ABI on 68K".into()));
    };
    let bytes = memory.read(buffer, 256)?;
    let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
    c.lr = word(0);
    c.cr = word(4);
    c.gpr[1] = word(8);
    c.gpr[2] = word(12);
    for i in 0..19 {
        c.gpr[13 + i] = word(20 + i * 4);
    }
    for i in 0..18 {
        c.fpr[14 + i] = u64::from_be_bytes(bytes[96 + i * 8..104 + i * 8].try_into().unwrap());
    }
    c.fpscr = word(244);
    // MPW longjmp's mr. tests the supplied value before converting zero to one.
    let condition = if (value as i32) < 0 {
        8
    } else if value == 0 {
        2
    } else {
        4
    };
    c.cr = (c.cr & 0x0fff_ffff) | ((condition | ((c.xer >> 31) & 1)) << 28);
    c.gpr[3] = if value == 0 { 1 } else { value };
    c.pc = c.lr;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cpu::Architecture, memory::Permissions};
    #[test]
    fn copied_buffer_restores_nonvolatile_registers_and_zero_becomes_one() {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0xa5; 512], Permissions::RW)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::PowerPc, 0x2000, 0x3000);
        let Cpu::PowerPc(c) = &mut cpu else {
            unreachable!()
        };
        c.lr = 0x4560;
        c.cr = 0x12345678;
        c.gpr[2] = 0x87654321;
        c.gpr[13] = 0x11223344;
        c.gpr[31] = 0x55667788;
        c.fpr[14] = 0x7ff8123456789abc;
        c.fpr[31] = 0xfff0000000000000;
        c.fpscr = 0x123456;
        save(&mut cpu, &mut memory, GuestPtr(0x1000)).unwrap();
        assert_eq!(memory.u32(GuestPtr(0x1014)).unwrap(), 0x11223344);
        assert_eq!(memory.u32(GuestPtr(0x1010)).unwrap(), 0xa5a5a5a5);
        assert_eq!(memory.u32(GuestPtr(0x10fc)).unwrap(), 0xa5a5a5a5);
        let bytes = memory.read(GuestPtr(0x1000), 256).unwrap().to_vec();
        memory.write(GuestPtr(0x1100), &bytes).unwrap();
        cpu = Cpu::new(Architecture::PowerPc, 0, 0);
        restore(&mut cpu, &memory, GuestPtr(0x1100), 0).unwrap();
        let Cpu::PowerPc(c) = &cpu else {
            unreachable!()
        };
        assert_eq!(
            (c.pc, c.lr, c.gpr[1], c.gpr[2], c.gpr[3]),
            (0x4560, 0x4560, 0x3000, 0x87654321, 1)
        );
        assert_eq!((c.gpr[13], c.gpr[31]), (0x11223344, 0x55667788));
        assert_eq!(
            (c.fpr[14], c.fpr[31], c.fpscr),
            (0x7ff8123456789abc, 0xfff0000000000000, 0x123456)
        );
        assert_eq!(c.cr, 0x22345678);
    }
    #[test]
    fn buffer_faults_leave_cpu_and_memory_untouched() {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0x5a; 256], Permissions::RX)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::PowerPc, 0x2000, 0x3000);
        let Cpu::PowerPc(c) = &mut cpu else {
            unreachable!()
        };
        c.gpr[3] = 42;
        assert!(save(&mut cpu, &mut memory, GuestPtr(0x1000)).is_err());
        assert_eq!(memory.read(GuestPtr(0x1000), 256).unwrap(), &[0x5a; 256]);
        assert!(restore(&mut cpu, &memory, GuestPtr(0x1004), 7).is_err());
        let Cpu::PowerPc(c) = &cpu else {
            unreachable!()
        };
        assert_eq!((c.pc, c.gpr[1], c.gpr[3]), (0x2000, 0x3000, 42));
    }
}
