//! Fixed frontend polling cadence, without catch-up bursts after a late update.
use std::time::{Duration, Instant};

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
            // is 16 ms; use u128 arithmetic even after a prolonged host suspend.
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
