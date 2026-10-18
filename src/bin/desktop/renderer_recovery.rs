//! Device health and bounded desktop recovery; guest state stays with its owner.
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Preserve hardware/driver identity alongside qualification logs, even if
/// presenter creation subsequently falls back and releases the adapter.
pub(super) fn report_adapter(info: &wgpu::AdapterInfo) {
    eprintln!(
        "[GPU-ADAPTER] {}",
        serde_json::json!({
            "name": info.name, "backend": format!("{:?}", info.backend),
            "device_type": format!("{:?}", info.device_type),
            "vendor": info.vendor, "device": info.device,
            "driver": info.driver, "driver_info": info.driver_info,
        })
    );
}

#[derive(Clone, Debug)]
pub(super) struct Failure {
    pub recoverable: bool,
    pub message: String,
}
impl Failure {
    pub fn lost(message: impl Into<String>) -> Self {
        Self {
            recoverable: true,
            message: message.into(),
        }
    }
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            recoverable: false,
            message,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub(super) struct Health {
    failed: AtomicBool,
    failure: Mutex<Option<Failure>>,
    wake: Option<winit::event_loop::EventLoopProxy<()>>,
}
impl Health {
    pub fn install(
        device: &wgpu::Device,
        wake: Option<winit::event_loop::EventLoopProxy<()>>,
    ) -> Arc<Self> {
        let health = Arc::new(Self {
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
            wake,
        });
        let lost = health.clone();
        device.set_device_lost_callback(move |reason, message| {
            // Dropping/replacing an owner intentionally destroys its device.
            if reason != wgpu::DeviceLostReason::Destroyed {
                lost.record(Failure::lost(format!("wgpu device lost: {message}")));
            }
        });
        let errors = health.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            errors.record(Failure::from(format!("wgpu error: {error}")));
        }));
        health
    }
    pub(super) fn record(&self, error: Failure) {
        let mut failure = self.failure.lock().unwrap_or_else(|e| e.into_inner());
        // Preserve a terminal error if a subsequent loss notification arrives.
        if failure
            .as_ref()
            .is_none_or(|old| old.recoverable && !error.recoverable)
        {
            *failure = Some(error);
        }
        self.failed.store(true, Ordering::Release);
        drop(failure);
        if let Some(wake) = &self.wake {
            let _ = wake.send_event(());
        }
    }
    pub fn check(&self) -> Result<(), Failure> {
        if !self.failed.load(Ordering::Acquire) {
            return Ok(());
        }
        Err(self
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .unwrap()
            .clone())
    }
}

#[derive(Default)]
pub(super) struct Recovery {
    pub rebuilds: u8,
    pub fallback: Option<String>,
}
impl Recovery {
    fn rebuild(&mut self, error: &Failure) -> bool {
        // A session budget, not a consecutive-failure budget: even a device
        // which fails after each successful frame cannot loop forever.
        if error.recoverable && self.rebuilds < 2 {
            self.rebuilds += 1;
            true
        } else {
            false
        }
    }
}

