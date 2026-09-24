//! Single-thread owner of guest execution and its immutable UI projections.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use systemless::debug_overlay::{DebugOverlayFrameStats, DebugOverlaySnapshot};
use systemless::display::{self, CursorImage};
use systemless::game::{self, ApplicationIdentity};
use systemless::menu_model::GuestMenuSnapshot;
use systemless::runner::FixtureRunner;
use systemless::trap::dispatch::ScreenCopyBitsRect;
use systemless::ui_theme::UiThemeId;
use winit::event_loop::EventLoopProxy;

use super::worker_mailbox::{CommandMailbox, LatestSlot, WorkerCommand};
use super::{App, HostMouseReleaseLatch, InputAction};

pub(super) struct WorkerConfig {
    pub game_path: PathBuf,
    pub arrows_as_numpad: bool,
    pub native_integrations: bool,
    pub addressing_24_bit: bool,
    pub screen_depth: Option<u16>,
    pub display_scale: Option<u32>,
    pub ui_theme: UiThemeId,
    pub fullscreen: bool,
    pub debug_socket: Option<PathBuf>,
}

/// No guest object or mutable memory escapes the worker. Both pixel vectors
/// own their bytes, and all guest-derived state is a value projection.
pub(super) struct FrameSnapshot {
    pub screen_mode: (u32, u32, u16, u16, u16),
    pub framebuffer: Vec<u8>,
    pub argb: Vec<u32>,
    pub argb_size: (u32, u32),
    pub argb_includes_cursor: bool,
    pub has_outline_detail: bool,
    pub palette: [u32; 256],
    pub cursor: Option<CursorImage>,
    pub mouse_position: (i16, i16),
    pub menu: GuestMenuSnapshot,
    pub menu_generation: u32,
    pub application: Option<Arc<ApplicationIdentity>>,
    pub dialog_bounds: Option<(i16, i16, i16, i16)>,
    pub framed_content: Option<ScreenCopyBitsRect>,
    pub manual_content: Option<ScreenCopyBitsRect>,
    pub declared_content: Option<ScreenCopyBitsRect>,
    pub last_copybits_rect: Option<ScreenCopyBitsRect>,
    pub copybits_screen_count: u64,
    pub menu_bar_height: u32,
    pub guest_tick: u32,
    pub total_instructions: u64,
    pub halted_by_exit: bool,
    pub debug_overlay: DebugOverlaySnapshot,
}

pub(super) struct EmulationWorker {
    pub commands: Arc<CommandMailbox>,
    pub latest: Arc<LatestSlot<FrameSnapshot>>,
    error: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

struct InputServiceProbe {
    last_report: Instant,
    samples: Vec<Duration>,
}

impl InputServiceProbe {
    fn new() -> Self {
        Self {
            last_report: Instant::now(),
            samples: Vec::new(),
        }
    }

    fn observe(&mut self, sent: Instant) {
        self.samples.push(sent.elapsed());
        if self.last_report.elapsed() < Duration::from_secs(10) {
            return;
        }
        self.samples.sort_unstable();
        let percentile = |percent: usize| {
            let index = ((self.samples.len() - 1) * percent).div_ceil(100);
            self.samples[index].as_secs_f64() * 1000.0
        };
        eprintln!(
            "[UI-PROFILE] input service n={} p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
            self.samples.len(),
            percentile(50),
            percentile(95),
            percentile(99),
            percentile(100),
        );
        self.samples.clear();
        self.last_report = Instant::now();
    }
}

thread_local! {
    static INPUT_SERVICE_PROBE: RefCell<InputServiceProbe> = RefCell::new(InputServiceProbe::new());
}

impl EmulationWorker {
    pub fn start(config: WorkerConfig, wake: EventLoopProxy<()>) -> Result<Self, String> {
        let commands = Arc::new(CommandMailbox::default());
        let latest = Arc::new(LatestSlot::default());
        let thread_commands = Arc::clone(&commands);
        let thread_latest = Arc::clone(&latest);
        let error = Arc::new(Mutex::new(None));
        let thread_error = Arc::clone(&error);
        let error_wake = wake.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let thread = std::thread::Builder::new()
            .name("systemless-emulation".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_worker(config, thread_commands, thread_latest, wake, ready_tx)
                }));
                if let Err(panic) = result {
                    let message = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                        .unwrap_or_else(|| "emulation worker panicked".to_owned());
                    *thread_error.lock().unwrap() = Some(message);
                    let _ = error_wake.send_event(());
                }
            })
            .map_err(|error| format!("failed to start emulation worker: {error}"))?;
        if ready_rx.recv().is_err() {
            let _ = thread.join();
            let message = error
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| "emulation worker failed during startup".to_owned());
            return Err(message);
        }
        Ok(Self {
            commands,
            latest,
            error,
            thread: Some(thread),
        })
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }

    pub fn shutdown(&mut self) {
        self.commands.push(WorkerCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[SYSTEMLESS] Emulation worker failed during shutdown: {error:?}");
            }
        }
    }
}

