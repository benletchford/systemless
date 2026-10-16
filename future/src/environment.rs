//! One virtual Macintosh compatibility profile, independent of caller ISA.
//! SysEnvRec layout/result codes: Inside Macintosh: Operating System Utilities,
//! Gestalt Manager, pp. 1-28 and 1-32--1-33. Flags describe implemented services.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
pub const SYSTEM_VERSION: u16 = 0x0700;
pub const PROCESSOR: u16 = 3;
/// Only capabilities implemented by this virtual Macintosh are registered.
pub fn gestalt(selector: u32) -> std::result::Result<u32, i16> {
    match &selector.to_be_bytes() {
        b"sysv" => Ok(u32::from(SYSTEM_VERSION)),
        b"snd " => Ok(crate::sound::ATTRIBUTES),
        b"proc" => Ok(u32::from(PROCESSOR)),
        b"addr" => Ok(crate::addressing::ATTRIBUTES),
        _ => Err(-5551),
    }
}
pub fn query(memory: &mut Memory, version: i16, record: GuestPtr) -> Result<i16> {
    query_display(memory, version, record, crate::display::Display::default())
}
pub(crate) fn query_display(
    memory: &mut Memory,
    version: i16,
    record: GuestPtr,
    display: crate::display::Display,
) -> Result<i16> {
    if version <= 0 {
        return Ok(-5501);
    }
    if record.0 == 0 {
        return Ok(-50);
    }
    // Unknown virtual model/keyboard, System 7 compatibility target, 68020.
    // No installed System file, AppleTalk driver or FPU. Color follows the
    // real configured indexed display; unsupported Color QuickDraw calls fault.
    let mut bytes = vec![0; 16];
    for (offset, value) in [(0, 2u16), (4, SYSTEM_VERSION), (6, PROCESSOR)] {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    bytes[9] = u8::from(display.depth() > 1);
    memory.write(record, &bytes)?;
    Ok(if version > 2 { -5502 } else { 0 })
}
