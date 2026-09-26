//! Guest initialization, pacing, sound and saves, independent of host windows.
//! The driver is still called on the event thread during the initial extraction.

use super::runtime_protocol::{CursorWarp, GuiCommand, GuiState};
use super::{
    configure_realtime_execution_rate, debug_server, foreground_cpu_batch_instructions,
    service_pending_sound_work, DesktopSaveStore, FramePhaseTimer, HostMouseReleaseLatch,
    AUDIO_CALLBACK_CHUNK_SAMPLES, FRAME_DURATION, MAX_AUDIO_MIX_INTERVAL, MAX_RENDER_HEADROOM,
    MIN_RENDER_HEADROOM, RENDER_HEADROOM_MARGIN,
};
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use systemless::runner::MenuBarPolicy;
use systemless::{game, runner::FixtureRunner, ui_theme::UiThemeId};

pub(super) struct GuiDriver {
    generation: u64,
    snapshot_sequence: u64,
    warp_serial: u64,
    pending_warp: Option<CursorWarp>,
    #[cfg(target_os = "macos")]
    identity: Option<std::sync::Arc<game::ApplicationIdentity>>,
    display_generation: u64,
    snapshot_screen_mode: Option<(u32, u32, u16, u16, u16)>,
    compact_cache: systemless::memory::CompactPresentationCache,
    compact_snapshot: Option<std::sync::Arc<systemless::memory::CompactPresentation>>,
    pub(super) runner: Option<FixtureRunner>,
    pub(super) debug_server: Option<debug_server::DebugServer>,
    pub(super) save_store: Option<DesktopSaveStore>,
    pub(super) guest_exit_reported: bool,
    pub(super) initialized: bool,
    pub(super) total_instructions: u64,
    /// Wall-clock origin for deriving tick targets.
    pub(super) start_time: Option<std::time::Instant>,
    /// Next frame target for pacing.
    pub(super) next_frame_time: Option<std::time::Instant>,
    /// Adaptive CPU/render split for the single-threaded GUI loop.
    pub(super) render_headroom: std::time::Duration,
    /// Fractional host samples carried between GUI slices to preserve rate.
    pub(super) audio_sample_remainder: f64,
    /// Wall-clock instant represented by the most recently queued audio.
    /// Unlike video, audio cannot simply drop a late host frame without
    /// starving the device ring buffer.
    pub(super) last_audio_mix_time: Option<std::time::Instant>,
    pub(super) mouse_release_latch: HostMouseReleaseLatch,
    /// Frame counter for diagnostic screenshots
    pub(super) frame_count: u64,
    /// Guest tick last presented to the host window.
    pub(super) last_presented_guest_tick: Option<u32>,
    /// Force the next host present even if the guest tick has not advanced.
    pub(super) force_next_render: bool,
    game_path: PathBuf,
    arrows_as_numpad: bool,
    #[cfg(target_os = "macos")]
    native_integrations: bool,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    ui_theme: UiThemeId,
}