fn run_worker(
    config: WorkerConfig,
    commands: Arc<CommandMailbox>,
    latest: Arc<LatestSlot<FrameSnapshot>>,
    wake: EventLoopProxy<()>,
    ready: mpsc::SyncSender<()>,
) {
    // Construct every Rc-backed guest object here. The runner never crosses
    // the thread boundary and no Send/Sync implementation is needed for it.
    let mut app = App::new_with_display_scale(
        config.game_path,
        config.arrows_as_numpad,
        false,
        config.addressing_24_bit,
        config.screen_depth,
        config.display_scale,
        config.ui_theme,
        config.fullscreen,
    );
    app.emulation_worker = true;
    #[cfg(target_os = "macos")]
    {
        app.native_integrations = config.native_integrations;
    }
    app.worker_commands = Some(Arc::clone(&commands));
    if let Some(path) = config.debug_socket {
        app.debug_server = Some(
            super::debug_server::DebugServer::bind(&path).unwrap_or_else(|error| {
                panic!("cannot bind debug socket {}: {error}", path.display())
            }),
        );
    }
    app.init_game();
    if let Some(snapshot) = capture_frame(&mut app) {
        latest.replace(snapshot);
    }
    ready.send(()).ok();
    let mut next_frame = Instant::now();
    loop {
        if let (Some(server), Some(runner)) = (app.debug_server.as_mut(), app.runner.as_mut()) {
            server.pump(runner);
        }
        let now = Instant::now();
        let timeout = next_frame.saturating_duration_since(now);
        let ready = commands.wait_until(timeout);
        if apply_commands(&mut app, ready) {
            break;
        }
        let now = Instant::now();
        if now < next_frame {
            continue;
        }
        let (target, _) = App::next_frame_target(now, next_frame);
        next_frame = target;
        app.next_frame_time = Some(next_frame);
        app.step_frame();
        if app.worker_shutdown_requested {
            break;
        }
        app.flush_ready_mouse_release();
        app.sync_save_files(false);
        let capture_start = Instant::now();
        if let Some(snapshot) = capture_frame(&mut app) {
            latest.replace(snapshot);
            let _ = wake.send_event(());
        }
        app.render_headroom = App::next_render_headroom(capture_start.elapsed());
        if let Some(runner) = app.runner.as_mut() {
            runner.finish_gui_frame();
        }
        app.frame_count += 1;
    }
    app.sync_save_files(true);
}

/// Returns true only for an explicit shutdown. Commands are applied in their
/// receive order before the next guest execution slice.
fn apply_commands(app: &mut App, commands: Vec<WorkerCommand>) -> bool {
    for command in commands {
        if let Some(runner) = app.runner.as_mut() {
            if apply_guest_command(
                runner,
                &mut app.mouse_release_latch,
                &mut app.menu_generation,
                &mut app.last_guest_menu,
                &mut app.worker_outline_scale,
                command,
            ) {
                return true;
            }
        } else if command == WorkerCommand::Shutdown {
            return true;
        }
    }
    false
}

pub(super) fn apply_guest_command(
    runner: &mut FixtureRunner,
    mouse_release_latch: &mut HostMouseReleaseLatch,
    menu_generation: &mut u32,
    last_guest_menu: &mut Option<GuestMenuSnapshot>,
    outline_scale: &mut u32,
    command: WorkerCommand,
) -> bool {
    match command {
        WorkerCommand::Input(input) => match input {
            InputAction::MouseMove { v, h } => {
                runner.set_mouse_position(v, h);
                runner.dispatcher_mut().show_cursor();
            }
            InputAction::MouseDown { v, h } => {
                runner.push_mouse_down(v, h);
                mouse_release_latch.press();
            }
            InputAction::MouseUp { v, h } => {
                if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                    runner.push_mouse_up(v, h);
                }
            }
            InputAction::KeyDown { key, ch } => runner.push_key_down(key, ch),
            InputAction::KeyUp { key, ch } => runner.push_key_up(key, ch),
        },
        WorkerCommand::MenuSelection {
            menu_id,
            item_number,
            generation,
        } => {
            let current = runner.guest_menu_snapshot();
            update_menu_generation(menu_generation, last_guest_menu, current);
            if generation == *menu_generation {
                runner.select_guest_menu_item(menu_id, item_number);
            }
        }
        WorkerCommand::ProbeInputService { sent } => {
            INPUT_SERVICE_PROBE.with_borrow_mut(|probe| probe.observe(sent));
        }
        WorkerCommand::SetOutlineScale(scale) => {
            *outline_scale = scale.clamp(1, 4);
        }
        WorkerCommand::Shutdown => return true,
    }
    false
}

fn update_menu_generation(
    generation: &mut u32,
    previous: &mut Option<GuestMenuSnapshot>,
    current: GuestMenuSnapshot,
) {
    if previous.as_ref() != Some(&current) {
        *generation = generation.wrapping_add(1);
        *previous = Some(current);
    }
}

