//! Self-driving window-system qualification, available only in test-support builds.
//! Requests go through the real host window; observations use native events
//! and, for macOS event-free cursor warps, an AppKit pointer-position query.
//! The cursor packet is injected at the owned-snapshot boundary: driver tests
//! separately prove guest instruction -> retained warp -> matching acknowledgement.
use super::*;
use std::io::Write;
use std::time::{Duration, Instant};
use winit::window::Window;

// Stream during the run so observer storage does not grow alongside RSS.
// Reconstruct the compatible report array only when reporting/closing.
struct ResourceLog {
    path: PathBuf,
    writer: std::io::BufWriter<std::fs::File>,
    count: usize,
}
impl ResourceLog {
    fn new(report: &std::path::Path) -> Self {
        let path = report.with_extension("gpu-resources.jsonl");
        let writer = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        Self {
            path,
            writer,
            count: 0,
        }
    }
    fn append(&mut self, sample: &serde_json::Value) {
        writeln!(self.writer, "{sample}").unwrap();
        self.writer.flush().unwrap();
        self.count += 1;
    }
    fn samples(&self) -> Vec<serde_json::Value> {
        let samples: Vec<_> = std::fs::read_to_string(&self.path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(samples.len(), self.count, "native resource log incomplete");
        samples
    }
}

struct Probe {
    app: App,
    report: PathBuf,
    started: Instant,
    phase_started: Instant,
    phase: usize,
    helper: Option<Window>,
    focused: bool,
    lost_focus: bool,
    returned_focus: bool,
    resized: bool,
    moved: Option<(f64, f64)>,
    expected_cursor: Option<(f64, f64)>,
    expected_guest: Option<(i16, i16)>,
    #[cfg(target_os = "macos")]
    cursor_input_sent: bool,
    sequence: u64,
    checks: Vec<&'static str>,
    complete: bool,
    fault: String,
    injected: u8,
    fault_sequence: u64,
    fault_generation: Option<u64>,
    soak: Duration,
    cycles: u32,
    gpu_resources: ResourceLog,
}

pub(super) fn run(event_loop: EventLoop<()>, app: App, report: PathBuf) {
    let now = Instant::now();
    let mut probe = Probe {
        app,
        gpu_resources: ResourceLog::new(&report),
        report,
        started: now,
        phase_started: now,
        phase: 0,
        helper: None,
        focused: false,
        lost_focus: false,
        returned_focus: false,
        resized: false,
        moved: None,
        expected_cursor: None,
        expected_guest: None,
        #[cfg(target_os = "macos")]
        cursor_input_sent: false,
        sequence: 0,
        checks: Vec::new(),
        complete: false,
        fault: std::env::var("SYSTEMLESS_GPU_FAULT").unwrap_or_default(),
        injected: 0,
        fault_sequence: 0,
        fault_generation: None,
        soak: Duration::from_secs(
            std::env::var("SYSTEMLESS_NATIVE_SOAK_SECONDS")
                .unwrap_or_else(|_| "0".into())
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds <= 21600)
                .expect("soak seconds must be 0..21600"),
        ),
        cycles: 0,
    };
    event_loop
        .run_app(&mut probe)
        .expect("native probe event loop");
    probe.complete = probe.phase == 10 && probe.app.runtime_error.is_none();
    if let Some(owner) = probe.app.owner.as_mut() {
        probe.complete &= owner.join_finished().expect("owner teardown");
    }
    if probe.complete {
        probe
            .checks
            .push("production close completes owner teardown");
    }
    probe.write_report();
    assert!(
        probe.complete,
        "native window probe incomplete at phase {}",
        probe.phase
    );
}

