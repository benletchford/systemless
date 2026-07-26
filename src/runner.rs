//! Fixture Runner - Loading and execution infrastructure

use crate::cpu::{M68kCpu, Register, StepResult};
use crate::debug_overlay::{DebugOverlayFrameStats, DebugOverlaySnapshot};
use crate::loader::{
    ApplicationSizeResource, Code0Header, CodeSegmentHeader, JumpTableEntry, LoadedApp,
    MpwFarSegmentHeader,
};
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::TrapDispatcher;
use crate::ui_theme::{ThemeMetricsMode, UiTheme, UiThemeId};
use crate::{Error, Result};
use m68k::BatchExit;
use std::collections::{BTreeSet, HashMap};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Cache env-var lookups (per-call syscall otherwise).
use std::sync::OnceLock;
static TRACE_DIALOG_FILTER: OnceLock<bool> = OnceLock::new();
static TRACE_TIMER: OnceLock<bool> = OnceLock::new();
static TRACE_VBL: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND_RUNNER: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_PROCS: OnceLock<bool> = OnceLock::new();

const APP_HEAP_FLOOR: u32 = 0x0020_0000;
const APP_ZONE_HEADER_SIZE: u32 = 64;
const APP_STACK_SAFETY_MARGIN: u32 = 0x2000;
const DEFAULT_LOAD_ADDRESS: u32 = 0x0001_0000;
const LARGE_SIZE_RELOCATION_MINIMUM: u32 = 2 * 1024 * 1024;
const APPLICATION_RESOURCE_REFNUM: u16 = 2;
const HFS_FCB_SIZE: u16 = 94;
const HFS_FCB_BUFFER_SIZE: u16 = 2 + HFS_FCB_SIZE;
const HFS_VCB_SIZE: u32 = 178;

fn trace_dialog_filter_enabled() -> bool {
    *TRACE_DIALOG_FILTER
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_FILTER").is_some())
}

fn trace_timer_enabled() -> bool {
    *TRACE_TIMER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TIMER").is_some())
}

fn trace_vbl_enabled() -> bool {
    *TRACE_VBL.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_VBL").is_some())
}

fn trace_sound_runner_enabled() -> bool {
    *TRACE_SOUND_RUNNER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn trace_dialog_procs_enabled() -> bool {
    *TRACE_DIALOG_PROCS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_PROCS").is_some())
}

// Gate the per-instruction trace_buffer behind an env var. The buffer
// is populated on EVERY instruction fetch and only used from
// `dump_trace()` on halt/crash. Default-disabled saves per-instruction
// `VecDeque` pop_front + push_back + an extra `bus.read_word` + 6
// register reads. Enable with `SYSTEMLESS_TRACE_BUFFER=1` when diagnosing
// a crash.
#[cfg(not(target_arch = "wasm32"))]
static TRACE_BUFFER_ENABLED: OnceLock<bool> = OnceLock::new();
fn trace_buffer_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_BUFFER_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_BUFFER").is_some())
}

#[cfg(not(target_arch = "wasm32"))]
static TRACE_PC_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static TRACE_PC_RANGE_TICKS: OnceLock<Option<(Option<u32>, Option<u32>)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range() -> Option<(u32, u32)> {
    *TRACE_PC_RANGE.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_TRACE_PC_RANGE").ok()?;
        let mut parts = value.split(':');
        let start = parts.next()?.trim();
        let end = parts.next()?.trim();
        let parse = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok();
        Some((parse(start)?, parse(end)?))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range_ticks() -> Option<(Option<u32>, Option<u32>)> {
    *TRACE_PC_RANGE_TICKS.get_or_init(|| {
        let min = std::env::var("SYSTEMLESS_TRACE_PC_RANGE_TICK_MIN")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());
        let max = std::env::var("SYSTEMLESS_TRACE_PC_RANGE_TICK_MAX")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());
        (min.is_some() || max.is_some()).then_some((min, max))
    })
}

fn trace_pc_range_contains(pc: u32, tick: u32) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (pc, tick);
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let in_range = trace_pc_range()
            .map(|(start, end)| pc >= start && pc <= end)
            .unwrap_or(false);
        if !in_range {
            return false;
        }
        trace_pc_range_ticks()
            .map(|(min, max)| {
                min.map(|min| tick >= min).unwrap_or(true)
                    && max.map(|max| tick <= max).unwrap_or(true)
            })
            .unwrap_or(true)
    }
}

// Gate the most-prominent startup/load chatter behind an env var.
// Library consumers shouldn't see arbitrary debug stderr output by
// default — these prints are useful when bring-up debugging a new game
// but pure noise once the loader works. Enable with
// `SYSTEMLESS_TRACE_LOAD=1` when diagnosing a load/halt.
static TRACE_LOAD_ENABLED: OnceLock<bool> = OnceLock::new();
pub(crate) fn trace_load_enabled() -> bool {
    *TRACE_LOAD_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_LOAD").is_some())
}

/// If `pc` matches a `GetTrapAddress` fake-pointer pattern, return a
/// human-readable hint identifying the trap that the game most
/// likely tried to JMP/JSR through. Else `None`.
///
/// Background: `GetTrapAddress` (and friends) on Systemless return a
/// unique-per-trap fake address so apps can compare against
/// `_Unimplemented` without hitting cache aliasing. Apps that ONLY
/// compare the address (the documented use) are fine. Apps that
/// actually `JMP (A0)` / `JSR (A0)` through the fake pointer land in
/// unmapped or garbage-filled memory and trip an IllegalInstruction
/// 30-100 instructions later. Surfacing the trap word at halt time
/// lets future investigators identify the missing trampoline at a
/// glance instead of having to disassemble around the halted PC.
///
/// Fake-pointer ranges (matching trap/memory.rs ranges):
///   OS-style:    `$00F00000 | (trap_word as u32)` — range
///                `$00F00000-$00F0FFFF`.
///   Tool-style:  `$CAFE0000 + (trap_word & 0x3FF)` — range
///                `$CAFE0000-$CAFE03FF`.
pub fn decode_fakeptr_pc(pc: u32) -> Option<String> {
    if (0x00F00000..=0x00F0FFFF).contains(&pc) {
        let trap_word = (pc & 0xFFFF) as u16;
        Some(format!(
            "PC matches GetTrapAddress fake-ptr ($A046/$A346/$A746) for trap ${:04X} — \
             game likely JMP/JSR'd through the unique-address placeholder. Implementing \
             a re-trap trampoline at the fake-ptr address would unblock this path.",
            trap_word
        ))
    } else if (0xCAFE0000..=0xCAFE03FF).contains(&pc) {
        let trap_num = (pc - 0xCAFE0000) as u16;
        let trap_word = 0xA800 | trap_num;
        Some(format!(
            "PC matches GetToolTrapAddress fake-ptr for trap ${:04X} (tool num=${:03X}) — \
             same trampoline gap as the OS-style fake-ptr range.",
            trap_word, trap_num
        ))
    } else {
        None
    }
}

// Per-opcode M68K histogram, opt-in via
// `SYSTEMLESS_TRACE_OPCODE_COUNTS=1`. Complements the trap histogram
// (which only sees A-line traps). Populated in `run_steps_internal`
// after each step succeeds. Use to prioritize decode-table / super-
// instruction-fusion work — the instruction mix is the input to that
// kind of optimization.
#[cfg(not(target_arch = "wasm32"))]
static TRACE_OPCODE_COUNTS: OnceLock<bool> = OnceLock::new();
fn trace_opcode_counts_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_OPCODE_COUNTS
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_OPCODE_COUNTS").is_some())
}

// Sampled PC histogram, opt-in via `SYSTEMLESS_TRACE_HOT_PC=1`. Every
// 1000th step's PC increments a `HashMap` entry. Use to locate the
// code address of a hot game loop. 1/1000 sampling keeps `HashMap`
// overhead negligible while still giving high-confidence attribution
// for any loop that takes more than ~0.1% of runtime.
const PC_SAMPLE_INTERVAL: u64 = 1000;

/// Upper bound on instructions handed to `m68k::run_batch` per call.
///
/// The run loop's per-iteration work (sound-callback polling, wait/delay
/// service, PC validity checks) now runs once per batch instead of once
/// per instruction, so this bounds the latency of those checks. 8192
/// instructions is tens of microseconds at batch execution speeds —
/// far below a guest tick (12k instructions) — while amortising the
/// loop overhead to well under 0.1%.
const BATCH_CHUNK: usize = 8192;

#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range_active() -> bool {
    trace_pc_range().is_some()
}

#[cfg(target_arch = "wasm32")]
fn trace_pc_range_active() -> bool {
    false
}

/// True when any opt-in tracer needs to observe every instruction
/// boundary. The run loop then falls back to single-instruction batches
/// so per-step diagnostics (trace buffer, watchpoints, histograms,
/// PC-range dumps) behave exactly as they did before batching. All the
/// gates are `OnceLock`-cached env-var reads, so this costs a handful of
/// branches per batch.
fn per_instruction_diagnostics_active() -> bool {
    #[cfg(debug_assertions)]
    {
        if crate::memory::bus::watchpoint_armed() {
            return true;
        }
    }
    trace_buffer_enabled()
        || trace_timer_enabled()
        || trace_opcode_counts_enabled()
        || trace_hot_pc_enabled()
        || trace_pc_range_active()
        || crate::memory::bus::fb_write_trace_active()
        || crate::memory::bus::mem_read_trace_active()
        || crate::memory::bus::mem_write_trace_active()
}
#[cfg(not(target_arch = "wasm32"))]
static TRACE_HOT_PC: OnceLock<bool> = OnceLock::new();
fn trace_hot_pc_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_HOT_PC.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_HOT_PC").is_some())
}

// Generic TickCount spin-wait fast-forward. Both headless and GUI callers use
// it by default. GUI execution supplies a per-frame tick cap, so a detected
// delay loop can advance only to the current VBL boundary; it cannot batch
// visible animation ticks ahead of the host. Two env vars override:
//   SYSTEMLESS_SPIN_WAIT_FASTFWD=1     force on (any mode)
//   SYSTEMLESS_DISABLE_SPIN_FASTFWD=1  force off (any mode)
static SPIN_WAIT_FASTFWD_FORCE_ON: OnceLock<bool> = OnceLock::new();
static SPIN_WAIT_FASTFWD_FORCE_OFF: OnceLock<bool> = OnceLock::new();

fn spin_wait_fastfwd_force_on() -> bool {
    *SPIN_WAIT_FASTFWD_FORCE_ON
        .get_or_init(|| std::env::var_os("SYSTEMLESS_SPIN_WAIT_FASTFWD").is_some())
}
fn spin_wait_fastfwd_force_off() -> bool {
    *SPIN_WAIT_FASTFWD_FORCE_OFF
        .get_or_init(|| std::env::var_os("SYSTEMLESS_DISABLE_SPIN_FASTFWD").is_some())
}

/// Resolve the fast-forward gate for the current run-loop call.
/// Override precedence:
///   1. force-off env wins.
///   2. force-on env wins next.
///   3. default = on for headless or tick-capped GUI execution; an uncapped
///      GUI caller stays off because it could otherwise batch visible ticks.
fn spin_wait_fastfwd_enabled_for(yield_for_ui: bool, tick_cap: Option<u32>) -> bool {
    spin_wait_fastfwd_gate(
        spin_wait_fastfwd_force_on(),
        spin_wait_fastfwd_force_off(),
        yield_for_ui,
        tick_cap.is_some(),
    )
}

/// Pure decision function for the override gate. Split out from
/// `spin_wait_fastfwd_enabled_for` so the env-var reads can be mocked
/// in unit tests (the `OnceLock`-based env caches initialise once per
/// process and would prevent testing all three modes in one test
/// run).
fn spin_wait_fastfwd_gate(
    force_on: bool,
    force_off: bool,
    yield_for_ui: bool,
    has_tick_cap: bool,
) -> bool {
    if force_off {
        return false;
    }
    if force_on {
        return true;
    }
    !yield_for_ui || has_tick_cap
}

/// Pure decision function for the ModalDialog noop-refire skip. ALL
/// of these must be true for the skip to fire:
///   - mode is headless (`yield_for_ui = false`)
///   - dialog tracking is active (`has_tracking = true`)
///   - no filter proc callback is due (`filter_allows_noop`)
///   - no button flash animating (`flash_remaining_zero`)
///   - initial draw procs all completed (`draw_procs_done`)
///   - dialog pixels already captured (`rendered_pixels_final`)
///   - event queue is empty (no input pending)
fn modaldialog_refire_is_noop(
    yield_for_ui: bool,
    has_tracking: bool,
    filter_allows_noop: bool,
    flash_remaining_zero: bool,
    draw_procs_done: bool,
    rendered_pixels_final: bool,
    event_queue_empty: bool,
) -> bool {
    !yield_for_ui
        && has_tracking
        && filter_allows_noop
        && flash_remaining_zero
        && draw_procs_done
        && rendered_pixels_final
        && event_queue_empty
}

/// Some tracking traps should block the application's foreground event
/// loop without advancing the app-visible tick clock in GUI mode. ModalDialog
/// is different: the dialog manager is itself the active event loop, and
/// Sound/VBL/Time Manager work must keep advancing while it tracks input.
fn tracking_refire_should_freeze_ticks(opcode: u16) -> bool {
    let trap_no_autopop = opcode & !0x0400;
    trap_no_autopop == 0xA93D // MenuSelect
        || trap_no_autopop == 0xA80B // PopUpMenuSelect / MenuKey tracking
        || trap_no_autopop == 0xA968 // TrackControl
}

fn canonical_trap_number(opcode: u16) -> (bool, u16) {
    let is_tool = (opcode & 0x0800) != 0;
    let trap_num = if is_tool {
        opcode & 0x03FF
    } else {
        opcode & 0x00FF
    };
    (is_tool, trap_num)
}

fn hle_trap_extra_tick_cost(opcode: u16) -> i32 {
    let (is_tool, trap_num) = canonical_trap_number(opcode);
    match (is_tool, trap_num) {
        // Event/time polling rates vary with host speed; charging these
        // creates false game-time drift instead of modelling ROM work.
        (true, 0x0170) // GetNextEvent
        | (true, 0x0060) // WaitNextEvent
        | (true, 0x0171) // EventAvail
        | (true, 0x0175) // TickCount
        | (true, 0x0062) // Button
        | (false, 0x0031) // GetOSEvent
        | (_, 0x003B) => 0, // Delay already advances requested ticks explicitly

        // SANE Pack4/Pack5 calls can be hot inner-loop arithmetic in games.
        // Treat them as guest computation, not manager work that should yield.
        (true, 0x006C) | (true, 0x006E) | (true, 0x01EB) | (true, 0x01EC) => 0,

        // QuickDraw blits and PICT draws do substantial HLE-side pixel work.
        (true, 0x00EC) | (true, 0x00F6) => 96,

        // Resource loads move and parse data that real ROM/file-system code
        // would not complete in one 68k instruction.
        (true, 0x01A0) | (true, 0x01A1) | (true, 0x01A2) | (true, 0x01BC) => 96,

        // Resource metadata/release calls are cheaper than loads but still
        // non-trivial manager work.
        (true, 0x019D..=0x01CF) => 24,

        // Other Toolbox/OS HLE traps should cost more than a single guest
        // opcode without making simple math/geometry helpers dominate timing.
        (true, _) => 4,
        (false, _) => 2,
    }
}

fn event_manager_yield_trap(opcode: u16) -> bool {
    matches!(
        canonical_trap_number(opcode),
        (true, 0x0170) // GetNextEvent
            | (true, 0x0060) // WaitNextEvent
            | (true, 0x0171) // EventAvail
    )
}

// Cap how many ticks the fast-forward will advance in one shot,
// to protect against pathological target values (e.g. overflowed
// unsigned register values being misinterpreted as huge-future
// ticks). If the cap trips, we fall back to normal spin — still
// correct, just not fast.
const SPIN_FASTFWD_MAX_TICKS: u32 = 1_000_000;

/// Outcome of `advance_until_tick`. Used to distinguish the "we
/// advanced, please synthesise the exit state" happy path from
/// the abort paths: tick_cap reached (caller must break the
/// outer run loop), pathological target difference (caller must
/// NOT synthesise — let the guest spin normally), and interrupt
/// callback injection (caller must leave the CPU at the callback
/// trampoline).
enum AdvanceResult {
    Advanced,
    CapHit,
    Interrupted,
    TooFar,
}

// Layout for dialog callback scratch region.
const DIALOG_DRAW_TRAMPOLINE_OFFSET: u32 = 0x00;
const DIALOG_FILTER_TRAMPOLINE_OFFSET: u32 = 0x40;
const DIALOG_FILTER_EVENT_OFFSET: u32 = 0x80;
// 2-byte scratch where the filter trampoline writes its Boolean return value.
const DIALOG_FILTER_RESULT_OFFSET: u32 = 0x96;
const MENU_HOOK_TRAMPOLINE_OFFSET: u32 = 0xA0;
const DIALOG_CALLBACK_SCRATCH_FALLBACK: u32 = 0x0000_1200;
/// Compact Mac video hardware refreshes at approximately 60.15 Hz.
pub const DEFAULT_VBL_HZ: f64 = 60.15;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileSummary {
    pub path: String,
    pub data_len: usize,
    pub resource_len: usize,
    pub data_hash: u64,
    pub resource_hash: u64,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileStat {
    pub path: String,
    pub data_len: usize,
    pub resource_len: usize,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsFileSnapshot {
    pub path: String,
    pub data_fork: Vec<u8>,
    pub resource_fork: Vec<u8>,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

fn vfs_fork_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
/// Default emulated CPU speed for realtime frontends (Mac IIci-class 68030).
pub const DEFAULT_REALTIME_CPU_MHZ: f64 =
    crate::machine_profile::REFERENCE_MACHINE_PROFILE.realtime_cpu_mhz;
/// Shared default realtime CPU budget used by the GUI runner and scripted
/// realtime mode so both frontends expose the same machine profile.
pub const DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND: f64 = DEFAULT_REALTIME_CPU_MHZ * 1_000_000.0;
// Default instructions per VBL tick for non-realtime execution (scripted harnesses, tests).
// Realtime frontends override this via set_instructions_per_tick() to match the
// shared default machine profile defined above.
// This lower value lets scripted harnesses run quickly without being wall-clock-paced.
const INSTRUCTIONS_PER_TICK: u32 = 12_000;
const DEFAULT_LAUNCH_TICKS: u32 = 600;
/// Default double-click interval: 20 VBL ticks, approximately one third of a
/// second. This is the conventional classic Mac OS setting exposed through
/// the low-memory `DoubleTime` global.
const DEFAULT_DOUBLE_TIME_TICKS: u32 = 20;
const MAC_EPOCH_OFFSET_FROM_UNIX: u64 = 2_082_844_800;
const CURSOR_TASK_NOOP_ADDR: u32 = 0x0000_0060;

fn current_mac_epoch_seconds() -> u32 {
    let unix_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    unix_now
        .saturating_add(MAC_EPOCH_OFFSET_FROM_UNIX)
        .min(u32::MAX as u64) as u32
}

#[derive(Clone, Copy, Debug)]
enum ActiveInterruptCallbackSource {
    Timer,
    Vbl,
    CursorTask,
    SoundCallback,
    SoundFileCompletion,
    SoundDoubleBack,
    DialogDrawProc,
    DialogFilterProc,
    MenuHook,
}

#[derive(Clone, Copy, Debug)]
struct ActiveInterruptCallback {
    source: ActiveInterruptCallbackSource,
    resume_pc: u32,
    resume_sp: u32,
    d_regs: [u32; 8],
    a_regs: [u32; 8],
    sr: u16,
    ccr: u8,
    restore_port: Option<(u32, u32)>,
}

fn is_sound_interrupt_source(source: ActiveInterruptCallbackSource) -> bool {
    matches!(
        source,
        ActiveInterruptCallbackSource::SoundCallback
            | ActiveInterruptCallbackSource::SoundFileCompletion
            | ActiveInterruptCallbackSource::SoundDoubleBack
    )
}

fn interrupt_callback_sr(source: ActiveInterruptCallbackSource, saved_sr: u16) -> u16 {
    match source {
        // A vertical retrace interrupt runs with processor priority level 1
        // and then restores the previous priority when it completes.
        // Inside Macintosh: Processes (1994), pp. 1-11 and 6-3.
        ActiveInterruptCallbackSource::Vbl | ActiveInterruptCallbackSource::CursorTask => {
            (saved_sr & !0x0700) | 0x2100
        }
        _ => saved_sr,
    }
}

fn align4(value: u32) -> u32 {
    value.saturating_add(3) & !3
}

fn app_heap_start_for_loaded_app(app: &LoadedApp) -> u32 {
    APP_HEAP_FLOOR.max(align4(app.loaded_image_end))
}

fn app_image_start_for_loaded_app(app: &LoadedApp) -> u32 {
    app.a5_base.saturating_sub(app.code0_header.below_a5)
}

fn app_visible_zone_start_for_loaded_app(app: &LoadedApp) -> u32 {
    let image_start = app_image_start_for_loaded_app(app);
    if image_start >= APP_HEAP_FLOOR.saturating_add(APP_ZONE_HEADER_SIZE) {
        APP_HEAP_FLOOR
    } else {
        app_heap_start_for_loaded_app(app)
    }
}

fn load_address_for_size_partition(
    configured_load_address: u32,
    header: &Code0Header,
    size_resource: Option<ApplicationSizeResource>,
    ram_size: u32,
) -> u32 {
    if configured_load_address != DEFAULT_LOAD_ADDRESS {
        return configured_load_address;
    }

    let Some(size) = size_resource else {
        return configured_load_address;
    };
    if size.preferred_partition_size().is_none()
        || size.minimum_size <= LARGE_SIZE_RELOCATION_MINIMUM
    {
        return configured_load_address;
    }

    let desired_a5 =
        APP_HEAP_FLOOR.saturating_add(size.minimum_size.saturating_sub(APP_STACK_SAFETY_MARGIN));
    let default_a5 = configured_load_address.saturating_add(header.below_a5);
    if desired_a5 <= default_a5 {
        return configured_load_address;
    }

    // Leave space above the relocated image for stack/screen buffers in
    // small synthetic runners. The standard frontends use 32 MB, so large
    // SIZE-partition apps still get the Mac-like high A5 placement.
    let max_reasonable_load = ram_size.saturating_sub(2 * 1024 * 1024);
    let relocated_load = align4(desired_a5.saturating_sub(header.below_a5));
    if relocated_load < max_reasonable_load {
        relocated_load
    } else {
        configured_load_address
    }
}

/// Configuration knobs for [`FixtureRunner`]. Use
/// [`FixtureRunnerConfig::default`] for the canonical defaults
/// (10M-instruction budget, 0x10000 load base, arrow-keys NOT
/// remapped to numpad) — only override fields you actually need.
pub struct FixtureRunnerConfig {
    /// Hard cap on instructions executed by the simpler unbounded
    /// [`FixtureRunner::run`] entry point. Not consulted by
    /// [`FixtureRunner::run_steps`], which uses its own per-call
    /// `max_steps` argument. Default: 10,000,000.
    pub max_instructions: usize,
    /// Base address where 68k CODE segments are loaded into guest
    /// RAM. Default: 0x10000 (64 KiB above the low-mem globals).
    /// Most games tolerate the default; a few with hardcoded
    /// expectations about A5 placement may need a higher value.
    pub load_address: u32,
    /// When true, arrow key virtual key codes are remapped to their numpad equivalents.
    /// Useful on keyboards without a numeric keypad, since many classic Mac games use
    /// the numpad for movement. Inside Macintosh Volume V, V-191.
    pub arrows_as_numpad: bool,
    /// Selected UI rendering provider. The default `classic-system7` provider
    /// represents the existing renderer; non-classic themes are explicit and
    /// must not alter guest-visible Toolbox behavior in classic metrics mode.
    pub ui_theme: UiThemeId,
    /// Declares whether theme rendering preserves classic guest metrics or opts
    /// into future themed hit/measurement behavior.
    pub theme_metrics_mode: ThemeMetricsMode,
}

impl Default for FixtureRunnerConfig {
    fn default() -> Self {
        Self {
            max_instructions: 10_000_000,
            load_address: DEFAULT_LOAD_ADDRESS,
            arrows_as_numpad: false,
            ui_theme: UiThemeId::ClassicSystem7,
            theme_metrics_mode: ThemeMetricsMode::ClassicGuestMetrics,
        }
    }
}

/// Canonical entry point of the systemless library.
///
/// `FixtureRunner` owns the three pieces of guest state — the [`M68kCpu`]
/// interpreter, the [`MacMemoryBus`], and the [`TrapDispatcher`] (Toolbox
/// + OS trap handlers) — and exposes the load / step / halt-inspect
/// surface that drives them.
///
/// **Lifecycle:**
/// 1. [`FixtureRunner::new`] — allocate guest RAM + dispatcher.
/// 2. [`crate::game::load_game`] — auto-detect StuffIt / MacBinary,
///    populate guest memory, seed CPU state.
/// 3. [`run_steps`](Self::run_steps) (preferred) or [`run`](Self::run)
///    — drive the CPU. `run_steps` returns `(steps_executed,
///    still_running)`; `run` runs until halt or
///    [`FixtureRunnerConfig::max_instructions`].
/// 4. After halt: [`halted_pc`](Self::halted_pc) /
///    [`halted_trap`](Self::halted_trap) /
///    [`halted_sp`](Self::halted_sp) / [`halted_d0`](Self::halted_d0)
///    expose per-halt detail, and
///    [`halted_by_exit_to_shell`](Self::halted_by_exit_to_shell) classifies
///    the common clean application-exit path.
///
/// **Defaults:** kiosk mode (Mac menu bar suppressed regardless of the
/// guest's `MBarHeight`); arrow keys NOT remapped to numpad. Override
/// each via [`set_menu_bar_visible`](Self::set_menu_bar_visible) /
/// [`set_arrows_as_numpad`](Self::set_arrows_as_numpad) or the
/// `SYSTEMLESS_SHOW_MENU_BAR` env var.
///
/// See `examples/run_headless.rs` for a runnable end-to-end example.
pub struct FixtureRunner {
    cpu: M68kCpu,
    bus: MacMemoryBus,
    dispatcher: TrapDispatcher,
    config: FixtureRunnerConfig,
    trace_buffer: std::collections::VecDeque<(u32, u16, u32, u32, u32, u32)>, // (PC, Op, A0, SP, A6, A5)
    /// Set to true when the application calls ExitToShell
    halted: bool,
    /// Trap opcode that caused the halt, if known.
    halted_trap: Option<u16>,
    /// Program counter at the point of halt.
    halted_pc: Option<u32>,
    /// Stack pointer at the point of halt.
    halted_sp: Option<u32>,
    /// D0 register at the point of halt.
    halted_d0: Option<u32>,
    /// Total guest instructions retired by the interpreter.
    total_instructions: u64,
    /// Number of interpreted guest instructions per `Ticks` increment.
    instructions_per_tick: u32,
    /// Optional cap on per-WaitNextEvent-call sleep tick advance in headless
    /// mode (when `run_steps` is called without a `tick_override`). `None`
    /// keeps the legacy drain-all behavior. `Some(n)` advances at most `n`
    /// ticks per WNE call, mirroring GUI mode's 1-tick cap. Used for
    /// scripted tick alignment with Basilisk.
    wait_sleep_cap_in_headless: Option<u32>,
    /// Remaining instruction budget for the current tick. Both 68k instructions
    /// and HLE trap costs are deducted. When this reaches zero or below, the
    /// tick advances and the budget is refilled from `instructions_per_tick`.
    tick_budget: i32,
    /// Tick value saved when menu tracking starts.  While set, run_steps caps
    /// its tick_override to this value so the game clock is frozen — matching
    /// the real Mac where MenuSelect blocks the application event loop.
    frozen_ticks: Option<u32>,
    /// Guest-memory address of the Time Manager interrupt trampoline code.
    /// Allocated once on first use and reused for all subsequent timer fires.
    timer_trampoline: u32,
    /// Guest-memory address of the Vertical Retrace Manager trampoline code.
    /// Allocated once on first use and reused for all VBL callbacks.
    vbl_trampoline: u32,
    /// Guest-memory address of the low-memory `JCrsrTask` callback trampoline.
    /// Allocated once on first use and reused for cursor task callbacks.
    cursor_task_trampoline: u32,
    /// Currently executing Time Manager callback, if any.
    ///
    /// Real timer delivery happens from interrupt context, so the same timer source
    /// must not be re-entered by our synthetic tick advancement while the callback
    /// is still unwinding back to interrupted guest code.
    active_interrupt_callback: Option<ActiveInterruptCallback>,
    /// Audio output backend (None = no audio output).
    audio: Option<Box<dyn crate::audio::AudioBackend>>,
    /// Accumulated audio samples for external consumers (e.g. WASM).
    /// Unsigned 8-bit mono PCM at OUTPUT_RATE Hz (silence = 0x80).
    audio_buffer: Vec<u8>,
    /// Guest-memory address of the SndPlayDoubleBuffer doubleback trampoline.
    /// Allocated once on first use and reused for all double-buffer callbacks.
    sound_doubleback_trampoline: u32,
    /// Guest-memory address of the SndNewChannel callback trampoline.
    /// Allocated once on first use and reused for all callback procedures.
    sound_callback_trampoline: u32,
    /// Guest-memory address of the SndStartFilePlay completion trampoline.
    /// Allocated once on first use and reused for all file completion routines.
    sound_file_completion_trampoline: u32,
    /// Guest-memory address of the dialog userItem draw proc trampoline (26 bytes).
    /// Allocated once on first use and reused for all subsequent draw proc calls.
    dialog_draw_trampoline: u32,
    /// Guest-memory address of the ModalDialog filter proc trampoline.
    /// Allocated once on first use and reused for all callback invocations.
    dialog_filter_trampoline: u32,
    /// Guest-memory address of the MenuSelect MenuHook trampoline.
    /// Allocated once on first use and reused for all callback invocations.
    menu_hook_trampoline: u32,
    /// Guest-memory address of a scratch EventRecord passed to ModalDialog filters.
    dialog_filter_event: u32,
    /// Last dialog/tick pair that received a synthetic ModalDialog null event.
    /// Real queued events bypass this; it only paces the no-input idle callback.
    dialog_filter_last_null_event_tick: Option<(u32, u32)>,
    /// Last dialog/update-window/tick triple that received a synthetic
    /// ModalDialog update event from the Window Manager invalid-region state.
    /// Queued mouse/key/update events bypass this; it only prevents the same
    /// still-invalid dialog from starving later input in one guest tick.
    dialog_filter_last_update_event_tick: Option<(u32, u32, u32)>,
    /// Override for the application's startup time in Mac-epoch seconds.
    /// Used by scripted frontends to keep guest-visible time deterministic.
    app_start_time: Option<u32>,
    /// Optional Finder-style application partition size override in bytes.
    /// Scripted frontends use this to model a user raising the preferred
    /// memory size before launch without mutating the application's resources.
    application_partition_size: Option<u32>,
    /// Per-opcode histogram for M68K instructions. Indexed by the
    /// full 16-bit opcode word (`cpu.core.ir` after step). Always
    /// allocated (512 KB); populated only when
    /// `SYSTEMLESS_TRACE_OPCODE_COUNTS=1` is set at startup. Zero cost
    /// on the hot path when disabled (cached bool compare, branch
    /// short-circuited). Complements the trap histogram, which only
    /// sees A-line opcodes; this captures MOVE/ADD/Bcc/etc. too,
    /// which is what decode-table or super-instruction-fusion work
    /// needs to prioritize.
    opcode_histogram: Box<[u64; 65536]>,
    /// Sampled PC histogram. When `SYSTEMLESS_TRACE_HOT_PC=1` is set,
    /// every 1000th step's PC increments a `HashMap` bucket. Answers
    /// "which CODE ADDRESS is hot", useful for locating game-side
    /// hot loops by routine. Sampling (1/1000) keeps `HashMap`
    /// overhead low; a million hot samples still fits in tens of
    /// unique addresses.
    pc_histogram: HashMap<u32, u64>,
}

impl FixtureRunner {
    /// Construct a fresh runner with `ram_size` bytes of guest RAM and
    /// the given [`FixtureRunnerConfig`]. The CPU starts halted at
    /// PC = 0; the framebuffer is whatever bytes the host allocator
    /// hands us. Call [`load_app`](Self::load_app) (or the higher-level
    /// `systemless::game::load_game`) to populate guest memory and seed the
    /// run state, then drive the guest with [`run_steps`](Self::run_steps).
    ///
    /// `ram_size` is typically 4 MiB to 16 MiB — most games never push
    /// past 8 MiB. The runner allocates a single contiguous host
    /// region of this size; bumping it costs only the upfront alloc.
    ///
    /// The dispatcher defaults to **kiosk mode** (Mac menu bar
    /// suppressed, regardless of the guest's `MBarHeight`). Call
    /// [`set_menu_bar_visible`](Self::set_menu_bar_visible) to opt back
    /// in to the original Mac menu-bar behaviour.
    pub fn new(ram_size: usize, config: FixtureRunnerConfig) -> Self {
        let mut dispatcher = TrapDispatcher::new();
        dispatcher.set_ui_theme_id(config.ui_theme);
        Self {
            cpu: M68kCpu::new(),
            bus: MacMemoryBus::new(ram_size),
            dispatcher,
            config,
            trace_buffer: std::collections::VecDeque::with_capacity(2000),
            halted: false,
            halted_trap: None,
            halted_pc: None,
            halted_sp: None,
            halted_d0: None,
            total_instructions: 0,
            instructions_per_tick: INSTRUCTIONS_PER_TICK,
            wait_sleep_cap_in_headless: None,
            tick_budget: INSTRUCTIONS_PER_TICK as i32,
            frozen_ticks: None,
            timer_trampoline: 0,
            vbl_trampoline: 0,
            cursor_task_trampoline: 0,
            active_interrupt_callback: None,
            audio: None,
            audio_buffer: Vec::new(),
            sound_doubleback_trampoline: 0,
            sound_callback_trampoline: 0,
            sound_file_completion_trampoline: 0,
            dialog_draw_trampoline: 0,
            dialog_filter_trampoline: 0,
            menu_hook_trampoline: 0,
            dialog_filter_event: 0,
            dialog_filter_last_null_event_tick: None,
            dialog_filter_last_update_event_tick: None,
            app_start_time: None,
            application_partition_size: None,
            opcode_histogram: Box::new([0u64; 65536]),
            pc_histogram: HashMap::new(),
        }
    }

    /// Returns true once guest execution has stopped.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Returns true when the halt was the documented clean application
    /// termination path, `_ExitToShell` (`$A9F4`).
    ///
    /// This lets runners and tests distinguish apps that intentionally quit
    /// from halts caused by faults, invalid PCs, or other fatal errors.
    pub fn halted_by_exit_to_shell(&self) -> bool {
        self.halted && self.halted_trap == Some(0xA9F4)
    }

    pub fn guest_tick(&self) -> u32 {
        self.bus.read_long(0x016A)
    }

    pub fn host_now(&self) -> Instant {
        Instant::now()
    }

    pub fn halted_trap(&self) -> Option<u16> {
        self.halted_trap
    }

    pub fn halted_pc(&self) -> Option<u32> {
        self.halted_pc
    }

    pub fn halted_sp(&self) -> Option<u32> {
        self.halted_sp
    }

    pub fn halted_stack_word0(&self) -> Option<u16> {
        self.halted_sp.map(|sp| self.bus.read_word(sp))
    }

    pub fn halted_stack_word(&self, word_index: u32) -> Option<u16> {
        self.halted_sp
            .map(|sp| self.bus.read_word(sp + word_index.saturating_mul(2)))
    }

    pub fn halted_d0(&self) -> Option<u32> {
        self.halted_d0
    }

    pub fn total_instructions(&self) -> u64 {
        self.total_instructions
    }

    pub fn debug_overlay_snapshot(
        &self,
        frame_stats: DebugOverlayFrameStats,
    ) -> DebugOverlaySnapshot {
        use crate::memory::globals::addr;

        let dispatcher = &self.dispatcher;
        let (_, _, screen_width, screen_height, pixel_size) = dispatcher.screen_mode;
        let cursor_image = dispatcher.cursor_data();
        let cursor_mask_nonzero_bytes = cursor_image
            .as_ref()
            .map(|(_, mask, _, _)| mask.iter().filter(|&&byte| byte != 0).count());
        let cursor_hotspot = cursor_image
            .as_ref()
            .map(|(_, _, hot_v, hot_h)| (*hot_v, *hot_h));

        DebugOverlaySnapshot {
            frame_stats,
            guest_tick: self.guest_tick(),
            total_instructions: self.total_instructions,
            trap_count: dispatcher.trap_count,
            game_trap_count: dispatcher.game_trap_count,
            cursor_visible: dispatcher.cursor_visible(),
            cursor_level: dispatcher.cursor_level(),
            cursor_data_present: dispatcher.cursor_data_present(),
            cursor_mask_nonzero_bytes,
            cursor_hotspot,
            cursor_position: dispatcher.mouse_position(),
            mouse_button: dispatcher.mouse_button,
            fullscreen_locked: dispatcher.fullscreen_locked,
            mbar_height: self.bus.read_word(addr::MBAR_HEIGHT),
            screen_width,
            screen_height,
            pixel_size,
            front_window: dispatcher.front_window(),
            window_bounds: dispatcher.window_bounds(),
            window_count: dispatcher.window_count(),
            menu_count: dispatcher.menu_count(),
            halted: self.halted,
            halted_trap: self.halted_trap,
            halted_pc: self.halted_pc,
        }
    }

    pub fn cpu(&self) -> &M68kCpu {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M68kCpu {
        &mut self.cpu
    }

    pub fn bus(&self) -> &MacMemoryBus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut MacMemoryBus {
        &mut self.bus
    }

    pub fn dispatcher(&self) -> &crate::trap::dispatch::TrapDispatcher {
        &self.dispatcher
    }

    pub fn dispatcher_mut(&mut self) -> &mut crate::trap::dispatch::TrapDispatcher {
        &mut self.dispatcher
    }

    /// Returns the selected UI theme provider. `classic-system7` is the
    /// default and maps to the existing renderer path; non-classic providers
    /// are explicit Systemless-owned rendering contracts.
    pub fn ui_theme(&self) -> &'static dyn UiTheme {
        self.config.ui_theme.provider()
    }

    pub fn ui_theme_id(&self) -> UiThemeId {
        self.config.ui_theme
    }

    pub fn theme_metrics_mode(&self) -> ThemeMetricsMode {
        self.config.theme_metrics_mode
    }

    pub fn uses_classic_guest_metrics(&self) -> bool {
        self.config
            .theme_metrics_mode
            .preserves_classic_guest_metrics()
    }

    /// Show or hide the Mac menu bar.
    ///
    /// systemless runs in **kiosk mode** by default — the Mac menu bar is
    /// suppressed regardless of the guest's `MBarHeight` ($0BAA) value
    /// and `DrawMenuBar` is a no-op. This matches the typical embedding
    /// case (running a single classic Mac game inside a fullscreen
    /// host window) where the host owns the chrome and the guest's
    /// menu bar would just diverge from the original-machine
    /// appearance whenever the cursor entered `y < 20`.
    ///
    /// Pass `true` to opt back in to original Mac behavior — for
    /// example, when running a Mac *application* that relies on the
    /// menu bar as its primary user surface.
    ///
    /// The same toggle is also accessible via the `SYSTEMLESS_SHOW_MENU_BAR`
    /// environment variable (set to any value to show) and via
    /// `systemless --show-menu-bar`. This library method is the
    /// preferred entry point for library embedders that don't want
    /// to depend on environment-variable plumbing.
    ///
    /// Inside Macintosh Volume I, I-354 (DrawMenuBar);
    /// Inside Macintosh Volume V, V-245 (MBarHeight global).
    pub fn set_menu_bar_visible(&mut self, visible: bool) {
        self.dispatcher.menu_bar_hidden = !visible;
    }

    /// Returns true when the Mac menu bar is currently being rendered.
    /// In the default kiosk configuration this returns `false`.
    pub fn menu_bar_visible(&self) -> bool {
        !self.dispatcher.menu_bar_hidden
    }

    /// Disassemble M68K instructions starting at `pc` for `count`
    /// instruction words. Returns one entry per word: `(pc, mnemonic,
    /// size_in_bytes)`. The size includes any operand words consumed
    /// by the instruction; advance `pc` by `size` to reach the next.
    ///
    /// Unknown opcodes (including A-line traps and other reserved
    /// patterns) come back as `DC.W $XXXX` with size 2 — the same
    /// convention the underlying [`m68k::dasm::disassemble`] uses.
    /// Reads past the end of guest RAM yield `(addr, "<unmapped>", 2)`
    /// rather than panicking.
    ///
    /// Diagnostic helper for pixel-divergence and trap-misroute
    /// investigations: pair with the framebuffer-write tracer
    /// (`SYSTEMLESS_TRACE_FB_WRITE_RANGE`) to see what the guest is
    /// actually executing at a suspect PC.
    pub fn disassemble_at(&self, pc: u32, count: usize) -> Vec<(u32, String, u32)> {
        use crate::memory::MemoryBus;
        let mut out = Vec::with_capacity(count);
        let mut cur = pc;
        for _ in 0..count {
            // bus.read_word returns 0 for OOB rather than panicking,
            // so wrap-around safety is a property of the underlying
            // bus impl. Tag explicitly when the read landed at an
            // address we know is past the framebuffer.
            let opcode = self.bus.read_word(cur);
            let unmapped = (cur as u64) >= (8 * 1024 * 1024);
            let (mnemonic, size) = if unmapped {
                ("<unmapped>".to_string(), 2)
            } else {
                m68k::dasm::disassemble(cur, opcode, m68k::CpuType::M68000)
            };
            // m68k's disassemble returns the instruction's TOTAL size
            // including operand words; cap at a reasonable max so a
            // malformed opcode doesn't run away.
            let size = size.clamp(2, 10);
            out.push((cur, mnemonic, size));
            cur = cur.wrapping_add(size);
        }
        out
    }

    /// Dump the top-N M68K opcodes by execution count. No-op when
    /// `SYSTEMLESS_TRACE_OPCODE_COUNTS` wasn't set at startup. Format:
    ///   [OPCODE-HIST]   43210123  $3F3C  MOVE.W #imm,-(SP)
    /// Unknown opcodes fall back to showing just the hex word.
    pub fn print_opcode_histogram(&self, top_n: usize) {
        if !trace_opcode_counts_enabled() {
            return;
        }
        let mut entries: Vec<(u16, u64)> = self
            .opcode_histogram
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| if c > 0 { Some((i as u16, c)) } else { None })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total: u64 = entries.iter().map(|(_, c)| c).sum();
        eprintln!(
            "[OPCODE-HIST] top {} of {} distinct opcodes ({} total non-Aline instructions)",
            top_n.min(entries.len()),
            entries.len(),
            total
        );
        for (opcode, count) in entries.iter().take(top_n) {
            let group = (opcode >> 12) & 0xF;
            let group_name = match group {
                0x0 => "bit-op/MOVEP/immediate",
                0x1 => "MOVE.B",
                0x2 => "MOVE.L",
                0x3 => "MOVE.W",
                0x4 => "misc (LEA/JSR/etc.)",
                0x5 => "ADDQ/SUBQ/Scc/DBcc",
                0x6 => "Bcc/BSR",
                0x7 => "MOVEQ",
                0x8 => "OR/DIV/SBCD",
                0x9 => "SUB/SUBX",
                0xA => "A-line (should be in trap-hist)",
                0xB => "CMP/EOR",
                0xC => "AND/MUL/ABCD/EXG",
                0xD => "ADD/ADDX",
                0xE => "shift/rotate",
                0xF => "F-line (FPU/coproc)",
                _ => "?",
            };
            eprintln!(
                "[OPCODE-HIST]   {:>10}  ${:04X}  group {:X}: {}",
                count, opcode, group, group_name
            );
        }
    }

    /// Dump the top-N hottest PCs by sampled hit count. No-op when
    /// `SYSTEMLESS_TRACE_HOT_PC` is unset. Each count represents one
    /// `PC_SAMPLE_INTERVAL` (=1000) M68K instructions; multiply by
    /// 1000 for an approximate instruction count attributed to that
    /// PC.
    pub fn print_pc_histogram(&self, top_n: usize) {
        if !trace_hot_pc_enabled() {
            return;
        }
        let mut entries: Vec<(u32, u64)> =
            self.pc_histogram.iter().map(|(&a, &c)| (a, c)).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total: u64 = entries.iter().map(|(_, c)| c).sum();
        eprintln!(
            "[PC-HIST] top {} of {} distinct PCs ({} samples × {} = ~{} instructions)",
            top_n.min(entries.len()),
            entries.len(),
            total,
            PC_SAMPLE_INTERVAL,
            total * PC_SAMPLE_INTERVAL
        );
        for (pc, count) in entries.iter().take(top_n) {
            // Classify by address region: the common Mac-app
            // convention for loaded segments is roughly $00010000-
            // $00600000 (game code) and $01000000+ (ROM).
            let region = match *pc {
                0x0000_0000..=0x0000_FFFF => "low-mem",
                0x0001_0000..=0x005F_FFFF => "app code",
                0x0060_0000..=0x00FF_FFFF => "heap/data",
                0x0100_0000..=0x01FF_FFFF => "ROM",
                _ => "other",
            };
            eprintln!("[PC-HIST]   {:>8}  PC=${:08X}  ({})", count, pc, region);
        }
    }

    pub fn install_application_clut(&mut self, clut: [[u16; 3]; 256]) {
        self.dispatcher
            .install_application_clut(&mut self.bus, clut);
    }

    pub fn set_app_start_time(&mut self, secs: u32) {
        self.app_start_time = Some(secs);
    }

    pub fn set_application_partition_size(&mut self, bytes: Option<u32>) {
        self.application_partition_size = bytes.filter(|&bytes| bytes >= 128 * 1024);
    }

    /// Getter for the pinned Mac epoch seconds. Returns `None` when no
    /// pin has been applied — without a pin, `init_app` falls back to
    /// `current_mac_epoch_seconds()` (host wall-clock), which leaks
    /// into the guest's `Time` global (`$020C`) and breaks
    /// reproducibility.
    pub fn app_start_time(&self) -> Option<u32> {
        self.app_start_time
    }

    pub fn application_partition_size(&self) -> Option<u32> {
        self.application_partition_size
    }

    /// Install a trace sink to receive runtime events and screen
    /// snapshots (see [`crate::trace::TraceSink`]). The host owns the sink
    /// and decides how/where its output is persisted.
    pub fn set_trace_sink(&mut self, sink: Box<dyn crate::trace::TraceSink>) {
        self.dispatcher.set_trace_sink(sink)
    }

    /// Composite chrome/dialog overlays onto the framebuffer.
    /// Call before reading raw pixels for screenshots.
    pub fn composite_frame(&mut self) {
        self.dispatcher.redraw_chrome(&mut self.bus);
    }

    /// Enable or disable arrow-key-to-numpad remapping.
    pub fn set_arrows_as_numpad(&mut self, enabled: bool) {
        self.config.arrows_as_numpad = enabled;
    }

    /// Returns true when arrow keys are remapped to numpad key codes.
    pub fn arrows_as_numpad(&self) -> bool {
        self.config.arrows_as_numpad
    }

    /// Move the mouse without changing the button state. Updates the
    /// dispatcher's tracked position and the six mouse-position
    /// low-memory globals (MTemp / RawMouse / Mouse) so guest code that
    /// reads them directly sees the new coordinates immediately. Leaves
    /// MBState ($0172) untouched. Inside Macintosh Volume II, II-371.
    pub fn set_mouse_position(&mut self, v: i16, h: i16) {
        self.dispatcher.set_mouse_position(v, h);
        self.sync_mouse_position_lowmem();
        if !self.wake_pending_wait_next_event_if_input_available() {
            self.wake_pending_wait_next_event_with_null_event_for_polling_input();
        }
        self.wake_foreground_after_input();
    }

    /// Inject a mouse-down event and sync low-memory globals.
    ///
    /// On real hardware the VBL interrupt handler updates MBState ($0172)
    /// and the mouse-position globals whenever the button state changes.
    /// Since our HLE has no interrupt-driven mouse driver, we sync these
    /// globals here so that code polling the low-memory locations directly
    /// (instead of calling Button or GetNextEvent) sees the correct state.
    pub fn push_mouse_down(&mut self, v: i16, h: i16) {
        self.dispatcher.push_mouse_down(v, h);
        self.sync_mouse_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Inject a mouse-up event.
    ///
    /// Sync MBState ($0172) immediately so code that polls the low-memory
    /// byte directly (rather than calling Button or GetNextEvent) sees the
    /// release without waiting for the next tick advance.
    ///
    /// On real hardware the ADB manager polls the mouse at ~200 Hz, updating
    /// MBState within a few milliseconds of the physical release. Deferring
    /// the update to the next advance_guest_tick left MBState stale for an
    /// entire tick (~16 ms), which is longer than real hardware and caused
    /// frame-rate-dependent games to read the wrong button state for too
    /// many loop iterations after a click-up.
    /// Inside Macintosh Volume II, II-371
    pub fn push_mouse_up(&mut self, v: i16, h: i16) {
        self.dispatcher.push_mouse_up(v, h);
        self.sync_mouse_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Write the three mouse-position low-memory globals (MTemp $0828,
    /// RawMouse $082C, Mouse $0830) from `self.dispatcher.mouse_pos`.
    /// Inside Macintosh Volume I, I-258.
    fn sync_mouse_position_lowmem(&mut self) {
        let (v, h) = self.dispatcher.mouse_pos;
        self.bus.write_word(0x0828, v as u16);
        self.bus.write_word(0x082A, h as u16);
        self.bus.write_word(0x082C, v as u16);
        self.bus.write_word(0x082E, h as u16);
        self.bus.write_word(0x0830, v as u16);
        self.bus.write_word(0x0832, h as u16);
    }

    /// Sync mouse button + position low-memory globals from internal state.
    ///
    /// MBState ($0172): 0x00 = button down, 0x80 = button up
    /// MTemp ($0828), RawMouse ($082C), Mouse ($0830): current position
    /// Inside Macintosh Volume I, I-258; Inside Macintosh Volume II, II-371
    fn sync_mouse_lowmem(&mut self) {
        let mb_state: u8 = if self.dispatcher.mouse_button {
            0x00
        } else {
            0x80
        };
        self.bus.write_byte(0x0172, mb_state);
        self.sync_mouse_position_lowmem();
    }

    /// Sync the 16-byte KeyMapLM low-memory bitmap from the dispatcher's
    /// current key state. Inside Macintosh Volume I, I-260 documents the
    /// KeyMap returned by GetKeys; MPW SysEqu.h exposes the ROM-maintained
    /// low-memory mirror at $0174 for code that polls it directly.
    fn sync_key_map_lowmem(&mut self) {
        use crate::memory::globals::addr;

        self.bus
            .write_bytes(addr::KEY_MAP_LM, self.dispatcher.key_map_bytes());
    }

    /// Inject a key-down event, applying arrow→numpad remapping if configured.
    pub fn push_key_down(&mut self, mac_key: u8, char_code: u8) {
        let (key, char_code) = self.remap_key(mac_key, char_code);
        self.dispatcher.push_key_down(key, char_code);
        self.sync_key_map_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Inject a key-up event, applying arrow→numpad remapping if configured.
    pub fn push_key_up(&mut self, mac_key: u8, char_code: u8) {
        let (key, char_code) = self.remap_key(mac_key, char_code);
        self.dispatcher.push_key_up(key, char_code);
        self.sync_key_map_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Remap arrow key virtual key codes to numpad equivalents when enabled.
    /// Arrow keys: Left=0x7B, Right=0x7C, Down=0x7D, Up=0x7E
    /// Numpad dirs: 4(left)=0x56, 6(right)=0x58, 5(down)=0x57, 8(up)=0x5B
    /// Inside Macintosh Volume V, V-191
    fn remap_key(&self, mac_key: u8, char_code: u8) -> (u8, u8) {
        if self.config.arrows_as_numpad {
            match mac_key {
                0x7B => (0x56, b'4'), // Left  -> Numpad4
                0x7C => (0x58, b'6'), // Right -> Numpad6
                0x7D => (0x57, b'5'), // Down  -> Numpad5
                0x7E => (0x5B, b'8'), // Up    -> Numpad8
                _ => (mac_key, char_code),
            }
        } else {
            (mac_key, char_code)
        }
    }

    /// Set the audio output backend. If not set, no audio is produced.
    pub fn set_audio(&mut self, audio: Box<dyn crate::audio::AudioBackend>) {
        self.audio = Some(audio);
    }

    pub fn set_instructions_per_tick(&mut self, instructions_per_tick: u32) {
        let old = self.instructions_per_tick.max(1);
        let new = instructions_per_tick.max(1);
        // Scale the remaining budget proportionally so a mid-run change
        // doesn't cause an immediate tick advance or an artificially long tick.
        self.tick_budget = ((self.tick_budget as i64 * new as i64) / old as i64) as i32;
        self.instructions_per_tick = new;
    }

    pub fn instructions_per_tick(&self) -> u32 {
        self.instructions_per_tick
    }

    /// Cap the per-WaitNextEvent-call sleep tick advance in headless mode.
    /// None (default) preserves the legacy drain-all behavior. Some(n) caps
    /// each WNE sleep to at most n tick advances, mirroring GUI mode.
    pub fn set_wait_sleep_cap_in_headless(&mut self, cap: Option<u32>) {
        self.wait_sleep_cap_in_headless = cap;
    }

    pub fn wait_sleep_cap_in_headless(&self) -> Option<u32> {
        self.wait_sleep_cap_in_headless
    }

    /// Drain accumulated audio samples for external consumers (e.g. WASM).
    /// Returns unsigned 8-bit mono PCM at 22050 Hz (silence = 0x80).
    pub fn drain_audio(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Drain accumulated audio samples into a caller-owned buffer.
    ///
    /// This avoids transferring the runner's `Vec` allocation out on every
    /// browser frame, so the next audio mix can reuse its existing capacity.
    pub fn drain_audio_into(&mut self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(&self.audio_buffer);
        self.audio_buffer.clear();
    }

    /// Current number of buffered audio samples (for diagnostics).
    pub fn audio_buffer_len(&self) -> usize {
        self.audio_buffer.len()
    }

    pub fn has_pending_sound_work(&self) -> bool {
        self.active_interrupt_callback
            .map(|callback| {
                matches!(
                    callback.source,
                    ActiveInterruptCallbackSource::SoundCallback
                        | ActiveInterruptCallbackSource::SoundFileCompletion
                        | ActiveInterruptCallbackSource::SoundDoubleBack
                )
            })
            .unwrap_or(false)
            || !self
                .dispatcher
                .sound_manager
                .pending_sound_callbacks
                .is_empty()
            || !self.dispatcher.sound_manager.pending_callbacks.is_empty()
    }

    pub fn is_ui_tracking_active(&self) -> bool {
        self.frozen_ticks.is_some()
            || self.dispatcher.is_menu_tracking()
            || self.dispatcher.is_dialog_tracking()
            || self.dispatcher.is_control_tracking()
    }

    /// Advance the guest tick counter by one, firing VBL and timer tasks.
    /// Used by the GUI runner to force-advance ticks when the CPU can't
    /// keep up with wall-clock time (e.g. during expensive PICT draws).
    pub fn force_advance_guest_tick(&mut self) {
        self.advance_guest_tick();
    }

    pub fn set_output_path(&mut self, path: std::path::PathBuf) {
        // The path points to a specific file (e.g. temp/foo/fixture_dump.bin).
        // Use its parent directory as the VFS output directory.
        if let Some(dir) = path.parent() {
            self.dispatcher.output_dir = Some(dir.to_path_buf());
        }
    }

    /// Get the contents of a file from the virtual filesystem.
    pub fn vfs_read(&self, filename: &str) -> Option<&[u8]> {
        self.dispatcher.vfs.get(filename).map(|v| v.as_slice())
    }

    pub fn vfs_file_summaries(&mut self) -> Vec<VfsFileSummary> {
        self.vfs_file_summaries_where(|_| true)
    }

    pub fn vfs_file_summaries_where<F>(&mut self, mut include: F) -> Vec<VfsFileSummary>
    where
        F: FnMut(&str) -> bool,
    {
        self.vfs_file_paths()
            .into_iter()
            .filter(|path| include(path))
            .filter_map(|path| self.vfs_file_summary_for_path(&path))
            .collect()
    }

    pub fn vfs_file_stats_where<F>(&mut self, mut include: F) -> Vec<VfsFileStat>
    where
        F: FnMut(&str) -> bool,
    {
        self.vfs_file_paths()
            .into_iter()
            .filter(|path| include(path))
            .filter_map(|path| self.vfs_file_stat_for_path(&path))
            .collect()
    }

    pub fn vfs_file_summary(&mut self, path: &str) -> Option<VfsFileSummary> {
        let normalized = TrapDispatcher::normalize_vfs_path(path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return None;
        }
        self.vfs_file_summary_for_path(&normalized)
    }

    pub fn vfs_file_snapshot(&mut self, path: &str) -> Option<VfsFileSnapshot> {
        let normalized = TrapDispatcher::normalize_vfs_path(path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return None;
        }
        if !self.dispatcher.vfs.contains_key(&normalized)
            && !self.dispatcher.vfs_rsrc.contains_key(&normalized)
        {
            return None;
        }
        let metadata = self.dispatcher.vfs_file_metadata(&normalized)?;
        Some(VfsFileSnapshot {
            path: normalized.clone(),
            data_fork: self
                .dispatcher
                .vfs
                .get(&normalized)
                .cloned()
                .unwrap_or_default(),
            resource_fork: self
                .dispatcher
                .vfs_rsrc
                .get(&normalized)
                .cloned()
                .unwrap_or_default(),
            file_type: metadata.file_type,
            creator: metadata.creator,
            finder_flags: metadata.finder_flags,
            created_date: metadata.created_date,
            modified_date: metadata.modified_date,
        })
    }

    pub fn import_vfs_file(&mut self, file: &VfsFileSnapshot) {
        let normalized = TrapDispatcher::normalize_vfs_path(&file.path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return;
        }

        self.dispatcher
            .vfs
            .insert(normalized.clone(), file.data_fork.clone());
        self.dispatcher
            .vfs_rsrc
            .insert(normalized.clone(), file.resource_fork.clone());
        self.dispatcher.ensure_vfs_file_metadata(&normalized);
        if let Some(metadata) = self.dispatcher.vfs_metadata.get_mut(&normalized) {
            metadata.file_type = file.file_type;
            metadata.creator = file.creator;
            metadata.finder_flags = file.finder_flags;
            if file.created_date != 0 {
                metadata.created_date = file.created_date;
            }
            if file.modified_date != 0 {
                metadata.modified_date = file.modified_date;
            }
        }
    }

    pub fn import_vfs_file_relative_to_launched_app(
        &mut self,
        relative_dir: &str,
        file: &VfsFileSnapshot,
    ) -> std::result::Result<(), String> {
        let app_path = self
            .dispatcher
            .launched_app_path
            .clone()
            .ok_or_else(|| "launched app path is not available".to_string())?;
        let app_parent = TrapDispatcher::vfs_parent_path(&app_path);
        if app_parent.is_empty() {
            return Err("launched app has no parent folder".to_string());
        }

        let relative_dir = TrapDispatcher::normalize_vfs_path(relative_dir);
        if relative_dir.is_empty() || relative_dir.starts_with('/') || relative_dir.contains("..") {
            return Err(format!("invalid relative VFS directory {relative_dir:?}"));
        }
        let file_path = TrapDispatcher::normalize_vfs_path(&file.path);
        let filename = TrapDispatcher::vfs_basename(&file_path);
        if filename.is_empty() {
            return Err("plugin file has no filename".to_string());
        }

        let mut mounted = file.clone();
        mounted.path = format!("{app_parent}/{relative_dir}/{filename}");
        self.import_vfs_file(&mounted);
        Ok(())
    }

    pub fn remove_vfs_file(&mut self, path: &str) -> bool {
        self.dispatcher.remove_vfs_path(path)
    }

    fn vfs_file_paths(&mut self) -> Vec<String> {
        self.dispatcher.ensure_vfs_catalog();
        let mut paths = BTreeSet::new();
        for path in self.dispatcher.vfs.keys() {
            if !path.starts_with("__rsrc__") {
                paths.insert(TrapDispatcher::normalize_vfs_path(path));
            }
        }
        for path in self.dispatcher.vfs_rsrc.keys() {
            if !path.starts_with("__rsrc__") {
                paths.insert(TrapDispatcher::normalize_vfs_path(path));
            }
        }
        paths.into_iter().filter(|path| !path.is_empty()).collect()
    }

    fn vfs_file_summary_for_path(&mut self, path: &str) -> Option<VfsFileSummary> {
        let stat = self.vfs_file_stat_for_path(path)?;
        let data_fork = self
            .dispatcher
            .vfs
            .get(path)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let resource_fork = self
            .dispatcher
            .vfs_rsrc
            .get(path)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        Some(VfsFileSummary {
            path: stat.path,
            data_len: stat.data_len,
            resource_len: stat.resource_len,
            data_hash: vfs_fork_hash(data_fork),
            resource_hash: vfs_fork_hash(resource_fork),
            file_type: stat.file_type,
            creator: stat.creator,
            finder_flags: stat.finder_flags,
            created_date: stat.created_date,
            modified_date: stat.modified_date,
        })
    }

    fn vfs_file_stat_for_path(&mut self, path: &str) -> Option<VfsFileStat> {
        let metadata = self.dispatcher.vfs_file_metadata(path)?;
        let data_len = self.dispatcher.vfs.get(path).map(Vec::len).unwrap_or(0);
        let resource_len = self
            .dispatcher
            .vfs_rsrc
            .get(path)
            .map(Vec::len)
            .unwrap_or(0);
        Some(VfsFileStat {
            path: path.to_string(),
            data_len,
            resource_len,
            file_type: metadata.file_type,
            creator: metadata.creator,
            finder_flags: metadata.finder_flags,
            created_date: metadata.created_date,
            modified_date: metadata.modified_date,
        })
    }

    /// Execute exactly one 68k instruction. Returns the per-step result
    /// (continue / halted / unimplemented opcode). Most embedders
    /// should call [`run_steps`](Self::run_steps) instead — it amortises
    /// the per-step bookkeeping (tick advancement, halt detection,
    /// trace ring filling) across the whole budget.
    pub fn step(&mut self) -> StepResult {
        self.cpu.step(&mut self.bus)
    }

    /// Load a parsed Mac resource fork into guest memory: registers
    /// every resource with the Resource Manager, links the application
    /// CODE segments through the trap dispatcher's segment table, and
    /// returns a [`LoadedApp`] describing the entry-point base address
    /// and per-segment offsets.
    ///
    /// Lower-level than [`systemless::game::load_game`](crate::game::load_game) —
    /// that helper auto-detects StuffIt / MacBinary / raw-resource-fork
    /// containers and calls this method internally. Use `load_app`
    /// directly only when you've already parsed the resource fork
    /// yourself (e.g. building a custom test fixture).
    pub fn load_app(&mut self, fork: &ResourceFork) -> Option<LoadedApp> {
        let app = load_app_generic(fork, &mut self.bus, self.config.load_address)?;
        let heap_start = app_heap_start_for_loaded_app(&app);

        // Resource data is allocated after direct CODE/global loading. Reserve
        // the app-zone header at the actual heap start so early app resources
        // cannot land in loader-owned memory or under `bkLim`/`hFstFree`.
        // Inside Macintosh Volume II, II-22.
        self.bus
            .reserve_heap_until(heap_start.saturating_add(APP_ZONE_HEADER_SIZE));
        self.dispatcher.load_resources(fork, &mut self.bus);

        let segments: HashMap<i16, u32> = app.segment_bases.iter().map(|(&k, &v)| (k, v)).collect();
        self.dispatcher.register_segments(segments);

        Some(app)
    }

    fn clear_startup_framebuffer(&mut self) {
        if self.menu_bar_visible() {
            let (scrn_base, row_bytes, screen_width, screen_height, pixel_size) =
                self.dispatcher.screen_mode;
            TrapDispatcher::fb_fill_pattern_rect(
                &mut self.bus,
                scrn_base,
                row_bytes,
                pixel_size,
                screen_width as i16,
                screen_height as i16,
                0,
                0,
                screen_height as i16,
                screen_width as i16,
                [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            );
            return;
        }

        let (scrn_base, row_bytes, _, scrn_height, _) = self.dispatcher.screen_mode;
        self.bus
            .fill_bytes(scrn_base, row_bytes * scrn_height as u32, 0xFF);
    }

    fn switch_to_launched_application(
        &mut self,
        app_path: &str,
    ) -> std::result::Result<(), String> {
        use crate::memory::globals::addr;

        let normalized = TrapDispatcher::normalize_vfs_path(app_path);
        let rsrc_key = self
            .dispatcher
            .find_vfs_rsrc_file(&normalized)
            .ok_or_else(|| format!("no resource fork for launched application {normalized:?}"))?;
        let rsrc_bytes = self
            .dispatcher
            .vfs_rsrc
            .get(&rsrc_key)
            .cloned()
            .ok_or_else(|| format!("resource fork {rsrc_key:?} disappeared before launch"))?;
        let fork = ResourceFork::parse(&rsrc_bytes)
            .ok_or_else(|| format!("failed to parse launched application {normalized:?}"))?;

        let ram_size = self.bus.ram_size() as usize;
        let config = FixtureRunnerConfig {
            max_instructions: self.config.max_instructions,
            load_address: self.config.load_address,
            arrows_as_numpad: self.config.arrows_as_numpad,
            ui_theme: self.config.ui_theme,
            theme_metrics_mode: self.config.theme_metrics_mode,
        };
        let menu_bar_visible = self.menu_bar_visible();
        let instructions_per_tick = self.instructions_per_tick;
        let wait_sleep_cap_in_headless = self.wait_sleep_cap_in_headless;
        let app_start_time = self.app_start_time;
        let application_partition_size = self.application_partition_size;
        let total_instructions = self.total_instructions;
        let launch_tick = self.guest_tick();
        let launch_time = self.bus.read_long(addr::TIME);
        let mouse_pos = self.dispatcher.mouse_pos;
        let mouse_button = self.dispatcher.mouse_button;
        let output_dir = self.dispatcher.output_dir.clone();
        let vfs = self.dispatcher.vfs.clone();
        let vfs_rsrc = self.dispatcher.vfs_rsrc.clone();
        let vfs_metadata = self.dispatcher.vfs_metadata.clone();
        let vfs_directories = self.dispatcher.vfs_directories.clone();
        let vfs_directory_paths = self.dispatcher.vfs_directory_paths.clone();
        let locked_files = self.dispatcher.locked_files.clone();
        let next_vfs_dir_id = self.dispatcher.next_vfs_dir_id;
        let next_vfs_file_id = self.dispatcher.next_vfs_file_id;
        let next_vfs_timestamp = self.dispatcher.next_vfs_timestamp;
        let next_working_dir_refnum = self.dispatcher.next_working_dir_refnum;

        let mut replacement = FixtureRunner::new(ram_size, config);
        replacement.set_menu_bar_visible(menu_bar_visible);
        replacement.instructions_per_tick = instructions_per_tick;
        replacement.tick_budget = instructions_per_tick as i32;
        replacement.wait_sleep_cap_in_headless = wait_sleep_cap_in_headless;
        replacement.app_start_time = app_start_time;
        replacement.application_partition_size = application_partition_size;
        replacement.total_instructions = total_instructions;

        replacement.dispatcher.output_dir = output_dir;
        replacement.dispatcher.vfs = vfs;
        replacement.dispatcher.vfs_rsrc = vfs_rsrc;
        replacement.dispatcher.vfs_metadata = vfs_metadata;
        replacement.dispatcher.vfs_directories = vfs_directories;
        replacement.dispatcher.vfs_directory_paths = vfs_directory_paths;
        replacement.dispatcher.locked_files = locked_files;
        replacement.dispatcher.next_vfs_dir_id = next_vfs_dir_id;
        replacement.dispatcher.next_vfs_file_id = next_vfs_file_id;
        replacement.dispatcher.next_vfs_timestamp = next_vfs_timestamp;
        replacement.dispatcher.next_working_dir_refnum = next_working_dir_refnum;
        replacement.dispatcher.set_launched_app_path(&normalized);

        let app = replacement
            .load_app(&fork)
            .ok_or_else(|| format!("failed to load launched application {normalized:?}"))?;
        replacement.init_app(&app);
        replacement.bus.write_long(addr::TICKS, launch_tick);
        replacement.bus.write_long(addr::TIME, launch_time);
        replacement.dispatcher.tick_count = launch_tick;
        replacement.dispatcher.mouse_pos = mouse_pos;
        replacement.dispatcher.mouse_button = mouse_button;
        replacement
            .bus
            .write_byte(addr::MB_STATE, if mouse_button { 0x00 } else { 0x80 });
        replacement.clear_startup_framebuffer();

        replacement.audio = self.audio.take();
        replacement.audio_buffer = std::mem::take(&mut self.audio_buffer);

        eprintln!("[LAUNCH] Switched foreground application to {normalized}");
        *self = replacement;
        Ok(())
    }

    fn service_pending_launch_application(&mut self, event_yield_reached: bool) -> bool {
        let Some(path) = self
            .dispatcher
            .take_pending_launch_application(event_yield_reached)
        else {
            return false;
        };

        if let Err(err) = self.switch_to_launched_application(&path) {
            eprintln!("[LAUNCH] Failed to switch to queued application {path:?}: {err}");
            self.halted = true;
            self.halted_pc = Some(self.cpu.read_reg(Register::PC));
            self.halted_sp = Some(self.cpu.read_reg(Register::A7));
            self.halted_d0 = Some((-43i32) as u32);
        }
        true
    }

    pub(crate) fn merge_resources_into_application(&mut self, fork: &ResourceFork) -> usize {
        self.dispatcher
            .merge_resources_into_existing_file(fork, &mut self.bus, 0)
    }

    fn alloc_handle_with_bytes(&mut self, bytes: &[u8]) -> u32 {
        let data_ptr = if bytes.is_empty() {
            0
        } else {
            let data_ptr = self.bus.alloc(bytes.len() as u32);
            if data_ptr == 0 {
                return 0;
            }
            self.bus.write_bytes(data_ptr, bytes);
            data_ptr
        };

        let handle = self.bus.alloc(4);
        if handle == 0 {
            if data_ptr != 0 {
                self.bus.free(data_ptr);
            }
            return 0;
        }

        self.bus.write_long(handle, data_ptr);
        if data_ptr != 0 {
            self.dispatcher.ptr_to_handle.insert(data_ptr, handle);
        }
        handle
    }

    fn write_fixed_pstring(&mut self, ptr: u32, value: &str, max_len: usize) {
        let bytes = value.as_bytes();
        let len = bytes.len().min(max_len);
        self.bus.write_byte(ptr, len as u8);
        for (i, &byte) in bytes.iter().take(len).enumerate() {
            self.bus.write_byte(ptr + 1 + i as u32, byte);
        }
    }

    fn seed_current_application_file_manager_state(&mut self) {
        use crate::memory::globals::addr;

        let Some(app_path) = self.dispatcher.launched_app_path.clone() else {
            return;
        };
        self.dispatcher.ensure_vfs_file_metadata(&app_path);
        let metadata = self
            .dispatcher
            .vfs_metadata
            .get(&app_path)
            .copied()
            .unwrap_or(crate::trap::dispatch::VfsMetadata {
                file_id: 0,
                parent_dir_id: self.dispatcher.default_dir_id,
                file_type: u32::from_be_bytes(*b"APPL"),
                creator: u32::from_be_bytes(*b"????"),
                finder_flags: 0,
                created_date: 0,
                modified_date: 0,
            });
        let resource_len = self
            .dispatcher
            .vfs_rsrc
            .get(&app_path)
            .map(|bytes| bytes.len() as u32)
            .unwrap_or(0);

        let vcb_ptr = self.bus.alloc(HFS_VCB_SIZE);
        if vcb_ptr == 0 {
            return;
        }
        self.bus.fill_bytes(vcb_ptr, HFS_VCB_SIZE, 0);
        self.bus.write_word(vcb_ptr + 8, 0x4244); // vcbSigWord: HFS volume
        self.write_fixed_pstring(
            vcb_ptr + 44,
            crate::trap::dispatch::TrapDispatcher::boot_volume_name(),
            27,
        );
        self.bus.write_word(
            vcb_ptr + 78,
            crate::trap::dispatch::BOOT_VOLUME_REF_NUM as u16,
        ); // vcbVRefNum
        self.bus
            .write_long(vcb_ptr + 172, self.dispatcher.default_dir_id);

        self.bus.write_long(addr::DEF_VCB_PTR, vcb_ptr);
        self.bus.write_word(addr::VCB_Q_HDR, 0);
        self.bus.write_long(addr::VCB_Q_HDR + 2, vcb_ptr);
        self.bus.write_long(addr::VCB_Q_HDR + 6, vcb_ptr);

        let fcb_buffer = self.bus.alloc(HFS_FCB_BUFFER_SIZE as u32);
        if fcb_buffer == 0 {
            return;
        }
        self.bus
            .fill_bytes(fcb_buffer, HFS_FCB_BUFFER_SIZE as u32, 0);
        self.bus.write_word(fcb_buffer, HFS_FCB_BUFFER_SIZE);
        let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
        self.bus.write_long(fcb, metadata.file_id);
        self.bus.write_word(fcb + 4, 0x0200); // fcbFlags bit 9: resource fork
        self.bus.write_long(fcb + 8, resource_len);
        self.bus.write_long(fcb + 12, resource_len);
        self.bus.write_long(fcb + 20, vcb_ptr);
        self.bus.write_long(fcb + 50, metadata.file_type);
        self.bus.write_long(fcb + 58, metadata.parent_dir_id);
        let app_name = crate::trap::dispatch::TrapDispatcher::vfs_basename(&app_path).to_string();
        self.write_fixed_pstring(fcb + 62, &app_name, 31);

        // Files 1992, 2-81 and 2-384: the FCB buffer begins with a length
        // word, file reference numbers are offsets into that buffer, and
        // System 7 FCBs are 94 bytes. The first application resource fork
        // access path therefore has refnum 2.
        self.bus.write_long(addr::FCB_S_PTR, fcb_buffer);
        self.bus.write_word(addr::FS_FCB_LEN, HFS_FCB_SIZE);
        self.bus
            .write_word(addr::CUR_APREF_NUM, APPLICATION_RESOURCE_REFNUM);

        self.dispatcher
            .open_files
            .insert(APPLICATION_RESOURCE_REFNUM, format!("__rsrc__{}", app_path));
        self.dispatcher
            .file_positions
            .insert(APPLICATION_RESOURCE_REFNUM, 0);
    }

    /// Seed the Mac-canonical low-memory globals (`MemTop`,
    /// `CurStackBase`, `ApplLimit`, `Lo3Bytes`, `Ticks`, etc.) and
    /// the A5 World start so `run_steps` lands the guest in a
    /// runnable state. Must be called after [`load_app`](Self::load_app)
    /// and before the first call to [`run_steps`](Self::run_steps);
    /// the higher-level `systemless::game::load_game` helper invokes it
    /// automatically.
    ///
    /// Without `init_app`, A5-relative startup code (CodeWarrior /
    /// Think C runtimes, e.g. Koji / Munchies) sees `CurStackBase` =
    /// 0 and spins forever in the globals-decompression loop.
    pub fn init_app(&mut self, app: &LoadedApp) {
        use crate::memory::globals::addr;
        let ram_size = self.bus.ram_size();

        // Classic Mac OS application code runs in supervisor mode with the
        // processor priority open to level-1 VBL interrupts. The m68k core
        // starts from CPU reset with all interrupts masked; make the launch
        // state explicit so interrupt-time HLE can honor guest SR masking.
        // Inside Macintosh: Processes (1994), pp. 1-11 and 6-3.
        let launch_sr = (self.cpu.core.get_sr() & !0x0700) | 0x2000;
        self.cpu.core.set_sr_noint_nosp(launch_sr);

        // Initialize low-memory globals
        self.bus.write_long(addr::MEM_TOP, ram_size);
        // CurStackBase ($0908): "Address of base of stack; start of
        // application global variables." Per Inside Macintosh: Memory
        // 1992, p. 2-104, this points at the boundary where the
        // application's stack region meets the A5 World — equivalently,
        // the address of the *first* below-A5 global. CodeWarrior /
        // Think C runtime startup code (e.g. Koji the Frog, Munchies)
        // reads $0908 as a destination pointer when decompressing
        // initial values into the A5 globals area; pointing it at the
        // stack top instead leaves that decompression loop spinning
        // forever because its termination condition compares the
        // walked-forward A1 against (A5)+. Use `a5_base - below_a5`
        // here — that's the Mac-canonical "start of application
        // globals" regardless of where the stack lives in our
        // (inverted) host memory map.
        let app_globals_start = app.a5_base.saturating_sub(app.code0_header.below_a5);
        self.bus.write_long(addr::CUR_STACK_BASE, app_globals_start);
        self.bus.write_long(addr::CURRENT_A5, app.a5_base);
        self.bus.write_word(addr::ROM85, 0x0000);
        // MMU32Bit ($0CB2): TRUE when 32-bit addressing mode is in effect.
        // Inside Macintosh: Memory 1992, p. 4-25 says applications can test
        // this low-memory byte directly; Systemless's TrapDispatcher already
        // defaults SwapMMUMode to true32b and Gestalt('addr') bit 0 to set.
        self.bus.write_byte(addr::MMU32_BIT, 1);
        // Initialize Ticks ($016A) to a realistic post-boot value.
        // On a real Mac, hundreds of ticks elapse during the boot ROM,
        // system extensions, and Finder startup before the application
        // launches. Games that read Ticks early (e.g. to seed a PRNG)
        // expect a non-zero value; starting at 0 produces degenerate
        // random sequences (e.g., ship heading always zero in EV).
        // 600 ticks ≈ 10 seconds of post-boot time, a conservative
        // estimate for a minimal System 7 configuration.
        self.bus.write_long(addr::TICKS, DEFAULT_LAUNCH_TICKS);
        self.dispatcher.tick_count = DEFAULT_LAUNCH_TICKS;
        let time = self
            .app_start_time
            .unwrap_or_else(current_mac_epoch_seconds);
        self.bus.write_long(addr::TIME, time);
        // DoubleTime ($02F0): maximum interval between mouseDown events that
        // constitutes a double-click. RAM starts zeroed, but zero makes the
        // canonical unsigned comparison `(thisClick - lastClick) < DoubleTime`
        // impossible. Lemmings uses that exact sequence for its nuke control.
        self.bus
            .write_long(addr::DOUBLE_TIME, DEFAULT_DOUBLE_TIME_TICKS);
        // RndSeed ($0156): system random seed initialized during boot.
        // On a real Mac, the boot code seeds this from the real-time clock
        // so that programs that read it directly (without calling Random)
        // get non-deterministic entropy. Use the startup time so play
        // scripts produce repeatable but non-trivial random sequences.
        // Inside Macintosh Volume II, II-387
        self.bus.write_long(addr::RND_SEED, time);
        // MBState: $80 = button UP (Mac convention: 0 = down, $80 = up).
        // RAM is zero-initialized which would mean "button down" — must set explicitly.
        self.bus.write_byte(addr::MB_STATE, 0x80);
        // KeyMapLM ($0174): ROM-maintained current-key bitmap mirrored by
        // GetKeys. Keep it explicitly clear at launch for direct pollers.
        // Inside Macintosh Volume I, I-260; MPW SysEqu.h `KeyMapLM`.
        self.sync_key_map_lowmem();
        // JCrsrTask ($08EE): address of the cursor VBL task routine.
        // MPW Interfaces/AIncludes/LowMemEqu.a lists `JCrsrTask EQU $8EE`.
        // Classic applications can wrap this low-memory vector and then wait
        // for their wrapper to run at interrupt time, so the default must be
        // both non-NIL and callable. `$0060` is one of Systemless's low-memory
        // RTS stubs for direct-call compatibility.
        self.bus.write_word(CURSOR_TASK_NOOP_ADDR, 0x4E75);
        self.bus
            .write_long(addr::J_CRSR_TASK, CURSOR_TASK_NOOP_ADDR);
        // MBarHeight: 20 pixels (standard Roman system script value).
        // Games may set this to 0 to hide the menu bar for full-screen mode.
        // Inside Macintosh Volume V, V-245
        self.bus.write_word(addr::MBAR_HEIGHT, 20);
        // SdVolume ($0260): current speaker volume, low three bits only.
        // Inside Macintosh Volume III, III-425. Some classic apps also read
        // this byte directly as a "Sound Driver present" sentinel — most
        // notably Marathon 1, whose sound module (CODE 5 +$0003F2:
        // `MOVE.B (mem $260).W, (A0)`) short-circuits its audio submission
        // path when this byte is zero. Initialize it to the minimum nonzero
        // compatibility value; higher legacy volume values can change old
        // Sound Driver clients' control flow.
        self.bus.write_byte(addr::SD_VOLUME, 1);

        // Memory Manager zone globals
        // Inside Macintosh Volume II, II-19 and II-29..II-30.
        // Keep actual Systemless allocations above the direct-loaded image,
        // while the guest-visible application zone can remain at the normal
        // floor when the loader has placed the application image above it.
        // Real Mac CODE/resources live inside the app heap; Systemless writes
        // them directly and then protects them by bumping the allocator.
        let allocation_heap_start = app_heap_start_for_loaded_app(app);
        let visible_zone_start = app_visible_zone_start_for_loaded_app(app);
        let zone_header_size: u32 = APP_ZONE_HEADER_SIZE;
        let initial_heap_end = visible_zone_start + zone_header_size;
        let minimum_safe_appl_limit = self.bus.heap_bump_ptr().max(allocation_heap_start);
        let stack_base = app.initial_sp;
        let default_appl_limit = stack_base - APP_STACK_SAFETY_MARGIN;
        let requested_partition_size = self.application_partition_size.or_else(|| {
            app.size_resource
                .and_then(|size| size.preferred_partition_size())
        });
        let appl_limit = requested_partition_size
            .and_then(|partition_size| {
                // Processes 1994, pp. 1-3 and 2-18: the Process Manager
                // allocates the application partition from the app's 'SIZE'
                // resource preferred size when available. A scripted override
                // represents the same Finder-style preferred-memory setting
                // applied to a temporary launch. Systemless keeps the physical
                // stack at the top of guest RAM, but narrows the observable
                // heap limit so FreeMem/MaxMem/process info see the same
                // partition pressure.
                partition_size
                    .checked_sub(APP_STACK_SAFETY_MARGIN)
                    .and_then(|heap_span| visible_zone_start.checked_add(heap_span))
            })
            .map(|limit| limit.max(minimum_safe_appl_limit))
            .filter(|&limit| limit > initial_heap_end && limit < default_appl_limit)
            .unwrap_or(default_appl_limit.max(initial_heap_end));
        let buf_ptr = appl_limit; // Buffer area at the limit
        self.bus.write_long(addr::SYS_ZONE, visible_zone_start);
        self.bus.write_long(addr::APP_L_ZONE, visible_zone_start);
        self.bus.write_long(addr::HEAP_END, initial_heap_end);
        self.bus.write_long(addr::APPL_LIMIT, appl_limit);
        self.bus.write_long(addr::BUF_PTR, buf_ptr);
        self.bus.write_long(addr::THE_ZONE, visible_zone_start);

        // CurApRefNum: resource file reference number of the application (word).
        // CurApName: application name as Pascal string (Str31).
        // AppParmHandle is allocated below, after the zone header is reserved.
        // Inside Macintosh Volume II, II-57 to II-58
        self.bus.write_word(addr::CUR_APREF_NUM, 0);
        if let Some(app_path) = &self.dispatcher.launched_app_path {
            let app_name = crate::trap::dispatch::TrapDispatcher::vfs_basename(app_path);
            let name_bytes = app_name.as_bytes();
            let len = name_bytes.len().min(31);
            self.bus.write_byte(addr::CUR_APNAME, len as u8);
            for (i, &b) in name_bytes.iter().take(len).enumerate() {
                self.bus.write_byte(addr::CUR_APNAME + 1 + i as u32, b);
            }
        }

        // Set the current directory to the application's parent folder so that
        // file-relative lookups (e.g. Marathon opening "Music") resolve correctly.
        // CurDirStore: directory ID of directory last opened (long)
        // SFSaveDisk: negative of volume reference number (word)
        // Inside Macintosh Volume IV, IV-72
        let app_dir_id = self.dispatcher.default_dir_id;
        self.bus.write_long(addr::CUR_DIR_STORE, app_dir_id);
        self.bus.write_word(
            addr::SF_SAVE_DISK,
            (-crate::trap::dispatch::BOOT_VOLUME_REF_NUM) as u16,
        );
        eprintln!(
            "[INIT] CurDirStore={} SFSaveDisk={}",
            app_dir_id,
            (-crate::trap::dispatch::BOOT_VOLUME_REF_NUM) as u16
        );

        // The application stack is ordinary RAM used for stack frames and
        // local variables; classic Mac code does not get it pre-cleared.
        // Memory 1992, 1-9 and 1-39 describe the stack as the region where
        // stack frames live and grow downward from high memory. Seed the
        // unused top-of-stack window with a deterministic nonzero pattern so
        // partially initialized stack records behave like real hardware
        // instead of inheriting zeroed RAM.
        let stack_seed_start = stack_base.saturating_sub(0x8000);
        self.bus
            .fill_bytes(stack_seed_start, stack_base - stack_seed_start, 0xA5);

        // Zone header at visible_zone_start (Inside Macintosh Volume II, II-22)
        // Apps and the Memory Manager read the zone header to determine
        // available memory. zcbFree (offset +12) must reflect free bytes.
        // Reserve heap space so alloc() doesn't overwrite the zone header.
        self.bus
            .reserve_heap_until(allocation_heap_start.saturating_add(zone_header_size));
        let zone_size = appl_limit.saturating_sub(visible_zone_start);
        let free_bytes = zone_size.saturating_sub(zone_header_size);
        self.bus.write_long(visible_zone_start, appl_limit); // bkLim: end of zone
        self.bus.write_long(
            visible_zone_start + 8,
            visible_zone_start + zone_header_size,
        ); // hFstFree
        self.bus.write_long(visible_zone_start + 12, free_bytes); // zcbFree: total free
        self.bus.write_long(
            visible_zone_start + 56,
            visible_zone_start + zone_header_size,
        ); // allocPtr
        eprintln!(
            "[INIT] Zone header: start=${:08X} allocStart=${:08X} bkLim=${:08X} zcbFree={} ({:.1}MB)",
            visible_zone_start,
            allocation_heap_start,
            appl_limit,
            free_bytes,
            free_bytes as f64 / (1024.0 * 1024.0)
        );
        self.seed_current_application_file_manager_state();

        // AppParmHandle: handle to Finder information about files selected
        // when launching the application. A normal Finder application launch
        // with no documents still provides the message/count header:
        // appOpen (0), count 0. Assembly code may read this global directly.
        // Inside Macintosh Volume II, II-57; Files 1992, 1-58.
        let app_param_handle = self.alloc_handle_with_bytes(&[0, 0, 0, 0]);
        self.bus.write_long(addr::APP_PARM_HANDLE, app_param_handle);

        // Write an ExitToShell trap at a known low-memory address so that when
        // main() returns via RTS, the CPU executes ExitToShell and halts cleanly.
        // We use address 0x100 (safe, unused low memory) to hold the A-line instruction.
        let exit_trampoline = 0x100u32;
        self.bus.write_word(exit_trampoline, 0xA9F4); // ExitToShell

        // Pre-allocate the main GDevice with 800x600 8bpp settings
        // and set the low-memory globals that games read directly.
        // screenBits is already initialized to 800x600 8bpp by the bus.
        let gdh = self.dispatcher.ensure_main_gdevice(&mut self.bus);
        let gd_ptr = self.bus.read_long(gdh);
        self.bus.write_long(0x8A4, gdh); // MainDevice
        self.bus.write_long(0xCC8, gdh); // TheGDevice
        self.bus.write_long(0x8A8, gdh); // DeviceList
        eprintln!(
            "[INIT] Set MainDevice=${:08X}, TheGDevice=${:08X} (ptr=${:08X})",
            gdh, gdh, gd_ptr
        );

        // Initialize screen_mode from the main GDevice PixMap.
        let pmap_h = self.bus.read_long(gd_ptr + 22);
        let pmap = self.bus.read_long(pmap_h);
        let scrn_base = self.bus.read_long(pmap);
        let rb = (self.bus.read_word(pmap + 4) & 0x3FFF) as u32;
        let top = self.bus.read_word(pmap + 6) as i16;
        let left = self.bus.read_word(pmap + 8) as i16;
        let bottom = self.bus.read_word(pmap + 10) as i16;
        let right = self.bus.read_word(pmap + 12) as i16;
        let pixel_size = self.bus.read_word(pmap + 32);
        let width = (right - left).max(1) as u16;
        let height = (bottom - top).max(1) as u16;
        self.dispatcher.screen_mode = (scrn_base, rb, width, height, pixel_size);

        // Debug: dump the GDevice chain to verify correctness
        {
            let main_dev = self.bus.read_long(0x8A4);
            let gd = self.bus.read_long(main_dev);
            let pmap_h = self.bus.read_long(gd + 22);
            let pmap = self.bus.read_long(pmap_h);
            let rb = self.bus.read_word(pmap + 4);
            let top = self.bus.read_word(pmap + 6) as i16;
            let left = self.bus.read_word(pmap + 8) as i16;
            let bottom = self.bus.read_word(pmap + 10) as i16;
            let right = self.bus.read_word(pmap + 12) as i16;
            let ps = self.bus.read_word(pmap + 32);
            let gd_top = self.bus.read_word(gd + 34) as i16;
            let gd_left = self.bus.read_word(gd + 36) as i16;
            let gd_bottom = self.bus.read_word(gd + 38) as i16;
            let gd_right = self.bus.read_word(gd + 40) as i16;
            let gd_flags = self.bus.read_word(gd + 20);
            eprintln!(
                "[INIT] GDevice chain: $8A4→${:08X}→${:08X} gdPMap→${:08X}→${:08X}",
                main_dev, gd, pmap_h, pmap
            );
            eprintln!(
                "[INIT]   PixMap: rowBytes=${:04X} bounds=({},{},{},{}) pixelSize={}",
                rb, top, left, bottom, right, ps
            );
            eprintln!(
                "[INIT]   GDevice: gdRect=({},{},{},{}) gdFlags=${:04X}",
                gd_top, gd_left, gd_bottom, gd_right, gd_flags
            );
        }

        // JSwapFont ($08E0): private Font Manager vector used by QuickDraw to
        // call FMSwapFont directly. Executor's clean-room low-memory table
        // identifies the address and initializes it from the $A901 routine.
        //
        // A JSR has placed its return address above the four-byte pointer to
        // the caller's FMInput record. Pop that return address before entering
        // the HLE trap so A7 points at the documented argument, then jump back
        // after the trap
        // leaves A7 on the four-byte FMOutPtr result slot. Allocate this only
        // after reserving the application zone header so it remains live.
        let swap_font_trampoline = self.bus.alloc(6);
        self.bus
            .write_word(swap_font_trampoline, 0x205F); // MOVEA.L (SP)+,A0
        self.bus
            .write_word(swap_font_trampoline + 2, 0xA901); // FMSwapFont
        self.bus
            .write_word(swap_font_trampoline + 4, 0x4ED0); // JMP (A0)
        self.bus.write_long(addr::J_SWAP_FONT, swap_font_trampoline);

        // Set CPU state
        self.cpu.write_reg(Register::A5, app.a5_base);
        // Push the exit trampoline as the return address on the stack
        let sp = app.initial_sp.wrapping_sub(4);
        self.bus.write_long(sp, exit_trampoline);
        self.cpu.write_reg(Register::A7, sp);
        // Initialize A6 (frame pointer) to the stack pointer.
        // On a real Mac, the Process Manager sets up A6 before launching
        // the application. The CRT startup code (e.g. Think C's __start)
        // expects A6 to be a valid stack address for its initial LINK frame.
        self.cpu.write_reg(Register::A6, sp);
        self.cpu
            .write_reg(Register::PC, app.entry_point(app.a5_base));
    }

    /// Mix and queue audio samples without full frame finalization.
    /// Used to keep the audio buffer fed during long CPU frames.
    pub fn mix_audio(&mut self, num_samples: usize) {
        self.mix_host_audio(num_samples);
    }

    fn queue_mixed_audio(&mut self, stereo_samples: &[u8]) {
        if stereo_samples.is_empty() {
            return;
        }
        if let Some(ref mut audio) = self.audio {
            audio.queue_stereo_samples(stereo_samples);
        }
        for frame in stereo_samples.chunks_exact(2) {
            let left = frame[0] as i32 - 0x80;
            let right = frame[1] as i32 - 0x80;
            self.audio_buffer
                .push(((left + right) / 2 + 0x80).clamp(0, 255) as u8);
        }
    }

    fn queue_host_silence_audio(&mut self, num_samples: usize) {
        if num_samples == 0 {
            return;
        }
        if let Some(ref mut audio) = self.audio {
            audio.queue_stereo_samples(&vec![0x80; num_samples * 2]);
        }
    }

    fn mix_host_audio(&mut self, mut remaining_samples: usize) {
        while remaining_samples > 0 {
            self.try_load_pending_double_buffers();
            self.dispatcher.service_guest_sound_queues(&mut self.bus);

            let chunk = self
                .dispatcher
                .sound_manager
                .samples_until_next_exhaustion()
                .map(|samples| samples.max(1).min(remaining_samples))
                .unwrap_or(remaining_samples);

            let samples = self.dispatcher.sound_manager.mix_frame_stereo(chunk);
            if samples.is_empty() {
                self.queue_host_silence_audio(remaining_samples);
                self.dispatcher
                    .release_finished_internal_sound_channels(&mut self.bus);
                break;
            }
            self.queue_mixed_audio(&samples);
            self.dispatcher
                .release_finished_internal_sound_channels(&mut self.bus);
            remaining_samples -= chunk;
        }
    }

    fn finish_host_frame(&mut self, audio_samples: usize, sound_interrupt_dispatched: bool) {
        // Redraw menu bar and window chrome after each frame.
        // On a real Mac the Window Manager maintains these as
        // separate layers; here they are raw framebuffer pixels
        // that game drawing (explosions, etc.) can overwrite.
        self.dispatcher.redraw_chrome(&mut self.bus);
        self.finish_audio_frame(audio_samples, sound_interrupt_dispatched);
    }

    fn finish_audio_frame(&mut self, audio_samples: usize, sound_interrupt_dispatched: bool) {
        // Try to load any double-buffer data that callbacks have refilled.
        self.try_load_pending_double_buffers();

        // Sound channels expose an in-memory queue that some games update
        // directly instead of routing every command through SndDoCommand.
        self.dispatcher.service_guest_sound_queues(&mut self.bus);

        // Mix and output audio for this frame.
        if audio_samples > 0 {
            self.mix_host_audio(audio_samples);
        }

        self.dispatcher
            .sync_guest_sound_channel_state(&mut self.bus);

        if !sound_interrupt_dispatched {
            let fired_sound_callback = self.fire_sound_callbacks();
            if !fired_sound_callback {
                // Fire pending double-buffer callbacks (SndPlayDoubleBuffer).
                self.fire_sound_doubleback_callbacks();
            }
        }
    }

    /// Mix a GUI-only audio slice without running foreground guest code or
    /// redrawing the frame. Used by realtime frontends to let Sound Manager
    /// doubleback callbacks run between small audio chunks when TickCount is
    /// already caught up to the wall clock.
    pub fn mix_gui_audio_slice(&mut self, audio_samples: usize) {
        self.finish_audio_frame(audio_samples, false);
    }

    /// Try to fast-forward past a TickCount spin-wait loop. Returns
    /// true iff the `tick_cap` was hit during advancement (caller
    /// should break the outer run loop); returns false if no match,
    /// if the advance succeeded, or if the max-cap
    /// (`SPIN_FASTFWD_MAX_TICKS`) protected us from a runaway target.
    /// All bytes are read from guest memory — the function never runs
    /// game code.
    fn try_tickcount_spin_fastfwd(
        &mut self,
        pc_after_trap: u32,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let w0 = self.bus.read_word(pc_after_trap);

        // Template D consumes TickCount directly from its stack result slot:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,Dn; Bcc.S <back-to-CLR>
        // Lemmings uses the BEQ form for its frame delay loop.
        if (w0 & 0xF1FF) == 0xB09F {
            let dn = ((w0 >> 9) & 7) as usize;
            return self.try_spin_template_d(pc_after_trap, dn, tick_cap);
        }

        // Step 1: MOVE.L (A7)+, Dn (shared by templates A-C).
        if (w0 & 0xF1FF) != 0x201F {
            return false;
        }
        let dn = ((w0 >> 9) & 7) as usize;

        let w1 = self.bus.read_word(pc_after_trap.wrapping_add(2));

        // Template A: SUBQ.L #imm, Dn; CMP.L Dn, Dm; BHI.S <back-to-SUBQ-#4,A7>
        if (w1 & 0xF1F8) == 0x5180 && (w1 & 0x0007) as usize == dn {
            return self.try_spin_template_a(pc_after_trap, dn, w1, tick_cap, count);
        }

        // Template B: CMP.L (d16, An), Dn; BLS.S <back>
        if (w1 & 0xF1F8) == 0xB0A8 && ((w1 >> 9) & 7) as usize == dn {
            return self.try_spin_template_b(pc_after_trap, dn, w1, tick_cap, count);
        }

        // Template C: CMP.L (xxx).L, Dn; BCS.S <back>
        if (w1 & 0xF1FF) == 0xB0B9 && ((w1 >> 9) & 7) as usize == dn {
            return self.try_spin_template_c(pc_after_trap, dn, tick_cap, count);
        }

        false
    }

    /// Template D: stack-result compare variant used by Lemmings.
    ///   CLR.L  -(A7)
    ///   _TickCount
    ///   CMP.L  (A7)+, Dn
    ///   BCC.S/BEQ.S <back-to-CLR.L>
    ///
    /// BCC repeats while `Dn >= TickCount()`, so its first fall-through tick is
    /// `Dn + 1`. BEQ repeats while `Dn == TickCount()`; only accelerate it when
    /// the just-captured tick equals Dn, and advance by exactly one tick. Leave
    /// CMP/Bcc for the CPU to execute once after advancing; that preserves its
    /// exact flags, stack update, and instruction count.
    fn try_spin_template_d(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        tick_cap: Option<u32>,
    ) -> bool {
        let w_branch = self.bus.read_word(pc_after_trap.wrapping_add(2));
        let branch_condition = w_branch & 0xFF00;
        if branch_condition != 0x6400 && branch_condition != 0x6700 {
            return false;
        }
        let displacement = (w_branch & 0xFF) as i8 as i32;
        if displacement == 0 {
            return false;
        }
        let branch_pc = pc_after_trap.wrapping_add(2);
        let target = (branch_pc.wrapping_add(2) as i32).wrapping_add(displacement) as u32;
        if target != pc_after_trap.wrapping_sub(4) || self.bus.read_word(target) != 0x42A7 {
            return false;
        }

        let dn_value = self.cpu.core.d(dn);
        let captured_tick = self.bus.read_long(self.cpu.core.a(7));
        if branch_condition == 0x6700 && captured_tick != dn_value {
            // BEQ would already fall through, so there is no wait to skip.
            return false;
        }
        let target_tick = dn_value.wrapping_add(1);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => {
                // The trap's result was captured before the synthetic VBLs.
                // Refresh it so the resumed comparison observes the same tick
                // that a real busy loop would obtain on its next iteration.
                let sp = self.cpu.core.a(7);
                self.bus.write_long(sp, self.dispatcher.tick_count);
                true
            }
            AdvanceResult::Advanced => {
                let sp = self.cpu.core.a(7);
                self.bus.write_long(sp, self.dispatcher.tick_count);
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => false,
        }
    }

    /// Template A: classic pre-System-7 SUBQ-compare spin.
    ///   MOVE.L (A7)+, Dn
    ///   SUBQ.L #imm, Dn
    ///   CMP.L  Dn, Dm
    ///   BHI.S  <SUBQ.W #4, A7 before the _TickCount>
    fn try_spin_template_a(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w1: u16,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let imm_bits = ((w1 >> 9) & 7) as u32;
        let imm = if imm_bits == 0 { 8 } else { imm_bits };

        let w2 = self.bus.read_word(pc_after_trap.wrapping_add(4));
        let w3 = self.bus.read_word(pc_after_trap.wrapping_add(6));

        // CMP.L Dn, Dm (0xB_80 family, src-mode 000 = data reg direct).
        if (w2 & 0xF1F8) != 0xB080 || (w2 & 0x0007) as usize != dn {
            return false;
        }
        let dm = ((w2 >> 9) & 7) as usize;

        // BHI.S
        if (w3 & 0xFF00) != 0x6200 {
            return false;
        }
        let disp8 = (w3 & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        let branch_src = pc_after_trap.wrapping_add(6);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        if target != pc_after_trap.wrapping_sub(4) {
            return false;
        }

        let dm_val = self.cpu.core.d(dm);
        let target_tick = dm_val.wrapping_add(imm);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        // Synthesise exit: Dn = final_tick - imm = Dm (by definition of
        // the fall-through condition), A7 += 4, PC past BHI.S.
        let final_tick = self.dispatcher.tick_count;
        let sp = self.cpu.core.a(7);
        self.cpu.core.set_a(7, sp.wrapping_add(4));
        self.cpu.core.set_d(dn, final_tick.wrapping_sub(imm));
        self.cpu.core.pc = pc_after_trap.wrapping_add(8);

        *count += 4;
        self.total_instructions = self.total_instructions.wrapping_add(4);
        false
    }

    /// Template B: memory-target variant.
    ///   MOVE.L (A7)+, Dn
    ///   CMP.L  (d16, An), Dn    ; 4 bytes (opcode word + d16)
    ///   BLS.S  <backward, into body that rewrites (An+d16)>
    ///
    /// Exit when `TickCount() > *(An+d16)`, i.e. `target_tick =
    /// *(An+d16) + 1`.
    fn try_spin_template_b(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w1: u16,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let an = (w1 & 7) as usize;
        let d16 = self.bus.read_word(pc_after_trap.wrapping_add(4)) as i16 as i32;
        let w_brk = self.bus.read_word(pc_after_trap.wrapping_add(6));

        // BLS.S disp8
        if (w_brk & 0xFF00) != 0x6300 {
            return false;
        }
        let disp8 = (w_brk & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        // Branch target must be a short backward branch. We don't
        // insist on an exact target since template B's body runs
        // BEFORE the _TickCount trap (not just the SUBQ #4, A7).
        let branch_src = pc_after_trap.wrapping_add(6);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        if target >= pc_after_trap || pc_after_trap.wrapping_sub(target) > 128 {
            return false;
        }

        let an_val = self.cpu.core.a(an);
        let mem_addr = (an_val as i32).wrapping_add(d16) as u32;
        let mem_target = self.bus.read_long(mem_addr);
        let target_tick = mem_target.wrapping_add(1);

        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        // Synthesise exit: Dn = final_tick, A7 += 4, PC past BLS.S.
        // body_size: MOVE.L (2) + CMP.L w/d16 (4) + BLS.S (2) = 8 bytes.
        let final_tick = self.dispatcher.tick_count;
        let sp = self.cpu.core.a(7);
        self.cpu.core.set_a(7, sp.wrapping_add(4));
        self.cpu.core.set_d(dn, final_tick);
        self.cpu.core.pc = pc_after_trap.wrapping_add(8);

        *count += 3;
        self.total_instructions = self.total_instructions.wrapping_add(3);
        false
    }

    /// Template C: absolute-long target variant.
    ///   MOVE.L (A7)+, Dn
    ///   CMP.L  (xxx).L, Dn
    ///   BCS.S  <back-to-SUBQ.W #4,A7 before the _TickCount>
    ///
    /// Exit when `TickCount() >= *(xxx).L`.
    fn try_spin_template_c(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let target_addr = self.bus.read_long(pc_after_trap.wrapping_add(4));
        let w_brk = self.bus.read_word(pc_after_trap.wrapping_add(8));

        // BCS.S/BLO.S disp8. The loop repeats while Dn < *(xxx).L.
        if (w_brk & 0xFF00) != 0x6500 {
            return false;
        }
        let disp8 = (w_brk & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        let branch_src = pc_after_trap.wrapping_add(8);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        if target != pc_after_trap.wrapping_sub(4) {
            return false;
        }

        let target_tick = self.bus.read_long(target_addr);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        let final_tick = self.dispatcher.tick_count;
        let sp = self.cpu.core.a(7);
        self.cpu.core.set_a(7, sp.wrapping_add(4));
        self.cpu.core.set_d(dn, final_tick);
        self.cpu.core.pc = pc_after_trap.wrapping_add(10);

        *count += 3;
        self.total_instructions = self.total_instructions.wrapping_add(3);
        false
    }

    /// Shared helper: advance guest ticks until `target_tick` is
    /// reached.
    fn advance_until_tick(&mut self, target_tick: u32, tick_cap: Option<u32>) -> AdvanceResult {
        let current_tick = self.dispatcher.tick_count;
        let ticks_to_advance = target_tick.wrapping_sub(current_tick);
        if ticks_to_advance > SPIN_FASTFWD_MAX_TICKS {
            return AdvanceResult::TooFar;
        }
        for _ in 0..ticks_to_advance {
            if let Some(cap) = tick_cap {
                if self.bus.read_long(0x016A) >= cap {
                    return AdvanceResult::CapHit;
                }
            }
            self.advance_guest_tick();
            if self.active_interrupt_callback.is_some() {
                return AdvanceResult::Interrupted;
            }
        }
        AdvanceResult::Advanced
    }

    fn run_steps_internal(
        &mut self,
        max_steps: usize,
        tick_cap: Option<u32>,
        audio_samples: usize,
        yield_for_ui: bool,
        sound_work_only: bool,
        finish_frame: bool,
    ) -> (usize, bool) {
        // Freeze ticks while menu/control tracking is active. ModalDialog
        // refires still return to the GUI for intermediate rendering, but
        // they must not freeze ticks: EV's pilot dialogs keep Sound/VBL/Time
        // Manager work alive through the dialog manager's event loop.
        // On entry, cap tick_cap to the frozen value; when tracking ends
        // mid-frame, snap $016A to wall-clock time so there's no gap to catch
        // up on.
        let real_tick_cap = tick_cap;
        let tick_cap = match self.frozen_ticks {
            Some(frozen) => tick_cap.map(|_| frozen),
            None => tick_cap,
        };

        self.dispatcher.instruction_count = self.total_instructions;
        let mut count = 0;
        let mut tick_cap_reached = false;
        let mut sound_interrupt_dispatched = self
            .active_interrupt_callback
            .map(|callback| is_sound_interrupt_source(callback.source))
            .unwrap_or(false);

        while count < max_steps && !self.halted && !tick_cap_reached {
            if sound_work_only
                && self.active_interrupt_callback.is_none()
                && (sound_interrupt_dispatched || !self.has_pending_sound_work())
            {
                break;
            }

            // Sound callbacks are interrupt work. If a previous slice queued
            // one, dispatch it before running more foreground guest code. Do
            // not drain the whole queue in one CPU slice: double-buffer
            // callbacks are paced by audio-buffer completion, and firing
            // several back-to-back at the same guest PC/tick makes games that
            // run their own mixer refill with click-sized fragments.
            if self.active_interrupt_callback.is_none() && !sound_interrupt_dispatched {
                sound_interrupt_dispatched = self.fire_sound_callbacks();
                if !sound_interrupt_dispatched {
                    sound_interrupt_dispatched = self.fire_sound_doubleback_callbacks();
                }
                if sound_work_only && !sound_interrupt_dispatched {
                    continue;
                }
            }

            if sound_work_only && self.active_interrupt_callback.is_none() {
                break;
            }

            // Service blocking traps (Delay, WaitNextEvent sleep).
            if !sound_work_only {
                if self.service_wait_sleep_ticks(tick_cap) {
                    break;
                }
                if self.service_delay_ticks(tick_cap) {
                    break;
                }
            }
            if self.cpu.is_stopped() {
                self.halted = true;
                self.halted_pc = Some(self.cpu.read_reg(Register::PC));
                self.halted_sp = Some(self.cpu.read_reg(Register::A7));
                self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                self.dump_trace();
                return (count, false);
            }

            let pc = self.cpu.read_reg(Register::PC);
            // Defer reading SP until needed. sp is only used by the
            // interrupt-callback match (rare), the env-gated
            // trace_buffer path, and the PC-bounds error branch.
            //
            // Opcode + trace_buffer reads are gated behind
            // `SYSTEMLESS_TRACE_BUFFER`. Without the gate the `read_word`
            // and `VecDeque` pop/push run on every instruction fetch
            // just so `dump_trace()` can show recent instructions on a
            // halt. Default off; enable for crash diagnostics.
            if trace_buffer_enabled() {
                let opcode = self.bus.read_word(pc);
                let a0 = self.cpu.read_reg(Register::A0);
                let a6 = self.cpu.read_reg(Register::A6);
                let a5 = self.cpu.read_reg(Register::A5);
                let sp = self.cpu.read_reg(Register::A7);
                if self.trace_buffer.len() >= 200 {
                    self.trace_buffer.pop_front();
                }
                self.trace_buffer.push_back((pc, opcode, a0, sp, a6, a5));
            }

            if let Some(active_interrupt_callback) = self.active_interrupt_callback {
                let sp = self.cpu.read_reg(Register::A7);
                if pc == active_interrupt_callback.resume_pc
                    && sp == active_interrupt_callback.resume_sp
                {
                    if trace_timer_enabled() {
                        eprintln!(
                            "[TIMER] resume {:?} pc=${:08X} sp=${:08X} restore_ccr=${:02X}",
                            active_interrupt_callback.source, pc, sp, active_interrupt_callback.ccr
                        );
                    }
                    if trace_sound_runner_enabled()
                        && is_sound_interrupt_source(active_interrupt_callback.source)
                    {
                        eprintln!(
                            "[SOUND-CB] resume {:?} pc=${:08X} sp=${:08X} restore_ccr=${:02X}",
                            active_interrupt_callback.source, pc, sp, active_interrupt_callback.ccr
                        );
                    }
                    for (index, value) in
                        active_interrupt_callback.d_regs.iter().copied().enumerate()
                    {
                        self.cpu.write_reg(
                            match index {
                                0 => Register::D0,
                                1 => Register::D1,
                                2 => Register::D2,
                                3 => Register::D3,
                                4 => Register::D4,
                                5 => Register::D5,
                                6 => Register::D6,
                                _ => Register::D7,
                            },
                            value,
                        );
                    }
                    for (index, value) in
                        active_interrupt_callback.a_regs.iter().copied().enumerate()
                    {
                        self.cpu.write_reg(
                            match index {
                                0 => Register::A0,
                                1 => Register::A1,
                                2 => Register::A2,
                                3 => Register::A3,
                                4 => Register::A4,
                                5 => Register::A5,
                                6 => Register::A6,
                                _ => Register::A7,
                            },
                            value,
                        );
                    }
                    self.cpu
                        .core
                        .set_sr_noint_nosp(active_interrupt_callback.sr);
                    if let Some((port, gdevice)) = active_interrupt_callback.restore_port {
                        self.dispatcher.set_current_port_state(
                            &mut self.bus,
                            &mut self.cpu,
                            port,
                            Some(gdevice),
                        );
                    }
                    let completed_dialog_draw_proc = matches!(
                        active_interrupt_callback.source,
                        ActiveInterruptCallbackSource::DialogDrawProc
                    );
                    let completed_modeless_dialog_draw_proc = completed_dialog_draw_proc
                        && self.dispatcher.active_modeless_dialog_draw_proc.is_some();
                    if completed_dialog_draw_proc {
                        self.dispatcher
                            .finalize_dialog_draw_procs_if_idle(&mut self.bus);
                    }
                    self.active_interrupt_callback = None;
                    self.refill_foreground_budget_after_async_return();
                    if completed_modeless_dialog_draw_proc && self.fire_modeless_dialog_draw_proc()
                    {
                        continue;
                    }
                    if sound_work_only {
                        break;
                    }
                } else if trace_timer_enabled() {
                    eprintln!(
                        "[TIMER] pending {:?} pc=${:08X} sp=${:08X} waiting_for pc=${:08X} sp=${:08X}",
                        active_interrupt_callback.source,
                        pc,
                        sp,
                        active_interrupt_callback.resume_pc,
                        active_interrupt_callback.resume_sp
                    );
                }
            }

            if pc == 0 {
                // App's RTS chain reached PC=0 — treat as clean exit.
                // Some apps (e.g. Centaurian 1.2.1) zero out our
                // exit-trampoline at \$100 during their CRT init then
                // pop past the saved A6 chain and JMP through a
                // popped-from-out-of-RAM zero. Real Mac OS would have
                // a launcher-provided return-to-Finder address; on
                // the HLE we just halt gracefully.
                if trace_load_enabled() {
                    eprintln!(
                        "[RUN_STEPS] App reached PC=0 (clean exit via deep RTS chain) at count={}",
                        count
                    );
                }
                self.dump_trace();
                self.halted = true;
                self.halted_pc = Some(0);
                self.halted_sp = Some(self.cpu.read_reg(Register::A7));
                self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                return (count, false);
            }

            if pc >= self.bus.ram_size() || pc < 0x60 {
                // Read opcode + sp on-demand in the error branch.
                let opcode = self.bus.read_word(pc);
                let sp = self.cpu.read_reg(Register::A7);
                eprintln!(
                    "[RUN_STEPS] Invalid PC ${:08X} at count={} sp=${:08X} op=${:04X}",
                    pc, count, sp, opcode
                );
                self.dump_invalid_pc_state();
                if let Some(hint) = decode_fakeptr_pc(pc) {
                    eprintln!("[RUN_STEPS]   {}", hint);
                } else if let Some((entry_pc, hint)) = self.trace_find_fakeptr_entry() {
                    eprintln!(
                        "[RUN_STEPS]   PC drifted ${:X} bytes from a fake-ptr entry at \
                         ${:08X}. {}",
                        pc.wrapping_sub(entry_pc),
                        entry_pc,
                        hint
                    );
                }
                self.halted = true;
                self.halted_pc = Some(pc);
                self.halted_sp = Some(sp);
                self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                self.dump_trace();
                return (count, false);
            }

            // Update debug counters for watchpoint tracking (debug builds only)
            #[cfg(debug_assertions)]
            if crate::memory::bus::watchpoint_armed() {
                crate::memory::bus::increment_step();
                crate::memory::bus::set_current_pc(pc);
                crate::memory::bus::set_watch_registers(
                    self.cpu.read_reg(Register::A0),
                    self.cpu.read_reg(Register::A1),
                    self.cpu.read_reg(Register::A6),
                    self.cpu.read_reg(Register::A7),
                );
            }

            // Mirror PC for release-mode memory/framebuffer traces;
            // watchpoint context above is debug-only.
            if crate::memory::bus::fb_write_trace_active()
                || crate::memory::bus::mem_read_trace_active()
                || crate::memory::bus::mem_write_trace_active()
            {
                crate::memory::bus::set_current_pc(pc);
            }

            let trace_pc_range_hit = trace_pc_range_contains(pc, self.dispatcher.tick_count);
            if trace_pc_range_hit {
                let sp = self.cpu.read_reg(Register::A7);
                let a6 = self.cpu.read_reg(Register::A6);
                let stack0 = self.bus.read_long(sp);
                let stack4 = self.bus.read_long(sp.wrapping_add(4));
                let stack8 = self.bus.read_word(sp.wrapping_add(8));
                let frame_ret = self.bus.read_long(a6.wrapping_add(4));
                let frame_arg = self.bus.read_word(a6.wrapping_add(8));
                eprintln!(
                    "[TRACE-PC-RANGE] pc=${:08X} op=${:04X} ccr=${:02X} d0=${:08X} d1=${:08X} d2=${:08X} d3=${:08X} d4=${:08X} d5=${:08X} d6=${:08X} d7=${:08X} a0=${:08X} a1=${:08X} a2=${:08X} a3=${:08X} a4=${:08X} a5=${:08X} a6=${:08X} sp=${:08X} stack0=${:08X} stack4=${:08X} stack8=${:04X} frame_ret=${:08X} frame_arg=${:04X}",
                    pc,
                    self.bus.read_word(pc),
                    self.cpu.core.get_ccr(),
                    self.cpu.read_reg(Register::D0),
                    self.cpu.read_reg(Register::D1),
                    self.cpu.read_reg(Register::D2),
                    self.cpu.read_reg(Register::D3),
                    self.cpu.read_reg(Register::D4),
                    self.cpu.read_reg(Register::D5),
                    self.cpu.read_reg(Register::D6),
                    self.cpu.read_reg(Register::D7),
                    self.cpu.read_reg(Register::A0),
                    self.cpu.read_reg(Register::A1),
                    self.cpu.read_reg(Register::A2),
                    self.cpu.read_reg(Register::A3),
                    self.cpu.read_reg(Register::A4),
                    self.cpu.read_reg(Register::A5),
                    a6,
                    sp,
                    stack0,
                    stack4,
                    stack8,
                    frame_ret,
                    frame_arg,
                );
            }
            // Execute up to `batch_max` instructions inside the m68k core's
            // JIT-enabled batch loop. Control returns here on the first
            // A-line trap, STOP, watched PC, or when the budget runs out —
            // so the per-iteration bookkeeping above amortises across the
            // whole batch instead of running per instruction.
            //
            // Tick accounting: the old per-step loop charged the budget
            // BEFORE each instruction, so tick-boundary side effects (a
            // tick-cap break, timer/VBL callbacks that redirect PC) took
            // effect before the boundary instruction executed. To keep
            // those semantics, the boundary instruction is pre-charged
            // here, and each batch is clamped to stop short of the next
            // boundary; everything in between is charged in bulk after
            // the batch retires (pre/post order is indistinguishable away
            // from a boundary).
            let charging = !sound_work_only
                && self.active_interrupt_callback.is_none()
                && self.frozen_ticks.is_none();
            let mut precharged = false;
            if charging && self.tick_budget <= 1 {
                if self.charge_tick_budget(1, tick_cap) {
                    break;
                }
                precharged = true;
            }
            let batch_max = if per_instruction_diagnostics_active() {
                1
            } else {
                let mut n = (max_steps - count).min(BATCH_CHUNK);
                if charging && self.active_interrupt_callback.is_none() {
                    n = n.min((self.tick_budget - 1).max(1) as usize);
                }
                n as u32
            };
            // PC 0 is always watched: a deep RTS chain unwinding to 0 is
            // the clean-exit signal handled at the top of this loop, and it
            // must halt before low memory gets executed as code. While an
            // interrupt callback is active (including one fired by the
            // pre-charge above), its resume PC is watched so the resume
            // PC+SP check above fires at the exact boundary.
            let mut watch_buf = [0u32; 2];
            let watch: &[u32] = if let Some(callback) = self.active_interrupt_callback {
                watch_buf[1] = callback.resume_pc;
                &watch_buf
            } else {
                &watch_buf[..1]
            };
            let batch = self.cpu.run_batch(&mut self.bus, batch_max, watch);
            // Trap exits consumed their opcode word too; count it like the
            // old per-step path did.
            let executed = batch.instructions as usize
                + usize::from(matches!(
                    batch.exit,
                    BatchExit::AlineTrap { .. } | BatchExit::FlineTrap { .. }
                ));
            if executed > 0 {
                count += executed;
                self.total_instructions = self.total_instructions.wrapping_add(executed as u64);
                if !sound_work_only {
                    let charge_units = executed as i32 - i32::from(precharged);
                    if self.charge_tick_budget(charge_units, tick_cap) {
                        tick_cap_reached = true;
                    }
                }
                // Per-instruction histograms: any enabled tracer forces
                // batch_max == 1 above, so these still see every retired
                // instruction. A-line exits retire zero and keep going
                // through the trap histogram instead, as before.
                if batch.instructions > 0 {
                    if trace_opcode_counts_enabled() {
                        let opcode = self.cpu.core.ir as u16 as usize;
                        self.opcode_histogram[opcode] =
                            self.opcode_histogram[opcode].saturating_add(1);
                    }
                    if trace_hot_pc_enabled()
                        && self.total_instructions.is_multiple_of(PC_SAMPLE_INTERVAL)
                    {
                        *self.pc_histogram.entry(pc).or_insert(0) += 1;
                    }
                }
            }
            match batch.exit {
                BatchExit::BudgetExhausted | BatchExit::WatchedPc { .. } => {
                    // Nothing to do: the loop top re-reads PC and handles
                    // watched addresses (interrupt-callback resume, clean
                    // exit at PC 0) exactly like the old per-step checks.
                }
                BatchExit::FlineTrap { .. } => {
                    // Preserved legacy behavior (see M68kCpu::step): F-line
                    // opcodes execute as 2-byte no-ops. PC has already
                    // advanced past the opcode word; accounting happened
                    // via `executed` above.
                }
                BatchExit::Stopped => {
                    // STOP retired mid-batch. Halt silently, matching the
                    // old flow where the next loop iteration's is_stopped
                    // check caught it.
                    self.halted = true;
                    self.halted_pc = Some(self.cpu.read_reg(Register::PC));
                    self.halted_sp = Some(self.cpu.read_reg(Register::A7));
                    self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                    self.dump_trace();
                    return (count, false);
                }
                BatchExit::TrapInstruction { trap_num } => {
                    eprintln!("[CPU] TrapInstruction: #{}", trap_num);
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::Breakpoint { bp_num } => {
                    eprintln!("[CPU] Breakpoint: #{}", bp_num);
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::IllegalInstruction { opcode } => {
                    eprintln!(
                        "[CPU] IllegalInstruction: ${:04X} at PC=${:08X}",
                        opcode, self.cpu.core.pc
                    );
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::AlineTrap { opcode } => {
                    // Accounting (count/ticks) happened via `executed`
                    // above. The batch may have retired instructions before
                    // the trap, so the loop-top `pc` is stale; the trap
                    // word's own address is in `ppc` (PC already advanced
                    // past it).
                    let pc = self.cpu.core.ppc;

                    // --- Runner-inline fast paths for hot traps ---
                    //
                    // Rule of thumb: only inline a trap's body here if
                    // BOTH hold:
                    //   (a) per-call saving > ~100ns (handler does
                    //       non-trivial work relative to dispatch
                    //       overhead), AND
                    //   (b) call count > ~5M per reference workload.
                    // Below either threshold, the dispatch-cost
                    // saving is swamped by I-cache pressure from
                    // adding more code to this hot loop.
                    //
                    // Current inlines:
                    //   $A975 TickCount
                    //   $A991 ModalDialog no-op
                    // plus:
                    //   $A975 spin-wait fast-fwd — detects TickCount
                    //       compare-and-branch templates and advances
                    //       guest ticks.
                    // --- end guidance ---

                    // Pre-dispatch fast path for TickCount ($A975).
                    // Handler body is `read self.tick_count` (cached)
                    // + write to SP. Skip the full dispatch →
                    // `dispatch_toolbox` → match chain; inline the
                    // 3-line body directly. Counters still update so
                    // logs and the trap histogram reflect real
                    // dispatch count.
                    if opcode == 0xA975 {
                        let sp = self.cpu.core.a(7);
                        let tick = self.dispatcher.tick_count;
                        self.bus.write_long(sp, tick);
                        self.dispatcher.trap_count += 1;
                        self.dispatcher.current_trap_word = opcode;
                        let idx = (opcode & 0xFFF) as usize;
                        self.dispatcher.trap_histogram[idx] =
                            self.dispatcher.trap_histogram[idx].saturating_add(1);
                        // Count this entry as inline-skipped — the
                        // fast path bypassed dispatch().
                        self.dispatcher.inline_skipped[idx] =
                            self.dispatcher.inline_skipped[idx].saturating_add(1);

                        // Generic TickCount spin-wait fast-forward.
                        // Check post-trap bytes against the spin-wait
                        // template; if matched, skip straight past the
                        // loop. m68k's step() advances PC past the A-trap
                        // (read_imm_16 does pc += 2), so the post-trap
                        // PC is already at pc + 2.
                        if spin_wait_fastfwd_enabled_for(yield_for_ui, tick_cap) {
                            let post_trap_pc = pc.wrapping_add(2);
                            let hit_cap =
                                self.try_tickcount_spin_fastfwd(post_trap_pc, tick_cap, &mut count);
                            if hit_cap {
                                break;
                            }
                        }
                        continue;
                    }

                    // Pre-dispatch fast skip for no-op ModalDialog
                    // refires. When dialog tracking is active and no
                    // state change is possible this step (draw procs
                    // done, pixels already captured, no filter, no
                    // flash animation, no queued events), skip the
                    // full dispatch → handler → post-dispatch rewind
                    // loop entirely. Rewind PC directly and continue.
                    // Counters still update so logs and the trap
                    // histogram reflect real dispatch count.
                    if opcode == 0xA991 {
                        // Extracted into `modaldialog_refire_is_noop` so
                        // the gate logic is unit-tested. See its
                        // doc-comment for the full list of conditions.
                        let (
                            has_tracking,
                            filter_allows_noop,
                            flash_remaining_zero,
                            draw_procs_done,
                            rendered_pixels_final,
                        ) = self
                            .dispatcher
                            .dialog_tracking
                            .as_ref()
                            .map(|t| {
                                let idle_dialog_mouse =
                                    self.dispatcher.mouse_down_over_dialog_button()
                                        || self.dispatcher.mouse_down_over_dialog_plain_user_item()
                                        || (self.dispatcher.event_queue.is_empty()
                                            && self
                                                .dispatcher
                                                .pending_dialog_plain_user_item_mouse_down());
                                let paced_filter_idle = t.filter_proc != 0
                                    && t.last_filter_event.is_none()
                                    && !idle_dialog_mouse
                                    && self.dialog_filter_null_event_already_sent_this_tick(
                                        t.dialog_ptr,
                                    )
                                    && !self.dialog_filter_has_real_event_pending(t.dialog_ptr);
                                (
                                    true,
                                    t.filter_proc == 0 || paced_filter_idle,
                                    t.flash_remaining == 0,
                                    t.draw_procs_done,
                                    t.rendered_pixels_final,
                                )
                            })
                            .unwrap_or((false, false, false, false, false));
                        let noop_refire = modaldialog_refire_is_noop(
                            yield_for_ui,
                            has_tracking,
                            filter_allows_noop,
                            flash_remaining_zero,
                            draw_procs_done,
                            rendered_pixels_final,
                            self.dispatcher.event_queue.is_empty(),
                        );
                        if noop_refire {
                            // Batch additional virtual no-op refires
                            // without re-entering `cpu.step`. Each saved
                            // step avoids the PC save + register snapshot
                            // + opcode fetch + `dispatch_group_a` branch
                            // path.
                            let idx = (opcode & 0xFFF) as usize;
                            self.dispatcher.trap_count += 1;
                            self.dispatcher.current_trap_word = opcode;
                            self.dispatcher.trap_histogram[idx] =
                                self.dispatcher.trap_histogram[idx].saturating_add(1);
                            // Count inline-skipped entries separately
                            // from real dispatches so the trap-timing
                            // histogram can show per-real-dispatch ns.
                            self.dispatcher.inline_skipped[idx] =
                                self.dispatcher.inline_skipped[idx].saturating_add(1);
                            const BATCH: u32 = 64;
                            let mut budget = BATCH - 1;
                            while budget > 0 && count < max_steps && !tick_cap_reached {
                                tick_cap_reached = self.charge_tick_budget(1, tick_cap);
                                if tick_cap_reached {
                                    break;
                                }
                                count += 1;
                                self.total_instructions = self.total_instructions.wrapping_add(1);
                                self.dispatcher.trap_count += 1;
                                self.dispatcher.trap_histogram[idx] =
                                    self.dispatcher.trap_histogram[idx].saturating_add(1);
                                self.dispatcher.inline_skipped[idx] =
                                    self.dispatcher.inline_skipped[idx].saturating_add(1);
                                budget -= 1;
                            }
                            self.cpu.write_reg(Register::PC, pc);
                            continue;
                        }
                    }

                    // Pre-dispatch fast path for PtInRect ($A8AD).
                    // EV calls this millions of times while walking dialog
                    // controls. The handler is pure stack/Rect arithmetic, so
                    // inline the exact Pascal ABI and keep accounting aligned
                    // with TrapDispatcher::dispatch.
                    if opcode == 0xA8AD {
                        let sp = self.cpu.core.a(7);
                        let rect_ptr = self.bus.read_long(sp);
                        let pt_v = self.bus.read_word(sp + 4) as i16;
                        let pt_h = self.bus.read_word(sp + 6) as i16;
                        let top = self.bus.read_word(rect_ptr) as i16;
                        let left = self.bus.read_word(rect_ptr + 2) as i16;
                        let bottom = self.bus.read_word(rect_ptr + 4) as i16;
                        let right = self.bus.read_word(rect_ptr + 6) as i16;
                        let in_rect = pt_v >= top && pt_v < bottom && pt_h >= left && pt_h < right;
                        self.bus
                            .write_word(sp + 8, if in_rect { 0x0100 } else { 0 });
                        self.cpu.write_reg(Register::A7, sp + 8);

                        self.dispatcher.trap_count += 1;
                        self.dispatcher.current_trap_word = opcode;
                        if pc < 0x0080_0000
                            && !self.dispatcher.is_menu_tracking()
                            && !self.dispatcher.is_dialog_tracking()
                            && !self.dispatcher.is_control_tracking()
                        {
                            self.dispatcher.game_trap_count += 1;
                        }
                        let idx = (opcode & 0xFFF) as usize;
                        self.dispatcher.trap_histogram[idx] =
                            self.dispatcher.trap_histogram[idx].saturating_add(1);
                        self.dispatcher.inline_skipped[idx] =
                            self.dispatcher.inline_skipped[idx].saturating_add(1);
                        if self.service_pending_launch_application(true) {
                            if self.halted {
                                return (count, false);
                            }
                            continue;
                        }
                        continue;
                    }

                    // Pre-dispatch fast path for EventAvail ($A971).
                    // Marathon polls this heavily while waiting at terminal
                    // panels. Reuse the dispatcher helpers so event filtering
                    // and EventRecord layout stay centralized.
                    if opcode == 0xA971 {
                        let sp = self.cpu.core.a(7);
                        let event_ptr = self.bus.read_long(sp);
                        let event_mask = self.bus.read_word(sp + 4);

                        if let Some(ev) = self.dispatcher.peek_toolbox_event(&self.bus, event_mask)
                        {
                            self.dispatcher.write_event_record(
                                &mut self.bus,
                                event_ptr,
                                ev.what,
                                ev.message,
                                ev.where_v,
                                ev.where_h,
                                ev.modifiers,
                            );
                            self.bus.write_word(sp + 6, 0xFFFF);
                        } else {
                            self.dispatcher.write_event_record(
                                &mut self.bus,
                                event_ptr,
                                0,
                                0,
                                self.dispatcher.mouse_pos.0,
                                self.dispatcher.mouse_pos.1,
                                self.dispatcher.current_event_modifiers(),
                            );
                            self.bus.write_word(sp + 6, 0);
                        }
                        self.cpu.write_reg(Register::A7, sp + 6);

                        self.dispatcher.trap_count += 1;
                        self.dispatcher.current_trap_word = opcode;
                        let idx = (opcode & 0xFFF) as usize;
                        self.dispatcher.trap_histogram[idx] =
                            self.dispatcher.trap_histogram[idx].saturating_add(1);
                        self.dispatcher.inline_skipped[idx] =
                            self.dispatcher.inline_skipped[idx].saturating_add(1);
                        continue;
                    }

                    self.dispatcher.yield_for_ui = yield_for_ui;
                    match self
                        .dispatcher
                        .dispatch(opcode, &mut self.cpu, &mut self.bus)
                    {
                        Ok(()) => {
                            let extra_tick_cost = hle_trap_extra_tick_cost(opcode)
                                .saturating_add(self.dispatcher.take_hle_tick_cost());
                            if extra_tick_cost > 0
                                && self.charge_tick_budget(extra_tick_cost, tick_cap)
                            {
                                tick_cap_reached = true;
                            }
                            // The m68k CPU already advanced PC past the A-line
                            // instruction during fetch (read_imm_16 does pc += 2).
                            //
                            // When menu or dialog tracking is active, REWIND PC
                            // back to the A-line instruction so it re-fires on
                            // the next frame.
                            //
                            // Shared check with `dispatch.rs`'s auto-pop
                            // push-back logic — both call
                            // `TrapDispatcher::is_tracking_refire` so
                            // they can never diverge. Strips auto-pop
                            // bit so `$AD3D` / `$AC0B` / `$AD91`
                            // match too.
                            let is_tracking_refire = self.dispatcher.is_tracking_refire(opcode);
                            if is_tracking_refire {
                                // In GUI mode, freeze ticks so the game clock doesn't
                                // advance while the host renders intermediate frames.
                                // In headless mode (scripted harnesses), let the budget
                                // advance ticks naturally — frozen_ticks would snap
                                // $016A on each re-fire, which consumes ticks at a
                                // different rate than real hardware where ModalDialog's
                                // WNE loop paces against the VBL.
                                if yield_for_ui
                                    && self.frozen_ticks.is_none()
                                    && tracking_refire_should_freeze_ticks(opcode)
                                {
                                    self.frozen_ticks = Some(self.bus.read_long(0x016A));
                                }
                                if yield_for_ui
                                    && self.frozen_ticks.is_some()
                                    && !tracking_refire_should_freeze_ticks(opcode)
                                {
                                    self.unfreeze_ticks_to(real_tick_cap);
                                }
                                self.cpu.write_reg(Register::PC, pc);

                                // Fire MenuSelect's documented MenuHook while
                                // the dropdown is still live on screen. The
                                // hook is guest code, so inject it before the
                                // next A93D re-fire instead of approximating it
                                // inside the HLE trap body.
                                let fired_menu_hook = self.fire_menu_hook_proc(opcode);

                                // Fire pending dialog userItem draw procs.
                                // The trampoline redirects PC to execute the
                                // 68K draw proc; when it RTS's, PC returns to
                                // the ModalDialog A-line for the next re-fire.
                                let fired_draw_proc = if fired_menu_hook {
                                    false
                                } else {
                                    self.fire_dialog_draw_procs()
                                };
                                let mut fired_filter_proc = false;
                                if !fired_menu_hook && !fired_draw_proc {
                                    // Fire the filter proc for any dialog that has one,
                                    // once draw procs are complete. On a real Mac,
                                    // ModalDialog calls the filter for every event
                                    // (including null events) regardless of item types.
                                    // Inside Macintosh Volume I, I-415
                                    if self.should_fire_dialog_filter_proc() {
                                        self.fire_dialog_filter_proc();
                                        fired_filter_proc = true;
                                    }
                                }

                                // In realtime frontends, yield only once the
                                // tracking trap is idle. A just-scheduled
                                // dialog draw/filter proc has not run yet, so
                                // presenting here shows half-painted screens.
                                // Headless mode keeps executing as before.
                                if yield_for_ui
                                    && !fired_menu_hook
                                    && !fired_draw_proc
                                    && !fired_filter_proc
                                {
                                    if (opcode & !0x0400) == 0xA991
                                        && !self.service_gui_modal_dialog_idle_tick(tick_cap)
                                    {
                                        continue;
                                    }
                                    if finish_frame {
                                        self.finish_host_frame(
                                            audio_samples,
                                            sound_interrupt_dispatched,
                                        );
                                    }
                                    return (count, true);
                                }
                            }
                            // If tracking just ended this trap (MenuSelect or
                            // ModalDialog completed), unfreeze ticks and snap
                            // $016A to wall-clock time so the game doesn't
                            // fast-forward through the pause gap.
                            if self.frozen_ticks.is_some() {
                                self.unfreeze_ticks_to(real_tick_cap);
                            }

                            if !is_tracking_refire && self.fire_modeless_dialog_draw_proc() {
                                continue;
                            }

                            // Service any pending Delay ticks immediately after
                            // the trap dispatch, before the next instruction.
                            self.service_delay_ticks(tick_cap);
                            if self.service_pending_launch_application(event_manager_yield_trap(
                                opcode,
                            )) {
                                if self.halted {
                                    return (count, false);
                                }
                                continue;
                            }
                        }
                        Err(Error::Halted) => {
                            // Surface the auto-pop caller PC if the
                            // halted trap was called via JSR through a
                            // trampoline. Without this, the halt log
                            // only shows the trampoline PC; the actual
                            // game-side caller is what investigators
                            // want to disassemble.
                            let caller_str = self
                                .dispatcher
                                .current_trap_caller
                                .map(|c| format!(" caller=${:08X}", c))
                                .unwrap_or_default();
                            eprintln!(
                                "[RUN_STEPS] Application halted at count={} pc=${:08X} trap=${:04X}{}",
                                count,
                                pc,
                                opcode,
                                caller_str,
                            );
                            self.halted = true;
                            self.halted_pc = Some(pc);
                            self.halted_trap = Some(opcode);
                            self.halted_sp = Some(self.cpu.read_reg(Register::A7));
                            self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                            self.dump_trace();
                            return (count, false);
                        }
                        Err(Error::UnimplementedTrap(t)) => {
                            eprintln!("[RUN_STEPS] Unimplemented trap ${:04X} — skipping", t);
                            self.cpu.write_reg(Register::PC, pc + 2);
                        }
                        Err(e) => {
                            eprintln!(
                                "[RUN_STEPS] Error {:?} at PC=${:08X} trap=${:04X} count={}",
                                e, pc, opcode, count
                            );
                            self.halted = true;
                            self.halted_pc = Some(pc);
                            self.halted_sp = Some(self.cpu.read_reg(Register::A7));
                            self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
                            self.dump_trace();
                            return (count, false);
                        }
                    }
                }
            }

            self.dispatcher.instruction_count = self.total_instructions;
        }

        if finish_frame {
            self.finish_host_frame(audio_samples, sound_interrupt_dispatched || sound_work_only);
        }

        (count, !self.halted)
    }

    /// Run for a specific number of steps and mix the supplied amount of host audio.
    /// Returns the number of instructions executed and whether the CPU is still running.
    ///
    /// `tick_override`: If `Some(ticks)`, `Ticks` is capped to the supplied external
    /// wall-clock target. If `None`, `Ticks` advances from the runner's configured
    /// instruction cadence.
    pub fn run_steps_with_audio(
        &mut self,
        max_steps: usize,
        tick_override: Option<u32>,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            tick_override,
            audio_samples,
            tick_override.is_some(),
            false,
            true,
        )
    }

    /// Run a realtime GUI/WASM slice using the runner's internal tick cadence.
    /// The caller is responsible for converting wall-clock time into `max_steps`
    /// and `audio_samples`.
    pub fn run_realtime_steps_with_audio(
        &mut self,
        max_steps: usize,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(max_steps, None, audio_samples, true, false, true)
    }

    /// Run a GUI frame slice paced by wall-clock time.
    ///
    /// Wall-clock GUI pacing works differently from the reference runtime:
    /// in the reference runtime, ticks are driven purely by the instruction budget
    /// (deterministic, host-speed-independent). In the GUI, the user expects
    /// the game to run at real time regardless of how fast the emulator can
    /// execute instructions, so the caller computes a `deadline_tick` from
    /// host wall-clock time and we cap `$016A` advancement there. The CPU
    /// runs flat out (up to `max_steps`) until either the tick cap is hit
    /// or the instruction budget is exhausted, at which point the caller
    /// yields to the UI thread for rendering.
    pub fn run_gui_slice_with_audio(
        &mut self,
        max_steps: usize,
        deadline_tick: u32,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            Some(deadline_tick),
            audio_samples,
            true,
            false,
            true,
        )
    }

    /// Run a GUI CPU slice paced by wall-clock time without finalizing a host
    /// frame. Browser frontends use this to execute several small CPU batches
    /// and then redraw chrome / mix queued audio once for the outer frame.
    pub fn run_gui_cpu_slice(&mut self, max_steps: usize, deadline_tick: u32) -> (usize, bool) {
        self.run_steps_internal(max_steps, Some(deadline_tick), 0, true, false, false)
    }

    /// Run pending Sound Manager interrupt work without advancing TickCount
    /// or continuing into foreground guest code after the callback returns.
    pub fn run_pending_sound_work(&mut self, max_steps: usize) -> (usize, bool) {
        self.run_steps_internal(max_steps, None, 0, true, true, true)
    }

    /// Run for a specific number of steps (for GUI/headless callers that don't
    /// provide a real wall-clock audio budget).
    ///
    /// Returns `(steps_executed, still_running)` — note that the bool is
    /// **`still_running`**, not `halted`. `false` means the CPU halted
    /// (via `ExitToShell`, an unimplemented opcode, or a memory fault).
    /// The per-halt detail (trap word, PC, SP, D0) is exposed via the
    /// [`halted_trap`](Self::halted_trap), [`halted_pc`](Self::halted_pc),
    /// [`halted_sp`](Self::halted_sp), [`halted_d0`](Self::halted_d0)
    /// accessors after this call returns.
    pub fn run_steps(&mut self, max_steps: usize, tick_override: Option<u32>) -> (usize, bool) {
        self.run_steps_internal(max_steps, tick_override, 0, false, false, true)
    }

    /// Halt at the CPU's current position with the standard crash
    /// diagnostics (stop banner, fake-ptr hints, trace dump). Shared by
    /// the batch-exit arms (TRAP #n / BKPT / illegal instruction) that
    /// previously funneled through `StepResult::Stopped`.
    fn halt_with_stop_diagnostics(&mut self, count: usize) {
        let halted_pc = self.cpu.read_reg(Register::PC);
        eprintln!(
            "[RUN_STEPS] CPU stopped at count={} pc=${:08X} op=${:04X}",
            count,
            halted_pc,
            self.bus.read_word(halted_pc)
        );
        if let Some(hint) = decode_fakeptr_pc(halted_pc) {
            eprintln!("[RUN_STEPS]   {}", hint);
        } else if let Some((entry_pc, hint)) = self.trace_find_fakeptr_entry() {
            eprintln!(
                "[RUN_STEPS]   PC drifted ${:X} bytes from a fake-ptr entry at \
                 ${:08X}. {}",
                halted_pc.wrapping_sub(entry_pc),
                entry_pc,
                hint
            );
        }
        self.halted = true;
        self.halted_pc = Some(halted_pc);
        self.halted_sp = Some(self.cpu.read_reg(Register::A7));
        self.halted_d0 = Some(self.cpu.read_reg(Register::D0));
        self.dump_trace();
    }

    fn charge_tick_budget(&mut self, units: i32, tick_cap: Option<u32>) -> bool {
        if units <= 0 {
            return false;
        }
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        self.tick_budget -= units;
        while self.tick_budget <= 0 && self.frozen_ticks.is_none() {
            if let Some(cap) = tick_cap {
                if self.bus.read_long(0x016A) >= cap {
                    return true;
                }
            }
            self.advance_guest_tick();
            self.tick_budget += self.instructions_per_tick as i32;
            if self.active_interrupt_callback.is_some() {
                return false;
            }
            if let Some(cap) = tick_cap {
                if self.bus.read_long(0x016A) >= cap {
                    return true;
                }
            }
        }
        false
    }

    fn advance_guest_tick(&mut self) -> u32 {
        let new_tick = self.bus.read_long(0x016A).wrapping_add(1);
        self.bus.write_long(0x016A, new_tick);
        self.dispatcher.tick_count = new_tick;

        // Sync MBState ($0172) from the internal button state.
        // On real hardware the VBL interrupt handler reads the ADB mouse
        // state and writes $0172 at each retrace. In our HLE, the button
        // state and the event queue are updated together by push_mouse_down;
        // keep $0172 at "pressed" while either the button is physically
        // held OR an unconsumed mouseDown is still pending in the queue
        // WITHOUT a later mouseUp pairing it off. This ensures code that
        // polls $0172 directly (rather than calling GetNextEvent) can
        // detect clicks injected before polling started, while a
        // mouse_up queued behind the mouse_down still flips MBState back
        // to 0x80 even when no GetNextEvent ever drains the queue —
        // critical for polling-only games (Bonkheads-Deluxe class titles)
        // that would otherwise see the button as "held forever".
        let has_pending_unmatched_down = self.dispatcher.has_unmatched_queued_mouse_down();
        let pressed = self.dispatcher.mouse_button || has_pending_unmatched_down;
        let mb_state: u8 = if pressed { 0x00 } else { 0x80 };
        self.bus.write_byte(0x0172, mb_state);

        // Advance the real-time clock ($020C) once per second.
        // On a real Mac the IOP or VIA increments Time every second;
        // we approximate this by incrementing every 60 ticks (~1 s at
        // 60.15 Hz VBL). Games that read $020C directly (e.g. for
        // PRNG seeding or save-file timestamps) need a changing value.
        // Inside Macintosh Volume II, II-378
        if new_tick.is_multiple_of(60) {
            let time = self.bus.read_long(0x020C);
            self.bus.write_long(0x020C, time.wrapping_add(1));
        }

        // Fire the cursor task and vertical-retrace tasks before Time Manager
        // tasks. Games commonly drive screen/audio housekeeping from VBL, so
        // letting those callbacks run first avoids starving them behind
        // unrelated timer traffic.
        self.fire_cursor_task();
        self.fire_vbl_tasks();
        self.fire_timer_tasks(new_tick);
        new_tick
    }

    fn deliver_pending_wait_next_event_if_available(&mut self) -> bool {
        let Some(pending) = self.dispatcher.pending_wait_next_event_return.take() else {
            if !self.dispatcher.event_queue.is_empty() {
                self.dispatcher.pending_wait_sleep_ticks = 0;
                return true;
            }
            return false;
        };

        if let (Some(resume_pc), Some(resume_sp)) = (pending.resume_pc, pending.resume_sp) {
            let current_pc = self.cpu.read_reg(Register::PC);
            let current_sp = self.cpu.read_reg(Register::A7);
            if current_pc != resume_pc || current_sp != resume_sp {
                self.dispatcher.pending_wait_sleep_ticks = 0;
                if crate::trap::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] dropping stale WaitNextEvent sleep return parked pc=${:08X} sp=${:08X}; current pc=${:08X} sp=${:08X}",
                        resume_pc, resume_sp, current_pc, current_sp
                    );
                }
                return false;
            }
        }

        let (mut what, mut message, mut where_v, mut where_h, mut modifiers, mut has_event) = self
            .dispatcher
            .dequeue_toolbox_event(&mut self.cpu, &mut self.bus, pending.event_mask);
        if !has_event {
            if let Some(event) = self.dispatcher.mouse_moved_event_for_region(
                &self.bus,
                pending.event_mask,
                pending.mouse_rgn,
            ) {
                what = event.what;
                message = event.message;
                where_v = event.where_v;
                where_h = event.where_h;
                modifiers = event.modifiers;
                has_event = true;
                self.dispatcher.debug_mouse_moved_event_count = self
                    .dispatcher
                    .debug_mouse_moved_event_count
                    .saturating_add(1);
            }
        }
        if !has_event {
            self.dispatcher.pending_wait_next_event_return = Some(pending);
            return false;
        }

        self.dispatcher.write_event_record(
            &mut self.bus,
            pending.event_ptr,
            what,
            message,
            where_v,
            where_h,
            modifiers,
        );
        self.bus.write_word(pending.result_ptr, 0xFFFF);
        self.dispatcher.pending_wait_sleep_ticks = 0;
        if crate::trap::dispatch::trace_input_enabled() {
            eprintln!(
                "[INPUT] WaitNextEvent sleep woke with input event what={} message=${:08X}",
                what, message
            );
        }
        true
    }

    fn wake_pending_wait_next_event_if_input_available(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }
        if self.dispatcher.pending_wait_sleep_ticks == 0
            || self.dispatcher.pending_wait_next_event_return.is_none()
        {
            return false;
        }
        // Event Manager sleep is interrupted as soon as a matching input event
        // is available; callers injecting input between run slices should not
        // have to wait for the next foreground CPU step to observe the wake.
        // Macintosh Toolbox Essentials 1992, p. 2-22.
        self.deliver_pending_wait_next_event_if_available()
    }

    fn wake_pending_wait_next_event_with_null_event_for_polling_input(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }
        if self.dispatcher.pending_wait_sleep_ticks == 0 {
            return false;
        }
        let Some(pending) = self.dispatcher.pending_wait_next_event_return.take() else {
            return false;
        };

        if let (Some(resume_pc), Some(resume_sp)) = (pending.resume_pc, pending.resume_sp) {
            let current_pc = self.cpu.read_reg(Register::PC);
            let current_sp = self.cpu.read_reg(Register::A7);
            if current_pc != resume_pc || current_sp != resume_sp {
                self.dispatcher.pending_wait_sleep_ticks = 0;
                if crate::trap::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] dropping stale WaitNextEvent polling wake parked pc=${:08X} sp=${:08X}; current pc=${:08X} sp=${:08X}",
                        resume_pc, resume_sp, current_pc, current_sp
                    );
                }
                return false;
            }
        }

        let (where_v, where_h) = self.dispatcher.mouse_position();
        let modifiers = self.dispatcher.current_event_modifiers();
        self.dispatcher.write_event_record(
            &mut self.bus,
            pending.event_ptr,
            0,
            0,
            where_v,
            where_h,
            modifiers,
        );
        self.bus.write_word(pending.result_ptr, 0);
        self.dispatcher.pending_wait_sleep_ticks = 0;
        if crate::trap::dispatch::trace_input_enabled() {
            eprintln!("[INPUT] WaitNextEvent sleep woke with null event for polling input");
        }
        true
    }

    fn wake_foreground_after_input(&mut self) {
        if self.tick_budget <= 0 {
            self.refill_foreground_budget_after_async_return();
        }
    }

    fn refill_foreground_budget_after_async_return(&mut self) {
        if self.tick_budget <= 0 {
            self.tick_budget = self.instructions_per_tick.max(2) as i32;
        }
    }

    fn service_wait_sleep_ticks(&mut self, tick_cap: Option<u32>) -> bool {
        if self.dispatcher.pending_wait_sleep_ticks == 0 || self.active_interrupt_callback.is_some()
        {
            return false;
        }

        if self.frozen_ticks.is_some() {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // On a real Mac, WaitNextEvent returns immediately when an event
        // is available, regardless of the requested sleep duration.
        // Macintosh Toolbox Essentials 1992, 2-22
        if self.deliver_pending_wait_next_event_if_available() {
            return false;
        }

        // If a dialog is being handled by ModalDialog, or a ModalDialog-owned
        // dialog is visibly retained between ModalDialog calls, treat
        // WaitNextEvent sleep as an app-yield hint rather than a wall-clock
        // delay. App-owned visible dialogs created with GetNewDialog can run
        // their own WaitNextEvent loops before ever entering ModalDialog; those
        // must still honor the requested sleep interval.
        let retained_modal_dialog_snapshot = self
            .dispatcher
            .dialog_visible_snapshots
            .keys()
            .any(|dialog_ptr| self.dispatcher.dialog_modal_entered.contains(dialog_ptr));
        let app_owned_visible_dialog_snapshot = self
            .dispatcher
            .dialog_visible_snapshots
            .keys()
            .any(|dialog_ptr| !self.dispatcher.dialog_modal_entered.contains(dialog_ptr));
        if tick_cap.is_some()
            && (self.dispatcher.is_dialog_tracking() || retained_modal_dialog_snapshot)
        {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // In GUI mode (tick_cap present), suspend foreground guest code until
        // either the requested WaitNextEvent sleep expires or this host frame's
        // tick cap is reached. The Process Manager makes the process eligible
        // to run again only after an event arrives or the sleep time expires;
        // if the time expires with no event pending, the app receives a null
        // event. Inside Macintosh: Processes 1994, p. 2-8.
        if let Some(cap) = tick_cap {
            while self.dispatcher.pending_wait_sleep_ticks > 0 && self.bus.read_long(0x016A) < cap {
                self.dispatcher.pending_wait_sleep_ticks -= 1;
                self.advance_guest_tick();
                self.tick_budget = self.instructions_per_tick as i32;
                if self.active_interrupt_callback.is_some() {
                    break;
                }
            }

            // Yield to the host if the frame tick cap was reached while the
            // process is still suspended; the next frame will continue draining
            // the remaining sleep without delivering another null event early.
            if self.dispatcher.pending_wait_sleep_ticks > 0 && self.bus.read_long(0x016A) >= cap {
                return true;
            }
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // Headless mode (no tick_cap from caller).
        //
        // Default: drain all pending sleep ticks at once (faster wall-clock).
        // Opt-in cap (set via `FixtureRunner::set_wait_sleep_cap_in_headless`):
        // honor the cap as a per-WNE-call ceiling, mirroring GUI mode's
        // 1-tick cap. Used by scripted harnesses to prevent Systemless's tick rate
        // from rocketing ahead of Basilisk's during event-loop-heavy
        // gameplay. App-owned visible dialogs keep the real sleep even when a
        // script sets the cap to zero; otherwise headless probes can run modal
        // background work that Basilisk is still sleeping through.
        if let Some(cap) = self.wait_sleep_cap_in_headless {
            let advance = if app_owned_visible_dialog_snapshot {
                self.dispatcher.pending_wait_sleep_ticks
            } else {
                self.dispatcher.pending_wait_sleep_ticks.min(cap)
            };
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            for _ in 0..advance {
                self.advance_guest_tick();
                self.tick_budget = self.instructions_per_tick as i32;
                if self.active_interrupt_callback.is_some() {
                    break;
                }
            }
            return false;
        }

        while self.dispatcher.pending_wait_sleep_ticks > 0 {
            self.dispatcher.pending_wait_sleep_ticks -= 1;
            self.advance_guest_tick();
            self.tick_budget = self.instructions_per_tick as i32;

            if self.active_interrupt_callback.is_some() {
                break;
            }
        }
        self.dispatcher.pending_wait_next_event_return = None;
        false
    }

    fn service_delay_ticks(&mut self, tick_cap: Option<u32>) -> bool {
        if self.dispatcher.pending_delay_ticks == 0 || self.active_interrupt_callback.is_some() {
            return false;
        }

        if self.frozen_ticks.is_some() {
            self.dispatcher.pending_delay_ticks = 0;
            return false;
        }

        // Drain delay ticks one at a time, firing VBL/timer callbacks each tick.
        // In GUI mode with a tick_cap, yield if we reach the cap.
        while self.dispatcher.pending_delay_ticks > 0 {
            if let Some(cap) = tick_cap {
                if self.bus.read_long(0x016A) >= cap {
                    return true;
                }
            }
            self.dispatcher.pending_delay_ticks -= 1;
            self.advance_guest_tick();
            self.tick_budget = self.instructions_per_tick as i32;
            if self.active_interrupt_callback.is_some() {
                break;
            }
        }

        if self.dispatcher.pending_delay_ticks == 0 {
            let final_ticks = self.bus.read_long(0x016A);
            self.cpu.write_reg(Register::D0, final_ticks);
        }

        false
    }

    fn service_gui_modal_dialog_idle_tick(&mut self, tick_cap: Option<u32>) -> bool {
        if self.active_interrupt_callback.is_some() || self.frozen_ticks.is_some() {
            return true;
        }

        if let Some(cap) = tick_cap {
            if self.bus.read_long(0x016A) >= cap {
                return true;
            }
        }

        self.advance_guest_tick();
        self.tick_budget = self.instructions_per_tick as i32;

        tick_cap
            .map(|cap| self.bus.read_long(0x016A) >= cap)
            .unwrap_or(true)
    }

    fn unfreeze_ticks_to(&mut self, target_tick: Option<u32>) {
        self.frozen_ticks = None;
        if let Some(target_tick) = target_tick {
            self.bus.write_long(0x016A, target_tick);
            // Keep `dispatcher.tick_count` in sync with $016A.
            // `advance_guest_tick` does this during ordinary advancement.
            self.dispatcher.tick_count = target_tick;
        }
    }

    /// Fire the low-memory cursor task vector, if an app has installed one.
    ///
    /// JCrsrTask runs from interrupt-time cursor/VBL maintenance. MPW
    /// Interfaces/AIncludes/LowMemEqu.a names the ProcPtr at $08EE.
    fn fire_cursor_task(&mut self) {
        if self.active_interrupt_callback.is_some() {
            return;
        }
        if (self.cpu.core.get_sr() & 0x0700) >= 0x0100 {
            if trace_vbl_enabled() {
                eprintln!(
                    "[VBL] defer JCrsrTask masked sr=${:04X} pc=${:08X}",
                    self.cpu.core.get_sr(),
                    self.cpu.read_reg(Register::PC)
                );
            }
            return;
        }

        let callback_addr = self
            .bus
            .read_long(crate::memory::globals::addr::J_CRSR_TASK);
        if callback_addr == 0 || callback_addr == CURSOR_TASK_NOOP_ADDR {
            return;
        }

        if self.cursor_task_trampoline == 0 {
            // JCrsrTask is a no-argument ProcPtr. Invoke it from interrupt
            // context and preserve the same volatile register set as VBL and
            // Time Manager callbacks. MPW Interfaces/AIncludes/LowMemEqu.a:
            // `JCrsrTask EQU $8EE`.
            let tramp = self.bus.alloc(16);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0);
            self.bus.write_word(tramp + 4, 0x4EB9); // JSR abs.L
                                                    // +6..+9: callback_addr (patched per-fire)
            self.bus.write_word(tramp + 10, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
            self.bus.write_word(tramp + 12, 0x0F0F);
            self.bus.write_word(tramp + 14, 0x4E75); // RTS
            self.cursor_task_trampoline = tramp;
        }

        let tramp = self.cursor_task_trampoline;
        self.bus.write_long(tramp + 6, callback_addr);
        if trace_vbl_enabled() {
            eprintln!(
                "[VBL] fire JCrsrTask addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X}",
                callback_addr,
                self.cpu.read_reg(Register::PC),
                self.cpu.read_reg(Register::A7)
            );
        }
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::CursorTask, tramp);
    }

    /// Fire the next due Vertical Retrace Manager task.
    ///
    /// VBL tasks run at interrupt time with A0 pointing at the task record.
    /// Processes 1994, 4-6 to 4-7; executor src/time/vbl.cpp
    fn fire_vbl_tasks(&mut self) {
        if self.active_interrupt_callback.is_some() {
            return;
        }
        if (self.cpu.core.get_sr() & 0x0700) >= 0x0100 {
            if trace_vbl_enabled() {
                eprintln!(
                    "[VBL] defer masked sr=${:04X} pc=${:08X}",
                    self.cpu.core.get_sr(),
                    self.cpu.read_reg(Register::PC)
                );
            }
            return;
        }

        let mut due_task = None;
        for task in &self.dispatcher.vbl_tasks {
            let count = self.bus.read_word(task.task_ptr + 10) as i16;
            if count <= 0 {
                continue;
            }
            let new_count = count - 1;
            self.bus.write_word(task.task_ptr + 10, new_count as u16);
            if new_count == 0 {
                due_task = Some(task.task_ptr);
                break;
            }
        }

        let Some(task_ptr) = due_task else {
            return;
        };

        let callback_addr = self.bus.read_long(task_ptr + 6);
        if callback_addr == 0 {
            return;
        }

        if self.vbl_trampoline == 0 {
            let tramp = self.bus.alloc_synthetic(22);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0);
            self.bus.write_word(tramp + 4, 0x207C); // MOVEA.L #imm,A0
            self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
            self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
            self.bus.write_word(tramp + 18, 0x0F0F);
            self.bus.write_word(tramp + 20, 0x4E75); // RTS
            self.vbl_trampoline = tramp;
        }

        let tramp = self.vbl_trampoline;
        self.bus.write_long(tramp + 6, task_ptr);
        self.bus.write_long(tramp + 12, callback_addr);

        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.cpu.core.get_ccr();
        let sr = self.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        let source = ActiveInterruptCallbackSource::Vbl;
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.cpu
            .core
            .set_sr_noint_nosp(interrupt_callback_sr(source, sr));
        self.cpu.write_reg(Register::PC, tramp);

        if trace_vbl_enabled() {
            eprintln!(
                "[VBL] fire task=${:08X} addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X} count={}",
                task_ptr,
                callback_addr,
                current_pc,
                sp,
                self.bus.read_word(task_ptr + 10) as i16
            );
        }
    }

    /// Fire any expired Time Manager tasks by injecting a call to their callback.
    ///
    /// On a real Mac, timer callbacks execute at interrupt time — the 68K hardware
    /// saves the entire CPU state (SR + PC + all registers via the exception frame)
    /// before dispatching the interrupt handler. The callback may freely clobber
    /// A0-A3 and D0-D3 (Processes 1994, 3-22).
    ///
    /// We simulate this by writing a small native 68K trampoline at a fixed
    /// low-memory address ($0110) that:
    ///   1. Saves D0-D3/A0-A3 via MOVEM.L to the stack
    ///   2. Loads A1 with the task record pointer (from inline data)
    ///   3. JSR's to the callback address (from inline data)
    ///   4. Restores D0-D3/A0-A3 via MOVEM.L from the stack
    ///   5. RTS back to the interrupted code
    fn fire_timer_tasks(&mut self, current_tick: u32) {
        if self.active_interrupt_callback.is_some() {
            return;
        }

        // Collect tasks that need to fire (avoid borrow issues)
        let mut to_fire: Vec<(u32, u32)> = Vec::new(); // (task_ptr, tm_addr)
        for task in &mut self.dispatcher.timer_tasks {
            if task.active && current_tick >= task.fire_at_tick {
                to_fire.push((task.task_ptr, task.tm_addr));
                task.active = false; // Mark as fired; callback may re-prime
            }
        }

        // Fire at most one task per tick to avoid deep nesting
        if let Some((task_ptr, tm_addr)) = to_fire.into_iter().next() {
            if tm_addr == 0 {
                return;
            }

            // Allocate trampoline code in guest heap on first use.
            // Layout (22 bytes):
            //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)  ; 48E7 F0F0
            //   +4:  MOVEA.L #task_ptr,A1         ; 227C xxxx xxxx
            //   +10: JSR     tm_addr              ; 4EB9 xxxx xxxx
            //   +16: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F
            //   +20: RTS                          ; 4E75
            if self.timer_trampoline == 0 {
                let tramp = self.bus.alloc(24); // 22 bytes + 2 padding
                self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                self.bus.write_word(tramp + 4, 0x227C); // MOVEA.L #imm32,A1
                                                        // +6..+9: task_ptr (patched per-fire)
                self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
                                                         // +12..+15: tm_addr (patched per-fire)
                self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,regs
                self.bus.write_word(tramp + 18, 0x0F0F); // D0-D3/A0-A3
                self.bus.write_word(tramp + 20, 0x4E75); // RTS
                self.timer_trampoline = tramp;
            }

            // Patch the inline data for this specific fire
            let tramp = self.timer_trampoline;
            self.bus.write_long(tramp + 6, task_ptr);
            self.bus.write_long(tramp + 12, tm_addr);

            // Snapshot the interrupted CPU state before mutating A7 for the
            // synthetic return address. The Time Manager callback should resume
            // with the guest stack exactly as it was when interrupted.
            let current_pc = self.cpu.read_reg(Register::PC);
            let sp = self.cpu.read_reg(Register::A7);
            let d_regs = [
                self.cpu.read_reg(Register::D0),
                self.cpu.read_reg(Register::D1),
                self.cpu.read_reg(Register::D2),
                self.cpu.read_reg(Register::D3),
                self.cpu.read_reg(Register::D4),
                self.cpu.read_reg(Register::D5),
                self.cpu.read_reg(Register::D6),
                self.cpu.read_reg(Register::D7),
            ];
            let a_regs = [
                self.cpu.read_reg(Register::A0),
                self.cpu.read_reg(Register::A1),
                self.cpu.read_reg(Register::A2),
                self.cpu.read_reg(Register::A3),
                self.cpu.read_reg(Register::A4),
                self.cpu.read_reg(Register::A5),
                self.cpu.read_reg(Register::A6),
                sp,
            ];
            let ccr = self.cpu.core.get_ccr();
            let sr = self.cpu.core.get_sr();

            // Inject: push current PC, jump to trampoline
            let new_sp = sp.wrapping_sub(4);
            self.bus.write_long(new_sp, current_pc);
            self.cpu.write_reg(Register::A7, new_sp);
            self.active_interrupt_callback = Some(ActiveInterruptCallback {
                source: ActiveInterruptCallbackSource::Timer,
                resume_pc: current_pc,
                resume_sp: sp,
                d_regs,
                a_regs,
                sr,
                ccr,
                restore_port: None,
            });
            if trace_timer_enabled() {
                eprintln!(
                    "[TIMER] fire task=${:08X} tm_addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X} ccr=${:02X}",
                    task_ptr, tm_addr, current_pc, sp, ccr
                );
            }
            self.cpu.write_reg(Register::PC, tramp);
        }
    }

    /// Check all channels with active double-buffers: if a channel is not
    /// currently playing but its current_buffer is ready in guest memory,
    /// load the samples so mix_frame() can produce audio.
    fn try_load_pending_double_buffers(&mut self) {
        if self.active_interrupt_callback.is_some() {
            return;
        }

        let queued_doublebacks = self
            .dispatcher
            .sound_manager
            .pending_callbacks
            .iter()
            .map(|cb| (cb.chan_ptr, cb.exhausted_buffer_index))
            .collect::<Vec<_>>();

        for chan in &mut self.dispatcher.sound_manager.channels {
            if chan.is_playing() {
                continue; // already has data
            }
            let (header_ptr, buf_idx, sample_rate, num_channels, sample_size) =
                match chan.double_buffer {
                    Some(ref db) if !db.last_buffer_seen => (
                        db.header_ptr,
                        db.current_buffer,
                        db.sample_rate,
                        db.num_channels,
                        db.sample_size,
                    ),
                    _ => continue,
                };
            let mut load_idx = buf_idx;
            let mut buf_ptr = self.bus.read_long(header_ptr + 12 + (buf_idx as u32) * 4);
            let mut can_load = buf_ptr != 0
                && self.bus.read_long(buf_ptr + 4) & 0x01 != 0
                && !queued_doublebacks
                    .iter()
                    .any(|&(pending_chan, pending_idx)| {
                        pending_chan == chan.guest_ptr && pending_idx == buf_idx
                    });
            let original_idx = load_idx;
            if !can_load {
                let other_idx = buf_idx ^ 1;
                let other_ptr = self.bus.read_long(header_ptr + 12 + (other_idx as u32) * 4);
                if other_ptr == 0 {
                    continue;
                }
                let other_flags = self.bus.read_long(other_ptr + 4);
                let other_pending =
                    queued_doublebacks
                        .iter()
                        .any(|&(pending_chan, pending_idx)| {
                            pending_chan == chan.guest_ptr && pending_idx == other_idx
                        });
                if other_flags & 0x01 == 0 || other_pending {
                    continue; // neither available buffer is ready yet
                }
                load_idx = other_idx;
                buf_ptr = other_ptr;
                can_load = true;
                if let Some(ref mut db) = chan.double_buffer {
                    db.current_buffer = other_idx;
                }
            }
            if !can_load {
                continue;
            }
            let flags = self.bus.read_long(buf_ptr + 4);
            if trace_sound_runner_enabled() {
                let preview = self
                    .bus
                    .read_bytes(buf_ptr + 16, 16)
                    .iter()
                    .map(|byte| format!("{:02X}", byte))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!(
                    "[SOUND-DB] load-ready chan=${:08X} header=${:08X} requested_idx={} load_idx={} buf=${:08X} frames={} flags=${:08X} pending={:?} first={}",
                    chan.guest_ptr,
                    header_ptr,
                    original_idx,
                    load_idx,
                    buf_ptr,
                    self.bus.read_long(buf_ptr),
                    flags,
                    chan.double_buffer
                        .as_ref()
                        .map(|db| db.pending_callback_buffers)
                        .unwrap_or([false; 2]),
                    preview
                );
            }
            crate::trap::TrapDispatcher::load_double_buffer_samples(
                &mut self.bus,
                chan,
                buf_ptr,
                sample_rate,
                num_channels,
                sample_size,
            );
            if flags & 0x01 != 0 {
                if let Some(ref mut db) = chan.double_buffer {
                    db.current_buffer = load_idx;
                    db.complete_callback_for(load_idx);
                }
            }
        }
    }

    fn dump_invalid_pc_state(&self) {
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            self.cpu.read_reg(Register::A7),
        ];
        eprintln!(
            "[RUN_STEPS]   D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
            d_regs[0], d_regs[1], d_regs[2], d_regs[3], d_regs[4], d_regs[5], d_regs[6], d_regs[7]
        );
        eprintln!(
            "[RUN_STEPS]   A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
            a_regs[0], a_regs[1], a_regs[2], a_regs[3], a_regs[4], a_regs[5], a_regs[6], a_regs[7]
        );
        eprintln!("[RUN_STEPS]   CCR=${:02X}", self.cpu.core.get_ccr());
        if let Some(active) = self.active_interrupt_callback {
            eprintln!(
                "[RUN_STEPS]   active_callback={:?} resume_pc=${:08X} resume_sp=${:08X}",
                active.source, active.resume_pc, active.resume_sp
            );
        }
        self.bus.dump_stack(a_regs[7], "invalid PC");
    }

    fn inject_interrupt_callback(
        &mut self,
        source: ActiveInterruptCallbackSource,
        trampoline: u32,
    ) {
        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.cpu.core.get_ccr();
        let sr = self.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.cpu
            .core
            .set_sr_noint_nosp(interrupt_callback_sr(source, sr));
        self.cpu.write_reg(Register::PC, trampoline);
    }

    /// Fire pending Sound Manager callback procedures and file completion routines.
    fn fire_sound_callbacks(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        if self
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .is_empty()
        {
            return false;
        }

        let cb = self
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .remove(0);
        match cb {
            crate::sound::PendingSoundCallback::Command {
                callback_addr,
                chan_ptr,
                cmd,
            } => {
                if callback_addr == 0 {
                    return false;
                }

                // Sound 1994, 2-152
                if self.sound_callback_trampoline == 0 {
                    // Sound callback:
                    //   PROCEDURE MyCallBack(chan: SndChannelPtr; cmd: SndCommand);
                    //
                    // In practice shipped apps commonly receive `cmd` as a
                    // pointer-sized argument and differ on how much stack they
                    // pop on return. Push cmdPtr nearest SP and chan beneath it,
                    // then reset SP to the saved-register frame after JSR so
                    // one-arg, two-arg, and C-style cleanup all resume safely.
                    let tramp = self.bus.alloc(42);
                    self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                    self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #chan,-(SP)
                    self.bus.write_word(tramp + 10, 0x2F3C); // MOVE.L #cmdPtr,-(SP)
                    self.bus.write_word(tramp + 16, 0x4EB9); // JSR abs.L
                    self.bus.write_word(tramp + 22, 0x2E7C); // MOVEA.L #savedSP,A7
                    self.bus.write_word(tramp + 28, 0x4CDF); // MOVEM.L (SP)+,regs
                    self.bus.write_word(tramp + 30, 0x0F0F); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 32, 0x4E75); // RTS
                    self.sound_callback_trampoline = tramp;
                }

                let tramp = self.sound_callback_trampoline;
                let cmd_ptr = tramp + 34;
                let interrupted_sp = self.cpu.read_reg(Register::A7);
                let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
                self.bus.write_long(tramp + 6, chan_ptr);
                self.bus.write_long(tramp + 12, cmd_ptr);
                self.bus.write_long(tramp + 18, callback_addr);
                self.bus.write_long(tramp + 24, saved_regs_sp);
                self.bus.write_word(cmd_ptr, cmd.cmd);
                self.bus.write_word(cmd_ptr + 2, cmd.param1 as u16);
                self.bus.write_long(cmd_ptr + 4, cmd.param2);
                self.inject_interrupt_callback(ActiveInterruptCallbackSource::SoundCallback, tramp);
                true
            }
            crate::sound::PendingSoundCallback::FileCompletion {
                callback_addr,
                chan_ptr,
            } => {
                if callback_addr == 0 {
                    return false;
                }

                // Sound 1994, 2-151
                if self.sound_file_completion_trampoline == 0 {
                    let tramp = self.bus.alloc(28);
                    self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                    self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #chan,-(SP)
                    self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
                    self.bus.write_word(tramp + 16, 0x2E7C); // MOVEA.L #savedSP,A7
                    self.bus.write_word(tramp + 22, 0x4CDF); // MOVEM.L (SP)+,regs
                    self.bus.write_word(tramp + 24, 0x0F0F); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 26, 0x4E75); // RTS
                    self.sound_file_completion_trampoline = tramp;
                }

                let tramp = self.sound_file_completion_trampoline;
                let interrupted_sp = self.cpu.read_reg(Register::A7);
                let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
                self.bus.write_long(tramp + 6, chan_ptr);
                self.bus.write_long(tramp + 12, callback_addr);
                self.bus.write_long(tramp + 18, saved_regs_sp);
                self.inject_interrupt_callback(
                    ActiveInterruptCallbackSource::SoundFileCompletion,
                    tramp,
                );
                true
            }
        }
    }

    /// Fire pending SndPlayDoubleBuffer doubleback callbacks.
    ///
    /// When mix_frame() exhausts a double buffer, it queues a callback request.
    /// Here we clear dbBufferReady on the exhausted buffer and inject a
    /// trampoline to call the game's doubleback proc to refill it.
    ///
    /// The doubleback procedure signature (Sound 1994, 2-146):
    ///   PROCEDURE MyDoubleBackProc(chan: SndChannelPtr;
    ///                              exhaustedBuffer: SndDoubleBufferPtr);
    fn fire_sound_doubleback_callbacks(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        if self.dispatcher.sound_manager.pending_callbacks.is_empty() {
            return false;
        }

        // Take one callback at a time (like timer tasks).
        let cb = self.dispatcher.sound_manager.pending_callbacks.remove(0);

        // Read the exhausted buffer pointer from the header.
        // dbhBufferPtr[0] at header+12, dbhBufferPtr[1] at header+16
        let exhausted_buf_ptr = self
            .bus
            .read_long(cb.header_ptr + 12 + (cb.exhausted_buffer_index as u32) * 4);

        // Clear dbBufferReady on the exhausted buffer.
        if exhausted_buf_ptr != 0 {
            let flags = self.bus.read_long(exhausted_buf_ptr + 4);
            if trace_sound_runner_enabled() {
                let preview = self
                    .bus
                    .read_bytes(exhausted_buf_ptr + 16, 16)
                    .iter()
                    .map(|byte| format!("{:02X}", byte))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!(
                    "[SOUND-DB] fire-doubleback tick={} chan=${:08X} header=${:08X} idx={} buf=${:08X} frames={} flags_before=${:08X} callback=${:08X} sr=${:04X} first={}",
                    self.bus.read_long(0x016A),
                    cb.chan_ptr,
                    cb.header_ptr,
                    cb.exhausted_buffer_index,
                    exhausted_buf_ptr,
                    self.bus.read_long(exhausted_buf_ptr),
                    flags,
                    cb.callback_addr,
                    self.cpu.core.get_sr(),
                    preview
                );
            }
            self.bus.write_long(exhausted_buf_ptr + 4, flags & !0x01);
        }

        if cb.callback_addr == 0 {
            return false;
        }

        // Allocate trampoline on first use.
        // The doubleback proc is a Pascal procedure (callee pops params):
        //   PROCEDURE MyDoubleBackProc(chan: SndChannelPtr;
        //                              exhaustedBuffer: SndDoubleBufferPtr);
        //
        // Trampoline layout (34 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)  ; 48E7 F0F0 (save regs)
        //   +4:  MOVE.L  #chanPtr,-(SP)       ; 2F3C xxxx xxxx (Pascal param1 = deeper)
        //   +10: MOVE.L  #exhaustedBuf,-(SP) ; 2F3C xxxx xxxx (Pascal param2 = top)
        //   +16: JSR     callback             ; 4EB9 xxxx xxxx
        //   +22: MOVEA.L #savedRegsSP,A7      ; ignore guest callback cleanup convention
        //   +28: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F (restore regs)
        //   +32: RTS                          ; 4E75
        if self.sound_doubleback_trampoline == 0 {
            let tramp = self.bus.alloc(34);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +6..+9: chan ptr (patched)
            self.bus.write_word(tramp + 10, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +12..+15: exhausted buf ptr (patched)
            self.bus.write_word(tramp + 16, 0x4EB9); // JSR abs.L
                                                     // +18..+21: callback addr (patched)
            self.bus.write_word(tramp + 22, 0x2E7C); // MOVEA.L #savedSP,A7
                                                     // +24..+27: saved regs SP (patched)
            self.bus.write_word(tramp + 28, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 30, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 32, 0x4E75); // RTS
            self.sound_doubleback_trampoline = tramp;
        }

        let tramp = self.sound_doubleback_trampoline;
        let interrupted_sp = self.cpu.read_reg(Register::A7);
        let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
        self.bus.write_long(tramp + 6, cb.chan_ptr);
        self.bus.write_long(tramp + 12, exhausted_buf_ptr);
        self.bus.write_long(tramp + 18, cb.callback_addr);
        self.bus.write_long(tramp + 24, saved_regs_sp);

        // Doubleback procedures execute at interrupt time, so the interrupted
        // guest CPU state must be restored after the callback unwinds.
        // Sound 1994, 2-72
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::SoundDoubleBack, tramp);
        true
    }

    fn dialog_callback_scratch_base(&self) -> u32 {
        DIALOG_CALLBACK_SCRATCH_FALLBACK
    }

    fn looks_like_dialog_proc_entry(&self, addr: u32) -> bool {
        if addr == 0 {
            return false;
        }
        let entry = self.bus.read_word(addr);
        entry == 0x4E56 || entry == 0x48E7 || entry == 0x4EF9 || entry == 0x4EFA
    }

    fn resolve_dialog_draw_proc_addr(&self, proc_addr: u32) -> Option<u32> {
        if self.looks_like_dialog_proc_entry(proc_addr) {
            return Some(proc_addr);
        }
        let a5_relative = self.cpu.read_reg(Register::A5).wrapping_add(proc_addr);
        if self.looks_like_dialog_proc_entry(a5_relative) {
            Some(a5_relative)
        } else {
            None
        }
    }

    fn inject_dialog_draw_proc(
        &mut self,
        proc_addr: u32,
        item_no: i16,
        dialog_ptr: u32,
        modeless: bool,
    ) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        if proc_addr == 0 {
            return false;
        }

        // Many dialogs stuff non-code placeholders into userItem proc fields.
        // Only fire callbacks that look like real 68K entry points.
        let Some(call_addr) = self.resolve_dialog_draw_proc_addr(proc_addr) else {
            if trace_dialog_procs_enabled() {
                let a5_relative = self.cpu.read_reg(Register::A5).wrapping_add(proc_addr);
                eprintln!(
                    "[DIALOG-PROC] skip dialog=${:08X} item={} proc=${:08X} a5rel=${:08X} entry=${:04X} a5entry=${:04X}",
                    dialog_ptr,
                    item_no,
                    proc_addr,
                    a5_relative,
                    self.bus.read_word(proc_addr),
                    self.bus.read_word(a5_relative),
                );
            }
            return false;
        };

        // Allocate trampoline on first use (32 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)   ; 48E7 F0F0
        //   +4:  MOVE.L  #dialogPtr,-(SP)      ; 2F3C xxxx xxxx
        //   +10: MOVE.W  #itemNo,-(SP)         ; 3F3C xxxx
        //   +14: JSR     proc_addr              ; 4EB9 xxxx xxxx
        //   +20: MOVEA.L #savedRegsSP,A7       ; 4FF9 xxxx xxxx
        //   +26: MOVEM.L (SP)+,D0-D3/A0-A3    ; 4CDF 0F0F
        //   +30: RTS                            ; 4E75
        if self.dialog_draw_trampoline == 0 {
            let tramp = self.dialog_callback_scratch_base() + DIALOG_DRAW_TRAMPOLINE_OFFSET;
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +6..+9: dialogPtr (patched per-fire)
            self.bus.write_word(tramp + 10, 0x3F3C); // MOVE.W #imm,-(SP)
                                                     // +12..+13: itemNo (patched per-fire)
            self.bus.write_word(tramp + 14, 0x4EB9); // JSR abs.L
                                                     // +16..+19: proc_addr (patched per-fire)
            self.bus.write_word(tramp + 20, 0x4FF9); // MOVEA.L #imm,A7
                                                     // +22..+25: savedRegsSP (patched per-fire)
            self.bus.write_word(tramp + 26, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 28, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 30, 0x4E75); // RTS
            self.dialog_draw_trampoline = tramp;
        }

        let tramp = self.dialog_draw_trampoline;
        self.bus.write_long(tramp + 6, dialog_ptr);
        self.bus.write_word(tramp + 12, item_no as u16);
        self.bus.write_long(tramp + 16, call_addr);

        // The Dialog Manager sets the current port to the dialog before
        // calling a userItem draw proc. It does not restore an older
        // application port over the callback's final QuickDraw state.
        self.dispatcher
            .set_current_port_state(&mut self.bus, &mut self.cpu, dialog_ptr, None);

        // Inject: push current PC, jump to trampoline
        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.cpu.core.get_ccr();
        let sr = self.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        let saved_regs_sp = new_sp.wrapping_sub(32);
        self.bus.write_long(tramp + 22, saved_regs_sp);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::DialogDrawProc,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        if modeless {
            self.dispatcher.active_modeless_dialog_draw_proc = Some(dialog_ptr);
        }
        if trace_dialog_procs_enabled() {
            eprintln!(
                "[DIALOG-PROC] fire {} dialog=${:08X} item={} proc=${:08X} call=${:08X} return_pc=${:08X}",
                if modeless { "modeless" } else { "modal" },
                dialog_ptr,
                item_no,
                proc_addr,
                call_addr,
                current_pc,
            );
        }
        self.cpu.write_reg(Register::PC, tramp);
        true
    }

    fn fire_modeless_dialog_draw_proc(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        while let Some((dialog_ptr, proc_addr, item_no)) =
            self.dispatcher.modeless_dialog_draw_proc_queue.pop_front()
        {
            if self.inject_dialog_draw_proc(proc_addr, item_no, dialog_ptr, true) {
                return true;
            }
        }
        false
    }

    /// Fire the next pending dialog userItem draw proc by injecting a trampoline.
    ///
    /// On a real Mac, ModalDialog calls each userItem's draw proc during
    /// the update pass. The draw proc is a Pascal callback:
    ///   PROCEDURE MyItem (theWindow: WindowPtr; itemNo: INTEGER);
    /// Inside Macintosh Volume I, I-405
    ///
    /// We simulate this by writing a small 68K trampoline that:
    ///   1. Saves D0-D3/A0-A3 via MOVEM.L to the stack
    ///   2. Pushes params so MPW-style Pascal prologues see itemNo at
    ///      8(A6) and theWindow at 10(A6), matching Pascal's stack layout
    ///   3. JSR to draw proc address
    ///   4. Resets A7 to the saved-register frame, tolerating callbacks
    ///      that return with either `RTD #6` or plain `RTS`
    ///   5. Restores D0-D3/A0-A3
    ///   6. RTS back to interrupted code (the ModalDialog A-line)
    fn fire_dialog_draw_procs(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        if let Some(tracking) = self
            .dispatcher
            .dialog_tracking
            .as_mut()
            .filter(|tracking| !tracking.draw_procs_done)
        {
            let Some((proc_addr, item_no)) = tracking.draw_proc_queue.pop_front() else {
                // All draw procs fired and returned
                tracking.draw_procs_done = true;
                return false;
            };
            let dialog_ptr = tracking.dialog_ptr;
            return self.inject_dialog_draw_proc(proc_addr, item_no, dialog_ptr, false);
        }

        self.fire_modeless_dialog_draw_proc()
    }

    fn fire_menu_hook_proc(&mut self, opcode: u16) -> bool {
        if self.active_interrupt_callback.is_some() || (opcode & !0x0400) != 0xA93D {
            return false;
        }
        if self.dispatcher.menu_tracking.is_none() || self.bus.read_byte(0x0172) != 0x00 {
            return false;
        }

        // MenuHook ($0A30)
        // Address of a no-argument routine that MenuSelect calls repeatedly
        // while the mouse button is down.
        // PROCEDURE MyMenuHook;
        // Inside Macintosh Volume I, I-356; Inside Macintosh Volume III, III-446
        let hook_addr = self.bus.read_long(0x0A30);
        let Some(call_addr) = self.resolve_dialog_draw_proc_addr(hook_addr) else {
            return false;
        };

        // Trampoline (22 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)   ; 48E7 F0F0
        //   +4:  JSR     hook_addr            ; 4EB9 xxxx xxxx
        //   +10: MOVEA.L #savedRegsSP,A7      ; 4FF9 xxxx xxxx
        //   +16: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F
        //   +20: RTS                          ; 4E75
        if self.menu_hook_trampoline == 0 {
            let tramp = self.dialog_callback_scratch_base() + MENU_HOOK_TRAMPOLINE_OFFSET;
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x4EB9); // JSR abs.L
                                                    // +6..+9: hook_addr
            self.bus.write_word(tramp + 10, 0x4FF9); // MOVEA.L #imm,A7
                                                     // +12..+15: savedRegsSP
            self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 18, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 20, 0x4E75); // RTS
            self.menu_hook_trampoline = tramp;
        }

        let tramp = self.menu_hook_trampoline;
        self.bus.write_long(tramp + 6, call_addr);

        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.cpu.core.get_ccr();
        let sr = self.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        let saved_regs_sp = new_sp.wrapping_sub(32);
        self.bus.write_long(tramp + 12, saved_regs_sp);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::MenuHook,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.cpu.write_reg(Register::PC, tramp);
        true
    }

    fn dialog_filter_has_real_event_pending(&self, dialog_ptr: u32) -> bool {
        self.dispatcher
            .event_queue
            .iter()
            .any(|event| matches!(event.what, 1 | 2 | 3 | 4 | 6))
            || self
                .dispatcher
                .pending_update_event(&self.bus, 1u16 << 6)
                .is_some_and(|event| {
                    event.message == dialog_ptr
                        && !self.dialog_filter_update_event_already_sent_this_tick(
                            dialog_ptr,
                            event.message,
                        )
                })
    }

    fn dialog_filter_null_event_already_sent_this_tick(&self, dialog_ptr: u32) -> bool {
        self.dialog_filter_last_null_event_tick
            .is_some_and(|(sent_dialog, sent_tick)| {
                sent_dialog == dialog_ptr && sent_tick == self.dispatcher.tick_count
            })
    }

    fn dialog_filter_update_event_already_sent_this_tick(
        &self,
        dialog_ptr: u32,
        update_window: u32,
    ) -> bool {
        self.dialog_filter_last_update_event_tick.is_some_and(
            |(sent_dialog, sent_window, sent_tick)| {
                sent_dialog == dialog_ptr
                    && sent_window == update_window
                    && sent_tick == self.dispatcher.tick_count
            },
        )
    }

    fn should_fire_dialog_filter_proc(&self) -> bool {
        let Some(tracking) = self.dispatcher.dialog_tracking.as_ref() else {
            return false;
        };

        if tracking.filter_proc == 0
            || !tracking.draw_procs_done
            || tracking.last_filter_event.is_some()
        {
            return false;
        }

        let dialog_ptr = tracking.dialog_ptr;
        let has_real_event = self.dialog_filter_has_real_event_pending(dialog_ptr);

        // A queued mouseDown on a standard dialog item is still a real event:
        // ModalDialog passes it to the filter first, then handles it itself if
        // the filter returns FALSE. Only suppress idle/null callbacks while
        // the mouse is physically held over a dialog item. IM:I 1985 I-415.
        if !has_real_event
            && (self.dispatcher.mouse_down_over_dialog_button()
                || self.dispatcher.mouse_down_over_dialog_plain_user_item()
                || self.dispatcher.pending_dialog_plain_user_item_mouse_down())
        {
            return false;
        }

        // ModalDialog gets events through GetNextEvent and passes them to the
        // filter proc. A null event means there was no real event to dequeue;
        // pace those synthetic idle callbacks to one per guest tick so the HLE
        // refire loop does not manufacture hundreds of thousands of no-input
        // filter calls between VBLs. Mouse/key/update events still bypass this
        // gate and are delivered immediately. IM:I 1985 I-415; MTE 1992 6-136.
        if !has_real_event && self.dialog_filter_null_event_already_sent_this_tick(dialog_ptr) {
            return false;
        }

        true
    }

    /// Fire the ModalDialog filter proc for game-managed dialogs.
    ///
    /// On a real Mac, ModalDialog's internal loop calls GetNextEvent (consuming
    /// the event) and then passes it to the filter proc. If the filter returns
    /// TRUE, ModalDialog returns immediately with the itemHit value the filter
    /// wrote. If FALSE, ModalDialog processes the event itself.
    /// Inside Macintosh Volume I, I-415
    ///
    /// We simulate this by:
    /// 1. Consuming the next actionable event from our queue (like GetNextEvent)
    /// 2. Writing it to a scratch EventRecord in guest memory
    /// 3. Injecting a 68K trampoline that calls the filter proc with correct
    ///    Pascal calling convention (Boolean result space + 3 params)
    /// 4. The trampoline saves the Boolean return value to a scratch location
    ///    so the ModalDialog re-fire path can read it
    fn fire_dialog_filter_proc(&mut self) -> bool {
        let (filter_proc, dialog_ptr, item_hit_ptr) = {
            let tracking = match self.dispatcher.dialog_tracking.as_ref() {
                Some(t) => t,
                None => return false,
            };
            (
                tracking.filter_proc,
                tracking.dialog_ptr,
                tracking.item_hit_ptr,
            )
        };

        if filter_proc == 0 {
            return false;
        }

        // Only fire the filter if the proc address contains recognisable 68K
        // function entry code. Some games pass a non-nil but invalid filterProc
        // (e.g. Marathon passes a stack address reused as a Rect buffer by
        // GetDItem, leaving it full of coordinate data, not instructions).
        // Executing garbage code would halt the CPU; skip the call instead.
        // Standard 68K function preambles: LINK A6 (0x4E56),
        //   MOVEM.L regs,-(SP) (0x48E7), JMP abs (0x4EF9), JMP PC+n (0x4EFA).
        // Inside Macintosh Volume I, I-415
        let entry = self.bus.read_word(filter_proc);
        if entry != 0x4E56 && entry != 0x48E7 && entry != 0x4EF9 && entry != 0x4EFA {
            if trace_dialog_filter_enabled() {
                eprintln!(
                    "[DIALOG-FILTER] skip invalid-entry dialog=${:08X} proc=${:08X} entry=${:04X}",
                    dialog_ptr, filter_proc, entry
                );
            }
            return false;
        }
        // Allocate EventRecord scratch space on first use.
        // EventRecord = what(2), message(4), when(4), where(4), modifiers(2)
        if self.dialog_filter_event == 0 {
            self.dialog_filter_event =
                self.dialog_callback_scratch_base() + DIALOG_FILTER_EVENT_OFFSET;
        }
        let evt = self.dialog_filter_event;

        // Allocate the 2-byte Boolean result scratch on first use.
        if self.dispatcher.dialog_filter_result_addr == 0 {
            self.dispatcher.dialog_filter_result_addr =
                self.dialog_callback_scratch_base() + DIALOG_FILTER_RESULT_OFFSET;
        }
        let result_addr = self.dispatcher.dialog_filter_result_addr;

        // Clear the filter result before each invocation.
        self.bus.write_word(result_addr, 0);

        let ticks = self.bus.read_long(0x016A);

        // Consume the next actionable event from the queue, mirroring the real
        // Mac ModalDialog which calls GetNextEvent before invoking the filter.
        // Inside Macintosh Volume I, I-415
        let idx = self
            .dispatcher
            .event_queue
            .iter()
            .position(|e| matches!(e.what, 1 | 2 | 3 | 4 | 6));
        let next_event = idx.map(|i| self.dispatcher.event_queue.remove(i).unwrap());

        let filter_event = if let Some(e) = next_event {
            e
        } else if let Some(update_event) = self
            .dispatcher
            .pending_update_event(&self.bus, 1u16 << 6)
            .filter(|event| {
                event.message == dialog_ptr
                    && !self.dialog_filter_update_event_already_sent_this_tick(
                        dialog_ptr,
                        event.message,
                    )
            })
        {
            // `GetNextEvent` normally obtains updateEvt records from the
            // Window Manager's invalid region state. The queued-event path is
            // a one-shot approximation, but apps can flush that queued event
            // before entering a nested ModalDialog filter. If the active dialog
            // itself is still invalid, deliver that real pending update to the
            // filter rather than falling through to a null event. Restrict this
            // to the current dialog so unrelated behind-window invalid regions
            // cannot flood modal filters. Pace this synthetic update source to
            // once per guest tick: IM:I I-8433 says GetNextEvent returns the
            // next available event subject to priority rules, and IM:I I-9079
            // describes update events as generated from the Window Manager's
            // accumulated update region. Re-offering the same still-invalid
            // region in a tight ModalDialog filter loop can otherwise starve
            // queued user input.
            self.dialog_filter_last_update_event_tick =
                Some((dialog_ptr, update_event.message, ticks));
            update_event
        } else {
            // Modal filters are called on null events too; many apps render
            // their dialog content from this path (e.g., idle redraw).
            let (v, h) = self.dispatcher.mouse_position();
            crate::trap::dispatch::QueuedEvent {
                what: 0,
                message: 0,
                where_v: v,
                where_h: h,
                modifiers: self.dispatcher.current_event_modifiers(),
            }
        };
        if let Some(tracking) = self.dispatcher.dialog_tracking.as_mut() {
            tracking.last_filter_event = Some(filter_event.clone());
        }
        let what = filter_event.what;
        let message = filter_event.message;
        let where_v = filter_event.where_v;
        let where_h = filter_event.where_h;
        let modifiers = filter_event.modifiers;
        if what == 0 {
            self.dialog_filter_last_null_event_tick = Some((dialog_ptr, ticks));
        } else {
            self.dialog_filter_last_null_event_tick = None;
        }
        self.dispatcher.tick_count = ticks;
        self.dispatcher.write_event_record(
            &mut self.bus,
            evt,
            what,
            message,
            where_v,
            where_h,
            modifiers,
        );
        if trace_dialog_filter_enabled() {
            eprintln!(
                "[DIALOG-FILTER] call dialog=${:08X} proc=${:08X} event=what:{} message=${:08X} where=({}, {}) mods=${:04X}",
                dialog_ptr, filter_proc, what, message, where_v, where_h, modifiers
            );
        }

        // Trampoline (48 bytes) with correct Pascal calling convention:
        //
        // FUNCTION MyFilter(theDialog: DialogPtr; VAR theEvent: EventRecord;
        //                   VAR itemHit: INTEGER): BOOLEAN;
        // Inside Macintosh Volume I, I-415
        //
        // Pascal convention: caller pushes 2-byte result space, then params
        // left-to-right. Callee pops params; result is left on stack.
        //
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)     ; 48E7 F0F0
        //   +4:  CLR.W   -(SP)                   ; 4267 — Boolean result space
        //   +6:  MOVE.L  #dialogPtr,-(SP)         ; 2F3C xxxx xxxx
        //   +12: MOVE.L  #eventPtr,-(SP)          ; 2F3C xxxx xxxx
        //   +18: MOVE.L  #itemHitPtr,-(SP)        ; 2F3C xxxx xxxx
        //   +24: JSR     filter_proc              ; 4EB9 xxxx xxxx
        //        ; callee popped 12 bytes of params; SP → 2-byte Boolean result
        //   +30: MOVE.W  (SP),(result_addr).L     ; 33D7 xxxx xxxx
        //   +36: MOVEA.L #savedSP,A7              ; 2E7C xxxx xxxx
        //   +42: MOVEM.L (SP)+,D0-D3/A0-A3       ; 4CDF 0F0F
        //   +46: RTS                              ; 4E75
        if self.dialog_filter_trampoline == 0 {
            let tramp = self.dialog_callback_scratch_base() + DIALOG_FILTER_TRAMPOLINE_OFFSET;
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x4267); // CLR.W -(SP) — result space
            self.bus.write_word(tramp + 6, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +8..+11: dialogPtr
            self.bus.write_word(tramp + 12, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +14..+17: eventPtr
            self.bus.write_word(tramp + 18, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +20..+23: itemHitPtr
            self.bus.write_word(tramp + 24, 0x4EB9); // JSR abs.L
                                                     // +26..+29: filter_proc
            self.bus.write_word(tramp + 30, 0x33D7); // MOVE.W (SP),(abs).L
                                                     // +32..+35: result_addr
            self.bus.write_word(tramp + 36, 0x2E7C); // MOVEA.L #imm,A7
                                                     // +38..+41: savedSP
            self.bus.write_word(tramp + 42, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 44, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 46, 0x4E75); // RTS
            self.dialog_filter_trampoline = tramp;
        }

        let tramp = self.dialog_filter_trampoline;
        self.bus.write_long(tramp + 8, dialog_ptr);
        self.bus.write_long(tramp + 14, evt);
        self.bus.write_long(tramp + 20, item_hit_ptr);
        self.bus.write_long(tramp + 26, filter_proc);
        self.bus.write_long(tramp + 32, result_addr);

        // ModalDialog handles events through DialogSelect, which selects the
        // dialog port before event handling. Leave that port current when the
        // filter returns so application follow-up drawing/invalidations target
        // the active dialog.
        self.dispatcher
            .set_current_port_state(&mut self.bus, &mut self.cpu, dialog_ptr, None);

        // Inject callback execution.
        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
        let d_regs = [
            self.cpu.read_reg(Register::D0),
            self.cpu.read_reg(Register::D1),
            self.cpu.read_reg(Register::D2),
            self.cpu.read_reg(Register::D3),
            self.cpu.read_reg(Register::D4),
            self.cpu.read_reg(Register::D5),
            self.cpu.read_reg(Register::D6),
            self.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.cpu.read_reg(Register::A0),
            self.cpu.read_reg(Register::A1),
            self.cpu.read_reg(Register::A2),
            self.cpu.read_reg(Register::A3),
            self.cpu.read_reg(Register::A4),
            self.cpu.read_reg(Register::A5),
            self.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.cpu.core.get_ccr();
        let sr = self.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        let saved_sp = new_sp.wrapping_sub(32); // SP after MOVEM save at trampoline entry

        // Zero the stack region the filter proc will use as local variables.
        //
        // On a real Mac, ModalDialog's internal event loop calls GetNextEvent
        // and DialogSelect between filter proc invocations, which naturally
        // overwrites the stack area with fresh data. In our HLE, the filter
        // proc is called directly without these intermediate calls, so stale
        // local variables from the previous invocation persist. This causes
        // bugs when the filter proc's code reads uninitialized locals that
        // happen to contain residual data (e.g., a stale Pascal string length
        // byte interpreted as a large count, overflowing a buffer).
        //
        // Clear 2KB below the filter proc's entry SP to simulate the stack
        // hygiene that ModalDialog's real event loop provides.
        let filter_entry_sp = saved_sp.wrapping_sub(50); // after MOVEM+params+JSR
        let clear_size: u32 = 2048;
        let clear_start = filter_entry_sp.wrapping_sub(clear_size);
        self.bus.fill_zeros(clear_start, clear_size);

        self.bus.write_long(tramp + 38, saved_sp);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::DialogFilterProc,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.cpu.write_reg(Register::PC, tramp);

        // Mark rendered_pixels stale while the filter proc is executing so
        // redraw_chrome skips restoration (which would erase the filter's
        // framebuffer output). After the filter returns and ModalDialog refires,
        // the re-snapshot path captures the filter's drawing into rendered_pixels.
        if let Some(tracking) = self.dispatcher.dialog_tracking.as_mut() {
            tracking.rendered_pixels_final = false;
        }
        true
    }

    /// Run the 68k guest until it halts or [`FixtureRunnerConfig::max_instructions`]
    /// is reached. Returns:
    /// - `Ok(())` on a clean halt (`Stopped`, ExitToShell, or invalid PC).
    /// - `Err(Error::Halted)` is *not* returned here — halt-via-trap maps
    ///   to `Ok(())`. Trap dispatch errors (other than `Halted`) propagate.
    /// - [`Error::Timeout`] when the instruction count cap is reached
    ///   before any halt condition fires.
    ///
    /// Most embedders should prefer [`FixtureRunner::run_steps`], which
    /// gives you per-call budget control, returns whether the CPU is
    /// still running, and exposes per-halt detail via the
    /// [`halted_pc`](Self::halted_pc) / [`halted_trap`](Self::halted_trap)
    /// accessors.
    pub fn run(&mut self) -> Result<()> {
        let mut count = 0;

        if trace_load_enabled() {
            eprintln!("========================================");
            eprintln!("        FIXTURE RUNNER STARTING         ");
            eprintln!("========================================");

            eprintln!(
                "[RUN] Starting at PC=${:08X}, A5=${:08X}, A7=${:08X}",
                self.cpu.read_reg(Register::PC),
                self.cpu.read_reg(Register::A5),
                self.cpu.read_reg(Register::A7)
            );
        }

        while count < self.config.max_instructions {
            if self.cpu.is_stopped() {
                if trace_load_enabled() {
                    eprintln!(
                        "[RUN] Stopped after {} instructions, PC=${:08X}",
                        count,
                        self.cpu.read_reg(Register::PC)
                    );
                }
                return Ok(());
            }

            let pc = self.cpu.read_reg(Register::PC);

            // Safety Trigger: If PC jumps outside RAM or to Low Mem, stop immediately
            // Allow $60+ since CRT relocation installs trampolines in low memory
            if pc >= self.bus.ram_size() || (pc < 0x60 && pc > 0) {
                eprintln!(
                    "[RUN] CRITICAL: PC jumped to invalid address ${:08X}! Halting trace.",
                    pc
                );
                self.dump_trace();
                return Ok(());
            }

            // Trace: Push current PC/Opcode/Regs (gated on env var).
            if trace_buffer_enabled() {
                let opcode = self.bus.read_word(pc);
                let a0 = self.cpu.read_reg(Register::A0);
                let sp = self.cpu.read_reg(Register::A7);
                let a6 = self.cpu.read_reg(Register::A6);
                let a5 = self.cpu.read_reg(Register::A5);
                if self.trace_buffer.len() >= 200 {
                    self.trace_buffer.pop_front();
                }
                self.trace_buffer.push_back((pc, opcode, a0, sp, a6, a5));
            }

            match self.cpu.step(&mut self.bus) {
                StepResult::Ok => {}
                StepResult::Stopped => {
                    if trace_load_enabled() {
                        let stopped_pc = self.cpu.read_reg(Register::PC);
                        let opcode = self.bus.read_word(stopped_pc);
                        eprintln!(
                            "[RUN] Step returned Stopped after {} instructions, PC=${:08X}, Opcode=${:04X}",
                            count, stopped_pc, opcode
                        );
                    }
                    self.dump_trace();
                    return Ok(());
                }
                StepResult::Aline(opcode) => {
                    match self
                        .dispatcher
                        .dispatch(opcode, &mut self.cpu, &mut self.bus)
                    {
                        Ok(()) => {
                            // Smart PC Advance:
                            // Only advance PC if the trap didn't change it
                            // (auto-pop traps set PC to return address)
                            let pc_after = self.cpu.read_reg(Register::PC);
                            if pc_after == pc {
                                self.cpu.write_reg(Register::PC, pc + 2);
                            }

                            // Log traps to stderr, but don't dump trace unless it's suspicious
                            // eprintln!("[RUN] Trap ${:04X} handled...", opcode);
                        }
                        Err(Error::Halted) => {
                            if trace_load_enabled() {
                                eprintln!("[RUN] Halted via trap after {} instructions", count);
                            }
                            self.dump_trace();
                            return Ok(());
                        }
                        Err(e) => {
                            self.dump_trace();
                            return Err(e);
                        }
                    }
                }
            }
            count += 1;
        }
        if trace_load_enabled() {
            eprintln!("[RUN] Timeout after {} instructions", count);
        }
        self.dump_trace();
        Err(Error::Timeout(count))
    }

    /// Walk the trace_buffer (most-recent first) and return the first
    /// PC that decode_fakeptr_pc recognises, plus its hint. Used by
    /// the halt log to surface drifted PCs that landed in unmapped
    /// memory after a JSR through a GetTrapAddress fakeptr — the
    /// halted PC itself can be 0x1000+ bytes past the original entry,
    /// well outside the documented fakeptr range, so a direct decode
    /// of the halted PC misses it. The trace_buffer is opt-in via
    /// SYSTEMLESS_TRACE_BUFFER=1; without it this scan returns None.
    fn trace_find_fakeptr_entry(&self) -> Option<(u32, String)> {
        for (pc, _op, _a0, _sp, _a6, _a5) in self.trace_buffer.iter().rev() {
            if let Some(hint) = decode_fakeptr_pc(*pc) {
                return Some((*pc, hint));
            }
        }
        None
    }

    /// Print the last N executed instructions to stderr in PC/Op/Reg
    /// form. Used by halt paths in `run` / `run_steps_internal` to
    /// surface the run-up to a crash. Early-exits when the trace
    /// buffer is empty (the default — `SYSTEMLESS_TRACE_BUFFER=1`
    /// must be set to populate the buffer in the first place).
    pub fn dump_trace(&self) {
        if self.trace_buffer.is_empty() {
            return;
        }
        eprintln!(
            "[TRACE] Last {} executed instructions:",
            self.trace_buffer.len()
        );
        eprintln!("  PC        Op    A0       SP       A6       D0");
        for (pc, opcode, a0, sp, a6, d0) in &self.trace_buffer {
            eprintln!(
                "  {:08X}  {:04X}  {:08X} {:08X} {:08X} {:08X}",
                pc, opcode, a0, sp, a6, d0
            );
        }
    }
}

/// Dump the diagnostic histograms when the runner is dropped. Each
/// `print_*_histogram` already early-returns when its env-var gate
/// isn't set, so this is a no-op for normal runs (including tests).
/// Investigate interactive-mode behavior with
/// `SYSTEMLESS_TRACE_TRAP_COUNTS=1`, `SYSTEMLESS_TRACE_OPCODE_COUNTS=1`,
/// `SYSTEMLESS_TRACE_HOT_PC=1`, or `SYSTEMLESS_TRACE_TRAP_TIMING=1`.
impl Drop for FixtureRunner {
    fn drop(&mut self) {
        self.dispatcher.print_trap_histogram(40);
        self.print_opcode_histogram(40);
        self.print_pc_histogram(40);
        self.dispatcher.print_trap_timing_histogram(40);
    }
}

// =============================================================================
// Loader Implementation
// =============================================================================

fn apply_mpw_far_segment_relocations<M: MemoryBus>(
    bus: &mut M,
    segment_id: i16,
    segment_addr: u32,
    data: &[u8],
    a5_base: u32,
) {
    let Some(header) = MpwFarSegmentHeader::parse(data) else {
        return;
    };

    let mut a5_relocation_count = 0usize;
    match header.a5_relocation_offsets(data) {
        Some(offsets) => {
            a5_relocation_count = offsets.len();
            for offset in offsets {
                let addr = segment_addr.wrapping_add(offset);
                let relocated = bus.read_long(addr).wrapping_add(a5_base);
                bus.write_long(addr, relocated);
            }
        }
        None => {
            if trace_load_enabled() {
                eprintln!(
                    "[LOAD] CODE {} has invalid MPW far A5 relocation data at ${:08X}",
                    segment_id, header.a5_relocation_data_offset
                );
            }
        }
    }

    let mut pc_relocation_count = 0usize;
    match header.pc_relocation_offsets(data) {
        Some(offsets) => {
            pc_relocation_count = offsets.len();
            for offset in offsets {
                let addr = segment_addr.wrapping_add(offset);
                let relocated = bus.read_long(addr).wrapping_add(segment_addr);
                bus.write_long(addr, relocated);
            }
        }
        None => {
            if trace_load_enabled() {
                eprintln!(
                    "[LOAD] CODE {} has invalid MPW far PC relocation data at ${:08X}",
                    segment_id, header.pc_relocation_data_offset
                );
            }
        }
    }

    bus.write_long(
        segment_addr + MpwFarSegmentHeader::CURRENT_A5_OFFSET,
        a5_base,
    );
    bus.write_long(
        segment_addr + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET,
        segment_addr,
    );

    if trace_load_enabled() {
        eprintln!(
            "[LOAD] Applied MPW far relocations for CODE {}: a5={}, pc={}",
            segment_id, a5_relocation_count, pc_relocation_count
        );
    }
}

fn load_app_generic<M: MemoryBus>(
    fork: &ResourceFork,
    bus: &mut M,
    configured_load_address: u32,
) -> Option<LoadedApp> {
    // 1. Load CODE 0 Header
    let code0 = fork.get_code(0)?;
    let header = Code0Header::parse(&code0.data)?;
    let size_resource = fork
        .get(*b"SIZE", -1)
        .and_then(|res| ApplicationSizeResource::parse(&res.data));
    let load_address = load_address_for_size_partition(
        configured_load_address,
        &header,
        size_resource,
        bus.ram_size(),
    );
    if trace_load_enabled() {
        eprintln!(
            "[LOAD] CODE 0 header: above_a5={}, below_a5={}, jt_size={}, jt_offset={}",
            header.above_a5, header.below_a5, header.jump_table_size, header.jump_table_offset
        );
        if load_address != configured_load_address {
            if let Some(size) = size_resource {
                eprintln!(
                    "[LOAD] Relocated load base for SIZE partition: configured=${:08X} effective=${:08X} preferred={} minimum={}",
                    configured_load_address,
                    load_address,
                    size.preferred_size,
                    size.minimum_size
                );
            }
        }
    }

    let a5_base = load_address + header.below_a5;
    // For classic Mac apps, above_a5 defines the space needed above A5.
    // However, some apps place QuickDraw globals at higher offsets (e.g., A5+39KB).
    // Add 48KB reserve to accommodate most classic apps.
    let qd_globals_reserve = 48 * 1024; // 48KB reserve for QD globals
    let globals_end = a5_base + header.above_a5 + qd_globals_reserve;

    // Clear A5 world
    let globals_zero_end = globals_end + 0x40000;
    bus.fill_zeros(load_address, globals_zero_end.saturating_sub(load_address));
    bus.write_long(0x0904, a5_base); // CurrentA5
    bus.write_word(0x0934, header.jump_table_offset as u16); // CurJTOffset - Inside Macintosh Volume II, II-62
    bus.write_word(0x028E, 0x0000); // ROM85

    // Write RTS stubs at low-memory jump vectors that some runtimes
    // (Think C, CodeWarrior) call directly instead of via A-line traps.
    // On a real Mac, these contain ROM routine addresses. In our HLE,
    // we place RTS instructions so JSRs to these addresses return safely.
    // Only cover $0060-$00FF to avoid corrupting system globals in $0100+
    // (e.g., $012D is a debugger presence flag that must remain 0).
    // The CRT's relocation pass will populate the real runtime trampolines.
    for addr in (0x0060..0x0100).step_by(2) {
        bus.write_word(addr, 0x4E75); // RTS
    }

    // Populate the 68k exception vector table the way a booted Mac leaves
    // it. Vector 0 holds the initial interrupt stack pointer and vector 1 the
    // initial program counter; vectors 2-63 hold the handler addresses the
    // ROM installs during startup, nearly all of them inside the ROM image.
    // Inside Macintosh Volume I, I-103 (Exception Vector Table);
    // M68000PRM, section 6.2 ("Exception Vectors").
    //
    // Leaving the table zeroed is not a neutral choice. Applications that
    // dereference an uninitialised pointer read address $0000, and a zero
    // there turns a stray write into low-memory corruption — SimCity 2000's
    // splash-screen colour animator runs before its CTabHandle is set and
    // writes through `*(long *)0`, which lands on `Ticks` ($016A) and stops
    // the clock. On real hardware the same write lands in ROM and is
    // discarded, which is why the bug stays latent there. Addresses past the
    // end of RAM are dropped by the bus, so the values below reproduce that.
    //
    // Ground truth: systemless-play/fixgen/fixtures/lowmem_vectors, captured
    // from BasiliskII (System 7.5.3, Quadra 650 ROM) — the four RAM-resident
    // entries are that ROM's RAM handlers, the rest live at $4080xxxx.
    const BOOT_EXCEPTION_VECTORS: [u32; 64] = [
        0x40810000, 0x40810000, 0x0001EAD6, 0x0001EAD8, 0x0001EADA, 0x0001EADC, 0x408026F8,
        0x408026FA, 0x408026FC, 0x408026FE, 0x408099B0, 0x4088D9FE, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x00083C16, 0x40809B40, 0x4080A1B0,
        0x00006436, 0x40809B00, 0x40809B00, 0x000737D6, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x4088D252,
        0x40802704, 0x40802704, 0x4088D856, 0x4088D28C, 0x4088D544, 0x4088D68E, 0x4088DAB0,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704,
    ];
    for (vector, &handler) in BOOT_EXCEPTION_VECTORS.iter().enumerate() {
        bus.write_long((vector as u32) * 4, handler);
    }

    // Install default RTE stubs for the "post-instruction" exception
    // vectors that real Mac OS would route to SysError. Because these
    // exceptions all stack the PC of the *next* instruction (per
    // M68000PRM, "Group 2 — internal" — vectors 5/6/7 advance PC past
    // the offending op before taking the trap), an RTE simply resumes
    // execution at the next instruction without re-entering the fault.
    // Inside Macintosh Volume I, I-103 (Exception Vector Table).
    //
    //   vector 5 ($14): Zero Divide  — DIVU/DIVS with src == 0
    //   vector 6 ($18): CHK          — bounds-check trap
    //   vector 7 ($1C): TRAPV        — programmed overflow trap
    //
    // Bus error (vector 2) and address error (vector 3) are deliberately
    // NOT installed: they stack PPC (the start of the faulting
    // instruction), so RTE-ing would re-execute it and loop forever.
    // Properly handling those requires a skip-the-instruction stub
    // which is a separate undertaking.
    bus.write_word(0x00FE, 0x4E73); // RTE
    bus.write_long(0x0014, 0x0000_00FE); // ZeroDivide vector
    bus.write_long(0x0018, 0x0000_00FE); // CHK vector
    bus.write_long(0x001C, 0x0000_00FE); // TRAPV vector

    // Load DATA 0 into A5 world (initialized globals)
    // DATA goes below A5 at address (A5 - below_a5) = load_address
    if let Some(data) = fork.get(*b"DATA", 0) {
        // DATA resource starts at offset 0 from load_address and fills up to A5
        let data_dest = load_address;
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Writing DATA 0 ({} bytes) to ${:08X}",
                data.data.len(),
                data_dest
            );
        }
        bus.write_bytes(data_dest, &data.data);
    }

    // 2. Parse Jump Table from CODE 0
    let mut jump_table = Vec::new();
    let jt_data = &code0.data[16..];

    for i in 0..header.num_entries() {
        let entry_offset = i * 8;
        if entry_offset + 8 > jt_data.len() {
            break;
        }

        let word_2_3 = u16::from_be_bytes([jt_data[entry_offset + 2], jt_data[entry_offset + 3]]);
        let (offset, segment) = if word_2_3 == 0xA9F0 {
            // FAR Format
            let seg = i16::from_be_bytes([jt_data[entry_offset], jt_data[entry_offset + 1]]);
            let off = u16::from_be_bytes([jt_data[entry_offset + 6], jt_data[entry_offset + 7]]);
            (off, seg)
        } else if word_2_3 == 0xFFFF {
            // NULL
            (0u16, 0i16)
        } else {
            // NEAR Format
            let off = u16::from_be_bytes([jt_data[entry_offset], jt_data[entry_offset + 1]]);
            let seg = i16::from_be_bytes([jt_data[entry_offset + 4], jt_data[entry_offset + 5]]);
            (off, seg)
        };

        jump_table.push(JumpTableEntry {
            offset,
            segment,
            loaded: false,
            address: 0,
        });
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Parsed JT[{}]: segment={}, offset=0x{:04X}, raw=[{:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}]",
                i, segment, offset,
                jt_data[entry_offset], jt_data[entry_offset+1],
                jt_data[entry_offset+2], jt_data[entry_offset+3],
                jt_data[entry_offset+4], jt_data[entry_offset+5],
                jt_data[entry_offset+6], jt_data[entry_offset+7]
            );
        }
    }

    // 3. Setup Layout
    let code0_base = globals_end;
    let code0_size = code0.data.len() as u32;
    let code0_user = code0_base + 4;
    let jt_base = a5_base + header.jump_table_offset;
    if trace_load_enabled() {
        eprintln!("[LOAD] Memory layout: a5_base=${:08X}, globals_end=${:08X}, code0_user=${:08X}, code0_size={}, jt_base=${:08X} (jt_offset={})",
                  a5_base, globals_end, code0_user, code0_size, jt_base, header.jump_table_offset);
    }

    // 4. Load CODE 0 (Resident)
    bus.write_long(code0_base, code0_size);
    bus.write_bytes(code0_user, &code0.data);

    // Copy the JT data from CODE 0 to the actual JT area at A5+jt_offset.
    // On a real Mac, the system writes CODE 0's JT content (bytes after the
    // 16-byte header) to A5+CurJTOffset. This populates the initial JT
    // entries with unloaded-format stubs (offset, MOVE.W #seg, _LoadSeg).
    // Inside Macintosh Volume II, II-60
    let jt_content = &code0.data[16..];
    if !jt_content.is_empty() {
        bus.write_bytes(jt_base, jt_content);
    }

    // 5. Load all other CODE resources
    let mut segment_bases = HashMap::new();
    segment_bases.insert(0, code0_user);
    crate::trap::dispatch::record_segment_base(0, code0_user);

    // Load CODE segments into memory but do NOT pre-patch jump table entries.
    // Think C / CodeWarrior apps populate the JT at runtime via their startup
    // code (crt0). Our LoadSeg trap handler patches entries on demand when
    // segments are first called, matching real Mac Segment Loader behavior.
    // Inside Macintosh Volume II, II-60; Executor segment.cpp
    //
    // Reserve space: scan CODE headers to find max JT extent so CODE segments
    // are placed above the JT area.
    let mut max_jt_end: u32 = jt_base + (jump_table.len() as u32 * 8);
    let mut all_codes = fork.get_all_code();
    all_codes.sort_by_key(|c| c.id);

    for code_res in &all_codes {
        if code_res.id == 0 || code_res.data.len() < 4 {
            continue;
        }
        let Some(segment_header) = CodeSegmentHeader::parse(&code_res.data) else {
            continue;
        };
        let Some(tab_off) = segment_header.jump_table_start_offset() else {
            continue;
        };
        let Some(n_entries) = segment_header.jump_table_entry_count() else {
            continue;
        };
        let end = jt_base + tab_off + n_entries * 8;
        if end > max_jt_end {
            max_jt_end = end;
        }

        // Pre-populate unloaded-JT-entry stubs for entries this segment
        // owns that are still ALL ZERO (i.e. not yet populated by CODE 0's
        // jt_content write). Per Inside Macintosh Volume II, II-60, an
        // unloaded entry is `offset(2) + \$3F3C(2) + seg(2) + \$A9F0(2)`
        // — JSR-ing to entry+2 fires LoadSeg via the trap. Real System 7
        // writes these stubs at app-launch time; without them, segments
        // not represented in CODE 0's jt_content stay zeroed, so a
        // guest JSR-through-JT walks zeros (or falls into the next
        // patched entry) and faults (Centaurian 1.2.1 hits this).
        //
        // Skip entries with non-zero content — they've already been
        // initialised by CODE 0's load (as stubs) or pre-patched as
        // loaded JMP.L. Stomping either of those would break MPW-style
        // fixtures where CODE 0 carries the canonical layout.
        for i in 0..n_entries {
            let entry = jt_base + tab_off + i * 8;
            let is_empty = bus.read_long(entry) == 0 && bus.read_long(entry + 4) == 0;
            let is_null_placeholder = bus.read_word(entry + 2) == 0xFFFF;
            if !is_empty && !is_null_placeholder {
                continue;
            }
            if is_null_placeholder {
                // Some near-model apps leave segment-owned CODE 0 entries as
                // `offset, FFFF, FFFF, FFFF` placeholders and call the slot at
                // entry+0. Materialize a Think-style unload stub so that first
                // call enters LoadSeg instead of executing the placeholder.
                let routine_offset = bus.read_word(entry);
                bus.write_word(entry, 0xA9F0);
                bus.write_word(entry + 2, 0);
                bus.write_word(entry + 4, routine_offset);
                bus.write_word(entry + 6, code_res.id as u16);
            } else {
                bus.write_word(entry, 0);
                bus.write_word(entry + 2, 0x3F3C);
                bus.write_word(entry + 4, code_res.id as u16);
                bus.write_word(entry + 6, 0xA9F0);
            }
        }
    }

    let reserved_boundary = std::cmp::max(code0_user + code0_size, max_jt_end);
    let mut current_load_ptr = (reserved_boundary + 4) & !3;

    for code_res in all_codes {
        if code_res.id == 0 {
            continue;
        }

        let size = code_res.data.len() as u32;
        let phys_addr = current_load_ptr;
        let user_addr = current_load_ptr + 4;

        // Dump segment header info
        let segment_header = CodeSegmentHeader::parse(&code_res.data);
        let hdr_info = match segment_header {
            Some(CodeSegmentHeader::MpwFar) => "mpw-far-model".to_string(),
            Some(CodeSegmentHeader::Near {
                table_offset,
                entry_count,
            }) => format!("near-model taboff={} n={}", table_offset, entry_count),
            Some(CodeSegmentHeader::ThinkFar {
                has_relocations,
                first_entry_index,
                entry_count,
            }) => format!(
                "think-far-model first_jt={} n={} relocs={}",
                first_entry_index, entry_count, has_relocations
            ),
            None => "unknown".to_string(),
        };
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Loading CODE {} ({} bytes) to ${:08X} [{}]",
                code_res.id, size, user_addr, hdr_info
            );
        }

        bus.write_long(phys_addr, size);
        bus.write_bytes(user_addr, &code_res.data);

        segment_bases.insert(code_res.id, user_addr);
        crate::trap::dispatch::record_segment_base(code_res.id, user_addr);

        // Only patch JT for CODE 0's entries (far-model segments from the
        // original CODE 0 parse). Near-model segments get their JT entries
        // populated by the app's startup code and patched by LoadSeg.
        if matches!(segment_header, Some(CodeSegmentHeader::MpwFar)) {
            apply_mpw_far_segment_relocations(bus, code_res.id, user_addr, &code_res.data, a5_base);

            for (i, entry) in jump_table.iter_mut().enumerate() {
                if entry.segment == code_res.id {
                    entry.loaded = true;
                    let effective_offset = entry.offset as u32;
                    // MPW far-model jump-table offsets are measured from the
                    // beginning of the CODE segment. The first externally
                    // callable routine can therefore sit at offset $28, just
                    // after the 40-byte far header.
                    entry.address = user_addr + effective_offset;

                    let jt_addr = jt_base + (i as u32 * 8);
                    bus.write_word(jt_addr, code_res.id as u16);
                    bus.write_word(jt_addr + 2, 0x4EF9); // JMP
                    bus.write_long(jt_addr + 4, entry.address);
                    if trace_load_enabled() {
                        eprintln!(
                            "[LOAD] JT[{}] -> CODE {} @ ${:08X} (far-model, off=${:04X})",
                            i, code_res.id, entry.address, effective_offset
                        );
                    }
                }
            }
        }

        current_load_ptr = (user_addr + size + 4 + 3) & !3;
    }

    let loaded_image_end = align4(globals_zero_end.max(current_load_ptr));
    if trace_load_enabled() {
        eprintln!("[LOAD] Loaded image end=${:08X}", loaded_image_end);
    }

    // Stack at top of RAM
    let stack_top = bus.ram_size() - 16;

    Some(LoadedApp {
        code0_header: header,
        a5_base,
        jump_table,
        segment_bases,
        loaded_image_end,
        initial_sp: stack_top,
        size_resource,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioBackend;
    use crate::loader::{ApplicationSizeResource, Code0Header, LoadedApp};
    use crate::sound::{
        DoubleBufferState, PendingDoubleBackCallback, PendingSoundCallback, PlaybackKind,
        SndChannel, SndCommand, OUTPUT_RATE,
    };
    use crate::trap::dispatch::{
        DialogItem, DialogTrackingState, PendingWaitNextEventReturn, QueuedEvent, TimerTask,
        VblTask,
    };
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct CapturingAudioBackend {
        stereo_samples: Rc<RefCell<Vec<u8>>>,
    }

    impl CapturingAudioBackend {
        fn new() -> (Self, Rc<RefCell<Vec<u8>>>) {
            let stereo_samples = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    stereo_samples: stereo_samples.clone(),
                },
                stereo_samples,
            )
        }
    }

    impl AudioBackend for CapturingAudioBackend {
        fn queue_samples(&mut self, samples: &[u8]) {
            self.stereo_samples.borrow_mut().extend(samples);
        }

        fn queue_stereo_samples(&mut self, samples: &[u8]) {
            self.stereo_samples.borrow_mut().extend(samples);
        }

        fn stop(&mut self) {}
    }

    fn test_region_handle(
        bus: &mut crate::memory::MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u32 {
        let rgn_ptr = 0x0030_0100;
        let rgn_handle = 0x0030_0140;
        bus.write_long(rgn_handle, rgn_ptr);
        bus.write_word(rgn_ptr, 10);
        bus.write_word(rgn_ptr + 2, top as u16);
        bus.write_word(rgn_ptr + 4, left as u16);
        bus.write_word(rgn_ptr + 6, bottom as u16);
        bus.write_word(rgn_ptr + 8, right as u16);
        rgn_handle
    }

    #[test]
    fn vfs_file_snapshot_round_trips_both_forks_and_metadata() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .vfs
            .insert("Pilots/Test Pilot".to_string(), vec![1, 2, 3]);
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Pilots/Test Pilot".to_string(), vec![4, 5, 6, 7]);
        runner
            .dispatcher
            .vfs
            .insert("__rsrc__Pilots/Test Pilot".to_string(), vec![0xEE]);
        runner
            .dispatcher
            .vfs
            .insert("Game Data/Shapes".to_string(), vec![0xAA; 1024]);
        runner
            .dispatcher
            .set_vfs_entry_metadata("Pilots/Test Pilot", *b"PIL ", *b"EVO!", 0x4000);
        runner
            .dispatcher
            .set_vfs_entry_metadata("Game Data/Shapes", *b"shap", *b"26.2", 0);

        let summaries = runner.vfs_file_summaries();
        assert_eq!(summaries.len(), 2);
        assert!(summaries
            .iter()
            .any(|summary| summary.path == "Game Data/Shapes"));

        let summaries = runner.vfs_file_summaries_where(|path| path.starts_with("Pilots/"));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].path, "Pilots/Test Pilot");
        assert_eq!(summaries[0].data_len, 3);
        assert_eq!(summaries[0].resource_len, 4);
        assert_eq!(summaries[0].file_type, u32::from_be_bytes(*b"PIL "));
        assert_eq!(summaries[0].creator, u32::from_be_bytes(*b"EVO!"));

        let snapshot = runner
            .vfs_file_snapshot("Pilots/Test Pilot")
            .expect("snapshot");
        assert_eq!(snapshot.data_fork, vec![1, 2, 3]);
        assert_eq!(snapshot.resource_fork, vec![4, 5, 6, 7]);
        assert_eq!(snapshot.finder_flags, 0x4000);

        let mut restored = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        restored.import_vfs_file(&snapshot);
        assert_eq!(
            restored.vfs_file_snapshot("Pilots/Test Pilot"),
            Some(snapshot)
        );

        assert!(restored.remove_vfs_file("Pilots/Test Pilot"));
        assert_eq!(restored.vfs_file_snapshot("Pilots/Test Pilot"), None);
    }

    #[test]
    fn import_vfs_file_relative_to_launched_app_mounts_under_app_parent() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .set_launched_app_path("EV Override/EV Override");
        let plugin = VfsFileSnapshot {
            path: "Warblade".to_string(),
            data_fork: Vec::new(),
            resource_fork: vec![1, 2, 3, 4],
            file_type: u32::from_be_bytes(*b"Op.f"),
            creator: u32::from_be_bytes(*b"Es.O"),
            finder_flags: 0x4000,
            created_date: 123,
            modified_date: 456,
        };

        runner
            .import_vfs_file_relative_to_launched_app("EV Plug-Ins", &plugin)
            .expect("relative plugin import");

        let mounted = runner
            .vfs_file_snapshot("EV Override/EV Plug-Ins/Warblade")
            .expect("mounted plugin snapshot");
        assert_eq!(mounted.resource_fork, plugin.resource_fork);
        assert_eq!(mounted.file_type, plugin.file_type);
        assert_eq!(mounted.creator, plugin.creator);
        assert_eq!(mounted.finder_flags, plugin.finder_flags);

        let parent_dir_id = runner
            .dispatcher
            .vfs_metadata
            .get("EV Override/EV Plug-Ins/Warblade")
            .expect("plugin metadata")
            .parent_dir_id;
        let entries = runner.dispatcher.list_vfs_catalog_entries(parent_dir_id);
        assert!(entries
            .iter()
            .any(|entry| !entry.is_directory && entry.name == "Warblade"));
    }

    fn make_resource_fork_bytes(resources: &[([u8; 4], i16, &[u8])]) -> Vec<u8> {
        let mut type_groups: Vec<([u8; 4], Vec<(i16, &[u8], u32)>)> = Vec::new();
        for (res_type, res_id, data) in resources {
            let group_idx = type_groups
                .iter()
                .position(|(existing_type, _)| existing_type == res_type)
                .unwrap_or_else(|| {
                    type_groups.push((*res_type, Vec::new()));
                    type_groups.len() - 1
                });
            type_groups[group_idx].1.push((*res_id, *data, 0));
        }
        type_groups.sort_by_key(|(res_type, _)| *res_type);
        for (_, entries) in &mut type_groups {
            entries.sort_by_key(|(res_id, _, _)| *res_id);
        }

        let data_offset = 16u32;
        let mut data_section = Vec::new();
        for (_, entries) in &mut type_groups {
            for (_, data, data_pos) in entries {
                *data_pos = data_section.len() as u32;
                data_section.extend_from_slice(&(data.len() as u32).to_be_bytes());
                data_section.extend_from_slice(data);
            }
        }

        let map_offset = data_offset + data_section.len() as u32;
        let type_list_offset = 30u16;
        let type_count = type_groups.len();
        let resource_count: usize = type_groups.iter().map(|(_, entries)| entries.len()).sum();
        let ref_lists_offset = 2 + type_count * 8;
        let name_list_offset = type_list_offset as usize + ref_lists_offset + resource_count * 12;
        let map_length = name_list_offset as u32;

        let mut bytes = vec![0u8; (map_offset + map_length) as usize];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&data_offset.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&(data_section.len() as u32).to_be_bytes());
        header[12..16].copy_from_slice(&map_length.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);
        bytes[data_offset as usize..data_offset as usize + data_section.len()]
            .copy_from_slice(&data_section);

        let map_start = map_offset as usize;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28]
            .copy_from_slice(&(name_list_offset as u16).to_be_bytes());
        bytes[map_start + 28..map_start + 30]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());
        let mut next_ref_list_offset = ref_lists_offset;
        for (i, (res_type, entries)) in type_groups.iter().enumerate() {
            let type_entry = type_list_start + 2 + i * 8;
            bytes[type_entry..type_entry + 4].copy_from_slice(res_type);
            bytes[type_entry + 4..type_entry + 6]
                .copy_from_slice(&((entries.len() as u16) - 1).to_be_bytes());
            bytes[type_entry + 6..type_entry + 8]
                .copy_from_slice(&(next_ref_list_offset as u16).to_be_bytes());

            let ref_list_start = type_list_start + next_ref_list_offset;
            for (j, (res_id, _, data_pos)) in entries.iter().enumerate() {
                let ref_entry = ref_list_start + j * 12;
                bytes[ref_entry..ref_entry + 2].copy_from_slice(&(*res_id as u16).to_be_bytes());
                bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
                bytes[ref_entry + 4] = 0;
                let data_offset_bytes = data_pos.to_be_bytes();
                bytes[ref_entry + 5..ref_entry + 8].copy_from_slice(&data_offset_bytes[1..4]);
            }

            next_ref_list_offset += entries.len() * 12;
        }

        bytes
    }

    fn minimal_code0(above_a5: u32, below_a5: u32, jt_size: u32, jt_offset: u32) -> Vec<u8> {
        let mut code0 = Vec::with_capacity(16 + jt_size as usize);
        code0.extend_from_slice(&above_a5.to_be_bytes());
        code0.extend_from_slice(&below_a5.to_be_bytes());
        code0.extend_from_slice(&jt_size.to_be_bytes());
        code0.extend_from_slice(&jt_offset.to_be_bytes());
        code0.resize(16 + jt_size as usize, 0);
        code0
    }

    fn size_resource_bytes(flags: u16, preferred_size: u32, minimum_size: u32) -> Vec<u8> {
        let mut size = Vec::with_capacity(10);
        size.extend_from_slice(&flags.to_be_bytes());
        size.extend_from_slice(&preferred_size.to_be_bytes());
        size.extend_from_slice(&minimum_size.to_be_bytes());
        size
    }

    #[test]
    fn init_app_preserves_resources_allocated_before_zone_header() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let bgas = [0x4E, 0x56, 0xFF, 0xA6, 0x2D, 0x7A, 0x1C, 0x72];
        let fork_bytes = make_resource_fork_bytes(&[(*b"BGAS", 128, &bgas), (*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let (_, bgas_ptr) = runner
            .dispatcher
            .find_resource_any(*b"BGAS", 128)
            .expect("BGAS resource loaded");
        assert_eq!(runner.bus.read_bytes(bgas_ptr, bgas.len()), bgas);

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_bytes(bgas_ptr, bgas.len()),
            bgas,
            "init_app must not overwrite resources loaded before zone setup"
        );
    }

    #[test]
    fn init_app_seeds_post_boot_ticks() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            DEFAULT_LAUNCH_TICKS,
            "fresh app launch should see a realistic nonzero post-boot TickCount"
        );
        assert_eq!(
            runner.dispatcher.tick_count, DEFAULT_LAUNCH_TICKS,
            "TickCount fast path must stay in sync with low-memory Ticks"
        );
    }

    #[test]
    fn init_app_seeds_classic_double_click_interval() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::DOUBLE_TIME),
            DEFAULT_DOUBLE_TIME_TICKS,
            "a zero DoubleTime makes every application-level double-click test fail"
        );
    }

    #[test]
    fn load_app_places_resources_above_large_loaded_image() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0x001D_0000, 0x0340, 0, 0);
        let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
        let fork_bytes =
            make_resource_fork_bytes(&[(*b"BGAS", 128, &marker), (*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        assert!(
            app.loaded_image_end > APP_HEAP_FLOOR,
            "fixture should force the loaded image across the default heap floor"
        );

        let heap_start = app_heap_start_for_loaded_app(&app);
        let (_, marker_ptr) = runner
            .dispatcher
            .find_resource_any(*b"BGAS", 128)
            .expect("BGAS resource loaded");
        assert!(
            marker_ptr >= heap_start + APP_ZONE_HEADER_SIZE,
            "resource data must be allocated after the relocated zone header"
        );
        assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), heap_start);
        assert_eq!(
            runner.bus.read_long(addr::HEAP_END),
            heap_start + APP_ZONE_HEADER_SIZE
        );
        assert_eq!(
            runner.bus.read_bytes(marker_ptr, marker.len()),
            marker,
            "launch initialization must not clobber resources for large loaded images"
        );
    }

    #[test]
    fn load_app_records_application_size_resource_id_minus_one() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let size = size_resource_bytes(0x0080, 0x0030_0000, 0x0020_0000);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(
            app.size_resource,
            Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: 0x0030_0000,
                minimum_size: 0x0020_0000,
            })
        );
    }

    #[test]
    fn load_app_relocates_large_size_partition_a5_above_application_zone() {
        let minimum_partition = 4_812_800;
        let preferred_partition = 6_348_800;
        let below_a5 = 0x7AF4;
        let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
        let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert!(
            app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE,
            "relocated app image must leave room for the visible app-zone header"
        );
        assert!(
            app.a5_base - APP_HEAP_FLOOR >= minimum_partition - APP_STACK_SAFETY_MARGIN,
            "large SIZE partitions should place A5 high enough for direct A5-zone memory checks"
        );
        assert!(
            app.a5_base - APP_HEAP_FLOOR >= 750 * 1024,
            "Spectre-style startup gates compare A5 - GetZone against a 750K floor"
        );
    }

    #[test]
    fn load_app_leaves_exact_2mb_size_partition_at_default_address() {
        let below_a5 = 0x68E8;
        let code0 = minimal_code0(0x0D18, below_a5, 0, 0);
        let size = size_resource_bytes(0x5880, 0x0020_0000, 0x0020_0000);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(app_image_start_for_loaded_app(&app), DEFAULT_LOAD_ADDRESS);
        assert_eq!(app.a5_base, DEFAULT_LOAD_ADDRESS + below_a5);
    }

    #[test]
    fn init_app_exposes_low_visible_zone_for_relocated_size_partition() {
        use crate::memory::globals::addr;

        let minimum_partition = 4_812_800;
        let preferred_partition = 6_348_800;
        let below_a5 = 0x7AF4;
        let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
        let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
        let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
        let fork_bytes = make_resource_fork_bytes(&[
            (*b"BGAS", 128, &marker),
            (*b"CODE", 0, &code0),
            (*b"SIZE", -1, &size),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let allocation_heap_start = app_heap_start_for_loaded_app(&app);
        let (_, marker_ptr) = runner
            .dispatcher
            .find_resource_any(*b"BGAS", 128)
            .expect("BGAS resource loaded");
        assert!(
            marker_ptr >= allocation_heap_start + APP_ZONE_HEADER_SIZE,
            "preloaded resources must still allocate above the relocated image"
        );

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), APP_HEAP_FLOOR);
        assert_eq!(runner.bus.read_long(addr::THE_ZONE), APP_HEAP_FLOOR);
        assert_eq!(
            runner.bus.read_long(addr::HEAP_END),
            APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE
        );
        assert_eq!(
            runner.bus.read_long(APP_HEAP_FLOOR),
            runner.bus.read_long(addr::APPL_LIMIT)
        );
        assert!(
            app.a5_base - runner.bus.read_long(addr::THE_ZONE) >= 750 * 1024,
            "GetZone-visible partition span should satisfy direct startup memory gates"
        );
        assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);
    }

    #[test]
    fn load_app_patches_mpw_far_jump_table_offsets_from_segment_start() {
        // Inside Macintosh: Processes 1994, p. 7-8: a loaded MPW jump-table
        // entry keeps the routine offset from the beginning of the segment.
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..24].copy_from_slice(&[
            0x00, 0x01, // segment 1
            0xA9, 0xF0, // far-model unloaded LoadSeg trap
            0x00, 0x00, 0x00, 0x28, // first routine immediately after the far header
        ]);

        let mut code1 = vec![0u8; 0x30];
        code1[0] = 0xFF;
        code1[1] = 0xFF;
        code1[0x28] = 0x4E;
        code1[0x29] = 0x75;

        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let jt_base = app.a5_base + app.code0_header.jump_table_offset;
        let code1_base = app.segment_bases[&1];

        assert_eq!(runner.bus.read_word(jt_base), 1);
        assert_eq!(runner.bus.read_word(jt_base + 2), 0x4EF9);
        assert_eq!(
            runner.bus.read_long(jt_base + 4),
            code1_base + 0x28,
            "MPW far offsets must not be adjusted by the 40-byte header twice"
        );
    }

    #[test]
    fn load_app_applies_mpw_far_a5_and_pc_relocations() {
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..24].copy_from_slice(&[
            0x00, 0x01, // segment 1
            0xA9, 0xF0, // far-model unloaded LoadSeg trap
            0x00, 0x00, 0x00, 0x28,
        ]);

        let mut code1 = vec![0u8; 0x60];
        code1[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        code1[20..24].copy_from_slice(&0x50u32.to_be_bytes());
        code1[28..32].copy_from_slice(&0x54u32.to_be_bytes());
        code1[0x28..0x2A].copy_from_slice(&[0x4E, 0xB9]); // JSR.L absolute
        code1[0x2A..0x2E].copy_from_slice(&0x100u32.to_be_bytes());
        code1[0x30..0x32].copy_from_slice(&[0x20, 0x79]); // MOVEA.L absolute
        code1[0x32..0x36].copy_from_slice(&0x40u32.to_be_bytes());
        code1[0x50..0x54].copy_from_slice(&[
            0x19, // A5 relocation at byte offset 0x32
            0x00, 0x00, 0x00,
        ]);
        code1[0x54..0x57].copy_from_slice(&[
            0x15, // PC relocation at byte offset 0x2A
            0x00, 0x00,
        ]);

        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let code1_base = app.segment_bases[&1];

        assert_eq!(
            runner.bus.read_long(code1_base + 0x2A),
            code1_base + 0x100,
            "PC relocation stream must add the loaded segment address"
        );
        assert_eq!(
            runner.bus.read_long(code1_base + 0x32),
            app.a5_base + 0x40,
            "A5 relocation stream must add the current A5"
        );
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::CURRENT_A5_OFFSET),
            app.a5_base
        );
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET),
            code1_base
        );
    }

    #[test]
    fn event_yield_services_pending_launch_application_from_vfs() {
        use crate::memory::globals::addr;

        let current_code0 = minimal_code0(0, 0x2000, 0, 0);
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
        let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
        let current_fork =
            ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner
            .dispatcher
            .vfs
            .insert("Apps/Main App".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Main App".to_string(), current_fork_bytes);
        runner
            .dispatcher
            .vfs
            .insert("Apps/Register Helper".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner.dispatcher.set_launched_app_path("Apps/Main App");

        let app = runner.load_app(&current_fork).expect("load current app");
        runner.init_app(&app);
        runner.bus.write_long(addr::TICKS, 1234);
        runner.dispatcher.tick_count = 1234;
        runner
            .dispatcher
            .queue_pending_launch_application("Apps/Register Helper", true);

        assert!(
            !runner.service_pending_launch_application(false),
            "launchContinue target must wait for an Event Manager yield"
        );
        let switched = runner.service_pending_launch_application(true);

        assert!(
            switched,
            "event yield should service the queued helper launch"
        );
        assert!(
            !runner.is_halted(),
            "queued helper launch should not halt the runner"
        );
        assert_eq!(
            runner.dispatcher.launched_app_path.as_deref(),
            Some("Apps/Register Helper")
        );
        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            1234,
            "Process Manager launch must preserve system TickCount"
        );
        let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
        let cur_ap_name = String::from_utf8(
            (0..cur_ap_len)
                .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
                .collect(),
        )
        .expect("CurApName is ASCII");
        assert_eq!(cur_ap_name, "Register Helper");
        assert!(
            runner.dispatcher.vfs.contains_key("Apps/Main App"),
            "archive VFS entries must survive the foreground app switch"
        );
        assert!(
            runner
                .dispatcher
                .vfs_rsrc
                .contains_key("Apps/Register Helper"),
            "launched app resource fork must remain available after the switch"
        );
    }

    #[test]
    fn immediate_pending_launch_application_switches_from_vfs_without_event_yield() {
        use crate::memory::globals::addr;

        let current_code0 = minimal_code0(0, 0x2000, 0, 0);
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
        let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
        let current_fork =
            ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner
            .dispatcher
            .vfs
            .insert("Apps/Main App".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Main App".to_string(), current_fork_bytes);
        runner
            .dispatcher
            .vfs
            .insert("Apps/Register Helper".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner.dispatcher.set_launched_app_path("Apps/Main App");

        let app = runner.load_app(&current_fork).expect("load current app");
        runner.init_app(&app);
        runner.bus.write_long(addr::TICKS, 4321);
        runner.dispatcher.tick_count = 4321;
        runner
            .dispatcher
            .queue_pending_launch_application("Apps/Register Helper", false);

        let switched = runner.service_pending_launch_application(false);

        assert!(
            switched,
            "immediate pending launch should not require an Event Manager yield"
        );
        assert!(
            !runner.is_halted(),
            "immediate queued helper launch should not halt the runner"
        );
        assert_eq!(
            runner.dispatcher.launched_app_path.as_deref(),
            Some("Apps/Register Helper")
        );
        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            4321,
            "foreground app switch must preserve system TickCount"
        );
        let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
        let cur_ap_name = String::from_utf8(
            (0..cur_ap_len)
                .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
                .collect(),
        )
        .expect("CurApName is ASCII");
        assert_eq!(cur_ap_name, "Register Helper");
    }

    #[test]
    fn fixture_runner_defaults_to_classic_system7_theme() {
        let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(runner.ui_theme().id(), UiThemeId::ClassicSystem7);
        assert_eq!(
            runner.theme_metrics_mode(),
            ThemeMetricsMode::ClassicGuestMetrics
        );
        assert!(runner.uses_classic_guest_metrics());
    }

    #[test]
    fn fixture_runner_accepts_explicit_systemless_theme_without_themed_metrics() {
        let runner = FixtureRunner::new(
            8 * 1024 * 1024,
            FixtureRunnerConfig {
                ui_theme: UiThemeId::SystemlessDefault,
                theme_metrics_mode: ThemeMetricsMode::ClassicGuestMetrics,
                ..FixtureRunnerConfig::default()
            },
        );
        let classic = UiThemeId::ClassicSystem7.provider();

        assert_eq!(runner.ui_theme_id(), UiThemeId::SystemlessDefault);
        assert_eq!(
            runner.dispatcher().ui_theme_id(),
            UiThemeId::SystemlessDefault
        );
        assert_eq!(runner.ui_theme().id(), UiThemeId::SystemlessDefault);
        assert!(runner.uses_classic_guest_metrics());
        assert_eq!(runner.ui_theme().menu_metrics(), classic.menu_metrics());
        assert_eq!(
            runner.ui_theme().control_metrics(),
            classic.control_metrics()
        );
        assert_ne!(runner.ui_theme().palette(), classic.palette());
    }

    fn write_double_buffer(bus: &mut MacMemoryBus, ptr: u32, samples: &[u8]) {
        bus.write_long(ptr, samples.len() as u32);
        bus.write_long(ptr + 4, 0x0000_0001);
        for (offset, sample) in samples.iter().copied().enumerate() {
            bus.write_byte(ptr + 16 + offset as u32, sample);
        }
    }

    #[test]
    fn headless_run_steps_does_not_implicitly_mix_audio() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        for offset in (0..16).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71); // NOP
        }
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);

        let mut chan = SndChannel::new(0x0039_38C8, false);
        chan.play_buffer(
            vec![0x90, 0x91, 0x92],
            OUTPUT_RATE << 16,
            PlaybackKind::Buffer,
            0,
        );
        runner.dispatcher.sound_manager.channels.push(chan);

        let (steps, running) = runner.run_steps(2, None);

        assert!(running);
        assert_eq!(steps, 2);
        assert_eq!(
            runner.audio_buffer_len(),
            0,
            "plain headless stepping must not consume sound buffers"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);

        runner.mix_audio(2);
        assert_eq!(runner.drain_audio(), vec![0x90, 0x91]);
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 2);
    }

    #[test]
    fn host_audio_backend_receives_silence_while_sound_manager_idle() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
        runner.set_audio(Box::new(audio_backend));

        runner.mix_audio(4);

        assert_eq!(
            stereo_samples.borrow().as_slice(),
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]
        );
        assert_eq!(
            runner.audio_buffer_len(),
            0,
            "host silence must not become captured guest audio"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);
    }

    #[test]
    fn host_audio_backend_receives_low_rate_sample_hold_output() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
        runner.set_audio(Box::new(audio_backend));

        let mut chan = SndChannel::new(0x0039_38C8, false);
        chan.play_buffer(
            vec![0x90, 0xA0],
            (OUTPUT_RATE / 2) << 16,
            PlaybackKind::Buffer,
            0,
        );
        runner.dispatcher.sound_manager.channels.push(chan);

        runner.mix_audio(4);

        assert_eq!(
            stereo_samples.borrow().as_slice(),
            &[
                0x90, 0x90, // source[0] at position 0.0
                0x90, 0x90, // source[0] held at position 0.5
                0xA0, 0xA0, // source[1] at position 1.0
                0xA0, 0xA0, // source[1] held at position 1.5
            ],
            "GUI/host audio path must receive the sample-hold low-rate output, not the old linear midpoint"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 4);
    }

    fn dialog_tracking_for_test(filter_proc: u32, item_hit_ptr: u32) -> DialogTrackingState {
        DialogTrackingState {
            dialog_ptr: 0x0020_0000,
            bounds: (0, 0, 32, 32),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: 0,
            item_hit_ptr,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_proc,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        }
    }

    #[test]
    fn arrows_as_numpad_remaps_key_and_char_together() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_arrows_as_numpad(true);

        assert_eq!(runner.remap_key(0x7B, 28), (0x56, b'4'));
        assert_eq!(runner.remap_key(0x7C, 29), (0x58, b'6'));
        assert_eq!(runner.remap_key(0x7D, 31), (0x57, b'5'));
        assert_eq!(runner.remap_key(0x7E, 30), (0x5B, b'8'));
        assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
    }

    #[test]
    fn arrows_not_remapped_by_default() {
        let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert!(!runner.arrows_as_numpad());
        assert_eq!(runner.remap_key(0x7B, 28), (0x7B, 28));
        assert_eq!(runner.remap_key(0x7C, 29), (0x7C, 29));
        assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
    }

    #[test]
    fn key_events_sync_low_memory_keymap() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 4), 0);
        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 15), 0);

        runner.push_key_down(0x26, b'j');
        runner.push_key_down(0x7E, 30);

        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 4),
            0x40,
            "J key should be visible to byte/bit KeyMap readers"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 5),
            0,
            "J key should not alias M at KeyMapLM byte 5 bit 6"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 15),
            0x40,
            "up arrow should be visible to byte/bit KeyMap readers"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 14),
            0,
            "up arrow should not be mirrored into the unused raw byte"
        );

        runner.push_key_up(0x26, b'j');

        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 4),
            0,
            "J key release should clear the low-memory mirror"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 5),
            0,
            "J key release should leave the M-key byte clear"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 15),
            0x40,
            "unrelated byte/bit down keys should remain mirrored"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 14),
            0,
            "unused raw byte should stay clear"
        );
    }

    #[test]
    fn init_app_seeds_top_of_stack_with_nonzero_bytes() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let stack_seed_start = app.initial_sp.saturating_sub(0x8000);
        assert_eq!(
            runner.bus.read_long(stack_seed_start),
            0xA5A5_A5A5,
            "top-of-stack window must not be zeroed"
        );
    }

    #[test]
    fn init_app_leaves_application_heap_room_below_appllimit() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let heap_end = runner.bus.read_long(addr::HEAP_END);
        let appl_limit = runner.bus.read_long(addr::APPL_LIMIT);
        assert_eq!(
            heap_end,
            0x0020_0000 + APP_ZONE_HEADER_SIZE,
            "HeapEnd should expose the initial application-zone extent"
        );
        assert!(
            appl_limit.saturating_sub(heap_end) >= 2300 * 1024,
            "direct low-memory startup checks should see growable heap room below ApplLimit"
        );
    }

    #[test]
    fn init_app_honors_size_resource_preferred_partition_for_heap_reporting() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let preferred_partition = 3 * 1024 * 1024;
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: preferred_partition,
                minimum_size: 2 * 1024 * 1024,
            }),
        };

        runner.init_app(&app);

        let expected_limit = 0x0020_0000 + preferred_partition - APP_STACK_SAFETY_MARGIN;
        let expected_free = expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE);
        assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
        assert_eq!(runner.bus.read_long(addr::BUF_PTR), expected_limit);
        assert_eq!(runner.bus.read_long(0x0020_0000), expected_limit);
        assert_eq!(runner.bus.read_long(0x0020_0000 + 12), expected_free);
        assert_eq!(
            crate::memory::app_heap_free_bytes(runner.bus()),
            expected_free
        );
        assert!(
            expected_free < crate::memory::APP_HEAP_COMPAT_FREE_FLOOR,
            "explicit SIZE partitions must bypass the compatibility floor"
        );
    }

    #[test]
    fn init_app_application_partition_override_takes_precedence_over_size_resource() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let size_partition = 3 * 1024 * 1024;
        let override_partition = 4 * 1024 * 1024;
        runner.set_application_partition_size(Some(override_partition));
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: size_partition,
                minimum_size: 2 * 1024 * 1024,
            }),
        };

        runner.init_app(&app);

        let expected_limit = 0x0020_0000 + override_partition - APP_STACK_SAFETY_MARGIN;
        assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
        assert_eq!(
            crate::memory::app_heap_free_bytes(runner.bus()),
            expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE)
        );
    }

    #[test]
    fn init_app_ignores_too_small_application_partition_override() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_application_partition_size(Some(64 * 1024));
        assert_eq!(runner.application_partition_size(), None);
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::APPL_LIMIT),
            app.initial_sp - APP_STACK_SAFETY_MARGIN,
            "invalid tiny overrides must fall back to the default launch limit"
        );
    }

    #[test]
    fn init_app_seeds_appparmhandle_with_empty_finder_information() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher_mut()
            .set_launched_app_path("Games/Armor Alley");
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let handle = runner.bus.read_long(addr::APP_PARM_HANDLE);
        assert_ne!(
            handle, 0,
            "AppParmHandle should point at Finder launch information"
        );
        let data_ptr = runner.bus.read_long(handle);
        assert_ne!(
            data_ptr, 0,
            "Finder launch information handle should be loaded"
        );
        assert_eq!(
            runner.bus.get_alloc_size(data_ptr),
            Some(4),
            "empty Finder launch information is message/count only"
        );
        assert_eq!(
            runner.bus.read_word(data_ptr),
            0,
            "message should be appOpen for a normal application launch"
        );
        assert_eq!(
            runner.bus.read_word(data_ptr + 2),
            0,
            "normal application launch has no selected documents"
        );
    }

    #[test]
    fn init_app_seeds_current_application_fcb_low_memory_state() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher_mut()
            .vfs_rsrc
            .insert("Games/Armor Alley".to_string(), vec![0xA5; 1234]);
        runner
            .dispatcher_mut()
            .set_vfs_entry_metadata("Games/Armor Alley", *b"APPL", *b"TEST", 0);
        runner
            .dispatcher_mut()
            .set_launched_app_path("Games/Armor Alley");
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_word(addr::CUR_APREF_NUM),
            APPLICATION_RESOURCE_REFNUM,
            "CurApRefNum should be the app resource fork access path"
        );
        assert_eq!(
            runner.bus.read_word(addr::FS_FCB_LEN),
            HFS_FCB_SIZE,
            "System 7 FCB size should be exposed for direct low-memory readers"
        );
        let fcb_buffer = runner.bus.read_long(addr::FCB_S_PTR);
        assert_ne!(fcb_buffer, 0, "FCBSPtr should point to an FCB buffer");
        assert_eq!(
            runner.bus.read_word(fcb_buffer),
            HFS_FCB_BUFFER_SIZE,
            "FCB buffer length should include the leading length word"
        );
        let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
        assert_eq!(
            runner.bus.read_word(fcb + 4),
            0x0200,
            "the application access path should describe a resource fork"
        );
        assert_eq!(runner.bus.read_long(fcb + 8), 1234);
        assert_eq!(runner.bus.read_long(fcb + 12), 1234);
        assert_eq!(runner.bus.read_long(fcb + 50), u32::from_be_bytes(*b"APPL"));
        assert_eq!(
            runner.bus.read_long(fcb + 58),
            runner.dispatcher.default_dir_id
        );

        let vcb = runner.bus.read_long(fcb + 20);
        assert_ne!(vcb, 0, "fcbVPtr should point to the boot volume VCB");
        assert_eq!(runner.bus.read_long(addr::DEF_VCB_PTR), vcb);
        assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 2), vcb);
        assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 6), vcb);
        assert_eq!(runner.bus.read_word(vcb + 8), 0x4244);
        assert_eq!(
            runner.bus.read_word(vcb + 78) as i16,
            crate::trap::dispatch::BOOT_VOLUME_REF_NUM
        );
        assert_eq!(
            runner
                .dispatcher
                .open_files
                .get(&APPLICATION_RESOURCE_REFNUM),
            Some(&"__rsrc__Games/Armor Alley".to_string())
        );
    }

    #[test]
    fn init_app_sets_legacy_sound_driver_low_memory_defaults() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::SD_VOLUME),
            1,
            "SdVolume ($0260) should boot to the nonzero legacy compatibility value"
        );
        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::SOUND_LEVEL),
            0,
            "SoundLevel ($027F) is a distinct Sound Driver amplitude byte"
        );
        let sound_base = runner
            .bus
            .read_long(crate::memory::globals::addr::SOUND_BASE);
        assert_eq!(
            sound_base, 0x007F_5300,
            "SoundBase ($0266) should point at the 370-word legacy sound buffer in reserved display memory"
        );
        assert_eq!(
            runner.bus.read_byte(sound_base),
            0x80,
            "legacy SoundBase buffer starts at neutral amplitude"
        );
    }

    #[test]
    fn init_app_seeds_mmu32bit_low_memory_flag() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::MMU32_BIT),
            1,
            "MMU32Bit ($0CB2) should mirror Systemless's default 32-bit addressing mode"
        );
    }

    #[test]
    fn init_app_seeds_cursor_task_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_word(CURSOR_TASK_NOOP_ADDR),
            0x4E75,
            "default cursor task target should be a callable RTS stub"
        );
        assert_eq!(
            runner
                .bus
                .read_long(crate::memory::globals::addr::J_CRSR_TASK),
            CURSOR_TASK_NOOP_ADDR,
            "JCrsrTask ($08EE) should boot to a callable no-op vector"
        );
    }

    #[test]
    fn init_app_seeds_callable_swap_font_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let swap_font_trampoline = runner
            .bus
            .read_long(crate::memory::globals::addr::J_SWAP_FONT);
        assert_ne!(swap_font_trampoline, 0);
        assert_eq!(
            [
                runner.bus.read_word(swap_font_trampoline),
                runner.bus.read_word(swap_font_trampoline + 2),
                runner.bus.read_word(swap_font_trampoline + 4),
            ],
            [0x205F, 0xA901, 0x4ED0]
        );

        let fm_input_sp = 0x007F_FE00u32;
        let fm_input = 0x0002_1000u32;
        let return_pc = 0x0002_0000u32;
        runner.bus.write_word(fm_input, 3); // family
        runner.bus.write_word(fm_input + 2, 12); // size
        runner.bus.write_byte(fm_input + 4, 0); // face
        runner.bus.write_byte(fm_input + 5, 1); // needBits
        runner.bus.write_word(fm_input + 6, 0); // device
        runner.bus.write_word(fm_input + 8, 1); // numer.v
        runner.bus.write_word(fm_input + 10, 1); // numer.h
        runner.bus.write_word(fm_input + 12, 1); // denom.v
        runner.bus.write_word(fm_input + 14, 1); // denom.h
        runner.bus.write_long(fm_input_sp, fm_input); // CONST VAR inRec
        runner.bus.write_long(fm_input_sp + 4, 0); // result slot
        runner.bus.write_long(fm_input_sp - 4, return_pc);
        runner.bus.write_word(return_pc, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, swap_font_trampoline);
        runner.cpu.write_reg(Register::A7, fm_input_sp - 4);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.cpu.read_reg(Register::PC), return_pc);
        assert_eq!(runner.cpu.read_reg(Register::A7), fm_input_sp + 4);
        assert_ne!(
            runner.bus.read_long(fm_input_sp + 4),
            0,
            "JSwapFont should return a non-NIL FMOutPtr through the Pascal result slot"
        );
    }

    #[test]
    fn cursor_task_noop_vector_does_not_fire_on_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_long(
            crate::memory::globals::addr::J_CRSR_TASK,
            CURSOR_TASK_NOOP_ADDR,
        );
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.advance_guest_tick();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cursor_task_trampoline, 0);
        assert_eq!(runner.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
        assert_eq!(runner.bus.read_long(crate::memory::globals::addr::TICKS), 1);
    }

    #[test]
    fn cursor_task_callback_arms_interrupt_from_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0004_1234;

        runner
            .bus
            .write_long(crate::memory::globals::addr::J_CRSR_TASK, callback_addr);
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.write_reg(Register::D0, 0x1111_1111);
        runner.cpu.write_reg(Register::D7, 0x7777_7777);
        runner.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.cpu.write_reg(Register::A6, 0xCCCC_0000);
        runner.cpu.core.set_ccr(0x04);
        runner.cpu.core.set_sr_noint_nosp(0x2004);

        runner.advance_guest_tick();

        let active = runner
            .active_interrupt_callback
            .expect("cursor task callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::CursorTask
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.a_regs[7], interrupted_sp);
        assert_eq!(active.a_regs[6], 0xCCCC_0000);
        assert_eq!(active.d_regs[0], 0x1111_1111);
        assert_eq!(active.d_regs[7], 0x7777_7777);
        assert_eq!(active.sr, 0x2004);
        assert_eq!(active.ccr, 0x04);
        assert_eq!(runner.cpu.core.get_sr(), 0x2104);

        assert_ne!(runner.cursor_task_trampoline, 0);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            runner.cursor_task_trampoline
        );
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
        assert_eq!(runner.bus.read_word(runner.cursor_task_trampoline), 0x48E7);
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 4),
            0x4EB9
        );
        assert_eq!(
            runner.bus.read_long(runner.cursor_task_trampoline + 6),
            callback_addr
        );
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 10),
            0x4CDF
        );
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 14),
            0x4E75
        );
    }

    #[test]
    fn cursor_task_defers_while_processor_priority_masks_level_one() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner
            .bus
            .write_long(crate::memory::globals::addr::J_CRSR_TASK, 0x0004_1234);
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_sr_noint_nosp(0x2100);

        runner.advance_guest_tick();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cursor_task_trampoline, 0);
        assert_eq!(runner.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn timer_callback_snapshot_preserves_interrupted_sp() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_8BAC;
        let interrupted_sp = 0x007F_FFC0;

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::D0, 0x1111_1111);
        runner.cpu.write_reg(Register::D7, 0x7777_7777);
        runner.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.cpu.write_reg(Register::A6, 0xCCCC_0000);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_ccr(0x1F);

        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            tm_addr: 0x0004_1234,
            active: true,
            fire_at_tick: 10,
        });

        runner.fire_timer_tasks(10);

        let active = runner
            .active_interrupt_callback
            .expect("timer callback should have been armed");

        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::Timer
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.a_regs[7], interrupted_sp);
        assert_eq!(active.a_regs[6], 0xCCCC_0000);
        assert_eq!(active.d_regs[0], 0x1111_1111);
        assert_eq!(active.d_regs[7], 0x7777_7777);
        assert_eq!(active.sr & 0x001F, 0x001F);
        assert_eq!(active.ccr, 0x1F);

        assert_ne!(runner.timer_trampoline, 0);
        assert_eq!(runner.cpu.read_reg(Register::PC), runner.timer_trampoline);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
    }

    #[test]
    fn timer_callback_fired_at_tick_cap_runs_before_yielding() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0002_0000;

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.tick_budget = 0;
        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            tm_addr: callback_addr,
            active: true,
            fire_at_tick: 101,
        });

        let (steps, running) = runner.run_steps(1, Some(101));

        assert!(running);
        assert_eq!(
            steps, 1,
            "a timer fired while reaching the tick cap must get a CPU slice"
        );
        assert_eq!(runner.bus.read_long(0x016A), 101);
        assert!(runner.active_interrupt_callback.is_some());
        assert_ne!(runner.timer_trampoline, 0);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            runner.timer_trampoline + 4
        );
    }

    #[test]
    fn timer_callback_return_runs_foreground_before_next_due_timer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0002_0000;

        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 101);
        runner.dispatcher.tick_count = 101;
        runner.set_instructions_per_tick(1);
        runner.tick_budget = 0;
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Timer,
            resume_pc: interrupted_pc,
            resume_sp: interrupted_sp,
            d_regs: [0; 8],
            a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            tm_addr: callback_addr,
            active: true,
            fire_at_tick: 102,
        });

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            interrupted_pc + 2,
            "resumed foreground instruction should run before the next timer interrupt"
        );
        assert_eq!(
            runner.guest_tick(),
            101,
            "returning from an interrupt must not immediately spend an exhausted budget on another tick"
        );
        assert!(runner.active_interrupt_callback.is_none());
        assert!(
            runner.dispatcher.timer_tasks[0].active,
            "the next due timer should remain queued until foreground code gets a slice"
        );
    }

    #[test]
    fn sound_doubleback_callback_resume_restores_ccr_before_branch() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let header_ptr = 0x0020_0000;
        let exhausted_buf_ptr = 0x0020_1000;

        // BEQ.s -> MOVEQ #2,D0 path should be taken when Z is preserved.
        runner.bus.write_word(interrupted_pc, 0x6704);
        runner.bus.write_word(interrupted_pc + 2, 0x7001);
        runner.bus.write_word(interrupted_pc + 4, 0x6002);
        runner.bus.write_word(interrupted_pc + 6, 0x7002);
        runner.bus.write_word(interrupted_pc + 8, 0x4E71);

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.write_reg(Register::D0, 0);
        runner.cpu.core.set_ccr(0x04);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr: 0x0004_1234,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();

        let active = runner
            .active_interrupt_callback
            .expect("sound callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::SoundDoubleBack
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);

        // Simulate the trampoline returning to interrupted code with CCR clobbered.
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_ccr(0);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.cpu.read_reg(Register::D0), 2);
        assert!(runner.active_interrupt_callback.is_none());
    }

    #[test]
    fn sound_doubleback_callback_trampoline_stacks_classic_pascal_order() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = 0x0020_0000;
        let chan_ptr = 0x0039_38C8;
        let exhausted_buf_ptr = 0x0020_1000;

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();
        let (_steps, running) = runner.run_steps(24, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        let saved_regs_sp = interrupted_sp - 4 - 32;
        assert_eq!(
            runner.bus.read_long(saved_regs_sp - 4),
            chan_ptr,
            "first declared Pascal argument is deeper on the stack"
        );
        assert_eq!(
            runner.bus.read_long(saved_regs_sp - 8),
            exhausted_buf_ptr,
            "last declared Pascal argument is nearest the return address"
        );
    }

    #[test]
    fn mix_audio_loads_ready_double_buffer_without_boundary_silence() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(18);
        let buf1_ptr = runner.bus.alloc(18);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0x90, 0x91]);
        write_double_buffer(&mut runner.bus, buf1_ptr, &[0xA0, 0xA1]);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        crate::trap::TrapDispatcher::load_double_buffer_samples(
            &mut runner.bus,
            &mut chan,
            buf0_ptr,
            OUTPUT_RATE << 16,
            1,
            8,
        );
        runner.dispatcher.sound_manager.channels.push(chan);

        runner.mix_audio(3);

        assert_eq!(
            runner.audio_buffer,
            vec![0x90, 0x91, 0xA0],
            "host mixing must continue into the ready paired buffer, not emit boundary silence"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "dbBufferReady stays set until the doubleback callback starts"
        );
        assert_eq!(
            runner.bus.read_long(buf1_ptr + 4) & 0x01,
            0x01,
            "the paired buffer is still marked ready while it is playing"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "exhausting buffer 0 still queues its doubleback refill"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
            0
        );

        let chan = &runner.dispatcher.sound_manager.channels[0];
        assert!(chan.is_playing(), "buffer 1 should still be playing");
        let db = chan.double_buffer.as_ref().expect("double-buffer active");
        assert_eq!(db.current_buffer, 1);
        assert!(db.waiting_for_callback);
    }

    #[test]
    fn mix_audio_can_queue_other_doubleback_while_callback_is_active() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);
        let buf1_ptr = runner.bus.alloc(17);
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_word(interrupted_pc, 0x4E71);
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
        runner.bus.write_long(buf1_ptr, 1);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        crate::trap::TrapDispatcher::load_double_buffer_samples(
            &mut runner.bus,
            &mut chan,
            buf0_ptr,
            OUTPUT_RATE << 16,
            1,
            8,
        );
        runner.dispatcher.sound_manager.channels.push(chan);

        runner.mix_audio(1);
        assert_eq!(runner.dispatcher.sound_manager.pending_callbacks.len(), 1);
        assert!(
            runner.dispatcher.sound_manager.channels[0]
                .double_buffer
                .as_ref()
                .expect("double-buffer active")
                .waiting_for_callback
        );

        runner.fire_sound_doubleback_callbacks();
        assert!(matches!(
            runner
                .active_interrupt_callback
                .expect("doubleback callback should be active")
                .source,
            ActiveInterruptCallbackSource::SoundDoubleBack
        ));
        assert!(
            runner.dispatcher.sound_manager.channels[0]
                .double_buffer
                .as_ref()
                .expect("double-buffer active")
                .waiting_for_callback,
            "callback remains outstanding until guest refills a buffer"
        );

        runner.mix_audio(16);

        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "the paired unready buffer may queue its own callback while buffer 0 is active"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index, 1,
            "buffer 0 must not be duplicated; buffer 1 gets the new callback"
        );
        let db = runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active");
        assert!(db.waiting_for_callback);
        assert_eq!(db.pending_callback_buffers, [true, true]);
    }

    #[test]
    fn mix_audio_does_not_load_ready_double_buffer_while_callback_is_active() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [true, false],
        });
        runner.dispatcher.sound_manager.channels.push(chan);
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::SoundDoubleBack,
            resume_pc: 0x0001_0000,
            resume_sp: 0x007F_FFC0,
            d_regs: [0; 8],
            a_regs: [0; 8],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });

        runner.mix_audio(1);

        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "ready buffer must not be consumed before the callback returns"
        );
        assert!(
            !runner.dispatcher.sound_manager.channels[0].is_playing(),
            "callback-active buffer load should be deferred"
        );
        assert_eq!(
            runner.audio_buffer,
            vec![0x80],
            "the host stream stays alive with silence while waiting"
        );

        runner.active_interrupt_callback = None;
        runner.try_load_pending_double_buffers();

        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "returned callback buffer stays marked ready while playback owns it"
        );
        assert!(
            runner.dispatcher.sound_manager.channels[0].is_playing(),
            "returned callback makes the refilled buffer available to the mixer"
        );
        let db = runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active");
        assert_eq!(db.pending_callback_buffers, [false, false]);

        runner.mix_audio(1);
        assert_eq!(runner.audio_buffer, vec![0x80, 0xA0]);
    }

    #[test]
    fn try_load_pending_double_buffers_recovers_ready_alternate_after_underrun() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(18);
        let buf1_ptr = runner.bus.alloc(18);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
        runner.bus.write_long(buf1_ptr, 2);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 1,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        runner.dispatcher.sound_manager.channels.push(chan);

        runner.try_load_pending_double_buffers();

        let chan = &runner.dispatcher.sound_manager.channels[0];
        assert!(chan.is_playing(), "ready alternate buffer should load");
        let db = chan.double_buffer.as_ref().expect("double-buffer active");
        assert_eq!(db.current_buffer, 0);
        assert!(
            !db.waiting_for_callback,
            "loading a ready buffer completes the outstanding refill wait"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "loading a ready alternate must not clear dbBufferReady before playback exhausts"
        );
    }

    #[test]
    fn try_load_pending_double_buffers_does_not_replay_callback_pending_slot() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);
        let buf1_ptr = runner.bus.alloc(17);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
        runner.bus.write_long(buf1_ptr, 1);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [true, false],
        });
        runner.dispatcher.sound_manager.channels.push(chan);
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.try_load_pending_double_buffers();

        assert!(
            !runner.dispatcher.sound_manager.channels[0].is_playing(),
            "an exhausted slot must not replay just because dbBufferReady remains set"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "the flag remains ready until fire_sound_doubleback_callbacks clears it"
        );
    }

    #[test]
    fn sound_command_callback_trampoline_passes_sndcommand_pointer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .push(crate::sound::PendingSoundCallback::Command {
                callback_addr: 0x0004_5678,
                chan_ptr: 0x0039_38C8,
                cmd: crate::sound::SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0x1234,
                    param2: 0x0001_43FC,
                },
            });

        runner.fire_sound_callbacks();

        let active = runner
            .active_interrupt_callback
            .expect("sound callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::SoundCallback
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);

        let tramp = runner.sound_callback_trampoline;
        let cmd_ptr = tramp + 34;
        let saved_regs_sp = interrupted_sp - 4 - 32;
        assert_eq!(runner.bus.read_long(tramp + 6), 0x0039_38C8);
        assert_eq!(runner.bus.read_long(tramp + 12), cmd_ptr);
        assert_eq!(runner.bus.read_long(tramp + 18), 0x0004_5678);
        assert_eq!(runner.bus.read_long(tramp + 24), saved_regs_sp);
        assert_eq!(runner.bus.read_word(cmd_ptr), crate::sound::cmd::CALLBACK);
        assert_eq!(runner.bus.read_word(cmd_ptr + 2), 0x1234);
        assert_eq!(runner.bus.read_long(cmd_ptr + 4), 0x0001_43FC);
    }

    #[test]
    fn sound_command_callback_trampoline_tolerates_one_long_pascal_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(4);

        // Some Pascal callback epilogues pop one long argument by copying the
        // return address over it, then RTS.
        runner.bus.write_word(callback_addr, 0x2E9F); // MOVE.L (SP)+,(SP)
        runner.bus.write_word(callback_addr + 2, 0x4E75); // RTS
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .push(crate::sound::PendingSoundCallback::Command {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: crate::sound::SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0x1234,
                    param2: 0x0001_43FC,
                },
            });

        runner.fire_sound_callbacks();
        let (steps, running) = runner.run_steps(10, None);

        assert!(running, "callback trampoline should resume foreground code");
        assert_eq!(steps, 10);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cpu.read_reg(Register::PC), interrupted_pc + 2);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
    }

    #[test]
    fn sound_file_completion_callback_trampoline_tolerates_c_style_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping chan.
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .push(crate::sound::PendingSoundCallback::FileCompletion {
                callback_addr,
                chan_ptr: 0x0039_38C8,
            });

        runner.fire_sound_callbacks();
        let (steps, running) = runner.run_steps(10, None);

        assert!(
            running,
            "file completion trampoline should resume foreground code"
        );
        assert_eq!(steps, 10);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
    }

    #[test]
    fn sound_doubleback_callback_trampoline_tolerates_c_style_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = 0x0020_0000;
        let exhausted_buf_ptr = 0x0020_1000;

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();
        let (steps, running) = runner.run_steps(12, None);

        assert!(
            running,
            "doubleback trampoline should resume foreground code"
        );
        assert_eq!(steps, 12);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
    }

    #[test]
    fn run_pending_sound_work_does_not_advance_ticks_or_foreground_code() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 41);
        runner.dispatcher.tick_count = 41;
        runner.set_instructions_per_tick(1);
        runner.tick_budget = 0;

        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::Command {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let (steps, running) = runner.run_pending_sound_work(32);

        assert!(running);
        assert!(steps > 0, "sound callback trampoline should execute");
        assert_eq!(
            runner.guest_tick(),
            41,
            "callback-only slices must not advance application-visible ticks"
        );
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            interrupted_pc,
            "sound callback service must stop before resumed foreground code runs"
        );
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(!runner.has_pending_sound_work());
    }

    #[test]
    fn gui_cpu_slice_does_not_finalize_host_frame() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::Command {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let (steps, running) = runner.run_gui_cpu_slice(0, 0);

        assert!(running);
        assert_eq!(steps, 0);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "CPU-only GUI slices must not fire host-frame sound callbacks"
        );
        assert!(runner.has_pending_sound_work());
    }

    #[test]
    fn run_steps_paces_pending_sound_doublebacks_to_one_per_slice() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(16);
        let buf1_ptr = runner.bus.alloc(16);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        for offset in (0..512).step_by(2) {
            runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
        }
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(buf0_ptr + 4, 0x0000_0001);
        runner.bus.write_long(buf1_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });
        runner
            .dispatcher
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 1,
            });

        let (_steps, running) = runner.run_steps(96, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "one CPU slice must not drain back-to-back doubleback interrupts"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
            1
        );

        let (_steps, running) = runner.run_steps(96, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(
            runner.dispatcher.sound_manager.pending_callbacks.is_empty(),
            "the next CPU slice may dispatch the next pending doubleback"
        );
    }

    #[test]
    fn vbl_callback_arms_interrupt_with_task_ptr_in_a0() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.cpu.core.set_ccr(0x04);
        runner.cpu.core.set_sr_noint_nosp(0x2004);

        runner.bus.write_word(task_ptr + 4, 1); // qType = vType
        runner.bus.write_long(task_ptr + 6, 0x0004_1234); // vblAddr
        runner.bus.write_word(task_ptr + 10, 1); // vblCount
        runner.bus.write_word(task_ptr + 12, 0); // vblPhase
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            slot: Some(9),
        });

        runner.fire_vbl_tasks();

        let active = runner
            .active_interrupt_callback
            .expect("vbl callback should have been armed");
        assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.sr, 0x2004);
        assert_eq!(active.ccr, 0x04);
        assert_eq!(runner.bus.read_word(task_ptr + 10), 0);

        assert_ne!(runner.vbl_trampoline, 0);
        assert_eq!(runner.cpu.core.get_sr(), 0x2104);
        assert_eq!(runner.cpu.read_reg(Register::PC), runner.vbl_trampoline);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
        assert_eq!(runner.bus.read_word(runner.vbl_trampoline + 4), 0x207C);
        assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), task_ptr);
        assert_eq!(
            runner.bus.read_long(runner.vbl_trampoline + 12),
            0x0004_1234
        );
    }

    #[test]
    fn vbl_callback_defers_while_processor_priority_masks_level_one() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_sr_noint_nosp(0x2100);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            slot: None,
        });

        runner.fire_vbl_tasks();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.bus.read_word(task_ptr + 10), 1);
        assert_eq!(runner.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn vbl_callback_restores_foreground_sr_after_return() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;
        let callback_addr = 0x0004_1234;

        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.bus.write_word(callback_addr, 0x4E75); // VBL callback RTS
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_sr_noint_nosp(0x2004);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, callback_addr);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            slot: None,
        });

        runner.fire_vbl_tasks();
        assert_eq!(runner.cpu.core.get_sr(), 0x2104);

        let (_steps, running) = runner.run_steps(8, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cpu.core.get_sr(), 0x2004);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn custom_instructions_per_tick_controls_tick_cadence() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 14;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(3);

        let (steps, running) = runner.run_steps(7, None);

        assert!(running);
        assert_eq!(steps, 7);
        assert_eq!(runner.bus.read_long(0x016A), 2);
    }

    #[test]
    fn non_idle_hle_trap_cost_advances_tick_budget() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8A8); // _OffsetRect
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, sp);
        runner.bus.write_word(sp, 1); // dv
        runner.bus.write_word(sp + 2, 2); // dh
        runner.bus.write_long(sp + 4, rect);
        runner.bus.write_word(rect, 10);
        runner.bus.write_word(rect + 2, 20);
        runner.bus.write_word(rect + 4, 30);
        runner.bus.write_word(rect + 6, 40);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.guest_tick(),
            1,
            "non-idle HLE traps should consume tick budget beyond the base instruction"
        );
        assert_eq!(runner.bus.read_word(rect), 11);
        assert_eq!(runner.bus.read_word(rect + 2), 22);
    }

    #[test]
    fn idle_hle_traps_do_not_apply_extra_tick_cost() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;

        runner.bus.write_word(base, 0xA975); // _TickCount
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 42);
        runner.dispatcher.tick_count = 42;
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.guest_tick(),
            42,
            "polling traps should not add synthetic HLE manager cost"
        );
        assert_eq!(runner.tick_budget, 4);
    }

    #[test]
    fn hle_trap_cost_stops_gui_slice_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8A8); // _OffsetRect
        runner.bus.write_word(base + 2, 0x4E71); // NOP that must wait for the next GUI slice
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, sp);
        runner.bus.write_word(sp, 1);
        runner.bus.write_word(sp + 2, 2);
        runner.bus.write_long(sp + 4, rect);
        runner.bus.write_word(rect, 10);
        runner.bus.write_word(rect + 2, 20);
        runner.bus.write_word(rect + 4, 30);
        runner.bus.write_word(rect + 6, 40);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_gui_slice_with_audio(8, 1, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 1);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            base + 2,
            "the next guest instruction should be deferred once HLE cost reaches the GUI tick cap"
        );
    }

    /// Regression gate for the `tick_count`-sync invariant.
    /// `advance_guest_tick` keeps bus `$016A` and
    /// `dispatcher.tick_count` lockstep; the unfreeze path also
    /// updates both. Any future change that writes `$016A` without
    /// updating `dispatcher.tick_count` (or vice versa) will desync
    /// double-click detection + the TickCount handler + diagnostic
    /// tick printouts.
    #[test]
    fn dispatcher_tick_count_stays_in_sync_with_bus() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 20;

        // NOPs keep the CPU stepping without producing traps that
        // could interfere with tick accounting.
        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        // Set both sides of the invariant to the same initial value.
        runner.dispatcher.tick_count = 0;
        runner.set_instructions_per_tick(3);

        // Step a few times; ticks should advance roughly every 3
        // instructions. After each run_steps, bus and dispatcher
        // must agree.
        for _ in 0..3 {
            let (_, running) = runner.run_steps(3, None);
            assert!(running);
            assert_eq!(
                runner.bus.read_long(0x016A),
                runner.dispatcher.tick_count,
                "bus $016A ({}) diverged from dispatcher.tick_count ({})",
                runner.bus.read_long(0x016A),
                runner.dispatcher.tick_count,
            );
        }
    }

    /// Regression gate for the TickCount spin fast-forward template A
    /// (classic MOVE+SUBQ+CMP+BHI with register target). Builds a
    /// synthetic spin body in RAM, calls the fast-forward directly,
    /// asserts the exit state matches what the guest loop's final
    /// fall-through iteration would produce.
    #[test]
    fn spin_fastfwd_template_a_advances_ticks_and_skips_loop() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Synthesised spin body:
        //   $base+0: SUBQ.W #4, A7   (0x594F)
        //   $base+2: _TickCount      (0xA975) ← trap fires before call site
        //   $base+4: MOVE.L (A7)+, D0 (0x201F)
        //   $base+6: SUBQ.L #1, D0   (0x5380)
        //   $base+8: CMP.L D0, D3    (0xB680)
        //   $base+10: BHI.S *-12     (0x62F4)
        //   $base+12: sentinel       (0x4E71 NOP)
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x5380);
        runner.bus.write_word(base + 8, 0xB680);
        runner.bus.write_word(base + 10, 0x62F4);
        runner.bus.write_word(base + 12, 0x4E71);

        // Initial tick 100, target D3=500 so target_tick = 501.
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D3, 500);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap, "no tick_cap was set, cap should not trip");
        assert_eq!(runner.dispatcher.tick_count, 501, "advanced to D3+imm");
        assert_eq!(runner.bus.read_long(0x016A), 501, "bus $016A in sync");
        // After fall-through: Dn = final_tick - imm = 501 - 1 = 500 (= D3).
        assert_eq!(runner.cpu.read_reg(Register::D0), 500);
        // A7 += 4 (the popped tick slot).
        assert_eq!(runner.cpu.read_reg(Register::A7), 0x0010_0004);
        // PC past BHI (base + 12).
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 12);
        // 4 synthesised instructions accounted for.
        assert_eq!(count, 4);
    }

    /// Rejection case — `MOVE.L (A7)+, D1` followed by `SUBQ.L #imm,
    /// D0` (different registers) must NOT match. Ensures the
    /// register-consistency check in `try_spin_template_a` guards
    /// against false positives where an unrelated MOVE happens to
    /// precede a SUBQ+CMP+BHI.
    #[test]
    fn spin_fastfwd_rejects_register_mismatch() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Same as template A but MOVE.L (A7)+ targets D1, while
        // SUBQ/CMP operate on D0. Template detector must reject.
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x221F); // MOVE.L (A7)+, D1 (NOT D0)
        runner.bus.write_word(base + 6, 0x5380); // SUBQ.L #1, D0
        runner.bus.write_word(base + 8, 0xB680); // CMP.L D0, D3
        runner.bus.write_word(base + 10, 0x62F4); // BHI.S

        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D3, 500);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        let pc_after_trap = base + 4;
        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        // No change: template rejected, tick_count stays at 100.
        assert_eq!(runner.dispatcher.tick_count, 100);
        // PC stays where it was (we passed pc_after_trap but the
        // fast-forward must have returned without mutating PC).
        assert_eq!(runner.cpu.read_reg(Register::PC), 0);
        assert_eq!(count, 0);
    }

    /// Regression gate for spin fast-forward template B (memory target,
    /// BLS variant). Sets up the post-trap state with A6 pointing at
    /// a stack frame and a target tick stored at `-4(A6)`; asserts the
    /// matcher advances to the memory target and synthesises the
    /// correct exit.
    #[test]
    fn spin_fastfwd_template_b_memory_target_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // $base+0: SUBQ.W #4, A7   (0x594F) — pre-trap SP adjust
        // $base+2: _TickCount      (0xA975) — trap
        // $base+4: MOVE.L (A7)+, D0 (0x201F)
        // $base+6: CMP.L (-4, A6), D0 — opcode 0xB0AE, d16=0xFFFC
        //          (1011 000 010 101 110 = 0xB0AE; next word 0xFFFC = -4)
        // $base+10: BLS.S *-12    (0x63F2) — target = $base+12 + (-14)
        //           = $base - 2. So target should be BEFORE $base.
        // $base+12: sentinel NOP  (0x4E71)
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0AE);
        runner.bus.write_word(base + 8, 0xFFFC);
        // Branch target must be < pc_after_trap. pc_after_trap = base+4.
        // Simplest: target = base + 2 → disp = target - (base+12) = -10
        // (since branch_src = base+10, branch_src+2 = base+12).
        // disp8 = -10 = 0xF6.
        runner.bus.write_word(base + 10, 0x63F6);
        runner.bus.write_word(base + 12, 0x4E71);

        // Memory target at -4(A6). A6 points at mid-stack; -4(A6)
        // holds the target tick.
        let a6 = 0x0010_1000u32;
        runner.bus.write_long(a6.wrapping_sub(4), 400);

        // Initial state
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::A6, a6);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap);
        // target_tick = mem_target + 1 = 400 + 1 = 401.
        assert_eq!(runner.dispatcher.tick_count, 401);
        assert_eq!(runner.bus.read_long(0x016A), 401);
        // Template B exit: D0 = final_tick (no SUBQ).
        assert_eq!(runner.cpu.read_reg(Register::D0), 401);
        assert_eq!(runner.cpu.read_reg(Register::A7), 0x0010_0004);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 12);
        // Template B synthesises 3 instructions (MOVE, CMP, BLS).
        assert_eq!(count, 3);
    }

    /// Regression gate for the TickCount spin fast-forward absolute
    /// LongInt target variant:
    ///
    ///   SUBQ.W #4,A7
    ///   _TickCount
    ///   MOVE.L (A7)+,Dn
    ///   CMP.L  (xxx).L,Dn
    ///   BCS.S  back-to-SUBQ
    ///
    /// This is the same wait-until-Ticks-reaches-memory-target shape as
    /// template B, but older MPW/Think-era code may address the target
    /// through an absolute long global instead of an A-register frame.
    #[test]
    fn spin_fastfwd_template_c_absolute_long_target_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let target_addr = 0x0002_FE44u32;

        // $base+0:  SUBQ.W #4, A7      (0x594F)
        // $base+2:  _TickCount         (0xA975)
        // $base+4:  MOVE.L (A7)+, D0   (0x201F)
        // $base+6:  CMP.L (xxx).L, D0  (0xB0B9 + absolute long)
        // $base+12: BCS.S $base        (0x65F2; base+14-14)
        // $base+14: sentinel NOP
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0B9);
        runner.bus.write_long(base + 8, target_addr);
        runner.bus.write_word(base + 12, 0x65F2);
        runner.bus.write_word(base + 14, 0x4E71);

        runner.bus.write_long(target_addr, 400);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.dispatcher.tick_count, 400);
        assert_eq!(runner.bus.read_long(0x016A), 400);
        assert_eq!(runner.cpu.read_reg(Register::D0), 400);
        assert_eq!(runner.cpu.read_reg(Register::A7), 0x0010_0004);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 14);
        assert_eq!(count, 3);
    }

    #[test]
    fn spin_fastfwd_template_d_bcc_stack_compare_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // Stack-result BCC variant:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,D7; BCC.S *-8
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x64F8);
        runner.bus.write_word(base + 8, 0x4E71);

        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D7, 500);
        runner.cpu.write_reg(Register::A7, sp - 4);
        runner.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 501);
        assert_eq!(runner.bus.read_long(sp - 4), 501);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "CMP/BCC remain for exact CPU execution");

        assert!(matches!(
            runner.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert!(matches!(
            runner.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 8);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn spin_fastfwd_template_d_lemmings_beq_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // Exact loop emitted by Lemmings 1.5.2:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,D7; BEQ.S *-8
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x67F8);
        runner.bus.write_word(base + 8, 0x4E71);

        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D7, 100);
        runner.cpu.write_reg(Register::A7, sp - 4);
        runner.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(sp - 4), 101);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "CMP/BEQ remain for exact CPU execution");

        assert!(matches!(
            runner.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert!(matches!(
            runner.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 8);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn spin_fastfwd_template_d_beq_does_not_advance_after_tick_changed() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x67F8);

        runner.bus.write_long(0x016A, 101);
        runner.dispatcher.tick_count = 101;
        runner.cpu.write_reg(Register::D7, 100);
        runner.cpu.write_reg(Register::A7, sp - 4);
        runner.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 101);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(sp - 4), 101);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_d_honors_gui_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x64F8);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D7, 500);
        runner.cpu.write_reg(Register::A7, sp - 4);
        runner.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, Some(102), &mut count);

        assert!(hit_cap);
        assert_eq!(runner.guest_tick(), 102);
        assert_eq!(runner.bus.read_long(sp - 4), 102);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp - 4);
    }

    #[test]
    fn spin_fastfwd_leaves_interrupt_callback_state_unsynthesized() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let target_addr = 0x0002_FE44u32;
        let task_ptr = 0x0020_2000u32;
        let sp = 0x0010_0000u32;

        // Same absolute-long TickCount spin as template C. The VBL task
        // becomes due during the accelerated tick advance, before the
        // loop reaches its target tick.
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0B9);
        runner.bus.write_long(base + 8, target_addr);
        runner.bus.write_word(base + 12, 0x65F2);
        runner.bus.write_word(base + 14, 0x4E71);

        runner.bus.write_long(target_addr, 400);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::PC, base + 4);
        runner.cpu.write_reg(Register::A7, sp);
        runner.cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        runner.cpu.core.set_sr_noint_nosp(0x2000);
        runner.bus.write_long(sp, 100);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            slot: None,
        });

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.dispatcher.tick_count, 101);
        assert_eq!(runner.bus.read_long(0x016A), 101);
        assert_eq!(count, 0);
        assert_eq!(runner.cpu.read_reg(Register::D0), 0xDEAD_BEEF);
        assert_eq!(runner.cpu.read_reg(Register::PC), runner.vbl_trampoline);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(runner.bus.read_long(sp - 4), base + 4);

        let active = runner
            .active_interrupt_callback
            .expect("VBL callback should remain active for normal resume handling");
        assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
        assert_eq!(active.resume_pc, base + 4);
        assert_eq!(active.resume_sp, sp);
    }

    /// Regression gates for the spin-fastfwd override. Tests the
    /// pure decision function so `OnceLock`-cached env vars don't
    /// interfere across tests.
    #[test]
    fn spin_fastfwd_gate_defaults_on_with_gui_cadence_guarded_by_tick_cap() {
        // Neither force_on nor force_off → default behaviour:
        //   headless (yield_for_ui = false) → enabled
        //   capped GUI → enabled; tick_cap preserves visible cadence
        //   uncapped GUI → disabled; it could otherwise batch visible ticks
        assert!(spin_wait_fastfwd_gate(false, false, false, false));
        assert!(spin_wait_fastfwd_gate(false, false, false, true));
        assert!(spin_wait_fastfwd_gate(false, false, true, true));
        assert!(!spin_wait_fastfwd_gate(false, false, true, false));
    }

    #[test]
    fn spin_fastfwd_gate_force_off_wins() {
        // force_off must dominate force_on and override the default in
        // either mode.
        assert!(!spin_wait_fastfwd_gate(false, true, false, false));
        assert!(!spin_wait_fastfwd_gate(false, true, true, true));
        assert!(!spin_wait_fastfwd_gate(true, true, false, true));
        assert!(!spin_wait_fastfwd_gate(true, true, true, false));
    }

    #[test]
    fn spin_fastfwd_gate_force_on_remains_enabled() {
        // The legacy force-on override remains accepted, including for an
        // uncapped GUI caller that defaults to disabled.
        assert!(spin_wait_fastfwd_gate(true, false, false, false));
        assert!(spin_wait_fastfwd_gate(true, false, true, false));
    }

    /// Regression gates for the ModalDialog noop-refire skip. The GUI
    /// gate is the most critical because tick-driven animations
    /// require real refires.
    #[test]
    fn modaldialog_refire_skip_gui_mode_never_fires() {
        // yield_for_ui=true should ALWAYS prevent the skip,
        // regardless of the other conditions.
        for has_tracking in [false, true] {
            for events in [false, true] {
                assert!(
                    !modaldialog_refire_is_noop(
                        true, // yield_for_ui = GUI
                        has_tracking,
                        true, // all "noop" conditions
                        true,
                        true,
                        true,
                        events,
                    ),
                    "GUI mode must never skip refires (has_tracking={}, events={})",
                    has_tracking,
                    events
                );
            }
        }
    }

    #[test]
    fn tracking_refire_freeze_policy_keeps_modaldialog_ticks_live() {
        // Menu/control tracking may freeze app-visible ticks while the GUI
        // renders intermediate tracking frames.
        assert!(tracking_refire_should_freeze_ticks(0xA93D));
        assert!(tracking_refire_should_freeze_ticks(0xAD3D));
        assert!(tracking_refire_should_freeze_ticks(0xA80B));
        assert!(tracking_refire_should_freeze_ticks(0xAC0B));
        assert!(tracking_refire_should_freeze_ticks(0xA968));
        assert!(tracking_refire_should_freeze_ticks(0xAD68));

        // ModalDialog must keep ticks/VBL/sound callbacks live. EV's pilot
        // dialog flow plays music through this path.
        assert!(!tracking_refire_should_freeze_ticks(0xA991));
        assert!(!tracking_refire_should_freeze_ticks(0xAD91));
    }

    #[test]
    fn gui_modaldialog_idle_refire_advances_one_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 1, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 1);
        assert_eq!(runner.tick_budget, runner.instructions_per_tick() as i32);
        assert_eq!(runner.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn gui_modaldialog_idle_refire_runs_until_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(16, 2, 0);

        assert!(running);
        assert_eq!(steps, 2);
        assert_eq!(runner.guest_tick(), 2);
        assert_eq!(runner.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn gui_modaldialog_refire_unfreezes_prior_control_tracking() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 182);
        runner.dispatcher.tick_count = 182;
        runner.frozen_ticks = Some(182);
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(16, 184, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.frozen_ticks, None);
        assert_eq!(runner.guest_tick(), 184);
        assert_eq!(runner.dispatcher.tick_count, 184);
    }

    #[test]
    fn gui_modaldialog_null_filter_fires_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 0);
        assert_ne!(runner.cpu.read_reg(Register::PC), base);
        assert!(!runner.has_pending_sound_work());
        assert!(
            !runner
                .dispatcher
                .dialog_tracking
                .as_ref()
                .unwrap()
                .rendered_pixels_final
        );
    }

    #[test]
    fn gui_modaldialog_update_filter_fires_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.dispatcher.event_queue.push_back(QueuedEvent {
            what: 6,
            message: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 0);
        assert_ne!(runner.cpu.read_reg(Register::PC), base);
        assert!(
            !runner
                .dispatcher
                .dialog_tracking
                .as_ref()
                .unwrap()
                .rendered_pixels_final
        );
    }

    #[test]
    fn gui_modaldialog_mouse_down_goes_to_filter_before_default_handling() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.tick_count = 0;
        runner.dispatcher.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 12,
            where_h: 24,
            modifiers: 0,
        });
        let mut tracking = dialog_tracking_for_test(filter_proc, 0x0010_0100);
        tracking.items.push(DialogItem {
            item_type: 4,
            rect: (8, 16, 20, 30),
            text: String::from("OK"),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        });
        runner.dispatcher.dialog_tracking = Some(tracking);

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_ne!(runner.cpu.read_reg(Register::PC), base);
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 1);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 12);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 24);
        assert!(
            runner.dispatcher.event_queue.is_empty(),
            "the filter callback should consume a queued button mouseDown event"
        );
    }

    #[test]
    fn modaldialog_filter_null_event_is_paced_per_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0001_1000u32;
        let dialog_ptr = 0x0020_0000u32;
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
        runner.dispatcher.tick_count = 42;
        runner.bus.write_long(0x016A, 42);

        assert!(runner.should_fire_dialog_filter_proc());

        runner.dialog_filter_last_null_event_tick = Some((dialog_ptr, 42));
        assert!(
            !runner.should_fire_dialog_filter_proc(),
            "a synthetic null event should not refire twice in the same guest tick"
        );

        runner.dispatcher.tick_count = 43;
        runner.bus.write_long(0x016A, 43);
        assert!(
            runner.should_fire_dialog_filter_proc(),
            "the next guest tick should allow another ModalDialog null-event filter call"
        );
    }

    #[test]
    fn modaldialog_filter_real_events_bypass_null_event_pacing() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0001_1000u32;
        let dialog_ptr = 0x0020_0000u32;
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
        runner.dispatcher.tick_count = 42;
        runner.bus.write_long(0x016A, 42);
        runner.dialog_filter_last_null_event_tick = Some((dialog_ptr, 42));
        runner.dispatcher.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 12,
            where_h: 34,
            modifiers: 0,
        });

        assert!(
            runner.should_fire_dialog_filter_proc(),
            "mouse/key/update events must still enter the filter immediately"
        );

        runner.dispatcher.event_queue.clear();
        let window_ptr = runner.bus.alloc(170);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            window_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = window_ptr;
        runner.dialog_filter_last_null_event_tick = Some((window_ptr, 42));

        assert!(
            runner.should_fire_dialog_filter_proc(),
            "a pending updateEvt for the active dialog must bypass null-event pacing"
        );
    }

    #[test]
    fn modaldialog_refire_skip_headless_requires_all_conditions() {
        // In headless mode, ALL noop conditions must be true.
        // Each condition false alone must prevent the skip.
        assert!(modaldialog_refire_is_noop(
            false, true, true, true, true, true, true,
        ));

        // Each one off in turn should prevent the skip.
        assert!(!modaldialog_refire_is_noop(
            false, false, true, true, true, true, true,
        ));
        assert!(!modaldialog_refire_is_noop(
            false, true, false, true, true, true, true,
        ));
        assert!(!modaldialog_refire_is_noop(
            false, true, true, false, true, true, true,
        ));
        assert!(!modaldialog_refire_is_noop(
            false, true, true, true, false, true, true,
        ));
        assert!(!modaldialog_refire_is_noop(
            false, true, true, true, true, false, true,
        ));
        assert!(!modaldialog_refire_is_noop(
            false, true, true, true, true, true, false,
        ));
    }

    #[test]
    fn spin_fastfwd_rejects_wrong_branch_target() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x5380);
        runner.bus.write_word(base + 8, 0xB680);
        // BHI.S with disp8 = 0xF6 (= -10, not -12). Target would
        // land at base+8, not at the SUBQ.W #4, A7 at base+0.
        runner.bus.write_word(base + 10, 0x62F6);

        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.cpu.write_reg(Register::D3, 500);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        let pc_after_trap = base + 4;
        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert_eq!(runner.dispatcher.tick_count, 100);
        assert_eq!(count, 0);
    }

    /// Regression gate for the `inline_skipped` counter on the
    /// TickCount fast path. Without this, a future change that
    /// removes the increment (or moves it to a path that doesn't
    /// actually fire) would silently produce wrong real-vs-inline
    /// counts in the timing histogram, masking real per-dispatch
    /// costs.
    #[test]
    fn tickcount_inline_skip_increments_inline_skipped() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Plain TickCount call: SUBQ.W #4, A7 ; _TickCount ; NOP
        // The runner's pre-dispatch fast path recognises 0xA975,
        // writes the tick to (A7), and continues without calling
        // dispatch().
        runner.bus.write_word(base, 0x594F); // SUBQ.W #4, A7 (reserve LONGINT slot)
        runner.bus.write_word(base + 2, 0xA975); // _TickCount
        runner.bus.write_word(base + 4, 0x4E71); // NOP (sentinel)
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.dispatcher.tick_count = 0;
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(1_000_000);

        let idx = (0xA975u16 & 0xFFF) as usize;
        let before = runner.dispatcher.inline_skipped[idx];

        // Two steps: the SUBQ first, then the trap that triggers the inline.
        let (steps, running) = runner.run_steps(2, None);
        assert!(running, "runner should not halt on a plain trap fast path");
        assert_eq!(steps, 2);

        let after = runner.dispatcher.inline_skipped[idx];
        assert_eq!(
            after - before,
            1,
            "TickCount fast path must increment inline_skipped[$0175]"
        );
        assert_eq!(
            runner.dispatcher.trap_histogram[idx], 1,
            "the same path must also increment trap_histogram[$0175]"
        );
    }

    #[test]
    fn halted_by_exit_to_shell_classifies_clean_application_quit() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA9F4); // _ExitToShell
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(!running, "ExitToShell should stop the runner");
        assert!(runner.is_halted());
        assert_eq!(runner.halted_trap(), Some(0xA9F4));
        assert!(
            runner.halted_by_exit_to_shell(),
            "ExitToShell halt must be classified as a clean application exit"
        );
    }

    #[test]
    fn halted_by_exit_to_shell_rejects_invalid_pc_halts() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.cpu.write_reg(Register::PC, runner.bus.ram_size());
        runner.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(!running, "invalid PC should stop the runner");
        assert!(runner.is_halted());
        assert_eq!(runner.halted_trap(), None);
        assert!(
            !runner.halted_by_exit_to_shell(),
            "invalid-PC halts must not be reported as clean application exits"
        );
    }

    #[test]
    fn ptinrect_inline_path_matches_pascal_stack_contract() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8AD); // _PtInRect
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, sp);
        runner.set_instructions_per_tick(1_000_000);

        runner.bus.write_long(sp, rect);
        runner.bus.write_word(sp + 4, 20); // pt.v
        runner.bus.write_word(sp + 6, 30); // pt.h
        runner.bus.write_word(rect, 10); // top
        runner.bus.write_word(rect + 2, 25); // left
        runner.bus.write_word(rect + 4, 40); // bottom
        runner.bus.write_word(rect + 6, 50); // right

        let idx = (0xA8ADu16 & 0xFFF) as usize;
        let before_inline = runner.dispatcher.inline_skipped[idx];
        let before_game = runner.dispatcher.game_trap_count;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running, "runner should not halt on PtInRect inline path");
        assert_eq!(steps, 1);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 2);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(runner.bus.read_word(sp + 8), 0x0100);
        assert_eq!(runner.dispatcher.inline_skipped[idx] - before_inline, 1);
        assert_eq!(runner.dispatcher.trap_histogram[idx], 1);
        assert_eq!(runner.dispatcher.game_trap_count - before_game, 1);
    }

    #[test]
    fn eventavail_inline_path_peeks_without_dequeueing() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let event = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA971); // _EventAvail
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, sp);
        runner.set_instructions_per_tick(1_000_000);
        runner.bus.write_long(sp, event);
        runner.bus.write_word(sp + 4, 0x0008); // keyDownMask
        runner.dispatcher.push_key_down(0x31, b' ');

        let idx = (0xA971u16 & 0xFFF) as usize;
        let before_inline = runner.dispatcher.inline_skipped[idx];
        let before_game = runner.dispatcher.game_trap_count;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running, "runner should not halt on EventAvail inline path");
        assert_eq!(steps, 1);
        assert_eq!(runner.cpu.read_reg(Register::PC), base + 2);
        assert_eq!(runner.cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(runner.bus.read_word(sp + 6), 0xFFFF);
        assert_eq!(runner.bus.read_word(event), 3);
        assert_eq!(
            runner.bus.read_long(event + 2),
            (0x31u32 << 8) | u32::from(b' ')
        );
        assert_eq!(
            runner.dispatcher.event_queue.len(),
            1,
            "EventAvail must not dequeue the matching event"
        );
        assert_eq!(runner.dispatcher.inline_skipped[idx] - before_inline, 1);
        assert_eq!(runner.dispatcher.trap_histogram[idx], 1);
        assert_eq!(
            runner.dispatcher.game_trap_count, before_game,
            "EventAvail remains excluded from game_trap_count as an idle trap"
        );
    }

    /// Regression gate for the `inline_skipped` counter on the
    /// ModalDialog batched no-op refire path. The runner's pre-
    /// dispatch fast path increments `inline_skipped[$0191]` once
    /// for the entry plus `BATCH-1=63` times in the inner loop.
    /// Without this test, a regression that drops the increment
    /// would silently make ModalDialog look ~99% inline-skipped
    /// without surfacing anywhere else.
    #[test]
    fn modaldialog_batched_skip_increments_inline_skipped_by_batch() {
        use crate::trap::dispatch::DialogTrackingState;
        use std::collections::VecDeque;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Bare ModalDialog at PC. The runner-level fast path rewinds
        // PC after each fire, so re-firing repeatedly into the same
        // trap word is the production behaviour.
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.dispatcher.tick_count = 0;
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(1_000_000);

        // Populate dialog_tracking so the noop_refire pure-decision
        // function returns true. modaldialog_refire_is_noop requires
        // ALL of: tracking present, filter_proc=0, flash_remaining=0,
        // draw_procs_done, rendered_pixels_final, event queue empty
        // (and yield_for_ui = false in headless run_steps).
        runner.dispatcher.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr: 0x0020_0000,
            bounds: (0, 0, 32, 32),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: 0,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let idx = (0xA991u16 & 0xFFF) as usize;
        let before_inline = runner.dispatcher.inline_skipped[idx];
        let before_hist = runner.dispatcher.trap_histogram[idx];

        // BATCH=64 in the runner. With max_steps=64 we should observe
        // exactly one entry + 63 batched iterations = 64 increments.
        let (steps, _running) = runner.run_steps(64, None);

        assert_eq!(
            steps, 64,
            "max_steps cap exhausted by 64 batched no-op refires"
        );
        let after_inline = runner.dispatcher.inline_skipped[idx];
        let after_hist = runner.dispatcher.trap_histogram[idx];
        assert_eq!(
            after_inline - before_inline,
            64,
            "batched skip must increment inline_skipped[$0191] by BATCH=64"
        );
        assert_eq!(
            after_hist - before_hist,
            64,
            "trap_histogram and inline_skipped must increment in lockstep on the inline path"
        );
    }

    #[test]
    fn modaldialog_batched_skip_applies_after_paced_filter_null_event() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        let dialog_ptr = 0x0020_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.dispatcher.tick_count = 42;
        runner.bus.write_long(0x016A, 42);
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
        runner.dialog_filter_last_null_event_tick = Some((dialog_ptr, 42));

        let idx = (0xA991u16 & 0xFFF) as usize;
        let before_inline = runner.dispatcher.inline_skipped[idx];
        let before_hist = runner.dispatcher.trap_histogram[idx];

        let (steps, running) = runner.run_steps(64, None);

        assert!(running);
        assert_eq!(steps, 64);
        assert_eq!(runner.cpu.read_reg(Register::PC), base);
        assert_eq!(runner.dispatcher.inline_skipped[idx] - before_inline, 64);
        assert_eq!(runner.dispatcher.trap_histogram[idx] - before_hist, 64);
    }

    #[test]
    fn tick_progress_persists_across_multiple_run_slices() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 12;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(5);

        let (steps1, running1) = runner.run_steps(3, None);
        let (steps2, running2) = runner.run_steps(3, None);

        assert!(running1);
        assert!(running2);
        assert_eq!(steps1, 3);
        assert_eq!(steps2, 3);
        assert_eq!(runner.bus.read_long(0x016A), 1);
    }

    #[test]
    fn tick_override_breaks_once_target_tick_is_reached() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 16;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(4);

        let (steps, running) = runner.run_steps_with_audio(16, Some(0), 0);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.bus.read_long(0x016A), 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_advance_in_headless_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 3;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_capped_to_zero_in_headless() {
        // `cap=Some(0)` is the scripted default — `WaitNextEvent`
        // sleep is treated as a zero-cost return (matching real Mac OS
        // where WNE doesn't directly tick; only the VBL hardware
        // interrupt does).
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(0));
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (_steps, _running) = runner.run_steps(1, None);

        // Zero ticks advanced (cap=0).
        assert_eq!(runner.bus.read_long(0x016A), 0);
        // But pending sleep is cleared so the game resumes immediately.
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_capped_in_headless_when_opt_in() {
        // Headless callers (e.g. scripted harnesses) can opt in to a
        // per-WNE-call sleep tick cap matching GUI mode, preventing
        // tick counts from racing ahead of real-Mac VBL pacing during
        // event-loop-heavy gameplay.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(1));
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        // Only 1 tick advanced (cap), not the full 60.
        assert_eq!(runner.bus.read_long(0x016A), 1);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert_eq!(runner.wait_sleep_cap_in_headless(), Some(1));
    }

    #[test]
    fn pending_wait_sleep_ticks_suspends_foreground_until_gui_tick_cap() {
        // In GUI mode (tick_override=Some), WNE sleep advances VBL/timer time
        // up to the current frame cap but keeps the foreground app suspended
        // until the requested sleep expires. This prevents sleep=60 loops from
        // receiving 60 null events per second. Inside Macintosh: Processes
        // 1994, p. 2-8.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 0,
            "foreground code should not resume while WNE sleep remains pending"
        );
        assert_eq!(runner.bus.read_long(0x016A), 10);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
    }

    #[test]
    fn pending_wait_sleep_ticks_wakes_wait_next_event_with_queued_input() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.sent_open_app_event = true;
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });
        runner.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.bus.read_word(event_ptr),
            1,
            "queued mouseDown should replace the pending null EventRecord"
        );
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0xFFFF,
            "WaitNextEvent result slot should be rewritten to TRUE"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    }

    #[test]
    fn push_mouse_down_wakes_pending_wait_next_event_immediately() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.sent_open_app_event = true;
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            1,
            "input injection should wake a sleeping WaitNextEvent before the next CPU slice"
        );
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
        assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    }

    #[test]
    fn set_mouse_position_wakes_pending_wait_next_event_with_mouse_moved_region() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;
        let mouse_rgn = test_region_handle(&mut runner.bus, 10, 20, 30, 40);

        runner.set_mouse_position(20, 25);
        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.sent_open_app_event = true;
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0x8000,
            mouse_rgn,
            resume_pc: None,
            resume_sp: None,
        });

        runner.set_mouse_position(50, 25);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            15,
            "mouse movement outside the pending mouseRgn should wake WaitNextEvent with osEvt"
        );
        assert_eq!(runner.bus.read_long(event_ptr + 2), 0xFA00_0000);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 50u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 25u16);
        assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
        assert_eq!(
            runner.dispatcher.debug_mouse_moved_event_count, 1,
            "async wake path should share the normal mouse-moved event accounting"
        );
    }

    #[test]
    fn set_mouse_position_wakes_pending_wait_next_event_with_null_for_polling_loop() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.tick_budget = 0;
        runner.bus.write_word(result_ptr, 0xFFFF);
        runner.dispatcher.sent_open_app_event = true;
        runner.dispatcher.write_event_record(
            &mut runner.bus,
            event_ptr,
            0xFFFF,
            0xABCD_EF01,
            1,
            2,
            3,
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });

        runner.set_mouse_position(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            0,
            "mouse movement with no mouseRgn event should wake WNE as a null event for polling loops"
        );
        assert_eq!(runner.bus.read_long(event_ptr + 2), 0);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0,
            "WaitNextEvent should return FALSE when the wake is only for polling input"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());

        let (steps, running) = runner.run_steps(1, Some(110));
        assert!(running);
        assert_eq!(
            steps, 1,
            "polling wake should let foreground code resume before the old sleep expires"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            100,
            "polling wake must not spend the next slice only advancing ticks"
        );
    }

    #[test]
    fn push_mouse_down_leaves_pending_wait_next_event_parked_during_interrupt_callback() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.sent_open_app_event = true;
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Timer,
            resume_pc: interrupted_pc,
            resume_sp: interrupted_sp,
            d_regs: [0; 8],
            a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            0,
            "input must not rewrite a foreground WaitNextEvent record while an interrupt callback is active"
        );
        assert_eq!(runner.bus.read_word(result_ptr), 0);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 60);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
        assert!(
            runner
                .dispatcher
                .event_queue
                .iter()
                .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
            "the mouseDown should remain queued for the foreground event loop"
        );
    }

    #[test]
    fn pending_wait_next_event_drops_stale_return_after_foreground_moves_on() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let parked_pc = 0x0001_0000;
        let stale_pc = 0x0001_0010;
        let parked_sp = 0x007F_FFC0;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(stale_pc, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, stale_pc);
        runner.cpu.write_reg(Register::A7, parked_sp);
        runner.bus.write_word(result_ptr, 0xA582);
        runner.dispatcher.sent_open_app_event = true;
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: Some(parked_pc),
            resume_sp: Some(parked_sp),
        });
        runner.dispatcher.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0xA582,
            "a stale WaitNextEvent return slot may now belong to a caller frame"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
        assert!(
            runner
                .dispatcher
                .event_queue
                .iter()
                .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
            "stale WNE cleanup should not silently consume a queued event"
        );
    }

    #[test]
    fn push_mouse_down_restores_foreground_budget_before_next_tick_cap_run() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.tick_budget = 0;

        runner.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(110));

        assert!(running);
        assert_eq!(
            steps, 1,
            "input injected at an exhausted tick boundary should let foreground code run"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            100,
            "foreground input wake must not spend the next slice only advancing ticks"
        );
    }

    #[test]
    fn set_mouse_position_restores_foreground_budget_before_next_tick_cap_run() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.dispatcher.tick_count = 100;
        runner.tick_budget = 0;

        runner.set_mouse_position(123, 456);

        let (steps, running) = runner.run_steps(1, Some(110));

        assert!(running);
        assert_eq!(
            steps, 1,
            "mouse movement at an exhausted tick boundary should let polling foreground code run"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            100,
            "foreground mouse-move wake must not spend the next slice only advancing ticks"
        );
    }

    #[test]
    fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new(),
            },
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 0,
            "app-owned visible dialogs must not collapse WaitNextEvent sleep before ModalDialog"
        );
        assert_eq!(runner.bus.read_long(0x016A), 10);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
    }

    #[test]
    fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_headless_cap_zero() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(0));
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new(),
            },
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(
            steps, 1,
            "headless cap zero must not collapse app-owned dialog sleep"
        );
        assert_eq!(runner.bus.read_long(0x016A), 60);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_collapses_retained_modaldialog_snapshot_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new(),
            },
        );
        runner.dispatcher.dialog_modal_entered.insert(dialog_ptr);
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 1,
            "retained ModalDialog snapshots keep the existing app-yield path"
        );
        assert_eq!(runner.bus.read_long(0x016A), 0);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_resumes_when_gui_sleep_expires_before_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 3;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_delay_ticks_advance_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_delay_ticks = 3;

        let (steps, _running) = runner.run_steps(1, Some(10));

        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
        assert_eq!(runner.cpu.read_reg(Register::D0), 3);
    }

    #[test]
    fn dialog_filter_synthesized_null_event_uses_live_modifiers() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.dispatcher.set_mouse_position(222, 333);
        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr: 0x0020_0000,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: 0x007F_FFC0,
            item_hit_ptr: 0x0030_0000,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: std::collections::VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_proc,
            game_managed: true,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 0);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 222);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 333);
        assert_eq!(
            runner.bus.read_word(event_ptr + 14),
            runner.dispatcher.current_event_modifiers()
        );
        assert_eq!(
            runner.dialog_filter_last_null_event_tick,
            Some((0x0020_0000, 0))
        );
    }

    #[test]
    fn dialog_filter_uses_active_dialog_pending_update_before_null_event() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.event_queue.clear();
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 6);
        assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);
        assert_eq!(runner.dialog_filter_last_null_event_tick, None);
    }

    #[test]
    fn dialog_filter_paces_synthetic_update_without_starving_queued_input() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 17);
        runner.dispatcher.tick_count = 17;
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.event_queue.clear();
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 6);
        assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);

        runner.active_interrupt_callback = None;
        runner.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .last_filter_event = None;
        assert!(
            !runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "the same invalid-region update should not refire indefinitely in one guest tick"
        );

        runner.dispatcher.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            where_v: 123,
            where_h: 234,
            modifiers: 0,
        });
        assert!(
            runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "queued user input must bypass synthetic update pacing"
        );
        assert!(runner.fire_dialog_filter_proc());
        assert_eq!(runner.bus.read_word(event_ptr), 1);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 234);
        assert!(
            runner.dispatcher.event_queue.is_empty(),
            "the queued mouse event should be consumed by the filter call"
        );

        runner.active_interrupt_callback = None;
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .last_filter_event = None;
        runner.bus.write_long(0x016A, 18);
        runner.dispatcher.tick_count = 18;
        assert!(
            runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "a still-invalid dialog can surface another update event on the next guest tick"
        );
    }

    #[test]
    fn dialog_filter_proc_leaves_dialog_port_current() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let main_port = runner.bus.alloc(170);
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            main_port,
            0,
            0,
            0,
            600,
            800,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner
            .dispatcher
            .set_current_port_state(&mut runner.bus, &mut runner.cpu, main_port, None);
        let filter_proc = runner.bus.alloc(8);
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry

        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        assert_eq!(runner.dispatcher.current_port, dialog_ptr);
        assert_eq!(
            runner
                .active_interrupt_callback
                .as_ref()
                .and_then(|callback| callback.restore_port),
            None
        );

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        let (_steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(runner.dispatcher.current_port, dialog_ptr);
        assert!(runner.active_interrupt_callback.is_none());
    }

    #[test]
    fn dialog_draw_proc_trampoline_passes_item_first_and_tolerates_plain_rts() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = 0x0020_0000u32;
        let proc_addr = 0x0004_2000u32;
        let item_no = 5i16;

        // Keep foreground execution stable after the callback returns.
        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        // MPW-style proc prologue shape. It returns with plain RTS, leaving
        // callback parameters on the stack; the trampoline must restore A7.
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 64, 64),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        let tramp = runner.dialog_draw_trampoline;
        assert_eq!(runner.bus.read_word(tramp), 0x48E7);
        assert_eq!(runner.bus.read_word(tramp + 4), 0x2F3C);
        assert_eq!(runner.bus.read_long(tramp + 6), dialog_ptr);
        assert_eq!(runner.bus.read_word(tramp + 10), 0x3F3C);
        assert_eq!(runner.bus.read_word(tramp + 12), item_no as u16);
        assert_eq!(runner.bus.read_word(tramp + 14), 0x4EB9);
        assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
        assert_eq!(runner.bus.read_word(tramp + 20), 0x4FF9);
        assert_eq!(runner.bus.read_long(tramp + 22), interrupted_sp - 36);

        let (_steps, running) = runner.run_steps(16, None);

        assert!(running);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "dialog callback should have resumed foreground code"
        );
        assert_eq!(runner.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn modeless_dialog_draw_proc_accepts_a5_relative_proc_ptr() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let a5 = 0x0020_0000u32;
        let proc_offset = 0x0000_4200u32;
        let proc_addr = a5 + proc_offset;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(interrupted_pc, 0x60FE);
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.write_reg(Register::A5, a5);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
        runner
            .dispatcher
            .modeless_dialog_draw_proc_queue
            .push_back((dialog_ptr, proc_offset, 5));

        assert!(runner.fire_modeless_dialog_draw_proc());

        let tramp = runner.dialog_draw_trampoline;
        assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
        assert_eq!(
            runner.dispatcher.active_modeless_dialog_draw_proc,
            Some(dialog_ptr)
        );
    }

    #[test]
    fn modeless_dialog_draw_procs_drain_after_plain_trap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = runner.bus.alloc(170);
        let proc_1 = 0x0004_2000u32;
        let proc_2 = 0x0004_2100u32;

        runner.bus.write_word(base, 0xA861); // _Random
        runner.bus.write_word(base + 2, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, base);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        for proc_addr in [proc_1, proc_2] {
            runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
            runner.bus.write_word(proc_addr + 2, 0x0000);
            runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
            runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
        }
        runner.dispatcher.modeless_dialog_draw_proc_queue =
            VecDeque::from([(dialog_ptr, proc_1, 3), (dialog_ptr, proc_2, 5)]);

        let (_steps, running) = runner.run_steps(128, None);

        assert!(running);
        assert!(runner.dispatcher.modeless_dialog_draw_proc_queue.is_empty());
        assert_eq!(runner.dispatcher.active_modeless_dialog_draw_proc, None);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "modeless draw callbacks should have returned to foreground code"
        );
    }

    #[test]
    fn dialog_draw_proc_does_not_restore_over_guest_selected_dialog_port() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let main_port = runner.bus.alloc(170);
        let dialog_ptr = runner.bus.alloc(170);
        let proc_addr = 0x0004_2000u32;
        let item_no = 5i16;

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            main_port,
            0,
            0,
            0,
            600,
            800,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner
            .dispatcher
            .set_current_port_state(&mut runner.bus, &mut runner.cpu, main_port, None);

        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x2F3C); // MOVE.L #dialog,-(SP)
        runner.bus.write_long(proc_addr + 6, dialog_ptr);
        runner.bus.write_word(proc_addr + 10, 0xA873); // _SetPort
        runner.bus.write_word(proc_addr + 12, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 14, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (120, 180, 240, 420),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        assert_eq!(
            runner
                .active_interrupt_callback
                .as_ref()
                .and_then(|callback| callback.restore_port),
            None
        );

        let (_steps, running) = runner.run_steps(32, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(
            runner.dispatcher.current_port, dialog_ptr,
            "Dialog Manager must leave the dialog port current after the draw proc"
        );
    }

    #[test]
    fn dialog_draw_proc_pascal_stack_places_item_number_before_window_pointer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = 0x0029_4240u32;
        let proc_addr = 0x0004_2000u32;
        let item_no = 2i16;
        let seen_item_addr = 0x0004_3000u32;
        let seen_dialog_addr = 0x0004_3004u32;

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);

        // PROCEDURE MyItem(theWindow: WindowPtr; itemNo: INTEGER);
        // Inside Macintosh Volume I, I-405. MPW Pascal prologues observe
        // itemNo at 8(A6) and theWindow at 10(A6).
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x302E); // MOVE.W 8(A6),D0
        runner.bus.write_word(proc_addr + 6, 0x0008);
        runner.bus.write_word(proc_addr + 8, 0x33C0); // MOVE.W D0,(abs).L
        runner.bus.write_long(proc_addr + 10, seen_item_addr);
        runner.bus.write_word(proc_addr + 14, 0x222E); // MOVE.L 10(A6),D1
        runner.bus.write_word(proc_addr + 16, 0x000A);
        runner.bus.write_word(proc_addr + 18, 0x23C1); // MOVE.L D1,(abs).L
        runner.bus.write_long(proc_addr + 20, seen_dialog_addr);
        runner.bus.write_word(proc_addr + 24, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 26, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (120, 180, 240, 420),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        let (_steps, running) = runner.run_steps(48, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.bus.read_word(seen_item_addr) as i16, item_no);
        assert_eq!(runner.bus.read_long(seen_dialog_addr), dialog_ptr);
    }

    #[test]
    fn pending_delay_ticks_fire_vbl_tasks_in_headless_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.bus.write_word(interrupted_pc, 0x4E71);
        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.core.set_sr_noint_nosp(0x2000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_delay_ticks = 1;

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            slot: None,
        });

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 1);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
        assert_eq!(runner.cpu.read_reg(Register::D0), 1);
        assert!(matches!(
            runner.active_interrupt_callback,
            Some(ActiveInterruptCallback {
                source: ActiveInterruptCallbackSource::Vbl,
                ..
            })
        ));
    }

    /// `set_mouse_position` updates both the dispatcher's tracked
    /// position and the six low-memory mouse globals (MTemp $0828,
    /// RawMouse $082C, Mouse $0830) so guest code that polls them
    /// directly sees the new coordinates without waiting for a click.
    /// Inside Macintosh Volume II, II-371.
    #[test]
    fn set_mouse_position_updates_dispatcher_and_low_mem_globals() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.set_mouse_position(123, 456);

        assert_eq!(runner.dispatcher.mouse_pos, (123, 456));
        for off in [0x0828u32, 0x082C, 0x0830] {
            assert_eq!(runner.bus.read_word(off), 123u16, "v at ${:04X}", off);
            assert_eq!(runner.bus.read_word(off + 2), 456u16, "h at ${:04X}", off);
        }
    }

    /// Running a `DIVU.W D0,D1` with `D0 = 0` must not halt the
    /// runner. The `load_app_generic` loader installs an RTE stub at
    /// `$00FE` and points vector 5 (`$14`) at it; the m68k crate's
    /// zero-divide trap stacks the *next* PC and jumps to that vector,
    /// so RTE-ing returns past the DIVU and execution continues.
    /// Inside Macintosh Volume I, I-103 (Exception Vector Table);
    /// M68000PRM ("If the source operand is zero, the result of the
    /// operation is unpredictable").
    #[test]
    fn zero_divide_rte_handler_resumes_after_divu_by_zero() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        // Mirror what load_app_generic installs: RTE stub + vector.
        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x0014, 0x0000_00FE);

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x82C0); // DIVU.W D0, D1
        runner.bus.write_word(prog + 2, 0x4E71); // NOP
        runner.bus.write_word(prog + 4, 0x4E71); // NOP

        runner.cpu.write_reg(Register::PC, prog);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.cpu.write_reg(Register::D0, 0);
        runner.cpu.write_reg(Register::D1, 100);

        // 1 step: DIVU.W traps, vectors to $00FE.
        // 2nd step: RTE at $00FE pops SR/PC, returns past DIVU.
        // 3rd step: NOP at prog+2.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on zero-divide");
        assert_eq!(steps, 3);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            prog + 4,
            "PC must advance past the DIVU+NOP without re-entering the trap"
        );
        assert_eq!(
            runner.cpu.read_reg(Register::D1),
            100,
            "DIVU by zero must leave the destination register unchanged"
        );
    }

    /// CHK exception (vector 6) shares the same `$00FE` RTE stub as
    /// the zero-divide handler. A `CHK.W #5, D0` with `D0 = 100`
    /// exceeds the bound and triggers the trap; on a real Mac the
    /// handler calls SysError, on Systemless we silently RTE so D0 is
    /// preserved and the next instruction runs.
    /// Inside Macintosh Volume I, I-103.
    #[test]
    fn chk_rte_handler_resumes_after_bounds_violation() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x0018, 0x0000_00FE); // CHK vector

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x41BC); // CHK.W #imm, D0
        runner.bus.write_word(prog + 2, 0x0005); // imm = 5
        runner.bus.write_word(prog + 4, 0x4E71); // NOP

        runner.cpu.write_reg(Register::PC, prog);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.cpu.write_reg(Register::D0, 100);

        // 1 step: CHK fires (100 > 5), vectors to $00FE.
        // 2nd step: RTE pops SR/PC, returns past CHK.
        // 3rd step: NOP executes.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on CHK bounds violation");
        assert_eq!(steps, 3);
        assert_eq!(
            runner.cpu.read_reg(Register::PC),
            prog + 6,
            "PC must advance past CHK (4 bytes) + NOP (2 bytes)"
        );
        assert_eq!(runner.cpu.read_reg(Register::D0), 100);
    }

    /// TRAPV (vector 7) shares the `$00FE` RTE stub. Pre-set the V
    /// flag in CCR via the m68k API and execute TRAPV; the trap fires
    /// because V is set, vectors to the RTE stub, and resumes at the
    /// next instruction. Inside Macintosh Volume I, I-103.
    #[test]
    fn trapv_rte_handler_resumes_when_v_flag_is_set() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x001C, 0x0000_00FE); // TRAPV vector

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x4E76); // TRAPV
        runner.bus.write_word(prog + 2, 0x4E71); // NOP

        runner.cpu.write_reg(Register::PC, prog);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.cpu.core.set_ccr(0x02); // V flag set

        // 1: TRAPV traps; 2: RTE; 3: NOP.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on TRAPV");
        assert_eq!(steps, 3);
        assert_eq!(runner.cpu.read_reg(Register::PC), prog + 4);
    }

    /// `set_mouse_position` does NOT modify MBState ($0172) — it's a
    /// move-without-button-change, so the button-state byte should
    /// retain its prior value. The default at runner construction is
    /// 0x80 (button up).
    #[test]
    fn set_mouse_position_leaves_mb_state_untouched() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_byte(0x0172, 0x80);
        runner.set_mouse_position(50, 60);
        assert_eq!(runner.bus.read_byte(0x0172), 0x80);

        runner.bus.write_byte(0x0172, 0x00);
        runner.set_mouse_position(70, 80);
        assert_eq!(runner.bus.read_byte(0x0172), 0x00);
    }

    /// `push_mouse_down` must update MBState ($0172) to 0x00 (button
    /// pressed) immediately AND sync the position globals so guest
    /// code that polls these bytes directly sees the click without
    /// waiting for the next tick advance.
    /// Inside Macintosh Volume I, I-258 (MTemp/RawMouse/Mouse);
    /// Inside Macintosh Volume II, II-371 (MBState polling).
    #[test]
    fn push_mouse_down_writes_mb_state_pressed_and_position() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.bus.write_byte(0x0172, 0x80); // start "button up"

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x00,
            "MBState must be 0x00 (pressed) immediately after push_mouse_down"
        );
        // All three position globals must mirror the click site so
        // games that poll them directly (Mouse $0830 etc.) see the
        // correct location, not the prior cursor-park position.
        assert_eq!(runner.bus.read_word(0x0828), 123u16);
        assert_eq!(runner.bus.read_word(0x082A), 456u16);
        assert_eq!(runner.bus.read_word(0x082C), 123u16);
        assert_eq!(runner.bus.read_word(0x082E), 456u16);
        assert_eq!(runner.bus.read_word(0x0830), 123u16);
        assert_eq!(runner.bus.read_word(0x0832), 456u16);
    }

    /// `push_mouse_up` must update MBState ($0172) to 0x80 (button
    /// released) immediately. On real hardware the ADB polls at ~200 Hz
    /// so the latency between physical release and MBState=0x80 is a
    /// few ms; deferring to advance_guest_tick (~16 ms) makes
    /// frame-rate-dependent games read the wrong button state for too
    /// many loop iterations after click-up. This test pins the
    /// immediate-sync contract documented at runner.rs `push_mouse_up`.
    #[test]
    fn push_mouse_up_writes_mb_state_released_immediately() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        assert_eq!(runner.bus.read_byte(0x0172), 0x00);

        runner.push_mouse_up(10, 20);
        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x80,
            "MBState must flip back to 0x80 (released) immediately on push_mouse_up — \
             not deferred to the next tick"
        );
    }

    /// Regression: advance_guest_tick must NOT keep MBState at 0x00
    /// when both mouseDown and a paired mouseUp are queued and
    /// unconsumed. Polling-only games (Bonkheads-Deluxe class) never
    /// call GetNextEvent — the queue accumulates indefinitely.
    /// Pre-fix, the "any pending mouseDown → pressed" override left
    /// $0172 stuck at 0x00 forever, so Button() always returned TRUE
    /// and click detection broke silently. The fix counts unmatched
    /// mouseDowns (mouseDown count − mouseUp count) and only treats
    /// those as "still pressed".
    #[test]
    fn mb_state_releases_when_paired_mouseup_queued_but_unconsumed() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        runner.push_mouse_up(10, 20);
        // Both events still queued (no GetNextEvent has run). Drive the
        // tick boundary that owns the MBState resync.
        runner.advance_guest_tick();

        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x80,
            "advance_guest_tick must release MBState to 0x80 once a \
             paired mouseUp is queued behind the mouseDown — even when \
             nothing has drained the event queue"
        );
        // Sanity-check the events ARE still in the queue (this test is
        // about MBState despite the unconsumed events, not about queue
        // state). The dispatcher field is pub(crate); read it through
        // the same accessor used by the production sync logic.
        assert!(
            runner.dispatcher.event_queue.iter().any(|e| e.what == 1),
            "mouseDown event must remain in the queue (would be drained by GetNextEvent)"
        );
        assert!(
            runner.dispatcher.event_queue.iter().any(|e| e.what == 2),
            "mouseUp event must remain in the queue"
        );
    }

    /// Mirror of `mb_state_releases_when_paired_mouseup_queued_but_unconsumed`:
    /// a SOLO mouseDown queued without a paired mouseUp must still pin
    /// MBState to 0x00 across tick boundaries. This preserves the
    /// original contract — code that hasn't yet started polling when
    /// the click was injected gets at least one TRUE pulse — without
    /// regressing into the stuck-pressed bug.
    #[test]
    fn mb_state_stays_pressed_with_solo_pending_mousedown() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        runner.advance_guest_tick();
        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x00,
            "MBState must stay pressed across a tick advance while only \
             a mouseDown is queued (no paired mouseUp yet)"
        );
    }

    #[test]
    fn set_menu_bar_visible_round_trips_through_public_api() {
        // Pins the FixtureRunner::set_menu_bar_visible / menu_bar_visible
        // pair as the public-API entry point for the kiosk-mode toggle.
        // Library embedders should not need to reach through
        // dispatcher_mut() into TrapDispatcher::menu_bar_hidden — the
        // method-based surface keeps the kiosk-on-by-default contract
        // discoverable from the FixtureRunner type alone.
        //
        // Default (constructor): kiosk on → menu bar NOT visible.
        // After set_menu_bar_visible(true): menu bar IS visible.
        // After set_menu_bar_visible(false): kiosk back on.
        // Skip when SYSTEMLESS_SHOW_MENU_BAR is set in the test env —
        // the env var pre-seeds menu_bar_hidden = false at construction
        // and would race the round-trip assertion.
        if std::env::var_os("SYSTEMLESS_SHOW_MENU_BAR").is_some() {
            return;
        }
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        assert!(
            !runner.menu_bar_visible(),
            "kiosk default: menu_bar_visible() must report false"
        );
        runner.set_menu_bar_visible(true);
        assert!(
            runner.menu_bar_visible(),
            "after set_menu_bar_visible(true), menu_bar_visible() must report true"
        );
        runner.set_menu_bar_visible(false);
        assert!(
            !runner.menu_bar_visible(),
            "after set_menu_bar_visible(false), menu_bar_visible() must report false"
        );
        // Internal field stays in sync with the public API — guards
        // against future refactors that introduce a parallel state
        // field but forget to wire it through the toggle.
        assert!(
            runner.dispatcher().menu_bar_hidden,
            "set_menu_bar_visible(false) must clear the kiosk-bypass bit"
        );
    }

    #[test]
    fn disassemble_at_decodes_known_opcodes_with_correct_advance() {
        // Pins the FixtureRunner::disassemble_at public-API helper.
        // This is the library-level entry point for pixel-divergence
        // and trap-misroute investigations: pair with
        // SYSTEMLESS_TRACE_FB_WRITE_RANGE to see what code lives at a
        // suspect PC.
        //
        // Seed three known instructions in guest RAM, disassemble,
        // and verify:
        //   1. each entry's PC advances by the previous size
        //   2. the mnemonic for $4E71 is "NOP" (well-known fixed
        //      instruction; no operand words to consume)
        //   3. an A-line trap word ($A8EC = CopyBits) comes back as
        //      "DC.W $A8EC" — the m68k crate's convention for opcodes
        //      it doesn't have a regular decoder for
        //   4. the size returned is at least 2 and at most 10 (the
        //      clamp guard that prevents a malformed opcode from
        //      consuming wrap-around amounts)
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let pc = 0x10000u32;
        // $4E71 NOP
        runner.bus.write_word(pc, 0x4E71);
        // $A8EC (CopyBits trap-line word)
        runner.bus.write_word(pc + 2, 0xA8EC);
        // $4E71 NOP again
        runner.bus.write_word(pc + 4, 0x4E71);
        let out = runner.disassemble_at(pc, 3);
        assert_eq!(
            out.len(),
            3,
            "disassemble_at must return exactly count entries"
        );
        assert_eq!(
            out[0].0, pc,
            "first entry's PC must equal the requested start"
        );
        assert!(
            out[0].1.contains("NOP"),
            "$4E71 must disassemble to NOP, got: {}",
            out[0].1
        );
        assert!(
            out[0].2 >= 2 && out[0].2 <= 10,
            "instruction size must be in clamp range [2, 10], got {}",
            out[0].2
        );
        assert_eq!(
            out[1].0,
            pc + out[0].2,
            "second entry's PC must equal first PC + first size"
        );
        assert!(
            out[1].1.contains("$A8EC"),
            "A-line trap $A8EC must surface in mnemonic (DC.W form), got: {}",
            out[1].1
        );
        assert!(
            out[2].1.contains("NOP"),
            "third entry must be the second NOP we seeded"
        );
    }
}