fn capture_frame(app: &mut App) -> Option<FrameSnapshot> {
    let runner = app.runner.as_mut()?;
    runner.prepare_text_presentation();
    runner.composite_frame();
    let screen_mode = runner.dispatcher().screen_mode;
    let framebuffer_len = screen_mode.1.saturating_mul(u32::from(screen_mode.3));
    let framebuffer = runner
        .bus()
        .ram_slice(screen_mode.0, framebuffer_len)
        .to_vec();
    let clut = *runner.dispatcher().device_clut;
    let gamma = runner.dispatcher().device_gamma();
    let palette = display::argb_palette_from_clut_with_gamma(&clut, &gamma);
    let mut argb = Vec::new();
    display::render_screen_argb_with_gamma(runner.bus(), screen_mode, &clut, &gamma, &mut argb);
    let has_outline_detail = runner.bus().has_visible_outline_detail();
    let cursor = runner.dispatcher().cursor().cloned();
    let mouse_position = runner.dispatcher().mouse_position();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let software_cursor = std::env::var_os("SYSTEMLESS_SOFTWARE_CURSOR").is_some();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let software_cursor = true;
    let mut argb_size = (u32::from(screen_mode.2), u32::from(screen_mode.3));
    let mut argb_includes_cursor = false;
    if has_outline_detail {
        let guest = argb.clone();
        if software_cursor {
            if let Some(cursor) = cursor.as_ref() {
                display::render_cursor_argb(
                    &mut argb,
                    argb_size.0,
                    argb_size.1,
                    cursor,
                    mouse_position,
                );
                argb_includes_cursor = true;
            }
        }
        let mut presented = Vec::new();
        if let Some(size) = runner.bus().presented_argb_scaled(
            &guest,
            &argb,
            app.worker_outline_scale,
            &mut presented,
        ) {
            argb = presented;
            argb_size = size;
        }
    }
    let dialog_bounds = runner
        .dispatcher()
        .visible_dialog_structure_bounds(runner.bus());
    let framed_content = runner
        .dispatcher()
        .framed_manual_cport_presentation_rect(runner.bus());
    let manual_content = runner
        .dispatcher()
        .manual_cport_presentation_rect(runner.bus());
    let declared_content = runner
        .dispatcher()
        .declared_centered_presentation_rect(runner.bus());
    let last_copybits_rect = runner.dispatcher().last_screen_copybits_rect;
    let copybits_screen_count = runner.dispatcher().copybits_screen_count;
    #[cfg(target_os = "macos")]
    let menu_bar_height = super::native_menu_bar_height(Some(runner), app.native_integrations);
    #[cfg(not(target_os = "macos"))]
    let menu_bar_height = 0;
    let guest_tick = runner.guest_tick();
    let halted_by_exit = runner.halted_by_exit_to_shell();
    let menu = runner.guest_menu_snapshot();
    update_menu_generation(
        &mut app.menu_generation,
        &mut app.last_guest_menu,
        menu.clone(),
    );
    let launched_path = runner.dispatcher().launched_app_path();
    if launched_path
        != app
            .worker_application_identity
            .as_deref()
            .map(|identity| identity.path.as_str())
    {
        app.worker_application_identity = game::loaded_application_identity(runner).map(Arc::new);
    }
    let application = app.worker_application_identity.clone();
    let debug_overlay = runner.debug_overlay_snapshot(DebugOverlayFrameStats::default());
    Some(FrameSnapshot {
        screen_mode,
        framebuffer,
        argb,
        argb_size,
        argb_includes_cursor,
        has_outline_detail,
        palette,
        cursor,
        mouse_position,
        menu,
        menu_generation: app.menu_generation,
        application,
        dialog_bounds,
        framed_content,
        manual_content,
        declared_content,
        last_copybits_rect,
        copybits_screen_count,
        menu_bar_height,
        guest_tick,
        total_instructions: app.total_instructions,
        halted_by_exit,
        debug_overlay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::menu_model::{GuestMenu, GuestMenuItem};

    #[test]
    fn menu_revision_changes_only_when_guest_menu_content_changes() {
        let mut generation = 0;
        let mut previous = None;
        let first = GuestMenuSnapshot::default();
        update_menu_generation(&mut generation, &mut previous, first.clone());
        assert_eq!(generation, 1);
        update_menu_generation(&mut generation, &mut previous, first);
        assert_eq!(generation, 1);
        let replacement = GuestMenuSnapshot {
            menus: vec![GuestMenu {
                id: 4,
                title: "File".to_owned(),
                enabled: true,
                hierarchical: false,
                visible_in_menu_bar: true,
                items: vec![GuestMenuItem {
                    number: 1,
                    text: "Open".to_owned(),
                    enabled: true,
                    checked: false,
                    key_equivalent: None,
                    submenu_id: None,
                    separator: false,
                }],
            }],
        };
        update_menu_generation(&mut generation, &mut previous, replacement);
        assert_eq!(generation, 2);
    }
}
