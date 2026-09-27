//! Self-driving window-system qualification, available only in test-support builds.
//! Requests go through the real host window; observations come from native events.
//! The cursor packet is injected at the owned-snapshot boundary: driver tests
//! separately prove guest instruction -> retained warp -> matching acknowledgement.
use super::*;
use std::time::{Duration, Instant};
use winit::window::Window;

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
    sequence: u64,
    checks: Vec<&'static str>,
    complete: bool,
}

pub(super) fn run(event_loop: EventLoop<()>, app: App, report: PathBuf) {
    let now = Instant::now();
    let mut probe = Probe {
        app,
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
        sequence: 0,
        checks: Vec::new(),
        complete: false,
    };
    event_loop
        .run_app(&mut probe)
        .expect("native probe event loop");
    probe.complete = probe.phase == 9 && probe.app.runtime_error.is_none();
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
    fn write_report(&self) {
        let report = serde_json::json!({
            "passed": self.complete, "phase": self.phase, "checks": self.checks,
            "elapsed_seconds": self.started.elapsed().as_secs_f64(),
            "os": std::env::consts::OS,
            "backend": if self.app.owner.is_some() { "thread" } else { "inline-or-stopped" },
            "cursor_scope": "owned snapshot through actual native cursor event; guest origin covered separately",
            "runtime_error": self.app.runtime_error,
        });
        std::fs::write(&self.report, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    fn advance(&mut self, check: &'static str) {
        self.checks.push(check);
        eprintln!("[NATIVE-PROBE] PASS {check}");
        self.phase += 1;
        self.phase_started = Instant::now();
        self.resized = false;
    }
    fn step(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > Duration::from_secs(90) {
            self.write_report();
            panic!("native probe timed out at phase {}", self.phase);
        }
        assert!(self.app.runtime_error.is_none(), "native runtime failed");
        let Some(window) = self.app.window.as_ref().cloned() else {
            return;
        };
        let settled = self.phase_started.elapsed() > Duration::from_millis(750);
        match self.phase {
            0 if self.app.frame.sequence > 2 => {
                window.focus_window();
                self.advance("guest frames reach real window");
            }
            1 if self.focused && settled => {
                let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(700, 500));
                self.advance("initial native focus");
            }
            2 if self.resized
                && window.inner_size() == winit::dpi::PhysicalSize::new(700, 500)
                && settled =>
            {
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
                if window.inner_size() != winit::dpi::PhysicalSize::new(700, 500) {
                    return;
                }
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
                self.advance("fullscreen exit restores window dimensions");
            }
            5 if self.lost_focus && settled => {
                self.returned_focus = false;
                window.focus_window();
                self.advance("native focus loss observed");
            }
            6 if self.returned_focus && settled => {
                self.helper = None;
                self.sequence = self.app.frame.sequence;
                self.advance("native focus return observed");
            }
            7 if self.app.frame.sequence > self.sequence + 2 && settled => {
                let size = window.inner_size();
                let target = self
                    .app
                    .physical_to_mac(f64::from(size.width) * 0.4, f64::from(size.height) * 0.4);
                self.app.guest_state.warp = Some(runtime_protocol::CursorWarp {
                    serial: u64::MAX,
                    position: target,
                });
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
            8 if self
                .moved
                .zip(self.expected_cursor)
                .is_some_and(|(a, b)| (a.0 - b.0).abs() < 2.0 && (a.1 - b.1).abs() < 2.0)
                && settled =>
            {
                self.advance("snapshot warp produces matching native cursor event");
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
                self.focused = *value;
                if !value {
                    self.lost_focus = true;
                }
                if *value && self.lost_focus {
                    self.returned_focus = true;
                }
            }
            WindowEvent::Resized(_) => self.resized = true,
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
