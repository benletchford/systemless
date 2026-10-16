// Shared desktop execution policy; included by the native and comparison hosts.
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

const MAX_AUDIO_MIX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
