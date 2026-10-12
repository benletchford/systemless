//! Independent wgpu host using master's production execution owner and transport.
use clap::Parser;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
include!("desktop/keyboard_input.rs");
use desktop_save_store::DesktopSaveStore;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use systemless::runner::FixtureRunner;
#[cfg(all(feature = "debug-server", unix))]
#[path = "desktop/debug_server.rs"]
mod debug_server;
#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;
#[path = "desktop/frame_metrics.rs"]
mod frame_metrics;
#[path = "desktop/frame_snapshot.rs"]
mod frame_snapshot;
#[path = "wgpu_menu.rs"]
mod menu;
#[cfg(target_os = "macos")]
#[path = "desktop/native_bundle.rs"]
mod native_bundle;
#[path = "wgpu_renderer.rs"]
mod renderer;
#[path = "desktop/runtime_driver.rs"]
mod runtime_driver;
#[path = "desktop/runtime_mailbox.rs"]
mod runtime_mailbox;
#[path = "desktop/runtime_owner.rs"]
mod runtime_owner;
#[path = "desktop/runtime_protocol.rs"]
mod runtime_protocol;
#[path = "wgpu_scene.rs"]
mod scene;
#[cfg(not(all(feature = "debug-server", unix)))]
mod debug_server {
    pub struct DebugServer;
    impl DebugServer {
        pub fn bind(_: &std::path::Path) -> std::io::Result<Self> {
            Err(std::io::Error::other("debug-server feature disabled"))
        }
        pub fn pump(&mut self, _: &mut systemless::runner::FixtureRunner) -> usize {
            0
        }
    }
}
include!("desktop/runtime_support.rs");
#[derive(Parser)]
struct Args {
    game: PathBuf,
    #[arg(long)]
    offscreen: bool,
    #[arg(long, requires = "offscreen")]
    expect_exit: bool,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 920)]
    updates: u64,
    #[arg(long, default_value_t = 1.0)]
    scale: f64,
    #[arg(long)]
    script: Option<PathBuf>,
    #[arg(long, value_delimiter = ',')]
    capture_at: Vec<u64>,
}
fn main() {
    let args = Args::parse();
    assert!(args.scale.is_finite() && args.scale > 0. && args.scale <= 4.);
    std::fs::create_dir_all(&args.output).unwrap();
    if !args.offscreen {
        run_live(args);
        return;
    }
    let actions: Vec<serde_json::Value> = args
        .script
        .as_ref()
        .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
        .unwrap_or_default();
    let (wake, events) = mpsc::channel();
    let mut owner = runtime_owner::RuntimeOwner::spawn(
        runtime_owner::RuntimeConfig {
            #[cfg(target_os = "macos")]
            native_preflight: false,
            game_path: args.game,
            arrows_as_numpad: false,
            native_integrations: false,
            addressing_24_bit: false,
            screen_depth: Some(8),
            ui_theme: systemless::ui_theme::UiThemeId::ClassicSystem7,
            debug_socket: None,
        },
        move || {
            let _ = wake.send(());
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(renderer::Renderer::offscreen(800, 600));
    let mut guest_exit = false;
    let mut observed = 0;
    let mut next = 0;
    let mut input_delay=0u64;
    let mut samples = Vec::new();
    let mut argb = Vec::new();
    let mut menus = menu::MenuPresenter::default();
    let started = Instant::now();
    while observed < args.updates {
        events
            .recv_timeout(Duration::from_secs(30))
            .expect("guest publication timeout");
        let update = owner.mailbox.poll();
        if let runtime_mailbox::RuntimeStatus::Stopped { error, .. } = update.status {
            if args.expect_exit && error.is_none() {
                guest_exit = true;
                break;
            }
            panic!("guest stopped early: {error:?}");
        }
        if let Some(frame) = update.frame {
            observed += 1;
            let (_, _, w, h, _) = frame.screen.screen_mode;
            frame.screen.render_argb(&mut argb);
            let pixels: Vec<u8> = argb.iter().flat_map(|p| p.to_le_bytes()).collect();
            let size = [
                (f64::from(w) * args.scale).round() as u32,
                (f64::from(h) * args.scale).round() as u32,
            ];
            if renderer.size() != size {
                renderer.resize(size[0], size[1]);
            }
            menus.reconcile(&frame.menus, frame.dialog.is_some());
            let now = Instant::now();
            let mut quads = scene::controls(frame.dialog.as_ref());
            quads.extend(scene::menu(&menus));
            renderer.draw(w.into(), h.into(), &pixels, &quads);
            samples.push(now.elapsed().as_secs_f64() * 1000.);
            while let Some(a) = actions
                .get(next)
                .filter(|a| a["after_updates"].as_u64().unwrap()+input_delay <= observed)
            {
                let ready=(!a["wait_for_no_dialog"].as_bool().unwrap_or(false)||frame.dialog.is_none())
                    && a["wait_for_dialog_text"].as_str().is_none_or(|text|frame.dialog.as_ref().is_some_and(|d|d.items.iter().any(|i|i.text.contains(text))));
                if !ready {assert!(observed<a["after_updates"].as_u64().unwrap()+input_delay+600,"guest readiness timeout: {a}");break;}
                input_delay=observed-a["after_updates"].as_u64().unwrap();
                eprintln!("[comparison-input] observed={observed} action={a}");
                use runtime_protocol::GuiCommand::*;
                let n = |k: &str| a[k].as_i64().unwrap();
                let cmd = match a["type"].as_str().unwrap() {
                    "mouse_down" => MouseDown {
                        v: n("v") as i16,
                        h: n("h") as i16,
                    },
                    "mouse_up" => MouseUp {
                        v: n("v") as i16,
                        h: n("h") as i16,
                    },
                    "mouse_move" => MouseMove {
                        v: n("v") as i16,
                        h: n("h") as i16,
                    },
                    "key_down" => KeyDown {
                        key: n("key") as u8,
                        character: n("ch") as u8,
                    },
                    "key_up" => KeyUp {
                        key: n("key") as u8,
                        character: n("ch") as u8,
                    },
                    other => panic!("unsupported action {other}"),
                };
                if let Some(cmd) = menus.route(cmd, &frame.menus, frame.dialog.is_some()) {
                    owner.mailbox.send(cmd).unwrap();
                }
                next += 1;
            }
            if args.capture_at.contains(&observed) {
                renderer.save_capture(&args.output.join(format!("frame-{observed}.png")));
                std::fs::write(
                    args.output.join(format!("dialog-{observed}.json")),
                    serde_json::to_vec_pretty(&frame.dialog).unwrap(),
                )
                .unwrap();
            }
            if observed == args.updates {
                renderer.save_capture(&args.output.join("frame.png"));
                let rgba:Vec<u8>=argb.iter().flat_map(|p|[(p>>16) as u8,(p>>8) as u8,*p as u8,(p>>24) as u8]).collect();
                image::save_buffer(args.output.join("guest.png"),&rgba,w.into(),h.into(),image::ColorType::Rgba8).unwrap();
                std::fs::write(
                    args.output.join("dialog.json"),
                    serde_json::to_vec_pretty(&frame.dialog).unwrap(),
                )
                .unwrap();
            }
            owner.mailbox.recycle(frame);
        }
    }
    assert_eq!(next, actions.len(), "not all scripted inputs delivered");
    assert!(
        !args.expect_exit || guest_exit,
        "guest did not exit after menu selection"
    );
    owner.mailbox.request_shutdown();
    while !owner.join_finished().unwrap() {
        events.recv_timeout(Duration::from_secs(1)).ok();
    }
    std::fs::write(args.output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"scope":"master execution owner; headless GPU framebuffer presentation","observed":observed,"completed_actions":next,"guest_exit":guest_exit,"elapsed_ms":started.elapsed().as_secs_f64()*1000.,"submit_cpu_ms":samples,"gpu_pass_ms":renderer.gpu_samples()})).unwrap()).unwrap();
}

