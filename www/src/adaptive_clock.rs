// Browser-side PPC clock selection. Guest ticks remain tied to wall time;
// the controller needs both guest progress and worker frame delivery before
// raising the instruction budget available inside each tick.
const WINDOW_MS: f64 = 1_000.0;
const PAUSE_MS: f64 = 5_000.0;
const SLOW_TICKS_PER_SEC: f64 = 55.0;
const HEALTHY_TICKS_PER_SEC: f64 = 58.0;
const UP_STEP_MHZ: u32 = 5;
const DOWN_STEP_MHZ: u32 = 10;
const RECOVERY_WINDOWS_AFTER_BACKOFF: u8 = 2;

pub struct AdaptiveClock {
    min_mhz: u32,
    max_mhz: u32,
    current_mhz: u32,
    window_start_ms: f64,
    window_start_tick: u32,
    window_frames: u32,
    recovery_windows: u8,
}

impl AdaptiveClock {
    pub fn new(min_mhz: u32, max_mhz: u32, now_ms: f64, guest_tick: u32) -> Self {
        assert!(min_mhz < max_mhz);
        Self {
            min_mhz,
            max_mhz,
            current_mhz: min_mhz,
            window_start_ms: now_ms,
            window_start_tick: guest_tick,
            window_frames: 0,
            recovery_windows: 0,
        }
    }

    pub fn current_mhz(&self) -> u32 {
        self.current_mhz
    }

    /// Return a new clock only after a complete progress window. Long browser
    /// pauses do not provide a meaningful measure of CPU capacity. Counting
    /// worker frames catches overload even when guest tick catch-up stays fast.
    pub fn observe(&mut self, now_ms: f64, guest_tick: u32) -> Option<u32> {
        self.window_frames = self.window_frames.saturating_add(1);
        let elapsed_ms = now_ms - self.window_start_ms;
        if !elapsed_ms.is_finite() || elapsed_ms < WINDOW_MS {
            return None;
        }
        let ticks_per_sec =
            guest_tick.wrapping_sub(self.window_start_tick) as f64 * 1_000.0 / elapsed_ms;
        let frames_per_sec = self.window_frames as f64 * 1_000.0 / elapsed_ms;
        self.window_start_ms = now_ms;
        self.window_start_tick = guest_tick;
        self.window_frames = 0;

        if elapsed_ms > PAUSE_MS {
            self.recovery_windows = 0;
            return None;
        }
        let next_mhz = if ticks_per_sec < SLOW_TICKS_PER_SEC || frames_per_sec < SLOW_TICKS_PER_SEC
        {
            self.recovery_windows = RECOVERY_WINDOWS_AFTER_BACKOFF;
            self.current_mhz
                .saturating_sub(DOWN_STEP_MHZ)
                .max(self.min_mhz)
        } else if self.recovery_windows > 0 {
            self.recovery_windows -= 1;
            self.current_mhz
        } else if ticks_per_sec >= HEALTHY_TICKS_PER_SEC && frames_per_sec >= HEALTHY_TICKS_PER_SEC
        {
            self.current_mhz
                .saturating_add(UP_STEP_MHZ)
                .min(self.max_mhz)
        } else {
            self.current_mhz
        };
        if next_mhz == self.current_mhz {
            return None;
        }
        self.current_mhz = next_mhz;
        Some(next_mhz)
    }
}

#[cfg(test)]
mod tests {
    use super::AdaptiveClock;

    fn window(clock: &mut AdaptiveClock, second: u32, start_tick: u32, ticks: u32, frames: u32) {
        for frame in 1..=frames {
            let now_ms = (second - 1) as f64 * 1_000.0 + frame as f64 * 1_000.0 / frames as f64;
            let guest_tick = start_tick + ticks * frame / frames;
            clock.observe(now_ms, guest_tick);
        }
    }

    #[test]
    fn sustained_guest_pacing_raises_clock_but_slow_progress_backs_off() {
        let mut clock = AdaptiveClock::new(25, 60, 0.0, 0);
        for second in 1..=21 {
            window(&mut clock, second, (second - 1) * 60, 60, 60);
        }
        assert_eq!(clock.current_mhz(), 60);
        window(&mut clock, 22, 21 * 60, 35, 60);
        assert_eq!(clock.current_mhz(), 50);
        window(&mut clock, 23, 21 * 60 + 35, 35, 60);
        assert_eq!(clock.current_mhz(), 40);
    }

    #[test]
    fn low_worker_frame_rate_backs_off_despite_healthy_guest_ticks() {
        let mut clock = AdaptiveClock::new(25, 60, 0.0, 0);
        for second in 1..=7 {
            window(&mut clock, second, (second - 1) * 60, 60, 60);
        }
        assert_eq!(clock.current_mhz(), 60);
        window(&mut clock, 8, 7 * 60, 60, 50);
        assert_eq!(clock.current_mhz(), 50);
        window(&mut clock, 9, 8 * 60, 60, 60);
        window(&mut clock, 10, 9 * 60, 60, 60);
        assert_eq!(clock.current_mhz(), 50);
        window(&mut clock, 11, 10 * 60, 60, 60);
        assert_eq!(clock.current_mhz(), 55);
    }

    #[test]
    fn background_pause_does_not_look_like_a_slow_cpu() {
        let mut clock = AdaptiveClock::new(25, 60, 0.0, 0);
        for second in 1..=3 {
            window(&mut clock, second, (second - 1) * 60, 60, 60);
        }
        assert_eq!(clock.current_mhz(), 40);
        assert_eq!(clock.observe(9_000.0, 181), None);
        assert_eq!(clock.current_mhz(), 40);
    }
}
