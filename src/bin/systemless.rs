//! Systemless Game Runner with graphical display.
//!
//! `cargo install systemless` installs this binary as `systemless`.
//! In a checkout, `cargo run`'s `default-run` (set in Cargo.toml)
//! routes here; the `gui` feature is on by default. So the local
//! invocation is:
//!
//! ```sh
//! cargo run --release -- [--headless] [--max-instructions N] \
//!     [--cpu-mhz N] [--show-menu-bar] [--arrows-as-numpad] <game>
//! ```
//!
//! Disable the GUI deps with `--no-default-features` to build a
//! headless-only library and skip the `winit` / `softbuffer` / `cpal`
//! link.

#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;
#[cfg(target_os = "macos")]
#[path = "desktop/metal_present.rs"]
mod metal_present;
#[cfg(target_os = "macos")]
#[path = "desktop/native_menu.rs"]
mod native_menu;

#[cfg(not(target_os = "macos"))]
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;

use desktop_save_store::DesktopSaveStore;
#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::NSObject};
#[cfg(target_os = "macos")]
use objc2_quartz_core::CATransaction;
use systemless::debug_overlay::DebugOverlayFrameStats;
use systemless::display;
use systemless::game;
use systemless::runner::FixtureRunner;
use systemless::trap::dispatch::ScreenCopyBitsRect;

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

/// Initial screen dimensions: 800x600 8bpp color mode by default.
const INITIAL_SCREEN_WIDTH: u32 = 800;
const INITIAL_SCREEN_HEIGHT: u32 = 600;
const SCALE: u32 = 1;
/// Frame duration at 60.15 Hz (Compact Mac VBL rate).
const FRAME_DURATION: std::time::Duration = std::time::Duration::from_micros(16_625);
const MIN_RENDER_HEADROOM: std::time::Duration = std::time::Duration::from_micros(1_500);
const MAX_RENDER_HEADROOM: std::time::Duration = std::time::Duration::from_micros(8_000);
const RENDER_HEADROOM_MARGIN: std::time::Duration = std::time::Duration::from_micros(500);
/// Foreground GUI work is checked against the host deadline only between
/// batches. Keep each slice well below a realtime VBL so heavy startup loads
/// can still present intermediate drawing and service Sound Manager callbacks.
const CPU_BATCH_INSTRUCTIONS: usize = 10_000;
const SOUND_CALLBACK_SLICE_INSTRUCTIONS: usize = CPU_BATCH_INSTRUCTIONS;
const SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME: usize = 25_000;
const AUDIO_CALLBACK_CHUNK_SAMPLES: usize = 32;
/// Pixel or CopyBits inference must agree across distinct guest drawing
/// updates before it can crop the presentation. Geometry from the manual
/// CPort already selected by the HLE is authoritative immediately.
const CONTENT_RECT_CONFIRMATIONS: u16 = 5;
#[cfg(target_os = "macos")]
const VIEWPORT_CACHE_FILE: &str = "viewport.json";
const MAX_AUDIO_MIX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const DEFAULT_GUI_ARROWS_AS_NUMPAD: bool = false;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct ContentRect {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

#[cfg(target_os = "macos")]
#[derive(serde::Deserialize, serde::Serialize)]
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
    cache.version == 1
        && cache.screen_width != 0
        && cache.screen_height != 0
        && content.width != 0
        && content.height != 0
        && content.left.saturating_add(content.width) <= u32::from(cache.screen_width)
        && content.top.saturating_add(content.height) <= u32::from(cache.screen_height)
        && content.left == (u32::from(cache.screen_width).saturating_sub(content.width)) / 2
        && content.top == (u32::from(cache.screen_height).saturating_sub(content.height)) / 2
}

#[cfg(target_os = "macos")]
fn load_cached_content_rect(game_path: &std::path::Path) -> Option<CachedContentRect> {
    let path = viewport_cache_path(game_path);
    let bytes = std::fs::read(&path).ok()?;
    let cache: CachedContentRect = serde_json::from_slice(&bytes).ok()?;
    valid_cached_content_rect(&cache).then_some(cache)
}

