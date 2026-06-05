//! Fixture Runner - Loading and execution infrastructure

use crate::cpu::{M68kCpu, Register, StepResult};
use crate::loader::{Code0Header, CodeSegmentHeader, JumpTableEntry, LoadedApp};
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::TrapDispatcher;
use crate::{Error, Result};
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Cache env-var lookups (per-call syscall otherwise).
use std::sync::OnceLock;
static TRACE_DIALOG_FILTER: OnceLock<bool> = OnceLock::new();
static TRACE_TIMER: OnceLock<bool> = OnceLock::new();
static TRACE_VBL: OnceLock<bool> = OnceLock::new();

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

// Gate the per-instruction trace_buffer behind an env var. The buffer
// is populated on EVERY instruction fetch and only used from
// `dump_trace()` on halt/crash. Default-disabled saves per-instruction
// `VecDeque` pop_front + push_back + an extra `bus.read_word` + 6
// register reads. Enable with `SYSTEMLESS_TRACE_BUFFER=1` when diagnosing
// a crash.
static TRACE_BUFFER_ENABLED: OnceLock<bool> = OnceLock::new();
fn trace_buffer_enabled() -> bool {
    *TRACE_BUFFER_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_BUFFER").is_some())
}

static TRACE_PC_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
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

fn trace_pc_range_contains(pc: u32) -> bool {
    trace_pc_range()
        .map(|(start, end)| pc >= start && pc <= end)
        .unwrap_or(false)
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
static TRACE_OPCODE_COUNTS: OnceLock<bool> = OnceLock::new();
fn trace_opcode_counts_enabled() -> bool {
    *TRACE_OPCODE_COUNTS
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_OPCODE_COUNTS").is_some())
}

// Sampled PC histogram, opt-in via `SYSTEMLESS_TRACE_HOT_PC=1`. Every
// 1000th step's PC increments a `HashMap` entry. Use to locate the
// code address of a hot game loop. 1/1000 sampling keeps `HashMap`
// overhead negligible while still giving high-confidence attribution
// for any loop that takes more than ~0.1% of runtime.
const PC_SAMPLE_INTERVAL: u64 = 1000;
static TRACE_HOT_PC: OnceLock<bool> = OnceLock::new();
fn trace_hot_pc_enabled() -> bool {
    *TRACE_HOT_PC.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_HOT_PC").is_some())
}

// Generic TickCount spin-wait fast-forward. Tri-state default:
// headless callers (scripted harnesses, no real-time pacing) get the fast-
// forward by default — they gain the spin-elimination win. GUI
// callers (`yield_for_ui = true`) get the fast-forward OFF by default
// because tick-driven animations look wrong when ticks advance in
// batches. Two env vars override:
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
/// Tri-state precedence:
///   1. force-off env wins.
///   2. force-on env wins next.
///   3. default = headless (yield_for_ui = false) gets it on,
///      GUI (yield_for_ui = true) gets it off.
fn spin_wait_fastfwd_enabled_for(yield_for_ui: bool) -> bool {
    spin_wait_fastfwd_gate(
        spin_wait_fastfwd_force_on(),
        spin_wait_fastfwd_force_off(),
        yield_for_ui,
    )
}

/// Pure decision function for the tri-state gate. Split out from
/// `spin_wait_fastfwd_enabled_for` so the env-var reads can be mocked
/// in unit tests (the `OnceLock`-based env caches initialise once per
/// process and would prevent testing all three modes in one test
/// run).
fn spin_wait_fastfwd_gate(force_on: bool, force_off: bool, yield_for_ui: bool) -> bool {
    if force_off {
        return false;
    }
    if force_on {
        return true;
    }
    !yield_for_ui
}

/// Pure decision function for the ModalDialog noop-refire skip. ALL
/// of these must be true for the skip to fire:
///   - mode is headless (`yield_for_ui = false`)
///   - dialog tracking is active (`has_tracking = true`)
///   - no filter proc installed (`filter_proc_zero`)
///   - no button flash animating (`flash_remaining_zero`)
///   - initial draw procs all completed (`draw_procs_done`)
///   - dialog pixels already captured (`rendered_pixels_final`)
///   - event queue is empty (no input pending)
fn modaldialog_refire_is_noop(
    yield_for_ui: bool,
    has_tracking: bool,
    filter_proc_zero: bool,
    flash_remaining_zero: bool,
    draw_procs_done: bool,
    rendered_pixels_final: bool,
    event_queue_empty: bool,
) -> bool {
    !yield_for_ui
        && has_tracking
        && filter_proc_zero
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
// Cap how many ticks the fast-forward will advance in one shot,
// to protect against pathological target values (e.g. overflowed
// unsigned register values being misinterpreted as huge-future
// ticks). If the cap trips, we fall back to normal spin — still
// correct, just not fast.
const SPIN_FASTFWD_MAX_TICKS: u32 = 1_000_000;

/// Outcome of `advance_until_tick`. Used to distinguish the "we
/// advanced, please synthesise the exit state" happy path from
/// the two abort paths: tick_cap reached (caller must break the
/// outer run loop) and pathological target difference (caller
/// must NOT synthesise — let the guest spin normally).
enum AdvanceResult {
    Advanced,
    CapHit,
    TooFar,
}

// Layout for dialog callback scratch region.
const DIALOG_DRAW_TRAMPOLINE_OFFSET: u32 = 0x00;
const DIALOG_FILTER_TRAMPOLINE_OFFSET: u32 = 0x40;
const DIALOG_FILTER_EVENT_OFFSET: u32 = 0x80;
// 2-byte scratch where the filter trampoline writes its Boolean return value.
const DIALOG_FILTER_RESULT_OFFSET: u32 = 0x96;
const DIALOG_CALLBACK_SCRATCH_FALLBACK: u32 = 0x0000_1200;
/// Compact Mac video hardware refreshes at approximately 60.15 Hz.
pub const DEFAULT_VBL_HZ: f64 = 60.15;
/// Default emulated CPU speed for realtime frontends (Mac IIci-class 68030).
pub const DEFAULT_REALTIME_CPU_MHZ: f64 =
    crate::machine_profile::ORACLE_MACHINE_PROFILE.realtime_cpu_mhz;
/// Shared default realtime CPU budget used by the GUI runner and scripted
/// realtime mode so both frontends expose the same machine profile.
pub const DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND: f64 = DEFAULT_REALTIME_CPU_MHZ * 1_000_000.0;
// Default instructions per VBL tick for non-realtime execution (scripted harnesses, tests).
// Realtime frontends override this via set_instructions_per_tick() to match the
// shared default machine profile defined above.
// This lower value lets scripted harnesses run quickly without being wall-clock-paced.
const INSTRUCTIONS_PER_TICK: u32 = 12_000;
const MAC_EPOCH_OFFSET_FROM_UNIX: u64 = 2_082_844_800;

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
    SoundCallback,
    SoundFileCompletion,
    SoundDoubleBack,
    DialogDrawProc,
}

#[derive(Clone, Copy, Debug)]
struct ActiveInterruptCallback {
    source: ActiveInterruptCallbackSource,
    resume_pc: u32,
    resume_sp: u32,
    d_regs: [u32; 8],
    a_regs: [u32; 8],
    ccr: u8,
    restore_port: Option<(u32, u32)>,
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
}

impl Default for FixtureRunnerConfig {
    fn default() -> Self {
        Self {
            max_instructions: 10_000_000,
            load_address: 0x10000,
            arrows_as_numpad: false,
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
///    expose per-halt detail.
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
    /// Guest-memory address of a scratch EventRecord passed to ModalDialog filters.
    dialog_filter_event: u32,
    /// Override for the application's startup time in Mac-epoch seconds.
    /// Used by scripted frontends to keep guest-visible time deterministic.
    app_start_time: Option<u32>,
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
        let dispatcher = TrapDispatcher::new();
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
            active_interrupt_callback: None,
            audio: None,
            audio_buffer: Vec::new(),
            sound_doubleback_trampoline: 0,
            sound_callback_trampoline: 0,
            sound_file_completion_trampoline: 0,
            dialog_draw_trampoline: 0,
            dialog_filter_trampoline: 0,
            dialog_filter_event: 0,
            app_start_time: None,
            opcode_histogram: Box::new([0u64; 65536]),
            pc_histogram: HashMap::new(),
        }
    }

    /// Returns true if the application has called ExitToShell.
    pub fn is_halted(&self) -> bool {
        self.halted
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

    /// Getter for the pinned Mac epoch seconds. Returns `None` when no
    /// pin has been applied — without a pin, `init_app` falls back to
    /// `current_mac_epoch_seconds()` (host wall-clock), which leaks
    /// into the guest's `Time` global (`$020C`) and breaks
    /// reproducibility.
    pub fn app_start_time(&self) -> Option<u32> {
        self.app_start_time
    }

    pub fn enable_oracle_recording(
        &mut self,
        output_dir: impl AsRef<std::path::Path>,
        source: crate::oracle::OracleSource,
    ) -> Result<()> {
        self.dispatcher.enable_oracle_recording(output_dir, source)
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

    /// Move the mouse without changing the button state. Updates the
    /// dispatcher's tracked position and the six mouse-position
    /// low-memory globals (MTemp / RawMouse / Mouse) so guest code that
    /// reads them directly sees the new coordinates immediately. Leaves
    /// MBState ($0172) untouched. Inside Macintosh Volume II, II-371.
    pub fn set_mouse_position(&mut self, v: i16, h: i16) {
        self.dispatcher.set_mouse_position(v, h);
        self.sync_mouse_position_lowmem();
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

    /// Inject a key-down event, applying arrow→numpad remapping if configured.
    pub fn push_key_down(&mut self, mac_key: u8, char_code: u8) {
        let key = self.remap_key(mac_key);
        self.dispatcher.push_key_down(key, char_code);
    }

    /// Inject a key-up event, applying arrow→numpad remapping if configured.
    pub fn push_key_up(&mut self, mac_key: u8, char_code: u8) {
        let key = self.remap_key(mac_key);
        self.dispatcher.push_key_up(key, char_code);
    }

    /// Remap arrow key virtual key codes to numpad equivalents when enabled.
    /// Arrow keys: Left=0x7B, Right=0x7C, Down=0x7D, Up=0x7E
    /// Numpad dirs: 4(left)=0x56, 6(right)=0x58, 5(down)=0x57, 8(up)=0x5B
    /// Inside Macintosh Volume V, V-191
    fn remap_key(&self, mac_key: u8) -> u8 {
        if self.config.arrows_as_numpad {
            match mac_key {
                0x7B => 0x56, // Left  → Numpad4
                0x7C => 0x58, // Right → Numpad6
                0x7D => 0x57, // Down  → Numpad5
                0x7E => 0x5B, // Up    → Numpad8
                _ => mac_key,
            }
        } else {
            mac_key
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
        // Load resources into dispatcher for GetResource trap
        self.dispatcher.load_resources(fork, &mut self.bus);

        let app = load_app_generic(fork, &mut self.bus, self.config.load_address);

        if let Some(ref loaded_app) = app {
            let segments: HashMap<i16, u32> = loaded_app
                .segment_bases
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect();
            self.dispatcher.register_segments(segments);
        }

        app
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
        // Initialize Ticks ($016A) to a realistic post-boot value.
        // On a real Mac, hundreds of ticks elapse during the boot ROM,
        // system extensions, and Finder startup before the application
        // launches. Games that read Ticks early (e.g. to seed a PRNG)
        // expect a non-zero value; starting at 0 produces degenerate
        // random sequences (e.g., ship heading always zero in EV).
        // 600 ticks ≈ 10 seconds of post-boot time, a conservative
        // estimate for a minimal System 7 configuration.
        self.bus.write_long(addr::TICKS, 0);
        let time = self
            .app_start_time
            .unwrap_or_else(current_mac_epoch_seconds);
        self.bus.write_long(addr::TIME, time);
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
        // MBarHeight: 20 pixels (standard Roman system script value).
        // Games may set this to 0 to hide the menu bar for full-screen mode.
        // Inside Macintosh Volume V, V-245
        self.bus.write_word(addr::MBAR_HEIGHT, 20);
        // SoundLevel ($0260): Sound Driver current PCM byte. Inside
        // Macintosh: Sound 1994 (Sound Driver chapter). Non-zero when
        // audio is being emitted; zero when idle. Some classic apps
        // read this byte directly as a "Sound Driver present and emitting"
        // sentinel — most notably Marathon 1, whose sound module
        // (CODE 5 +$0003F2: `MOVE.B (mem $260).W, (A0)`) short-circuits
        // its entire audio submission path when this byte is zero.
        // Systemless's HLE skips the legacy Sound Driver layer (we mix PCM
        // directly via the SoundManager), so this byte was previously
        // always zero — falsifying classic Sound-Driver-presence checks.
        // Initialize to 1 (the minimum non-zero) so SoundLevel-readers
        // see "driver alive".
        self.bus.write_byte(addr::SOUND_LEVEL, 1);

        // Memory Manager zone globals
        // Inside Macintosh Volume II, II-19
        // The heap starts at 0x200000 (set in bus.rs). ApplLimit is set
        // to allow the heap to grow up to the stack minus a safety margin.
        // BufPtr sits between ApplLimit and the stack for sound/disk buffers.
        let heap_start: u32 = 0x200000;
        let stack_base = app.initial_sp;
        let appl_limit = stack_base - 0x2000; // 8KB stack safety margin
        let buf_ptr = appl_limit; // Buffer area at the limit
        self.bus.write_long(addr::SYS_ZONE, heap_start);
        self.bus.write_long(addr::APP_L_ZONE, heap_start);
        self.bus.write_long(addr::HEAP_END, appl_limit);
        self.bus.write_long(addr::APPL_LIMIT, appl_limit);
        self.bus.write_long(addr::BUF_PTR, buf_ptr);
        self.bus.write_long(addr::THE_ZONE, heap_start);

        // CurApRefNum: resource file reference number of the application (word).
        // CurApName: application name as Pascal string (Str31).
        // Inside Macintosh Volume II, II-57 to II-58
        self.bus.write_word(addr::CUR_APREF_NUM, 0); // app resources use refnum 0
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

        // Zone header at heap_start (Inside Macintosh Volume II, II-22)
        // Apps and the Memory Manager read the zone header to determine
        // available memory. zcbFree (offset +12) must reflect free bytes.
        // Reserve heap space so alloc() doesn't overwrite the zone header.
        let zone_header_size: u32 = 64;
        self.bus.reserve_heap(zone_header_size);
        let zone_size = appl_limit - heap_start;
        let free_bytes = zone_size - zone_header_size;
        self.bus.write_long(heap_start, appl_limit); // bkLim: end of zone
        self.bus
            .write_long(heap_start + 8, heap_start + zone_header_size); // hFstFree
        self.bus.write_long(heap_start + 12, free_bytes); // zcbFree: total free
        self.bus
            .write_long(heap_start + 56, heap_start + zone_header_size); // allocPtr
        eprintln!(
            "[INIT] Zone header: start=${:08X} bkLim=${:08X} zcbFree={} ({:.1}MB)",
            heap_start,
            appl_limit,
            free_bytes,
            free_bytes as f64 / (1024.0 * 1024.0)
        );

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

    fn queue_mixed_audio(&mut self, samples: &[u8]) {
        if samples.is_empty() {
            return;
        }
        if let Some(ref mut audio) = self.audio {
            audio.queue_samples(samples);
        }
        self.audio_buffer.extend_from_slice(samples);
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

            let samples = self.dispatcher.sound_manager.mix_frame(chunk);
            if samples.is_empty() {
                break;
            }
            self.queue_mixed_audio(&samples);
            remaining_samples -= chunk;
        }
    }

    fn finish_host_frame(&mut self, audio_samples: usize) {
        // Redraw menu bar and window chrome after each frame.
        // On a real Mac the Window Manager maintains these as
        // separate layers; here they are raw framebuffer pixels
        // that game drawing (explosions, etc.) can overwrite.
        self.dispatcher.redraw_chrome(&mut self.bus);

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

        self.fire_sound_callbacks();

        // Fire pending double-buffer callbacks (SndPlayDoubleBuffer).
        self.fire_sound_doubleback_callbacks();
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
        // Step 1: MOVE.L (A7)+, Dn (shared by all templates).
        let w0 = self.bus.read_word(pc_after_trap);
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

        false
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
            AdvanceResult::TooFar => return false,
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
            AdvanceResult::TooFar => return false,
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
        }
        AdvanceResult::Advanced
    }

    fn run_steps_internal(
        &mut self,
        max_steps: usize,
        tick_cap: Option<u32>,
        audio_samples: usize,
        yield_for_ui: bool,
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

        while count < max_steps && !self.halted && !tick_cap_reached {
            // Sound callbacks are interrupt work. If a previous slice queued
            // one, dispatch it before running more foreground guest code.
            if self.active_interrupt_callback.is_none() {
                self.fire_sound_callbacks();
                self.fire_sound_doubleback_callbacks();
            }

            // Service blocking traps (Delay, WaitNextEvent sleep).
            if self.service_wait_sleep_ticks(tick_cap) {
                break;
            }
            if self.service_delay_ticks(tick_cap) {
                break;
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
                    self.cpu.core.set_ccr(active_interrupt_callback.ccr);
                    if let Some((port, gdevice)) = active_interrupt_callback.restore_port {
                        self.dispatcher.set_current_port_state(
                            &mut self.bus,
                            &mut self.cpu,
                            port,
                            Some(gdevice),
                        );
                    }
                    if matches!(
                        active_interrupt_callback.source,
                        ActiveInterruptCallbackSource::DialogDrawProc
                    ) {
                        self.dispatcher
                            .finalize_dialog_draw_procs_if_idle(&mut self.bus);
                    }
                    self.active_interrupt_callback = None;
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

            // Tick advancement: deduct 1 instruction; when the budget hits
            // zero, advance $016A and refill from instructions_per_tick.
            self.tick_budget -= 1;
            while self.tick_budget <= 0 && self.frozen_ticks.is_none() {
                if let Some(cap) = tick_cap {
                    if self.bus.read_long(0x016A) >= cap {
                        tick_cap_reached = true;
                        break;
                    }
                }
                self.advance_guest_tick();
                self.tick_budget += self.instructions_per_tick as i32;
            }
            if tick_cap_reached {
                break;
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

            // Mirror PC for [FB-WRITE] trace; watchpoint above is debug-only.
            if crate::memory::bus::fb_write_trace_active() {
                crate::memory::bus::set_current_pc(pc);
            }

            if trace_pc_range_contains(pc) {
                eprintln!(
                    "[TRACE-PC-RANGE] pc=${:08X} op=${:04X} sp=${:08X} a6=${:08X} a5=${:08X} d0=${:08X} a0=${:08X}",
                    pc,
                    self.bus.read_word(pc),
                    self.cpu.read_reg(Register::A7),
                    self.cpu.read_reg(Register::A6),
                    self.cpu.read_reg(Register::A5),
                    self.cpu.read_reg(Register::D0),
                    self.cpu.read_reg(Register::A0),
                );
            }

            // Execute one instruction.
            match self.cpu.step(&mut self.bus) {
                StepResult::Ok => {
                    count += 1;
                    self.total_instructions = self.total_instructions.wrapping_add(1);
                    // Populate opcode histogram if enabled. cpu.core.ir
                    // holds the last-fetched opcode (set by step's
                    // read_imm_16). Only counts successful non-trap
                    // steps here; A-line opcodes go through the trap
                    // histogram instead. Zero cost when disabled
                    // (cached bool).
                    if trace_opcode_counts_enabled() {
                        let opcode = self.cpu.core.ir as u16 as usize;
                        self.opcode_histogram[opcode] =
                            self.opcode_histogram[opcode].saturating_add(1);
                    }
                    // Sampled PC histogram. 1/1000 sampling keeps the
                    // HashMap cost bounded.
                    if trace_hot_pc_enabled()
                        && self.total_instructions.is_multiple_of(PC_SAMPLE_INTERVAL)
                    {
                        *self.pc_histogram.entry(pc).or_insert(0) += 1;
                    }
                }
                StepResult::Stopped => {
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
                    return (count, false);
                }
                StepResult::Aline(opcode) => {
                    count += 1;
                    self.total_instructions = self.total_instructions.wrapping_add(1);

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

                        // Generic TickCount spin-wait fast-forward (opt-in).
                        // Check post-trap bytes against the spin-wait
                        // template; if matched, skip straight past the
                        // loop. m68k's step() advances PC past the A-trap
                        // (read_imm_16 does pc += 2), so the post-trap
                        // PC is already at pc + 2.
                        if spin_wait_fastfwd_enabled_for(yield_for_ui) {
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
                            filter_proc_zero,
                            flash_remaining_zero,
                            draw_procs_done,
                            rendered_pixels_final,
                        ) = self
                            .dispatcher
                            .dialog_tracking
                            .as_ref()
                            .map(|t| {
                                (
                                    true,
                                    t.filter_proc == 0,
                                    t.flash_remaining == 0,
                                    t.draw_procs_done,
                                    t.rendered_pixels_final,
                                )
                            })
                            .unwrap_or((false, false, false, false, false));
                        let noop_refire = modaldialog_refire_is_noop(
                            yield_for_ui,
                            has_tracking,
                            filter_proc_zero,
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
                                self.tick_budget -= 1;
                                while self.tick_budget <= 0 && self.frozen_ticks.is_none() {
                                    if let Some(cap) = tick_cap {
                                        if self.bus.read_long(0x016A) >= cap {
                                            tick_cap_reached = true;
                                            break;
                                        }
                                    }
                                    self.advance_guest_tick();
                                    self.tick_budget += self.instructions_per_tick as i32;
                                }
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

                    match self
                        .dispatcher
                        .dispatch(opcode, &mut self.cpu, &mut self.bus)
                    {
                        Ok(()) => {
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
                                self.cpu.write_reg(Register::PC, pc);

                                // Fire pending dialog userItem draw procs.
                                // The trampoline redirects PC to execute the
                                // 68K draw proc; when it RTS's, PC returns to
                                // the ModalDialog A-line for the next re-fire.
                                let fired_draw_proc = self.fire_dialog_draw_procs();
                                let mut fired_filter_proc = false;
                                if !fired_draw_proc {
                                    // Fire the filter proc for any dialog that has one,
                                    // once draw procs are complete. On a real Mac,
                                    // ModalDialog calls the filter for every event
                                    // (including null events) regardless of item types.
                                    // Inside Macintosh Volume I, I-415
                                    let should_fire_filter = self
                                        .dispatcher
                                        .dialog_tracking
                                        .as_ref()
                                        .map(|t| t.filter_proc != 0 && t.draw_procs_done)
                                        .unwrap_or(false);
                                    if should_fire_filter {
                                        self.fire_dialog_filter_proc();
                                        fired_filter_proc = true;
                                    }
                                }

                                // In realtime frontends, yield only once the
                                // tracking trap is idle. A just-scheduled
                                // dialog draw/filter proc has not run yet, so
                                // presenting here shows half-painted screens.
                                // Headless mode keeps executing as before.
                                if yield_for_ui && !fired_draw_proc && !fired_filter_proc {
                                    self.finish_host_frame(audio_samples);
                                    return (count, true);
                                }
                            }
                            // If tracking just ended this trap (MenuSelect or
                            // ModalDialog completed), unfreeze ticks and snap
                            // $016A to wall-clock time so the game doesn't
                            // fast-forward through the pause gap.
                            if self.frozen_ticks.is_some() {
                                self.frozen_ticks = None;
                                if let Some(real_target) = real_tick_cap {
                                    self.bus.write_long(0x016A, real_target);
                                    // Keep `dispatcher.tick_count` in sync
                                    // with $016A. `advance_guest_tick`
                                    // does this; without this assignment
                                    // the unfreeze path would write the
                                    // bus but leave `self.tick_count`
                                    // stale, breaking the TickCount
                                    // handler's fast path.
                                    self.dispatcher.tick_count = real_target;
                                }
                            }

                            // Service any pending Delay ticks immediately after
                            // the trap dispatch, before the next instruction.
                            self.service_delay_ticks(tick_cap);
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

        self.finish_host_frame(audio_samples);

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
        self.run_steps_internal(max_steps, None, audio_samples, true)
    }

    /// Run a GUI frame slice paced by wall-clock time.
    ///
    /// Wall-clock GUI pacing works differently from the reference-runtime oracle:
    /// in the oracle, ticks are driven purely by the instruction budget
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
        self.run_steps_internal(max_steps, Some(deadline_tick), audio_samples, true)
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
        self.run_steps_internal(
            max_steps,
            tick_override,
            crate::sound::OUTPUT_RATE as usize / 60,
            false,
        )
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
        let mut unmatched_mousedowns: i32 = 0;
        for e in self.dispatcher.event_queue.iter() {
            match e.what {
                1 => unmatched_mousedowns += 1, // mouseDown
                2 => unmatched_mousedowns -= 1, // mouseUp pairs off the previous mouseDown
                _ => {}
            }
        }
        let has_pending_unmatched_down = unmatched_mousedowns > 0;
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

        // Fire vertical-retrace tasks before Time Manager tasks. Games commonly
        // drive screen/audio housekeeping from VBL, so letting those callbacks
        // run first avoids starving them behind unrelated timer traffic.
        self.fire_vbl_tasks();
        self.fire_timer_tasks(new_tick);
        new_tick
    }

    fn service_wait_sleep_ticks(&mut self, tick_cap: Option<u32>) -> bool {
        if self.dispatcher.pending_wait_sleep_ticks == 0 || self.active_interrupt_callback.is_some()
        {
            return false;
        }

        if self.frozen_ticks.is_some() {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            return false;
        }

        // On a real Mac, WaitNextEvent returns immediately when an event
        // is available, regardless of the requested sleep duration.
        // Macintosh Toolbox Essentials 1992, 2-22
        if !self.dispatcher.event_queue.is_empty() {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            return false;
        }

        // In GUI mode (tick_cap present), cap the effective sleep to 1 tick.
        // The sleep parameter in WaitNextEvent is a hint to the OS about how
        // long the app is willing to wait; on a real Mac the Process Manager
        // may return sooner. Sleeping for the full duration (often 60 ticks =
        // 1 second) starves the game loop of CPU time. Advancing 1 tick per
        // WaitNextEvent call matches the behavior of a 60 Hz VBL-paced game.
        if let Some(cap) = tick_cap {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            if self.bus.read_long(0x016A) < cap {
                self.advance_guest_tick();
                self.tick_budget = self.instructions_per_tick as i32;
            }
            // Yield to the host if we've reached the tick cap for this frame,
            // otherwise let the game continue processing.
            if self.bus.read_long(0x016A) >= cap {
                return true;
            }
            return false;
        }

        // Headless mode (no tick_cap from caller).
        //
        // Default: drain all pending sleep ticks at once (faster wall-clock).
        // Opt-in cap (set via `FixtureRunner::set_wait_sleep_cap_in_headless`):
        // honor the cap as a per-WNE-call ceiling, mirroring GUI mode's
        // 1-tick cap. Used by scripted harnesses to prevent Systemless's tick rate
        // from rocketing ahead of Basilisk's during event-loop-heavy
        // gameplay.
        if let Some(cap) = self.wait_sleep_cap_in_headless {
            let advance = self.dispatcher.pending_wait_sleep_ticks.min(cap);
            self.dispatcher.pending_wait_sleep_ticks = 0;
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

    /// Fire the next due Vertical Retrace Manager task.
    ///
    /// VBL tasks run at interrupt time with A0 pointing at the task record.
    /// Processes 1994, 4-6 to 4-7; executor src/time/vbl.cpp
    fn fire_vbl_tasks(&mut self) {
        if self.active_interrupt_callback.is_some() {
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
            let tramp = self.bus.alloc(22);
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
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Vbl,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            ccr,
            restore_port: None,
        });
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
            let buf_ptr = self.bus.read_long(header_ptr + 12 + (buf_idx as u32) * 4);
            if buf_ptr == 0 {
                continue;
            }
            let flags = self.bus.read_long(buf_ptr + 4);
            if flags & 0x01 == 0 {
                continue; // not ready yet
            }
            crate::trap::TrapDispatcher::load_double_buffer_samples(
                &mut self.bus,
                chan,
                buf_ptr,
                sample_rate,
                num_channels,
                sample_size,
            );
        }
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
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            ccr,
            restore_port: None,
        });
        self.cpu.write_reg(Register::PC, trampoline);
    }

    /// Fire pending Sound Manager callback procedures and file completion routines.
    fn fire_sound_callbacks(&mut self) {
        if self.active_interrupt_callback.is_some() {
            return;
        }

        if self
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .is_empty()
        {
            return;
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
                    return;
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
            }
            crate::sound::PendingSoundCallback::FileCompletion {
                callback_addr,
                chan_ptr,
            } => {
                if callback_addr == 0 {
                    return;
                }

                // Sound 1994, 2-151
                if self.sound_file_completion_trampoline == 0 {
                    let tramp = self.bus.alloc(22);
                    self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                    self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #chan,-(SP)
                    self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
                    self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,regs
                    self.bus.write_word(tramp + 18, 0x0F0F); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 20, 0x4E75); // RTS
                    self.sound_file_completion_trampoline = tramp;
                }

                let tramp = self.sound_file_completion_trampoline;
                self.bus.write_long(tramp + 6, chan_ptr);
                self.bus.write_long(tramp + 12, callback_addr);
                self.inject_interrupt_callback(
                    ActiveInterruptCallbackSource::SoundFileCompletion,
                    tramp,
                );
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
    fn fire_sound_doubleback_callbacks(&mut self) {
        if self.active_interrupt_callback.is_some() {
            return;
        }

        if self.dispatcher.sound_manager.pending_callbacks.is_empty() {
            return;
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
            self.bus.write_long(exhausted_buf_ptr + 4, flags & !0x01);
        }

        // Mark that we've dispatched the callback.
        if let Some(chan) = self.dispatcher.sound_manager.find_channel_mut(cb.chan_ptr) {
            if let Some(ref mut db) = chan.double_buffer {
                db.waiting_for_callback = false;
            }
        }

        if cb.callback_addr == 0 {
            return;
        }

        // Allocate trampoline on first use.
        // The doubleback proc is a Pascal procedure (callee pops params):
        //   PROCEDURE MyDoubleBackProc(chan: SndChannelPtr;
        //                              exhaustedBuffer: SndDoubleBufferPtr);
        //
        // Trampoline layout (28 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)  ; 48E7 F0F0 (save regs)
        //   +4:  MOVE.L  #chanPtr,-(SP)       ; 2F3C xxxx xxxx (push param1 first = deeper)
        //   +10: MOVE.L  #exhaustedBuf,-(SP) ; 2F3C xxxx xxxx (push param2 last = top)
        //   +16: JSR     callback             ; 4EB9 xxxx xxxx
        //   +22: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F (restore regs)
        //   +26: RTS                          ; 4E75
        if self.sound_doubleback_trampoline == 0 {
            let tramp = self.bus.alloc(28);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +6..+9: exhausted buf ptr (patched)
            self.bus.write_word(tramp + 10, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +12..+15: chan ptr (patched)
            self.bus.write_word(tramp + 16, 0x4EB9); // JSR abs.L
                                                     // +18..+21: callback addr (patched)
            self.bus.write_word(tramp + 22, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 24, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 26, 0x4E75); // RTS
            self.sound_doubleback_trampoline = tramp;
        }

        let tramp = self.sound_doubleback_trampoline;
        self.bus.write_long(tramp + 6, cb.chan_ptr);
        self.bus.write_long(tramp + 12, exhausted_buf_ptr);
        self.bus.write_long(tramp + 18, cb.callback_addr);

        // Doubleback procedures execute at interrupt time, so the interrupted
        // guest CPU state must be restored after the callback unwinds.
        // Sound 1994, 2-72
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
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::SoundDoubleBack,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            ccr,
            restore_port: None,
        });
        self.cpu.write_reg(Register::PC, tramp);
    }

    fn dialog_callback_scratch_base(&self) -> u32 {
        DIALOG_CALLBACK_SCRATCH_FALLBACK
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
    ///   2. Pushes params in Pascal order: theWindow, then itemNo
    ///      (callee cleans up)
    ///   3. JSR to draw proc address
    ///   4. Restores D0-D3/A0-A3
    ///   5. RTS back to interrupted code (the ModalDialog A-line)
    fn fire_dialog_draw_procs(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }

        let (proc_addr, item_no, dialog_ptr) = {
            let tracking = match self.dispatcher.dialog_tracking.as_mut() {
                Some(t) if !t.draw_procs_done => t,
                _ => return false,
            };
            match tracking.draw_proc_queue.pop_front() {
                Some((addr, num)) => (addr, num, tracking.dialog_ptr),
                None => {
                    // All draw procs fired and returned
                    tracking.draw_procs_done = true;
                    return false;
                }
            }
        };

        if proc_addr == 0 {
            return false;
        }

        // Many dialogs stuff non-code placeholders into userItem proc fields.
        // Only fire callbacks that look like real 68K entry points.
        let entry = self.bus.read_word(proc_addr);
        if entry != 0x4E56 && entry != 0x48E7 && entry != 0x4EF9 && entry != 0x4EFA {
            return false;
        }

        // Allocate trampoline on first use (26 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)   ; 48E7 F0F0
        //   +4:  MOVE.L  #dialogPtr,-(SP)      ; 2F3C xxxx xxxx
        //   +10: MOVE.W  #itemNo,-(SP)         ; 3F3C xxxx
        //   +14: JSR     proc_addr              ; 4EB9 xxxx xxxx
        //   +20: MOVEM.L (SP)+,D0-D3/A0-A3    ; 4CDF 0F0F
        //   +24: RTS                            ; 4E75
        // Pascal calling convention: callee pops the 6 bytes of params.
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
            self.bus.write_word(tramp + 20, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 22, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 24, 0x4E75); // RTS
            self.dialog_draw_trampoline = tramp;
        }

        let tramp = self.dialog_draw_trampoline;
        self.bus.write_long(tramp + 6, dialog_ptr);
        self.bus.write_word(tramp + 12, item_no as u16);
        self.bus.write_long(tramp + 16, proc_addr);

        let previous_port = self.dispatcher.current_port;
        let previous_gdevice = self.dispatcher.current_gdevice;
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
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::DialogDrawProc,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            ccr,
            restore_port: Some((previous_port, previous_gdevice)),
        });
        self.cpu.write_reg(Register::PC, tramp);
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

        // Inject callback execution.
        let current_pc = self.cpu.read_reg(Register::PC);
        let sp = self.cpu.read_reg(Register::A7);
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
        for addr in (clear_start..filter_entry_sp).step_by(4) {
            self.bus.write_long(addr, 0);
        }

        self.bus.write_long(tramp + 38, saved_sp);
        self.bus.write_long(new_sp, current_pc);
        self.cpu.write_reg(Register::A7, new_sp);
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

fn load_app_generic<M: MemoryBus>(
    fork: &ResourceFork,
    bus: &mut M,
    load_address: u32,
) -> Option<LoadedApp> {
    // 1. Load CODE 0 Header
    let code0 = fork.get_code(0)?;
    let header = Code0Header::parse(&code0.data)?;
    if trace_load_enabled() {
        eprintln!(
            "[LOAD] CODE 0 header: above_a5={}, below_a5={}, jt_size={}, jt_offset={}",
            header.above_a5, header.below_a5, header.jump_table_size, header.jump_table_offset
        );
    }

    let a5_base = load_address + header.below_a5;
    // For classic Mac apps, above_a5 defines the space needed above A5.
    // However, some apps place QuickDraw globals at higher offsets (e.g., A5+39KB).
    // Add 48KB reserve to accommodate most classic apps.
    let qd_globals_reserve = 48 * 1024; // 48KB reserve for QD globals
    let globals_end = a5_base + header.above_a5 + qd_globals_reserve;

    // Clear A5 world
    let globals_zero_end = globals_end + 0x40000;
    for addr in load_address..globals_zero_end {
        bus.write_byte(addr, 0);
    }
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
            let already_set = bus.read_long(entry) != 0 || bus.read_long(entry + 4) != 0;
            if already_set {
                continue;
            }
            bus.write_word(entry, 0);
            bus.write_word(entry + 2, 0x3F3C);
            bus.write_word(entry + 4, code_res.id as u16);
            bus.write_word(entry + 6, 0xA9F0);
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
            // Far-model CODE segment (40-byte header)
            let header_size = 40u32;
            for (i, entry) in jump_table.iter_mut().enumerate() {
                if entry.segment == code_res.id {
                    entry.loaded = true;
                    let effective_offset = entry.offset as u32;
                    entry.address = user_addr + header_size + effective_offset;

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

    // Stack at top of RAM
    let stack_top = bus.ram_size() - 16;

    Some(LoadedApp {
        code0_header: header,
        a5_base,
        jump_table,
        segment_bases,
        initial_sp: stack_top,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{DoubleBufferState, PendingDoubleBackCallback, SndChannel, OUTPUT_RATE};
    use crate::trap::dispatch::{TimerTask, VblTask};

    fn write_double_buffer(bus: &mut MacMemoryBus, ptr: u32, samples: &[u8]) {
        bus.write_long(ptr, samples.len() as u32);
        bus.write_long(ptr + 4, 0x0000_0001);
        for (offset, sample) in samples.iter().copied().enumerate() {
            bus.write_byte(ptr + 16 + offset as u32, sample);
        }
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
        assert_eq!(active.ccr, 0x1F);

        assert_ne!(runner.timer_trampoline, 0);
        assert_eq!(runner.cpu.read_reg(Register::PC), runner.timer_trampoline);
        assert_eq!(runner.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
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
            0,
            "loaded buffers are consumed by clearing dbBufferReady"
        );
        assert_eq!(
            runner.bus.read_long(buf1_ptr + 4) & 0x01,
            0,
            "the immediately loaded paired buffer is consumed too"
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

        // Playroom's callback epilogue has this shape: pop one long argument
        // by copying the return address over it, then RTS.
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
    fn vbl_callback_arms_interrupt_with_task_ptr_in_a0() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.cpu.write_reg(Register::PC, interrupted_pc);
        runner.cpu.write_reg(Register::A7, interrupted_sp);
        runner.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.cpu.core.set_ccr(0x04);

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
        assert_eq!(runner.bus.read_word(task_ptr + 10), 0);

        assert_ne!(runner.vbl_trampoline, 0);
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

    /// Regression gates for the tri-state spin-fastfwd gate. Tests the
    /// pure decision function so `OnceLock`-cached env vars don't
    /// interfere across tests.
    #[test]
    fn spin_fastfwd_gate_default_headless_on_gui_off() {
        // Neither force_on nor force_off → default behaviour:
        //   headless (yield_for_ui = false) → enabled
        //   GUI     (yield_for_ui = true)  → disabled
        assert!(spin_wait_fastfwd_gate(false, false, false));
        assert!(!spin_wait_fastfwd_gate(false, false, true));
    }

    #[test]
    fn spin_fastfwd_gate_force_off_wins() {
        // force_off must dominate force_on and override the default in
        // either mode.
        assert!(!spin_wait_fastfwd_gate(false, true, false));
        assert!(!spin_wait_fastfwd_gate(false, true, true));
        assert!(!spin_wait_fastfwd_gate(true, true, false));
        assert!(!spin_wait_fastfwd_gate(true, true, true));
    }

    #[test]
    fn spin_fastfwd_gate_force_on_overrides_gui_default() {
        // force_on (without force_off) flips GUI from off → on.
        assert!(spin_wait_fastfwd_gate(true, false, false));
        assert!(spin_wait_fastfwd_gate(true, false, true));
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
    fn pending_wait_sleep_ticks_capped_to_one_in_gui_mode() {
        // In GUI mode (tick_override=Some), WNE sleep is capped to 1 tick
        // per call. The full sleep duration is a hint, not a requirement.
        // This prevents long WNE sleeps (e.g. 60 ticks) from starving the
        // game loop. Macintosh Toolbox Essentials 1992, 2-22
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.cpu.write_reg(Register::PC, program_start);
        runner.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, _running) = runner.run_steps(1, Some(10));

        assert_eq!(steps, 1);
        // Only 1 tick advanced (not the full 60).
        assert_eq!(runner.bus.read_long(0x016A), 1);
        // Remaining sleep cleared — game resumes immediately.
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