fn run_live(args: Args) {
    use std::sync::Arc;
    use winit::{
        application::ApplicationHandler,
        event::{ElementState, MouseButton, WindowEvent},
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };
    struct Live {
        args: Args,
        window: Option<Arc<Window>>,
        renderer: Option<renderer::Renderer>,
        owner: runtime_owner::RuntimeOwner,
        frame: Option<Box<frame_snapshot::GuiFrame>>,
        pointer: (i16, i16),
        keys: std::collections::BTreeMap<u8, u8>,
        closing: bool,
        menus: menu::MenuPresenter,
    }
    impl Live {
        fn send(&mut self, c: runtime_protocol::GuiCommand) {
            let menu_was_open=self.menus.open.is_some();
            let c = if let Some(f) = &self.frame {
                self.menus.route(c, &f.menus, f.dialog.is_some())
            } else {
                Some(c)
            };
            if !menu_was_open && self.menus.open.is_some() {
                for (key,character) in std::mem::take(&mut self.keys) {
                    let _=self.owner.mailbox.send(runtime_protocol::GuiCommand::KeyUp{key,character});
                }
            }
            let Some(c) = c else {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            };
            if let Err(e) = self.owner.mailbox.send(c) {
                eprintln!("input rejected: {e:?}");
            }
        }
        fn release(&mut self) {
            self.menus.open = None;
            self.menus.selected = 0;
            for (key, character) in std::mem::take(&mut self.keys) {
                self.send(runtime_protocol::GuiCommand::KeyUp { key, character });
            }
            self.send(runtime_protocol::GuiCommand::MouseUp {
                v: self.pointer.0,
                h: self.pointer.1,
            });
        }
    }
    impl ApplicationHandler<()> for Live {
        fn resumed(&mut self, e: &ActiveEventLoop) {
            if self.window.is_none() {
                let w = Arc::new(
                    e.create_window(
                        Window::default_attributes()
                            .with_title("Systemless · wgpu / master")
                            .with_inner_size(winit::dpi::LogicalSize::new(
                                800. * self.args.scale,
                                600. * self.args.scale,
                            )),
                    )
                    .unwrap(),
                );
                self.renderer = Some(pollster::block_on(renderer::Renderer::new(w.clone())));
                self.window = Some(w);
            }
        }
        fn user_event(&mut self, e: &ActiveEventLoop, _: ()) {
            let update = self.owner.mailbox.poll();
            if let Some(frame) = update.frame {
                self.menus.reconcile(&frame.menus, frame.dialog.is_some());
                if let Some(old) = self.frame.replace(frame) {
                    self.owner.mailbox.recycle(old);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            if let runtime_mailbox::RuntimeStatus::Stopped { error, .. } = update.status {
                if let Some(error) = error {
                    eprintln!("{error}");
                }
                e.exit();
            }
        }
        fn window_event(&mut self, e: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            use runtime_protocol::GuiCommand::*;
            match event {
                WindowEvent::CloseRequested => {
                    self.release();
                    self.closing = true;
                    self.owner.mailbox.request_shutdown();
                }
                WindowEvent::Focused(false) => self.release(),
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width, size.height);
                    }
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    if let (Some(r), Some(f)) = (&self.renderer, &self.frame) {
                        let (_, _, w, h, _) = f.screen.screen_mode;
                        self.pointer = r.guest_point(position.x, position.y, w.into(), h.into());
                        self.send(MouseMove {
                            v: self.pointer.0,
                            h: self.pointer.1,
                        });
                    }
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    self.send(if state == ElementState::Pressed {
                        MouseDown {
                            v: self.pointer.0,
                            h: self.pointer.1,
                        }
                    } else {
                        MouseUp {
                            v: self.pointer.0,
                            h: self.pointer.1,
                        }
                    });
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let Some((key, character)) = host_keyboard_event_to_mac(
                        &event.logical_key,
                        &event.physical_key,
                        event.text.as_deref(),
                    ) {
                        if event.state == ElementState::Pressed {
                            self.keys.insert(key, character);
                            self.send(KeyDown { key, character });
                        } else {
                            let character = self.keys.remove(&key).unwrap_or(character);
                            self.send(KeyUp { key, character });
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let (Some(r), Some(f)) = (&mut self.renderer, &self.frame) {
                        let (_, _, w, h, _) = f.screen.screen_mode;
                        let mut pixels = Vec::new();
                        f.screen.render_argb(&mut pixels);
                        let bytes: Vec<_> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
                        let mut quads = scene::controls(f.dialog.as_ref());
                        quads.extend(scene::menu(&self.menus));
                        r.draw(w.into(), h.into(), &bytes, &quads);
                    }
                }
                _ => {}
            }
            if self.closing && self.owner.join_finished().unwrap_or(true) {
                e.exit();
            }
        }
    }
    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let owner = runtime_owner::RuntimeOwner::spawn(
        runtime_owner::RuntimeConfig {
            #[cfg(target_os = "macos")]
            native_preflight: false,
            game_path: args.game.clone(),
            arrows_as_numpad: false,
            native_integrations: false,
            addressing_24_bit: false,
            screen_depth: Some(8),
            ui_theme: systemless::ui_theme::UiThemeId::ClassicSystem7,
            debug_socket: None,
        },
        move || {
            let _ = proxy.send_event(());
        },
    )
    .unwrap();
    let mut app = Live {
        args,
        window: None,
        renderer: None,
        owner,
        frame: None,
        pointer: (0, 0),
        keys: Default::default(),
        closing: false,
        menus: menu::MenuPresenter::default(),
    };
    event_loop.run_app(&mut app).unwrap();
}
