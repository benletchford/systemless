//! Opt-in macOS GPUI Kit experiment for live guest menus.

#[cfg(target_os = "macos")]
fn main() {
    desktop::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The GPUI menu demo currently supports macOS only.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
mod desktop {
    //! Opt-in GPUI Kit presentation experiment for live guest menus.

    use std::{
        path::PathBuf,
        sync::{mpsc, Arc, Mutex},
        time::{Duration, Instant},
    };

    use clap::Parser;
    use gpui_kit::{
        component::{
            button::{Button, ButtonVariants},
            menu::{DropdownMenu, PopupMenu, PopupMenuItem},
            ActiveTheme, Disableable, Sizable,
        },
        prelude::*,
        *,
    };
    use systemless::{
        memory::{globals::addr::MBAR_HEIGHT, MemoryBus},
        menu_model::GuestMenuSnapshot,
        systems::macintosh::session::{MacintoshInput, MacintoshSession},
    };

    #[derive(Parser)]
    #[command(about = "Experimental GPUI Kit guest menu runner (no audio or persistent saves)")]
    struct Args {
        game: PathBuf,
        #[arg(long)]
        prefer_powerpc: bool,
        #[arg(long, value_parser = parse_depth)]
        screen_depth: Option<u16>,
    }

    fn parse_depth(value: &str) -> Result<u16, String> {
        match value {
            "1" => Ok(1),
            "2" => Ok(2),
            "4" => Ok(4),
            "8" => Ok(8),
            _ => Err("expected 1, 2, 4 or 8".into()),
        }
    }

    enum Command {
        Menu(i16, i16),
        Input(MacintoshInput),
    }

    #[derive(Default)]
    struct Update {
        menus: GuestMenuSnapshot,
        frame: Option<(u32, u32, u32, Vec<u8>)>,
        status: String,
    }

    fn run_guest(
        args: Args,
        commands: mpsc::Receiver<Command>,
        updates: Arc<Mutex<Option<Update>>>,
    ) {
        let result = std::panic::catch_unwind(|| {
            let mut session = MacintoshSession::new(true, args.screen_depth);
            session
                .runner_mut()
                .set_prefer_powerpc_executables(args.prefer_powerpc);
            let app = session.load_path(&args.game)?;
            session.initialize(&app);
            let architecture = if session.status().powerpc_application {
                "PowerPC"
            } else {
                "68k"
            };
            let epoch = Instant::now();
            let initial_tick = session.runner().guest_tick();
            let mut previous = epoch;
            loop {
                let start = Instant::now();
                loop {
                    match commands.try_recv() {
                        Ok(Command::Menu(menu, item)) => {
                            session.runner_mut().select_guest_menu_item(menu, item);
                        }
                        Ok(Command::Input(input)) => session.deliver_input(input),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return Ok::<(), String>(()),
                    }
                }
                session
                    .runner_mut()
                    .advance_menu_presentation_clock(start.duration_since(previous));
                previous = start;
                let deadline =
                    initial_tick.wrapping_add((epoch.elapsed().as_secs_f64() * 60.) as u32);
                session
                    .runner_mut()
                    .run_gui_slice_with_audio(100_000, deadline, 367);
                session.drain_audio();
                let menus = session.runner_mut().guest_menu_snapshot();
                let frame = session.video_frame().map(|frame| {
                    // Preserve guest MBarHeight and crop only the displayed image,
                    // as the native menu frontend does (Inside Macintosh V, V-245).
                    let top = u32::from(session.runner().bus().read_word(MBAR_HEIGHT));
                    let top = if top < frame.height { top } else { 0 };
                    let mut pixels = frame.pixels[(top * frame.width * 4) as usize..].to_vec();
                    for pixel in pixels.chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }
                    (frame.width, frame.height - top, top, pixels)
                });
                let running = session.status().running;
                *updates.lock().unwrap() = Some(Update {
                    menus,
                    frame,
                    status: format!(
                        "{architecture} · {} · GPUI Kit menu demo",
                        if running { "Running" } else { "Guest stopped" }
                    ),
                });
                if !running {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(16).saturating_sub(start.elapsed()));
            }
        });
        let error = match result {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error,
            Err(_) => "Guest worker stopped unexpectedly; see terminal output".to_owned(),
        };
        *updates.lock().unwrap() = Some(Update {
            status: error,
            ..Default::default()
        });
    }

    struct Demo {
        commands: mpsc::Sender<Command>,
        menus: GuestMenuSnapshot,
        image: Option<Arc<RenderImage>>,
        width: u32,
        height: u32,
        crop_top: u32,
        status: String,
        focus: FocusHandle,
        _poll: Task<()>,
    }

    impl Demo {
        fn new(
            commands: mpsc::Sender<Command>,
            updates: Arc<Mutex<Option<Update>>>,
            cx: &mut Context<Self>,
        ) -> Self {
            let poll = cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let update = updates.lock().unwrap().take();
                if this
                    .update(cx, |this, cx| {
                        if let Some(update) = update {
                            this.menus = update.menus;
                            this.status = update.status;
                            if let Some((width, height, top, pixels)) = update.frame {
                                this.width = width;
                                this.height = height;
                                this.crop_top = top;
                                if this.image.as_ref().is_none_or(|image| {
                                    image.as_bytes(0) != Some(pixels.as_slice())
                                        || image.size(0).width.0 != width as i32
                                        || image.size(0).height.0 != height as i32
                                }) {
                                    let buffer =
                                        image::RgbaImage::from_raw(width, height, pixels).unwrap();
                                    if let Some(old) =
                                        this.image.replace(Arc::new(RenderImage::new(vec![
                                            image::Frame::new(buffer),
                                        ])))
                                    {
                                        cx.drop_image(old, None);
                                    }
                                }
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            });
            Self {
                commands,
                menus: Default::default(),
                image: None,
                width: 640,
                height: 460,
                crop_top: 20,
                status: "Loading guest…".into(),
                focus: cx.focus_handle(),
                _poll: poll,
            }
        }

        fn pointer(&self, position: Point<Pixels>) -> (i16, i16) {
            // The framebuffer is displayed at 1:1 beneath the fixed-height bar.
            let x = f32::from(position.x).clamp(0., self.width.saturating_sub(1) as f32);
            let y = (f32::from(position.y) - 36.).clamp(0., self.height.saturating_sub(1) as f32);
            ((y as u32 + self.crop_top) as i16, x as i16)
        }
    }

    fn populate_menu(
        mut popup: PopupMenu,
        snapshot: &GuestMenuSnapshot,
        id: i16,
        commands: &mpsc::Sender<Command>,
        ancestors: &[i16],
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        if ancestors.contains(&id) {
            return popup;
        }
        let Some(menu) = snapshot.menus.iter().find(|menu| menu.id == id) else {
            return popup;
        };
        let mut ancestors = ancestors.to_vec();
        ancestors.push(id);
        for item in &menu.items {
            if item.separator {
                popup = popup.separator();
                continue;
            }
            if let Some(submenu_id) = item.submenu_id {
                let submenu = PopupMenu::build(window, cx, |popup, window, cx| {
                    populate_menu(
                        popup, snapshot, submenu_id, commands, &ancestors, window, cx,
                    )
                });
                let enabled = item.enabled
                    && snapshot
                        .menus
                        .iter()
                        .any(|m| m.id == submenu_id && m.enabled);
                popup = popup
                    .item(PopupMenuItem::submenu(item.text.clone(), submenu).disabled(!enabled));
            } else {
                let commands = commands.clone();
                let number = item.number;
                let label = match item.key_equivalent {
                    Some(key) => format!("{}    ⌘{}", item.text, key.to_uppercase()),
                    None => item.text.clone(),
                };
                popup = popup.item(
                    PopupMenuItem::new(label)
                        .checked(item.checked)
                        .disabled(!menu.enabled || !item.enabled)
                        .on_click(move |_, _, _| {
                            let _ = commands.send(Command::Menu(id, number));
                        }),
                );
            }
        }
        popup
    }

    impl Render for Demo {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut bar = div()
                .flex()
                .items_center()
                .h(px(36.))
                .w_full()
                .flex_shrink_0()
                .bg(cx.theme().background)
                .border_b_1()
                .border_color(cx.theme().border);
            for menu in self.menus.menus.iter().filter(|m| m.visible_in_menu_bar) {
                let snapshot = self.menus.clone();
                let commands = self.commands.clone();
                let id = menu.id;
                bar = bar.child(
                    Button::new(("guest-menu", id as u16 as usize))
                        .label(menu.title.clone())
                        .ghost()
                        .small()
                        .disabled(!menu.enabled)
                        .dropdown_menu(move |popup, window, cx| {
                            populate_menu(
                                popup.scrollable(true).max_h(px(420.)),
                                &snapshot,
                                id,
                                &commands,
                                &[],
                                window,
                                cx,
                            )
                        }),
                );
            }
            let mut screen = div()
                .id("guest-screen")
                .w(px(self.width as f32))
                .h(px(self.height as f32))
                .flex_shrink_0()
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, _| {
                    let (vertical, horizontal) = this.pointer(event.position);
                    let _ = this
                        .commands
                        .send(Command::Input(MacintoshInput::MouseMove {
                            vertical,
                            horizontal,
                        }));
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.focus.focus(window, cx);
                        let (vertical, horizontal) = this.pointer(event.position);
                        let _ = this
                            .commands
                            .send(Command::Input(MacintoshInput::MouseDown {
                                vertical,
                                horizontal,
                            }));
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseUpEvent, _, _| {
                        let (vertical, horizontal) = this.pointer(event.position);
                        let _ = this.commands.send(Command::Input(MacintoshInput::MouseUp {
                            vertical,
                            horizontal,
                        }));
                    }),
                );
            if let Some(image) = &self.image {
                screen = screen.child(img(image.clone()).size_full());
            }
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.modifiers.platform {
                        if let Some(key) = event.keystroke.key.chars().next() {
                            for menu in &this.menus.menus {
                                if !menu.enabled {
                                    continue;
                                }
                                if let Some(item) = menu.items.iter().find(|item| {
                                    item.enabled
                                        && !item.separator
                                        && item.submenu_id.is_none()
                                        && item
                                            .key_equivalent
                                            .is_some_and(|k| k.eq_ignore_ascii_case(&key))
                                }) {
                                    let _ = this.commands.send(Command::Menu(menu.id, item.number));
                                    cx.stop_propagation();
                                    return;
                                }
                            }
                        }
                    }
                    if let Some((mac_key, character)) = basic_key(&event.keystroke.key) {
                        let _ = this.commands.send(Command::Input(MacintoshInput::KeyDown {
                            mac_key,
                            character,
                        }));
                    }
                }))
                .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                    if let Some((mac_key, character)) = basic_key(&event.keystroke.key) {
                        let _ = this
                            .commands
                            .send(Command::Input(MacintoshInput::KeyUp { mac_key, character }));
                    }
                }))
                .child(bar)
                .child(screen)
                .child(div().px_3().py_1().text_xs().child(self.status.clone()))
        }
    }

    fn basic_key(key: &str) -> Option<(u8, u8)> {
        match key {
            "enter" => Some((0x24, 13)),
            "escape" => Some((0x35, 27)),
            "space" => Some((0x31, 32)),
            "tab" => Some((0x30, 9)),
            "backspace" => Some((0x33, 8)),
            _ => None,
        }
    }

    pub(super) fn main() {
        let args = Args::parse();
        let (commands, receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let worker_updates = updates.clone();
        std::thread::spawn(move || run_guest(args, receiver, worker_updates));
        gpui_kit::application()
            .with_assets(gpui_kit::assets::Assets)
            .run(move |cx| {
                gpui_kit::init(cx);
                cx.on_window_closed(|cx, _| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(900.), px(740.)),
                            cx,
                        ))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Systemless · GPUI menu demo".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        let view = cx.new(|cx| Demo::new(commands, updates, cx));
                        let focus = view.read(cx).focus.clone();
                        focus.focus(window, cx);
                        view
                    },
                )
                .expect("open GPUI demo window");
                cx.activate(true);
            });
    }

    #[cfg(test)]
    mod tests {
        use super::{MacintoshSession, PathBuf};

        fn wait_for_menu(
            session: &mut MacintoshSession,
            menu_id: i16,
            item_number: i16,
            checked: bool,
        ) {
            for _ in 0..300 {
                session.runner_mut().run_steps(100_000, None);
                let snapshot = session.runner_mut().guest_menu_snapshot();
                if snapshot.menus.iter().any(|menu| {
                    menu.id == menu_id
                        && menu
                            .items
                            .iter()
                            .any(|item| item.number == item_number && item.checked == checked)
                }) {
                    return;
                }
                assert!(
                    session.status().running,
                    "guest halted before updating menu"
                );
            }
            panic!("menu {menu_id} item {item_number} did not reach checked={checked}");
        }

        #[test]
        fn live_menu_bridge_updates_pages_and_nested_checks_across_guest_modes() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                assert_eq!(session.status().powerpc_application, powerpc);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(!session.runner_mut().select_guest_menu_item(129, 99));
                assert!(session.runner_mut().select_guest_menu_item(129, 2));
                wait_for_menu(&mut session, 129, 2, true);
                wait_for_menu(&mut session, 129, 1, false);
                assert!(session.runner_mut().select_guest_menu_item(140, 3));
                wait_for_menu(&mut session, 140, 3, true);
                let frame = session.video_frame().unwrap();
                assert_eq!(
                    frame.pixels.len(),
                    (frame.width * frame.height * 4) as usize
                );
                assert!(frame
                    .pixels
                    .chunks_exact(4)
                    .any(|p| p != &frame.pixels[..4]));
            }
        }
    }
}