impl App {
    pub(super) fn create_wgpu_presenter(&self) -> Result<wgpu_present::WgpuPresenter, String> {
        #[cfg(feature = "test-support")]
        if std::env::var_os("SYSTEMLESS_NATIVE_WINDOW_PROBE").is_some() {
            let fault = std::env::var("SYSTEMLESS_GPU_FAULT").unwrap_or_default();
            if fault == "startup" || (fault == "recreate" && self.wgpu_recovery.rebuilds > 0) {
                return Err("qualification: injected presenter creation failure".to_owned());
            }
        }
        wgpu_present::WgpuPresenter::new(
            self.window.as_ref().ok_or("window unavailable")?.clone(),
            self.wgpu_display
                .as_ref()
                .ok_or("display unavailable")?
                .clone(),
            self.wgpu_wake.clone(),
        )
    }
    pub(super) fn create_platform_presenter(&mut self) -> Result<(), String> {
        let window = self.window.as_ref().ok_or("window unavailable")?.clone();
        #[cfg(target_os = "macos")]
        {
            self.surface = Some(metal_present::MetalPresenter::new(window)?);
        }
        #[cfg(not(target_os = "macos"))]
        {
            #[cfg(target_os = "windows")]
            if std::env::var_os("SYSTEMLESS_D3D11").as_deref() != Some(std::ffi::OsStr::new("0")) {
                match d3d_present::D3dPresenter::new(
                    window.clone(),
                    self.gpu_wake
                        .as_ref()
                        .ok_or("display wake unavailable")?
                        .clone(),
                ) {
                    Ok(gpu) => {
                        self.gpu = Some(gpu);
                        return Ok(());
                    }
                    Err(error) => eprintln!("[GPU] {error}; using software presentation"),
                }
            }
            let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            self.surface = Some(Surface::new(&context, window).map_err(|e| e.to_string())?);
            self.surface_size = None;
        }
        Ok(())
    }
    pub(super) fn stop_renderer(&mut self, error: String) {
        eprintln!("[GPU] {error}");
        self.runtime_error = Some(error);
        self.closing = true;
        if let Some(owner) = &self.owner {
            owner.mailbox.request_shutdown();
        }
        if let Some(mut driver) = self.driver.take() {
            driver.sync_save_files(true);
        }
        self.show_runtime_error();
    }
    pub(super) fn recover_wgpu(&mut self, error: Failure) {
        // Destroy the old surface before a replacement claims the native view.
        self.wgpu = None;
        self.last_presented_guest_tick = None;
        self.force_next_render = true;
        #[cfg(target_os = "macos")]
        {
            self.force_gpu_present = true;
        }
        if self.wgpu_recovery.rebuild(&error) {
            match self.create_wgpu_presenter() {
                Ok(gpu) => {
                    eprintln!(
                        "[GPU] {}; recreated wgpu presenter ({}/2)",
                        error, self.wgpu_recovery.rebuilds
                    );
                    self.wgpu = Some(gpu);
                }
                Err(message) => {
                    self.fallback_wgpu(format!("{error}; recreation failed: {message}"))
                }
            }
        } else {
            self.fallback_wgpu(error.to_string());
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
    pub(super) fn fallback_wgpu(&mut self, message: String) {
        self.wgpu = None;
        self.use_wgpu = false;
        self.wgpu_recovery.fallback = Some(message.clone());
        self.force_next_render = true;
        eprintln!("[GPU] {message}; switching to platform presentation");
        if let Err(error) = self.create_platform_presenter() {
            self.stop_renderer(format!(
                "GPU fallback failed: {error} (original failure: {message})"
            ));
        }
    }
    pub(super) fn check_wgpu_health(&mut self) -> bool {
        if let Some(error) = self.wgpu.as_ref().and_then(|gpu| gpu.check_health().err()) {
            self.recover_wgpu(error);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_uncaptured_validation_error_is_reported_without_panicking() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let (device, _) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let health = Health::install(&device, None);
        // MAP_READ cannot be combined with VERTEX without the optional mapping feature.
        let _invalid = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("qualification invalid usage"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        let error = health.check().expect_err("uncaptured validation callback");
        assert!(!error.recoverable);
        assert!(error.message.contains("wgpu error"));
    }
    #[test]
    fn gpu_recovery_budget_survives_successful_frames_and_terminal_errors() {
        let mut policy = Recovery::default();
        assert!(policy.rebuild(&Failure::lost("surface lost")));
        assert!(policy.rebuild(&Failure::lost("device lost after a successful frame")));
        assert!(!policy.rebuild(&Failure::lost("flapping device")));
        assert!(!Recovery::default().rebuild(&Failure::from("out of memory".to_owned())));
    }
    #[test]
    fn gpu_health_preserves_terminal_failure_over_later_loss() {
        let health = Health {
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
            wake: None,
        };
        assert!(health.check().is_ok());
        health.record(Failure::lost("lost"));
        assert!(health.check().unwrap_err().recoverable);
        health.record(Failure::from("memory failure".to_owned()));
        health.record(Failure::lost("subsequent loss"));
        assert_eq!(health.check().unwrap_err().message, "memory failure");
    }
}
