//! Hand-assembled real machine code. The gates/records are a private fixture
//! protocol, intentionally not advertised as Macintosh application loading.
use crate::{
    cpu::{Architecture, Cpu, IMPORT_BASE, RETURN_GATE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Runtime,
    Result,
};
pub const PIXELS: u32 = 0x4000;
pub const COMMAND: u32 = 0x3000;
pub fn program(arch: Architecture, draw: bool) -> Vec<u8> {
    match arch {
        Architecture::M68k => {
            let mut words = vec![0x2f3c, 0, 7, 0xa000]; // move.l #7,-(sp); private Add
            if draw {
                words.extend([0x13fc, 5, 0, PIXELS as u16]); // move.b #5,absolute.l
                words.extend([0x2f3c, 0, COMMAND as u16, 0xa001]);
            }
            words.extend([0x2f3c, 0, 0, 0xa003]);
            words.into_iter().flat_map(u16::to_be_bytes).collect()
        }
        Architecture::PowerPc => {
            let mut words = vec![0x38600007]; // li r3,7
            words.push(branch_link(0x1004, IMPORT_BASE));
            if draw {
                words.extend([0x38800005, 0x98800000 | PIXELS, 0x38600000 | COMMAND]);
                words.push(branch_link(0x1014, IMPORT_BASE + 4));
            }
            words.push(0x38600000);
            let pc = 0x1000 + words.len() as u32 * 4;
            words.push(branch_link(pc, IMPORT_BASE + 12));
            words.into_iter().flat_map(u32::to_be_bytes).collect()
        }
    }
}
pub fn branch_link(pc: u32, target: u32) -> u32 {
    0x48000001 | (target.wrapping_sub(pc) & 0x03fffffc)
}
pub fn runtime(arch: Architecture, draw: bool) -> Result<Runtime> {
    let mut memory = Memory::default();
    memory.map(GuestPtr(0x1000), program(arch, draw), Permissions::RX)?;
    memory.map(GuestPtr(0x2000), vec![0; 0x1000], Permissions::RW)?;
    let mut command = vec![0; 16];
    command[..10].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4, 3, 2]); // XOR 3 over 4x4
    memory.map(GuestPtr(COMMAND), command, Permissions::RW)?;
    memory.map(GuestPtr(PIXELS), vec![0; 64], Permissions::RW)?;
    memory.write(GuestPtr(0x2ff0), &RETURN_GATE.to_be_bytes())?;
    Ok(Runtime::new(memory, Cpu::new(arch, 0x1000, 0x2ff0), None))
}
