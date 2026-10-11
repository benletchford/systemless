//! Opt-in worker timing. Measures worker phases, not presented frames or FPS.
use std::time::Instant;
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
}
impl WorkerProfiler {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var_os("SYSTEMLESS_GPUI_PROFILE").is_some(),
            samples: Vec::new(),
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn begin(&self) -> FrameTiming {
        FrameTiming {
            previous: self.enabled.then(Instant::now),
            values: Vec::new(),
        }
    }
    pub fn record(&mut self, timing: FrameTiming) {
        if !self.enabled {
            return;
        }
        assert_eq!(timing.values.len(), STAGES.len());
        self.samples.push(timing.values);
        if self.samples.len() < 120 {
            return;
        }
        let phases: Vec<_> = STAGES.iter().enumerate().map(|(i, name)| {
            let mut values: Vec<_> = self.samples.iter().map(|row| row[i]).collect();
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            values.sort_by(f64::total_cmp);
            serde_json::json!({ "phase": name, "mean_ms": mean, "p95_ms": values[values.len() * 95 / 100], "max_ms": values[values.len() - 1] })
        }).collect();
        eprintln!(
            "[GPUI-PROFILE] {}",
            serde_json::json!({ "worker_cycles": self.samples.len(), "phases": phases, "scope": "worker work only; excludes sleeping, GPUI drawing and native frame presentation" })
        );
        self.samples.clear();
    }
}
