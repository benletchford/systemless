//! Test helpers for trap handler unit tests.
//!
//! Provides a MockCpu implementing CpuOps, and a setup() helper that
//! creates a (TrapDispatcher, MockCpu, MacMemoryBus) tuple with sensible
//! defaults for testing.

use std::collections::HashMap;

use super::TrapDispatcher;
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};

/// Mock CPU for trap handler testing.
///
/// Stores registers in a HashMap and tracks CCR changes.
pub struct MockCpu {
    regs: HashMap<u8, u32>,
    pub ccr: u8,
}

impl MockCpu {
    pub fn new() -> Self {
        Self {
            regs: HashMap::new(),
            ccr: 0,
        }
    }

    fn reg_index(reg: Register) -> u8 {
        match reg {
            Register::D0 => 0,
            Register::D1 => 1,
            Register::D2 => 2,
            Register::D3 => 3,
            Register::D4 => 4,
            Register::D5 => 5,
            Register::D6 => 6,
            Register::D7 => 7,
            Register::A0 => 8,
            Register::A1 => 9,
            Register::A2 => 10,
            Register::A3 => 11,
            Register::A4 => 12,
            Register::A5 => 13,
            Register::A6 => 14,
            Register::A7 => 15,
            Register::PC => 16,
        }
    }
}

impl CpuOps for MockCpu {
    fn read_reg(&self, reg: Register) -> u32 {
        *self.regs.get(&Self::reg_index(reg)).unwrap_or(&0)
    }

    fn write_reg(&mut self, reg: Register, value: u32) {
        self.regs.insert(Self::reg_index(reg), value);
    }

    fn get_ccr(&self) -> u8 {
        self.ccr
    }

    fn set_ccr(&mut self, ccr: u8) {
        self.ccr = ccr;
    }
}

/// Standard stack pointer address for tests.
pub const TEST_SP: u32 = 0x100000;

/// Create a test environment with sensible defaults.
///
/// Returns (TrapDispatcher, MockCpu, MacMemoryBus) with:
/// - 4MB RAM
/// - A7 (SP) at TEST_SP (0x100000)
/// - A5 → QD globals area at 0x180000 (globals_ptr stored at 0x180000, actual globals at 0x180004)
/// - TickCount low-memory global ($016A) set to 100
/// - screenBits set up at $0824/$0828
pub fn setup() -> (TrapDispatcher, MockCpu, MacMemoryBus) {
    let mut dispatcher = TrapDispatcher::new();
    dispatcher.scrap.clipboard_writable = true;
    let mut cpu = MockCpu::new();
    let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

    // Set up stack pointer
    cpu.write_reg(Register::A7, TEST_SP);

    // Set up A5 → QD globals
    // A5 points to a location that contains a pointer to the QD globals area.
    // The QD globals area is 256 bytes; the pointer to the current port is at offset 0.
    let a5_addr = 0x180000u32;
    let qd_globals = 0x180004u32;
    cpu.write_reg(Register::A5, a5_addr);
    bus.write_long(a5_addr, qd_globals); // A5 → globals_ptr → qd_globals

    // Set up TickCount low-memory global
    bus.write_long(0x016A, 100);
    bus.write_word(
        crate::memory::globals::addr::SYS_EVT_MASK,
        crate::memory::globals::DEFAULT_SYS_EVT_MASK,
    );
    bus.write_word(
        crate::memory::globals::addr::MENU_FLASH,
        crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
    );
    // MBState ($0172) is 0 when the mouse button is down and $80 when it is
    // up. Real startup code initializes it to up; zero-filled test RAM would
    // otherwise look like a held mouse. Inside Macintosh Volume II, II-371.
    bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
    // Keep framebuffer tests from drawing into low memory. ScrnBase ($0824)
    // names the screen base address; the test dispatcher's screen mode must
    // match it. Inside Macintosh Volume II, II-371.
    let screen_base = 0x300000u32;
    let (_old_base, row_bytes, width, height, depth) = dispatcher.screen_mode;
    dispatcher.set_screen_mode_for_test(screen_base, row_bytes, width, height, depth);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);
    // Sync dispatcher tick_count (normally done by runner's advance_guest_tick)
    dispatcher.tick_count = 100;

    (dispatcher, cpu, bus)
}

/// Create a test environment and set up a basic GrafPort so drawing/port traps work.
///
/// Returns (TrapDispatcher, MockCpu, MacMemoryBus) with a port at 0x181000
/// that has valid bitmap, portRect, visRgn, and clipRgn.
pub fn setup_with_port() -> (TrapDispatcher, MockCpu, MacMemoryBus) {
    let (disp, cpu, mut bus) = setup();

    let port_ptr = 0x181000u32;
    let screen_base = bus.read_long(0x0824);

    // BitMap
    bus.write_word(port_ptr, 0); // device
    bus.write_long(port_ptr + 2, screen_base); // baseAddr
    bus.write_word(port_ptr + 6, 64); // rowBytes (512/8)
    bus.write_word(port_ptr + 8, 0); // bounds.top
    bus.write_word(port_ptr + 10, 0); // bounds.left
    bus.write_word(port_ptr + 12, 342); // bounds.bottom
    bus.write_word(port_ptr + 14, 512); // bounds.right

    // portRect
    bus.write_word(port_ptr + 16, 0); // top
    bus.write_word(port_ptr + 18, 0); // left
    bus.write_word(port_ptr + 20, 342); // bottom
    bus.write_word(port_ptr + 22, 512); // right

    // visRgn
    let vis_rgn = 0x182000u32;
    bus.write_word(vis_rgn, 10); // rgnSize
    bus.write_word(vis_rgn + 2, 0); // top
    bus.write_word(vis_rgn + 4, 0); // left
    bus.write_word(vis_rgn + 6, 342); // bottom
    bus.write_word(vis_rgn + 8, 512); // right
    let vis_rgn_handle = 0x182100u32;
    bus.write_long(vis_rgn_handle, vis_rgn);
    bus.write_long(port_ptr + 24, vis_rgn_handle);

    // clipRgn
    let clip_rgn = 0x182200u32;
    bus.write_word(clip_rgn, 10);
    bus.write_word(clip_rgn + 2, 0);
    bus.write_word(clip_rgn + 4, 0);
    bus.write_word(clip_rgn + 6, 342);
    bus.write_word(clip_rgn + 8, 512);
    let clip_rgn_handle = 0x182300u32;
    bus.write_long(clip_rgn_handle, clip_rgn);
    bus.write_long(port_ptr + 28, clip_rgn_handle);

    // Pen defaults
    bus.write_bytes(port_ptr + 32, &[0x00; 8]); // bkPat
    bus.write_bytes(port_ptr + 40, &[0xFF; 8]); // fillPat
    bus.write_long(port_ptr + 48, 0); // pnLoc
    bus.write_word(port_ptr + 52, 1); // pnSize.v
    bus.write_word(port_ptr + 54, 1); // pnSize.h
    bus.write_word(port_ptr + 56, 8); // pnMode
    bus.write_bytes(port_ptr + 58, &[0xFF; 8]); // pnPat
    bus.write_word(port_ptr + 66, 0); // pnVis
    bus.write_word(port_ptr + 68, 0); // txFont
    bus.write_word(port_ptr + 70, 0); // txFace
    bus.write_word(port_ptr + 72, 1); // txMode
    bus.write_word(port_ptr + 74, 0); // txSize

    // Set current port in QD globals
    let a5 = cpu.read_reg(Register::A5);
    let globals_ptr = bus.read_long(a5);
    bus.write_long(globals_ptr, port_ptr);

    (disp, cpu, bus)
}
