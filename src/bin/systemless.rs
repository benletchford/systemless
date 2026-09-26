//! Systemless Game Runner with graphical display.
//!
//! `cargo install systemless` installs this binary as `systemless`.
//! In a checkout, `cargo run`'s `default-run` (set in Cargo.toml)
//! routes here; the `gui` feature is on by default. So the local
//! invocation is:
//!
//! ```sh
//! cargo run --release -- [--headless] [--max-instructions N] \
//!     [--arrows-as-numpad] [--prefer-powerpc] <game>
//! ```
//!
//! Disable the GUI deps with `--no-default-features` to build a
//! headless-only library and skip the `winit` / `softbuffer` / `cpal`
//! link.

#[cfg(target_os = "windows")]
#[path = "desktop/d3d_present.rs"]
mod d3d_present;

#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;
#[path = "desktop/frame_metrics.rs"]
mod frame_metrics;
#[path = "desktop/frame_snapshot.rs"]
mod frame_snapshot;
#[path = "desktop/headless_time.rs"]
mod headless_time;
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[path = "desktop/host_cursor.rs"]
mod host_cursor;
#[cfg(target_os = "macos")]
#[path = "desktop/metal_present.rs"]
mod metal_present;
#[cfg(target_os = "macos")]
#[path = "desktop/native_application.rs"]
mod native_application;
#[cfg(target_os = "macos")]
#[path = "desktop/native_bundle.rs"]
mod native_bundle;
#[cfg(target_os = "macos")]
#[path = "desktop/native_menu.rs"]
mod native_menu;
#[cfg(target_os = "macos")]
#[path = "desktop/native_termination.rs"]
mod native_termination;
#[path = "desktop/runtime_driver.rs"]
mod runtime_driver;
#[path = "desktop/runtime_mailbox.rs"]
mod runtime_mailbox;
#[path = "desktop/runtime_owner.rs"]
mod runtime_owner;
#[path = "desktop/runtime_protocol.rs"]
mod runtime_protocol;

#[cfg(all(feature = "debug-server", unix))]
#[path = "desktop/debug_server.rs"]
mod debug_server;
#[cfg(not(all(feature = "debug-server", unix)))]
mod debug_server {
    use std::path::Path;

    pub struct DebugServer;

    impl DebugServer {
        pub fn bind(_path: &Path) -> std::io::Result<Self> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "debug socket transport requires the `debug-server` feature on a Unix platform",
            ))
        }

        pub fn pump(&mut self, _runner: &mut systemless::runner::FixtureRunner) -> usize {
            0
        }
    }
}

#[cfg(not(target_os = "macos"))]
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;

use clap::Parser;
use desktop_save_store::DesktopSaveStore;
#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::NSObject};
#[cfg(target_os = "macos")]
use objc2_quartz_core::CATransaction;
use runtime_driver::GuiDriver;
use systemless::debug_overlay::DebugOverlayFrameStats;
use systemless::display;
use systemless::game;
use systemless::runner::FixtureRunner;
use systemless::trap::dispatch::ScreenCopyBitsRect;
use systemless::ui_theme::UiThemeId;

#[cfg(not(target_os = "macos"))]
use softbuffer::Surface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;
#[cfg(target_os = "macos")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use winit::window::WindowAttributes;
use winit::window::WindowId;

/// Opt-in stall diagnostics; disabled runs avoid reading the clock per phase.
struct FramePhaseTimer {
    phase: &'static str,
    start: Option<std::time::Instant>,
    log_stalls: bool,
}

impl FramePhaseTimer {
    fn new(phase: &'static str) -> Self {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let enabled =
            *ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PROFILE_FRAMES").is_some());
        Self {
            phase,
            start: (enabled || frame_metrics::enabled()).then(std::time::Instant::now),
            log_stalls: enabled,
        }
    }
}

impl Drop for FramePhaseTimer {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let elapsed = start.elapsed();
            if frame_metrics::enabled() {
                frame_metrics::record(self.phase, elapsed);
            }
            if self.log_stalls && elapsed >= std::time::Duration::from_millis(50) {
                eprintln!(
                    "[SLOW-FRAME] {}: {:.1} ms",
                    self.phase,
                    elapsed.as_secs_f64() * 1000.0
                );
            }
        }
    }
}

/// Initial screen dimensions, taken from the active machine profile
/// (800x600 8bpp by default, overridable through `SYSTEMLESS_SCREEN_WIDTH`
/// and `SYSTEMLESS_SCREEN_HEIGHT`).
fn initial_screen_width() -> u32 {
    u32::from(systemless::machine_profile::reference_machine_profile().screen_width)
}

fn initial_screen_height() -> u32 {
    u32::from(systemless::machine_profile::reference_machine_profile().screen_height)
}

/// Frame duration at 60.15 Hz (Compact Mac VBL rate).
const FRAME_DURATION: std::time::Duration = std::time::Duration::from_micros(16_625);
const MIN_RENDER_HEADROOM: std::time::Duration = std::time::Duration::from_micros(1_500);
const MAX_RENDER_HEADROOM: std::time::Duration = std::time::Duration::from_micros(8_000);
const RENDER_HEADROOM_MARGIN: std::time::Duration = std::time::Duration::from_micros(500);
/// Foreground GUI work is checked against the host deadline only between
/// batches. Keep each slice well below a realtime VBL so heavy startup loads
/// can still present intermediate drawing and service Sound Manager callbacks.
const CPU_BATCH_INSTRUCTIONS: usize = 10_000;
/// PPC HLE slices move process-owned collections into the dispatch closure
/// and restore them afterward, so every slice boundary has a fixed cost and
/// batches stay coarser than the 68K instruction batches. Subdividing the
/// guest tick (rather than running one full-tick batch) keeps that cost
/// bounded while letting the GUI loop re-check its wall-clock CPU deadline
/// between sub-tick batches: draw-heavy imports used to let a single
/// full-tick batch run for ~1 s before `cpu_deadline` was consulted again.
const PPC_GUI_BATCH_TICK_DIVISOR: usize = 16;
const SOUND_CALLBACK_SLICE_INSTRUCTIONS: usize = CPU_BATCH_INSTRUCTIONS;
const SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME: usize = 25_000;
const AUDIO_CALLBACK_CHUNK_SAMPLES: usize = 32;
/// Pixel or CopyBits inference must agree across distinct guest drawing
/// updates before it can crop the presentation. Geometry from the manual
/// CPort already selected by the HLE is authoritative immediately.
const CONTENT_RECT_CONFIRMATIONS: u16 = 5;
/// After rejecting a crop, require a longer quiet-margin period before
/// shrinking again so startup phases cannot make the native window oscillate.
const CONTENT_RECT_RELEARN_CONFIRMATIONS: u16 = 120;

/// A stable crop only needs the margin histogram refreshed periodically: its
/// verdict changes only when the guest paints into the excluded margins. At
/// 60 Hz eight frames bound that staleness to roughly 130 ms, and the per-frame
/// confirmation counter keeps its cadence because the cached verdict is still
/// consulted on every frame.
const CONTENT_RECT_MARGIN_REFRESH_FRAMES: u16 = 8;

fn foreground_cpu_batch_instructions(powerpc: bool, instructions_per_tick: u32) -> usize {
    if powerpc {
        (instructions_per_tick.max(1) as usize).div_ceil(PPC_GUI_BATCH_TICK_DIVISOR)
    } else {
        CPU_BATCH_INSTRUCTIONS
    }
}

/// Both realtime frontends must use the loaded architecture's execution rate.
/// Calling GUI slice methods alone does not change the fixture's much smaller
/// default budget, which would put identical tick-script inputs at different
/// points in application startup.
fn configure_realtime_execution_rate(runner: &mut FixtureRunner) -> u32 {
    let instructions =
        systemless::runner::default_realtime_instructions_per_tick(runner.is_powerpc_app());
    runner.set_instructions_per_tick(instructions);
    instructions
}

/// A learned crop is discarded when substantial drawing persists in the area
/// it excludes. This catches apps whose large offscreen playfield is only one
/// part of a full-screen layout without reacting to a cursor or brief overlay.
const CONTENT_RECT_ACTIVE_MARGIN_PERCENT: usize = 10;
#[cfg(target_os = "macos")]
const VIEWPORT_CACHE_FILE: &str = "viewport.json";
const MAX_AUDIO_MIX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const DEFAULT_GUI_ARROWS_AS_NUMPAD: bool = false;

#[derive(Debug, Parser)]
#[command(name = "systemless", version, about)]
struct Cli {
    /// Application or game archive to launch
    #[arg(value_name = "GAME")]
    game: PathBuf,

    /// Run without opening a window
    #[arg(long)]
    headless: bool,

    /// Disable host-native desktop integrations
    #[arg(long)]
    no_native_integrations: bool,

    /// Map arrow keys to the numeric keypad
    #[arg(long, conflicts_with = "literal_arrows")]
    arrows_as_numpad: bool,

    /// Keep arrow keys mapped as literal arrow keys
    #[arg(
        long,
        visible_alias = "no-arrows-as-numpad",
        conflicts_with = "arrows_as_numpad"
    )]
    literal_arrows: bool,

    /// Stop a headless run after this many instructions
    #[arg(long, value_name = "N")]
    max_instructions: Option<usize>,

    /// Run headlessly for this many simulated frontend ticks (60.15 Hz),
    /// using GUI wait/callback scheduling. Defaults to 600 without legacy options.
    #[arg(
        long,
        requires = "headless",
        conflicts_with_all = ["max_instructions", "input_script"],
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    max_ticks: Option<u32>,

    /// Input script timestamped in elapsed frontend ticks, not instructions
    #[arg(
        long,
        requires = "max_ticks",
        conflicts_with = "input_script",
        value_name = "FILE"
    )]
    tick_input_script: Option<PathBuf>,

    /// Deterministic Mac-epoch startup seconds for simulated-time headless runs
    #[arg(
        long,
        requires = "headless",
        conflicts_with_all = ["max_instructions", "input_script"]
    )]
    headless_start_time: Option<u32>,

    /// Prefer a native PowerPC slice when a classic 68K slice is also available
    #[arg(long, visible_alias = "prefer-ppc")]
    prefer_powerpc: bool,

    /// Start with classic 24-bit guest address translation
    #[arg(long)]
    addressing_24_bit: bool,

    /// Override the guest framebuffer depth (defaults to 8-bit for 68K and 16-bit for PPC)
    #[arg(long, value_name = "BITS", value_parser = parse_screen_depth)]
    screen_depth: Option<u16>,

    /// Override automatic window sizing with an integer physical-pixel scale
    #[arg(long, value_name = "N", value_parser = parse_display_scale)]
    display_scale: Option<u32>,

    /// Guest chrome theme
    #[arg(
        long,
        value_name = "THEME",
        default_value = "classic-system7",
        value_parser = UiThemeId::parse
    )]
    ui_theme: UiThemeId,
    /// Start in a borderless fullscreen space. On systems where macOS selects
    /// direct scan-out for the fullscreen surface this measurably reduced
    /// pointer-to-screen latency in testing; the benefit depends on the
    /// machine and compositor state.
    #[arg(long)]
    fullscreen: bool,

    /// Legacy diagnostic input script timestamped in retired instructions.
    /// Not a matched-time workload: wait optimizations change when inputs land.
    #[arg(long, value_name = "FILE")]
    input_script: Option<PathBuf>,

    /// Expose the debugger over a Unix-domain socket. One controlling client
    /// sends one JSON request per line and receives one JSON reply per line;
    /// requests are applied on the runner thread between frames. Headless
    /// runs start paused until the client resumes or steps execution.
    #[arg(long, value_name = "PATH")]
    debug_socket: Option<PathBuf>,
}

fn parse_screen_depth(value: &str) -> Result<u16, String> {
    match value {
        "1" => Ok(1),
        "2" => Ok(2),
        "4" => Ok(4),
        "8" => Ok(8),
        _ => Err("screen depth must be 1, 2, 4, or 8".to_string()),
    }
}

fn parse_display_scale(value: &str) -> Result<u32, String> {
    let scale = value
        .parse::<u32>()
        .map_err(|_| "display scale must be an integer from 1 through 8".to_string())?;
    if (1..=8).contains(&scale) {
        Ok(scale)
    } else {
        Err("display scale must be an integer from 1 through 8".to_string())
    }
}

fn guest_scaled_physical_size(
    width: u32,
    height: u32,
    display_scale: u32,
) -> winit::dpi::PhysicalSize<u32> {
    winit::dpi::PhysicalSize::new(
        width.saturating_mul(display_scale),
        height.saturating_mul(display_scale),
    )
}

