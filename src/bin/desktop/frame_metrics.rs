//! Bounded, opt-in host phase measurements. These describe host costs, not
//! guest progress; compare them alongside deterministic tick/instruction totals.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const SAMPLES_PER_REPORT: usize = 600;
static SAMPLES: OnceLock<Mutex<BTreeMap<&'static str, Vec<f64>>>> = OnceLock::new();

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_MEASURE_FRAMES").is_some())
}

pub fn record(phase: &'static str, elapsed: Duration) {
    let report = {
        let Ok(mut phases) = SAMPLES.get_or_init(Mutex::default).lock() else {
            return;
        };
        let samples = phases
            .entry(phase)
            .or_insert_with(|| Vec::with_capacity(SAMPLES_PER_REPORT));
        samples.push(elapsed.as_secs_f64() * 1000.0);
        if samples.len() < SAMPLES_PER_REPORT {
            return;
        }
        std::mem::replace(samples, Vec::with_capacity(SAMPLES_PER_REPORT))
    };
    report_samples(phase, report);
}

/// Emit partial batches on normal host shutdown without holding the lock.
pub fn flush() {
    let Some(samples) = SAMPLES.get() else { return };
    let batches = {
        let Ok(mut samples) = samples.lock() else {
            return;
        };
        std::mem::take(&mut *samples)
    };
    for (phase, samples) in batches {
        if !samples.is_empty() {
            report_samples(phase, samples);
        }
    }
}

fn report_samples(phase: &str, mut report: Vec<f64>) {
    // Sorting and logging never hold the sample lock.
    report.sort_unstable_by(f64::total_cmp);
    eprintln!(
        "[FRAME-METRICS] phase={phase:?} n={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3}",
        report.len(),
        percentile(&report, 50),
        percentile(&report, 95),
        percentile(&report, 99),
        report.last().copied().unwrap_or_default(),
    );
}

fn percentile(sorted: &[f64], percent: usize) -> f64 {
    sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank_including_tail_stalls() {
        let samples: Vec<_> = (1..=600).map(f64::from).collect();
        assert_eq!(percentile(&samples, 50), 300.0);
        assert_eq!(percentile(&samples, 95), 570.0);
        assert_eq!(percentile(&samples, 99), 594.0);
        assert_eq!(percentile(&[2.0], 99), 2.0);
    }
}