impl Probe {
    fn presentation_backend(&self) -> &'static str {
        if self.app.wgpu.is_some() {
            return "wgpu";
        }
        #[cfg(target_os = "macos")]
        {
            self.app
                .surface
                .as_ref()
                .map_or("none", |p| p.backend_name())
        }
        #[cfg(not(target_os = "macos"))]
        {
            #[cfg(target_os = "windows")]
            if self.app.gpu.is_some() {
                return "d3d11";
            }
            if self.app.surface.is_some() {
                "software"
            } else {
                "none"
            }
        }
    }
    fn software_layer(&self) -> Option<serde_json::Value> {
        #[cfg(target_os = "macos")]
        {
            self.app
                .surface
                .as_ref()
                .and_then(|p| p.software_layer_snapshot())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    fn sample_gpu_resources(&mut self) {
        self.gpu_resources.append(&serde_json::json!({
            "seconds": self.started.elapsed().as_secs_f64(), "cycle": self.cycles,
            "rebuilds": self.app.wgpu_recovery.rebuilds,
            "renderer": if self.app.use_wgpu { "wgpu" } else if self.app.use_software { "software" } else { "platform" },
            "presentation_backend": self.presentation_backend(),
            "guest_generation": self.app.frame.generation,
            "software_layer": self.software_layer(),
            "resources": self.app.wgpu.as_ref().map(|gpu| gpu.resource_snapshot()),
        }));
    }

    fn write_report(&self) {
        let report = serde_json::json!({
            "passed": self.complete, "phase": self.phase, "checks": self.checks,
            "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            "soak_seconds": self.soak.as_secs(), "lifecycle_cycles": self.cycles,
            "os": std::env::consts::OS,
            "backend": if self.app.owner.is_some() { "thread" } else { "inline-or-stopped" },
            "cursor_scope": if cfg!(target_os = "macos") {
                "owned snapshot -> OS pointer query; separate mapped input-command round trip; guest origin covered by driver tests"
            } else {
                "owned snapshot -> actual native cursor event -> guest input round trip; guest origin covered by driver tests"
            },
            "renderer": if self.app.use_wgpu { "wgpu" } else if self.app.use_software { "software" } else { "platform" },
            "presentation_backend": self.presentation_backend(),
            "guest_generation": self.app.frame.generation,
            "software_layer": self.software_layer(),
            "runtime_error": self.app.runtime_error,
            "fault": self.fault,
            "fault_generation": self.fault_generation,
            "requested_gpu_fallback": self.app.gpu_fallback.name(),
            "injected_failures": self.injected,
            "rebuilds": self.app.wgpu_recovery.rebuilds,
            "fallback": self.app.wgpu_recovery.fallback,
            "guest_sequence": self.app.frame.sequence,
            "gpu_resources": self.gpu_resources.samples(),
            "window": self.app.window.as_ref().map(|w| serde_json::json!({
                "width": w.inner_size().width, "height": w.inner_size().height,
                "scale_factor": w.scale_factor(),
                "monitor": w.current_monitor().map(|m| serde_json::json!({
                    "name": m.name(), "refresh_millihertz": m.refresh_rate_millihertz(),
                    "width": m.size().width, "height": m.size().height,
                })),
                "fullscreen": w.fullscreen().is_some(), "focused": self.focused,
                "resized_event": self.resized, "cursor_event": self.moved,
                "expected_cursor": self.expected_cursor,
                "guest_mouse": self.app.frame.mouse_position,
                "expected_guest": self.expected_guest,
            })),
        });
        std::fs::write(&self.report, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    fn advance(&mut self, check: &'static str) {
        if !self.checks.contains(&check) {
            self.checks.push(check);
        }
        eprintln!("[NATIVE-PROBE] PASS {check}");
        self.phase += 1;
        self.phase_started = Instant::now();
        self.resized = false;
    }
    fn step(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > self.soak + Duration::from_secs(90) {
            self.write_report();
            panic!("native probe timed out at phase {}", self.phase);
        }
        assert!(self.app.runtime_error.is_none(), "native runtime failed");
        let Some(window) = self.app.window.as_ref().cloned() else {
            return;
        };
        #[cfg(target_os = "macos")]
        if self.phase == 9 {
            // CGWarpMouseCursorPosition intentionally generates no event:
            // https://developer.apple.com/documentation/coregraphics/cgwarpmousecursorposition(_:)
            self.moved = macos_cursor_position(&window);
            if !self.cursor_input_sent
                && self
                    .moved
                    .zip(self.expected_cursor)
                    .is_some_and(|(a, b)| (a.0 - b.0).abs() < 2.0 && (a.1 - b.1).abs() < 2.0)
            {
                let (x, y) = self.moved.unwrap();
                let (v, h) = self.app.host_mouse_to_mac(x, y);
                self.expected_guest = Some((v, h));
                self.app
                    .send_command(runtime_protocol::GuiCommand::MouseMove { v, h });
                self.cursor_input_sent = true;
            }
        }
        // A repeated cycle must request a different size: requesting current
        // bounds need not generate a native Resized event on any platform.
        let target_size = if self.cycles % 2 == 0 {
            winit::dpi::PhysicalSize::new(700, 500)
        } else {
            winit::dpi::PhysicalSize::new(720, 520)
        };
        let settled = self.phase_started.elapsed() > Duration::from_secs(2);
        match self.phase {
            0 if self.app.frame.sequence > 2 && self.app.last_presented_guest_tick.is_some() => {
                if self.gpu_resources.count == 0 {
                    self.sample_gpu_resources();
                }
                let generation = self
                    .fault_generation
                    .get_or_insert(self.app.frame.generation);
                assert_eq!(
                    self.app.frame.generation, *generation,
                    "renderer transition restarted the guest"
                );
                if self.fault == "software-recreate" {
                    if self.app.frame.sequence <= self.fault_sequence + 2 {
                        return;
                    }
                    if self.injected < 3 {
                        self.sample_gpu_resources();
                        self.app.surface = None;
                        self.app
                            .create_platform_presenter()
                            .expect("recreate software surface");
                        self.app.last_presented_guest_tick = None;
                        self.app.force_next_render = true;
                        #[cfg(target_os = "macos")]
                        {
                            self.app.force_gpu_present = true;
                        }
                        self.fault_sequence = self.app.frame.sequence;
                        self.injected += 1;
                        window.request_redraw();
                        return;
                    }
                    self.checks.push("software surface recreation preserves guest generation and releases old image layers");
                } else if !self.fault.is_empty() {
                    let count = if self.fault == "repeated-loss" { 3 } else { 1 };
                    if !matches!(self.fault.as_str(), "startup" | "platform-startup")
                        && self.injected < count
                    {
                        if self.app.frame.sequence <= self.fault_sequence + 2 {
                            return;
                        }
                        let gpu = self
                            .app
                            .wgpu
                            .as_ref()
                            .expect("presenter before injected fault");
                        let failure = if matches!(self.fault.as_str(), "fatal" | "platform-fatal") {
                            renderer_recovery::Failure::from(
                                "qualification: terminal error".to_owned(),
                            )
                        } else {
                            renderer_recovery::Failure::lost("qualification: device lost")
                        };
                        gpu.inject_failure(failure);
                        self.injected += 1;
                        self.fault_sequence = self.app.frame.sequence;
                        return;
                    }
                    if self.app.frame.sequence <= self.fault_sequence + 2 {
                        return;
                    }
                    let rebuilds = match self.fault.as_str() {
                        "loss" | "recreate" => 1,
                        "repeated-loss" => 2,
                        "fatal" | "startup" | "platform-startup" | "platform-fatal" => 0,
                        _ => panic!("unknown GPU fault"),
                    };
                    assert_eq!(self.app.wgpu_recovery.rebuilds, rebuilds);
                    assert_eq!(self.app.use_wgpu, self.fault == "loss");
                    assert_eq!(
                        self.app.wgpu_recovery.fallback.is_some(),
                        self.fault != "loss"
                    );
                    if self.fault.starts_with("platform-") {
                        assert!(self.app.use_software);
                        assert!(self
                            .app
                            .wgpu_recovery
                            .fallback
                            .as_deref()
                            .unwrap()
                            .contains("injected platform creation failure"));
                        self.checks.push(
                            "failed platform fallback reaches software and preserves diagnostic",
                        );
                    }
                    self.checks.push(
                        "guest continues and replacement presenter submits after injected failure",
                    );
                }
                if std::env::var_os("SYSTEMLESS_NATIVE_RECOVERY_ONLY").is_some() {
                    self.checks.push("guest frames reach real window");
                    self.sample_gpu_resources();
                    self.phase = 10;
                    self.app
                        .window_event(event_loop, window.id(), WindowEvent::CloseRequested);
                    return;
                }
                window.focus_window();
                self.advance("guest frames reach real window");
            }
            1 if self.focused && settled => {
                let _ = window.request_inner_size(target_size);
                self.advance("initial native focus");
            }
            2 if self.resized && window.inner_size() == target_size && settled => {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                self.advance("native resize observed");
            }
            3 if self.resized && window.fullscreen().is_some() && settled => {
                let monitor = window.current_monitor().expect("fullscreen monitor");
                if window.inner_size() != monitor.size() {
                    return;
                }
                window.set_fullscreen(None);
                self.advance("fullscreen entry reaches monitor dimensions");
            }
            4 if self.resized && window.fullscreen().is_none() && settled => {
                let size = window.inner_size();
                let monitor = window.current_monitor().expect("window monitor").size();
                if size.width == 0 || size.height == 0 || size == monitor {
                    return;
                }
                // AppKit may restore its remembered standard (zoom) frame.
                // Verify windowed bounds, then require an exact resize after exit.
                eprintln!("[NATIVE-PROBE] restored normal size={size:?}");
                let _ = window.request_inner_size(target_size);
                self.advance("fullscreen exit returns to normal window bounds");
            }
            5 if window.inner_size() == target_size && settled => {
                let helper = event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Systemless focus probe")
                            .with_inner_size(winit::dpi::PhysicalSize::new(240, 160)),
                    )
                    .unwrap();
                self.lost_focus = false;
                helper.focus_window();
                self.helper = Some(helper);
                self.advance("exact resize works after fullscreen exit");
            }
            6 if self.lost_focus && settled => {
                self.returned_focus = false;
                window.focus_window();
                self.advance("native focus loss observed");
            }
            7 if self.returned_focus && settled => {
                self.helper = None;
                self.sequence = self.app.frame.sequence;
                self.advance("native focus return observed");
            }
            8 if self.app.frame.sequence > self.sequence + 2 && settled => {
                let size = window.inner_size();
                let target = self
                    .app
                    .physical_to_mac(f64::from(size.width) * 0.4, f64::from(size.height) * 0.4);
                self.app.guest_state.warp = Some(runtime_protocol::CursorWarp {
                    serial: u64::MAX,
                    position: target,
                });
                self.expected_guest = Some(target);
                self.moved = None;
                self.app.sync_guest_cursor_warp();
                assert!(
                    self.app.guest_state.warp.is_none(),
                    "warp packet not consumed"
                );
                assert_eq!(self.app.mouse_guest_offset, (0, 0), "native warp fell back");
                self.expected_cursor = Some(self.app.mouse_physical);
                self.advance("frame delivery continues after focus return");
            }
            9 if self
                .moved
                .zip(self.expected_cursor)
                .is_some_and(|(a, b)| (a.0 - b.0).abs() < 2.0 && (a.1 - b.1).abs() < 2.0)
                && Some(self.app.frame.mouse_position) == self.expected_guest
                && settled =>
            {
                self.advance("native cursor position and guest input round trip verified");
                self.cycles += 1;
                self.sample_gpu_resources();
                if self.started.elapsed() < self.soak || (!self.soak.is_zero() && self.cycles < 2) {
                    self.phase = 0;
                    self.lost_focus = false;
                    self.returned_focus = false;
                    self.expected_cursor = None;
                    self.expected_guest = None;
                    #[cfg(target_os = "macos")]
                    {
                        self.cursor_input_sent = false;
                    }
                    return;
                }
                self.app
                    .window_event(event_loop, window.id(), WindowEvent::CloseRequested);
            }
            _ => {}
        }
    }
}
impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ()) {
        self.app.user_event(event_loop, event);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.app.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        match &event {
            WindowEvent::Focused(value) => {
                eprintln!("[NATIVE-PROBE] phase={} focus={value}", self.phase);
                self.focused = *value;
                if !value {
                    self.lost_focus = true;
                }
                if *value && self.lost_focus {
                    self.returned_focus = true;
                }
            }
            WindowEvent::Resized(size) => {
                eprintln!("[NATIVE-PROBE] phase={} resize={size:?}", self.phase);
                self.resized = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.moved = Some((position.x, position.y))
            }
            _ => {}
        }
        self.app.window_event(event_loop, id, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
        self.step(event_loop);
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

#[cfg(target_os = "macos")]
fn macos_cursor_position(window: &Window) -> Option<(f64, f64)> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: the main-thread event loop owns this live winit NSView/NSWindow.
    // These are read-only AppKit queries; no event injection or global hooks.
    unsafe {
        let view: &NSObject = handle.ns_view.cast().as_ref();
        let native_window: *mut NSObject = msg_send![view, window];
        let native_window = native_window.as_ref()?;
        let point: objc2_foundation::CGPoint =
            msg_send![native_window, mouseLocationOutsideOfEventStream];
        let local: objc2_foundation::CGPoint =
            msg_send![view, convertPoint: point fromView: std::ptr::null::<NSObject>()];
        let bounds: objc2_foundation::CGRect = msg_send![view, bounds];
        let flipped: bool = msg_send![view, isFlipped];
        let y = if flipped {
            local.y - bounds.origin.y
        } else {
            bounds.origin.y + bounds.size.height - local.y
        };
        let scale = window.scale_factor();
        Some(((local.x - bounds.origin.x) * scale, y * scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_resource_log_preserves_samples_without_retaining_them() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let report = std::env::temp_dir().join(format!(
            "systemless-native-{}-{unique}.json",
            std::process::id()
        ));
        let mut log = ResourceLog::new(&report);
        let a = serde_json::json!({"cycle": 0, "resources": null});
        let b = serde_json::json!({"cycle": 1, "resources": {"buffers": 12}});
        log.append(&a);
        log.append(&b);
        assert_eq!(log.samples(), vec![a, b]);
        let path = log.path.clone();
        drop(log);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
        std::fs::remove_file(path).unwrap();
    }
}