/// Fit guest content into a host-pixel box, including fractional enlargement
/// and downscaling. Integer rounding here used to progressively shrink windows
/// when a game changed its crop or resolution.
fn fit_window_size(
    width: u32,
    height: u32,
    bounds: winit::dpi::PhysicalSize<u32>,
) -> winit::dpi::PhysicalSize<u32> {
    let width = f64::from(width.max(1));
    let height = f64::from(height.max(1));
    let scale =
        (f64::from(bounds.width.max(1)) / width).min(f64::from(bounds.height.max(1)) / height);
    winit::dpi::PhysicalSize::new(
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

/// On macOS, start at one guest pixel per logical point. Other platforms
/// target a 960×720-point box. Leave room for desktop chrome on small monitors
/// and apply backing DPI once before fitting the content.
fn automatic_window_size(
    width: u32,
    height: u32,
    monitor: Option<winit::dpi::PhysicalSize<u32>>,
    scale_factor: f64,
) -> winit::dpi::PhysicalSize<u32> {
    let dpi = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    #[cfg(target_os = "macos")]
    let logical_bounds = (f64::from(width.max(1)), f64::from(height.max(1)));
    #[cfg(not(target_os = "macos"))]
    let logical_bounds = (960.0, 720.0);
    let mut bounds = winit::dpi::PhysicalSize::new(
        (logical_bounds.0 * dpi).round().max(1.0) as u32,
        (logical_bounds.1 * dpi).round().max(1.0) as u32,
    );
    if let Some(monitor) = monitor.filter(|m| m.width > 0 && m.height > 0) {
        bounds.width = bounds.width.min((f64::from(monitor.width) * 0.8) as u32);
        bounds.height = bounds.height.min((f64::from(monitor.height) * 0.8) as u32);
    }
    fit_window_size(width, height, bounds)
}

/// Use the actual window's display, including the macOS menu bar and Dock
/// exclusions. Other platforms leave a conservative margin around the monitor.
fn window_monitor_bounds(
    window: &Window,
) -> Option<(
    winit::dpi::PhysicalPosition<i32>,
    winit::dpi::PhysicalSize<u32>,
)> {
    let monitor = window.current_monitor()?;
    let position = monitor.position();
    let size = monitor.size();
    #[cfg(target_os = "macos")]
    {
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                // SAFETY: winit owns the NSView/NSWindow, and this helper is
                // called on the AppKit main thread during window creation.
                unsafe {
                    let view: &NSObject = handle.ns_view.cast().as_ref();
                    let native_window: *mut NSObject = msg_send![view, window];
                    if let Some(native_window) = native_window.as_ref() {
                        let screen: *mut NSObject = msg_send![native_window, screen];
                        if let Some(screen) = screen.as_ref() {
                            let frame: objc2_foundation::CGRect = msg_send![screen, frame];
                            let visible: objc2_foundation::CGRect = msg_send![screen, visibleFrame];
                            let dpi = window.scale_factor();
                            if visible.size.width > 0.0 && visible.size.height > 0.0 {
                                // AppKit's Y axis points up; winit's points down.
                                let left = (visible.origin.x - frame.origin.x) * dpi;
                                let top = (frame.origin.y + frame.size.height
                                    - visible.origin.y
                                    - visible.size.height)
                                    * dpi;
                                return Some((
                                    winit::dpi::PhysicalPosition::new(
                                        position.x.saturating_add(left.round() as i32),
                                        position.y.saturating_add(top.round() as i32),
                                    ),
                                    winit::dpi::PhysicalSize::new(
                                        (visible.size.width * dpi).round().max(1.0) as u32,
                                        (visible.size.height * dpi).round().max(1.0) as u32,
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    Some((position, size))
}

fn guest_resize_scale(
    display_scale: Option<u32>,
    fullscreen: bool,
    maximized: bool,
) -> Option<u32> {
    display_scale.filter(|_| !fullscreen && !maximized)
}

fn window_guest_resize_scale(window: &Window, display_scale: Option<u32>) -> Option<u32> {
    guest_resize_scale(
        display_scale,
        window.fullscreen().is_some(),
        window.is_maximized(),
    )
}

/// One scripted input. The caller explicitly chooses the clock: retired
/// instructions for legacy diagnostic scripts, or elapsed simulated frontend
/// ticks for time-based replays. Neither clock is host wall time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScriptedInput {
    at: usize,
    action: InputAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAction {
    MouseMove { v: i16, h: i16 },
    MouseDown { v: i16, h: i16 },
    MouseUp { v: i16, h: i16 },
    KeyDown { key: u8, ch: u8 },
    KeyUp { key: u8, ch: u8 },
}

/// Parse an input script: one `<at> <action> [args]` per line, `#`
/// comments and blank lines ignored. `click` and `press` expand to a
/// down/up pair at the same instant, which is what a guest sees for an
/// ordinary click or keystroke.
///
/// Numbers accept `0x` prefixes so key codes can be written the way the
/// Mac key tables list them.
fn parse_input_script(text: &str) -> Result<Vec<ScriptedInput>, String> {
    fn num<T: TryFrom<u64>>(tok: &str, line: usize) -> Result<T, String> {
        let raw = tok.strip_prefix("0x").map_or_else(
            || tok.parse::<u64>().map_err(|e| e.to_string()),
            |hex| u64::from_str_radix(hex, 16).map_err(|e| e.to_string()),
        );
        let value = raw.map_err(|e| format!("line {line}: bad number {tok:?}: {e}"))?;
        T::try_from(value).map_err(|_| format!("line {line}: {tok:?} out of range"))
    }

    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let body = raw.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            continue;
        }
        let tok: Vec<&str> = body.split_whitespace().collect();
        if tok.len() < 2 {
            return Err(format!("line {line}: expected `<at> <action> [args]`"));
        }
        let at: usize = num(tok[0], line)?;
        let args = &tok[2..];
        let expect = |n: usize| -> Result<(), String> {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!(
                    "line {line}: `{}` takes {n} argument(s), got {}",
                    tok[1],
                    args.len()
                ))
            }
        };
        let mut push = |action| out.push(ScriptedInput { at, action });
        match tok[1] {
            "mousemove" => {
                expect(2)?;
                push(InputAction::MouseMove {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "mousedown" => {
                expect(2)?;
                push(InputAction::MouseDown {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "mouseup" => {
                expect(2)?;
                push(InputAction::MouseUp {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "click" => {
                expect(2)?;
                let (v, h) = (num(args[0], line)?, num(args[1], line)?);
                push(InputAction::MouseDown { v, h });
                push(InputAction::MouseUp { v, h });
            }
            "keydown" => {
                expect(2)?;
                push(InputAction::KeyDown {
                    key: num(args[0], line)?,
                    ch: num(args[1], line)?,
                });
            }
            "keyup" => {
                expect(2)?;
                push(InputAction::KeyUp {
                    key: num(args[0], line)?,
                    ch: num(args[1], line)?,
                });
            }
            "press" => {
                expect(2)?;
                let (key, ch) = (num(args[0], line)?, num(args[1], line)?);
                push(InputAction::KeyDown { key, ch });
                push(InputAction::KeyUp { key, ch });
            }
            other => return Err(format!("line {line}: unknown action {other:?}")),
        }
    }
    out.sort_by_key(|event| event.at);
    Ok(out)
}

/// How many instructions to run before the next scheduled event.
///
/// Without this the loop would overshoot by up to a whole chunk and the
/// delivery point would depend on chunk size rather than on the script,
/// which would break the determinism the schedule exists to provide.
fn steps_until_next_event(
    chunk: usize,
    remaining: usize,
    total: usize,
    next_at: Option<usize>,
) -> usize {
    let bounded = chunk.min(remaining);
    match next_at {
        Some(at) => bounded.min(at.saturating_sub(total).max(1)),
        None => bounded,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct ContentRect {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct CachedContentRect {
    version: u8,
    screen_width: u16,
    screen_height: u16,
    pixel_size: u16,
    content: ContentRect,
}

#[cfg(target_os = "macos")]
fn viewport_cache_path(game_path: &std::path::Path) -> PathBuf {
    DesktopSaveStore::root_for_game_path(game_path).join(VIEWPORT_CACHE_FILE)
}

#[cfg(target_os = "macos")]
fn valid_cached_content_rect(cache: &CachedContentRect) -> bool {
    let content = cache.content;
    // A rect learned under a different guest screen size says nothing about
    // where this run's guest draws; sizing the window from it would letterbox
    // the new mode inside the old one.
    let profile = systemless::machine_profile::reference_machine_profile();
    cache.version == 2
        && cache.screen_width == profile.screen_width
        && cache.screen_height == profile.screen_height
        && content.width != 0
        && content.height != 0
        && content.left.saturating_add(content.width) <= u32::from(cache.screen_width)
        && content.top.saturating_add(content.height) <= u32::from(cache.screen_height)
}

#[cfg(target_os = "macos")]
fn load_cached_content_rect(game_path: &std::path::Path) -> Option<CachedContentRect> {
    let path = viewport_cache_path(game_path);
    let bytes = std::fs::read(&path).ok()?;
    let cache: CachedContentRect = serde_json::from_slice(&bytes).ok()?;
    valid_cached_content_rect(&cache).then_some(cache)
}

#[cfg(target_os = "macos")]
fn cached_content_rect_for_presentation(
    cache: Option<CachedContentRect>,
    native_integrations: bool,
) -> Option<CachedContentRect> {
    native_integrations.then_some(cache).flatten()
}

#[cfg(target_os = "macos")]
fn should_learn_content_rect(debug_overlay_visible: bool, native_integrations: bool) -> bool {
    !debug_overlay_visible && native_integrations
}

#[cfg(target_os = "macos")]
fn persist_content_rect(
    game_path: &std::path::Path,
    screen_mode: (u32, u32, u16, u16, u16),
    content: ContentRect,
) {
    let cache = CachedContentRect {
        version: 2,
        screen_width: screen_mode.2,
        screen_height: screen_mode.3,
        pixel_size: screen_mode.4,
        content,
    };
    let path = viewport_cache_path(game_path);
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&cache)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        std::fs::write(&path, bytes)
    })();
    if let Err(err) = result {
        eprintln!(
            "[SYSTEMLESS] Could not cache guest viewport at {}: {}",
            path.display(),
            err
        );
    }
}

#[cfg(target_os = "macos")]
fn platform_window_attrs(attrs: WindowAttributes) -> WindowAttributes {
    attrs
        .with_disallow_hidpi(false)
        .with_accepts_first_mouse(true)
}

#[cfg(not(target_os = "macos"))]
fn platform_window_attrs(attrs: WindowAttributes) -> WindowAttributes {
    attrs
}

fn service_pending_sound_work(
    runner: &mut FixtureRunner,
    _cpu_deadline: std::time::Instant,
    slice_budget: usize,
    total_steps: usize,
    reserved_sound_steps: &mut usize,
) -> Option<usize> {
    service_pending_sound_work_budgeted(runner, slice_budget, total_steps, reserved_sound_steps)
}

fn service_pending_sound_work_budgeted(
    runner: &mut FixtureRunner,
    slice_budget: usize,
    total_steps: usize,
    reserved_sound_steps: &mut usize,
) -> Option<usize> {
    if !runner.has_pending_sound_work() || runner.is_halted() {
        return None;
    }

    // Double-buffer callbacks are Sound Manager interrupt work, not foreground
    // application execution. Give them reserved time even when the GUI frame
    // has spent its foreground budget, but cap that reserve per host frame so
    // audio refills cannot monopolize the single-threaded event loop.
    let remaining = slice_budget.saturating_sub(total_steps);
    let using_reserved_slice = remaining == 0;
    let callback_budget = if using_reserved_slice {
        let reserved_remaining =
            SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME.saturating_sub(*reserved_sound_steps);
        if reserved_remaining == 0 {
            return None;
        }
        reserved_remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
    } else {
        remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
    };

    // This slice services audio interrupts, not a presentation frame. The
    // outer presentation pass restores native chrome once.
    let (steps, _running) = runner.run_gui_pending_sound_work(callback_budget);
    if using_reserved_slice {
        *reserved_sound_steps = reserved_sound_steps.saturating_add(steps);
    }
    Some(steps)
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct TransientWindowGeometry {
    inner_size: winit::dpi::PhysicalSize<u32>,
    outer_position: Option<winit::dpi::PhysicalPosition<i32>>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct PendingWindowTransition {
    content: ContentRect,
    target_size: winit::dpi::PhysicalSize<u32>,
    required_resize_event: u64,
}

#[cfg(target_os = "macos")]
struct CoreAnimationTransaction;

#[cfg(target_os = "macos")]
impl CoreAnimationTransaction {
    fn begin() -> Self {
        // SAFETY: GUI rendering and AppKit window mutation both happen on the
        // main thread. The guard guarantees a matching commit on every path.
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        Self
    }
}

#[cfg(target_os = "macos")]
impl Drop for CoreAnimationTransaction {
    fn drop(&mut self) {
        // SAFETY: paired with begin above on the same main thread.
        CATransaction::commit();
    }
}

/// Keeps a host press/release pair from collapsing before the guest executes.
/// Winit can deliver both button events in one event-loop batch. Polling Mac
/// applications must still execute with the button down before they observe
/// the release, just as they would for a physical ADB mouse.
#[derive(Default)]
struct HostMouseReleaseLatch {
    pressed: bool,
    guest_observed_press: bool,
    guest_needs_release_observation: bool,
    pending_release: Option<(i16, i16)>,
}

impl HostMouseReleaseLatch {
    fn press(&mut self) {
        self.pressed = true;
        self.guest_observed_press = false;
        self.guest_needs_release_observation = false;
        self.pending_release = None;
    }

    fn release(&mut self, position: (i16, i16)) -> Option<(i16, i16)> {
        if self.pressed && !self.guest_observed_press {
            self.pending_release = Some(position);
            None
        } else {
            let had_press = self.pressed;
            self.pressed = false;
            self.guest_observed_press = false;
            self.guest_needs_release_observation = had_press;
            Some(position)
        }
    }

    fn observe_guest_progress(&mut self) {
        if self.pressed && !self.guest_observed_press {
            self.guest_observed_press = true;
        } else if self.guest_needs_release_observation {
            self.guest_needs_release_observation = false;
        }
    }

    fn requires_guest_progress(&self) -> bool {
        (self.pressed && !self.guest_observed_press) || self.guest_needs_release_observation
    }

    fn take_ready_release(&mut self) -> Option<(i16, i16)> {
        if !self.guest_observed_press || self.pending_release.is_none() {
            return None;
        }
        self.pressed = false;
        self.guest_observed_press = false;
        self.guest_needs_release_observation = true;
        self.pending_release.take()
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct PendingGpuFrame {
    size: (u32, u32),
    rect: (u32, u32, u32, u32),
    guest_tick: u32,
}

struct App {
    driver: Option<GuiDriver>,
    owner: Option<runtime_owner::RuntimeOwner>,
    #[cfg(target_os = "macos")]
    pending_native_bundle: Option<native_bundle::NativeBundle>,
    #[cfg(target_os = "macos")]
    native_startup_fallback: Option<(
        runtime_owner::RuntimeConfig,
        winit::event_loop::EventLoopProxy<()>,
    )>,
    closing: bool,
    runtime_error: Option<String>,
    force_next_render: bool,
    last_presented_guest_tick: Option<u32>,
    render_headroom: std::time::Duration,
    guest_state: runtime_protocol::GuiState,
    frame: frame_snapshot::GuiFrame,
    retained_frame_cache: frame_snapshot::RetainedFrameCache,
    #[cfg(target_os = "windows")]
    gpu: Option<d3d_present::D3dPresenter>,
    #[cfg(target_os = "windows")]
    gpu_wake: Option<winit::event_loop::EventLoopProxy<()>>,
    #[cfg(target_os = "windows")]
    gpu_frame: std::sync::Arc<systemless::memory::CompactPresentation>,
    #[cfg(target_os = "windows")]
    gpu_pending: Option<PendingGpuFrame>,
    window: Option<Rc<Window>>,
    #[cfg(target_os = "macos")]
    surface: Option<metal_present::MetalPresenter>,
    #[cfg(not(target_os = "macos"))]
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    #[cfg(not(target_os = "macos"))]
    surface_size: Option<(u32, u32)>,
    frame_argb: Vec<u32>,
    guest_frame_argb: Vec<u32>,
    presentation_argb: Vec<u32>,
    #[cfg(target_os = "macos")]
    content_rect: Option<ContentRect>,
    #[cfg(target_os = "macos")]
    content_rect_candidate: Option<(ContentRect, u16)>,
    #[cfg(target_os = "macos")]
    content_rect_copybits_count: u64,
    #[cfg(target_os = "macos")]
    content_rect_active_margin_frames: u16,
    #[cfg(target_os = "macos")]
    content_rect_margin_cache: Option<(ContentRect, bool)>,
    #[cfg(target_os = "macos")]
    content_rect_margin_refresh: u16,
    /// Continue looking for a valid crop after active margins forced this run
    /// back to the full screen; a startup splash may later become letterboxed.
    #[cfg(target_os = "macos")]
    content_rect_relearn_after_full: bool,
    /// Previous raw guest frame used only while learning the pixel-content
    /// fallback. Repeated host presentations of one image do not confirm it.
    #[cfg(target_os = "macos")]
    content_rect_previous_frame: Vec<u8>,
    #[cfg(target_os = "macos")]
    content_rect_screen_mode: Option<(u16, u16, u16)>,
    #[cfg(target_os = "macos")]
    content_rect_frame: Option<(u64, u64)>,
    /// Stable presentation rectangle for which the native window was last
    /// sized. Transient dialogs may expand it without replacing the cached
    /// gameplay crop.
    #[cfg(target_os = "macos")]
    window_sized_content_rect: Option<ContentRect>,
    /// Native size and position to restore after a transient dialog closes.
    #[cfg(target_os = "macos")]
    transient_window_restore_geometry: Option<TransientWindowGeometry>,
    /// Automatic dialog geometry waits for AppKit's resize callback before
    /// exposing the new crop. Until then Metal retains the prior complete
    /// drawable, avoiding a clipped or moving intermediate frame.
    #[cfg(target_os = "macos")]
    pending_window_transition: Option<PendingWindowTransition>,
    #[cfg(target_os = "macos")]
    window_resize_events: u64,
    #[cfg(not(target_os = "macos"))]
    scaled_frame: Vec<u32>,
    game_path: PathBuf,
    /// Current mouse position in physical window pixels
    mouse_physical: (f64, f64),
    /// Guest displacement used when the window system cannot warp its pointer.
    mouse_guest_offset: (i16, i16),
    /// Current game screen dimensions (tracks screen_mode changes)
    current_screen_width: u32,
    current_screen_height: u32,
    /// Force a Metal submission even if all visible guest inputs are
    /// unchanged, for native expose/resize events that need a fresh drawable.
    #[cfg(target_os = "macos")]
    force_gpu_present: bool,
    start_fullscreen: bool,
    /// Show the Systemless debug overlay on top of the game framebuffer.
    debug_overlay_visible: bool,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    host_cursor: host_cursor::HostCursor,
    debug_last_frame_at: Option<std::time::Instant>,
    debug_host_fps: Option<f64>,
    debug_frame_ms: Option<f64>,
    /// Constructor configuration assertion; the runtime owns key remapping.
    #[cfg(test)]
    arrows_as_numpad: bool,
    /// None chooses a comfortable monitor-aware size; Some preserves an
    /// explicit physical guest-to-host pixel ratio.
    display_scale: Option<u32>,
    #[cfg(target_os = "macos")]
    native_integrations: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<native_menu::NativeMenuBridge>,
    #[cfg(target_os = "macos")]
    native_app_path: Option<String>,
    #[cfg(target_os = "macos")]
    native_app_name: String,
    #[cfg(target_os = "macos")]
    native_app_icon: Option<game::ApplicationIcon>,
}

impl App {
    #[cfg(test)]
    fn new(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        native_integrations: bool,
        addressing_24_bit: bool,
        screen_depth: u16,
    ) -> Self {
        Self::new_with_display_scale(
            game_path,
            arrows_as_numpad,
            native_integrations,
            addressing_24_bit,
            Some(screen_depth),
            Some(1),
            UiThemeId::ClassicSystem7,
            false,
            false,
        )
    }

    fn new_with_display_scale(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        native_integrations: bool,
        addressing_24_bit: bool,
        screen_depth: Option<u16>,
        display_scale: Option<u32>,
        ui_theme: UiThemeId,
        start_fullscreen: bool,
        threaded: bool,
    ) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = native_integrations;
        #[cfg(target_os = "macos")]
        let native_menu_app_name = game_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Systemless")
            .to_owned();
        #[cfg(target_os = "macos")]
        let native_app_name = native_menu_app_name.clone();
        #[cfg(target_os = "macos")]
        let cached_content = cached_content_rect_for_presentation(
            load_cached_content_rect(&game_path),
            native_integrations,
        );
        #[cfg(target_os = "macos")]
        if let Some(cache) = cached_content.as_ref() {
            eprintln!(
                "[SYSTEMLESS] Cached guest content: {}x{} at ({},{}) inside {}x{}",
                cache.content.width,
                cache.content.height,
                cache.content.left,
                cache.content.top,
                cache.screen_width,
                cache.screen_height
            );
        }
        Self {
            driver: (!threaded).then(|| {
                GuiDriver::new(
                    game_path.clone(),
                    arrows_as_numpad,
                    native_integrations,
                    addressing_24_bit,
                    screen_depth,
                    ui_theme,
                )
            }),
            owner: None,
            #[cfg(target_os = "macos")]
            pending_native_bundle: None,
            #[cfg(target_os = "macos")]
            native_startup_fallback: None,
            closing: false,
            runtime_error: None,
            force_next_render: true,
            last_presented_guest_tick: None,
            render_headroom: MIN_RENDER_HEADROOM,
            guest_state: Default::default(),
            frame: Default::default(),
            retained_frame_cache: Default::default(),
            window: None,
            #[cfg(target_os = "windows")]
            gpu: None,
            #[cfg(target_os = "windows")]
            gpu_wake: None,
            #[cfg(target_os = "windows")]
            gpu_frame: Default::default(),
            #[cfg(target_os = "windows")]
            gpu_pending: None,
            surface: None,
            #[cfg(not(target_os = "macos"))]
            surface_size: None,
            frame_argb: Vec::new(),
            guest_frame_argb: Vec::new(),
            presentation_argb: Vec::new(),
            #[cfg(target_os = "macos")]
            content_rect: cached_content.as_ref().map(|cache| cache.content),
            #[cfg(target_os = "macos")]
            content_rect_candidate: None,
            #[cfg(target_os = "macos")]
            content_rect_copybits_count: 0,
            #[cfg(target_os = "macos")]
            content_rect_active_margin_frames: 0,
            #[cfg(target_os = "macos")]
            content_rect_margin_cache: None,
            #[cfg(target_os = "macos")]
            content_rect_margin_refresh: 0,
            #[cfg(target_os = "macos")]
            content_rect_relearn_after_full: false,
            #[cfg(target_os = "macos")]
            content_rect_previous_frame: Vec::new(),
            #[cfg(target_os = "macos")]
            content_rect_frame: None,
            #[cfg(target_os = "macos")]
            content_rect_screen_mode: cached_content
                .as_ref()
                .map(|cache| (cache.screen_width, cache.screen_height, cache.pixel_size)),
            #[cfg(target_os = "macos")]
            window_sized_content_rect: cached_content.as_ref().map(|cache| cache.content),
            #[cfg(target_os = "macos")]
            transient_window_restore_geometry: None,
            #[cfg(target_os = "macos")]
            pending_window_transition: None,
            #[cfg(target_os = "macos")]
            window_resize_events: 0,
            #[cfg(not(target_os = "macos"))]
            scaled_frame: Vec::new(),
            game_path,
            mouse_physical: (0.0, 0.0),
            mouse_guest_offset: (0, 0),
            current_screen_width: initial_screen_width(),
            current_screen_height: initial_screen_height(),
            #[cfg(target_os = "macos")]
            force_gpu_present: true,
            start_fullscreen,
            debug_overlay_visible: false,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            host_cursor: host_cursor::HostCursor::new(),
            debug_last_frame_at: None,
            debug_host_fps: None,
            debug_frame_ms: None,
            #[cfg(test)]
            arrows_as_numpad,
            display_scale,
            #[cfg(target_os = "macos")]
            native_integrations,
            #[cfg(target_os = "macos")]
            native_menu: native_integrations
                .then(|| native_menu::NativeMenuBridge::new(native_menu_app_name)),
            #[cfg(target_os = "macos")]
            native_app_path: None,
            #[cfg(target_os = "macos")]
            native_app_name,
            #[cfg(target_os = "macos")]
            native_app_icon: None,
        }
    }

    fn show_runtime_error(&self) {
        if let (Some(window), Some(error)) = (self.window.as_ref(), self.runtime_error.as_ref()) {
            window.set_title(&format!("Systemless runtime stopped: {error}"));
        }
    }

    fn send_command(&mut self, command: runtime_protocol::GuiCommand) {
        if self.closing {
            return;
        }
        if let Some(owner) = self.owner.as_ref() {
            if let Err(error) = owner.mailbox.send(command) {
                if matches!(error, runtime_mailbox::CommandError::Full(_)) {
                    self.runtime_error = Some("Input queue is full; the runtime is stopping to preserve accepted input and saves.".into());
                    owner.mailbox.request_shutdown();
                    eprintln!("[SYSTEMLESS] {}", self.runtime_error.as_ref().unwrap());
                    self.show_runtime_error();
                }
            }
        } else if let Some(driver) = self.driver.as_mut() {
            driver.apply_command(command);
        }
    }

    fn sync_native_state(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.sync_host_cursor(event_loop);

        #[cfg(target_os = "macos")]
        self.sync_native_application_identity();

        #[cfg(target_os = "macos")]
        if let (Some(native_menu), Some(snapshot)) =
            (self.native_menu.as_mut(), self.guest_state.menus.as_ref())
        {
            native_menu.sync((**snapshot).clone());
        }

        // Check if screen mode changed
        if self.guest_state.generation != 0 {
            let (_, _, sw, sh, _) = self.guest_state.screen_mode;
            let sw = sw as u32;
            let sh = sh as u32;
            if sw != self.current_screen_width || sh != self.current_screen_height {
                self.current_screen_width = sw;
                self.current_screen_height = sh;
                if let Some(window) = &self.window {
                    // Automatic sizing owns only the initial geometry. Keep
                    // the user's window through guest mode changes and fit
                    // the new framebuffer into it during presentation.
                    if let Some(scale) = window_guest_resize_scale(window, self.display_scale) {
                        let _ =
                            window.request_inner_size(guest_scaled_physical_size(sw, sh, scale));
                    }
                }
                self.force_next_render = true;
            }
        }
    }

    fn poll_owner(&mut self, event_loop: &ActiveEventLoop) {
        use runtime_mailbox::RuntimeStatus;
        #[cfg(target_os = "macos")]
        if native_termination::pending() {
            // AppKit's modal termination loop owns completion. Its timer waits
            // for owner teardown without relying on winit callbacks or joining.
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        let update = self
            .owner
            .as_ref()
            .expect("threaded runtime")
            .mailbox
            .poll();
        #[cfg(target_os = "macos")]
        if let Some(bundle) = update.native_bundle {
            self.pending_native_bundle = Some(bundle);
        }
        if let Some(state) = update.state {
            self.guest_state = state;
        }
        let has_frame = update.frame.is_some();
        if let Some(mut frame) = update.frame {
            std::mem::swap(&mut self.frame, &mut frame);
            self.owner.as_ref().unwrap().mailbox.recycle(frame);
        }
        self.sync_guest_cursor_warp();
        self.sync_native_state(event_loop);
        if has_frame && !self.closing {
            let now = std::time::Instant::now();
            self.update_debug_frame_stats(now);
            self.present_frame(now);
        }
        if let RuntimeStatus::Stopped {
            error,
            instructions,
        } = update.status
        {
            if self.runtime_error.is_none() {
                if let Some(error) = error.as_ref() {
                    eprintln!("[SYSTEMLESS] Runtime failed: {error}");
                }
                self.runtime_error = error;
            }
            let joined = match self.owner.as_mut().unwrap().join_finished() {
                Ok(joined) => joined,
                Err(error) => {
                    self.runtime_error = Some(error);
                    true
                }
            };
            if !joined {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    std::time::Instant::now() + std::time::Duration::from_millis(2),
                ));
                return;
            }
            #[cfg(target_os = "macos")]
            if !self.closing && self.runtime_error.is_none() && !native_termination::pending() {
                if let Some(bundle) = self.pending_native_bundle.take() {
                    // The inspection owner has finished and was joined above.
                    // Exec starts a fresh process through its native bundle;
                    // never move a live runner or AppKit object between threads.
                    let error = native_bundle::exec_bundle(&bundle);
                    eprintln!(
                        "[SYSTEMLESS] Could not enter native bundle: {error}; continuing unbundled"
                    );
                    let (mut config, proxy) = self
                        .native_startup_fallback
                        .take()
                        .expect("native bootstrap fallback");
                    config.native_preflight = false;
                    match runtime_owner::RuntimeOwner::spawn(config, move || {
                        let _ = proxy.send_event(());
                    }) {
                        Ok(owner) => {
                            native_termination::activate(owner.mailbox.clone());
                            self.owner = Some(owner);
                            event_loop.set_control_flow(ControlFlow::Wait);
                            return;
                        }
                        Err(error) => {
                            self.runtime_error =
                                Some(format!("Failed to start runtime owner: {error}"))
                        }
                    }
                }
            }
            if self.closing || self.runtime_error.is_none() {
                eprintln!("[SYSTEMLESS] Runtime stopped. Total instructions: {instructions}");
                event_loop.exit();
            } else if let Some(window) = self.window.as_ref() {
                window.set_title(&format!(
                    "Systemless runtime stopped: {}",
                    self.runtime_error.as_ref().unwrap()
                ));
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            return;
        }
        let force = std::mem::take(&mut self.force_next_render);
        #[cfg(target_os = "macos")]
        let (capture_crop, learning_crop) = (
            should_learn_content_rect(self.debug_overlay_visible, self.native_integrations),
            self.content_rect.is_none() || self.content_rect_relearn_after_full,
        );
        #[cfg(not(target_os = "macos"))]
        let (capture_crop, learning_crop) = (false, false);
        self.owner.as_ref().unwrap().mailbox.request_presentation(
            runtime_mailbox::PresentationOptions {
                debug: self
                    .debug_overlay_visible
                    .then_some(DebugOverlayFrameStats {
                        host_fps: self.debug_host_fps,
                        frame_ms: self.debug_frame_ms,
                        ..Default::default()
                    }),
                capture_crop,
                learning_crop,
                render_headroom: Some(self.render_headroom),
                force,
            },
        );
        #[cfg(target_os = "windows")]
        if let Some(at) = self.gpu.as_ref().and_then(|gpu| gpu.retry_at()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            return;
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn init_game(&mut self) {
        self.driver.as_mut().expect("serial runtime").init_game();
        self.driver
            .as_mut()
            .expect("serial runtime")
            .capture_state(&mut self.guest_state, true);
        #[cfg(target_os = "macos")]
        self.sync_native_application_identity();
    }

    /// Convert physical window coordinates to Mac screen coordinates.
    fn physical_to_mac(&self, px: f64, py: f64) -> (i16, i16) {
        let sw = self.current_screen_width;
        let sh = self.current_screen_height;
        let size = self
            .window
            .as_ref()
            .map(|w| w.inner_size())
            .unwrap_or(winit::dpi::PhysicalSize::new(sw, sh));

        #[cfg(target_os = "macos")]
        let content = if self.debug_overlay_visible {
            ContentRect {
                left: 0,
                top: 0,
                width: sw,
                height: sh,
            }
        } else {
            presentation_content_rect(
                self.content_rect.unwrap_or(ContentRect {
                    left: 0,
                    top: 0,
                    width: sw,
                    height: sh,
                }),
                self.guest_state.dialog_bounds,
                sw,
                sh,
                self.guest_state.hidden_menu_height,
            )
        };
        #[cfg(not(target_os = "macos"))]
        let content = ContentRect {
            left: 0,
            top: 0,
            width: sw,
            height: sh,
        };

        physical_to_mac_in_viewport(px, py, content, size.width, size.height)
    }

    fn host_mouse_to_mac(&self, px: f64, py: f64) -> (i16, i16) {
        let (v, h) = self.physical_to_mac(px, py);
        (
            v.saturating_add(self.mouse_guest_offset.0),
            h.saturating_add(self.mouse_guest_offset.1),
        )
    }

    fn sync_guest_cursor_warp(&mut self) {
        let Some(warp) = self.guest_state.warp else {
            return;
        };
        let (v, h) = warp.position;
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        let sw = self.current_screen_width;
        let sh = self.current_screen_height;
        #[cfg(target_os = "macos")]
        let content = if self.debug_overlay_visible {
            ContentRect {
                left: 0,
                top: 0,
                width: sw,
                height: sh,
            }
        } else {
            presentation_content_rect(
                self.content_rect.unwrap_or(ContentRect {
                    left: 0,
                    top: 0,
                    width: sw,
                    height: sh,
                }),
                self.guest_state.dialog_bounds,
                sw,
                sh,
                self.guest_state.hidden_menu_height,
            )
        };
        #[cfg(not(target_os = "macos"))]
        let content = ContentRect {
            left: 0,
            top: 0,
            width: sw,
            height: sh,
        };
        let Some((px, py)) = mac_to_physical_in_viewport(v, h, content, size.width, size.height)
        else {
            return;
        };
        if window
            .set_cursor_position(winit::dpi::PhysicalPosition::new(px, py))
            .is_ok()
        {
            self.mouse_physical = (px, py);
            self.mouse_guest_offset = (0, 0);
        } else {
            let (host_v, host_h) =
                self.physical_to_mac(self.mouse_physical.0, self.mouse_physical.1);
            self.mouse_guest_offset = (v.saturating_sub(host_v), h.saturating_sub(host_h));
        }
        self.send_command(runtime_protocol::GuiCommand::AcknowledgeWarp {
            generation: self.guest_state.generation,
            serial: warp.serial,
        });
        self.guest_state.warp = None;
    }

    #[cfg(target_os = "macos")]
    fn sync_native_application_identity(&mut self) {
        if !self.native_integrations {
            return;
        }

        let Some(identity) = self.guest_state.identity.as_ref() else {
            return;
        };
        if self.native_app_path.as_deref() == Some(identity.path.as_str()) {
            return;
        }

        if let Some(native_menu) = self.native_menu.as_mut() {
            native_menu.set_app_name(identity.name.clone());
        }
        native_application::set_application_icon(identity.icon.as_ref());
        if let Some(window) = &self.window {
            window.set_title(&identity.name);
        }
        self.native_app_path = Some(identity.path.clone());
        self.native_app_name = identity.name.clone();
        self.native_app_icon = identity.icon.clone();
    }

    fn update_debug_frame_stats(&mut self, now: std::time::Instant) {
        let Some(previous) = self.debug_last_frame_at.replace(now) else {
            return;
        };
        let delta = now.saturating_duration_since(previous).as_secs_f64();
        if delta <= 0.0 {
            return;
        }
        let frame_ms = delta * 1000.0;
        let smoothed_ms = self
            .debug_frame_ms
            .map(|current| current.mul_add(0.8, frame_ms * 0.2))
            .unwrap_or(frame_ms);
        self.debug_frame_ms = Some(smoothed_ms);
        self.debug_host_fps = Some(1000.0 / smoothed_ms);
    }

    #[cfg(target_os = "windows")]
    fn retry_gpu_present(&mut self) {
        let Some(pending) = self.gpu_pending else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            gpu.cancel_retry();
            return;
        }
        if pending.size != (size.width, size.height) {
            // The resize event prepares the new geometry. Never submit an
            // old-size pending image into the replacement drawable.
            gpu.cancel_retry();
            self.force_next_render = true;
            return;
        }
        let _timing = FramePhaseTimer::new("GPU readiness retry work");
        match gpu.present(&self.gpu_frame, pending.size, pending.rect) {
            Ok(true) => {
                self.gpu_pending = None;
                self.last_presented_guest_tick = Some(pending.guest_tick);
            }
            Ok(false) => {}
            Err(message) => {
                eprintln!("[GPU] {message}; switching to software presentation");
                self.gpu = None;
                self.gpu_pending = None;
                let window = self.window.as_ref().unwrap().clone();
                let context = softbuffer::Context::new(window.clone())
                    .expect("Failed to create software context");
                self.surface = Some(
                    Surface::new(&context, window).expect("Failed to create software surface"),
                );
                self.surface_size = None;
                self.force_next_render = true;
                self.present_frame(std::time::Instant::now());
            }
        }
    }

    fn render_frame(&mut self) {
        let _timing = FramePhaseTimer::new("render frame (main thread)");
        let render_start = std::time::Instant::now();
        self.update_debug_frame_stats(render_start);
        let size = {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            window.inner_size()
        };
        if size.width == 0 || size.height == 0 {
            return;
        }
        #[cfg(target_os = "macos")]
        let capture_crop =
            should_learn_content_rect(self.debug_overlay_visible, self.native_integrations);
        #[cfg(not(target_os = "macos"))]
        let capture_crop = false;
        #[cfg(target_os = "macos")]
        let learning_crop = self.content_rect.is_none() || self.content_rect_relearn_after_full;
        #[cfg(not(target_os = "macos"))]
        let learning_crop = false;
        if !self.driver.as_mut().expect("serial runtime").capture_frame(
            &mut self.frame,
            self.debug_overlay_visible
                .then_some(DebugOverlayFrameStats {
                    host_fps: self.debug_host_fps,
                    frame_ms: self.debug_frame_ms,
                    ..DebugOverlayFrameStats::default()
                }),
            capture_crop,
            learning_crop,
        ) {
            return;
        }
        self.present_frame(render_start);
    }

    /// Present only complete owned data; no guest execution or memory reads.
    fn present_frame(&mut self, render_start: std::time::Instant) {
        let _timing = FramePhaseTimer::new("owned frame presentation");
        #[cfg(target_os = "macos")]
        let force_gpu_present = self.force_gpu_present;
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let frame = &self.frame;
        let presented_tick = frame.guest_tick;
        let (_, _, scrn_right, scrn_bottom, _) = frame.screen.screen_mode;
        let game_w = scrn_right as u32;
        let game_h = scrn_bottom as u32;
        let mut buf_w = size.width;
        let mut buf_h = size.height;
        #[cfg(target_os = "macos")]
        let mut presentation_rect = ContentRect {
            left: 0,
            top: 0,
            width: game_w,
            height: game_h,
        };

        #[cfg(target_os = "macos")]
        let mut core_animation_transaction: Option<CoreAnimationTransaction> = None;

        if buf_w == 0 || buf_h == 0 || game_w == 0 || game_h == 0 {
            return;
        }

        let screen_mode = frame.screen.screen_mode;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let cursor = if self.host_cursor.enabled() {
            None
        } else {
            frame.cursor.clone()
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let cursor = frame.cursor.clone();
        let mouse_pos = frame.mouse_position;

        #[cfg(target_os = "macos")]
        if should_learn_content_rect(self.debug_overlay_visible, self.native_integrations) {
            let framebuffer = frame.screen.pixels.as_slice();
            // A resize can present this packet repeatedly. Crop confirmation
            // counts completed owner frames, not host redraw callbacks.
            let key = (frame.generation, frame.sequence);
            if self.content_rect_frame != Some(key) {
                self.content_rect_frame = Some(key);
                let screen_signature = (screen_mode.2, screen_mode.3, screen_mode.4);
                if self.content_rect_screen_mode != Some(screen_signature) {
                    self.content_rect_screen_mode = Some(screen_signature);
                    self.content_rect = None;
                    self.content_rect_candidate = None;
                    self.content_rect_copybits_count = 0;
                    self.content_rect_active_margin_frames = 0;
                    self.content_rect_margin_cache = None;
                    self.content_rect_margin_refresh = 0;
                    self.content_rect_relearn_after_full = false;
                    self.content_rect_previous_frame.clear();
                }

                let full_screen = ContentRect {
                    left: 0,
                    top: 0,
                    width: game_w,
                    height: game_h,
                };
                let visible_dialog = frame.crop.dialog_bounds.is_some();
                // The histogram walks every framebuffer byte, so a crop that is
                // already accepted only needs it refreshed every
                // CONTENT_RECT_MARGIN_REFRESH_FRAMES frames. A reused verdict can be
                // that many frames stale, so paint entering the margins is noticed
                // up to one refresh period late.
                let mut margin_cache = self.content_rect_margin_cache;
                let mut margin_refresh = self.content_rect_margin_refresh;
                let mut has_inactive_margins = |rect: ContentRect| {
                    if let Some((cached_rect, verdict)) = margin_cache {
                        if cached_rect == rect && margin_refresh > 0 {
                            margin_refresh -= 1;
                            return verdict;
                        }
                    }
                    let verdict = content_rect_has_inactive_margins_8bpp(
                        framebuffer,
                        screen_mode.1 as usize,
                        usize::from(screen_mode.2),
                        usize::from(screen_mode.3),
                        rect,
                    );
                    margin_cache = Some((rect, verdict));
                    margin_refresh = CONTENT_RECT_MARGIN_REFRESH_FRAMES - 1;
                    verdict
                };
                let active_margin_crop = self.content_rect.filter(|&rect| {
                    rect != full_screen
                        && !visible_dialog
                        && screen_mode.4 == 8
                        && !has_inactive_margins(rect)
                });
                if active_margin_crop.is_some() {
                    self.content_rect_active_margin_frames =
                        self.content_rect_active_margin_frames.saturating_add(1);
                } else {
                    self.content_rect_active_margin_frames = 0;
                }
                let invalidated_crop = active_margin_crop.filter(|_| {
                    self.content_rect_active_margin_frames >= CONTENT_RECT_CONFIRMATIONS
                });
                let learning_content_rect =
                    self.content_rect.is_none() || self.content_rect_relearn_after_full;

                // A guest-drawn screen frame is stronger evidence than an
                // earlier inferred or cached crop. Keep looking for it after a
                // provisional crop has been accepted: some applications first
                // blit their unpositioned backing PixMap at (0,0), then draw the
                // actual presentation frame later in startup.
                let framed_rect = frame
                    .crop
                    .framed_rect
                    .and_then(|rect| content_rect_from_copybits(rect, game_w, game_h))
                    .filter(|&rect| {
                        !self.content_rect_relearn_after_full
                            || screen_mode.4 != 8
                            || has_inactive_margins(rect)
                    });
                let authoritative_rect = framed_rect.or_else(|| {
                    learning_content_rect.then(|| {
                        frame
                            .crop
                            .manual_rect
                            .or(frame.crop.declared_rect)
                            .and_then(|rect| content_rect_from_copybits(rect, game_w, game_h))
                            .filter(|&rect| screen_mode.4 != 8 || has_inactive_margins(rect))
                    })?
                });

                let mut accepted_rect = invalidated_crop.map(|_| full_screen);
                let allow_detection = !self.content_rect_relearn_after_full || !visible_dialog;
                let mut detected = None;
                if accepted_rect.is_none() {
                    if self.content_rect_relearn_after_full {
                        if allow_detection {
                            detected = authoritative_rect.map(|rect| (rect, 1));
                        }
                    } else {
                        accepted_rect = authoritative_rect;
                    }
                }
                if learning_content_rect && accepted_rect.is_none() {
                    let copybits_count = frame.crop.copybits_count;
                    if detected.is_none()
                        && allow_detection
                        && copybits_count != self.content_rect_copybits_count
                    {
                        let delta = copybits_count.saturating_sub(self.content_rect_copybits_count);
                        let confirmations = if self.content_rect_relearn_after_full {
                            1
                        } else {
                            delta.min(u64::from(u16::MAX)) as u16
                        };
                        self.content_rect_copybits_count = copybits_count;
                        detected = frame
                            .crop
                            .last_copybits_rect
                            .and_then(|rect| content_rect_from_copybits(rect, game_w, game_h))
                            .filter(|&rect| screen_mode.4 != 8 || has_inactive_margins(rect))
                            .map(|rect| (rect, confirmations));
                    }
                    if detected.is_none()
                        && allow_detection
                        && screen_mode.4 == 8
                        && self.content_rect_previous_frame.as_slice() != framebuffer
                    {
                        detected = detect_centered_content_rect_8bpp(
                            framebuffer,
                            screen_mode.1 as usize,
                            usize::from(screen_mode.2),
                            usize::from(screen_mode.3),
                        )
                        .map(|rect| (rect, 1));
                        self.content_rect_previous_frame.clear();
                        self.content_rect_previous_frame
                            .extend_from_slice(framebuffer);
                    }
                    if let Some((candidate, confirmations)) = detected {
                        self.content_rect_candidate = match self.content_rect_candidate {
                            Some((previous, count)) if previous == candidate => {
                                Some((candidate, count.saturating_add(confirmations)))
                            }
                            _ => Some((candidate, confirmations)),
                        };
                        let required_confirmations = if self.content_rect_relearn_after_full {
                            CONTENT_RECT_RELEARN_CONFIRMATIONS
                        } else {
                            CONTENT_RECT_CONFIRMATIONS
                        };
                        accepted_rect = self
                            .content_rect_candidate
                            .filter(|(_, count)| *count >= required_confirmations)
                            .map(|(rect, _)| rect);
                    } else if self.content_rect_relearn_after_full {
                        self.content_rect_candidate = None;
                    }
                }

                if let Some(rect) = accepted_rect.filter(|rect| self.content_rect != Some(*rect)) {
                    let replacing_provisional_crop = self.content_rect.is_some();
                    if let Some(previous) = invalidated_crop {
                        eprintln!(
                        "[SYSTEMLESS] Guest content expanded from {}x{} at ({},{}) to the full {}x{} screen after persistent margin drawing",
                        previous.width,
                        previous.height,
                        previous.left,
                        previous.top,
                        game_w,
                        game_h
                    );
                    } else if replacing_provisional_crop {
                        eprintln!(
                        "[SYSTEMLESS] Guest content updated from explicit frame: {}x{} at ({},{}) inside {}x{}",
                        rect.width, rect.height, rect.left, rect.top, game_w, game_h
                    );
                    } else {
                        eprintln!(
                            "[SYSTEMLESS] Guest content: {}x{} at ({},{}) inside {}x{}",
                            rect.width, rect.height, rect.left, rect.top, game_w, game_h
                        );
                    }
                    self.content_rect = Some(rect);
                    self.content_rect_candidate = None;
                    self.content_rect_active_margin_frames = 0;
                    self.content_rect_relearn_after_full = invalidated_crop.is_some();
                    persist_content_rect(&self.game_path, screen_mode, rect);
                    let rect = presentation_content_rect(
                        rect,
                        None,
                        game_w,
                        game_h,
                        frame.crop.hidden_menu_height,
                    );
                    if let Some(window) = self.window.as_ref() {
                        if let Some(scale) = window_guest_resize_scale(window, self.display_scale) {
                            let _ = window.request_inner_size(guest_scaled_physical_size(
                                rect.width,
                                rect.height,
                                scale,
                            ));
                        }
                    }
                    self.window_sized_content_rect = Some(rect);
                }
                self.content_rect_margin_cache = margin_cache;
                self.content_rect_margin_refresh = margin_refresh;
            }

            let stable_content = self.content_rect.unwrap_or(ContentRect {
                left: 0,
                top: 0,
                width: game_w,
                height: game_h,
            });
            let stable_content = presentation_content_rect(
                stable_content,
                None,
                game_w,
                game_h,
                frame.crop.hidden_menu_height,
            );
            let desired_content = presentation_content_rect(
                stable_content,
                frame.crop.dialog_bounds,
                game_w,
                game_h,
                frame.crop.hidden_menu_height,
            );
            let allow_guest_resize = self.window.as_ref().is_some_and(|window| {
                window_guest_resize_scale(window, self.display_scale).is_some()
            });
            if !allow_guest_resize {
                self.pending_window_transition = None;
                self.transient_window_restore_geometry = None;
                self.window_sized_content_rect = Some(desired_content);
            }
            if let Some(pending) = self.pending_window_transition {
                if pending.content != desired_content {
                    // The dialog changed or closed before AppKit delivered the
                    // prior resize. Replace that transition below.
                    self.pending_window_transition = None;
                } else if self.window_resize_events >= pending.required_resize_event
                    && size == pending.target_size
                {
                    self.window_sized_content_rect = Some(pending.content);
                    self.pending_window_transition = None;
                    if pending.content == stable_content {
                        self.transient_window_restore_geometry = None;
                    }
                } else {
                    // Retain the last complete drawable. Presenting the live
                    // guest framebuffer here would expose the dialog through
                    // the old crop for one or two display frames.
                    self.force_next_render = true;
                    return;
                }
            }

            // Stable crop changes are sized by the detector above. During
            // startup that detector may temporarily clear `content_rect` while
            // the native window still has the cached crop; that is not a
            // transient-dialog restore and has no saved geometry to restore.
            let transition_needed = if desired_content == stable_content {
                self.transient_window_restore_geometry.is_some()
            } else {
                self.window_sized_content_rect != Some(desired_content)
            };
            if !transition_needed && self.window_sized_content_rect != Some(desired_content) {
                // Guest writes to MBarHeight can change the visible rows even
                // when the learned gameplay crop and screen mode are unchanged.
                if let Some(window) = self.window.as_ref() {
                    if let Some(scale) = window_guest_resize_scale(window, self.display_scale) {
                        let _ = window.request_inner_size(guest_scaled_physical_size(
                            desired_content.width,
                            desired_content.height,
                            scale,
                        ));
                    }
                }
                self.window_sized_content_rect = Some(desired_content);
            }
            if transition_needed && allow_guest_resize {
                let (target_size, target_position) = if desired_content != stable_content {
                    if self.transient_window_restore_geometry.is_none() {
                        self.transient_window_restore_geometry = Some(TransientWindowGeometry {
                            inner_size: size,
                            outer_position: self
                                .window
                                .as_ref()
                                .and_then(|window| window.outer_position().ok()),
                        });
                    }
                    let original = self
                        .transient_window_restore_geometry
                        .expect("transient geometry was initialized");
                    let target_size = native_size_preserving_guest_scale(
                        stable_content,
                        desired_content,
                        original.inner_size,
                    );
                    let target_position = original.outer_position.map(|original_position| {
                        native_position_preserving_guest_anchor(
                            stable_content,
                            desired_content,
                            original.inner_size,
                            target_size,
                            original_position,
                        )
                    });
                    (target_size, target_position)
                } else {
                    let restore = self
                        .transient_window_restore_geometry
                        .expect("stable transition has geometry to restore");
                    (restore.inner_size, restore.outer_position)
                };

                // AppKit changes the layer bounds synchronously. Present the
                // correctly sized replacement drawable in the same Core
                // Animation transaction so the compositor never exposes the
                // old drawable re-centered in the new window for one frame.
                let transaction = CoreAnimationTransaction::begin();
                if let Some(surface) = self.surface.as_ref() {
                    surface.set_transactional_presentation(true);
                }
                let changed_atomically = self.window.as_ref().is_some_and(|window| {
                    target_position.is_some_and(|position| {
                        set_macos_window_geometry(window, target_size, position)
                    })
                });
                if changed_atomically {
                    self.window_sized_content_rect = Some(desired_content);
                    self.pending_window_transition = None;
                    if desired_content == stable_content {
                        self.transient_window_restore_geometry = None;
                    }
                    buf_w = target_size.width;
                    buf_h = target_size.height;
                    core_animation_transaction = Some(transaction);
                } else {
                    drop(transaction);
                    if let Some(surface) = self.surface.as_ref() {
                        surface.set_transactional_presentation(false);
                    }
                    if let Some(window) = self.window.as_ref() {
                        let _ = window.request_inner_size(target_size);
                        if let Some(position) = target_position {
                            window.set_outer_position(position);
                        }
                    }
                    self.pending_window_transition = Some(PendingWindowTransition {
                        content: desired_content,
                        target_size,
                        required_resize_event: self.window_resize_events.saturating_add(1),
                    });
                    self.force_next_render = true;
                    return;
                }
            }
            let content = self.window_sized_content_rect.unwrap_or(stable_content);
            presentation_rect = content;
            let palette = &frame.screen.palette;
            if let Some(surface) = self.surface.as_mut().filter(|_| frame.retained.is_none()) {
                let presented_directly = surface
                    .present_guest_frame(
                        framebuffer,
                        screen_mode,
                        (content.left, content.top, content.width, content.height),
                        palette,
                        cursor.as_ref().map(|image| (image, mouse_pos)),
                        (buf_w, buf_h),
                        force_gpu_present,
                    )
                    .expect("Failed to present native guest framebuffer");
                if let Some(transaction) = core_animation_transaction.take() {
                    drop(transaction);
                    surface.set_transactional_presentation(false);
                }
                if presented_directly {
                    self.last_presented_guest_tick = Some(presented_tick);
                    self.force_next_render = false;
                    self.force_gpu_present = false;
                    self.render_headroom = GuiDriver::next_render_headroom(render_start.elapsed());
                    return;
                }
            }
        }

        let mut retained = frame.retained.clone();
        #[cfg(target_os = "windows")]
        let compact_ready = self.gpu.is_some()
            && retained.is_some()
            && cursor.is_none()
            && !self.debug_overlay_visible;
        #[cfg(not(target_os = "windows"))]
        let compact_ready = false;
        let mut frame_argb = std::mem::take(&mut self.frame_argb);
        if !compact_ready {
            frame.screen.render_argb(&mut frame_argb);
            let guest = retained
                .as_ref()
                .filter(|_| cursor.is_some() || self.debug_overlay_visible)
                .map(|_| refresh_guest_frame(&mut self.guest_frame_argb, &frame_argb));
            if let Some(cursor) = cursor.as_ref() {
                display::render_cursor_argb(&mut frame_argb, game_w, game_h, cursor, mouse_pos);
            }
            if self.debug_overlay_visible {
                display::render_debug_overlay_argb(
                    &mut frame_argb,
                    game_w,
                    game_h,
                    &frame.debug_lines,
                );
            }
            if let (Some(guest), Some(image)) = (guest.as_ref(), retained.as_mut()) {
                if !std::sync::Arc::make_mut(image).apply_overlay(guest, &frame_argb) {
                    retained = None;
                }
            }
            if let Some(guest) = guest {
                self.guest_frame_argb = guest;
            }
        }

        #[cfg(target_os = "windows")]
        if self.gpu.is_some() {
            let exported = if let Some(image) = retained.as_ref() {
                self.gpu_frame = image.clone();
                true
            } else {
                let output = std::sync::Arc::make_mut(&mut self.gpu_frame);
                output.width = game_w;
                output.height = game_h;
                output.scale = 1;
                output.cells.clear();
                output.cells.extend(frame_argb.iter().map(|p| p & 0xffffff));
                output.detail.clear();
                true
            };
            // This is a single latest-image slot. Preparation can overlap
            // display backpressure; a newer guest frame replaces an older
            // pending one without queuing extra GPU frames.
            self.gpu_pending = Some(PendingGpuFrame {
                size: (buf_w, buf_h),
                rect: aspect_fit_dimensions(game_w, game_h, buf_w, buf_h),
                guest_tick: presented_tick,
            });
            let result = if exported {
                self.gpu.as_mut().unwrap().present(
                    &self.gpu_frame,
                    (buf_w, buf_h),
                    aspect_fit_dimensions(game_w, game_h, buf_w, buf_h),
                )
            } else {
                Err("frame cannot use the opaque compact transport".into())
            };
            match result {
                Ok(submitted) => {
                    self.frame_argb = frame_argb;
                    if submitted {
                        self.gpu_pending = None;
                        self.last_presented_guest_tick = Some(presented_tick);
                    }
                    // The prepared image satisfies this redraw request. A
                    // readiness wake submits it without recompositing. Later
                    // input/expose requests must survive that submission.
                    self.force_next_render = false;
                    self.render_headroom = GuiDriver::next_render_headroom(render_start.elapsed());
                    return;
                }
                Err(message) => {
                    eprintln!("[GPU] {message}; switching to software presentation");
                    self.gpu = None;
                    self.gpu_pending = None;
                    let window = self.window.as_ref().unwrap().clone();
                    let context = softbuffer::Context::new(window.clone())
                        .expect("Failed to create software context");
                    self.surface = Some(
                        Surface::new(&context, window).expect("Failed to create software surface"),
                    );
                    self.surface_size = None;
                    if compact_ready {
                        // The fast export skipped both logical images. Rebuild
                        // them for this same frame before entering software
                        // presentation; self.frame_argb may hold an older size
                        // or screen, including after a resize/device failure.
                        frame.screen.render_argb(&mut frame_argb);
                    }
                }
            }
        }

        let mut presented = std::mem::take(&mut self.presentation_argb);
        #[cfg(target_os = "macos")]
        let logical_size = (presentation_rect.width, presentation_rect.height);
        #[cfg(not(target_os = "macos"))]
        let drawable_rect = aspect_fit_dimensions(game_w, game_h, buf_w, buf_h);
        #[cfg(target_os = "macos")]
        let output_scale = display::outline_output_scale(logical_size, (buf_w, buf_h));
        #[cfg(target_os = "macos")]
        let borrowed_size = if cursor.is_none()
            && !self.debug_overlay_visible
            && presentation_rect.left == 0
            && presentation_rect.top == 0
            && presentation_rect.width == game_w
            && presentation_rect.height == game_h
        {
            let target = (game_w * output_scale, game_h * output_scale);
            retained.as_ref().and_then(|image| {
                self.retained_frame_cache
                    .prepare(image, target)
                    .then_some(target)
            })
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        let borrowed_size: Option<(u32, u32)> = None;
        let mut used_outlines = false;
        #[allow(unused_variables)] // macOS crops by the physical presentation rectangle.
        let (game_w, game_h) = if let Some(size) = borrowed_size {
            size
        } else if let Some((width, height)) = retained.as_ref().and_then(|image| {
            let _timing = FramePhaseTimer::new("owned outline pixel expansion");
            #[cfg(target_os = "macos")]
            let target = (game_w * output_scale, game_h * output_scale);
            #[cfg(not(target_os = "macos"))]
            let target = (drawable_rect.2, drawable_rect.3);
            self.retained_frame_cache
                .render(image, target, &mut presented)
                .then_some(target)
        }) {
            #[cfg(target_os = "macos")]
            {
                let scale = width / game_w;
                presentation_rect.left *= scale;
                presentation_rect.top *= scale;
                presentation_rect.width *= scale;
                presentation_rect.height *= scale;
            }
            std::mem::swap(&mut frame_argb, &mut presented);
            used_outlines = true;
            (width, height)
        } else {
            (game_w, game_h)
        };

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.surface.as_mut() else {
                if used_outlines {
                    std::mem::swap(&mut frame_argb, &mut presented);
                }
                self.presentation_argb = presented;
                self.frame_argb = frame_argb;
                return;
            };
            let _timing = FramePhaseTimer::new("raster presentation submission");
            if let Some((width, height)) = borrowed_size {
                surface
                    .present(
                        self.retained_frame_cache.pixels(),
                        width,
                        height,
                        buf_w,
                        buf_h,
                    )
                    .expect("Failed to present Metal framebuffer");
            } else {
                frame_argb = surface
                    .present_owned(
                        frame_argb,
                        presentation_layout(presentation_rect, game_w),
                        buf_w,
                        buf_h,
                    )
                    .expect("Failed to present Metal framebuffer");
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let (draw_x, draw_y, draw_w, draw_h) = drawable_rect;
            let draw_x = draw_x as usize;
            let draw_y = draw_y as usize;
            let draw_w = draw_w as usize;
            let draw_h = draw_h as usize;
            let mut scaled_frame = std::mem::take(&mut self.scaled_frame);

            let Some(surface) = self.surface.as_mut() else {
                if used_outlines {
                    std::mem::swap(&mut frame_argb, &mut presented);
                }
                self.presentation_argb = presented;
                self.frame_argb = frame_argb;
                self.scaled_frame = scaled_frame;
                return;
            };

            if self.surface_size != Some((buf_w, buf_h)) {
                surface
                    .resize(
                        NonZeroU32::new(buf_w).unwrap(),
                        NonZeroU32::new(buf_h).unwrap(),
                    )
                    .expect("Failed to resize surface");
                self.surface_size = Some((buf_w, buf_h));
            }

            let mut buffer = surface.buffer_mut().expect("Failed to get buffer");

            if draw_x != 0 || draw_y != 0 || draw_w != buf_w as usize || draw_h != buf_h as usize {
                buffer.fill(0xFF000000);
            }

            if draw_w == game_w as usize && draw_h == game_h as usize {
                for row in 0..game_h as usize {
                    let src_row = &frame_argb[row * game_w as usize..(row + 1) * game_w as usize];
                    let dst_offset = (draw_y + row) * buf_w as usize + draw_x;
                    buffer[dst_offset..dst_offset + game_w as usize].copy_from_slice(src_row);
                }
            } else {
                display::resize_argb_coverage(
                    &frame_argb,
                    (game_w, game_h),
                    (draw_w as u32, draw_h as u32),
                    &mut scaled_frame,
                );
                for row in 0..draw_h {
                    let dst_offset = (draw_y + row) * buf_w as usize + draw_x;
                    buffer[dst_offset..dst_offset + draw_w]
                        .copy_from_slice(&scaled_frame[row * draw_w..(row + 1) * draw_w]);
                }
            }

            self.scaled_frame = scaled_frame;
            buffer.present().expect("Failed to present buffer");
        }

        if used_outlines {
            std::mem::swap(&mut frame_argb, &mut presented);
        }
        self.presentation_argb = presented;
        self.frame_argb = frame_argb;
        self.last_presented_guest_tick = Some(presented_tick);
        self.force_next_render = false;
        self.render_headroom = GuiDriver::next_render_headroom(render_start.elapsed());
    }
}

#[allow(dead_code)]
fn aspect_fit_dimensions(
    source_width: u32,
    source_height: u32,
    drawable_width: u32,
    drawable_height: u32,
) -> (u32, u32, u32, u32) {
    if source_width == 0 || source_height == 0 || drawable_width == 0 || drawable_height == 0 {
        return (0, 0, 0, 0);
    }

    let width_limited = u64::from(drawable_width) * u64::from(source_height)
        <= u64::from(drawable_height) * u64::from(source_width);
    let (width, height) = if width_limited {
        (
            drawable_width,
            (u64::from(drawable_width) * u64::from(source_height) / u64::from(source_width)) as u32,
        )
    } else {
        (
            (u64::from(drawable_height) * u64::from(source_width) / u64::from(source_height))
                as u32,
            drawable_height,
        )
    };
    (
        (drawable_width - width) / 2,
        (drawable_height - height) / 2,
        width,
        height,
    )
}

fn physical_to_mac_in_viewport(
    px: f64,
    py: f64,
    content: ContentRect,
    drawable_width: u32,
    drawable_height: u32,
) -> (i16, i16) {
    if content.width == 0 || content.height == 0 || drawable_width == 0 || drawable_height == 0 {
        return (0, 0);
    }
    let scale = (drawable_width as f64 / content.width as f64)
        .min(drawable_height as f64 / content.height as f64);
    let viewport_width = content.width as f64 * scale;
    let viewport_height = content.height as f64 * scale;
    let origin_x = (drawable_width as f64 - viewport_width) * 0.5;
    let origin_y = (drawable_height as f64 - viewport_height) * 0.5;
    let mac_x = content.left as i32 + ((px - origin_x) / scale).floor() as i32;
    let mac_y = content.top as i32 + ((py - origin_y) / scale).floor() as i32;
    (
        mac_y.clamp(
            content.top as i32,
            (content.top + content.height - 1) as i32,
        ) as i16,
        mac_x.clamp(
            content.left as i32,
            (content.left + content.width - 1) as i32,
        ) as i16,
    )
}

fn mac_to_physical_in_viewport(
    v: i16,
    h: i16,
    content: ContentRect,
    drawable_width: u32,
    drawable_height: u32,
) -> Option<(f64, f64)> {
    if content.width == 0 || content.height == 0 || drawable_width == 0 || drawable_height == 0 {
        return None;
    }
    let scale = (drawable_width as f64 / content.width as f64)
        .min(drawable_height as f64 / content.height as f64);
    let origin_x = (drawable_width as f64 - content.width as f64 * scale) * 0.5;
    let origin_y = (drawable_height as f64 - content.height as f64 * scale) * 0.5;
    Some((
        origin_x + (f64::from(h) - f64::from(content.left) + 0.5) * scale,
        origin_y + (f64::from(v) - f64::from(content.top) + 0.5) * scale,
    ))
}

/// Rebuild the pre-overlay snapshot in a buffer `App` owns.
///
/// The outline present path diffs the software frame against the pixels the
/// host overlays wrote over it, so the snapshot is rewritten every frame; only
/// the allocation is reused.
fn refresh_guest_frame(pool: &mut Vec<u32>, frame: &[u32]) -> Vec<u32> {
    let mut snapshot = std::mem::take(pool);
    snapshot.clear();
    snapshot.extend_from_slice(frame);
    snapshot
}

#[cfg(target_os = "macos")]
/// Where the presented content rectangle sits inside a whole raster frame.
///
/// `frame_width` is the buffer's row length, which the presenter uses as the
/// upload stride: it reads the rectangle in place instead of receiving a
/// cropped copy of the frame.
fn presentation_layout(content: ContentRect, frame_width: u32) -> metal_present::RasterLayout {
    metal_present::RasterLayout {
        left: content.left,
        top: content.top,
        width: content.width,
        height: content.height,
        buffer_width: frame_width,
    }
}

#[cfg(target_os = "macos")]
fn presentation_content_rect(
    base: ContentRect,
    transient_bounds: Option<(i16, i16, i16, i16)>,
    screen_width: u32,
    screen_height: u32,
    hidden_menu_height: u32,
) -> ContentRect {
    let content = expanded_content_rect(base, transient_bounds, screen_width, screen_height);
    // MBarHeight is guest geometry, not a fixed 20-pixel constant. Keep it
    // intact in guest memory and exclude it only when presenting native menus.
    // Inside Macintosh Volume V, V-245.
    if hidden_menu_height == 0 || hidden_menu_height >= screen_height {
        return content;
    }
    let bottom = content.top.saturating_add(content.height);
    let top = content.top.max(hidden_menu_height);
    if top >= bottom {
        return ContentRect {
            left: 0,
            top: hidden_menu_height,
            width: screen_width,
            height: screen_height - hidden_menu_height,
        };
    }
    ContentRect {
        top,
        height: bottom - top,
        ..content
    }
}

/// Extend a stable gameplay crop just enough to include transient system UI.
/// The learned/cached rectangle remains unchanged, so dismissing a dialog
/// restores the normal viewport without relearning it or resizing the native
/// window.
#[cfg(target_os = "macos")]
fn expanded_content_rect(
    base: ContentRect,
    transient_bounds: Option<(i16, i16, i16, i16)>,
    screen_width: u32,
    screen_height: u32,
) -> ContentRect {
    let Some((top, left, bottom, right)) = transient_bounds else {
        return base;
    };
    let transient_left = i32::from(left).clamp(0, screen_width as i32) as u32;
    let transient_top = i32::from(top).clamp(0, screen_height as i32) as u32;
    let transient_right = i32::from(right).clamp(0, screen_width as i32) as u32;
    let transient_bottom = i32::from(bottom).clamp(0, screen_height as i32) as u32;
    if transient_right <= transient_left || transient_bottom <= transient_top {
        return base;
    }

    let left = base.left.min(transient_left);
    let top = base.top.min(transient_top);
    let right = base
        .left
        .saturating_add(base.width)
        .max(transient_right)
        .min(screen_width);
    let bottom = base
        .top
        .saturating_add(base.height)
        .max(transient_bottom)
        .min(screen_height);
    ContentRect {
        left,
        top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

#[cfg(target_os = "macos")]
fn native_size_preserving_guest_scale(
    stable_content: ContentRect,
    presentation_content: ContentRect,
    original_size: winit::dpi::PhysicalSize<u32>,
) -> winit::dpi::PhysicalSize<u32> {
    if stable_content.width == 0 || stable_content.height == 0 {
        return original_size;
    }
    let scale = (original_size.width as f64 / stable_content.width as f64)
        .min(original_size.height as f64 / stable_content.height as f64);
    winit::dpi::PhysicalSize::new(
        original_size
            .width
            .max((presentation_content.width as f64 * scale).ceil() as u32),
        original_size
            .height
            .max((presentation_content.height as f64 * scale).ceil() as u32),
    )
}

/// Move a transiently enlarged window so the stable gameplay crop remains at
/// the same desktop coordinates. Without this adjustment, adding guest pixels
/// above or to the left of the crop makes macOS keep the outer top-left fixed
/// and visibly pushes the gameplay down or right.
#[cfg(target_os = "macos")]
fn native_position_preserving_guest_anchor(
    stable_content: ContentRect,
    presentation_content: ContentRect,
    original_size: winit::dpi::PhysicalSize<u32>,
    target_size: winit::dpi::PhysicalSize<u32>,
    original_position: winit::dpi::PhysicalPosition<i32>,
) -> winit::dpi::PhysicalPosition<i32> {
    let guest_anchor = |content: ContentRect, size: winit::dpi::PhysicalSize<u32>| {
        if content.width == 0 || content.height == 0 {
            return (0.0, 0.0);
        }
        let scale = (size.width as f64 / content.width as f64)
            .min(size.height as f64 / content.height as f64);
        let viewport_width = content.width as f64 * scale;
        let viewport_height = content.height as f64 * scale;
        let viewport_left = (size.width as f64 - viewport_width) * 0.5;
        let viewport_top = (size.height as f64 - viewport_height) * 0.5;
        (
            viewport_left + stable_content.left.saturating_sub(content.left) as f64 * scale,
            viewport_top + stable_content.top.saturating_sub(content.top) as f64 * scale,
        )
    };
    let original_anchor = guest_anchor(stable_content, original_size);
    let transient_anchor = guest_anchor(presentation_content, target_size);
    winit::dpi::PhysicalPosition::new(
        original_position.x + (original_anchor.0 - transient_anchor.0).round() as i32,
        original_position.y + (original_anchor.1 - transient_anchor.1).round() as i32,
    )
}

/// Apply the native content size and desktop position in one NSWindow frame
/// mutation. Calling winit's size and position setters separately exposes an
/// intermediate window geometry to the compositor, making the old drawable
/// jump before its correctly cropped replacement is ready.
#[cfg(target_os = "macos")]
fn set_macos_window_geometry(
    window: &Window,
    target_inner_size: winit::dpi::PhysicalSize<u32>,
    target_outer_position: winit::dpi::PhysicalPosition<i32>,
) -> bool {
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    let Ok(current_outer_position) = window.outer_position() else {
        return false;
    };
    let scale = window.scale_factor();
    if scale <= 0.0 {
        return false;
    }

    // SAFETY: winit owns the NSView and its NSWindow for the duration of this
    // call. The GUI runner invokes this only on AppKit's main thread. CGRect
    // is ABI-compatible with NSRect on 64-bit macOS.
    unsafe {
        let view: &NSObject = handle.ns_view.cast().as_ref();
        let native_window: *mut NSObject = msg_send![view, window];
        let Some(native_window) = native_window.as_ref() else {
            return false;
        };
        let current_frame: objc2_foundation::CGRect = msg_send![native_window, frame];
        let content_rect = objc2_foundation::CGRect::new(
            objc2_foundation::CGPoint::new(0.0, 0.0),
            objc2_foundation::CGSize::new(
                target_inner_size.width as f64 / scale,
                target_inner_size.height as f64 / scale,
            ),
        );
        let mut target_frame: objc2_foundation::CGRect =
            msg_send![native_window, frameRectForContentRect: content_rect];
        let delta_x = f64::from(target_outer_position.x - current_outer_position.x) / scale;
        let delta_y = f64::from(target_outer_position.y - current_outer_position.y) / scale;
        target_frame.origin.x = current_frame.origin.x + delta_x;
        target_frame.origin.y =
            current_frame.origin.y + current_frame.size.height - target_frame.size.height - delta_y;
        let _: () = msg_send![native_window, setFrame: target_frame display: false animate: false];
    }
    true
}

fn detect_centered_content_rect_8bpp(
    framebuffer: &[u8],
    row_bytes: usize,
    width: usize,
    height: usize,
) -> Option<ContentRect> {
    if width < 32 || height < 32 || row_bytes < width || framebuffer.len() < row_bytes * height {
        return None;
    }

    let corners = [
        framebuffer[0],
        framebuffer[width - 1],
        framebuffer[(height - 1) * row_bytes],
        framebuffer[(height - 1) * row_bytes + width - 1],
    ];
    let background = corners
        .iter()
        .copied()
        .find(|candidate| corners.iter().filter(|value| *value == candidate).count() >= 3)?;
    let row_threshold = (width / 64).max(8);
    let row_has_content = |row: usize| {
        framebuffer[row * row_bytes..row * row_bytes + width]
            .iter()
            .filter(|&&pixel| pixel != background)
            .take(row_threshold)
            .count()
            >= row_threshold
    };
    let top = (0..height).find(|&row| row_has_content(row))?;
    let bottom = (0..height)
        .rev()
        .find(|&row| row_has_content(row))?
        .saturating_add(1);

    let column_threshold = ((bottom - top) / 64).max(8);
    let column_has_content = |column: usize| {
        (top..bottom)
            .filter(|&row| framebuffer[row * row_bytes + column] != background)
            .take(column_threshold)
            .count()
            >= column_threshold
    };
    let left = (0..width).find(|&column| column_has_content(column))?;
    let right = (0..width)
        .rev()
        .find(|&column| column_has_content(column))?
        .saturating_add(1);

    let right_margin = width - right;
    let bottom_margin = height - bottom;
    let horizontal_tolerance = (width / 100).max(4);
    let vertical_tolerance = (height / 100).max(4);
    let content_width = right - left;
    let content_height = bottom - top;
    if left < 4
        || right_margin < 4
        || top < 4
        || bottom_margin < 4
        || left.abs_diff(right_margin) > horizontal_tolerance
        || top.abs_diff(bottom_margin) > vertical_tolerance
        || content_width < width / 2
        || content_height < height / 2
    {
        return None;
    }

    Some(ContentRect {
        left: left as u32,
        top: top as u32,
        width: content_width as u32,
        height: content_height as u32,
    })
}

/// Return whether the pixels excluded by a proposed crop still look like a
/// letterbox border. A real border is dominated by one palette index; a HUD or
/// other independently drawn screen region contains substantial variation.
fn content_rect_has_inactive_margins_8bpp(
    framebuffer: &[u8],
    row_bytes: usize,
    width: usize,
    height: usize,
    content: ContentRect,
) -> bool {
    let Ok(left) = usize::try_from(content.left) else {
        return false;
    };
    let Ok(top) = usize::try_from(content.top) else {
        return false;
    };
    let Ok(content_width) = usize::try_from(content.width) else {
        return false;
    };
    let Ok(content_height) = usize::try_from(content.height) else {
        return false;
    };
    let Some(right) = left.checked_add(content_width) else {
        return false;
    };
    let Some(bottom) = top.checked_add(content_height) else {
        return false;
    };
    if content_width == 0
        || content_height == 0
        || right > width
        || bottom > height
        || row_bytes < width
        || framebuffer.len() < row_bytes.saturating_mul(height)
    {
        return false;
    }
    if left == 0 && top == 0 && right == width && bottom == height {
        return true;
    }

    let mut histogram = [0usize; 256];
    let mut total = 0usize;
    let mut count_pixels = |pixels: &[u8]| {
        for &pixel in pixels {
            histogram[usize::from(pixel)] += 1;
        }
        total += pixels.len();
    };
    for row in 0..top {
        count_pixels(&framebuffer[row * row_bytes..row * row_bytes + width]);
    }
    for row in top..bottom {
        let pixels = &framebuffer[row * row_bytes..row * row_bytes + width];
        count_pixels(&pixels[..left]);
        count_pixels(&pixels[right..]);
    }
    for row in bottom..height {
        count_pixels(&framebuffer[row * row_bytes..row * row_bytes + width]);
    }

    let dominant = histogram.into_iter().max().unwrap_or(0);
    let active = total.saturating_sub(dominant);
    total != 0
        && active.saturating_mul(100) < total.saturating_mul(CONTENT_RECT_ACTIVE_MARGIN_PERCENT)
}

fn content_rect_from_copybits(
    rect: ScreenCopyBitsRect,
    screen_width: u32,
    screen_height: u32,
) -> Option<ContentRect> {
    if screen_width < 32 || screen_height < 32 {
        return None;
    }
    let left = u32::try_from(rect.dst_left).ok()?;
    let top = u32::try_from(rect.dst_top).ok()?;
    let right = u32::try_from(rect.dst_right).ok()?;
    let bottom = u32::try_from(rect.dst_bottom).ok()?;
    if right <= left || bottom <= top || right > screen_width || bottom > screen_height {
        return None;
    }
    let content_width = right - left;
    let content_height = bottom - top;
    let right_margin = screen_width - right;
    let bottom_margin = screen_height - bottom;
    let has_border = left >= 4 || right_margin >= 4 || top >= 4 || bottom_margin >= 4;
    if !has_border || content_width < screen_width / 2 || content_height < screen_height / 2 {
        return None;
    }
    Some(ContentRect {
        left,
        top,
        width: content_width,
        height: content_height,
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl App {
    /// Keep the host pointer in step with the guest cursor image, visibility,
    /// and the window's guest-to-screen scale.
    fn sync_host_cursor(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        // The cursor's guest-pixel scale must match the presentation viewport
        // (content rectangle + binding axis), not the raw window/guest ratio:
        // height-constrained windows, learned gameplay crops, and transient
        // dialog expansion all change it (issue #1049).
        let (_, _, sw, sh, _) = self.guest_state.screen_mode;
        let (sw, sh) = (u32::from(sw), u32::from(sh));
        #[cfg(target_os = "macos")]
        let content = if self.debug_overlay_visible {
            ContentRect {
                left: 0,
                top: 0,
                width: sw,
                height: sh,
            }
        } else {
            presentation_content_rect(
                self.content_rect.unwrap_or(ContentRect {
                    left: 0,
                    top: 0,
                    width: sw,
                    height: sh,
                }),
                self.guest_state.dialog_bounds,
                sw,
                sh,
                self.guest_state.hidden_menu_height,
            )
        };
        #[cfg(target_os = "windows")]
        let content = ContentRect {
            left: 0,
            top: 0,
            width: sw,
            height: sh,
        };
        let size = window.inner_size();
        let scale =
            host_cursor::presentation_scale(content.width, content.height, size.width, size.height);
        #[cfg(target_os = "macos")]
        self.host_cursor
            .sync(window, self.guest_state.cursor.as_ref(), scale);
        #[cfg(target_os = "windows")]
        self.host_cursor
            .sync(_event_loop, window, self.guest_state.cursor.as_ref(), scale);
    }
}

impl ApplicationHandler for App {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, (): ()) {
        // Upload the latest prepared image. Do not execute the guest, change
        // its clock, or redo CPU composition on this display readiness wake.
        #[cfg(target_os = "windows")]
        self.retry_gpu_present();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            #[cfg(target_os = "macos")]
            if self.native_integrations {
                native_application::set_application_icon(self.native_app_icon.as_ref());
            }
            #[cfg(target_os = "macos")]
            let initial_size = {
                let content = presentation_content_rect(
                    self.content_rect.unwrap_or(ContentRect {
                        left: 0,
                        top: 0,
                        width: initial_screen_width(),
                        height: initial_screen_height(),
                    }),
                    None,
                    initial_screen_width(),
                    initial_screen_height(),
                    self.guest_state.hidden_menu_height,
                );
                (content.width, content.height)
            };
            #[cfg(not(target_os = "macos"))]
            let initial_size = (initial_screen_width(), initial_screen_height());
            #[cfg(target_os = "macos")]
            let window_title = if self.native_startup_fallback.is_some() {
                "Systemless — Loading application…"
            } else if self.native_integrations {
                self.native_app_name.as_str()
            } else {
                "Systemless - Macintosh Emulator"
            };
            #[cfg(not(target_os = "macos"))]
            let window_title = "Systemless - Macintosh Emulator";
            let monitor = event_loop.primary_monitor();
            let initial_window_size = self.display_scale.map_or_else(
                || {
                    automatic_window_size(
                        initial_size.0,
                        initial_size.1,
                        monitor.as_ref().map(|monitor| monitor.size()),
                        monitor
                            .as_ref()
                            .map_or(1.0, |monitor| monitor.scale_factor()),
                    )
                },
                |scale| guest_scaled_physical_size(initial_size.0, initial_size.1, scale),
            );
            let window_attrs = Window::default_attributes()
                .with_title(window_title)
                .with_visible(false)
                .with_inner_size(initial_window_size)
                .with_resizable(true);
            let window_attrs = platform_window_attrs(window_attrs);

            let window = Rc::new(
                event_loop
                    .create_window(window_attrs)
                    .expect("Failed to create window"),
            );
            // The window manager may choose a different monitor from the
            // primary one. Resolve its actual DPI before showing the window.
            if self.display_scale.is_none() {
                let bounds = window_monitor_bounds(&window);
                let target = automatic_window_size(
                    initial_size.0,
                    initial_size.1,
                    bounds.map(|(_, size)| size),
                    window.scale_factor(),
                );
                let _ = window.request_inner_size(target);
                if let Some((origin, extent)) = bounds {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(
                        origin
                            .x
                            .saturating_add((extent.width.saturating_sub(target.width) / 2) as i32),
                        origin.y.saturating_add(
                            (extent.height.saturating_sub(target.height) / 2) as i32,
                        ),
                    ));
                }
            }
            // The guest cursor is the host pointer on macOS; it is drawn into
            // the frame elsewhere.
            #[cfg(target_os = "macos")]
            if !self.host_cursor.enabled() {
                window.set_cursor_visible(false);
            }
            #[cfg(not(target_os = "macos"))]
            window.set_cursor_visible(false);
            if self.start_fullscreen {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            window.set_visible(true);

            #[cfg(target_os = "macos")]
            let surface = metal_present::MetalPresenter::new(window.clone())
                .expect("Failed to create Metal presenter");
            #[cfg(target_os = "windows")]
            if std::env::var_os("SYSTEMLESS_D3D11").as_deref() != Some(std::ffi::OsStr::new("0")) {
                match d3d_present::D3dPresenter::new(
                    window.clone(),
                    self.gpu_wake.as_ref().unwrap().clone(),
                ) {
                    Ok(gpu) => {
                        eprintln!("[GPU] D3D11 compact coverage enabled");
                        self.gpu = Some(gpu);
                    }
                    Err(message) => eprintln!("[GPU] {message}; using software presentation"),
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_os = "windows")]
                let software = self.gpu.is_none();
                #[cfg(not(target_os = "windows"))]
                let software = true;
                if software {
                    let context =
                        softbuffer::Context::new(window.clone()).expect("Failed to create context");
                    self.surface = Some(
                        Surface::new(&context, window.clone()).expect("Failed to create surface"),
                    );
                }
            }
            #[cfg(target_os = "macos")]
            {
                self.surface = Some(surface);
            }
            self.window = Some(window);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.owner.is_none() && self.driver.is_none() {
                    event_loop.exit();
                    return;
                }
                if let Some(owner) = self.owner.as_ref() {
                    self.closing = true;
                    owner.mailbox.request_shutdown();
                    if let Some(window) = self.window.as_ref() {
                        window.set_visible(false);
                    }
                    self.poll_owner(event_loop);
                    return;
                }
                self.driver
                    .as_mut()
                    .expect("serial runtime")
                    .sync_save_files(true);
                eprintln!(
                    "[SYSTEMLESS] Window closed. Total instructions: {}",
                    self.driver
                        .as_mut()
                        .expect("serial runtime")
                        .total_instructions
                );
                systemless::runner::dump_wait_stats();
                event_loop.exit();
            }

            WindowEvent::CursorEntered { .. } => {
                #[cfg(target_os = "macos")]
                {
                    self.host_cursor.set_pointer_inside(true);
                    self.host_cursor.reassert();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse_guest_offset = (0, 0);
                #[cfg(target_os = "macos")]
                self.host_cursor.set_pointer_inside(false);
            }
            WindowEvent::CursorMoved { position, .. } => {
                #[cfg(target_os = "macos")]
                {
                    self.host_cursor.set_pointer_inside(true);
                    self.host_cursor.reassert();
                }
                self.force_next_render = true;
                self.mouse_physical = (position.x, position.y);
                let (v, h) = self.host_mouse_to_mac(position.x, position.y);
                self.send_command(runtime_protocol::GuiCommand::MouseMove { v, h });
                if let Some(driver) = self.driver.as_mut() {
                    driver.capture_state(&mut self.guest_state, false);
                }
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                self.sync_host_cursor(event_loop);
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.force_next_render = true;
                let (v, h) = self.host_mouse_to_mac(self.mouse_physical.0, self.mouse_physical.1);
                self.send_command(match state {
                    ElementState::Pressed => runtime_protocol::GuiCommand::MouseDown { v, h },
                    ElementState::Released => runtime_protocol::GuiCommand::MouseUp { v, h },
                });
            }

            WindowEvent::KeyboardInput { event, .. } => {
                self.force_next_render = true;
                let translated = host_keyboard_event_to_mac(
                    &event.logical_key,
                    &event.physical_key,
                    event.text.as_ref().map(|t| t.as_str()),
                );
                if std::env::var_os("SYSTEMLESS_TRACE_GUI_KEY").is_some() {
                    eprintln!(
                        "[GUI-KEY] state={:?} logical_key={:?} physical_key={:?} mac={:?} text={:?}",
                        event.state, event.logical_key, event.physical_key, translated, event.text,
                    );
                }
                let Some((mac_key, char_code)) = translated else {
                    return;
                };
                if matches!(event.physical_key, PhysicalKey::Code(KeyCode::F3)) {
                    if event.state == ElementState::Pressed && !event.repeat {
                        self.debug_overlay_visible = !self.debug_overlay_visible;
                        if self.debug_overlay_visible {
                            self.debug_last_frame_at = None;
                            self.debug_host_fps = None;
                            self.debug_frame_ms = None;
                        }
                    }
                    return;
                }
                self.send_command(match event.state {
                    ElementState::Pressed => runtime_protocol::GuiCommand::KeyDown {
                        key: mac_key,
                        character: char_code,
                    },
                    ElementState::Released => runtime_protocol::GuiCommand::KeyUp {
                        key: mac_key,
                        character: char_code,
                    },
                });
            }

            WindowEvent::Resized(size) => {
                self.force_next_render = true;
                #[cfg(target_os = "macos")]
                {
                    self.force_gpu_present = true;
                    self.window_resize_events = self.window_resize_events.saturating_add(1);
                }
                // Live resizing runs independently of the guest VBL. Present
                // the latest complete guest image at the new drawable size
                // immediately instead of stretching a stale drawable.
                if size.width != 0 && size.height != 0 && self.frame.sequence != 0 {
                    self.present_frame(std::time::Instant::now());
                }
            }

            WindowEvent::RedrawRequested => {
                self.force_next_render = true;
                #[cfg(target_os = "macos")]
                {
                    self.force_gpu_present = true;
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.owner.is_none() && self.driver.is_none() {
            self.show_runtime_error();
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if self.owner.is_some() {
            #[cfg(target_os = "windows")]
            if self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.retry_at())
                .is_some_and(|at| std::time::Instant::now() >= at)
            {
                self.retry_gpu_present();
            }
            #[cfg(target_os = "macos")]
            if let Some(native_menu) = self.native_menu.as_mut() {
                let commands = native_menu.drain_commands();
                for (menu, item) in commands {
                    self.send_command(runtime_protocol::GuiCommand::Menu { menu, item });
                }
            }
            self.poll_owner(event_loop);
            return;
        }
        {
            let driver = self.driver.as_mut().expect("serial runtime");
            driver.force_next_render |= self.force_next_render;
            driver.last_presented_guest_tick = self.last_presented_guest_tick;
            driver.render_headroom = self.render_headroom;
        }

        self.driver
            .as_mut()
            .expect("serial runtime")
            .pump_debugger();
        let now = std::time::Instant::now();
        let next = self
            .driver
            .as_mut()
            .expect("serial runtime")
            .next_frame_time
            .unwrap_or(now);

        #[cfg(target_os = "windows")]
        if self
            .gpu
            .as_ref()
            .and_then(|gpu| gpu.retry_at())
            .is_some_and(|at| now >= at)
        {
            self.retry_gpu_present();
        }

        if now < next {
            #[cfg(target_os = "windows")]
            let next = self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.retry_at())
                .map_or(next, |at| next.min(at));
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            return;
        }

        let _timing = FramePhaseTimer::new("host frame work");
        // Schedule the next host frame. If startup/resource loading makes us
        // miss a full presentation interval, drop the missed host frame instead
        // of running immediate catch-up frames that bunch audio and graphics.
        let (next_target, _) = GuiDriver::next_frame_target(now, next);
        self.driver
            .as_mut()
            .expect("serial runtime")
            .next_frame_time = Some(next_target);
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_target));

        #[cfg(target_os = "macos")]
        if let Some(native_menu) = self.native_menu.as_mut() {
            let commands = native_menu.drain_commands();
            for (menu, item) in commands {
                self.send_command(runtime_protocol::GuiCommand::Menu { menu, item });
            }
        }

        // Step emulation, then render
        self.driver.as_mut().expect("serial runtime").step_frame();
        self.driver
            .as_mut()
            .expect("serial runtime")
            .capture_state(&mut self.guest_state, true);
        self.sync_guest_cursor_warp();
        self.driver
            .as_mut()
            .expect("serial runtime")
            .flush_ready_mouse_release();
        if self
            .driver
            .as_mut()
            .expect("serial runtime")
            .guest_requested_exit()
        {
            if !self
                .driver
                .as_mut()
                .expect("serial runtime")
                .guest_exit_reported
            {
                self.driver
                    .as_mut()
                    .expect("serial runtime")
                    .sync_save_files(true);
                eprintln!(
                    "[SYSTEMLESS] Guest exited. Total instructions: {}",
                    self.driver
                        .as_mut()
                        .expect("serial runtime")
                        .total_instructions
                );
                self.driver
                    .as_mut()
                    .expect("serial runtime")
                    .guest_exit_reported = true;
            }
            if self
                .driver
                .as_mut()
                .expect("serial runtime")
                .debug_server
                .is_none()
            {
                event_loop.exit();
                return;
            }
        }
        self.driver
            .as_mut()
            .expect("serial runtime")
            .sync_save_files(false);

        self.sync_native_state(event_loop);

        self.force_next_render |= self
            .driver
            .as_ref()
            .expect("serial runtime")
            .force_next_render;
        self.driver
            .as_mut()
            .expect("serial runtime")
            .force_next_render = self.force_next_render;
        if self
            .driver
            .as_mut()
            .expect("serial runtime")
            .should_render_frame(self.debug_overlay_visible)
        {
            self.render_frame();
        }
        self.driver
            .as_mut()
            .expect("serial runtime")
            .force_next_render = self.force_next_render;
        self.driver.as_mut().expect("serial runtime").finish_frame();
        #[cfg(target_os = "windows")]
        if let Some(at) = self.gpu.as_ref().and_then(|gpu| gpu.retry_at()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_target.min(at)));
        }
    }
}

fn run_gui(
    game_path: PathBuf,
    arrows_as_numpad: bool,
    native_integrations: bool,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    display_scale: Option<u32>,
    ui_theme: UiThemeId,
    fullscreen: bool,
    debug_socket: Option<PathBuf>,
) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    eprintln!(
        "[SYSTEMLESS] GUI arrow keys: {}",
        if arrows_as_numpad {
            "keypad flight controls"
        } else {
            "literal Mac arrow keys"
        }
    );

    let threaded = std::env::var("SYSTEMLESS_DESKTOP_RUNTIME").as_deref() == Ok("thread");
    #[cfg(target_os = "macos")]
    let native_termination = if threaded {
        match native_termination::NativeTermination::install() {
            Ok(guard) => Some(guard),
            Err(error) => {
                eprintln!("[SYSTEMLESS] Cannot protect native Quit/save handling: {error}");
                return;
            }
        }
    } else {
        None
    };
    let config = runtime_owner::RuntimeConfig {
        #[cfg(target_os = "macos")]
        native_preflight: native_integrations && !native_bundle::already_relaunched(),
        game_path: game_path.clone(),
        arrows_as_numpad,
        native_integrations,
        addressing_24_bit,
        screen_depth,
        ui_theme,
        debug_socket: debug_socket.clone(),
    };
    let mut app = App::new_with_display_scale(
        game_path,
        arrows_as_numpad,
        native_integrations,
        addressing_24_bit,
        screen_depth,
        display_scale,
        ui_theme,
        fullscreen,
        threaded,
    );
    #[cfg(target_os = "windows")]
    {
        app.gpu_wake = Some(event_loop.create_proxy());
    }
    if threaded {
        let proxy = event_loop.create_proxy();
        #[cfg(target_os = "macos")]
        if config.native_preflight {
            app.native_startup_fallback = Some((config.clone(), proxy.clone()));
        }
        match runtime_owner::RuntimeOwner::spawn(config, move || {
            let _ = proxy.send_event(());
        }) {
            Ok(owner) => {
                #[cfg(target_os = "macos")]
                native_termination
                    .as_ref()
                    .unwrap()
                    .activate(owner.mailbox.clone());
                app.owner = Some(owner);
            }
            Err(error) => {
                let message = format!("Failed to start runtime owner thread: {error}");
                eprintln!("[SYSTEMLESS] {message}");
                app.runtime_error = Some(message);
            }
        }
    } else {
        if let Some(path) = debug_socket {
            match debug_server::DebugServer::bind(&path) {
                Ok(server) => {
                    app.driver.as_mut().expect("serial runtime").debug_server = Some(server)
                }
                Err(error) => {
                    eprintln!(
                        "Error: cannot bind debug socket {}: {error}",
                        path.display()
                    );
                    std::process::exit(1);
                }
            }
        }
        // `run_app` is the first point at which `resumed` can create a native
        // window. Finish archive decompression and guest initialization before
        // entering the event loop so startup never exposes an empty host window.
        app.init_game();
    }
    event_loop.run_app(&mut app).expect("Event loop failed");
    frame_metrics::flush();
}

#[cfg(target_os = "macos")]
fn relaunch_with_native_guest_identity(game_path: &std::path::Path) {
    match native_bundle::prepare_for_game(game_path) {
        Ok(Some(bundle)) => {
            let error = native_bundle::exec_bundle(&bundle);
            eprintln!("[SYSTEMLESS] Could not enter native bundle: {error}");
        }
        Ok(None) => {}
        Err(error) => eprintln!("[SYSTEMLESS] Native startup fallback: {error}"),
    }
}

fn save_screenshot(runner: &FixtureRunner, num: usize) {
    let (_, _, scrn_width, scrn_height, _) = runner.dispatcher().screen_mode;
    let w = scrn_width as u32;
    let h = scrn_height as u32;
    if w == 0 || h == 0 {
        eprintln!(
            "[HEADLESS] Screenshot #{}: skipped (screen not initialized)",
            num
        );
        return;
    }

    let device_gamma = runner.dispatcher().device_gamma();
    let rgba = display::render_screen_with_gamma(
        runner.bus(),
        runner.dispatcher().screen_mode,
        &runner.dispatcher().device_clut,
        &device_gamma,
    );

    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let idx = ((y * w + x) * 4) as usize;
        image::Rgb([rgba[idx], rgba[idx + 1], rgba[idx + 2]])
    });

    let ticks = runner.guest_tick();
    let path = std::env::temp_dir().join(format!("systemless_headless_{:04}.png", num));
    img.save(&path).expect("Failed to save screenshot");
    eprintln!(
        "[HEADLESS] Screenshot #{}: {} (ticks={})",
        num,
        path.display(),
        ticks
    );
}

// Both headless clocks use the same transport and command-safe point. A debug
// run starts paused so the simulated clock cannot outrun client attachment.
fn bind_headless_debug_server(
    path: Option<PathBuf>,
    runner: &mut FixtureRunner,
) -> Option<debug_server::DebugServer> {
    let path = path?;
    let server = debug_server::DebugServer::bind(&path).unwrap_or_else(|error| {
        eprintln!(
            "Error: cannot bind debug socket {}: {error}",
            path.display()
        );
        std::process::exit(1);
    });
    #[cfg(all(feature = "debug-server", unix))]
    systemless::debug::handle_debug_request(runner, systemless::debug::DebugRequest::Pause)
        .expect("initial debugger pause");
    let _ = runner;
    eprintln!("[HEADLESS] Debugger paused at startup; connect and resume to execute");
    Some(server)
}

fn wait_for_debug_resume(server: &mut debug_server::DebugServer, runner: &mut FixtureRunner) {
    loop {
        server.pump(runner);
        if !runner.debug_is_paused() && !runner.is_halted() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn run_headless(
    game_path: &std::path::Path,
    max_instructions: usize,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    script: &[ScriptedInput],
    ui_theme: UiThemeId,
    debug_socket: Option<PathBuf>,
) {
    eprintln!(
        "[HEADLESS] Legacy instruction-budget diagnostic mode: retained Toolbox waits may re-fire repeatedly; do not use these totals as GUI CPU measurements. Use --max-ticks with --tick-input-script for time-based runs."
    );
    eprintln!("[HEADLESS] Starting: {}", game_path.display());
    eprintln!("[HEADLESS] Max instructions: {}", max_instructions);

    let mut runner = match screen_depth {
        Some(screen_depth) => game::new_runner_with_configuration(!addressing_24_bit, screen_depth),
        None => game::new_runner_with_addressing(!addressing_24_bit),
    };
    runner.set_ui_theme(ui_theme);
    let app = game::load_game_from_path(&mut runner, game_path).expect("Failed to load game");
    let mut save_store = DesktopSaveStore::for_loaded_archive(game_path, &mut runner);
    eprintln!(
        "[SYSTEMLESS] Desktop save dir: {}",
        save_store.root().display()
    );
    let restored_saves = save_store.load_saved_files();
    for file in &restored_saves {
        runner.import_vfs_file(file);
    }
    if !restored_saves.is_empty() {
        eprintln!(
            "[SYSTEMLESS] Restored {} desktop save file(s)",
            restored_saves.len()
        );
    }
    game::init_game(&mut runner, &app);

    let mut debug_server = bind_headless_debug_server(debug_socket, &mut runner);

    let chunk = 100_000;
    let mut total: usize = 0;
    let mut last_screenshot = 0usize;
    let mut next_event = 0usize;

    while total < max_instructions {
        if let Some(server) = debug_server.as_mut() {
            wait_for_debug_resume(server, &mut runner);
        }
        // Deliver everything the script has scheduled at or before this
        // point, then run only as far as the next event so its delivery
        // point comes from the script and not from the chunk size.
        while let Some(event) = script.get(next_event) {
            if event.at > total {
                break;
            }
            match event.action {
                InputAction::MouseMove { v, h } => runner.set_mouse_position(v, h),
                InputAction::MouseDown { v, h } => runner.push_mouse_down(v, h),
                InputAction::MouseUp { v, h } => runner.push_mouse_up(v, h),
                InputAction::KeyDown { key, ch } => runner.push_key_down(key, ch),
                InputAction::KeyUp { key, ch } => runner.push_key_up(key, ch),
            }
            eprintln!("[HEADLESS] input @{}: {:?}", event.at, event.action);
            next_event += 1;
        }
        let steps_to_run = steps_until_next_event(
            chunk,
            max_instructions - total,
            total,
            script.get(next_event).map(|event| event.at),
        );
        let (steps, running) = runner.run_steps(steps_to_run, None);
        total += steps;

        let screenshot_num = total / 500_000;
        if screenshot_num > last_screenshot {
            last_screenshot = screenshot_num;
            // Measurement-only switch: timing A/Bs suppress the periodic
            // PNG encodes (a constant ~7s of host work per census run)
            // while keeping the final screenshot and its tick check.
            if std::env::var("SYSTEMLESS_HEADLESS_PERIODIC_SCREENSHOTS")
                .map(|v| v != "0")
                .unwrap_or(true)
            {
                runner.composite_frame();
                save_screenshot(&runner, screenshot_num);
            }
        }

        if !running {
            eprintln!("[HEADLESS] CPU stopped after {} instructions", total);
            break;
        }
    }

    eprintln!("[HEADLESS] Completed {} instructions", total);
    save_store.sync_save_files_now(&mut runner);
    save_screenshot(&runner, 9999);
    // Measurement-only: prints nothing unless SYSTEMLESS_WAIT_STATS is set.
    systemless::runner::dump_wait_stats();
    if debug_server.is_some() && runner.is_halted() {
        eprintln!(
            "[HEADLESS] Debugger remains available after terminal stop (press Ctrl-C to exit)"
        );
        loop {
            if let Some(server) = debug_server.as_mut() {
                server.pump(&mut runner);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if cli.prefer_powerpc {
        // SAFETY: the runner has not started and no worker threads exist yet.
        unsafe { std::env::set_var("SYSTEMLESS_PREFER_POWERPC", "1") };
        eprintln!("[SYSTEMLESS] Native PowerPC slice preferred");
    }
    let game_path = cli.game;
    let native_integrations = !cli.no_native_integrations;
    let arrows_as_numpad = if cli.literal_arrows {
        false
    } else if cli.arrows_as_numpad {
        true
    } else {
        DEFAULT_GUI_ARROWS_AS_NUMPAD
    };

    if !game_path.exists() {
        eprintln!("Error: Game file not found: {}", game_path.display());
        std::process::exit(1);
    }

    #[cfg(target_os = "macos")]
    if !cli.headless
        && native_integrations
        && std::env::var("SYSTEMLESS_DESKTOP_RUNTIME").as_deref() != Ok("thread")
    {
        relaunch_with_native_guest_identity(&game_path);
    }

    eprintln!("[SYSTEMLESS] Starting emulator...");
    eprintln!("[SYSTEMLESS] Game: {}", game_path.display());

    if cli.headless {
        let timed = cli.max_instructions.is_none() && cli.input_script.is_none();
        let script_path = if timed {
            cli.tick_input_script.as_deref()
        } else {
            cli.input_script.as_deref()
        };
        let script = match script_path {
            Some(path) => {
                let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
                    eprintln!("Error: cannot read input script {}: {e}", path.display());
                    std::process::exit(1);
                });
                parse_input_script(&text).unwrap_or_else(|e| {
                    eprintln!("Error in input script {}: {e}", path.display());
                    std::process::exit(1);
                })
            }
            None => Vec::new(),
        };
        if timed {
            headless_time::run(
                &game_path,
                cli.max_ticks.unwrap_or(600),
                cli.headless_start_time.unwrap_or(3_871_497_600),
                cli.addressing_24_bit,
                cli.screen_depth,
                &script,
                cli.ui_theme,
                cli.debug_socket,
            );
        } else {
            run_headless(
                &game_path,
                cli.max_instructions.unwrap_or(5_000_000),
                cli.addressing_24_bit,
                cli.screen_depth,
                &script,
                cli.ui_theme,
                cli.debug_socket,
            );
        }
    } else {
        if cli.input_script.is_some() {
            eprintln!("Error: --input-script requires --headless");
            std::process::exit(1);
        }
        run_gui(
            game_path,
            arrows_as_numpad,
            native_integrations,
            cli.addressing_24_bit,
            cli.screen_depth,
            cli.display_scale,
            cli.ui_theme,
            cli.fullscreen,
            cli.debug_socket,
        );
    }
}

fn logical_arrow_to_mac(key: &Key) -> Option<(u8, u8)> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some((0x7B, 28)),
        Key::Named(NamedKey::ArrowRight) => Some((0x7C, 29)),
        Key::Named(NamedKey::ArrowDown) => Some((0x7D, 31)),
        Key::Named(NamedKey::ArrowUp) => Some((0x7E, 30)),
        _ => None,
    }
}

fn physical_numpad_to_mac(key: &PhysicalKey) -> Option<(u8, u8)> {
    match key {
        PhysicalKey::Code(
            KeyCode::NumpadDecimal
            | KeyCode::NumpadMultiply
            | KeyCode::NumpadAdd
            | KeyCode::NumpadDivide
            | KeyCode::NumpadEnter
            | KeyCode::NumpadSubtract
            | KeyCode::NumpadEqual
            | KeyCode::Numpad0
            | KeyCode::Numpad1
            | KeyCode::Numpad2
            | KeyCode::Numpad3
            | KeyCode::Numpad4
            | KeyCode::Numpad5
            | KeyCode::Numpad6
            | KeyCode::Numpad7
            | KeyCode::Numpad8
            | KeyCode::Numpad9,
        ) => Some((keycode_to_mac(key), keycode_to_mac_char(key))),
        _ => None,
    }
}

fn host_keyboard_event_to_mac(
    logical_key: &Key,
    physical_key: &PhysicalKey,
    text: Option<&str>,
) -> Option<(u8, u8)> {
    // Laptop media actions can share a physical function-key position. The
    // logical volume action belongs to the host and has no classic key event.
    if matches!(
        logical_key,
        Key::Named(NamedKey::AudioVolumeDown | NamedKey::AudioVolumeUp | NamedKey::AudioVolumeMute)
    ) {
        return None;
    }
    Some(host_key_to_mac(logical_key, physical_key, text))
}

fn host_key_to_mac(logical_key: &Key, physical_key: &PhysicalKey, text: Option<&str>) -> (u8, u8) {
    let (mac_key, mac_char_fallback) = physical_numpad_to_mac(physical_key)
        .or_else(|| logical_arrow_to_mac(logical_key))
        .unwrap_or_else(|| {
            (
                keycode_to_mac(physical_key),
                keycode_to_mac_char(physical_key),
            )
        });

    // Control keys (Enter / Tab / Escape / arrows / Space / Backspace)
    // have canonical Mac char codes (CR = 13 for Enter, not LF = 10).
    // winit's `event.text` reports the platform's text-input view (often
    // "\n" for Enter on Linux / wayland), which is wrong for classic Mac.
    // Use `keycode_to_mac_char` first; it returns the correct Mac code for
    // every control key we handle, and 0 for printable keys.
    let char_code = if mac_char_fallback != 0 {
        mac_char_fallback
    } else {
        text.and_then(|t| t.bytes().next())
            .unwrap_or_else(|| keycode_to_mac_printable_char(physical_key))
    };

    (mac_key, char_code)
}

/// Map a winit PhysicalKey to a classic Mac virtual key code.
/// Inside Macintosh Volume V, V-191 (key code assignments)
fn keycode_to_mac(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => 0x00,
            KeyCode::KeyS => 0x01,
            KeyCode::KeyD => 0x02,
            KeyCode::KeyF => 0x03,
            KeyCode::KeyH => 0x04,
            KeyCode::KeyG => 0x05,
            KeyCode::KeyZ => 0x06,
            KeyCode::KeyX => 0x07,
            KeyCode::KeyC => 0x08,
            KeyCode::KeyV => 0x09,
            KeyCode::KeyB => 0x0B,
            KeyCode::KeyQ => 0x0C,
            KeyCode::KeyW => 0x0D,
            KeyCode::KeyE => 0x0E,
            KeyCode::KeyR => 0x0F,
            KeyCode::KeyY => 0x10,
            KeyCode::KeyT => 0x11,
            KeyCode::Digit1 => 0x12,
            KeyCode::Digit2 => 0x13,
            KeyCode::Digit3 => 0x14,
            KeyCode::Digit4 => 0x15,
            KeyCode::Digit6 => 0x16,
            KeyCode::Digit5 => 0x17,
            KeyCode::Equal => 0x18,
            KeyCode::Digit9 => 0x19,
            KeyCode::Digit7 => 0x1A,
            KeyCode::Minus => 0x1B,
            KeyCode::Digit8 => 0x1C,
            KeyCode::Digit0 => 0x1D,
            KeyCode::BracketRight => 0x1E,
            KeyCode::KeyO => 0x1F,
            KeyCode::KeyU => 0x20,
            KeyCode::BracketLeft => 0x21,
            KeyCode::KeyI => 0x22,
            KeyCode::KeyP => 0x23,
            KeyCode::Enter => 0x24,
            KeyCode::KeyL => 0x25,
            KeyCode::KeyJ => 0x26,
            KeyCode::Quote => 0x27,
            KeyCode::KeyK => 0x28,
            KeyCode::Semicolon => 0x29,
            KeyCode::Backslash => 0x2A,
            KeyCode::Comma => 0x2B,
            KeyCode::Slash => 0x2C,
            KeyCode::KeyN => 0x2D,
            KeyCode::KeyM => 0x2E,
            KeyCode::Period => 0x2F,
            KeyCode::Tab => 0x30,
            KeyCode::Space => 0x31,
            KeyCode::Backquote => 0x32,
            KeyCode::Backspace => 0x33,
            KeyCode::Escape => 0x35,
            KeyCode::SuperLeft => 0x37,
            KeyCode::ShiftLeft => 0x38,
            KeyCode::CapsLock => 0x39,
            KeyCode::AltLeft => 0x3A,
            KeyCode::ControlLeft => 0x3B,
            KeyCode::ShiftRight => 0x3C,
            KeyCode::AltRight => 0x3D,
            KeyCode::ControlRight => 0x3E,
            KeyCode::NumpadDecimal => 0x41,
            KeyCode::NumpadMultiply => 0x43,
            KeyCode::NumpadAdd => 0x45,
            KeyCode::NumLock => 0x47,
            KeyCode::NumpadDivide => 0x4B,
            KeyCode::NumpadEnter => 0x4C,
            KeyCode::NumpadSubtract => 0x4E,
            KeyCode::NumpadEqual => 0x51,
            KeyCode::Numpad0 => 0x52,
            KeyCode::Numpad1 => 0x53,
            KeyCode::Numpad2 => 0x54,
            KeyCode::Numpad3 => 0x55,
            KeyCode::Numpad4 => 0x56,
            KeyCode::Numpad5 => 0x57,
            KeyCode::Numpad6 => 0x58,
            KeyCode::Numpad7 => 0x59,
            KeyCode::Numpad8 => 0x5B,
            KeyCode::Numpad9 => 0x5C,
            KeyCode::ArrowLeft => 0x7B,
            KeyCode::ArrowRight => 0x7C,
            KeyCode::ArrowDown => 0x7D,
            KeyCode::ArrowUp => 0x7E,
            KeyCode::F1 => 0x7A,
            KeyCode::F2 => 0x78,
            KeyCode::F3 => 0x63,
            KeyCode::F4 => 0x76,
            KeyCode::F5 => 0x60,
            KeyCode::F6 => 0x61,
            KeyCode::F7 => 0x62,
            KeyCode::F8 => 0x64,
            KeyCode::F9 => 0x65,
            KeyCode::F10 => 0x6D,
            KeyCode::F11 => 0x67,
            KeyCode::F12 => 0x6F,
            KeyCode::F13 => 0x69,
            KeyCode::F14 => 0x6B,
            KeyCode::F15 => 0x71,
            _ => 0xFF,
        },
        _ => 0xFF,
    }
}

/// Fallback char code for non-text keys (arrows, return, etc.).
fn keycode_to_mac_char(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::Enter => 13,
            KeyCode::NumpadEnter => 0x03,
            KeyCode::Tab => 9,
            KeyCode::Space => 32,
            KeyCode::Backspace => 8,
            KeyCode::Escape => 27,
            KeyCode::ArrowLeft => 28,
            KeyCode::ArrowRight => 29,
            KeyCode::ArrowUp => 30,
            KeyCode::ArrowDown => 31,
            // Apple Events.h: kFunctionKeyCharCode = 16. Preserve the
            // virtual key separately, as described by Inside Macintosh:
            // Macintosh Toolbox Essentials, "Handling Low-Level Events".
            KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::F13
            | KeyCode::F14
            | KeyCode::F15 => 0x10,
            _ => 0,
        },
        _ => 0,
    }
}

/// Last-resort printable fallback when the windowing layer reports a physical
/// key event without text. This preserves menu hotkeys and EventRecord readers;
/// when text is available, the platform's layout-aware character still wins.
fn keycode_to_mac_printable_char(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => b'a',
            KeyCode::KeyB => b'b',
            KeyCode::KeyC => b'c',
            KeyCode::KeyD => b'd',
            KeyCode::KeyE => b'e',
            KeyCode::KeyF => b'f',
            KeyCode::KeyG => b'g',
            KeyCode::KeyH => b'h',
            KeyCode::KeyI => b'i',
            KeyCode::KeyJ => b'j',
            KeyCode::KeyK => b'k',
            KeyCode::KeyL => b'l',
            KeyCode::KeyM => b'm',
            KeyCode::KeyN => b'n',
            KeyCode::KeyO => b'o',
            KeyCode::KeyP => b'p',
            KeyCode::KeyQ => b'q',
            KeyCode::KeyR => b'r',
            KeyCode::KeyS => b's',
            KeyCode::KeyT => b't',
            KeyCode::KeyU => b'u',
            KeyCode::KeyV => b'v',
            KeyCode::KeyW => b'w',
            KeyCode::KeyX => b'x',
            KeyCode::KeyY => b'y',
            KeyCode::KeyZ => b'z',
            KeyCode::Digit0 | KeyCode::Numpad0 => b'0',
            KeyCode::Digit1 | KeyCode::Numpad1 => b'1',
            KeyCode::Digit2 | KeyCode::Numpad2 => b'2',
            KeyCode::Digit3 | KeyCode::Numpad3 => b'3',
            KeyCode::Digit4 | KeyCode::Numpad4 => b'4',
            KeyCode::Digit5 | KeyCode::Numpad5 => b'5',
            KeyCode::Digit6 | KeyCode::Numpad6 => b'6',
            KeyCode::Digit7 | KeyCode::Numpad7 => b'7',
            KeyCode::Digit8 | KeyCode::Numpad8 => b'8',
            KeyCode::Digit9 | KeyCode::Numpad9 => b'9',
            KeyCode::Minus | KeyCode::NumpadSubtract => b'-',
            KeyCode::Equal | KeyCode::NumpadEqual => b'=',
            KeyCode::BracketLeft => b'[',
            KeyCode::BracketRight => b']',
            KeyCode::Backslash => b'\\',
            KeyCode::Semicolon => b';',
            KeyCode::Quote => b'\'',
            KeyCode::Comma => b',',
            KeyCode::Period | KeyCode::NumpadDecimal => b'.',
            KeyCode::Slash | KeyCode::NumpadDivide => b'/',
            KeyCode::NumpadMultiply => b'*',
            KeyCode::NumpadAdd => b'+',
            KeyCode::Backquote => b'`',
            _ => 0,
        },
        _ => 0,
    }
}

#[cfg(test)]
mod input_script_tests {
    use super::*;

    #[test]
    fn parses_actions_comments_and_blank_lines() {
        let script = parse_input_script(
            "# get into the game\n\
             \n\
             1000 mousemove 10 20\n\
             2000 click 100 200   # dismiss the splash\n\
             3000 press 0x24 13\n\
             4000 keydown 0x7B 28\n\
             4500 keyup 0x7B 28\n\
             5000 mousedown 1 2\n\
             5500 mouseup 1 2\n",
        )
        .expect("script parses");
        assert_eq!(
            script,
            vec![
                ScriptedInput {
                    at: 1000,
                    action: InputAction::MouseMove { v: 10, h: 20 }
                },
                // click expands to the down/up pair a guest sees
                ScriptedInput {
                    at: 2000,
                    action: InputAction::MouseDown { v: 100, h: 200 }
                },
                ScriptedInput {
                    at: 2000,
                    action: InputAction::MouseUp { v: 100, h: 200 }
                },
                ScriptedInput {
                    at: 3000,
                    action: InputAction::KeyDown { key: 0x24, ch: 13 }
                },
                ScriptedInput {
                    at: 3000,
                    action: InputAction::KeyUp { key: 0x24, ch: 13 }
                },
                ScriptedInput {
                    at: 4000,
                    action: InputAction::KeyDown { key: 0x7B, ch: 28 }
                },
                ScriptedInput {
                    at: 4500,
                    action: InputAction::KeyUp { key: 0x7B, ch: 28 }
                },
                ScriptedInput {
                    at: 5000,
                    action: InputAction::MouseDown { v: 1, h: 2 }
                },
                ScriptedInput {
                    at: 5500,
                    action: InputAction::MouseUp { v: 1, h: 2 }
                },
            ]
        );
    }

    #[test]
    fn out_of_order_lines_are_sorted_by_instruction_count() {
        // The loop consumes the schedule in order, so a script written out
        // of order must not silently drop its early events.
        let script = parse_input_script("900 click 1 1\n100 press 2 2\n").expect("parses");
        assert_eq!(script.first().map(|e| e.at), Some(100));
        assert_eq!(script.last().map(|e| e.at), Some(900));
    }

    #[test]
    fn bad_lines_name_the_line_and_the_problem() {
        for (text, needle) in [
            ("1000 click 1\n", "takes 2 argument"),
            ("1000 wiggle 1 2\n", "unknown action"),
            ("abc click 1 2\n", "bad number"),
            ("1000\n", "expected"),
            ("1000 click 99999 1\n", "out of range"),
        ] {
            let err = parse_input_script(text).expect_err("must reject");
            assert!(
                err.contains(needle) && err.contains("line 1"),
                "error {err:?} should mention line 1 and {needle:?}"
            );
        }
    }

    #[test]
    fn the_run_stops_exactly_at_the_next_event() {
        // Overshooting would make delivery depend on chunk size rather than
        // on the script, which is precisely the determinism being bought.
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 0, Some(1_500)),
            1_500
        );
        // Never zero: a zero-step run would spin without ever advancing.
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 1_500, Some(1_500)),
            1
        );
        // No events left, or none near: the ordinary chunk applies.
        assert_eq!(steps_until_next_event(100_000, 500_000, 0, None), 100_000);
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 0, Some(900_000)),
            100_000
        );
        // The instruction budget still wins over both.
        assert_eq!(steps_until_next_event(100_000, 250, 0, None), 250);
        assert_eq!(steps_until_next_event(100_000, 250, 0, Some(1_000)), 250);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct CountingAudioBackend {
        queued_stereo_bytes: Rc<RefCell<usize>>,
    }

    struct RecordingAudioBackend {
        queued_stereo_bytes: Rc<RefCell<Vec<u8>>>,
    }

    impl systemless::audio::AudioBackend for CountingAudioBackend {
        fn queue_samples(&mut self, samples: &[u8]) {
            *self.queued_stereo_bytes.borrow_mut() += samples.len() * 2;
        }

        fn queue_stereo_samples(&mut self, samples: &[u8]) {
            *self.queued_stereo_bytes.borrow_mut() += samples.len();
        }

        fn stop(&mut self) {}
    }

    impl systemless::audio::AudioBackend for RecordingAudioBackend {
        fn queue_samples(&mut self, samples: &[u8]) {
            self.queued_stereo_bytes.borrow_mut().extend(samples);
        }

        fn queue_stereo_samples(&mut self, samples: &[u8]) {
            self.queued_stereo_bytes.borrow_mut().extend(samples);
        }

        fn stop(&mut self) {}
    }

    #[test]
    fn threaded_window_state_does_not_construct_a_guest_driver() {
        let app = App::new_with_display_scale(
            PathBuf::from("missing-archive"),
            false,
            false,
            false,
            Some(8),
            Some(1),
            UiThemeId::ClassicSystem7,
            false,
            true,
        );
        assert!(app.driver.is_none());
        assert!(
            app.owner.is_none(),
            "owner startup is separate from window state construction"
        );
        assert_eq!(app.frame.sequence, 0);
        assert_eq!(app.guest_state.generation, 0);
    }

    #[test]
    fn cli_parses_typed_runner_options() {
        let cli = Cli::try_parse_from([
            "systemless",
            "--headless",
            "--no-native-integrations",
            "--arrows-as-numpad",
            "--prefer-powerpc",
            "--addressing-24-bit",
            "--screen-depth",
            "4",
            "--display-scale",
            "2",
            "--ui-theme",
            "systemless-default",
            "--max-instructions",
            "1234",
            "game.sit",
        ])
        .expect("runner options should parse");

        assert_eq!(cli.game, PathBuf::from("game.sit"));
        assert!(cli.headless);
        assert!(cli.no_native_integrations);
        assert!(cli.arrows_as_numpad);
        assert!(!cli.literal_arrows);
        assert!(cli.prefer_powerpc);
        assert!(cli.addressing_24_bit);
        assert_eq!(cli.screen_depth, Some(4));
        assert_eq!(cli.display_scale, Some(2));
        assert_eq!(cli.ui_theme, UiThemeId::SystemlessDefault);
        assert_eq!(cli.max_instructions, Some(1234));
    }

    #[test]
    fn cli_defaults_to_automatic_window_size_and_classic_theme() {
        let cli = Cli::try_parse_from(["systemless", "game.sit"])
            .expect("default display scale should parse");
        assert_eq!(cli.display_scale, None);
        assert_eq!(cli.screen_depth, None);
        assert_eq!(cli.ui_theme, UiThemeId::ClassicSystem7);
        assert_eq!(
            guest_scaled_physical_size(800, 600, 1),
            winit::dpi::PhysicalSize::new(800, 600)
        );

        let invalid = Cli::try_parse_from(["systemless", "--display-scale", "0", "game.sit"])
            .expect_err("zero display scale should be rejected");
        assert_eq!(invalid.kind(), ErrorKind::ValueValidation);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn automatic_windows_have_the_same_logical_size_across_dpi() {
        use winit::dpi::PhysicalSize;
        for dpi in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let monitor = PhysicalSize::new((1920.0 * dpi) as u32, (1080.0 * dpi) as u32);
            for (width, height) in [(320, 240), (640, 480), (1600, 1200), (4096, 3072)] {
                assert_eq!(
                    automatic_window_size(width, height, Some(monitor), dpi),
                    PhysicalSize::new((960.0 * dpi) as u32, (720.0 * dpi) as u32)
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn automatic_mac_windows_use_guest_points_at_native_and_retina_dpi() {
        use winit::dpi::PhysicalSize;
        for dpi in [1.0, 2.0] {
            let monitor = PhysicalSize::new((2560.0 * dpi) as u32, (1600.0 * dpi) as u32);
            for (width, height) in [(320, 200), (640, 480), (800, 600), (800, 580)] {
                let expected = PhysicalSize::new(
                    (f64::from(width) * dpi) as u32,
                    (f64::from(height) * dpi) as u32,
                );
                assert_eq!(
                    automatic_window_size(width, height, Some(monitor), dpi),
                    expected
                );
                assert_eq!(automatic_window_size(width, height, None, dpi), expected);
            }
        }
    }

    #[test]
    fn automatic_windows_fit_small_and_portrait_monitors() {
        use winit::dpi::PhysicalSize;
        for monitor in [
            PhysicalSize::new(800, 600),
            PhysicalSize::new(1080, 1920),
            PhysicalSize::new(640, 360),
        ] {
            for (width, height) in [(640, 480), (320, 200), (480, 900), (4096, 2160)] {
                let size = automatic_window_size(width, height, Some(monitor), 2.0);
                assert!(size.width > 0 && size.height > 0);
                assert!(size.width <= monitor.width * 4 / 5);
                assert!(size.height <= monitor.height * 4 / 5);
                assert!(
                    (f64::from(size.width) / f64::from(width)
                        - f64::from(size.height) / f64::from(height))
                    .abs()
                        < 0.01
                );
            }
        }
    }

    #[test]
    fn automatic_windows_have_safe_monitor_and_dpi_fallbacks() {
        use winit::dpi::PhysicalSize;
        let (width, height) = if cfg!(target_os = "macos") {
            (640, 480)
        } else {
            (960, 720)
        };
        for dpi in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                automatic_window_size(640, 480, None, dpi),
                PhysicalSize::new(width, height)
            );
        }
        assert_eq!(
            automatic_window_size(640, 480, Some(PhysicalSize::new(0, 0)), 2.0),
            PhysicalSize::new(width * 2, height * 2)
        );
        let zero = automatic_window_size(0, 0, None, 1.0);
        assert!(zero.width > 0 && zero.height > 0);
    }

    #[test]
    fn guest_layout_changes_preserve_automatic_fullscreen_and_maximized_windows() {
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                assert_eq!(guest_resize_scale(None, fullscreen, maximized), None);
            }
        }
        assert_eq!(guest_resize_scale(Some(2), false, false), Some(2));
        assert_eq!(guest_resize_scale(Some(2), true, false), None);
        assert_eq!(guest_resize_scale(Some(2), false, true), None);
        assert_eq!(
            guest_scaled_physical_size(640, 480, 2),
            winit::dpi::PhysicalSize::new(1280, 960)
        );
    }

    #[test]
    fn cli_accepts_one_bit_screen_depth() {
        let cli = Cli::try_parse_from(["systemless", "--screen-depth", "1", "game.sit"])
            .expect("one-bit display depth should parse");

        assert_eq!(cli.screen_depth, Some(1));
    }

    #[test]
    fn cli_accepts_two_bit_screen_depth() {
        let cli = Cli::try_parse_from(["systemless", "--screen-depth", "2", "game.sit"])
            .expect("two-bit display depth should parse");

        assert_eq!(cli.screen_depth, Some(2));
    }

    #[test]
    fn cli_preserves_literal_arrows_compatibility_alias() {
        let cli = Cli::try_parse_from(["systemless", "--no-arrows-as-numpad", "game.sit"])
            .expect("compatibility alias should parse");

        assert!(cli.literal_arrows);
    }

    #[test]
    fn cli_accepts_prefer_ppc_alias() {
        let cli = Cli::try_parse_from(["systemless", "--prefer-ppc", "game.sit"])
            .expect("PowerPC preference alias should parse");

        assert!(cli.prefer_powerpc);
    }

    #[test]
    fn cli_generates_help_and_version() {
        let help =
            Cli::try_parse_from(["systemless", "--help"]).expect_err("--help should stop parsing");
        let version = Cli::try_parse_from(["systemless", "--version"])
            .expect_err("--version should stop parsing");

        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        assert!(help.to_string().contains("--prefer-ppc"));
        assert_eq!(version.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn cli_rejects_missing_game_removed_options_and_unknown_options() {
        let missing_game =
            Cli::try_parse_from(["systemless"]).expect_err("game path should be required");
        let removed_cpu = Cli::try_parse_from(["systemless", "--cpu-mhz", "25", "game.sit"])
            .expect_err("removed CPU option should be rejected");
        let removed_menu = Cli::try_parse_from(["systemless", "--show-menu-bar", "game.sit"])
            .expect_err("removed menu option should be rejected");
        let unknown_option = Cli::try_parse_from(["systemless", "--wat", "game.sit"])
            .expect_err("unknown options should be rejected");

        assert_eq!(missing_game.kind(), ErrorKind::MissingRequiredArgument);
        assert_eq!(removed_cpu.kind(), ErrorKind::UnknownArgument);
        assert_eq!(removed_menu.kind(), ErrorKind::UnknownArgument);
        assert_eq!(unknown_option.kind(), ErrorKind::UnknownArgument);
    }

    fn gui_runner_with_counting_audio() -> (FixtureRunner, Rc<RefCell<usize>>) {
        let queued = Rc::new(RefCell::new(0usize));
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        runner.set_audio(Box::new(CountingAudioBackend {
            queued_stereo_bytes: queued.clone(),
        }));
        (runner, queued)
    }

    fn gui_runner_with_recording_audio() -> (FixtureRunner, Rc<RefCell<Vec<u8>>>) {
        let queued = Rc::new(RefCell::new(Vec::new()));
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        runner.set_audio(Box::new(RecordingAudioBackend {
            queued_stereo_bytes: queued.clone(),
        }));
        (runner, queued)
    }

    #[test]
    fn desktop_detects_clean_guest_exit() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let base = 0x0001_0000u32;
        runner.bus_mut().write_word(base, 0xA9F4); // _ExitToShell
        runner.cpu_mut().write_reg(Register::PC, base);
        runner.cpu_mut().write_reg(Register::A7, 0x0010_0000);
        driver.runner = Some(runner);

        assert!(!driver.guest_requested_exit());
        let (_steps, running) = driver.runner.as_mut().unwrap().run_steps(1, None);

        assert!(!running);
        assert!(driver.guest_requested_exit());
    }

    #[test]
    fn wall_clock_origin_starts_pacer_at_seeded_guest_tick() {
        // The runner boots with a non-zero TickCount (~600). Anchoring the
        // wall-clock origin at that seeded tick means the pacer is level with
        // the guest immediately instead of waiting ~10 real seconds for the
        // wall clock to reach tick 600 before running any CPU.
        let now = std::time::Instant::now();
        let seeded_tick = 600;
        let origin = GuiDriver::wall_clock_origin_for_guest_tick(now, seeded_tick);

        assert_eq!(
            GuiDriver::tick_due_at(origin, now),
            seeded_tick,
            "a non-zero launch TickCount must not make the pacer wait real time before running CPU"
        );
    }

    #[test]
    fn seeded_guest_tick_can_advance_on_first_frame() {
        // One host frame after boot, the wall-clock target must be at least one
        // tick ahead of the seeded guest tick so the CPU loop has runnable work.
        let start = std::time::Instant::now();
        let seeded_tick = 600;
        let origin = GuiDriver::wall_clock_origin_for_guest_tick(start, seeded_tick);
        let one_frame_later = start + FRAME_DURATION;

        assert!(
            GuiDriver::tick_due_at(origin, one_frame_later) > seeded_tick,
            "the first post-boot frame should have runnable guest work"
        );
    }

    #[test]
    fn gui_defaults_to_literal_arrow_controls() {
        let app = App::new(
            PathBuf::from("dummy"),
            DEFAULT_GUI_ARROWS_AS_NUMPAD,
            true,
            false,
            8,
        );

        assert!(
            !app.arrows_as_numpad,
            "the interactive GUI should leave arrow keys literal by default; --arrows-as-numpad opts into keypad movement"
        );
    }

    #[test]
    fn physical_numpad_events_keep_keypad_identity_even_when_logical_key_is_arrow() {
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowLeft),
                &PhysicalKey::Code(KeyCode::Numpad4),
                None,
            ),
            (0x56, b'4')
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowRight),
                &PhysicalKey::Code(KeyCode::Numpad6),
                None,
            ),
            (0x58, b'6')
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowDown),
                &PhysicalKey::Code(KeyCode::Numpad2),
                None,
            ),
            (0x54, b'2')
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowUp),
                &PhysicalKey::Code(KeyCode::Numpad8),
                None,
            ),
            (0x5B, b'8')
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::Enter),
                &PhysicalKey::Code(KeyCode::NumpadEnter),
                None,
            ),
            (0x4C, 0x03)
        );
    }

    #[test]
    fn host_volume_actions_do_not_generate_guest_text_or_function_keys() {
        for key in [
            NamedKey::AudioVolumeDown,
            NamedKey::AudioVolumeUp,
            NamedKey::AudioVolumeMute,
        ] {
            for physical in [
                PhysicalKey::Code(KeyCode::F2),
                PhysicalKey::Code(KeyCode::AudioVolumeDown),
            ] {
                assert_eq!(
                    host_keyboard_event_to_mac(&Key::Named(key.clone()), &physical, None),
                    None
                );
            }
        }
        assert_eq!(
            host_keyboard_event_to_mac(
                &Key::Named(NamedKey::F2),
                &PhysicalKey::Code(KeyCode::F2),
                None
            ),
            Some((0x78, 0x10)),
        );
    }

    #[test]
    fn function_key_events_preserve_mac_virtual_keys_and_function_character() {
        for (key, mac_key) in [
            (KeyCode::F1, 0x7A),
            (KeyCode::F2, 0x78),
            (KeyCode::F3, 0x63),
            (KeyCode::F4, 0x76),
            (KeyCode::F5, 0x60),
            (KeyCode::F6, 0x61),
            (KeyCode::F7, 0x62),
            (KeyCode::F8, 0x64),
            (KeyCode::F9, 0x65),
            (KeyCode::F10, 0x6D),
            (KeyCode::F11, 0x67),
            (KeyCode::F12, 0x6F),
            (KeyCode::F13, 0x69),
            (KeyCode::F14, 0x6B),
            (KeyCode::F15, 0x71),
        ] {
            for text in [None, Some("\0")] {
                assert_eq!(
                    host_key_to_mac(&Key::Named(NamedKey::F2), &PhysicalKey::Code(key), text),
                    (mac_key, 0x10)
                );
            }
        }
    }

    #[test]
    fn physical_arrow_events_keep_literal_arrow_identity() {
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowLeft),
                &PhysicalKey::Code(KeyCode::ArrowLeft),
                None,
            ),
            (0x7B, 28)
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowRight),
                &PhysicalKey::Code(KeyCode::ArrowRight),
                None,
            ),
            (0x7C, 29)
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowDown),
                &PhysicalKey::Code(KeyCode::ArrowDown),
                None,
            ),
            (0x7D, 31)
        );
        assert_eq!(
            host_key_to_mac(
                &Key::Named(NamedKey::ArrowUp),
                &PhysicalKey::Code(KeyCode::ArrowUp),
                None,
            ),
            (0x7E, 30)
        );
    }

    #[test]
    fn printable_physical_keys_have_char_fallbacks() {
        assert_eq!(
            keycode_to_mac_printable_char(&PhysicalKey::Code(KeyCode::KeyJ)),
            b'j'
        );
        assert_eq!(
            keycode_to_mac_printable_char(&PhysicalKey::Code(KeyCode::KeyM)),
            b'm'
        );
        assert_eq!(
            keycode_to_mac_printable_char(&PhysicalKey::Code(KeyCode::Numpad8)),
            b'8'
        );
        assert_eq!(
            keycode_to_mac_printable_char(&PhysicalKey::Code(KeyCode::ArrowUp)),
            0,
            "control keys use canonical Mac control-character fallback instead"
        );
    }

    #[test]
    fn service_pending_sound_work_uses_reserved_slice_after_spent_frame_budget() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;
        use systemless::runner::FixtureRunnerConfig;
        use systemless::sound::{PendingSoundCallback, SndCommand};

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let resume_pc = runner.bus_mut().alloc(16);
        runner.bus_mut().write_word(resume_pc, 0x4E71); // NOP
        let callback_addr = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(callback_addr, 0x4E75); // RTS
        runner.cpu_mut().write_reg(Register::PC, resume_pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);

        runner
            .dispatcher_mut()
            .queue_sound_callback(PendingSoundCallback::Command {
                architecture: systemless::callback_manager::CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0001_2340,
                cmd: SndCommand {
                    cmd: systemless::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let spent_deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
        let mut reserved_sound_steps = 0usize;
        let steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_SLICE_INSTRUCTIONS,
            &mut reserved_sound_steps,
        );

        assert!(
            steps.is_some_and(|steps| steps > 0),
            "sound callbacks should run from their reserved interrupt slice even after the foreground frame budget/deadline is spent"
        );
        assert!(
            !runner.has_pending_sound_work(),
            "sound callback should complete from the reserved interrupt slice"
        );
        assert!(!runner.is_halted());
    }

    #[test]
    fn service_pending_sound_work_caps_reserved_slice_per_frame() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;
        use systemless::runner::FixtureRunnerConfig;
        use systemless::sound::{PendingSoundCallback, SndCommand};

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let resume_pc = runner.bus_mut().alloc(16);
        runner.bus_mut().write_word(resume_pc, 0x4E71); // NOP
        let callback_addr = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(callback_addr, 0x60FE); // BRA.S *: spinning callback
        runner.cpu_mut().write_reg(Register::PC, resume_pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);

        runner
            .dispatcher_mut()
            .queue_sound_callback(PendingSoundCallback::Command {
                architecture: systemless::callback_manager::CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0001_2340,
                cmd: SndCommand {
                    cmd: systemless::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let spent_deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
        let mut reserved_sound_steps = 0usize;
        let first_steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_SLICE_INSTRUCTIONS,
            &mut reserved_sound_steps,
        )
        .expect("first reserved sound slice should run");

        assert_eq!(first_steps, SOUND_CALLBACK_SLICE_INSTRUCTIONS);
        assert_eq!(reserved_sound_steps, SOUND_CALLBACK_SLICE_INSTRUCTIONS);
        assert!(
            runner.has_pending_sound_work(),
            "spinning callback should remain pending after one reserved sound slice"
        );

        let second_steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_SLICE_INSTRUCTIONS + first_steps,
            &mut reserved_sound_steps,
        );

        assert_eq!(second_steps, Some(SOUND_CALLBACK_SLICE_INSTRUCTIONS));
        assert!(runner.has_pending_sound_work());
        assert!(!runner.is_halted());

        let final_partial_steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_SLICE_INSTRUCTIONS * 2 + first_steps,
            &mut reserved_sound_steps,
        );

        assert_eq!(
            final_partial_steps,
            Some(
                SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME
                    - SOUND_CALLBACK_SLICE_INSTRUCTIONS * 2
            )
        );
        assert!(runner.has_pending_sound_work());
        assert!(!runner.is_halted());

        let exhausted_steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME + first_steps,
            &mut reserved_sound_steps,
        );

        assert_eq!(
            exhausted_steps, None,
            "same-frame reserved sound work should stop at the cap so the GUI event loop can process input"
        );
        assert_eq!(
            reserved_sound_steps,
            SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME
        );
    }

    #[test]
    fn audio_samples_for_duration_preserves_fractional_rate() {
        let mut remainder = 0.0;
        let mut total = 0usize;

        for _ in 0..120 {
            let samples = GuiDriver::audio_samples_for_duration(FRAME_DURATION, &mut remainder);
            assert!(samples > 0);
            total += samples;
        }

        let expected =
            (FRAME_DURATION.as_secs_f64() * systemless::sound::OUTPUT_RATE as f64 * 120.0).floor()
                as usize;
        assert_eq!(total, expected);
        assert!(remainder >= 0.0);
        assert!(remainder < 1.0);
    }

    #[test]
    fn step_frame_mixes_one_audio_frame_when_guest_tick_does_not_advance() {
        let now = std::time::Instant::now();
        let (runner, queued) = gui_runner_with_counting_audio();
        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now);
        driver.next_frame_time = Some(now);

        driver.step_frame();

        assert!(
            (732..=734).contains(&*queued.borrow()),
            "same-tick GUI/menu frames should still queue one host audio frame, got {} bytes",
            *queued.borrow()
        );
    }

    #[test]
    fn step_frame_forces_render_after_same_tick_foreground_progress() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;

        let now = std::time::Instant::now();
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(pc, 0x60FE); // BRA.S *
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        runner.bus_mut().write_long(0x016A, 0);
        runner.set_instructions_per_tick((game::MAX_INSTRUCTIONS_PER_FRAME * 2) as u32);

        // Install a guest menu so repeated batch-level compositing is observable.
        // NewMenu: Inside Macintosh I-352; InsertMenu: Toolbox Essentials 3-108.
        let title = runner.bus_mut().alloc(5);
        runner.bus_mut().write_bytes(title, b"\x04File");
        runner.bus_mut().write_word(pc, 0xA931);
        runner.bus_mut().write_long(0x0008_0000, title);
        runner.bus_mut().write_word(0x0008_0004, 1);
        assert!(runner.run_gui_cpu_slice(1, u32::MAX).1);
        let menu = runner.bus().read_long(0x0008_0006);
        assert_ne!(menu, 0);
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        runner.bus_mut().write_word(pc, 0xA935);
        runner.bus_mut().write_word(0x0008_0000, 0);
        runner.bus_mut().write_long(0x0008_0002, menu);
        assert!(runner.run_gui_cpu_slice(1, u32::MAX).1);
        runner.bus_mut().write_word(pc, 0x60FE);
        runner.cpu_mut().write_reg(Register::PC, pc);
        let screen = runner.bus_mut().alloc(800 * 600);
        runner.dispatcher_mut().screen_mode = (screen, 800, 800, 600, 8);
        runner.bus_mut().write_word(0x0BAA, 20);
        runner.bus_mut().fill_bytes(screen, 800 * 20, 0xAA);

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now - FRAME_DURATION);
        driver.next_frame_time = Some(now + FRAME_DURATION * 4);
        driver.last_presented_guest_tick = Some(0);
        driver.force_next_render = false;

        // Keep the frame budget independent of emulator setup time and host
        // scheduling. The instruction cap still bounds foreground execution.
        driver.step_frame_with_clock(|| now);

        let runner = driver.runner.as_ref().unwrap();
        assert!(
            driver.total_instructions > 0,
            "test setup should execute foreground startup work"
        );
        assert_eq!(
            runner.bus().read_byte(screen + 400),
            0xAA,
            "CPU and audio batches must defer chrome painting until presentation"
        );
        assert!(!runner.is_halted(), "foreground loop must remain runnable");
        assert_eq!(
            runner.guest_tick(),
            0,
            "test setup should stay within the same VBL tick"
        );
        assert!(
            driver.should_render_frame(app.debug_overlay_visible),
            "same-tick foreground drawing progress should force a present"
        );
        driver.runner.as_mut().unwrap().composite_frame();
        assert_ne!(
            driver
                .runner
                .as_ref()
                .unwrap()
                .bus()
                .read_byte(screen + 400),
            0xAA,
            "the presentation pass must still paint the menu"
        );
    }

    #[test]
    fn step_frame_drives_click_transitions_when_guest_tick_is_ahead() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;

        let now = std::time::Instant::now();
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(pc, 0x60FE); // BRA.S *
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        runner.bus_mut().write_long(0x016A, 0);
        runner.set_instructions_per_tick(1);
        let (steps, running) = runner.run_steps(10, None);
        assert_eq!(steps, 10);
        assert!(running);
        let input_tick = runner.guest_tick();
        assert!(input_tick >= 10);
        runner.set_instructions_per_tick((game::MAX_INSTRUCTIONS_PER_FRAME * 2) as u32);
        runner.push_mouse_down(350, 580);

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now + FRAME_DURATION * 240);
        driver.next_frame_time = Some(now + FRAME_DURATION * 120);
        driver.mouse_release_latch.press();
        assert_eq!(driver.mouse_release_latch.release((350, 580)), None);
        assert!(driver.mouse_release_latch.requires_guest_progress());

        driver.step_frame();

        let runner = driver.runner.as_ref().unwrap();
        assert!(driver.total_instructions > 0);
        assert_eq!(runner.guest_tick(), input_tick);
        assert_eq!(runner.bus().read_byte(0x0172), 0x00);

        driver.flush_ready_mouse_release();

        let runner = driver.runner.as_ref().unwrap();
        assert_eq!(runner.bus().read_byte(0x0172), 0x80);
        assert!(driver.mouse_release_latch.requires_guest_progress());

        let instructions_after_press = driver.total_instructions;
        driver.step_frame();

        assert!(driver.total_instructions > instructions_after_press);
        assert!(!driver.mouse_release_latch.requires_guest_progress());
    }

    #[test]
    fn step_frame_services_pending_sound_before_late_same_tick_audio_mix() {
        check_pending_sound_before_audio_mix(false);
    }

    #[test]
    fn headless_frame_services_pending_sound_before_audio_mix() {
        check_pending_sound_before_audio_mix(true);
    }

    fn check_pending_sound_before_audio_mix(headless: bool) {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;
        use systemless::sound::{
            DoubleBufferState, PendingDoubleBackCallback, SndChannel, OUTPUT_RATE,
        };

        const FRAMES: usize = 512;

        let now = std::time::Instant::now();
        let scheduled_frame_end = now;
        let (mut runner, queued) = gui_runner_with_recording_audio();
        let interrupted_pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(interrupted_pc, 0x60FE); // foreground BRA.S *
        runner.cpu_mut().write_reg(Register::PC, interrupted_pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);

        let chan_ptr = 0x0001_2340;
        let header_ptr = runner.bus_mut().alloc(24);
        let buf0_ptr = runner.bus_mut().alloc(16 + FRAMES as u32);
        let callback_addr = runner.bus_mut().alloc((FRAMES / 4) as u32 * 10 + 12);

        runner.bus_mut().write_word(header_ptr, 1);
        runner.bus_mut().write_word(header_ptr + 2, 8);
        runner
            .bus_mut()
            .write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus_mut().write_long(header_ptr + 12, buf0_ptr);
        runner.bus_mut().write_long(header_ptr + 16, 0);
        runner.bus_mut().write_long(header_ptr + 20, callback_addr);
        runner.bus_mut().write_long(buf0_ptr, FRAMES as u32);
        runner.bus_mut().write_long(buf0_ptr + 4, 0);

        let mut pc = callback_addr;
        for offset in (0..FRAMES).step_by(4) {
            runner.bus_mut().write_word(pc, 0x23FC); // MOVE.L #imm,abs.L
            runner.bus_mut().write_long(pc + 2, 0xA0A0_A0A0);
            runner
                .bus_mut()
                .write_long(pc + 6, buf0_ptr + 16 + offset as u32);
            pc += 10;
        }
        runner.bus_mut().write_word(pc, 0x23FC); // MOVE.L #dbBufferReady,flags
        runner.bus_mut().write_long(pc + 2, 0x0000_0001);
        runner.bus_mut().write_long(pc + 6, buf0_ptr + 4);
        runner.bus_mut().write_word(pc + 10, 0x4E75); // RTS

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
        runner.dispatcher_mut().add_sound_channel(chan);
        runner
            .dispatcher_mut()
            .queue_sound_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(scheduled_frame_end);
        driver.next_frame_time = Some(scheduled_frame_end);

        if headless {
            headless_time::frame(driver.runner.as_mut().unwrap(), 366);
        } else {
            driver.step_frame();
        }

        let queued = queued.borrow();
        assert!(
            (732..=734).contains(&queued.len()),
            "same-tick GUI frame should queue one host audio frame, got {} bytes",
            queued.len()
        );
        assert!(
            queued.iter().any(|&sample| sample == 0xA0),
            "pending doubleback must refill before same-tick audio is mixed"
        );
        assert!(
            !driver.runner.as_ref().unwrap().has_pending_sound_work(),
            "sound callback should complete during the GUI sound-work slice"
        );
    }

    #[test]
    fn step_frame_services_doubleback_between_late_audio_chunks() {
        check_doubleback_between_audio_chunks(false);
    }

    #[test]
    fn headless_frame_services_doubleback_between_audio_chunks() {
        check_doubleback_between_audio_chunks(true);
    }

    fn check_doubleback_between_audio_chunks(headless: bool) {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;
        use systemless::sound::{DoubleBufferState, SndChannel, OUTPUT_RATE};

        const REFILL_FRAMES: usize = 64;

        let now = std::time::Instant::now();
        let (mut runner, queued) = gui_runner_with_recording_audio();
        let interrupted_pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(interrupted_pc, 0x60FE); // foreground BRA.S *
        runner.cpu_mut().write_reg(Register::PC, interrupted_pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);

        let chan_ptr = 0x0001_2340;
        let header_ptr = runner.bus_mut().alloc(24);
        let buf0_ptr = runner.bus_mut().alloc(16 + REFILL_FRAMES as u32);
        let buf1_ptr = runner.bus_mut().alloc(16 + REFILL_FRAMES as u32);
        let callback_addr = runner
            .bus_mut()
            .alloc((REFILL_FRAMES as u32 / 4 + 2) * 20 + 2);

        runner.bus_mut().write_word(header_ptr, 1);
        runner.bus_mut().write_word(header_ptr + 2, 8);
        runner
            .bus_mut()
            .write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus_mut().write_long(header_ptr + 12, buf0_ptr);
        runner.bus_mut().write_long(header_ptr + 16, buf1_ptr);
        runner.bus_mut().write_long(header_ptr + 20, callback_addr);
        runner.bus_mut().write_long(buf0_ptr, 1);
        runner.bus_mut().write_long(buf0_ptr + 4, 0x0000_0001);
        runner.bus_mut().write_byte(buf0_ptr + 16, 0x90);
        runner.bus_mut().write_long(buf1_ptr, REFILL_FRAMES as u32);
        runner.bus_mut().write_long(buf1_ptr + 4, 0);

        let mut pc = callback_addr;
        for buf_ptr in [buf0_ptr, buf1_ptr] {
            runner.bus_mut().write_word(pc, 0x23FC); // MOVE.L #frames,abs.L
            runner.bus_mut().write_long(pc + 2, REFILL_FRAMES as u32);
            runner.bus_mut().write_long(pc + 6, buf_ptr);
            pc += 10;
            for offset in (0..REFILL_FRAMES).step_by(4) {
                runner.bus_mut().write_word(pc, 0x23FC); // MOVE.L #imm,abs.L
                runner.bus_mut().write_long(pc + 2, 0xB0B0_B0B0);
                runner
                    .bus_mut()
                    .write_long(pc + 6, buf_ptr + 16 + offset as u32);
                pc += 10;
            }
            runner.bus_mut().write_word(pc, 0x23FC); // MOVE.L #dbBufferReady,flags
            runner.bus_mut().write_long(pc + 2, 0x0000_0001);
            runner.bus_mut().write_long(pc + 6, buf_ptr + 4);
            pc += 10;
        }
        runner.bus_mut().write_word(pc, 0x4E75); // RTS

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
        systemless::trap::TrapDispatcher::load_double_buffer_samples(
            runner.bus_mut(),
            &mut chan,
            buf0_ptr,
            OUTPUT_RATE << 16,
            1,
            8,
        );
        runner.dispatcher_mut().add_sound_channel(chan);

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now);
        driver.next_frame_time = Some(now);

        if headless {
            headless_time::frame(driver.runner.as_mut().unwrap(), 366);
        } else {
            driver.step_frame();
        }

        let queued = queued.borrow();
        assert!(
            (732..=734).contains(&queued.len()),
            "same-tick GUI frame should still queue one host audio frame, got {} bytes",
            queued.len()
        );
        let first_refill_frame = queued
            .chunks_exact(2)
            .position(|frame| frame[0] == 0xB0 && frame[1] == 0xB0)
            .expect("refilled double-buffer samples should be heard in the same GUI frame");
        assert!(
            first_refill_frame <= AUDIO_CALLBACK_CHUNK_SAMPLES + 1,
            "doubleback refill should be serviced between late-audio chunks, not after a long silence tail; first refill frame={}",
            first_refill_frame
        );
    }

    #[test]
    fn step_frame_recovers_audio_elapsed_during_a_dropped_video_frame() {
        let now = std::time::Instant::now();
        let (runner, queued) = gui_runner_with_counting_audio();
        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now);
        driver.next_frame_time = Some(now);
        driver.last_audio_mix_time =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(100));

        driver.step_frame();

        assert!(
            (4_400..=4_600).contains(&*queued.borrow()),
            "100 ms of elapsed host time should queue about 2,205 stereo frames, got {} bytes",
            *queued.borrow()
        );
    }

    #[test]
    fn step_frame_mixes_audio_for_actual_guest_tick_advance() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;

        let now = std::time::Instant::now();
        let (mut runner, queued) = gui_runner_with_counting_audio();
        let pc = runner.bus_mut().alloc(4);
        runner.bus_mut().write_word(pc, 0x4E71); // NOP
        runner.bus_mut().write_word(pc + 2, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        runner.set_instructions_per_tick(1);

        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(runner);
        driver.start_time = Some(now - FRAME_DURATION * 2);
        driver.next_frame_time = Some(now + FRAME_DURATION);

        driver.step_frame();

        assert!(
            (732..=734).contains(&*queued.borrow()),
            "one GUI frame should queue about 367 stereo frames, got {} bytes",
            *queued.borrow()
        );
    }

    #[test]
    fn render_headroom_tracks_render_cost_with_bounds() {
        assert_eq!(
            GuiDriver::next_render_headroom(std::time::Duration::from_micros(200)),
            MIN_RENDER_HEADROOM
        );
        assert_eq!(
            GuiDriver::next_render_headroom(std::time::Duration::from_micros(3_000)),
            std::time::Duration::from_micros(3_500)
        );
        assert_eq!(
            GuiDriver::next_render_headroom(std::time::Duration::from_micros(20_000)),
            MAX_RENDER_HEADROOM
        );
    }

    #[test]
    fn frame_scheduler_preserves_cadence_when_on_time_or_slightly_late() {
        let scheduled = std::time::Instant::now();
        let half_frame = std::time::Duration::from_secs_f64(FRAME_DURATION.as_secs_f64() / 2.0);

        let (on_time_target, on_time_dropped) = GuiDriver::next_frame_target(scheduled, scheduled);
        assert_eq!(on_time_target, scheduled + FRAME_DURATION);
        assert!(!on_time_dropped);

        let (late_target, late_dropped) =
            GuiDriver::next_frame_target(scheduled + half_frame, scheduled);
        assert_eq!(late_target, scheduled + FRAME_DURATION);
        assert!(!late_dropped);
    }

    #[test]
    fn frame_scheduler_drops_missed_host_frame_instead_of_catchup_burst() {
        let scheduled = std::time::Instant::now();

        let full_frame_late = scheduled + FRAME_DURATION;
        let (full_frame_target, full_frame_dropped) =
            GuiDriver::next_frame_target(full_frame_late, scheduled);
        assert_eq!(full_frame_target, full_frame_late + FRAME_DURATION);
        assert!(full_frame_dropped);

        let several_frames_late = scheduled + FRAME_DURATION * 4;
        let (late_target, late_dropped) =
            GuiDriver::next_frame_target(several_frames_late, scheduled);
        assert_eq!(late_target, several_frames_late + FRAME_DURATION);
        assert!(late_dropped);
    }

    #[test]
    fn gui_foreground_batches_match_guest_architecture_costs() {
        let m68k_instructions_per_tick =
            systemless::runner::default_realtime_instructions_per_tick(false) as usize;
        let ppc_instructions_per_tick =
            systemless::runner::default_realtime_instructions_per_tick(true) as usize;

        assert!(
            CPU_BATCH_INSTRUCTIONS <= m68k_instructions_per_tick / 32,
            "GUI batches should yield frequently during heavy drawing and slow HLE startup paths; batch={} vbl_budget={}",
            CPU_BATCH_INSTRUCTIONS,
            m68k_instructions_per_tick
        );
        assert_eq!(
            foreground_cpu_batch_instructions(false, m68k_instructions_per_tick as u32),
            CPU_BATCH_INSTRUCTIONS
        );
        assert_eq!(
            foreground_cpu_batch_instructions(true, ppc_instructions_per_tick as u32),
            ppc_instructions_per_tick.div_ceil(PPC_GUI_BATCH_TICK_DIVISOR),
            "PPC batches should subdivide the guest tick so draw-heavy HLE imports \
             honor the wall-clock CPU deadline between sub-tick batches"
        );
        assert_eq!(
            SOUND_CALLBACK_SLICE_INSTRUCTIONS, CPU_BATCH_INSTRUCTIONS,
            "Sound Manager callback slices should stay aligned with GUI yield cadence"
        );
    }

    #[test]
    fn render_gate_waits_for_guest_tick_unless_forced() {
        let mut app = App::new(PathBuf::from("dummy"), false, true, false, 8);
        let mut driver = app.driver.take().unwrap();
        driver.runner = Some(FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        ));

        assert!(
            driver.should_render_frame(app.debug_overlay_visible),
            "initial forced render should present the first frame"
        );

        let tick = driver.runner.as_ref().unwrap().guest_tick();
        driver.last_presented_guest_tick = Some(tick);
        driver.force_next_render = false;
        assert!(
            !driver.should_render_frame(app.debug_overlay_visible),
            "same guest tick should not present another partial frame"
        );

        driver.force_next_render = true;
        assert!(
            driver.should_render_frame(app.debug_overlay_visible),
            "host input can force a present"
        );
        driver.force_next_render = false;

        driver.runner.as_mut().unwrap().force_advance_guest_tick();
        assert!(
            driver.should_render_frame(app.debug_overlay_visible),
            "a new guest tick is a fresh VBL presentation point"
        );
    }

    #[test]
    fn copybits_detection_accepts_bordered_off_center_blits() {
        let centered = ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 480,
            src_right: 640,
            dst_top: 60,
            dst_left: 80,
            dst_bottom: 540,
            dst_right: 720,
        };
        assert_eq!(
            content_rect_from_copybits(centered, 800, 600),
            Some(ContentRect {
                left: 80,
                top: 60,
                width: 640,
                height: 480,
            })
        );

        let fullscreen = ScreenCopyBitsRect {
            src_bottom: 600,
            src_right: 800,
            dst_top: 0,
            dst_left: 0,
            dst_bottom: 600,
            dst_right: 800,
            ..centered
        };
        assert_eq!(content_rect_from_copybits(fullscreen, 800, 600), None);

        let off_center = ScreenCopyBitsRect {
            dst_left: 10,
            dst_right: 650,
            ..centered
        };
        assert_eq!(
            content_rect_from_copybits(off_center, 800, 600),
            Some(ContentRect {
                left: 10,
                top: 60,
                width: 640,
                height: 480,
            })
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cached_viewport_must_fit_guest_screen_geometry() {
        let valid = CachedContentRect {
            version: 2,
            screen_width: 800,
            screen_height: 600,
            pixel_size: 8,
            content: ContentRect {
                left: 80,
                top: 104,
                width: 640,
                height: 392,
            },
        };
        assert!(valid_cached_content_rect(&valid));

        let off_center = CachedContentRect {
            content: ContentRect {
                left: 79,
                ..valid.content
            },
            ..valid
        };
        assert!(valid_cached_content_rect(&off_center));

        let out_of_bounds = CachedContentRect {
            content: ContentRect {
                left: 700,
                ..valid.content
            },
            ..valid
        };
        assert!(!valid_cached_content_rect(&out_of_bounds));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn no_native_presentation_ignores_cached_viewports_and_skips_learning() {
        let cache = CachedContentRect {
            version: 2,
            screen_width: 800,
            screen_height: 600,
            pixel_size: 8,
            content: ContentRect {
                left: 80,
                top: 100,
                width: 640,
                height: 400,
            },
        };

        assert_eq!(
            cached_content_rect_for_presentation(Some(cache), false),
            None
        );
        assert_eq!(
            cached_content_rect_for_presentation(Some(cache), true),
            Some(cache)
        );
        assert!(!should_learn_content_rect(false, false));
        assert!(!should_learn_content_rect(true, true));
        assert!(should_learn_content_rect(false, true));
    }

    #[test]
    fn centered_pixel_fallback_finds_content_without_game_identity() {
        let width = 800usize;
        let height = 600usize;
        let mut framebuffer = vec![0u8; width * height];
        for row in 104..496 {
            framebuffer[row * width + 96..row * width + 709].fill(7);
        }
        assert_eq!(
            detect_centered_content_rect_8bpp(&framebuffer, width, width, height),
            Some(ContentRect {
                left: 96,
                top: 104,
                width: 613,
                height: 392,
            })
        );

        framebuffer.fill(7);
        assert_eq!(
            detect_centered_content_rect_8bpp(&framebuffer, width, width, height),
            None,
            "a full-screen image must not be cropped"
        );
    }

    #[test]
    fn active_margin_validation_rejects_partial_full_screen_layouts() {
        let width = 800usize;
        let height = 600usize;
        let content = ContentRect {
            left: 68,
            top: 0,
            width: 664,
            height: 600,
        };
        let mut framebuffer = vec![0u8; width * height];
        for row in 0..height {
            framebuffer[row * width + 68..row * width + 732].fill(7);
        }
        assert!(content_rect_has_inactive_margins_8bpp(
            &framebuffer,
            width,
            width,
            height,
            content,
        ));

        for row in 0..height {
            framebuffer[row * width + 732..(row + 1) * width].fill(2);
        }
        assert!(
            !content_rect_has_inactive_margins_8bpp(&framebuffer, width, width, height, content,),
            "a separately drawn side panel must keep the full screen visible"
        );
    }

    #[test]
    fn software_presenter_aspect_fits_and_centers_wide_windows() {
        assert_eq!(
            aspect_fit_dimensions(800, 600, 1920, 1080),
            (240, 0, 1440, 1080),
            "a 4:3 guest should fill a 16:9 window vertically"
        );
        assert_eq!(
            aspect_fit_dimensions(800, 600, 1280, 960),
            (0, 0, 1280, 960),
            "matching aspect ratios should fill the drawable"
        );
        assert_eq!(
            aspect_fit_dimensions(800, 600, 600, 800),
            (0, 175, 600, 450),
            "portrait windows should center the guest vertically"
        );
    }

    #[test]
    fn cropped_aspect_fit_mouse_mapping_inverts_the_presenter_viewport() {
        let content = ContentRect {
            left: 80,
            top: 60,
            width: 640,
            height: 480,
        };
        assert_eq!(
            physical_to_mac_in_viewport(0.0, 0.0, content, 1280, 960),
            (60, 80)
        );
        assert_eq!(
            physical_to_mac_in_viewport(1279.0, 959.0, content, 1280, 960),
            (539, 719)
        );
        assert_eq!(
            physical_to_mac_in_viewport(160.0, 0.0, content, 1280, 720),
            (60, 80),
            "left letterbox pixels clamp to the cropped guest edge"
        );
        for (v, h) in [(60, 80), (140, 300), (539, 719)] {
            let (px, py) = mac_to_physical_in_viewport(v, h, content, 1280, 720).unwrap();
            assert_eq!(
                physical_to_mac_in_viewport(px, py, content, 1280, 720),
                (v, h)
            );
        }
    }

    #[test]
    fn host_click_release_waits_until_the_guest_executes() {
        let mut latch = HostMouseReleaseLatch::default();
        latch.press();

        assert_eq!(latch.release((350, 580)), None);
        assert!(latch.requires_guest_progress());
        assert_eq!(latch.take_ready_release(), None);
        latch.observe_guest_progress();
        assert!(!latch.requires_guest_progress());
        assert_eq!(latch.take_ready_release(), Some((350, 580)));
        assert!(latch.requires_guest_progress());
        assert_eq!(latch.take_ready_release(), None);
        latch.observe_guest_progress();
        assert!(!latch.requires_guest_progress());
    }

    #[test]
    fn host_release_after_guest_progress_is_not_delayed() {
        let mut latch = HostMouseReleaseLatch::default();
        latch.press();
        latch.observe_guest_progress();

        assert_eq!(latch.release((350, 580)), Some((350, 580)));
        assert!(latch.requires_guest_progress());
        assert_eq!(latch.take_ready_release(), None);
        latch.observe_guest_progress();
        assert!(!latch.requires_guest_progress());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn software_presentation_uses_the_same_cropped_pixels() {
        // The presenter reads this rectangle out of the whole frame with the
        // frame's own row stride; metal_present tests the upload itself.
        let layout = presentation_layout(
            ContentRect {
                left: 1,
                top: 1,
                width: 4,
                height: 3,
            },
            6,
        );
        assert_eq!(
            layout,
            metal_present::RasterLayout {
                left: 1,
                top: 1,
                width: 4,
                height: 3,
                buffer_width: 6,
            }
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_menu_viewport_excludes_only_reserved_guest_rows() {
        let full = ContentRect {
            left: 0,
            top: 0,
            width: 800,
            height: 600,
        };
        let visible = ContentRect {
            top: 20,
            height: 580,
            ..full
        };
        assert_eq!(presentation_content_rect(full, None, 800, 600, 20), visible);
        assert_eq!(
            presentation_content_rect(visible, None, 800, 600, 20),
            visible
        );
        // Guest fullscreen and disabled native integration keep every row.
        assert_eq!(presentation_content_rect(full, None, 800, 600, 0), full);
        // Respect non-default fonts, and ignore invalid heights.
        assert_eq!(presentation_content_rect(full, None, 800, 600, 24).top, 24);
        assert_eq!(presentation_content_rect(full, None, 800, 600, 600), full);
        assert_eq!(
            presentation_content_rect(full, None, 800, 600, u32::MAX),
            full
        );
        assert_eq!(
            physical_to_mac_in_viewport(0.0, 0.0, visible, 1600, 1160),
            (20, 0)
        );
        assert_eq!(
            physical_to_mac_in_viewport(1599.0, 1159.0, visible, 1600, 1160),
            (599, 799)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_menu_dialog_expansion_does_not_restore_hidden_menu_rows() {
        let game = ContentRect {
            left: 100,
            top: 100,
            width: 600,
            height: 400,
        };
        let expanded = presentation_content_rect(game, Some((0, 40, 550, 760)), 800, 600, 20);
        assert_eq!(
            expanded,
            ContentRect {
                left: 40,
                top: 20,
                width: 720,
                height: 530
            }
        );
        assert_eq!(presentation_content_rect(game, None, 800, 600, 20), game);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_menu_viewport_reads_guest_height_without_mutating_it() {
        use systemless::memory::{globals::addr::MBAR_HEIGHT, MemoryBus};
        for enabled in [false, true] {
            let mut app = App::new(PathBuf::from("dummy"), false, enabled, false, 8);
            let mut driver = app.driver.take().unwrap();
            driver.runner = Some(FixtureRunner::new(8 * 1024 * 1024, Default::default()));
            driver
                .runner
                .as_mut()
                .unwrap()
                .bus_mut()
                .write_word(MBAR_HEIGHT, 24);
            driver.capture_state(&mut app.guest_state, false);
            assert_eq!(
                app.guest_state.hidden_menu_height,
                if enabled { 24 } else { 0 }
            );
            assert_eq!(
                driver.runner.as_ref().unwrap().bus().read_word(MBAR_HEIGHT),
                24
            );
            driver
                .runner
                .as_mut()
                .unwrap()
                .bus_mut()
                .write_word(MBAR_HEIGHT, 0);
            driver.capture_state(&mut app.guest_state, false);
            assert_eq!(app.guest_state.hidden_menu_height, 0);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn visible_dialog_temporarily_extends_cached_gameplay_crop() {
        let gameplay = ContentRect {
            left: 80,
            top: 104,
            width: 640,
            height: 392,
        };
        assert_eq!(
            presentation_content_rect(gameplay, Some((85, 228, 233, 572)), 800, 600, 0),
            ContentRect {
                left: 80,
                top: 85,
                width: 640,
                height: 411,
            }
        );
        assert_eq!(
            presentation_content_rect(gameplay, None, 800, 600, 0),
            gameplay,
            "dismissing the dialog must restore the cached gameplay crop"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dialog_expands_native_window_without_reducing_guest_pixel_scale() {
        let gameplay = ContentRect {
            left: 80,
            top: 104,
            width: 640,
            height: 392,
        };
        let dialog = ContentRect {
            left: 80,
            top: 85,
            width: 640,
            height: 411,
        };
        assert_eq!(
            native_size_preserving_guest_scale(
                gameplay,
                dialog,
                winit::dpi::PhysicalSize::new(640, 392)
            ),
            winit::dpi::PhysicalSize::new(640, 411)
        );
        assert_eq!(
            native_size_preserving_guest_scale(
                gameplay,
                dialog,
                winit::dpi::PhysicalSize::new(1280, 784)
            ),
            winit::dpi::PhysicalSize::new(1280, 822)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dialog_growth_keeps_gameplay_fixed_on_the_desktop() {
        let gameplay = ContentRect {
            left: 80,
            top: 104,
            width: 640,
            height: 392,
        };
        let dialog = ContentRect {
            left: 80,
            top: 85,
            width: 640,
            height: 411,
        };
        let original_position = winit::dpi::PhysicalPosition::new(300, 200);
        assert_eq!(
            native_position_preserving_guest_anchor(
                gameplay,
                dialog,
                winit::dpi::PhysicalSize::new(640, 392),
                winit::dpi::PhysicalSize::new(640, 411),
                original_position,
            ),
            winit::dpi::PhysicalPosition::new(300, 181),
            "adding 19 guest pixels above the crop should grow the window upward"
        );
        assert_eq!(
            native_position_preserving_guest_anchor(
                gameplay,
                dialog,
                winit::dpi::PhysicalSize::new(1280, 784),
                winit::dpi::PhysicalSize::new(1280, 822),
                original_position,
            ),
            winit::dpi::PhysicalPosition::new(300, 162),
            "the desktop adjustment should scale with the native pixel multiple"
        );
    }
}
