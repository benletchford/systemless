//! Host-time audio pacing, independent of worker publication frequency.
use std::time::{Duration, Instant};

pub(crate) struct AudioClock {
    previous: Instant,
    fraction: u128,
}

impl AudioClock {
    pub fn new(now: Instant) -> Self {
        // Prime one nominal worker interval, then follow elapsed host time.
        Self {
            previous: now - Duration::from_millis(16),
            fraction: 0,
        }
    }

    pub fn samples(&mut self, now: Instant) -> usize {
        // Do not queue seconds of stale sound after host suspend. Discard
        // elapsed time beyond this bound rather than carrying a catch-up debt.
        let elapsed = now
            .duration_since(self.previous)
            .min(Duration::from_millis(100));
        self.previous = now;
        let units = elapsed.as_nanos() * u128::from(systemless::sound::OUTPUT_RATE) + self.fraction;
        self.fraction = units % 1_000_000_000;
        (units / 1_000_000_000) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_audio_rate_is_exact_across_worker_cadences() {
        for period_ms in [16, 20, 33] {
            let start = Instant::now();
            let mut clock = AudioClock::new(start);
            let mut total = clock.samples(start);
            let mut elapsed = 0;
            while elapsed < 60_000 {
                elapsed = (elapsed + period_ms).min(60_000);
                total += clock.samples(start + Duration::from_millis(elapsed));
            }
            assert_eq!(
                total,
                (60_016 * u64::from(systemless::sound::OUTPUT_RATE) / 1000) as usize
            );
        }
    }

    #[test]
    fn suspend_does_not_leave_audio_catch_up_debt() {
        let start = Instant::now();
        let mut clock = AudioClock::new(start);
        let _ = clock.samples(start);
        assert_eq!(clock.samples(start + Duration::from_secs(30)), 2205);
        assert_eq!(clock.samples(start + Duration::from_secs(30)), 0);
        assert_eq!(clock.samples(start + Duration::from_millis(30_016)), 353);
    }
}
