//! Shared guest Toolbox dispatch table and readable built-in entry veneers.
//! Inside Macintosh: Operating System Utilities, Trap Manager, pp. 8-27–8-32.
use crate::{
    memory::{GuestPtr, Memory, Permissions},
    Result,
};
pub const UNIMPLEMENTED: u32 = 0x7100_0000;
const TOOLBOX_TABLE: u32 = 0x7110_0000;
// Canonical OS trap words and Toolbox trap words. Modifier bits on OS traps
// select variants of one table entry, rather than creating distinct routines.
const INSTALLED: &[u16] = &[
    0xa8e4, 0xa8e5, 0xa8e6, 0xa8e7, 0xa8e8, 0xa8e2, 0xa8d8, 0xa8d9, 0xa8dc, 0xa8df, 0xa879, 0xa87a,
    0xa870, 0xa871, 0xa86d, 0xa86e, 0xa86f, 0xa87d, 0xa873, 0xa874, 0xa875, 0xa87b, 0xa8a2, 0xa89c,
    0xa89d, 0xa860, 0xa02f, 0xa030, 0xa031, 0xa032, 0xa004, 0xa055, 0xa122, 0xa029, 0xa02a, 0xa023,
    0xa024, 0xa025, 0xa260, 0xa346, 0xa820, 0xa975, 0xa647, 0xa994, 0xa9a0, 0xa9a1, 0xa9a3, 0xa9af,
    0xa9f4, 0xaa5a, 0xa861, 0xa8b0, 0xa8b1, 0xa9b8, 0xa862, 0xa863, 0xa11e, 0xa01f, 0xa020, 0xa021,
    0xa9d3, 0xa9cf, 0xa9d0, 0xa9d2, 0xa83e, 0xa9cc, 0xa9cb, 0xa9cd, 0xa83d, 0xa885, 0xa886, 0xa883,
    0xa884, 0xa88c, 0xa88d, 0xa893, 0xa8fe, 0xa88b, 0xa887, 0xa888, 0xa889, 0xa88a, 0xa912, 0xa913,
    0xa924, 0xa92d, 0xa914, 0xa817, 0xa9a8, 0xa039, 0xa8dd, 0xa878, 0xa8e0, 0xa97f, 0xa8ae, 0xa939,
    0xa93a, 0xa938, 0xa93d, 0xaa66, 0xa86a, 0xa86b, 0xa959, 0xa95c, 0xa9bb, 0xa910, 0xa9ea, 0xa009,
    0xa209, 0xa807, 0xa801,
];
pub fn address(number: u16, toolbox: bool) -> u32 {
    let mask = if toolbox { 0x3ff } else { 0xff };
    INSTALLED
        .iter()
        .position(|op| (*op & 0x800 != 0) == toolbox && *op & mask == number & mask)
        .map_or(UNIMPLEMENTED, |i| UNIMPLEMENTED + 4 + i as u32 * 4)
}
/// The table is process-wide guest state, shared by both ABI adapters.
pub fn lookup(memory: &Memory, number: u16, toolbox: bool) -> Result<u32> {
    if toolbox {
        memory.u32(GuestPtr(TOOLBOX_TABLE + u32::from(number & 0x3ff) * 4))
    } else {
        Ok(address(number, false))
    }
}
pub fn set_toolbox(memory: &mut Memory, number: u16, target: u32) -> Result<()> {
    // Classic setters store addresses; validate execution when dispatched.
    memory.write(
        GuestPtr(TOOLBOX_TABLE + u32::from(number & 0x3ff) * 4),
        &target.to_be_bytes(),
    )
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
    memory.map(GuestPtr(UNIMPLEMENTED), code, Permissions::RX)?;
    let mut table = Vec::with_capacity(4096);
    for number in 0..1024 {
        table.extend(address(number, true).to_be_bytes());
    }
    memory.map(GuestPtr(TOOLBOX_TABLE), table, Permissions::RW)
}
