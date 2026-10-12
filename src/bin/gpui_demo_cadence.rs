//! Guest worker cadence, without catch-up bursts after a late update.
use std::time::{Duration, Instant};

pub(crate) fn sleep_worker(duration: Duration) {
    #[cfg(target_os = "macos")]
    {
        // Long host sleeps can return ~8 ms late even at interactive QoS.
        // Recheck an absolute deadline between progressively shorter sleeps.
        // Every iteration sleeps; there is no busy wait or early publication.
        let Some(deadline) = Instant::now().checked_add(duration) else {
            std::thread::sleep(duration);
            return;
        };
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            std::thread::sleep((remaining / 2).max(Duration::from_nanos(1)));
        }
    }
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(duration);
}

// Use the same rate for worker deadlines and the elapsed-time guest clock.
// A fixed 16 ms period publishes 62.5 updates per second against a 60 Hz clock,
// periodically scheduling an update before the next guest tick is due.
pub(crate) const GUEST_TICKS_PER_SECOND: f64 = 60.;

pub(crate) fn worker_period() -> Duration {
    Duration::from_secs_f64(1. / GUEST_TICKS_PER_SECOND)
}

pub(crate) struct Cadence {
    next: Instant,
    period: Duration,
}

impl Cadence {
    pub fn new(now: Instant, period: Duration) -> Self {
        assert!(!period.is_zero());
        Self {
            next: now + period,
            period,
        }
    }

    pub fn delay(&mut self, now: Instant) -> Duration {
        if now > self.next {
            // Retain the original phase and skip expired intervals. The period
            // can be fractional milliseconds; use u128 arithmetic even after
            // a prolonged host suspend.
            let remainder = now.duration_since(self.next).as_nanos() % self.period.as_nanos();
            let delay = self.period
                - Duration::new(
                    (remainder / 1_000_000_000) as u64,
                    (remainder % 1_000_000_000) as u32,
                );
            self.next = now + delay;
        }
        let delay = self.next.saturating_duration_since(now);
        self.next += self.period;
        delay
    }

    /// Deliver completed work on a stable phase. Overruns publish immediately;
    /// advance to the next future boundary without replaying missed frames.
    pub fn publication_delay(&mut self, now: Instant) -> Duration {
        if now >= self.next {
            let remainder = now.duration_since(self.next).as_nanos() % self.period.as_nanos();
            let remaining = self.period - Duration::new(
                (remainder / 1_000_000_000) as u64,
                (remainder % 1_000_000_000) as u32,
            );
            self.next = now + remaining;
            Duration::ZERO
        } else {
            let delay = self.next.duration_since(now);
            self.next += self.period;
            delay
        }
    }

    pub fn worker_delay(&mut self, now: Instant) -> Duration {
        // A worker that finishes slightly late has already delivered its
        // update. Do not add a whole idle interval to that overrun. Recover
        // one deadline immediately; the next deadline remains in the future.
        // A long stall still skips expired intervals instead of bursting.
        if now >= self.next && now.duration_since(self.next) < self.period {
            self.next += self.period;
            Duration::ZERO
        } else {
            self.delay(now)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_work_finishes_at_stable_publication_boundaries() {
        let start = Instant::now(); let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16)); let mut now = start;
        for (index, work) in [1, 12, 2, 14, 1, 8].into_iter().cycle().take(120).enumerate() {
            now += ms(work);
            now += cadence.publication_delay(now);
            assert_eq!(now.duration_since(start), ms((index as u64 + 1) * 16));
        }
    }

    #[test]
    fn publication_overruns_are_immediate_then_recover_without_bursts() {
        let start = Instant::now(); let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16));
        assert_eq!(cadence.publication_delay(start + ms(17)), Duration::ZERO);
        assert_eq!(cadence.publication_delay(start + ms(18)), ms(14));
        assert_eq!(cadence.publication_delay(start + ms(101)), Duration::ZERO);
        assert_eq!(cadence.publication_delay(start + ms(102)), ms(10));
        assert_eq!(cadence.publication_delay(start + ms(112)), ms(16));
    }

    #[test]
    fn worker_deadlines_follow_each_guest_tick_without_duplicate_ticks() {
        let start = Instant::now();
        let mut cadence = Cadence::new(start, worker_period());
        let mut now = start;
        for tick in 1..=600 {
            now += cadence.worker_delay(now);
            let due = (now.duration_since(start).as_secs_f64()
                * GUEST_TICKS_PER_SECOND) as u32;
            assert_eq!(due, tick, "a worker deadline must reach the next guest tick");
        }
        assert!(now.duration_since(start).abs_diff(Duration::from_secs(10))
            < Duration::from_micros(1));
    }

    #[test]
    fn worker_recovers_small_overrun_without_an_extra_idle_interval() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16));
        assert_eq!(cadence.worker_delay(start + ms(17)), Duration::ZERO);
        assert_eq!(cadence.worker_delay(start + ms(18)), ms(14));
        assert_eq!(cadence.worker_delay(start + ms(33)), ms(15));
    }

    #[test]
    fn worker_long_stall_skips_deadlines_without_catch_up_bursts() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16));
        assert_eq!(cadence.worker_delay(start + ms(101)), ms(11));
        assert_eq!(cadence.worker_delay(start + ms(112)), ms(16));
    }

    #[test]
    fn repeated_late_wakeups_recover_the_original_phase() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16));
        let mut now = start;
        for _ in 0..100 {
            // A timer wakes 5 ms late and the next cycle does 1 ms of work.
            // Rebased sleeps would accumulate all 600 ms of that delay.
            now += cadence.delay(now) + ms(5) + ms(1);
        }
        assert_eq!(now.duration_since(start), ms(1606));
    }

    #[test]
    fn update_work_and_host_stalls_do_not_accumulate_or_burst() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut cadence = Cadence::new(start, ms(16));
        assert_eq!(cadence.delay(start), ms(16));
        // First update takes 4 ms: the next deadline remains 32 ms, not 36 ms.
        assert_eq!(cadence.delay(start + ms(20)), ms(12));
        // A long stall skips expired deliveries and keeps the original phase.
        assert_eq!(cadence.delay(start + ms(101)), ms(11));
        assert_eq!(cadence.delay(start + ms(116)), ms(12));
        assert_eq!(cadence.delay(start + ms(144)), Duration::ZERO);
        assert_eq!(cadence.delay(start + ms(144)), ms(16));
    }
}
