//! Installed trap identities and readable native entry veneers. No patching yet.
//! Inside Macintosh: Operating System Utilities, Trap Manager, pp. 8-27–8-32.
use crate::{
    memory::{GuestPtr, Memory, Permissions},
    Result,
};
pub const UNIMPLEMENTED: u32 = 0x7100_0000;
// Canonical OS trap words and Toolbox trap words. Modifier bits on OS traps
// select variants of one table entry, rather than creating distinct routines.
const INSTALLED: &[u16] = &[
    0xa86d, 0xa86e, 0xa86f, 0xa87d, 0xa873, 0xa874, 0xa875, 0xa87b, 0xa8a2, 0xa89c, 0xa89d, 0xa860,
    0xa02f, 0xa030, 0xa031, 0xa032, 0xa122, 0xa023, 0xa025, 0xa260, 0xa346, 0xa820, 0xa975, 0xa994,
    0xa9a0, 0xa9a1, 0xa9af, 0xa9f4,
];
pub fn address(number: u16, toolbox: bool) -> u32 {
    let mask = if toolbox { 0x3ff } else { 0xff };
    INSTALLED
        .iter()
        .position(|op| (*op & 0x800 != 0) == toolbox && *op & mask == number & mask)
        .map_or(UNIMPLEMENTED, |i| UNIMPLEMENTED + 4 + i as u32 * 4)
}
pub fn opcode(address: u32) -> Option<u16> {
    let offset = address.checked_sub(UNIMPLEMENTED + 4)?;
    if !offset.is_multiple_of(4) {
        return None;
    }
    INSTALLED.get((offset / 4) as usize).copied()
}
pub fn install(memory: &mut Memory) -> Result<()> {
    let mut code = vec![0xa8, 0x9f, 0x4e, 0x75]; // Unimplemented; RTS
    for op in INSTALLED {
        code.extend(op.to_be_bytes());
        code.extend(0x4e75u16.to_be_bytes());
    }
    memory.map(GuestPtr(UNIMPLEMENTED), code, Permissions::RX)
}