impl GuiDriver {
    pub(super) fn new(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        native_integrations: bool,
        addressing_24_bit: bool,
        screen_depth: Option<u16>,
        ui_theme: UiThemeId,
    ) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = native_integrations;
        static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let generation = NEXT_GENERATION
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |value| value.checked_add(1),
            )
            .expect("desktop runtime generation exhausted");
        Self {
            generation,
            snapshot_sequence: 0,
            warp_serial: 0,
            pending_warp: None,
            #[cfg(target_os = "macos")]
            identity: None,
            display_generation: 0,
            snapshot_screen_mode: None,
            compact_cache: Default::default(),
            compact_snapshot: None,
            runner: None,
            debug_server: None,
            save_store: None,
            guest_exit_reported: false,
            initialized: false,
            total_instructions: 0,
            start_time: None,
            next_frame_time: None,
            render_headroom: MIN_RENDER_HEADROOM,
            audio_sample_remainder: 0.0,
            last_audio_mix_time: None,
            mouse_release_latch: HostMouseReleaseLatch::default(),
            frame_count: 0,
            last_presented_guest_tick: None,
            force_next_render: true,
            game_path,
            arrows_as_numpad,
            addressing_24_bit,
            screen_depth,
            ui_theme,
            #[cfg(target_os = "macos")]
            native_integrations,
        }
    }

    pub(super) fn init_game(&mut self) {
        let _timing = FramePhaseTimer::new("guest initialization");
        if self.initialized {
            return;
        }

        let mut runner = match self.screen_depth {
            Some(screen_depth) => {
                game::new_runner_with_configuration(!self.addressing_24_bit, screen_depth)
            }
            None => game::new_runner_with_addressing(!self.addressing_24_bit),
        };
        runner.set_ui_theme(self.ui_theme);
        #[cfg(target_os = "macos")]
        if self.native_integrations {
            runner.set_menu_bar_policy(MenuBarPolicy::ForceHidden);
        }
        let app =
            game::load_game_from_path(&mut runner, &self.game_path).expect("Failed to load game");
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
        runner.prepare_text_presentation();
        runner.set_arrows_as_numpad(self.arrows_as_numpad);

        let ipt = configure_realtime_execution_rate(&mut runner);
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

    pub(super) fn apply_command(&mut self, command: GuiCommand) {
        if let GuiCommand::AcknowledgeWarp { generation, serial } = command {
            if generation == self.generation
                && self.pending_warp.is_some_and(|warp| warp.serial == serial)
            {
                self.pending_warp = None;
            }
            return;
        }
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        self.force_next_render = true;
        Self::apply_guest_command(runner, &mut self.mouse_release_latch, command);
    }

    pub(super) fn apply_guest_command(
        runner: &mut FixtureRunner,
        mouse_release_latch: &mut HostMouseReleaseLatch,
        command: GuiCommand,
    ) {
        match command {
            GuiCommand::MouseMove { v, h } => {
                runner.set_mouse_position(v, h);
                runner.dispatcher_mut().show_cursor();
            }
            GuiCommand::MouseDown { v, h } => {
                runner.push_mouse_down(v, h);
                mouse_release_latch.press();
            }
            GuiCommand::MouseUp { v, h } => {
                if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                    runner.push_mouse_up(v, h);
                }
            }
            GuiCommand::KeyDown { key, character } => runner.push_key_down(key, character),
            GuiCommand::KeyUp { key, character } => runner.push_key_up(key, character),
            GuiCommand::Menu { menu, item } => {
                runner.select_guest_menu_item(menu, item);
            }
            GuiCommand::AcknowledgeWarp { .. } => unreachable!(),
        }
    }

    pub(super) fn capture_state(&mut self, output: &mut GuiState, native_models: bool) {
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if let Some(position) = runner.take_guest_cursor_warp() {
            self.warp_serial = self
                .warp_serial
                .checked_add(1)
                .expect("cursor warp serial exhausted");
            self.pending_warp = Some(CursorWarp {
                serial: self.warp_serial,
                position,
            });
        }
        output.generation = self.generation;
        output.screen_mode = runner.dispatcher().screen_mode;
        output.cursor = runner.dispatcher().cursor().cloned();
        output.warp = self.pending_warp;
        #[cfg(target_os = "macos")]
        {
            use systemless::memory::MemoryBus;
            output.dialog_bounds = runner
                .dispatcher()
                .visible_dialog_structure_bounds(runner.bus());
            output.hidden_menu_height = if self.native_integrations {
                runner
                    .bus()
                    .read_word(systemless::memory::globals::addr::MBAR_HEIGHT)
                    .into()
            } else {
                0
            };
            if native_models && self.native_integrations {
                let path = runner.dispatcher().launched_app_path();
                if self
                    .identity
                    .as_ref()
                    .map(|identity| identity.path.as_str())
                    != path
                {
                    self.identity =
                        game::loaded_application_identity(runner).map(std::sync::Arc::new);
                }
                output.identity = self.identity.clone();
                let menus = runner.guest_menu_snapshot();
                if output.menus.as_ref().is_none_or(|old| **old != menus) {
                    output.menus = Some(std::sync::Arc::new(menus));
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = native_models;
    }

    pub(super) fn pump_debugger(&mut self) {
        if let (Some(server), Some(runner)) = (self.debug_server.as_mut(), self.runner.as_mut()) {
            server.pump(runner);
        }
    }

    pub(super) fn finish_frame(&mut self) {
        if let Some(runner) = self.runner.as_mut() {
            runner.finish_gui_frame();
        }
        self.frame_count += 1;
    }

    pub(super) fn capture_frame(
        &mut self,
        output: &mut super::frame_snapshot::GuiFrame,
        debug: Option<systemless::debug_overlay::DebugOverlayFrameStats>,
        capture_crop: bool,
        learning_crop: bool,
    ) -> bool {
        let Some(runner) = self.runner.as_mut() else {
            return false;
        };
        {
            let _timing = FramePhaseTimer::new("outline palette preparation");
            runner.prepare_text_presentation();
        }
        {
            let _timing = FramePhaseTimer::new("window compositing");
            runner.composite_frame();
        }
        let _timing = FramePhaseTimer::new("owned frame export");
        let mode = runner.dispatcher().screen_mode;
        let learning_crop = learning_crop || self.snapshot_screen_mode != Some(mode);
        if self.snapshot_screen_mode != Some(mode) {
            self.display_generation = self
                .display_generation
                .checked_add(1)
                .expect("desktop display generation exhausted");
            self.snapshot_screen_mode = Some(mode);
        }
        self.snapshot_sequence = self
            .snapshot_sequence
            .checked_add(1)
            .expect("desktop frame sequence exhausted");
        output.generation = self.generation;
        output.sequence = self.snapshot_sequence;
        output.display_generation = self.display_generation;
        output.guest_tick = runner.guest_tick();
        output.screen.capture(
            runner.bus(),
            mode,
            &runner.dispatcher().device_clut,
            &runner.dispatcher().device_gamma(),
        );
        output.cursor = runner.dispatcher().cursor().cloned();
        output.mouse_position = runner.dispatcher().mouse_position();
        output.debug_lines = debug
            .map(|stats| runner.debug_overlay_snapshot(stats).lines())
            .unwrap_or_default();
        if runner.bus().has_visible_outline_detail() {
            match self
                .compact_cache
                .prepare_changed(runner.bus(), (mode.2.into(), mode.3.into()))
            {
                Some(changed) => {
                    if changed || self.compact_snapshot.is_none() {
                        self.compact_snapshot =
                            Some(std::sync::Arc::new(self.compact_cache.frame().clone()));
                    }
                    output.retained = self.compact_snapshot.clone();
                }
                None => {
                    output.retained = None;
                    self.compact_snapshot = None;
                }
            }
        } else {
            output.retained = None;
            self.compact_snapshot = None;
        }
        #[cfg(target_os = "macos")]
        {
            use systemless::memory::MemoryBus;
            let dispatcher = runner.dispatcher();
            output.crop = super::frame_snapshot::CropObservations {
                dialog_bounds: dispatcher.visible_dialog_structure_bounds(runner.bus()),
                framed_rect: capture_crop
                    .then(|| dispatcher.framed_manual_cport_presentation_rect(runner.bus()))
                    .flatten(),
                manual_rect: (capture_crop && learning_crop)
                    .then(|| dispatcher.manual_cport_presentation_rect(runner.bus()))
                    .flatten(),
                declared_rect: (capture_crop && learning_crop)
                    .then(|| dispatcher.declared_centered_presentation_rect(runner.bus()))
                    .flatten(),
                copybits_count: dispatcher.copybits_screen_count,
                last_copybits_rect: dispatcher.last_screen_copybits_rect,
                hidden_menu_height: if self.native_integrations {
                    runner
                        .bus()
                        .read_word(systemless::memory::globals::addr::MBAR_HEIGHT)
                        .into()
                } else {
                    0
                },
            };
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (capture_crop, learning_crop);
        true
    }

    pub(super) fn sync_save_files(&mut self, force: bool) {
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

    pub(super) fn guest_requested_exit(&self) -> bool {
        self.runner
            .as_ref()
            .is_some_and(FixtureRunner::halted_by_exit_to_shell)
    }

    /// Wall-clock origin such that `tick_due_at(origin, now)` equals `guest_tick`.
    /// Shifts the origin back so a boot-seeded, non-zero TickCount does not make
    /// the pacer wait real time before running any guest CPU work.
    pub(super) fn wall_clock_origin_for_guest_tick(
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

    pub(super) fn tick_due_at(origin: std::time::Instant, at: std::time::Instant) -> u32 {
        at.checked_duration_since(origin)
            .unwrap_or_default()
            .as_secs_f64()
            .mul_add(systemless::runner::DEFAULT_VBL_HZ, 0.0)
            .floor() as u32
    }

    pub(super) fn audio_samples_for_duration(
        duration: std::time::Duration,
        remainder: &mut f64,
    ) -> usize {
        let total_samples = duration
            .as_secs_f64()
            .mul_add(systemless::sound::OUTPUT_RATE as f64, *remainder);
        let whole_samples = total_samples.floor();
        *remainder = total_samples - whole_samples;
        whole_samples as usize
    }

    pub(super) fn next_render_headroom(render_time: std::time::Duration) -> std::time::Duration {
        let target = render_time.saturating_add(RENDER_HEADROOM_MARGIN);
        target.clamp(MIN_RENDER_HEADROOM, MAX_RENDER_HEADROOM)
    }

    pub(super) fn next_frame_target(
        now: std::time::Instant,
        scheduled: std::time::Instant,
    ) -> (std::time::Instant, bool) {
        if now.saturating_duration_since(scheduled) >= FRAME_DURATION {
            (now + FRAME_DURATION, true)
        } else {
            (scheduled + FRAME_DURATION, false)
        }
    }

    pub(super) fn flush_ready_mouse_release(&mut self) {
        let Some((v, h)) = self.mouse_release_latch.take_ready_release() else {
            return;
        };
        if let Some(runner) = self.runner.as_mut() {
            runner.push_mouse_up(v, h);
        }
    }

    pub(super) fn step_frame(&mut self) {
        self.step_frame_with_clock(std::time::Instant::now);
    }

    pub(super) fn step_frame_with_clock(&mut self, host_now: impl FnMut() -> std::time::Instant) {
        self.step_frame_with_safe_points(host_now, |_, _| true);
    }

    /// The callback runs only between complete guest CPU/audio batches. A
    /// false result requests shutdown; it cannot preempt a long Toolbox call.
    pub(super) fn step_frame_with_safe_points(
        &mut self,
        mut host_now: impl FnMut() -> std::time::Instant,
        mut safe_point: impl FnMut(&mut FixtureRunner, &mut HostMouseReleaseLatch) -> bool,
    ) {
        let _timing = FramePhaseTimer::new("CPU and audio frame");
        let Some(runner) = self.runner.as_ref() else {
            return;
        };

        if runner.is_halted() {
            return;
        }

        let now = host_now();
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
        // Host input wakes the foreground application even when its TickCount
        // is ahead of the wall-clock target. Give each mouse transition one
        // bounded guest slice so a polling loop cannot be starved by pacing.
        let input_progress_ticks = u32::from(self.mouse_release_latch.requires_guest_progress());
        let effective_target =
            current_tick.saturating_add(ticks_behind.min(2).max(input_progress_ticks));

        // CPU budget: wall-clock time left in this frame, minus render headroom.
        // The CPU runs in small batches, checking the clock between batches.
        let cpu_deadline = scheduled_frame_end
            .checked_sub(self.render_headroom)
            .map(|d| d.max(now))
            .unwrap_or(now);

        let slice_budget = game::MAX_INSTRUCTIONS_PER_FRAME;
        let presentation_interval = self
            .last_audio_mix_time
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or(FRAME_DURATION);
        let audio_interval = presentation_interval.min(MAX_AUDIO_MIX_INTERVAL);
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
        runner.advance_menu_presentation_clock(presentation_interval);
        // A PPC HLE slice currently borrows its large mutable state by moving
        // collections into a dispatch closure and restoring them afterward.
        // Yield a few times per guest VBL rather than paying that boundary
        // thousands of times per second, so the wall-clock CPU deadline is
        // still rechecked within a tick. The interpreter stops at the tick cap.
        let foreground_batch_instructions = foreground_cpu_batch_instructions(
            runner.is_powerpc_app(),
            runner.instructions_per_tick(),
        );

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

        let mut cancelled = false;
        loop {
            if !safe_point(runner, &mut self.mouse_release_latch) {
                cancelled = true;
                break;
            }
            if runner.guest_tick() >= effective_target || runner.is_halted() {
                break;
            }
            if host_now() >= cpu_deadline {
                break;
            }

            let remaining = slice_budget.saturating_sub(total_steps);
            if remaining == 0 {
                break;
            }

            let batch_size = remaining.min(foreground_batch_instructions);
            let remaining_audio = audio_samples.saturating_sub(audio_mixed);
            let batches_left = remaining.div_ceil(foreground_batch_instructions).max(1);
            let batch_audio = if remaining_audio == 0 {
                0
            } else {
                remaining_audio.div_ceil(batches_left)
            };
            let (steps, running) = {
                let _timing = FramePhaseTimer::new("foreground CPU batch");
                runner.run_gui_cpu_slice(batch_size, effective_target)
            };
            total_steps += steps;
            foreground_steps += steps;
            audio_mixed += batch_audio;
            if batch_audio > 0 {
                // CPU batches share one presentation pass in render_frame.
                // Keep audio callbacks serviced without repainting every window.
                runner.mix_gui_audio_slice(batch_audio);
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
            if !running || runner.is_ui_tracking_active() {
                break;
            }
        }

        if !cancelled && audio_mixed < audio_samples {
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

        if !cancelled && audio_mixed < audio_samples {
            let mut remaining_audio = audio_samples - audio_mixed;
            while remaining_audio > 0 && !runner.is_halted() {
                if !safe_point(runner, &mut self.mouse_release_latch) {
                    cancelled = true;
                    break;
                }
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

        if !cancelled {
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

        self.total_instructions += total_steps as u64;
        if foreground_steps > 0 {
            self.mouse_release_latch.observe_guest_progress();
        }
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

    pub(super) fn should_render_frame(&self, debug_overlay_visible: bool) -> bool {
        if self.force_next_render {
            return true;
        }
        if debug_overlay_visible {
            return true;
        }
        let Some(runner) = self.runner.as_ref() else {
            return false;
        };
        runner.is_halted()
            || runner.is_ui_tracking_active()
            || self.last_presented_guest_tick != Some(runner.guest_tick())
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::frame_snapshot::GuiFrame;
    use systemless::memory::MemoryBus;

    fn driver() -> GuiDriver {
        let mut driver = GuiDriver::new(
            "snapshot-test".into(),
            false,
            false,
            false,
            Some(8),
            UiThemeId::ClassicSystem7,
        );
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, Default::default());
        let address = runner.bus_mut().alloc(32 * 32);
        runner.dispatcher_mut().screen_mode = (address, 32, 32, 32, 8);
        runner.bus_mut().fill_bytes(address, 32 * 32, 1);
        driver.runner = Some(runner);
        driver
    }

    #[test]
    fn complete_frames_survive_frozen_ticks_guest_changes_and_owner_destruction() {
        let mut driver = driver();
        let mut first = GuiFrame::default();
        assert!(driver.capture_frame(&mut first, None, false, false));
        let original_bytes = first.screen.pixels.clone();
        let original_palette = first.screen.palette;
        let runner = driver.runner.as_mut().unwrap();
        let address = runner.dispatcher().screen_mode.0;
        runner.bus_mut().fill_bytes(address, 32 * 32, 2);
        runner.dispatcher_mut().set_mouse_position(19, 23);
        let mut second = GuiFrame::default();
        assert!(driver.capture_frame(&mut second, None, false, false));
        assert_eq!(first.guest_tick, second.guest_tick);
        assert_eq!(first.generation, second.generation);
        assert_eq!(second.sequence, first.sequence + 1);
        assert_eq!(first.display_generation, second.display_generation);
        assert_eq!(second.mouse_position, (19, 23));
        assert_eq!(first.screen.palette, second.screen.palette);
        assert_ne!(first.screen.pixels, second.screen.pixels);
        driver
            .runner
            .as_mut()
            .unwrap()
            .dispatcher_mut()
            .screen_mode
            .2 = 16;
        let mut third = GuiFrame::default();
        assert!(driver.capture_frame(&mut third, None, false, false));
        assert_eq!(third.display_generation, second.display_generation + 1);
        assert_eq!(third.sequence, second.sequence + 1);
        drop(driver);
        assert_eq!(first.screen.pixels, original_bytes);
        assert_eq!(first.screen.palette, original_palette);
        let mut pixels = Vec::new();
        first.screen.render_argb(&mut pixels);
        assert_eq!(pixels.len(), 32 * 32);
    }

    #[test]
    fn runtime_generations_do_not_reuse_frame_identifiers() {
        let mut first = driver();
        let mut second = driver();
        let mut a = GuiFrame::default();
        let mut b = GuiFrame::default();
        assert!(first.capture_frame(&mut a, None, false, false));
        assert!(second.capture_frame(&mut b, None, false, false));
        assert_eq!(a.sequence, b.sequence);
        assert_ne!(a.generation, b.generation);
    }
    fn guest_warp(driver: &mut GuiDriver, position: (i16, i16)) {
        use systemless::cpu::Register;
        use systemless::memory::globals::addr;
        let runner = driver.runner.as_mut().unwrap();
        runner.bus_mut().write_word(0x10000, 0x21fc); // MOVE.L #point,Mouse.W
        runner.bus_mut().write_long(
            0x10002,
            (u32::from(position.0 as u16) << 16) | u32::from(position.1 as u16),
        );
        runner
            .bus_mut()
            .write_word(0x10006, addr::MOUSE_LOC2 as u16);
        runner.cpu_mut().write_reg(Register::PC, 0x10000);
        runner.run_steps(1, None);
    }

    #[test]
    fn cursor_warps_survive_replaced_snapshots_until_matching_acknowledgement() {
        let mut driver = driver();
        guest_warp(&mut driver, (11, 13));
        let mut state = GuiState::default();
        driver.capture_state(&mut state, false);
        let first = state.warp.expect("guest instruction should request a warp");
        assert_eq!(first.position, (11, 13));
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(first));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation + 1,
            serial: first.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(first));
        guest_warp(&mut driver, (17, 19));
        driver.capture_state(&mut state, false);
        let second = state.warp.unwrap();
        assert!(second.serial > first.serial);
        assert_eq!(second.position, (17, 19));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation,
            serial: first.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(second));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation,
            serial: second.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, None);
    }

    #[test]
    fn owned_mouse_commands_preserve_delayed_release_coordinates() {
        let mut driver = driver();
        driver.apply_command(GuiCommand::MouseDown { v: 3, h: 5 });
        driver.apply_command(GuiCommand::MouseUp { v: 7, h: 11 });
        driver.apply_command(GuiCommand::MouseMove { v: 13, h: 17 });
        assert_eq!(driver.mouse_release_latch.pending_release, Some((7, 11)));
        driver.mouse_release_latch.observe_guest_progress();
        driver.flush_ready_mouse_release();
        assert_eq!(driver.mouse_release_latch.pending_release, None);
        assert_eq!(
            driver
                .runner
                .as_ref()
                .unwrap()
                .dispatcher()
                .mouse_position(),
            (7, 11)
        );
    }
}