#[cfg(target_os = "macos")]
fn persist_content_rect(
    game_path: &std::path::Path,
    screen_mode: (u32, u32, u16, u16, u16),
    content: ContentRect,
) {
    let cache = CachedContentRect {
        version: 1,
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
        .with_disallow_hidpi(true)
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

    let (steps, _running) = runner.run_pending_sound_work(callback_budget);
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

struct App {
    window: Option<Rc<Window>>,
    #[cfg(target_os = "macos")]
    surface: Option<metal_present::MetalPresenter>,
    #[cfg(not(target_os = "macos"))]
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    #[cfg(not(target_os = "macos"))]
    surface_size: Option<(u32, u32)>,
    frame_argb: Vec<u32>,
    #[cfg(target_os = "macos")]
    content_rect: Option<ContentRect>,
    #[cfg(target_os = "macos")]
    content_rect_candidate: Option<(ContentRect, u16)>,
    #[cfg(target_os = "macos")]
    content_rect_copybits_count: u64,
    /// Previous raw guest frame used only while learning the pixel-content
    /// fallback. Repeated host presentations of one image do not confirm it.
    #[cfg(target_os = "macos")]
    content_rect_previous_frame: Vec<u8>,
    #[cfg(target_os = "macos")]
    content_rect_screen_mode: Option<(u16, u16, u16)>,
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
    scaled_row: Vec<u32>,
    runner: Option<FixtureRunner>,
    save_store: Option<DesktopSaveStore>,
    game_path: PathBuf,
    initialized: bool,
    total_instructions: u64,
    /// Wall-clock origin for deriving tick targets.
    start_time: Option<std::time::Instant>,
    /// Next frame target for pacing.
    next_frame_time: Option<std::time::Instant>,
    /// Adaptive CPU/render split for the single-threaded GUI loop.
    render_headroom: std::time::Duration,
    /// Wall-clock boundary up to which GUI CPU time has been budgeted.
    next_cpu_budget_time: Option<std::time::Instant>,
    /// Fractional guest instructions carried between GUI slices.
    cpu_instruction_credit: f64,
    /// Fractional host samples carried between GUI slices to preserve rate.
    audio_sample_remainder: f64,
    /// Wall-clock instant represented by the most recently queued audio.
    /// Unlike video, audio cannot simply drop a late host frame without
    /// starving the device ring buffer.
    last_audio_mix_time: Option<std::time::Instant>,
    /// Current mouse position in physical window pixels
    mouse_physical: (f64, f64),
    /// Current game screen dimensions (tracks screen_mode changes)
    current_screen_width: u32,
    current_screen_height: u32,
    /// Frame counter for diagnostic screenshots
    frame_count: u64,
    /// Guest tick last presented to the host window.
    last_presented_guest_tick: Option<u32>,
    /// Force the next host present even if the guest tick has not advanced.
    force_next_render: bool,
    /// Force a Metal submission even if all visible guest inputs are
    /// unchanged, for native expose/resize events that need a fresh drawable.
    #[cfg(target_os = "macos")]
    force_gpu_present: bool,
    /// Show the Systemless debug overlay on top of the game framebuffer.
    debug_overlay_visible: bool,
    debug_last_frame_at: Option<std::time::Instant>,
    debug_host_fps: Option<f64>,
    debug_frame_ms: Option<f64>,
    /// Remap arrow keys to numpad equivalents (for keyboards without a numpad)
    arrows_as_numpad: bool,
    /// Optional GUI CPU cap in instructions per second (cpu_mhz × 1_000_000).
    emulated_ips: Option<f64>,
    /// CLI override of the default-hidden menu bar. When true, the
    /// HLE renders the menu bar even though the dispatcher's
    /// `menu_bar_hidden` defaults to true. Wired through to
    /// `runner.dispatcher_mut().menu_bar_hidden = false` at runner
    /// construction.
    show_menu_bar: bool,
    #[cfg(target_os = "macos")]
    native_menu: native_menu::NativeMenuBridge,
}

impl App {
    fn new(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        cpu_mhz: Option<f64>,
        show_menu_bar: bool,
    ) -> Self {
        #[cfg(target_os = "macos")]
        let native_menu_app_name = game_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Systemless")
            .to_owned();
        #[cfg(target_os = "macos")]
        let cached_content = load_cached_content_rect(&game_path);
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
            window: None,
            surface: None,
            #[cfg(not(target_os = "macos"))]
            surface_size: None,
            frame_argb: Vec::new(),
            #[cfg(target_os = "macos")]
            content_rect: cached_content.as_ref().map(|cache| cache.content),
            #[cfg(target_os = "macos")]
            content_rect_candidate: None,
            #[cfg(target_os = "macos")]
            content_rect_copybits_count: 0,
            #[cfg(target_os = "macos")]
            content_rect_previous_frame: Vec::new(),
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
            scaled_row: Vec::new(),
            runner: None,
            save_store: None,
            game_path,
            initialized: false,
            total_instructions: 0,
            start_time: None,
            next_frame_time: None,
            render_headroom: MIN_RENDER_HEADROOM,
            next_cpu_budget_time: None,
            cpu_instruction_credit: 0.0,
            audio_sample_remainder: 0.0,
            last_audio_mix_time: None,
            mouse_physical: (0.0, 0.0),
            current_screen_width: INITIAL_SCREEN_WIDTH,
            current_screen_height: INITIAL_SCREEN_HEIGHT,
            frame_count: 0,
            last_presented_guest_tick: None,
            force_next_render: true,
            #[cfg(target_os = "macos")]
            force_gpu_present: true,
            debug_overlay_visible: false,
            debug_last_frame_at: None,
            debug_host_fps: None,
            debug_frame_ms: None,
            arrows_as_numpad,
            emulated_ips: cpu_mhz.map(|mhz| mhz * 1_000_000.0),
            show_menu_bar,
            #[cfg(target_os = "macos")]
            native_menu: native_menu::NativeMenuBridge::new(native_menu_app_name),
        }
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
        let content = presentation_content_rect(
            self.content_rect.unwrap_or(ContentRect {
                left: 0,
                top: 0,
                width: sw,
                height: sh,
            }),
            self.runner.as_ref().and_then(|runner| {
                runner
                    .dispatcher()
                    .visible_dialog_structure_bounds(runner.bus())
            }),
            sw,
            sh,
        );
        #[cfg(not(target_os = "macos"))]
        let content = ContentRect {
            left: 0,
            top: 0,
            width: sw,
            height: sh,
        };

        physical_to_mac_in_viewport(px, py, content, size.width, size.height)
    }

    fn init_game(&mut self) {
        if self.initialized {
            return;
        }

        let mut runner = game::new_runner();
        if self.show_menu_bar {
            runner.set_menu_bar_visible(true);
        }
        let app =
            game::load_game_from_path(&mut runner, &self.game_path).expect("Failed to load game");
        #[cfg(target_os = "macos")]
        if let Some(executable_name) = runner
            .dispatcher()
            .launched_app_path()
            .and_then(|path| path.rsplit('/').next())
            .filter(|name| !name.is_empty())
        {
            self.native_menu.set_app_name(executable_name.to_owned());
        }
        let mut save_store = DesktopSaveStore::for_loaded_archive(&self.game_path, &mut runner);
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
        runner.set_arrows_as_numpad(self.arrows_as_numpad);

        // Configure realtime instructions/tick budget so the wall-clock-paced
        // GUI can actually make progress per frame. Without this the runner
        // uses INSTRUCTIONS_PER_TICK = 12_000 (intended for scripted harnesses/tests)
        // and the deadline_tick cap throttles EV's boot to ~700K IPS — far
        // too slow to ever reach the menu. The realtime target is 25 MHz at
        // 60.15 Hz VBL ≈ 415_628 instructions per tick.
        let ipt = (systemless::runner::DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND
            / systemless::runner::DEFAULT_VBL_HZ) as u32;
        runner.set_instructions_per_tick(ipt);
        eprintln!("[SYSTEMLESS] Instructions per tick: {}", ipt);

        // Initialize audio output.
        if let Some(audio) = systemless::audio::CpalAudioBackend::new() {
            runner.set_audio(Box::new(audio));
        } else {
            eprintln!("[SYSTEMLESS] Warning: could not initialize audio output");
        }

        eprintln!("[SYSTEMLESS] Game loaded: {}", self.game_path.display());
        eprintln!(
            "[SYSTEMLESS] A5=${:08X}, Entry=${:08X}",
            app.a5_base,
            app.entry_point(app.a5_base)
        );

        self.runner = Some(runner);
        self.save_store = Some(save_store);
        self.initialized = true;
    }

    fn sync_save_files(&mut self, force: bool) {
        let Some(save_store) = self.save_store.as_mut() else {
            return;
        };
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if force {
            save_store.sync_save_files_now(runner);
        } else {
            save_store.sync_save_files(runner);
        }
    }

    fn cpu_budget_for_duration(duration: std::time::Duration, ips: f64, credit: &mut f64) -> usize {
        *credit += duration.as_secs_f64() * ips;
        let budget = credit.floor().min(game::MAX_INSTRUCTIONS_PER_FRAME as f64) as usize;
        *credit -= budget as f64;
        budget
    }

    /// Wall-clock origin such that `tick_due_at(origin, now)` equals `guest_tick`.
    /// Shifts the origin back so a boot-seeded, non-zero TickCount does not make
    /// the pacer wait real time before running any guest CPU work.
    fn wall_clock_origin_for_guest_tick(
        now: std::time::Instant,
        guest_tick: u32,
    ) -> std::time::Instant {
        // Add a half-tick of lead before flooring so `tick_due_at` reliably
        // maps `now` back to `guest_tick` (rather than `guest_tick - 1` after
        // float truncation), guaranteeing the first frame already has runnable
        // guest work. The half-tick (~8ms) lead is sub-frame and harmless.
        now.checked_sub(std::time::Duration::from_secs_f64(
            (guest_tick as f64 + 0.5) / systemless::runner::DEFAULT_VBL_HZ,
        ))
        .unwrap_or(now)
    }

    fn tick_due_at(origin: std::time::Instant, at: std::time::Instant) -> u32 {
        at.checked_duration_since(origin)
            .unwrap_or_default()
            .as_secs_f64()
            .mul_add(systemless::runner::DEFAULT_VBL_HZ, 0.0)
            .floor() as u32
    }

    fn audio_samples_for_duration(duration: std::time::Duration, remainder: &mut f64) -> usize {
        let total_samples = duration
            .as_secs_f64()
            .mul_add(systemless::sound::OUTPUT_RATE as f64, *remainder);
        let whole_samples = total_samples.floor();
        *remainder = total_samples - whole_samples;
        whole_samples as usize
    }

    fn next_render_headroom(render_time: std::time::Duration) -> std::time::Duration {
        let target = render_time.saturating_add(RENDER_HEADROOM_MARGIN);
        target.clamp(MIN_RENDER_HEADROOM, MAX_RENDER_HEADROOM)
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

    fn next_frame_target(
        now: std::time::Instant,
        scheduled: std::time::Instant,
    ) -> (std::time::Instant, bool) {
        if now.saturating_duration_since(scheduled) >= FRAME_DURATION {
            (now + FRAME_DURATION, true)
        } else {
            (scheduled + FRAME_DURATION, false)
        }
    }

    fn step_frame(&mut self) {
        let Some(runner) = self.runner.as_ref() else {
            return;
        };

        if runner.is_halted() {
            return;
        }

        let now = runner.host_now();
        // Seed the wall-clock origin from the guest's current tick, not `now`.
        // The runner boots with a non-zero TickCount (DEFAULT_LAUNCH_TICKS ≈ 600
        // ≈ 10s of simulated post-boot time), so anchoring the origin at `now`
        // would leave the guest clock 600 ticks "ahead" of the wall clock. With
        // `ticks_behind` saturating to 0, the CPU loop would advance no work for
        // ~10 real seconds until the wall clock caught up — a launch stall. See
        // wall_clock_origin_for_guest_tick in systemless.org/src/emulator.rs.
        let start = *self.start_time.get_or_insert_with(|| {
            Self::wall_clock_origin_for_guest_tick(now, runner.guest_tick())
        });
        let scheduled_frame_end = self.next_frame_time.unwrap_or(now + FRAME_DURATION);

        // Wall-clock tick target: where the game clock should be right now.
        let target_tick = Self::tick_due_at(start, scheduled_frame_end);
        let current_tick = runner.guest_tick();

        // Cap ticks-to-advance at 2 per frame. If the game is behind,
        // we accept the lag rather than trying to catch up (which causes
        // the CPU to run for 100ms+ and drops frames further). When the
        // game is more than 2 ticks behind, we reset the wall-clock
        // origin so it can recover without a runaway spiral.
        let ticks_behind = target_tick.saturating_sub(current_tick);
        if ticks_behind > 4 {
            // Game fell too far behind — snap the wall-clock origin forward
            // so the target aligns with where the game actually is.
            // This prevents the death spiral where each frame tries to
            // catch up, takes too long, falls further behind, repeat.
            self.start_time = Some(
                now - std::time::Duration::from_secs_f64(
                    (current_tick + 2) as f64 / systemless::runner::DEFAULT_VBL_HZ,
                ),
            );
        }
        let effective_target = current_tick.saturating_add(ticks_behind.min(2));

        // CPU budget: wall-clock time left in this frame, minus render headroom.
        // The CPU runs in small batches, checking the clock between batches.
        let cpu_deadline = scheduled_frame_end
            .checked_sub(self.render_headroom)
            .map(|d| d.max(now))
            .unwrap_or(now);

        let slice_budget = if let Some(ips) = self.emulated_ips {
            let cpu_interval_start = self.next_cpu_budget_time.unwrap_or(now);
            let cpu_interval = cpu_deadline
                .checked_duration_since(cpu_interval_start)
                .unwrap_or_default();
            let budget =
                Self::cpu_budget_for_duration(cpu_interval, ips, &mut self.cpu_instruction_credit);
            self.next_cpu_budget_time = Some(cpu_deadline);
            budget
        } else {
            self.next_cpu_budget_time = Some(cpu_deadline);
            game::MAX_INSTRUCTIONS_PER_FRAME
        };
        let audio_interval = self
            .last_audio_mix_time
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or(FRAME_DURATION)
            .min(MAX_AUDIO_MIX_INTERVAL);
        let audio_samples =
            Self::audio_samples_for_duration(audio_interval, &mut self.audio_sample_remainder);
        if std::env::var_os("SYSTEMLESS_TRACE_AUDIO").is_some()
            && audio_interval > FRAME_DURATION + FRAME_DURATION / 2
        {
            eprintln!(
                "[AUDIO] recovering {:.1} ms of host time ({} source samples)",
                audio_interval.as_secs_f64() * 1000.0,
                audio_samples
            );
        }

        let runner = self.runner.as_mut().expect("runner checked above");

        // Mix one host frame of audio per GUI frame. Sound Manager doubleback
        // callbacks run at interrupt time, including while menu/control
        // tracking keeps the application-visible TickCount fixed, so same-tick
        // frames still need audio. Do not catch up multiple late host frames at
        // once: that drains SndPlayDoubleBuffer queues faster than their
        // callbacks can refill them and turns low-rate effects into fragments.
        // Sound 1994, 2-72 and 2-146 to 2-148.
        let mut audio_mixed = 0usize;
        let mut total_steps = 0usize;
        let mut foreground_steps = 0usize;
        let mut reserved_sound_steps = 0usize;

        loop {
            if runner.guest_tick() >= effective_target || runner.is_halted() {
                break;
            }
            if runner.host_now() >= cpu_deadline {
                break;
            }

            let remaining = slice_budget.saturating_sub(total_steps);
            if remaining == 0 {
                break;
            }

            let batch_size = remaining.min(CPU_BATCH_INSTRUCTIONS);
            let remaining_audio = audio_samples.saturating_sub(audio_mixed);
            let batches_left = remaining.div_ceil(CPU_BATCH_INSTRUCTIONS).max(1);
            let batch_audio = if remaining_audio == 0 {
                0
            } else {
                remaining_audio.div_ceil(batches_left)
            };
            let (steps, running) =
                runner.run_gui_slice_with_audio(batch_size, effective_target, batch_audio);
            total_steps += steps;
            foreground_steps += steps;
            audio_mixed += batch_audio;
            if batch_audio > 0 {
                if let Some(steps) = service_pending_sound_work(
                    runner,
                    cpu_deadline,
                    slice_budget,
                    total_steps,
                    &mut reserved_sound_steps,
                ) {
                    total_steps += steps;
                }
            }
            if !running {
                break;
            }
        }

        if audio_mixed < audio_samples {
            if let Some(steps) = service_pending_sound_work(
                runner,
                cpu_deadline,
                slice_budget,
                total_steps,
                &mut reserved_sound_steps,
            ) {
                total_steps += steps;
            }
        }

        if audio_mixed < audio_samples {
            let mut remaining_audio = audio_samples - audio_mixed;
            while remaining_audio > 0 && !runner.is_halted() {
                let chunk_audio = remaining_audio.min(AUDIO_CALLBACK_CHUNK_SAMPLES);
                runner.mix_gui_audio_slice(chunk_audio);
                remaining_audio -= chunk_audio;
                if let Some(steps) = service_pending_sound_work(
                    runner,
                    cpu_deadline,
                    slice_budget,
                    total_steps,
                    &mut reserved_sound_steps,
                ) {
                    total_steps += steps;
                }
            }
        }

        if let Some(steps) = service_pending_sound_work(
            runner,
            cpu_deadline,
            slice_budget,
            total_steps,
            &mut reserved_sound_steps,
        ) {
            total_steps += steps;
        }

        self.total_instructions += total_steps as u64;
        if foreground_steps > 0 && runner.guest_tick() == current_tick {
            // Loading and animation code can draw substantial work before the
            // next VBL tick. Present that progress instead of batching it into
            // a later tick, which makes startup look choppy.
            self.force_next_render = true;
        }

        // Optional tick-lag instrumentation. Gate on
        // SYSTEMLESS_TRACE_TICK_LAG=1. Logs target/current tick counts and
        // CPU budget vs instructions actually executed each frame.
        //   - Logs EVERY frame when ticks_behind > 0 (lag event).
        //   - Also logs ONCE PER SECOND (every 60 frames) as a steady-
        //     state sample so the user sees baseline performance.
        // Interpretation: if cpu_used / slice_budget < 1.0 consistently,
        // the host CPU can't keep up with the 25 MHz target and
        // animations will lag.
        if std::env::var_os("SYSTEMLESS_TRACE_TICK_LAG").is_some() {
            let final_tick = runner.guest_tick();
            let advanced = final_tick.saturating_sub(current_tick);
            let steady_sample = self.frame_count.is_multiple_of(60);
            if ticks_behind > 0 || steady_sample {
                let tag = if ticks_behind > 0 { "LAG" } else { "OK " };
                eprintln!(
                    "[TICK_LAG {}] frame={} target={} current={} behind={} \
                     advanced={} budget={} used={}",
                    tag,
                    self.frame_count,
                    target_tick,
                    current_tick,
                    ticks_behind,
                    advanced,
                    slice_budget,
                    total_steps,
                );
            }
        }
    }

    fn should_render_frame(&self) -> bool {
        if self.force_next_render {
            return true;
        }
        if self.debug_overlay_visible {
            return true;
        }
        let Some(runner) = self.runner.as_ref() else {
            return false;
        };
        runner.is_halted()
            || runner.is_ui_tracking_active()
            || self.last_presented_guest_tick != Some(runner.guest_tick())
    }

    fn render_frame(&mut self) {
        let render_start = std::time::Instant::now();
        #[cfg(target_os = "macos")]
        let force_gpu_present = self.force_gpu_present;
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
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        runner.composite_frame();
        let presented_tick = runner.guest_tick();

        let (_, _, scrn_right, scrn_bottom, _) = runner.dispatcher().screen_mode;
        let game_w = scrn_right as u32;
        let game_h = scrn_bottom as u32;
        let mut buf_w = size.width;
        let mut buf_h = size.height;

        #[cfg(target_os = "macos")]
        let mut core_animation_transaction: Option<CoreAnimationTransaction> = None;

        if buf_w == 0 || buf_h == 0 || game_w == 0 || game_h == 0 {
            return;
        }

        let screen_mode = runner.dispatcher().screen_mode;
        let device_clut = runner.dispatcher().device_clut;
        let cursor = runner.dispatcher().cursor().cloned();
        let mouse_pos = runner.dispatcher().mouse_position();

        #[cfg(target_os = "macos")]
        if !self.debug_overlay_visible {
            let screen_signature = (screen_mode.2, screen_mode.3, screen_mode.4);
            if self.content_rect_screen_mode != Some(screen_signature) {
                self.content_rect_screen_mode = Some(screen_signature);
                self.content_rect = None;
                self.content_rect_candidate = None;
                self.content_rect_copybits_count = 0;
                self.content_rect_previous_frame.clear();
            }

            let authoritative_rect = (self.content_rect.is_none())
                .then(|| {
                    let dispatcher = runner.dispatcher();
                    dispatcher
                        .declared_centered_presentation_rect(runner.bus())
                        .or_else(|| dispatcher.manual_cport_presentation_rect(runner.bus()))
                })
                .flatten()
                .and_then(|rect| centered_content_rect_from_copybits(rect, game_w, game_h));

            let framebuffer_len = screen_mode.1.saturating_mul(u32::from(screen_mode.3));
            let framebuffer = runner.bus().ram_slice(screen_mode.0, framebuffer_len);
            let mut accepted_rect = authoritative_rect;
            if self.content_rect.is_none() && accepted_rect.is_none() {
                let copybits_count = runner.dispatcher().copybits_screen_count;
                let mut detected = None;
                if copybits_count != self.content_rect_copybits_count {
                    let confirmations = copybits_count
                        .saturating_sub(self.content_rect_copybits_count)
                        .min(u64::from(u16::MAX)) as u16;
                    self.content_rect_copybits_count = copybits_count;
                    detected = runner
                        .dispatcher()
                        .last_screen_copybits_rect
                        .and_then(|rect| centered_content_rect_from_copybits(rect, game_w, game_h))
                        .map(|rect| (rect, confirmations));
                }
                if detected.is_none()
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
                    accepted_rect = self
                        .content_rect_candidate
                        .filter(|(_, count)| *count >= CONTENT_RECT_CONFIRMATIONS)
                        .map(|(rect, _)| rect);
                }
            }

            if self.content_rect.is_none() {
                if let Some(rect) = accepted_rect {
                    eprintln!(
                        "[SYSTEMLESS] Guest content: {}x{} at ({},{}) inside {}x{}",
                        rect.width, rect.height, rect.left, rect.top, game_w, game_h
                    );
                    self.content_rect = Some(rect);
                    persist_content_rect(&self.game_path, screen_mode, rect);
                    if let Some(window) = self.window.as_ref() {
                        let current = window.inner_size();
                        let integer_scale = (current.width / rect.width)
                            .min(current.height / rect.height)
                            .max(1);
                        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(
                            rect.width.saturating_mul(integer_scale),
                            rect.height.saturating_mul(integer_scale),
                        ));
                    }
                    self.window_sized_content_rect = Some(rect);
                }
            }

            let stable_content = self.content_rect.unwrap_or(ContentRect {
                left: 0,
                top: 0,
                width: game_w,
                height: game_h,
            });
            let desired_content = presentation_content_rect(
                stable_content,
                runner
                    .dispatcher()
                    .visible_dialog_structure_bounds(runner.bus()),
                game_w,
                game_h,
            );
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
            if transition_needed {
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
            let palette = display::argb_palette_from_clut(&device_clut);
            if let Some(surface) = self.surface.as_mut() {
                let presented_directly = surface
                    .present_guest_frame(
                        framebuffer,
                        screen_mode,
                        (content.left, content.top, content.width, content.height),
                        &palette,
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
                    self.render_headroom = Self::next_render_headroom(render_start.elapsed());
                    return;
                }
            }
        }

        let mut frame_argb = std::mem::take(&mut self.frame_argb);
        display::render_screen_argb(runner.bus(), screen_mode, &device_clut, &mut frame_argb);
        if let Some(cursor) = cursor.as_ref() {
            display::render_cursor_argb(&mut frame_argb, game_w, game_h, cursor, mouse_pos);
        }
        if self.debug_overlay_visible {
            let lines = runner
                .debug_overlay_snapshot(DebugOverlayFrameStats {
                    host_fps: self.debug_host_fps,
                    frame_ms: self.debug_frame_ms,
                    ..DebugOverlayFrameStats::default()
                })
                .lines();
            display::render_debug_overlay_argb(&mut frame_argb, game_w, game_h, &lines);
        }

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.surface.as_mut() else {
                self.frame_argb = frame_argb;
                return;
            };
            surface
                .present(&frame_argb, game_w, game_h, buf_w, buf_h)
                .expect("Failed to present Metal framebuffer");
        }

        #[cfg(not(target_os = "macos"))]
        {
            let scale = (buf_w / game_w).min(buf_h / game_h).max(1) as usize;
            let draw_w = game_w as usize * scale;
            let draw_h = game_h as usize * scale;
            let mut scaled_row = std::mem::take(&mut self.scaled_row);

            let Some(surface) = self.surface.as_mut() else {
                self.frame_argb = frame_argb;
                self.scaled_row = scaled_row;
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

            if draw_w != buf_w as usize || draw_h != buf_h as usize {
                buffer.fill(0xFF000000);
            }

            if scale == 1 {
                for row in 0..game_h as usize {
                    let src_row = &frame_argb[row * game_w as usize..(row + 1) * game_w as usize];
                    let dst_offset = row * buf_w as usize;
                    buffer[dst_offset..dst_offset + game_w as usize].copy_from_slice(src_row);
                }
            } else {
                scaled_row.resize(draw_w, 0xFF000000);
                for row in 0..game_h as usize {
                    let src_row = &frame_argb[row * game_w as usize..(row + 1) * game_w as usize];
                    for (dst_chunk, &pixel) in
                        scaled_row.chunks_exact_mut(scale).zip(src_row.iter())
                    {
                        dst_chunk.fill(pixel);
                    }
                    let dst_row_start = row * scale * buf_w as usize;
                    for repeat in 0..scale {
                        let dst_offset = dst_row_start + repeat * buf_w as usize;
                        buffer[dst_offset..dst_offset + draw_w].copy_from_slice(&scaled_row);
                    }
                }
            }

            self.scaled_row = scaled_row;
            buffer.present().expect("Failed to present buffer");
        }

        self.frame_argb = frame_argb;
        self.last_presented_guest_tick = Some(presented_tick);
        self.force_next_render = false;
        self.render_headroom = Self::next_render_headroom(render_start.elapsed());
    }
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

/// Extend a stable gameplay crop just enough to include transient system UI.
/// The learned/cached rectangle remains unchanged, so dismissing a dialog
/// restores the normal viewport without relearning it or resizing the native
/// window.
#[cfg(target_os = "macos")]
fn presentation_content_rect(
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

fn centered_content_rect_from_copybits(
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
    let horizontal_tolerance = (screen_width / 100).max(4);
    let vertical_tolerance = (screen_height / 100).max(4);
    let has_border = left >= 4 || right_margin >= 4 || top >= 4 || bottom_margin >= 4;
    if !has_border
        || left.abs_diff(right_margin) > horizontal_tolerance
        || top.abs_diff(bottom_margin) > vertical_tolerance
        || content_width < screen_width / 2
        || content_height < screen_height / 2
    {
        return None;
    }
    Some(ContentRect {
        left,
        top,
        width: content_width,
        height: content_height,
    })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            #[cfg(target_os = "macos")]
            let initial_size = self
                .content_rect
                .map(|content| (content.width, content.height))
                .unwrap_or((INITIAL_SCREEN_WIDTH, INITIAL_SCREEN_HEIGHT));
            #[cfg(not(target_os = "macos"))]
            let initial_size = (INITIAL_SCREEN_WIDTH, INITIAL_SCREEN_HEIGHT);
            let window_attrs = Window::default_attributes()
                .with_title("Systemless - Macintosh Emulator")
                .with_inner_size(winit::dpi::LogicalSize::new(
                    initial_size.0 * SCALE,
                    initial_size.1 * SCALE,
                ))
                .with_resizable(true);
            let window_attrs = platform_window_attrs(window_attrs);

            let window = Rc::new(
                event_loop
                    .create_window(window_attrs)
                    .expect("Failed to create window"),
            );
            window.set_cursor_visible(false);

            #[cfg(target_os = "macos")]
            let surface = metal_present::MetalPresenter::new(window.clone())
                .expect("Failed to create Metal presenter");
            #[cfg(not(target_os = "macos"))]
            let context =
                softbuffer::Context::new(window.clone()).expect("Failed to create context");
            #[cfg(not(target_os = "macos"))]
            let surface = Surface::new(&context, window.clone()).expect("Failed to create surface");

            self.window = Some(window);
            self.surface = Some(surface);

            // Initialize the game
            self.init_game();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.sync_save_files(true);
                eprintln!(
                    "[SYSTEMLESS] Window closed. Total instructions: {}",
                    self.total_instructions
                );
                event_loop.exit();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.force_next_render = true;
                self.mouse_physical = (position.x, position.y);
                let (v, h) = self.physical_to_mac(position.x, position.y);
                if let Some(runner) = self.runner.as_mut() {
                    runner.set_mouse_position(v, h);
                    runner.dispatcher_mut().show_cursor();
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.force_next_render = true;
                let (v, h) = self.physical_to_mac(self.mouse_physical.0, self.mouse_physical.1);
                if let Some(runner) = self.runner.as_mut() {
                    match state {
                        ElementState::Pressed => {
                            runner.push_mouse_down(v, h);
                        }
                        ElementState::Released => {
                            runner.push_mouse_up(v, h);
                        }
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                self.force_next_render = true;
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
                if let Some(runner) = self.runner.as_mut() {
                    let (mac_key, char_code) = host_key_to_mac(
                        &event.logical_key,
                        &event.physical_key,
                        event.text.as_ref().map(|t| t.as_str()),
                    );
                    // GUI key logging env-gated on `SYSTEMLESS_TRACE_GUI_KEY=1`
                    // — leaving it on would spam stderr for every keystroke.
                    if std::env::var_os("SYSTEMLESS_TRACE_GUI_KEY").is_some() {
                        eprintln!(
                            "[GUI-KEY] state={:?} physical_key={:?} mac_key=${:02X} char=${:02X} text={:?}",
                            event.state,
                            event.physical_key,
                            mac_key,
                            char_code,
                            event.text,
                        );
                    }
                    match event.state {
                        ElementState::Pressed => {
                            runner.push_key_down(mac_key, char_code);
                        }
                        ElementState::Released => {
                            runner.push_key_up(mac_key, char_code);
                        }
                    }
                }
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
                if size.width != 0 && size.height != 0 && self.runner.is_some() {
                    self.render_frame();
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
        let now = std::time::Instant::now();
        let next = self.next_frame_time.unwrap_or(now);

        if now < next {
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            return;
        }

        // Schedule the next host frame. If startup/resource loading makes us
        // miss a full presentation interval, drop the missed host frame instead
        // of running immediate catch-up frames that bunch audio and graphics.
        let (next_target, dropped_missed_frame) = Self::next_frame_target(now, next);
        if dropped_missed_frame {
            self.next_cpu_budget_time = Some(now);
            self.cpu_instruction_credit = 0.0;
        }
        self.next_frame_time = Some(next_target);
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_target));

        #[cfg(target_os = "macos")]
        if let Some(runner) = self.runner.as_mut() {
            for (menu_id, item_number) in self.native_menu.drain_commands() {
                runner.select_guest_menu_item(menu_id, item_number);
            }
        }

        // Step emulation, then render
        self.step_frame();
        self.sync_save_files(false);

        #[cfg(target_os = "macos")]
        if let Some(snapshot) = self.runner.as_mut().map(FixtureRunner::guest_menu_snapshot) {
            self.native_menu.sync(snapshot);
        }

        // Check if screen mode changed
        if let Some(runner) = &self.runner {
            let (_, _, sw, sh, _) = runner.dispatcher().screen_mode;
            let sw = sw as u32;
            let sh = sh as u32;
            if sw != self.current_screen_width || sh != self.current_screen_height {
                self.current_screen_width = sw;
                self.current_screen_height = sh;
                if let Some(window) = &self.window {
                    let _ = window
                        .request_inner_size(winit::dpi::LogicalSize::new(sw * SCALE, sh * SCALE));
                }
                self.force_next_render = true;
            }
        }

        if self.should_render_frame() {
            self.render_frame();
        }
        self.frame_count += 1;
    }
}

fn run_gui(game_path: PathBuf, arrows_as_numpad: bool, cpu_mhz: Option<f64>, show_menu_bar: bool) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    match cpu_mhz {
        Some(mhz) => eprintln!("[SYSTEMLESS] GUI CPU cap: {:.1} MHz", mhz),
        None => eprintln!("[SYSTEMLESS] GUI CPU cap: uncapped"),
    }
    eprintln!(
        "[SYSTEMLESS] GUI arrow keys: {}",
        if arrows_as_numpad {
            "keypad flight controls"
        } else {
            "literal Mac arrow keys"
        }
    );

    let mut app = App::new(game_path, arrows_as_numpad, cpu_mhz, show_menu_bar);
    event_loop.run_app(&mut app).expect("Event loop failed");
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

    let rgba = display::render_screen(
        runner.bus(),
        runner.dispatcher().screen_mode,
        &runner.dispatcher().device_clut,
    );

    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let idx = ((y * w + x) * 4) as usize;
        image::Rgb([rgba[idx], rgba[idx + 1], rgba[idx + 2]])
    });

    let ticks = runner.guest_tick();
    let path = format!("/tmp/systemless_headless_{:04}.png", num);
    img.save(&path).expect("Failed to save screenshot");
    eprintln!("[HEADLESS] Screenshot #{}: {} (ticks={})", num, path, ticks);
}

fn run_headless(game_path: &std::path::Path, max_instructions: usize, show_menu_bar: bool) {
    eprintln!("[HEADLESS] Starting: {}", game_path.display());
    eprintln!("[HEADLESS] Max instructions: {}", max_instructions);

    let mut runner = game::new_runner();
    if show_menu_bar {
        // CLI override of the default kiosk-mode hide. See
        // FixtureRunner::set_menu_bar_visible for the rationale.
        runner.set_menu_bar_visible(true);
    }
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

    let chunk = 100_000;
    let mut total: usize = 0;
    let mut last_screenshot = 0usize;

    while total < max_instructions {
        let steps_to_run = chunk.min(max_instructions - total);
        let (steps, running) = runner.run_steps(steps_to_run, None);
        total += steps;

        let screenshot_num = total / 500_000;
        if screenshot_num > last_screenshot {
            last_screenshot = screenshot_num;
            runner.composite_frame();
            save_screenshot(&runner, screenshot_num);
        }

        if !running {
            eprintln!("[HEADLESS] CPU stopped after {} instructions", total);
            break;
        }
    }

    eprintln!("[HEADLESS] Completed {} instructions", total);
    save_store.sync_save_files_now(&mut runner);
    save_screenshot(&runner, 9999);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut headless = false;
    let mut arrows_as_numpad = DEFAULT_GUI_ARROWS_AS_NUMPAD;
    let mut cpu_mhz: Option<f64> = None;
    let mut game_path_str = None;
    let mut max_instructions: Option<usize> = None;
    let mut show_menu_bar = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--headless" => headless = true,
            "--arrows-as-numpad" => arrows_as_numpad = true,
            "--literal-arrows" | "--no-arrows-as-numpad" => arrows_as_numpad = false,
            "--show-menu-bar" => show_menu_bar = true,
            "--cpu-mhz" => {
                i += 1;
                if let Some(mhz) = args.get(i).and_then(|s| s.parse::<f64>().ok()) {
                    cpu_mhz = Some(mhz);
                }
            }
            "--max-instructions" => {
                i += 1;
                max_instructions = args.get(i).and_then(|s| s.parse().ok());
            }
            _ => {
                if game_path_str.is_none() {
                    game_path_str = Some(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let game_path = match game_path_str {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!(
                "Usage: {} [--headless] [--arrows-as-numpad] [--literal-arrows] \
                 [--cpu-mhz N] [--max-instructions N] \
                 [--show-menu-bar] <game>",
                args[0]
            );
            std::process::exit(1);
        }
    };

    if !game_path.exists() {
        eprintln!("Error: Game file not found: {}", game_path.display());
        std::process::exit(1);
    }

    eprintln!("[SYSTEMLESS] Starting emulator...");
    eprintln!("[SYSTEMLESS] Game: {}", game_path.display());

    if headless {
        run_headless(
            &game_path,
            max_instructions.unwrap_or(5_000_000),
            show_menu_bar,
        );
    } else {
        run_gui(game_path, arrows_as_numpad, cpu_mhz, show_menu_bar);
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
mod tests {
    use super::*;
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
    fn wall_clock_origin_starts_pacer_at_seeded_guest_tick() {
        // The runner boots with a non-zero TickCount (~600). Anchoring the
        // wall-clock origin at that seeded tick means the pacer is level with
        // the guest immediately instead of waiting ~10 real seconds for the
        // wall clock to reach tick 600 before running any CPU.
        let now = std::time::Instant::now();
        let seeded_tick = 600;
        let origin = App::wall_clock_origin_for_guest_tick(now, seeded_tick);

        assert_eq!(
            App::tick_due_at(origin, now),
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
        let origin = App::wall_clock_origin_for_guest_tick(start, seeded_tick);
        let one_frame_later = start + FRAME_DURATION;

        assert!(
            App::tick_due_at(origin, one_frame_later) > seeded_tick,
            "the first post-boot frame should have runnable guest work"
        );
    }

    #[test]
    fn gui_defaults_to_literal_arrow_controls() {
        let app = App::new(
            PathBuf::from("dummy"),
            DEFAULT_GUI_ARROWS_AS_NUMPAD,
            None,
            false,
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
            .sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::Command {
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
            .sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::Command {
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
            let samples = App::audio_samples_for_duration(FRAME_DURATION, &mut remainder);
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
        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(runner);
        app.start_time = Some(now);
        app.next_frame_time = Some(now);

        app.step_frame();

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
        let pc = runner.bus_mut().alloc(256 * 1024);
        for offset in (0..256 * 1024).step_by(2) {
            runner.bus_mut().write_word(pc + offset, 0x4E71); // NOP
        }
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        runner.bus_mut().write_long(0x016A, 0);
        runner.set_instructions_per_tick(1_000_000);

        let mut app = App::new(PathBuf::from("dummy"), false, Some(1.0), false);
        app.runner = Some(runner);
        app.start_time = Some(now - FRAME_DURATION);
        app.next_frame_time = Some(now + FRAME_DURATION * 4);
        app.next_cpu_budget_time = Some(now);
        app.last_presented_guest_tick = Some(0);
        app.force_next_render = false;

        app.step_frame();

        let runner = app.runner.as_ref().unwrap();
        assert!(
            app.total_instructions > 0,
            "test setup should execute foreground startup work"
        );
        assert_eq!(
            runner.guest_tick(),
            0,
            "test setup should stay within the same VBL tick"
        );
        assert!(
            app.should_render_frame(),
            "same-tick foreground drawing progress should force a present"
        );
    }

    #[test]
    fn step_frame_services_pending_sound_before_late_same_tick_audio_mix() {
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
        runner.bus_mut().write_word(interrupted_pc, 0x4E71); // foreground NOP
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
        runner.dispatcher_mut().sound_manager.channels.push(chan);
        runner
            .dispatcher_mut()
            .sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(runner);
        app.start_time = Some(scheduled_frame_end);
        app.next_frame_time = Some(scheduled_frame_end);

        app.step_frame();

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
            !app.runner.as_ref().unwrap().has_pending_sound_work(),
            "sound callback should complete during the GUI sound-work slice"
        );
    }

    #[test]
    fn step_frame_services_doubleback_between_late_audio_chunks() {
        use systemless::cpu::Register;
        use systemless::memory::MemoryBus;
        use systemless::sound::{DoubleBufferState, SndChannel, OUTPUT_RATE};

        const REFILL_FRAMES: usize = 64;

        let now = std::time::Instant::now();
        let (mut runner, queued) = gui_runner_with_recording_audio();
        let interrupted_pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(interrupted_pc, 0x4E71); // foreground NOP
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
        runner.dispatcher_mut().sound_manager.channels.push(chan);

        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(runner);
        app.start_time = Some(now);
        app.next_frame_time = Some(now);

        app.step_frame();

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
        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(runner);
        app.start_time = Some(now);
        app.next_frame_time = Some(now);
        app.last_audio_mix_time =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(100));

        app.step_frame();

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

        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(runner);
        app.start_time = Some(now - FRAME_DURATION * 2);
        app.next_frame_time = Some(now + FRAME_DURATION);

        app.step_frame();

        assert!(
            (732..=734).contains(&*queued.borrow()),
            "one GUI frame should queue about 367 stereo frames, got {} bytes",
            *queued.borrow()
        );
    }

    #[test]
    fn cpu_budget_for_duration_preserves_average_mhz() {
        let mut credit = 0.0;
        let mut total = 0usize;
        let ips = systemless::runner::DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND;

        total += App::cpu_budget_for_duration(
            FRAME_DURATION.saturating_sub(MIN_RENDER_HEADROOM),
            ips,
            &mut credit,
        );
        for _ in 1..120 {
            total += App::cpu_budget_for_duration(FRAME_DURATION, ips, &mut credit);
        }

        let total_duration = FRAME_DURATION
            .saturating_sub(MIN_RENDER_HEADROOM)
            .as_secs_f64()
            + FRAME_DURATION.as_secs_f64() * 119.0;
        let expected = (total_duration * ips).floor() as usize;
        assert_eq!(total, expected);
        assert!(credit >= 0.0);
        assert!(credit < 1.0);
    }

    #[test]
    fn render_headroom_tracks_render_cost_with_bounds() {
        assert_eq!(
            App::next_render_headroom(std::time::Duration::from_micros(200)),
            MIN_RENDER_HEADROOM
        );
        assert_eq!(
            App::next_render_headroom(std::time::Duration::from_micros(3_000)),
            std::time::Duration::from_micros(3_500)
        );
        assert_eq!(
            App::next_render_headroom(std::time::Duration::from_micros(20_000)),
            MAX_RENDER_HEADROOM
        );
    }

    #[test]
    fn frame_scheduler_preserves_cadence_when_on_time_or_slightly_late() {
        let scheduled = std::time::Instant::now();
        let half_frame = std::time::Duration::from_secs_f64(FRAME_DURATION.as_secs_f64() / 2.0);

        let (on_time_target, on_time_dropped) = App::next_frame_target(scheduled, scheduled);
        assert_eq!(on_time_target, scheduled + FRAME_DURATION);
        assert!(!on_time_dropped);

        let (late_target, late_dropped) = App::next_frame_target(scheduled + half_frame, scheduled);
        assert_eq!(late_target, scheduled + FRAME_DURATION);
        assert!(!late_dropped);
    }

    #[test]
    fn frame_scheduler_drops_missed_host_frame_instead_of_catchup_burst() {
        let scheduled = std::time::Instant::now();

        let full_frame_late = scheduled + FRAME_DURATION;
        let (full_frame_target, full_frame_dropped) =
            App::next_frame_target(full_frame_late, scheduled);
        assert_eq!(full_frame_target, full_frame_late + FRAME_DURATION);
        assert!(full_frame_dropped);

        let several_frames_late = scheduled + FRAME_DURATION * 4;
        let (late_target, late_dropped) = App::next_frame_target(several_frames_late, scheduled);
        assert_eq!(late_target, several_frames_late + FRAME_DURATION);
        assert!(late_dropped);
    }

    #[test]
    fn gui_foreground_batches_stay_well_below_one_realtime_vbl() {
        let realtime_instructions_per_tick =
            (systemless::runner::DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND
                / systemless::runner::DEFAULT_VBL_HZ) as usize;

        assert!(
            CPU_BATCH_INSTRUCTIONS <= realtime_instructions_per_tick / 32,
            "GUI batches should yield frequently during heavy drawing and slow HLE startup paths; batch={} vbl_budget={}",
            CPU_BATCH_INSTRUCTIONS,
            realtime_instructions_per_tick
        );
        assert_eq!(
            SOUND_CALLBACK_SLICE_INSTRUCTIONS, CPU_BATCH_INSTRUCTIONS,
            "Sound Manager callback slices should stay aligned with GUI yield cadence"
        );
    }

    #[test]
    fn render_gate_waits_for_guest_tick_unless_forced() {
        let mut app = App::new(PathBuf::from("dummy"), false, None, false);
        app.runner = Some(FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        ));

        assert!(
            app.should_render_frame(),
            "initial forced render should present the first frame"
        );

        let tick = app.runner.as_ref().unwrap().guest_tick();
        app.last_presented_guest_tick = Some(tick);
        app.force_next_render = false;
        assert!(
            !app.should_render_frame(),
            "same guest tick should not present another partial frame"
        );

        app.force_next_render = true;
        assert!(app.should_render_frame(), "host input can force a present");
        app.force_next_render = false;

        app.runner.as_mut().unwrap().force_advance_guest_tick();
        assert!(
            app.should_render_frame(),
            "a new guest tick is a fresh VBL presentation point"
        );
    }

    #[test]
    fn centered_copybits_detection_rejects_fullscreen_and_off_center_blits() {
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
            centered_content_rect_from_copybits(centered, 800, 600),
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
        assert_eq!(
            centered_content_rect_from_copybits(fullscreen, 800, 600),
            None
        );

        let off_center = ScreenCopyBitsRect {
            dst_left: 10,
            dst_right: 650,
            ..centered
        };
        assert_eq!(
            centered_content_rect_from_copybits(off_center, 800, 600),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cached_viewport_must_match_centered_guest_screen_geometry() {
        let valid = CachedContentRect {
            version: 1,
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
        assert!(!valid_cached_content_rect(&off_center));
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
            presentation_content_rect(gameplay, Some((85, 228, 233, 572)), 800, 600),
            ContentRect {
                left: 80,
                top: 85,
                width: 640,
                height: 411,
            }
        );
        assert_eq!(
            presentation_content_rect(gameplay, None, 800, 600),
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
