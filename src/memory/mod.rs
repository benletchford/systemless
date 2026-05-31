//! Memory subsystem for Mac emulation
//!
//! Provides Big-Endian memory access and Mac low-memory globals.

pub mod bus;
pub mod globals;

pub use bus::{
    arm_watchpoint, disarm_watchpoint, get_step, increment_step, set_current_pc,
    set_watch_registers, watchpoint_armed, STEP_COUNTER,
};
pub use bus::{MacMemoryBus, MemoryBus};
pub use globals::LowMemGlobals;

// `#[inline]` on each method so LLVM inlines the delegation through to
// the fast-path bus read/write implementations. Without inline, each
// M68K instruction fetch goes through a virtual call boundary.
impl m68k::AddressBus for MacMemoryBus {
    #[inline]
    fn read_byte(&mut self, addr: u32) -> u8 {
        MemoryBus::read_byte(self, addr)
    }
    #[inline]
    fn write_byte(&mut self, addr: u32, val: u8) {
        MemoryBus::write_byte(self, addr, val)
    }
    #[inline]
    fn read_word(&mut self, addr: u32) -> u16 {
        MemoryBus::read_word(self, addr)
    }
    #[inline]
    fn write_word(&mut self, addr: u32, val: u16) {
        MemoryBus::write_word(self, addr, val)
    }
    #[inline]
    fn read_long(&mut self, addr: u32) -> u32 {
        MemoryBus::read_long(self, addr)
    }
    #[inline]
    fn write_long(&mut self, addr: u32, val: u32) {
        MemoryBus::write_long(self, addr, val)
    }
}
