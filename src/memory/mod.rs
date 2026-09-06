//! Memory subsystem for Mac emulation
//!
//! Provides Big-Endian memory access and Mac low-memory globals.

mod address_space;
pub mod bus;
pub mod globals;

pub use address_space::GuestAddressSpace;
pub(crate) use address_space::{
    flat_memory_route, GuestMemoryRoute, SharedGuestAddressSpace,
};
pub use bus::{
    arm_watchpoint, disarm_watchpoint, get_step, increment_step, set_current_pc,
    set_watch_registers, watchpoint_armed, STEP_COUNTER,
};
pub use bus::{MacMemoryBus, MemoryBus};
pub use globals::LowMemGlobals;

pub(crate) const APP_HEAP_COMPAT_FREE_FLOOR: u32 = 24 * 1024 * 1024;
pub(crate) const APP_HEAP_COMPAT_FREE_CAP: u32 = 64 * 1024 * 1024;
const EXPLICIT_PARTITION_HEADROOM: u32 = 1024 * 1024;

/// Estimate caller-visible free application heap bytes from the Memory
/// Manager low-memory globals.
///
/// Systemless normally gives a launched app most of guest RAM and keeps the
/// historical 24 MB compatibility floor for titles with optimistic startup
/// gates. When `ApplLimit` has been deliberately set far below `MemTop`
/// (for example from an application's `'SIZE'` resource), report the real
/// partition span so memory-warning and fallback paths can observe it.
pub(crate) fn app_heap_free_bytes(bus: &MacMemoryBus) -> u32 {
    let heap_end = bus.read_long(globals::addr::HEAP_END);
    let appl_limit = bus.read_long(globals::addr::APPL_LIMIT);
    let raw_span = appl_limit.saturating_sub(heap_end);
    let raw = app_zone_free_bytes(bus).unwrap_or(raw_span);
    let mem_top = bus.read_long(globals::addr::MEM_TOP);
    let explicitly_partitioned = mem_top != 0
        && appl_limit != 0
        && appl_limit.saturating_add(EXPLICIT_PARTITION_HEADROOM) < mem_top;

    if explicitly_partitioned {
        raw.min(APP_HEAP_COMPAT_FREE_CAP)
    } else {
        raw.clamp(APP_HEAP_COMPAT_FREE_FLOOR, APP_HEAP_COMPAT_FREE_CAP)
    }
}

fn app_zone_free_bytes(bus: &MacMemoryBus) -> Option<u32> {
    let app_zone = bus.read_long(globals::addr::APP_L_ZONE);
    let appl_limit = bus.read_long(globals::addr::APPL_LIMIT);
    if app_zone == 0 || appl_limit <= app_zone {
        return None;
    }

    // Memory 1992, pp. 2-20 and 2-66: `zcbFree` is the number of free
    // bytes remaining in the zone, and FreeMem reports that current-zone
    // free space. This matters after MaxApplZone expands HeapEnd to
    // ApplLimit; ApplLimit-HeapEnd is then zero even though the zone still
    // contains free bytes.
    let bk_lim = bus.read_long(app_zone);
    let zcb_free = bus.read_long(app_zone + 12);
    let zone_size = appl_limit.saturating_sub(app_zone);
    if bk_lim == appl_limit && zcb_free > 0 && zcb_free <= zone_size {
        Some(zcb_free)
    } else {
        None
    }
}

pub(crate) fn app_partition_size_bytes(bus: &MacMemoryBus) -> u32 {
    let app_zone = bus.read_long(globals::addr::APP_L_ZONE);
    let appl_limit = bus.read_long(globals::addr::APPL_LIMIT);
    if app_zone != 0 && appl_limit > app_zone {
        appl_limit - app_zone
    } else {
        bus.ram_size()
    }
}

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

    /// Guest RAM is one flat side-effect-free array, so expose it all to
    /// the m68k batch loop. Returns `None` while bus-access diagnostics
    /// (tracers/watchpoints) are active so they keep seeing every access.
    #[inline]
    fn fast_mem(&mut self) -> Option<m68k::FastMem> {
        let (ptr, len) = self.fast_mem_window()?;
        Some(m68k::FastMem { ptr, base: 0, len })
    }
}

pub(crate) mod presentation;
