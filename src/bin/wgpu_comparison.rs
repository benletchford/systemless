//! Experimental wgpu presentation using PR #4190's guest worker.
#![allow(dead_code)]
#[cfg(feature = "gui")]
compile_error!("wgpu-comparison must be built with --no-default-features --features wgpu-demo,jit; GPUI is forbidden");
#[path = "gpui_demo_activation.rs"]
mod activation;
#[path = "gpui_demo_audio_clock.rs"]
mod audio_clock;
#[path = "gpui_demo_cadence.rs"]
mod cadence;
#[path = "gpui_demo_clipboard.rs"]
mod clipboard;
#[path = "gpui_demo_coverage.rs"]
mod coverage;
#[path = "desktop/cpu_frame.rs"]
mod cpu_frame;
#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;
#[path = "gpui_demo_frames.rs"]
mod frames;
#[path = "gpui_demo_input.rs"]
mod input;
#[path = "gpui_demo_mailbox.rs"]
mod mailbox;
#[path = "gpui_demo_marked_document.rs"]
mod marked_document;
#[path = "gpui_demo_performance.rs"]
mod performance;
#[path = "gpui_demo_scroll.rs"]
mod scroll;
#[path = "gpui_demo_text.rs"]
mod text;

struct LaunchOptions {
    addressing_24_bit: bool,
    ui_theme: systemless::ui_theme::UiThemeId,
    native_integrations: bool,
    debug_socket: Option<std::path::PathBuf>,
}
mod desktop {
    use clap::Parser;
    use std::{
        path::PathBuf,
        sync::{mpsc, Arc},
        time::{Duration, Instant},
    };
    use systemless::{
        memory::{globals::addr::MBAR_HEIGHT, MemoryBus},
        menu_model::GuestMenuSnapshot,
        runner::{
            default_realtime_instructions_per_tick, ControlSnapshot, DialogSnapshot,
            ListManagerSnapshot, StandardFileSnapshot, TextEditSnapshot, WindowFrameSnapshot,
        },
        systems::macintosh::{
            game,
            session::{MacintoshInput, MacintoshSession},
        },
    };
    use winit::{
        application::ApplicationHandler,
        event::{ElementState, MouseButton, WindowEvent},
        event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
        keyboard::{Key, NamedKey},
        window::{Window, WindowId},
    };
    #[derive(Parser)]
    struct Args {
        game: PathBuf,
        #[arg(long)]
        prefer_powerpc: bool,
        #[arg(long)]
        screen_depth: Option<u16>,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long)]
        offscreen: bool,
        #[arg(long)]
        capture_menu: Option<i16>,
        #[arg(long, default_value_t = 1050)]
        updates: u64,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_delimiter = ',')]
        capture_at: Vec<u64>,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        seconds: Option<f64>,
        #[arg(skip)]
        options: Option<super::LaunchOptions>,
    }
    fn gpui_pixels(mut pixels: Vec<u8>) -> Vec<u8> {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        pixels
    }
    include!("desktop/guest_worker.rs");
    include!("desktop/comparison_driver.rs");
    include!("desktop/guest_text_plans.rs");
    #[path = "../wgpu_accessibility.rs"]
    mod accessibility;
    #[path = "../wgpu_menu.rs"]
    mod menu;
    #[path = "../wgpu_renderer.rs"]
    mod renderer;
    #[path = "../wgpu_scene.rs"]
    mod scene;
    enum Event {
        Publication,
        Accessibility(accesskit_winit::Event),
    }
    impl From<accesskit_winit::Event> for Event {
        fn from(event: accesskit_winit::Event) -> Self {
            Self::Accessibility(event)
        }
    }

    struct Wake(EventLoopProxy<Event>);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send_event(Event::Publication);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.send_event(Event::Publication);
        }
    }
    pub(super) fn main() {
        let args = Args::parse();
        assert!(args.scale.is_finite() && args.scale > 0. && args.scale <= 4.);
        assert!(args
            .seconds
            .is_none_or(|s| s.is_finite() && s > 0. && s <= 86400.));
        if args.offscreen {
            run_offscreen(args);
            return;
        }
        let event_loop = EventLoop::<Event>::with_user_event().build().unwrap();
        let updates = Arc::new(Updates::default());
        let (commands, receiver) = mpsc::channel();
        let output = updates.clone();
        let mut app = App {
            menu: Default::default(),
            adapter: None,
            accessibility: Default::default(),
            window: None,
            renderer: None,
            updates,
            commands,
            wake: Arc::new(Wake(event_loop.create_proxy())),
            latest: None,
            keyboard: Default::default(),
            modifiers: Default::default(),
            pointer: (0, 0),
            scale: args.scale,
            report: args.report.clone(),
            seconds: args.seconds,
            composition: Default::default(),
            ime_allowed: false,
            comparison: ComparisonDriver::from_env(),
            started: Instant::now(),
            frames: Vec::new(),
            last_frame: None,
            last_input_profiled: 0,
        };
        app.comparison.metrics |= app.report.is_some();
        let worker = std::thread::spawn(move || run_guest(args, receiver, output, true));
        event_loop.run_app(&mut app).unwrap();
        let _ = app.commands.send(Command::Shutdown);
        worker.join().unwrap();
        if let Some(path) = &app.report {
            std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
                "backend": "wgpu", "measurement": "CPU submit/present-call timing; not physical display or input-to-photon",
                "frames": app.frames,
                "gpu_render_pass_samples_sequence_ms": app.renderer.as_ref().and_then(|r|r.gpu_samples()),
                "event_to_acknowledged_snapshot_ms":app.comparison.latencies,
                "completed_actions": app.comparison.next, "requested_actions": app.comparison.actions.len(),
            })).unwrap()).unwrap();
        }
    }
    fn comparison_preedit(
        update: &Update,
        composition: &super::input::GuestComposition,
    ) -> Option<Update> {
        let preedit = composition.preedit.as_ref()?;
        // This probe uses guest font coverage. Unrepresentable candidates are
        // left to the OS candidate window; no substitution reaches guest text.
        if !preedit
            .text
            .chars()
            .all(|c| systemless::systems::macintosh::mac_roman::encode_mac_roman_char(c).is_some())
        {
            return None;
        }
        let owner = composition.owner()?;
        let super::input::TextInputTarget::Dialog { item, .. } = owner.target else {
            return None;
        };
        let text = composition.virtual_document()?.text()?;
        let mut preview = update.clone();
        let dialog = preview
            .dialogs
            .iter_mut()
            .find(|d| (d.guest_id, d.generation) == owner.identity)?;
        let field = dialog.items.iter_mut().find(|i| i.number == item)?;
        field.text = text;
        let start = owner
            .selection
            .start
            .checked_add(preedit.selection_utf16.start)?;
        let end = owner
            .selection
            .start
            .checked_add(preedit.selection_utf16.end)?;
        field.selection = Some((i16::try_from(start).ok()?, i16::try_from(end).ok()?));
        field.caret_visible = Some(true);
        Some(preview)
    }
    fn run_offscreen(args: Args) {
        let output = args.output.clone().expect("--offscreen requires --output");
        std::fs::create_dir_all(&output).unwrap();
        let capture_menu = args.capture_menu;
        let count = args.updates;
        let captures = args.capture_at.clone();
        let scale = args.scale;
        let updates = Arc::new(Updates::default());
        let worker_updates = updates.clone();
        let (commands, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || run_guest(args, receiver, worker_updates, false));
        let mut renderer = pollster::block_on(renderer::Renderer::offscreen(640, 480));
        let mut driver = ComparisonDriver::from_env();
        let mut samples = Vec::new();
        let started = Instant::now();
        for observed in 1..=count {
            let update = updates
                .take_timeout(Duration::from_secs(10))
                .expect("guest publication timeout");
            driver.observe(&update, &commands);
            let (w, h, pixels) = update
                .frame
                .as_ref()
                .unwrap_or_else(|| panic!("missing guest frame: {}", update.status));
            let width = (*w as f64 * scale).round() as u32;
            let height = (*h as f64 * scale).round() as u32;
            if renderer.size() != [width, height] {
                renderer.resize(width, height);
            }
            let start = Instant::now();
            let mut quads = scene::controls(&update, *w, *h, renderer.raster(*w, *h));
            if observed == count {
                if let Some(id) = capture_menu {
                    let p = update
                        .menu_presentations
                        .iter()
                        .find(|p| p.pane.menu.id == id)
                        .expect("supported capture menu");
                    let mut state = menu::MenuPresenter::default();
                    state.open = Some((p.pane.menu.guest_id, p.pane.menu.generation));
                    quads.extend(scene::menu(
                        &update,
                        &state,
                        *w,
                        *h,
                        renderer.raster(*w, *h),
                    ));
                }
            }
            renderer.draw(*w, *h, pixels, &quads);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
            if captures.contains(&observed) || observed == count {
                renderer.save_capture(&output.join(format!("frame-{observed}.png")));
                let rgba = gpui_pixels(pixels.clone());
                image::save_buffer(
                    output.join(format!("frame-{observed}.guest.png")),
                    &rgba,
                    *w,
                    *h,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let dialogs:Vec<_>=update.dialogs.iter().map(|d|serde_json::json!({"id":d.guest_id,"generation":d.generation,"bounds":d.bounds,"active":d.active,"items":d.items.iter().map(|i|serde_json::json!({"number":i.number,"kind":format!("{:?}",i.kind),"text":i.text,"bounds":i.bounds,"selection":i.selection,"value":i.value,"caret":i.caret_visible})).collect::<Vec<_>>()})).collect();
                std::fs::write(output.join(format!("frame-{observed}.json")),serde_json::to_vec_pretty(&serde_json::json!({"dialogs":dialogs,"quads":quads.len(),"status":update.status,"menu_tracking":update.guest_menu_tracking})).unwrap()).unwrap();
            }
        }
        commands.send(Command::Shutdown).unwrap();
        worker.join().unwrap();
        let passed = driver.next == driver.actions.len();
        std::fs::write(output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"passed":passed,"scope":"offscreen GPU composition and production worker actions; excludes native display/input/audio","observed":count,"completed_actions":driver.next,"requested_actions":driver.actions.len(),"elapsed_ms":started.elapsed().as_secs_f64()*1000.,"compose_submit_cpu_ms":samples,"event_to_acknowledged_snapshot_ms":driver.latencies,"gpu_render_pass_samples_sequence_ms":renderer.gpu_samples()})).unwrap()).unwrap();
        assert!(passed, "not all scheduled actions completed");
    }
    struct App {
        menu: menu::MenuPresenter,
        adapter: Option<accesskit_winit::Adapter>,
        accessibility: accessibility::Accessibility,
        window: Option<Arc<Window>>,
        renderer: Option<renderer::Renderer>,
        updates: Arc<Updates>,
        commands: mpsc::Sender<Command>,
        wake: Arc<Wake>,
        latest: Option<Update>,
        keyboard: super::input::KeyboardState,
        modifiers: super::input::Modifiers,
        pointer: (i16, i16),
        scale: f64,
        report: Option<PathBuf>,
        seconds: Option<f64>,
        started: Instant,
        composition: super::input::GuestComposition,
        ime_allowed: bool,
        comparison: ComparisonDriver,
        frames: Vec<serde_json::Value>,
        last_frame: Option<Instant>,
        last_input_profiled: u64,
    }
    impl App {
        fn update_ime_geometry(&mut self) {
            let Some(window) = &self.window else {
                return;
            };
            let allowed = self.composition.owner().is_some()
                && self.menu.open.is_none()
                && window.has_focus();
            if allowed != self.ime_allowed {
                window.set_ime_allowed(allowed);
                self.ime_allowed = allowed;
            }
            let (Some(update), Some(renderer)) = (&self.latest, &self.renderer) else {
                return;
            };
            let Some((width, height, _)) = &update.frame else {
                return;
            };
            let Some(owner) = self.composition.owner() else {
                return;
            };
            let super::input::TextInputTarget::Dialog { item, .. } = owner.target else {
                return;
            };
            let Some(dialog) = update
                .dialogs
                .iter()
                .find(|d| (d.guest_id, d.generation) == owner.identity)
            else {
                return;
            };
            let Some(field) = dialog.items.iter().find(|i| i.number == item) else {
                return;
            };
            let Some(layout) = &field.edit_text_layout else {
                return;
            };
            let line = super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
            let x = i32::from(field.bounds.1)
                + 1
                + line.positions[owner.selection.end.min(line.positions.len() - 1)];
            let (left, top, w, h) = renderer.viewport(*width, *height);
            window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(
                    left + x as f32 * w / *width as f32,
                    top + f32::from(field.bounds.0) * h / *height as f32,
                ),
                winit::dpi::PhysicalSize::new(
                    (w / *width as f32).max(1.),
                    f32::from(layout.line_height) * h / *height as f32,
                ),
            );
        }
        fn update_accessibility(&mut self) {
            let viewport = self
                .latest
                .as_ref()
                .and_then(|u| u.frame.as_ref())
                .and_then(|(w, h, _)| self.renderer.as_ref().map(|r| r.viewport(*w, *h)))
                .unwrap_or((0., 0., 640., 480.));
            if let Some(adapter) = &mut self.adapter {
                adapter
                    .update_if_active(|| self.accessibility.tree(self.latest.as_ref(), viewport));
            }
        }

        fn send(&mut self, input: MacintoshInput) {
            let _ = self.commands.send(timed_input(input));
        }
        fn publication(&mut self) {
            let waker = std::task::Waker::from(self.wake.clone());
            if let Some(update) = self.updates.take_and_subscribe(&waker) {
                self.menu.reconcile(&update);
                self.composition.synchronize(comparison_text_owner(&update));
                if let Some((_, owner)) = &update.text_commit_rejection {
                    self.composition.reject(owner);
                }
                self.comparison.observe(&update, &self.commands);
                self.latest = Some(update);
                self.update_accessibility();
                self.update_ime_geometry();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
    impl ApplicationHandler<Event> for App {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, _: winit::event::StartCause) {
            if self
                .seconds
                .is_some_and(|seconds| self.started.elapsed().as_secs_f64() >= seconds)
            {
                event_loop.exit();
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if let Some(seconds) = self.seconds {
                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    self.started + Duration::from_secs_f64(seconds),
                ));
            }
        }
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_visible(false)
                            .with_title("Systemless · wgpu comparison")
                            .with_inner_size(winit::dpi::LogicalSize::new(
                                640. * self.scale,
                                480. * self.scale,
                            )),
                    )
                    .unwrap(),
            );
            self.adapter = Some(accesskit_winit::Adapter::with_event_loop_proxy(
                event_loop,
                &window,
                self.wake.0.clone(),
            ));
            window.set_visible(true);
            window.set_ime_allowed(false);
            self.renderer = Some(pollster::block_on(renderer::Renderer::new(window.clone())));
            self.window = Some(window);
            self.publication();
        }
        fn user_event(&mut self, _: &ActiveEventLoop, event: Event) {
            match event {
                Event::Publication => self.publication(),
                Event::Accessibility(event) => match event.window_event {
                    accesskit_winit::WindowEvent::InitialTreeRequested => {
                        self.update_accessibility()
                    }
                    accesskit_winit::WindowEvent::ActionRequested(request) => {
                        if self.window.as_ref().is_some_and(|w| w.has_focus())
                            && self.composition.preedit.is_none()
                            && self.menu.open.is_none()
                        {
                            if let Some(update) = &self.latest {
                                if let Some(command) = self.accessibility.action(request, update) {
                                    let _ = self.commands.send(command);
                                }
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if let (Some(adapter), Some(window)) = (&mut self.adapter, &self.window) {
                adapter.process_event(window, &event);
            }
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(size.width, size.height);
                    }
                    self.update_accessibility();
                    self.update_ime_geometry();
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    self.window.as_ref().unwrap().request_redraw()
                }
                WindowEvent::Focused(active) => {
                    if !active {
                        self.menu.open = None;
                        self.composition.cancel();
                        for input in self.keyboard.release_all() {
                            self.send(input);
                        }
                    }
                    let _ = self.commands.send(Command::Foreground(active, 0));
                    self.update_ime_geometry();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    if let (Some(renderer), Some(update)) = (&self.renderer, &self.latest) {
                        if let Some((w, h, _)) = &update.frame {
                            self.pointer = renderer.guest_point(position.x, position.y, *w, *h);
                            if !self.menu.move_pointer(self.pointer, update) {
                                self.send(MacintoshInput::MouseMove {
                                    vertical: self.pointer.0,
                                    horizontal: self.pointer.1,
                                });
                            } else {
                                self.window.as_ref().unwrap().request_redraw();
                            }
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(update) = &self.latest {
                        let (handled, command) =
                            self.menu
                                .button(state == ElementState::Pressed, self.pointer, update);
                        if let Some(command) = command {
                            let _ = self.commands.send(command);
                        }
                        if handled {
                            for input in self.keyboard.release_all() {
                                self.send(input);
                            }
                            self.composition.cancel();
                            self.update_ime_geometry();
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                    }
                    self.send(if state == ElementState::Pressed {
                        MacintoshInput::MouseDown {
                            vertical: self.pointer.0,
                            horizontal: self.pointer.1,
                        }
                    } else {
                        MacintoshInput::MouseUp {
                            vertical: self.pointer.0,
                            horizontal: self.pointer.1,
                        }
                    });
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    let m = modifiers.state();
                    self.modifiers = super::input::Modifiers {
                        platform: m.super_key(),
                        shift: m.shift_key(),
                        alt: m.alt_key(),
                        control: m.control_key(),
                    };
                    for input in self.keyboard.sync_host_modifiers(self.modifiers) {
                        self.send(input);
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if self.menu.open.is_some() => {
                    if event.state == ElementState::Pressed {
                        if let Some(update) = &self.latest {
                            if let Some(command) = self.menu.key(&event.logical_key, update) {
                                let _ = self.commands.send(command);
                            }
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.composition.preedit.is_some()
                        && event.logical_key == Key::Named(NamedKey::Escape) =>
                {
                    self.composition.cancel();
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. }
                    if !event.repeat && self.composition.preedit.is_none() =>
                {
                    let key = match &event.logical_key {
                        Key::Character(s) => s.to_ascii_lowercase(),
                        Key::Named(k) => match k {
                            NamedKey::Enter => "enter",
                            NamedKey::Escape => "escape",
                            NamedKey::Tab => "tab",
                            NamedKey::Space => "space",
                            NamedKey::Backspace => "backspace",
                            NamedKey::ArrowLeft => "left",
                            NamedKey::ArrowRight => "right",
                            NamedKey::ArrowUp => "up",
                            NamedKey::ArrowDown => "down",
                            _ => "",
                        }
                        .to_owned(),
                        _ => String::new(),
                    };
                    let stroke = super::input::Keystroke {
                        key,
                        key_char: event.text.map(|s| s.to_string()),
                        modifiers: self.modifiers,
                    };
                    if let Some((mac_key, character)) = super::input::guest_key(&stroke) {
                        let input = if event.state == ElementState::Pressed {
                            self.keyboard.press_host_key(mac_key, character)
                        } else {
                            self.keyboard.release_host_key(mac_key)
                        };
                        if let Some(input) = input {
                            self.send(input);
                        }
                    }
                }
                WindowEvent::Ime(event) => {
                    use winit::event::Ime;
                    match event {
                        Ime::Preedit(text, selection) => {
                            if text.is_empty() {
                                self.composition.cancel();
                            } else {
                                let (start, end) = selection.unwrap_or((text.len(), text.len()));
                                if let (Some(prefix), Some(end_prefix)) =
                                    (text.get(..start), text.get(..end))
                                {
                                    self.composition.mark(
                                        &text,
                                        prefix.encode_utf16().count()
                                            ..end_prefix.encode_utf16().count(),
                                    );
                                }
                            }
                        }
                        Ime::Commit(text) => {
                            if let Some((owner, bytes)) = self.composition.commit(&text) {
                                let _ = self.commands.send(Command::CommitText(owner, bytes));
                            } else {
                                eprintln!("[wgpu-ime] commit rejected: stale field or unsupported guest encoding");
                            }
                        }
                        Ime::Disabled => self.composition.cancel(),
                        Ime::Enabled => {}
                    }
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if let (Some(renderer), Some(update)) = (&mut self.renderer, &self.latest) {
                        if let Some((w, h, pixels)) = &update.frame {
                            let start = Instant::now();
                            let preview = comparison_preedit(update, &self.composition);
                            let mut quads = scene::controls(
                                preview.as_ref().unwrap_or(update),
                                *w,
                                *h,
                                renderer.raster(*w, *h),
                            );
                            quads.extend(scene::menu(
                                update,
                                &self.menu,
                                *w,
                                *h,
                                renderer.raster(*w, *h),
                            ));
                            if renderer.draw(*w, *h, pixels, &quads) {
                                let end = Instant::now();
                                if self.report.is_some() && self.frames.len() < 100_000 {
                                    self.frames.push(serde_json::json!({"elapsed_ms": self.started.elapsed().as_secs_f64()*1000.,
                                    "submit_cpu_ms": end.duration_since(start).as_secs_f64()*1000.,
                                    "present_call_gap_ms": self.last_frame.replace(end).map(|t|end.duration_since(t).as_secs_f64()*1000.),
                                    "event_to_acknowledged_submit_ms": update.input_receipt.filter(|(id,_)|*id>self.last_input_profiled).map(|(id,t)|{self.last_input_profiled=id;end.duration_since(t).as_secs_f64()*1000.}),
                                    "scale_factor": self.window.as_ref().unwrap().scale_factor(),
                                    "surface": renderer.size(), "guest": [w,h]}));
                                }
                            }
                        }
                    }
                    if self
                        .seconds
                        .is_some_and(|seconds| self.started.elapsed().as_secs_f64() >= seconds)
                    {
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        }
    }
}
fn main() {
    desktop::main();
}
