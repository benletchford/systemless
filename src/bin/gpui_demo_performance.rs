//! Opt-in worker timing. Measures worker phases, not presented frames or FPS.
use std::time::{Duration, Instant};
const STAGES: [&str; 7] = [
    "commands",
    "guest_execution",
    "audio_and_persistence",
    "video_frame",
    "ui_snapshots",
    "paint_qualification",
    "frame_conversion_and_publish",
];
pub(crate) struct FrameTiming {
    previous: Option<Instant>,
    values: Vec<f64>,
    cycle_interval_ms: Option<f64>,
}
impl FrameTiming {
    pub fn checkpoint(&mut self) {
        if let Some(previous) = self.previous.as_mut() {
            let now = Instant::now();
            self.values
                .push(now.duration_since(*previous).as_secs_f64() * 1000.);
            *previous = now;
        }
    }
}
pub(crate) struct WorkerProfiler {
    enabled: bool,
    samples: Vec<Vec<f64>>,
    previous_cycle: Option<Instant>,
    cycle_intervals_ms: Vec<f64>,
    previous_phases: Vec<f64>,
    previous_wait: Option<(f64, f64)>,
    longest_interval: Option<(f64, Vec<f64>, Option<(f64, f64)>)>,
}
impl WorkerProfiler {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var_os("SYSTEMLESS_GPUI_PROFILE").is_some(),
            samples: Vec::new(),
            previous_cycle: None,
            cycle_intervals_ms: Vec::new(),
            previous_phases: Vec::new(),
            previous_wait: None,
            longest_interval: None,
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn sleep(&mut self, requested: Duration) {
        if !self.enabled {
            std::thread::sleep(requested);
            return;
        }
        let start = Instant::now();
        std::thread::sleep(requested);
        self.previous_wait = Some((requested.as_secs_f64() * 1000., start.elapsed().as_secs_f64() * 1000.));
    }
    pub fn begin(&mut self) -> FrameTiming {
        let now = self.enabled.then(Instant::now);
        let cycle_interval_ms = now.and_then(|now| {
            self.previous_cycle
                .replace(now)
                .map(|previous| now.duration_since(previous).as_secs_f64() * 1000.)
        });
        if let Some(interval) = cycle_interval_ms {
            if self.longest_interval.as_ref().is_none_or(|(longest, _, _)| interval > *longest) {
                self.longest_interval = Some((interval, self.previous_phases.clone(), self.previous_wait));
            }
        }
        FrameTiming {
            previous: now,
            values: Vec::new(),
            cycle_interval_ms,
        }
    }
    pub fn record(&mut self, timing: FrameTiming) {
        if !self.enabled {
            return;
        }
        assert_eq!(timing.values.len(), STAGES.len());
        self.previous_phases.clone_from(&timing.values);
        self.samples.push(timing.values);
        if let Some(interval) = timing.cycle_interval_ms {
            self.cycle_intervals_ms.push(interval);
        }
        if self.samples.len() < 120 {
            return;
        }
        let phases: Vec<_> = STAGES.iter().enumerate().map(|(i, name)| {
            let mut values: Vec<_> = self.samples.iter().map(|row| row[i]).collect();
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            values.sort_by(f64::total_cmp);
            serde_json::json!({ "phase": name, "mean_ms": mean, "p95_ms": values[values.len() * 95 / 100], "max_ms": values[values.len() - 1] })
        }).collect();
        // Consecutive cycle starts include sleeping and scheduling delay. These
        // are production intervals, not compositor submission/presentation times.
        self.cycle_intervals_ms.sort_by(f64::total_cmp);
        let intervals = &self.cycle_intervals_ms;
        let cadence = serde_json::json!({
            "samples": intervals.len(),
            "mean_ms": intervals.iter().sum::<f64>() / intervals.len() as f64,
            "p95_ms": intervals[intervals.len() * 95 / 100],
            "max_ms": intervals[intervals.len() - 1],
        });
        // An interval ends at this cycle's start, so its work belongs to the
        // preceding cycle, including across the 120-sample reporting boundary.
        let longest = self.longest_interval.as_ref().map(|(interval, phases, wait)| {
            let work_ms = phases.iter().sum::<f64>();
            serde_json::json!({ "interval_ms": interval, "preceding_work_ms": work_ms,
                "outside_measured_work_ms": (interval - work_ms).max(0.),
                "preceding_sleep": wait.map(|(requested, elapsed)| serde_json::json!({
                    "requested_ms": requested, "elapsed_ms": elapsed,
                    "overshoot_ms": (elapsed - requested).max(0.) })),
                "preceding_phases": STAGES.iter().zip(phases).map(|(name, value)|
                    serde_json::json!({ "phase": name, "elapsed_ms": value })).collect::<Vec<_>>(),
                "scope": "Outside work includes intended sleep, timer/scheduler delay and profiler overhead; phase elapsed time includes preemption. Not CPU utilization or native presentation." })
        });
        eprintln!(
            "[GPUI-PROFILE] {}",
            serde_json::json!({ "worker_cycles": self.samples.len(), "phases": phases,
                "worker_cycle_intervals": cadence,
                "longest_interval_attribution": longest,
                "scope": "phases measure worker work only; cycle intervals include sleep and scheduling; neither measures native frame presentation" })
        );
        self.samples.clear();
        self.cycle_intervals_ms.clear();
        self.longest_interval = None;
    }
}
