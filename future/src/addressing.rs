//! Address-mode contract for the current flat 32-bit virtual Macintosh.
//! IM: Memory, Memory Management Utilities, pp.4-26--4-27.
//! A 24-bit transition needs real translation, not just a different flag byte.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
pub const MMU32_BIT: GuestPtr = GuestPtr(0x0cb2);
pub const STARTUP_MODE: u8 = 1;
/// Started in 32-bit mode and supports 32-bit addresses. System-zone
/// compatibility is not advertised until a distinct system-zone model exists.
pub const ATTRIBUTES: u32 = (1 << 0) | (1 << 2);
pub(crate) fn install(memory: &mut Memory) -> Result<()> {
    memory.write(MMU32_BIT, &[STARTUP_MODE])
}
pub fn current(memory: &Memory) -> Result<u8> {
    let mode = memory.read(MMU32_BIT, 1)?[0];
    if mode != STARTUP_MODE {
        return Err(Error::AddressMode(mode));
    }
    Ok(mode)
}
pub fn swap(memory: &Memory, requested: u8) -> Result<u8> {
    if requested != STARTUP_MODE {
        return Err(Error::AddressMode(requested));
    }
    current(memory)
}
