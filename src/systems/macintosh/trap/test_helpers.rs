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
    dispatcher.scrap.set_clipboard_writable(true);
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

    // Match automatic resource loading at application startup (IM I-118).
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);

    // Set up TickCount low-memory global
    bus.write_long(0x016A, 100);
    bus.write_word(
        crate::memory::globals::addr::SYS_EVT_MASK,
        crate::memory::globals::DEFAULT_SYS_EVT_MASK,
    );
    bus.write_word(
        crate::memory::globals::addr::KEY_THRESH,
        crate::memory::globals::DEFAULT_AUTO_KEY_THRESHOLD_TICKS,
    );
    bus.write_word(
        crate::memory::globals::addr::KEY_REP_THRESH,
        crate::memory::globals::DEFAULT_AUTO_KEY_RATE_TICKS,
    );
    bus.write_word(
        crate::memory::globals::addr::MENU_FLASH,
        crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
    );
    bus.write_long(crate::memory::globals::addr::CARET_TIME, crate::memory::globals::DEFAULT_CARET_TIME_TICKS);
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
    // Import the guest-owned low-memory Ticks value into the host pacing
    // snapshot (normally done at the runner's ABI boundary).
    dispatcher.read_tick_count(&bus);

    (dispatcher, cpu, bus)
}

/// Create a classic process environment with live writable trap tables.
/// Patch fixtures use the same topology and setter as initialized runners.
/// Inside Macintosh: Operating System Utilities (1994), pp. 8-4--8-6.
pub fn setup_with_trap_tables() -> (TrapDispatcher, MockCpu, MacMemoryBus) {
    let (mut dispatcher, cpu, mut bus) = setup();
    dispatcher
        .materialize_trap_tables(&mut bus, super::dispatch::TrapTableProfile::M68k68040)
        .expect("trap table construction requires writable cells and system storage");
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

/// An 8-bit offscreen CGrafPort for differential drawing tests, described
/// as plain data so the PowerPC path can build the same port.
pub(crate) struct IndexedPortFixture<'a> {
    /// The PixMap bounds, in port coordinates (top, left, bottom, right).
    pub(crate) bounds: (i16, i16, i16, i16),
    pub(crate) row_bytes: u16,
    /// Initial pixel bytes, `row_bytes` per row.
    pub(crate) pixels: &'a [u8],
    /// The resolved fgColor/bkColor pixel values.
    pub(crate) fg_index: u8,
    pub(crate) bg_index: u8,
    /// visRgn and clipRgn as a bounding box plus region data words.
    pub(crate) vis: ((i16, i16, i16, i16), &'a [i16]),
    pub(crate) clip: ((i16, i16, i16, i16), &'a [i16]),
}

/// The region storage QuickDraw keeps behind a region handle: size,
/// bounding box, then the data words.
pub(crate) fn region_storage_bytes(bbox: (i16, i16, i16, i16), data: &[i16]) -> Vec<u8> {
    let size = 10 + 2 * data.len();
    let mut bytes = (size as u16).to_be_bytes().to_vec();
    for word in [bbox.0, bbox.1, bbox.2, bbox.3].iter().chain(data) {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes
}

/// Runs the 68K FillRect drawing path, `draw_rect` with `ShapeOp::Fill`,
/// on `fixture` and returns the pixel bytes afterwards.
pub(crate) fn fill_rect_8bpp_68k(
    fixture: &IndexedPortFixture<'_>,
    (top, left, bottom, right): (i16, i16, i16, i16),
    pattern: [u8; 8],
) -> Vec<u8> {
    let (mut d, mut cpu, mut bus) = setup();
    let base = bus.alloc(fixture.pixels.len() as u32);
    bus.write_bytes(base, fixture.pixels);
    let pixmap_handle = bus.alloc(4);
    let pixmap = bus.alloc(50);
    bus.write_long(pixmap_handle, pixmap);
    bus.write_long(pixmap, base);
    bus.write_word(pixmap + 4, 0x8000 | fixture.row_bytes);
    let (b_top, b_left, b_bottom, b_right) = fixture.bounds;
    for (offset, word) in [(6, b_top), (8, b_left), (10, b_bottom), (12, b_right)] {
        bus.write_word(pixmap + offset, word as u16);
    }
    bus.write_word(pixmap + 30, 0); // pixelType: indexed
    bus.write_word(pixmap + 32, 8); // pixelSize
    bus.write_word(pixmap + 34, 1); // cmpCount
    bus.write_word(pixmap + 36, 8); // cmpSize
    bus.write_long(pixmap + 42, 0); // pmTable

    let port = bus.alloc(108);
    bus.write_long(port + 2, pixmap_handle);
    bus.write_word(port + 6, 0xC000);
    for (offset, word) in [(16, b_top), (18, b_left), (20, b_bottom), (22, b_right)] {
        bus.write_word(port + offset, word as u16);
    }
    for (offset, (bbox, data)) in [(24, fixture.vis), (28, fixture.clip)] {
        let storage = region_storage_bytes(bbox, data);
        let handle = bus.alloc(4);
        let ptr = bus.alloc(storage.len() as u32);
        bus.write_bytes(ptr, &storage);
        bus.write_long(handle, ptr);
        bus.write_long(port + offset, handle);
    }
    bus.write_long(port + 80, u32::from(fixture.fg_index));
    bus.write_long(port + 84, u32::from(fixture.bg_index));
    d.set_current_port_state(&mut bus, &mut cpu, port, None);
    // fgColor and bkColor hold the resolved pixel values above.
    d.resolved_port_color_fields.insert(port, 0x03);

    let rect = super::types::Rect {
        top,
        left,
        bottom,
        right,
    };
    d.draw_rect(
        &mut cpu,
        &mut bus,
        &rect,
        super::types::ShapeOp::Fill(pattern),
    );
    bus.read_bytes(base, fixture.pixels.len())
}
