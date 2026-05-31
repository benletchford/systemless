//! Canonical guest machine profile used by Systemless's accuracy harness.
//!
//! Frozen reference values for "what does the guest see when it asks
//! about the host machine" — Gestalt selectors, screen geometry, RAM
//! size, VBL rate, etc. There's exactly one shipped profile
//! ([`BASILISK_II_PLAY_PROFILE`]); a const alias [`ORACLE_MACHINE_PROFILE`]
//! exposes it under the role-name the trap dispatcher uses.
//!
//! Library consumers don't normally need to read this directly — the
//! Memory Manager, Gestalt, and screen-mode init paths in
//! [`crate::trap`] consult it on the consumer's behalf.

use m68k::CpuType;

/// Bag of constants describing one canonical guest machine: Gestalt
/// selector responses, screen geometry, RAM size, VBL rate, realtime
/// CPU MHz target.
///
/// Field accessors are field-name-direct (`profile.screen_width`)
/// since downstream callers compare specific values rather than
/// treating the profile as opaque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MachineProfile {
    /// Mac model ID — what `gestaltMachineType` returns.
    pub model_id: i16,
    /// Gestalt 'mach' selector response.
    pub gestalt_machine_type: u16,
    /// System version in BCD (e.g. 0x0753 = System 7.5.3).
    pub system_version_bcd: u16,
    /// Gestalt 'cput' selector — `gestaltCPU68040 = 4` per
    /// IM:Operating System Utilities 1994.
    pub gestalt_native_cpu_type: u32,
    /// Gestalt 'proc' selector — `gestalt68040 = 5` (legacy numbering;
    /// not the same scheme as `gestalt_native_cpu_type`).
    pub gestalt_processor_type: u32,
    /// Gestalt 'fpu ' selector — non-zero implies FPU presence.
    pub gestalt_fpu_type: u32,
    /// Gestalt 'mmu ' selector — MMU type.
    pub gestalt_mmu_type: u32,
    /// Size of the guest RAM region in bytes. Must accommodate the
    /// largest resource fork the profile's games unpack into the heap
    /// (Bonkheads Deluxe peaks ~30 MB during merge — see comment on
    /// [`BASILISK_II_PLAY_PROFILE`]).
    pub ram_size_bytes: u32,
    /// Framebuffer width in pixels.
    pub screen_width: u16,
    /// Framebuffer height in pixels.
    pub screen_height: u16,
    /// Framebuffer depth in bits per pixel (typically 8 for indexed
    /// 8bpp screens with the standard Mac CLUT).
    pub screen_depth: u16,
    /// VBL interrupt rate in Hz. 60.15 matches Compact Mac timing.
    pub vbl_hz: f64,
    /// Target instruction throughput for realtime frontends, in
    /// MHz × 1,000,000 instructions/sec equivalent. Used by
    /// `systemless`'s wall-clock pacing — non-realtime callers
    /// (scripted harnesses, tests) ignore this.
    pub realtime_cpu_mhz: f64,
}

impl MachineProfile {
    /// Concrete `m68k::CpuType` corresponding to this profile.
    /// Hardcoded to `M68040` for now — the only shipped profile is
    /// the Basilisk-II play machine, which is a Quadra 900 (68040).
    pub fn cpu_type(self) -> CpuType {
        CpuType::M68040
    }

    /// True when the guest exposes an FPU via Gestalt
    /// (`gestalt_fpu_type != 0`). Const-eval friendly so
    /// trap-table builders can branch on it at compile time.
    pub const fn has_fpu(self) -> bool {
        self.gestalt_fpu_type != 0
    }

    /// Bytes per scanline for an 8bpp screen of this profile's
    /// geometry. Equal to `screen_width` in this trivial mapping
    /// (1 byte/pixel at depth 8); kept as a method so deeper
    /// pixel formats can override later without ripple changes.
    pub const fn screen_row_bytes(self) -> u32 {
        self.screen_width as u32
    }
}

/// Basilisk maps model ID 14 to a Quadra 900 / Gestalt machine type 20.
/// `gestalt_native_cpu_type = 4` per IM:Operating System Utilities 1994
/// (line 1439, line 2299): `gestaltCPU68040 = $004` under the
/// `gestaltNativeCPUtype` ('cput') selector — value 5 there is
/// `gestaltCPU68LC040`, which contradicts this profile's 68040 FPU.
/// `gestalt_processor_type = 5` because the legacy `gestaltProcessorType`
/// ('proc') selector uses its own numbering where `gestalt68040 = 5`
/// (IM:OSU line 1470).
pub const BASILISK_II_PLAY_PROFILE: MachineProfile = MachineProfile {
    model_id: 14,
    gestalt_machine_type: 20,
    system_version_bcd: 0x0753,
    gestalt_native_cpu_type: 4,
    gestalt_processor_type: 5,
    gestalt_fpu_type: 3,
    gestalt_mmu_type: 4,
    // Bonkheads_Deluxe peaks above 30 MB during resource-fork merge (it
    // bundles ~669 resources, several individual chunks > 250 KB), which
    // exhausts the 32 MB-default heap before the title even renders.
    // 64 MB matches what real-world Power Macintosh users would have
    // configured for that era of game and clears the OOM without
    // cascading failures elsewhere.
    ram_size_bytes: 64 * 1024 * 1024,
    screen_width: 800,
    screen_height: 600,
    screen_depth: 8,
    vbl_hz: 60.15,
    realtime_cpu_mhz: 25.0,
};

pub const ORACLE_MACHINE_PROFILE: MachineProfile = BASILISK_II_PLAY_PROFILE;

/// Returns the oracle machine profile, optionally overridden by
/// `SYSTEMLESS_SCREEN_WIDTH` and `SYSTEMLESS_SCREEN_HEIGHT` environment variables.
pub fn oracle_machine_profile() -> MachineProfile {
    let mut p = ORACLE_MACHINE_PROFILE;
    if let Ok(w) = std::env::var("SYSTEMLESS_SCREEN_WIDTH") {
        if let Ok(w) = w.parse::<u16>() {
            p.screen_width = w;
        }
    }
    if let Ok(h) = std::env::var("SYSTEMLESS_SCREEN_HEIGHT") {
        if let Ok(h) = h.parse::<u16>() {
            p.screen_height = h;
        }
    }
    p
}
