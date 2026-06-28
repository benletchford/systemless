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

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;

use systemless::display;
use systemless::game;
use systemless::runner::FixtureRunner;

use softbuffer::Surface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;
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
const CPU_BATCH_INSTRUCTIONS: usize = 25_000;
const SOUND_CALLBACK_SLICE_INSTRUCTIONS: usize = CPU_BATCH_INSTRUCTIONS;
const SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME: usize = SOUND_CALLBACK_SLICE_INSTRUCTIONS;
const AUDIO_CALLBACK_CHUNK_SAMPLES: usize = 32;
const DEFAULT_GUI_ARROWS_AS_NUMPAD: bool = false;

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

struct App {
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    surface_size: Option<(u32, u32)>,
    frame_argb: Vec<u32>,
    scaled_row: Vec<u32>,
    runner: Option<FixtureRunner>,
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
}

impl App {
    fn new(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        cpu_mhz: Option<f64>,
        show_menu_bar: bool,
    ) -> Self {
        Self {
            window: None,
            surface: None,
            surface_size: None,
            frame_argb: Vec::new(),
            scaled_row: Vec::new(),
            runner: None,
            game_path,
            initialized: false,
            total_instructions: 0,
            start_time: None,
            next_frame_time: None,
            render_headroom: MIN_RENDER_HEADROOM,
            next_cpu_budget_time: None,
            cpu_instruction_credit: 0.0,
            audio_sample_remainder: 0.0,
            mouse_physical: (0.0, 0.0),
            current_screen_width: INITIAL_SCREEN_WIDTH,
            current_screen_height: INITIAL_SCREEN_HEIGHT,
            frame_count: 0,
            last_presented_guest_tick: None,
            force_next_render: true,
            arrows_as_numpad,
            emulated_ips: cpu_mhz.map(|mhz| mhz * 1_000_000.0),
            show_menu_bar,
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

        let scale_x = size.width as f64 / sw as f64;
        let scale_y = size.height as f64 / sh as f64;
        let scale = scale_x.min(scale_y).max(1.0);

        let mac_x = (px / scale) as i16;
        let mac_y = (py / scale) as i16;
        (mac_y.clamp(0, sh as i16 - 1), mac_x.clamp(0, sw as i16 - 1))
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
        self.initialized = true;
    }

    fn cpu_budget_for_duration(duration: std::time::Duration, ips: f64, credit: &mut f64) -> usize {
        *credit += duration.as_secs_f64() * ips;
        let budget = credit.floor().min(game::MAX_INSTRUCTIONS_PER_FRAME as f64) as usize;
        *credit -= budget as f64;
        budget
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
        let start = *self.start_time.get_or_insert(now);
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
        let audio_samples =
            Self::audio_samples_for_duration(FRAME_DURATION, &mut self.audio_sample_remainder);

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
        let Some(runner) = self.runner.as_ref() else {
            return false;
        };
        runner.is_halted()
            || runner.is_ui_tracking_active()
            || self.last_presented_guest_tick != Some(runner.guest_tick())
    }

    fn render_frame(&mut self) {
        let render_start = std::time::Instant::now();
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
        let buf_w = size.width;
        let buf_h = size.height;

        if buf_w == 0 || buf_h == 0 || game_w == 0 || game_h == 0 {
            return;
        }

        let screen_mode = runner.dispatcher().screen_mode;
        let device_clut = runner.dispatcher().device_clut;
        let cursor = runner.dispatcher().cursor().cloned();
        let mouse_pos = runner.dispatcher().mouse_position();

        let mut frame_argb = std::mem::take(&mut self.frame_argb);
        display::render_screen_argb(runner.bus(), screen_mode, &device_clut, &mut frame_argb);
        if let Some(cursor) = cursor.as_ref() {
            display::render_cursor_argb(&mut frame_argb, game_w, game_h, cursor, mouse_pos);
        }

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
                for (dst_chunk, &pixel) in scaled_row.chunks_exact_mut(scale).zip(src_row.iter()) {
                    dst_chunk.fill(pixel);
                }
                let dst_row_start = row * scale * buf_w as usize;
                for repeat in 0..scale {
                    let dst_offset = dst_row_start + repeat * buf_w as usize;
                    buffer[dst_offset..dst_offset + draw_w].copy_from_slice(&scaled_row);
                }
            }
        }

        self.frame_argb = frame_argb;
        self.scaled_row = scaled_row;
        buffer.present().expect("Failed to present buffer");
        self.last_presented_guest_tick = Some(presented_tick);
        self.force_next_render = false;
        self.render_headroom = Self::next_render_headroom(render_start.elapsed());
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes()
                .with_title("Systemless - Macintosh Emulator")
                .with_inner_size(winit::dpi::LogicalSize::new(
                    INITIAL_SCREEN_WIDTH * SCALE,
                    INITIAL_SCREEN_HEIGHT * SCALE,
                ))
                .with_resizable(true);
            let window_attrs = platform_window_attrs(window_attrs);

            let window = Rc::new(
                event_loop
                    .create_window(window_attrs)
                    .expect("Failed to create window"),
            );

            let context =
                softbuffer::Context::new(window.clone()).expect("Failed to create context");
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

            WindowEvent::Resized(_) => {
                self.force_next_render = true;
            }

            WindowEvent::RedrawRequested => {}
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

        // Step emulation, then render
        self.step_frame();

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
            KeyCode::Enter | KeyCode::NumpadEnter => 13,
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

        assert_eq!(first_steps, SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME);
        assert_eq!(
            reserved_sound_steps,
            SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME
        );
        assert!(
            runner.has_pending_sound_work(),
            "spinning callback should remain pending after the capped reserved slice"
        );

        let second_steps = service_pending_sound_work(
            &mut runner,
            spent_deadline,
            0,
            SOUND_CALLBACK_SLICE_INSTRUCTIONS + first_steps,
            &mut reserved_sound_steps,
        );

        assert_eq!(
            second_steps, None,
            "same-frame reserved sound work should stop at the cap so the GUI event loop can process input"
        );
        assert!(runner.has_pending_sound_work());
        assert!(!runner.is_halted());
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
            CPU_BATCH_INSTRUCTIONS <= realtime_instructions_per_tick / 8,
            "GUI batches should yield several times per realtime VBL during heavy drawing; batch={} vbl_budget={}",
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
}
