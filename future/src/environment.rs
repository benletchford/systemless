//! One virtual Macintosh compatibility profile, independent of caller ISA.
//! SysEnvRec layout/result codes: Inside Macintosh: Operating System Utilities,
//! Gestalt Manager, pp. 1-28 and 1-32--1-33. Flags describe implemented services.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
pub fn query(memory: &mut Memory, version: i16, record: GuestPtr) -> Result<i16> {
    if version <= 0 {
        return Ok(-5501);
    }
    if record.0 == 0 {
        return Ok(-50);
    }
    // Unknown virtual model/keyboard, System 7 compatibility target, 68020.
    // No installed System file, AppleTalk driver, FPU or Color QuickDraw.
    let mut bytes = vec![0; 16];
    for (offset, value) in [(0, 2u16), (4, 0x0700), (6, 3)] {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    memory.write(record, &bytes)?;
    Ok(if version > 2 { -5502 } else { 0 })
}
