//! Canonical guest machine profile used by Systemless's accuracy harness.
//!
//! Frozen reference values for "what does the guest see when it asks
//! about the host machine" — Gestalt selectors, screen geometry, RAM
//! size, VBL rate, etc. There's exactly one shipped profile
//! ([`BASILISK_II_PLAY_PROFILE`]); a const alias [`REFERENCE_MACHINE_PROFILE`]
//! exposes it under the role-name the trap dispatcher uses.
//!
//! Library consumers don't normally need to read this directly — the
//! Memory Manager, Gestalt, and screen-mode init paths in
//! [`crate::trap`] consult it on the consumer's behalf.

use m68k::CpuType;

/// Bag of constants describing one canonical guest machine: Gestalt
/// selector responses, screen geometry, RAM size, VBL rate, realtime
/// guest-advertised CPU MHz.
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
    /// Guest-visible effective CPU speed in MHz, returned by
    /// `GetCPUSpeed`.
    pub realtime_cpu_mhz: f64,
}

/// Guest-visible processor capabilities for one execution architecture.
///
/// This is crate-private so the trap adapters can share one value record
/// without expanding the public [`MachineProfile`] shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GuestExecutionCapabilities {
    /// `gestaltSysArchitecture` response, or `None` when the adapter does not
    /// support that selector.
    pub(crate) system_architecture: Option<u32>,
    pub(crate) native_cpu_type: u32,
    pub(crate) processor_type: u32,
    pub(crate) fpu_type: u32,
    pub(crate) mmu_type: u32,
}

/// Host-only execution rates. These values control work performed between
/// guest ticks; they do not describe values returned through guest APIs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HostExecutionPolicy {
    pub(crate) scripted_instructions_per_tick: u32,
    pub(crate) realtime_m68k_cpu_mhz: f64,
    pub(crate) realtime_powerpc_cpu_mhz: f64,
}

pub(crate) const DEFAULT_HOST_EXECUTION_POLICY: HostExecutionPolicy = HostExecutionPolicy {
    scripted_instructions_per_tick: 12_000,
    realtime_m68k_cpu_mhz: 25.0,
    realtime_powerpc_cpu_mhz: 120.0,
};

impl MachineProfile {
    /// 68K guest capabilities derived from this profile's existing fields.
    pub(crate) const fn m68k_execution_capabilities(self) -> GuestExecutionCapabilities {
        GuestExecutionCapabilities {
            // Inside Macintosh: Operating System Utilities (1994), p. 1-24:
            // gestalt68k = 1.
            system_architecture: Some(1),
            native_cpu_type: self.gestalt_native_cpu_type,
            processor_type: self.gestalt_processor_type,
            fpu_type: self.gestalt_fpu_type,
            mmu_type: self.gestalt_mmu_type,
        }
    }

    /// Native PowerPC capabilities, preserving the adapter's 604 identity.
    /// FPU and MMU values intentionally retain the existing shared-profile
    /// compatibility behavior; this does not claim a corrected native
    /// hardware profile.
    pub(crate) const fn powerpc_execution_capabilities(self) -> GuestExecutionCapabilities {
        GuestExecutionCapabilities {
            // The native adapter does not currently support the 'sysa'
            // selector, so keep that absence explicit.
            system_architecture: None,
            native_cpu_type: 0x0104,
            processor_type: 2,
            fpu_type: self.gestalt_fpu_type,
            mmu_type: self.gestalt_mmu_type,
        }
    }

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

    /// Bytes per scanline for this profile's indexed screen. `rowBytes` is an
    /// offset and may include storage beyond the visible pixels; matching the
    /// offscreen 16-byte quantum keeps direct full-row transfers coherent.
    /// Imaging With QuickDraw 1994, p. 4-5
    pub const fn screen_row_bytes(self) -> u32 {
        let bytes = (self.screen_width as u32 * self.screen_depth as u32).div_ceil(8);
        (bytes / 16 + 1) * 16
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
    // Mac OS 8.1 is the last release supported on 68040 Macs and provides
    // the late-classic Toolbox surface exposed by this HLE profile.
    system_version_bcd: 0x0810,
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

pub const REFERENCE_MACHINE_PROFILE: MachineProfile = BASILISK_II_PLAY_PROFILE;

pub(crate) const REFERENCE_M68K_EXECUTION_CAPABILITIES: GuestExecutionCapabilities =
    REFERENCE_MACHINE_PROFILE.m68k_execution_capabilities();
pub(crate) const REFERENCE_POWERPC_EXECUTION_CAPABILITIES: GuestExecutionCapabilities =
    REFERENCE_MACHINE_PROFILE.powerpc_execution_capabilities();

/// Returns the reference machine profile, optionally overridden by
/// `SYSTEMLESS_SCREEN_WIDTH` and `SYSTEMLESS_SCREEN_HEIGHT` environment variables.
pub fn reference_machine_profile() -> MachineProfile {
    let mut p = REFERENCE_MACHINE_PROFILE;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m68k_capabilities_derive_from_machine_profile_fields() {
        let profile = MachineProfile {
            gestalt_native_cpu_type: 0x11,
            gestalt_processor_type: 0x22,
            gestalt_fpu_type: 0x33,
            gestalt_mmu_type: 0x44,
            ..REFERENCE_MACHINE_PROFILE
        };

        assert_eq!(
            profile.m68k_execution_capabilities(),
            GuestExecutionCapabilities {
                system_architecture: Some(1),
                native_cpu_type: 0x11,
                processor_type: 0x22,
                fpu_type: 0x33,
                mmu_type: 0x44,
            }
        );
    }

    #[test]
    fn host_execution_policy_is_independent_of_guest_cpu_speed() {
        let guest_profile = MachineProfile {
            realtime_cpu_mhz: 1.0,
            ..REFERENCE_MACHINE_PROFILE
        };

        assert_eq!(DEFAULT_HOST_EXECUTION_POLICY.realtime_m68k_cpu_mhz, 25.0);
        assert_ne!(
            DEFAULT_HOST_EXECUTION_POLICY.realtime_m68k_cpu_mhz,
            guest_profile.realtime_cpu_mhz
        );
    }

    #[test]
    fn m68k_capability_record_preserves_shipped_values() {
        assert_eq!(
            REFERENCE_M68K_EXECUTION_CAPABILITIES,
            GuestExecutionCapabilities {
                system_architecture: Some(1),
                native_cpu_type: 4,
                processor_type: 5,
                fpu_type: 3,
                mmu_type: 4,
            }
        );
    }

    #[test]
    fn powerpc_capability_record_preserves_shipped_values_and_unsupported_sysa() {
        assert_eq!(
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES,
            GuestExecutionCapabilities {
                system_architecture: None,
                native_cpu_type: 0x0104,
                processor_type: 2,
                fpu_type: 3,
                mmu_type: 4,
            }
        );
    }
}
