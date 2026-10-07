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
#[path = "gpui_demo_frames.rs"]
mod frames;

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
            checkbox::Checkbox,
            menu::{DropdownMenu, PopupMenu, PopupMenuItem},
            radio::Radio,
            ActiveTheme, Disableable, Sizable,
        },
        prelude::*,
        *,
    };
    use systemless::{
        memory::{globals::addr::MBAR_HEIGHT, MemoryBus},
        menu_model::GuestMenuSnapshot,
        runner::{ControlSnapshot, DialogItemKind, DialogSnapshot, WindowFrameSnapshot},
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
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_about_alert: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls_changed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls_dragged: Option<PathBuf>,
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
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
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
                        Ok(Command::Input(input)) => {
                            session.deliver_input(input);
                            // Tracking calls must observe the held button before
                            // a queued release clears it (Inside Macintosh I, I-288).
                            if matches!(input, MacintoshInput::MouseDown { .. }) {
                                break;
                            }
                        }
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
                let windows = session.runner_mut().window_frame_snapshot();
                let dialogs = session.runner_mut().dialog_snapshot();
                let controls = session.runner_mut().control_snapshot();
                *updates.lock().unwrap() = Some(Update {
                    menus,
                    windows,
                    dialogs,
                    controls,
                    frame,
                    status: format!(
                        "{architecture} · {} · GPUI Kit UI demo",
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
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
        image: Option<Arc<RenderImage>>,
        logo: Arc<RenderImage>,
        width: u32,
        height: u32,
        crop_top: u32,
        status: String,
        focus: FocusHandle,
        mouse_down: bool,
        mouse_position: (i16, i16),
        scrollbar_drag: Option<(u32, u64, (i16, i16))>,
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
                            this.windows = update.windows;
                            this.dialogs = update.dialogs;
                            this.controls = update.controls;
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
                windows: Vec::new(),
                dialogs: Vec::new(),
                controls: Vec::new(),
                image: None,
                logo: Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::load_from_memory(include_bytes!("../../www/assets/icons/icon-192.png"))
                        .expect("decode Systemless logo")
                        .to_rgba8(),
                )])),
                width: 640,
                height: 460,
                crop_top: 20,
                status: "Loading guest…".into(),
                focus: cx.focus_handle(),
                mouse_down: false,
                mouse_position: (0, 0),
                scrollbar_drag: None,
                _poll: poll,
            }
        }

        fn pointer(&self, position: Point<Pixels>) -> (i16, i16) {
            // The framebuffer is displayed at 1:1 beneath the fixed-height bar.
            let x = f32::from(position.x).clamp(0., self.width.saturating_sub(1) as f32);
            let y = (f32::from(position.y) - 36.).clamp(0., self.height.saturating_sub(1) as f32);
            ((y as u32 + self.crop_top) as i16, x as i16)
        }

        fn scrollbar_at(&self, point: (i16, i16)) -> Option<(u32, u64, (i16, i16))> {
            let viewport = super::frames::Rect {
                top: self.crop_top as i32,
                left: 0,
                bottom: (self.crop_top + self.height) as i32,
                right: self.width as i32,
            };
            super::frames::control_pieces(&self.controls, &self.windows, viewport)
                .into_iter()
                .find_map(|piece| {
                    let control = &self.controls[piece.control];
                    if control.proc_id != 16
                        || !control.enabled
                        || control.minimum >= control.maximum
                    {
                        return None;
                    }
                    let p = (i32::from(point.0), i32::from(point.1));
                    if p.0 < piece.clip.top
                        || p.0 >= piece.clip.bottom
                        || p.1 < piece.clip.left
                        || p.1 >= piece.clip.right
                    {
                        return None;
                    }
                    let geometry = super::frames::scrollbar_geometry(control);
                    let source = piece.source;
                    let axis = if geometry.vertical {
                        p.0 - source.top
                    } else {
                        p.1 - source.left
                    };
                    (axis >= geometry.thumb_start
                        && axis < geometry.thumb_start + geometry.thumb_extent)
                        .then_some((control.guest_id, control.generation, point))
                })
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

    fn standard_dbox_dialog<'a>(
        dialogs: &'a [DialogSnapshot],
        windows: &[WindowFrameSnapshot],
    ) -> Option<&'a DialogSnapshot> {
        dialogs.iter().find(|dialog| {
            dialog.visible
                && dialog.active
                && windows.iter().any(|frame| {
                    frame.guest_id == dialog.guest_id
                        && frame.generation == dialog.generation
                        && frame.definition_id == Some(1)
                })
                && !dialog.items.is_empty()
                && dialog.items.iter().all(|item| {
                    matches!(item.kind, DialogItemKind::Button | DialogItemKind::StaticText)
                })
        })
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
            bar = bar.child(
                div()
                    .ml(px(10.))
                    .mr(px(2.))
                    .w(px(20.))
                    .h(px(20.))
                    .child(img(self.logo.clone()).size_full()),
            );
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
                .relative()
                .overflow_hidden()
                .w(px(self.width as f32))
                .h(px(self.height as f32))
                .flex_shrink_0()
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    let (vertical, horizontal) = this.pointer(event.position);
                    this.mouse_position = (vertical, horizontal);
                    if this.scrollbar_drag.is_some() {
                        cx.notify();
                    }
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
                        this.mouse_down = true;
                        this.focus.focus(window, cx);
                        let (vertical, horizontal) = this.pointer(event.position);
                        this.mouse_position = (vertical, horizontal);
                        this.scrollbar_drag = this.scrollbar_at((vertical, horizontal));
                        cx.notify();
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
                    cx.listener(|this, event: &MouseUpEvent, _, cx| {
                        this.mouse_down = false;
                        this.scrollbar_drag = None;
                        cx.notify();
                        let (vertical, horizontal) = this.pointer(event.position);
                        let _ = this.commands.send(Command::Input(MacintoshInput::MouseUp {
                            vertical,
                            horizontal,
                        }));
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseUpEvent, _, cx| {
                        if this.mouse_down {
                            this.mouse_down = false;
                            this.scrollbar_drag = None;
                            cx.notify();
                            let (vertical, horizontal) = this.pointer(event.position);
                            let _ = this.commands.send(Command::Input(MacintoshInput::MouseUp {
                                vertical,
                                horizontal,
                            }));
                        }
                    }),
                );
            if let Some(image) = &self.image {
                screen = screen.child(img(image.clone()).absolute().top_0().left_0().size_full());
            }
            let viewport = super::frames::Rect {
                top: self.crop_top as i32,
                left: 0,
                bottom: (self.crop_top + self.height) as i32,
                right: self.width as i32,
            };
            for piece in super::frames::frame_pieces(&self.windows, viewport) {
                let frame = &self.windows[piece.window];
                let source = piece.source;
                let clip = piece.clip;
                let mut strip = div()
                    .absolute()
                    .left(px((source.left - clip.left) as f32))
                    .top(px((source.top - clip.top) as f32))
                    .w(px(source.width() as f32))
                    .h(px(source.height() as f32))
                    .bg(cx.theme().border);
                if piece.title {
                    let foreground = if frame.window.active {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    };
                    strip = strip
                        .bg(if frame.window.active {
                            cx.theme().secondary
                        } else {
                            cx.theme().background
                        })
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_color(foreground)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .child(
                            div()
                                .px(px(26.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(frame.window.title.clone()),
                        );
                    // Keep controls over the standard WDEF hit cells. Input still
                    // reaches FindWindow/TrackGoAway/DragWindow in the guest.
                    // Inside Macintosh I, I-287--I-289.
                    if frame.close_box
                        && frame.window.active
                        && matches!(frame.definition_id, Some(0 | 4 | 8 | 12 | 16))
                    {
                        let (v, h) = self.mouse_position;
                        let close_pressed = self.mouse_down
                            && i32::from(v) >= source.top
                            && v < frame.window.bounds.0
                            && h >= frame.window.bounds.1
                            && i32::from(h) < i32::from(frame.window.bounds.1) + 18;
                        strip = strip.child(
                            div()
                                .absolute()
                                .left(px((i32::from(frame.window.bounds.1) - source.left) as f32))
                                .top_0()
                                .w(px(18.))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(close_pressed, |control| control.bg(cx.theme().accent))
                                .child("×"),
                        );
                    }
                    if frame.window.active && matches!(frame.definition_id, Some(8 | 12)) {
                        strip = strip.child(
                            div()
                                .absolute()
                                .left(px(
                                    (i32::from(frame.window.bounds.3) - 15 - source.left) as f32
                                ))
                                .top_0()
                                .w(px(15.))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("□"),
                        );
                    }
                }
                screen = screen.child(
                    div()
                        .absolute()
                        .overflow_hidden()
                        .left(px(clip.left as f32))
                        .top(px((clip.top - self.crop_top as i32) as f32))
                        .w(px(clip.width() as f32))
                        .h(px(clip.height() as f32))
                        .child(strip),
                );
            }
            for piece in super::frames::gutter_pieces(&self.windows, viewport) {
                let source = piece.source;
                let clip = piece.clip;
                let mut gutter = div()
                    .absolute()
                    .left(px((source.left - clip.left) as f32))
                    .top(px((source.top - clip.top) as f32))
                    .w(px(source.width() as f32))
                    .h(px(source.height() as f32))
                    .bg(cx.theme().secondary)
                    .border_color(cx.theme().border);
                gutter = match piece.kind {
                    super::frames::GutterKind::Vertical => gutter.border_l_1(),
                    super::frames::GutterKind::Horizontal => gutter.border_t_1(),
                    super::frames::GutterKind::GrowBox => {
                        let mut corner = gutter.border_l_1().border_t_1();
                        if self.windows[piece.window].window.active {
                            for (left, top) in [
                                (4., 10.),
                                (7., 7.),
                                (7., 10.),
                                (10., 4.),
                                (10., 7.),
                                (10., 10.),
                            ] {
                                corner = corner.child(
                                    div()
                                        .absolute()
                                        .left(px(left))
                                        .top(px(top))
                                        .w(px(2.))
                                        .h(px(2.))
                                        .bg(cx.theme().muted_foreground),
                                );
                            }
                        }
                        corner
                    }
                };
                screen = screen.child(
                    div()
                        .absolute()
                        .overflow_hidden()
                        .left(px(clip.left as f32))
                        .top(px((clip.top - self.crop_top as i32) as f32))
                        .w(px(clip.width() as f32))
                        .h(px(clip.height() as f32))
                        .child(gutter),
                );
            }
            // CDEF-owned standard controls can use Kit components while their
            // ControlRecord state and tracking remain guest-owned.
            // Macintosh Toolbox Essentials (1992), pp. 5-58--5-64.
            for piece in super::frames::control_pieces(&self.controls, &self.windows, viewport) {
                let control = &self.controls[piece.control];
                let source = piece.source;
                let clip = piece.clip;
                let mut overlay = div()
                    .absolute()
                    .left(px((source.left - clip.left) as f32))
                    .top(px((source.top - clip.top) as f32))
                    .w(px(source.width() as f32))
                    .h(px(source.height() as f32))
                    .bg(cx.theme().background);
                match control.proc_id {
                    0 => {
                        overlay = overlay.child(
                            Button::new(format!(
                                "guest-control-button-{}-{}",
                                control.guest_id, control.generation
                            ))
                                .label(control.title.clone())
                                .small()
                                .compact()
                                .tab_stop(false)
                                .disabled(!control.enabled)
                                .w_full()
                                .h_full(),
                        );
                    }
                    1 => {
                        overlay = overlay.child(
                            Checkbox::new(format!(
                                "guest-control-checkbox-{}-{}",
                                control.guest_id, control.generation
                            ))
                                .label(control.title.clone())
                                .checked(control.value != 0)
                                .disabled(!control.enabled)
                                .tab_stop(false)
                                .small()
                                .w_full()
                                .h_full(),
                        );
                    }
                    2 => {
                        overlay = overlay.child(
                            Radio::new(format!(
                                "guest-control-radio-{}-{}",
                                control.guest_id, control.generation
                            ))
                                .label(control.title.clone())
                                .checked(control.value != 0)
                                .disabled(!control.enabled)
                                .tab_stop(false)
                                .small()
                                .w_full()
                                .h_full(),
                        );
                    }
                    16 => {
                        let geometry = super::frames::scrollbar_geometry(control);
                        let active = control.enabled && control.minimum < control.maximum;
                        let arrow = geometry.arrow_extent as f32;
                        let (arrow_width, arrow_height, end_left, end_top) = if geometry.vertical {
                            (
                                source.width() as f32,
                                arrow,
                                0.,
                                source.height() as f32 - arrow,
                            )
                        } else {
                            (
                                arrow,
                                source.height() as f32,
                                source.width() as f32 - arrow,
                                0.,
                            )
                        };
                        let (thumb_left, thumb_top, thumb_width, thumb_height) =
                            if geometry.vertical {
                                (
                                    0.,
                                    geometry.thumb_start as f32,
                                    source.width() as f32,
                                    geometry.thumb_extent as f32,
                                )
                            } else {
                                (
                                    geometry.thumb_start as f32,
                                    0.,
                                    geometry.thumb_extent as f32,
                                    source.height() as f32,
                                )
                            };
                        let (before, after) = if geometry.vertical {
                            ("▴", "▾")
                        } else {
                            ("◂", "▸")
                        };
                        overlay = overlay
                            .bg(cx.theme().secondary)
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .w(px(arrow_width))
                                    .h(px(arrow_height))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(if active {
                                        cx.theme().foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(before),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(end_left))
                                    .top(px(end_top))
                                    .w(px(arrow_width))
                                    .h(px(arrow_height))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(if active {
                                        cx.theme().foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(after),
                            );
                        if active && geometry.thumb_extent > 0 {
                            overlay = overlay.child(
                                div()
                                    .absolute()
                                    .left(px(thumb_left))
                                    .top(px(thumb_top))
                                    .w(px(thumb_width))
                                    .h(px(thumb_height))
                                    .bg(cx.theme().accent)
                                    .border_1()
                                    .border_color(cx.theme().border),
                            );
                            if let Some((id, generation, start)) = self.scrollbar_drag {
                                if control.guest_id == id && control.generation == generation {
                                    if let Some(position) = super::frames::scrollbar_drag_outline(
                                        control,
                                        start,
                                        self.mouse_position,
                                    ) {
                                        let (left, top) = if geometry.vertical {
                                            (0., position as f32)
                                        } else {
                                            (position as f32, 0.)
                                        };
                                        overlay = overlay.child(
                                            div()
                                                .absolute()
                                                .left(px(left))
                                                .top(px(top))
                                                .w(px(thumb_width))
                                                .h(px(thumb_height))
                                                .border_2()
                                                .border_color(cx.theme().foreground),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => unreachable!(),
                }
                screen = screen.child(
                    div()
                        .absolute()
                        .overflow_hidden()
                        .left(px(clip.left as f32))
                        .top(px((clip.top - self.crop_top as i32) as f32))
                        .w(px(clip.width() as f32))
                        .h(px(clip.height() as f32))
                        .child(overlay),
                );
            }
            // dBoxProc dialogs with only standard DITL text and buttons can be
            // restyled without covering application-owned user items. Their
            // guest bounds and event handling remain authoritative.
            // Macintosh Toolbox Essentials (1992), pp. 6-13--6-15, 6-120.
            if let Some(dialog) = standard_dbox_dialog(&self.dialogs, &self.windows) {
                for item in &dialog.items {
                    if !item.visible {
                        continue;
                    }
                    let item_rect = super::frames::Rect::from(item.bounds);
                    // The Dialog Manager draws the default button outline
                    // outside its DITL rectangle. Cover those pixels too.
                    let source = if item.kind == DialogItemKind::Button
                        && dialog.default_item == Some(item.number)
                    {
                        super::frames::Rect {
                            top: item_rect.top - 4,
                            left: item_rect.left - 4,
                            bottom: item_rect.bottom + 4,
                            right: item_rect.right + 4,
                        }
                    } else {
                        item_rect
                    };
                    let Some(clip) = source
                        .intersection(dialog.bounds.into())
                        .and_then(|rect| rect.intersection(viewport))
                    else {
                        continue;
                    };
                    let mut overlay = div()
                        .absolute()
                        .left(px((source.left - clip.left) as f32))
                        .top(px((source.top - clip.top) as f32))
                        .w(px(source.width() as f32))
                        .h(px(source.height() as f32))
                        .bg(cx.theme().background);
                    overlay = match item.kind {
                        DialogItemKind::Button => overlay.child(
                            Button::new(format!(
                                "guest-dialog-button-{}-{}-{}",
                                dialog.guest_id, dialog.generation, item.number
                            ))
                            .label(item.text.clone())
                            .small()
                            .compact()
                            .tab_stop(false)
                            .disabled(!item.enabled)
                            .absolute()
                            .left(px((item_rect.left - source.left) as f32))
                            .top(px((item_rect.top - source.top) as f32))
                            .w(px(item_rect.width() as f32))
                            .h(px(item_rect.height() as f32)),
                        ),
                        DialogItemKind::StaticText => overlay
                            .text_size(px(13.))
                            .text_color(cx.theme().foreground)
                            .child(item.text.replace('\r', "\n")),
                        _ => unreachable!(),
                    };
                    screen = screen.child(
                        div()
                            .absolute()
                            .overflow_hidden()
                            .left(px(clip.left as f32))
                            .top(px((clip.top - self.crop_top as i32) as f32))
                            .w(px(clip.width() as f32))
                            .h(px(clip.height() as f32))
                            .child(overlay),
                    );
                }
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

    #[cfg(feature = "gpui-demo-test")]
    fn capture_fixture_screen(
        game: &std::path::Path,
        output: &std::path::Path,
        prefer_powerpc: bool,
        screen_depth: Option<u16>,
        controls_page: bool,
        controls_changed: bool,
        controls_dragged: bool,
    ) {
        use gpui_kit::{platform, VisualTestAppContext};

        let mut session = MacintoshSession::new(true, screen_depth.or(Some(8)));
        session
            .runner_mut()
            .set_prefer_powerpc_executables(prefer_powerpc);
        let app = session.load_path(game).unwrap();
        session.initialize(&app);
        for _ in 0..300 {
            session.runner_mut().run_steps(100_000, None);
            if session
                .runner_mut()
                .guest_menu_snapshot()
                .menus
                .iter()
                .any(|menu| menu.id == 129 && !menu.items.is_empty())
            {
                break;
            }
        }
        let (menu_id, item) = if controls_page { (129, 2) } else { (128, 1) };
        assert!(session.runner_mut().select_guest_menu_item(menu_id, item));
        let dialogs = if controls_page {
            for _ in 0..300 {
                session.runner_mut().run_steps(100_000, None);
                if session
                    .runner_mut()
                    .control_snapshot()
                    .iter()
                    .any(|control| control.visible && control.title == "Checkbox")
                {
                    break;
                }
            }
            assert!(session
                .runner_mut()
                .control_snapshot()
                .iter()
                .any(|control| control.visible && control.proc_id == 16));
            Vec::new()
        } else {
            (0..300)
                .find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let dialogs = session.runner_mut().dialog_snapshot();
                    dialogs.iter().any(|dialog| dialog.visible).then_some(dialogs)
                })
                .expect("About alert should become visible")
        };
        let windows = session.runner_mut().window_frame_snapshot();
        if !controls_page {
            assert!(standard_dbox_dialog(&dialogs, &windows).is_some());
        }
        if controls_changed {
            let controls = session.runner_mut().control_snapshot();
            let checkbox = controls
                .iter()
                .find(|control| control.visible && control.title == "Checkbox")
                .unwrap();
            let bar = controls
                .iter()
                .find(|control| control.visible && control.proc_id == 16)
                .unwrap();
            let checkbox_point = (
                (checkbox.bounds.0 + checkbox.bounds.2) / 2,
                (checkbox.bounds.1 + checkbox.bounds.3) / 2,
            );
            let bar_point = ((bar.bounds.0 + bar.bounds.2) / 2, bar.bounds.3 - 8);
            for (vertical, horizontal) in [checkbox_point, bar_point] {
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical,
                    horizontal,
                });
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical,
                    horizontal,
                });
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            let controls = session.runner_mut().control_snapshot();
            assert!(controls.iter().any(|control| {
                control.visible && control.title == "Checkbox" && control.value == 1
            }));
            assert!(controls
                .iter()
                .any(|control| control.visible && control.proc_id == 16 && control.value > 0));
        }
        if controls_dragged {
            let controls = session.runner_mut().control_snapshot();
            let bar = controls
                .iter()
                .find(|control| control.visible && control.proc_id == 16)
                .unwrap();
            let thumb = super::frames::scrollbar_geometry(bar);
            let from = (
                (bar.bounds.0 + bar.bounds.2) / 2,
                bar.bounds.1 + thumb.thumb_start as i16 + 8,
            );
            let to = (from.0, bar.bounds.3 - 28);
            for input in [
                MacintoshInput::MouseDown {
                    vertical: from.0,
                    horizontal: from.1,
                },
                MacintoshInput::MouseMove {
                    vertical: to.0,
                    horizontal: to.1,
                },
                MacintoshInput::MouseUp {
                    vertical: to.0,
                    horizontal: to.1,
                },
            ] {
                session.deliver_input(input);
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            assert!(session
                .runner_mut()
                .control_snapshot()
                .iter()
                .any(|control| control.guest_id == bar.guest_id && control.value >= 8));
        }
        let controls = session.runner_mut().control_snapshot();
        let menus = session.runner_mut().guest_menu_snapshot();
        let frame = session.video_frame().unwrap();
        let top = u32::from(session.runner().bus().read_word(MBAR_HEIGHT));
        let mut pixels = frame.pixels[(top * frame.width * 4) as usize..].to_vec();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let frame_height = frame.height - top;

        let mut visual = VisualTestAppContext::with_asset_source(
            platform::current_platform(true),
            Arc::new(gpui_kit::assets::Assets),
        );
        visual.update(gpui_kit::init);
        let (sender, _receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let mut view = None;
        let window = visual
            .open_offscreen_window(size(px(900.), px(740.)), |_, cx| {
                let entity = cx.new(|cx| Demo::new(sender, updates, cx));
                view = Some(entity.clone());
                entity
            })
            .unwrap();
        let view = view.unwrap();
        visual.update(|cx| {
            view.update(cx, |demo, cx| {
                demo.menus = menus;
                demo.windows = windows;
                demo.dialogs = dialogs;
                demo.controls = controls;
                demo.width = frame.width;
                demo.height = frame_height;
                demo.crop_top = top;
                demo.status = format!(
                    "{} · Running · GPUI Kit UI demo",
                    if prefer_powerpc { "PowerPC" } else { "68k" }
                );
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::from_raw(frame.width, frame_height, pixels).unwrap(),
                )])));
                cx.notify();
            });
        });
        visual.run_until_parked();
        visual
            .capture_screenshot(window.into())
            .unwrap()
            .save(output)
            .unwrap();
        eprintln!("saved composed GPUI capture to {}", output.display());
    }

    pub(super) fn main() {
        let args = Args::parse();
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_about_alert.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                false,
                false,
                false,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_controls.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                true,
                false,
                false,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_controls_changed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                true,
                true,
                false,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_controls_dragged.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                true,
                false,
                true,
            );
            return;
        }
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
                            title: Some("Systemless · GPUI UI demo".into()),
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
        use super::{MacintoshInput, MacintoshSession, PathBuf};

        #[test]
        fn worker_preserves_rapid_close_button_press_and_release() {
            use super::{run_guest, Args, Command, Update};
            use std::{
                sync::{mpsc, Arc, Mutex},
                time::{Duration, Instant},
            };
            fn wait(updates: &Mutex<Option<Update>>, ready: impl Fn(&Update) -> bool) -> Update {
                let start = Instant::now();
                loop {
                    if let Some(update) = updates.lock().unwrap().take() {
                        if ready(&update) {
                            return update;
                        }
                    }
                    assert!(
                        start.elapsed() < Duration::from_secs(30),
                        "worker did not reach expected window state"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let updates = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            let worker_updates = updates.clone();
            let worker = std::thread::spawn(move || {
                run_guest(
                    Args {
                        game: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                        prefer_powerpc: false,
                        screen_depth: Some(8),
                        capture_about_alert: None,
                        capture_controls: None,
                        capture_controls_changed: None,
                        capture_controls_dragged: None,
                    },
                    rx,
                    worker_updates,
                )
            });
            wait(&updates, |u| u.menus.menus.iter().any(|m| m.id == 129));
            tx.send(Command::Menu(129, 3)).unwrap();
            let update = wait(&updates, |u| u.windows.len() == 3);
            let (top, left, _, _) = update.windows[0].window.bounds;
            tx.send(Command::Input(MacintoshInput::MouseDown {
                vertical: top - 9,
                horizontal: left + 9,
            }))
            .unwrap();
            tx.send(Command::Input(MacintoshInput::MouseUp {
                vertical: top - 9,
                horizontal: left + 9,
            }))
            .unwrap();
            let update = wait(&updates, |u| u.windows.len() == 2);
            assert_eq!(update.windows[0].window.title, "Auxiliary Window");
            drop(tx);
            worker.join().unwrap();
        }

        fn settle(session: &mut MacintoshSession) {
            let start = session.runner().guest_tick();
            for _ in 0..100 {
                session.runner_mut().run_steps(10_000, None);
                if session.runner().guest_tick().wrapping_sub(start) >= 2 {
                    return;
                }
            }
            panic!("guest did not advance while tracking input");
        }

        fn check_frames(session: &mut MacintoshSession) {
            assert!(session.runner_mut().select_guest_menu_item(129, 3));
            wait_for_menu(session, 129, 3, true);
            for _ in 0..100 {
                settle(session);
                if session.runner_mut().window_frame_snapshot().len() == 3 {
                    break;
                }
            }
            settle(session);
            let before = session.runner_mut().window_frame_snapshot();
            assert_eq!(before.len(), 3);
            assert_eq!(before[0].window.title, "Stacked Inspector");
            assert_ne!(before[0].guest_id, 0);
            assert!(before.iter().all(|frame| frame.generation != 0));
            assert!(before.iter().all(|frame| {
                before
                    .iter()
                    .filter(|other| other.guest_id == frame.guest_id)
                    .count()
                    == 1
            }));
            assert_eq!(before[0].definition_id, Some(8));
            assert!(before[0].close_box && before[0].window.active);
            let (top, left, bottom, right) = before[0].window.bounds;
            let from = (top - 9, (left + right) / 2);
            session.deliver_input(MacintoshInput::MouseDown {
                vertical: from.0,
                horizontal: from.1,
            });
            settle(session);
            session.deliver_input(MacintoshInput::MouseMove {
                vertical: from.0 + 12,
                horizontal: from.1 + 16,
            });
            settle(session);
            session.deliver_input(MacintoshInput::MouseUp {
                vertical: from.0 + 12,
                horizontal: from.1 + 16,
            });
            settle(session);
            let moved = session.runner_mut().window_frame_snapshot();
            assert_eq!(moved[0].guest_id, before[0].guest_id);
            assert_eq!(moved[0].generation, before[0].generation);
            assert_eq!(
                moved[0].window.bounds,
                (top + 12, left + 16, bottom + 12, right + 16)
            );
            assert_eq!(
                moved[0].window.structure_bounds.unwrap().0,
                moved[0].window.bounds.0 - 19
            );
            let close = (moved[0].window.bounds.0 - 9, moved[0].window.bounds.1 + 9);
            session.deliver_input(MacintoshInput::MouseDown {
                vertical: close.0,
                horizontal: close.1,
            });
            settle(session);
            session.deliver_input(MacintoshInput::MouseUp {
                vertical: close.0,
                horizontal: close.1,
            });
            settle(session);
            let closed = session.runner_mut().window_frame_snapshot();
            assert_eq!(closed.len(), 2);
            assert_eq!(closed[0].window.title, "Auxiliary Window");
            assert!(closed[0].window.active);
        }

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
                check_frames(&mut session);
            }
        }

        #[test]
        fn live_controls_expose_guest_values_on_both_cpus() {
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
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 2));
                wait_for_menu(&mut session, 129, 2, true);
                let controls = session.runner_mut().control_snapshot();
                let button = controls
                    .iter()
                    .find(|control| control.visible && control.title == "Activate")
                    .unwrap();
                assert_eq!(button.proc_id, 0);
                assert_eq!(button.bounds, (305, 80, 329, 190));
                let checkbox = controls
                    .iter()
                    .find(|control| control.visible && control.title == "Checkbox")
                    .unwrap();
                assert_eq!(checkbox.proc_id, 1);
                assert_eq!(checkbox.bounds, (305, 225, 329, 355));
                assert_eq!(checkbox.value, 0);
                assert!(checkbox.enabled);
                assert_ne!(checkbox.generation, 0);
                let checkbox_id = checkbox.guest_id;
                let checkbox_generation = checkbox.generation;
                let bar = controls
                    .iter()
                    .find(|control| control.visible && control.proc_id == 16)
                    .unwrap();
                assert_eq!(bar.bounds, (360, 80, 376, 540));
                assert_eq!((bar.value, bar.minimum, bar.maximum), (0, 0, 10));
                assert_ne!(bar.generation, 0);
                let bar_id = bar.guest_id;
                let bar_generation = bar.generation;
                let (top, left, bottom, right) = checkbox.bounds;
                let (vertical, horizontal) = ((top + bottom) / 2, (left + right) / 2);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical,
                    horizontal,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical,
                    horizontal,
                });
                settle(&mut session);
                let updated = session.runner_mut().control_snapshot();
                let checkbox = updated
                    .iter()
                    .find(|control| control.guest_id == checkbox_id)
                    .unwrap();
                assert_eq!(checkbox.value, 1);
                assert_eq!(checkbox.generation, checkbox_generation);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: 368,
                    horizontal: 532,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: 368,
                    horizontal: 532,
                });
                settle(&mut session);
                let updated = session.runner_mut().control_snapshot();
                let bar = updated
                    .iter()
                    .find(|control| control.guest_id == bar_id)
                    .unwrap();
                assert!(bar.value > 0 && bar.value <= bar.maximum);
                assert_eq!(bar.generation, bar_generation);

                let thumb = super::super::frames::scrollbar_geometry(bar);
                let drag_from = (
                    (bar.bounds.0 + bar.bounds.2) / 2,
                    bar.bounds.1 + thumb.thumb_start as i16 + 8,
                );
                let drag_to = (drag_from.0, bar.bounds.3 - 28);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: drag_from.0,
                    horizontal: drag_from.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseMove {
                    vertical: drag_to.0,
                    horizontal: drag_to.1,
                });
                settle(&mut session);
                let held = session.runner_mut().control_snapshot();
                assert_eq!(
                    held.iter()
                        .find(|control| control.guest_id == bar_id)
                        .unwrap()
                        .value,
                    bar.value,
                    "thumb value should commit on release"
                );
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: drag_to.0,
                    horizontal: drag_to.1,
                });
                settle(&mut session);
                let dragged = session.runner_mut().control_snapshot();
                let dragged_bar = dragged
                    .iter()
                    .find(|control| control.guest_id == bar_id)
                    .unwrap();
                assert!(dragged_bar.value >= 8, "{powerpc:?} {:?}", dragged_bar.value);

                let thumb = super::super::frames::scrollbar_geometry(dragged_bar);
                let cancel_from = (
                    (dragged_bar.bounds.0 + dragged_bar.bounds.2) / 2,
                    dragged_bar.bounds.1 + thumb.thumb_start as i16 + 8,
                );
                let cancel_to = (dragged_bar.bounds.0 - 40, cancel_from.1);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: cancel_from.0,
                    horizontal: cancel_from.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseMove {
                    vertical: cancel_to.0,
                    horizontal: cancel_to.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: cancel_to.0,
                    horizontal: cancel_to.1,
                });
                settle(&mut session);
                let cancelled = session.runner_mut().control_snapshot();
                assert_eq!(
                    cancelled
                        .iter()
                        .find(|control| control.guest_id == bar_id)
                        .unwrap()
                        .value,
                    dragged_bar.value
                );
            }
        }

        #[test]
        fn dialog_items_have_shared_geometry_and_identity_across_guest_modes() {
            use systemless::runner::DialogItemKind;

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
                wait_for_menu(&mut session, 128, 1, false);
                assert!(session.runner_mut().select_guest_menu_item(128, 1));
                let dialog = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .dialog_snapshot()
                            .into_iter()
                            .find(|dialog| dialog.visible && dialog.active)
                    })
                    .expect("About alert should expose a live dialog snapshot");
                assert_ne!(dialog.guest_id, 0);
                assert_ne!(dialog.generation, 0);
                assert_eq!(dialog.default_item, Some(1));
                assert_eq!(dialog.items.len(), 2);
                assert_eq!(dialog.items[0].kind, DialogItemKind::Button);
                assert_eq!(dialog.items[0].text, "OK");
                assert_eq!(dialog.items[0].number, 1);
                assert_eq!(dialog.items[0].bounds, (220, 360, 240, 430));
                assert_eq!(dialog.items[1].kind, DialogItemKind::StaticText);
                assert!(!dialog.items[1].enabled);
                let windows = session.runner_mut().window_frame_snapshot();
                assert_eq!(
                    windows
                        .iter()
                        .find(|window| window.guest_id == dialog.guest_id)
                        .unwrap()
                        .generation,
                    dialog.generation
                );
                assert_eq!(
                    super::standard_dbox_dialog(&[dialog.clone()], &windows)
                        .map(|selected| selected.guest_id),
                    Some(dialog.guest_id)
                );
                let (top, left, bottom, right) = dialog.items[0].bounds;
                let (vertical, horizontal) = ((top + bottom) / 2, (left + right) / 2);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical,
                    horizontal,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical,
                    horizontal,
                });
                let dismissed = (0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().is_empty()
                });
                assert!(dismissed, "guest should dismiss the About alert after its button click");
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn themed_dialog_button_forwards_one_guest_press_and_release(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::runner::{
                DialogItemSnapshot, DialogSnapshot, WindowFrameSnapshot, WindowSnapshot,
            };

            let (sender, receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            gpui_kit::size(gpui_kit::px(900.), gpui_kit::px(740.)),
                            cx,
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|cx| super::Demo::new(sender, updates, cx)),
                )
                .unwrap()
            });
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    let bounds = (130, 150, 260, 450);
                    demo.windows = vec![WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: WindowSnapshot {
                            title: String::new(),
                            bounds,
                            structure_bounds: Some((122, 142, 268, 458)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(1),
                        close_box: false,
                    }];
                    demo.dialogs = vec![DialogSnapshot {
                        guest_id: 7,
                        generation: 1,
                        bounds,
                        visible: true,
                        active: true,
                        default_item: Some(1),
                        cancel_item: None,
                        edit_field: None,
                        items: vec![DialogItemSnapshot {
                            number: 1,
                            kind: super::DialogItemKind::Button,
                            bounds: (220, 360, 240, 430),
                            text: "OK".into(),
                            enabled: true,
                            visible: true,
                            value: None,
                            selection: None,
                        }],
                    }];
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-dialog-button-7-1-1", cx);
            })
            .unwrap();
            let inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .collect();
            assert_eq!(
                inputs
                    .iter()
                    .filter(|input| matches!(input, MacintoshInput::MouseDown { .. }))
                    .count(),
                1
            );
            assert_eq!(
                inputs
                    .iter()
                    .filter(|input| matches!(input, MacintoshInput::MouseUp { .. }))
                    .count(),
                1
            );
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn themed_checkbox_forwards_one_guest_press_and_release(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::runner::{ControlSnapshot, WindowFrameSnapshot, WindowSnapshot};

            let (sender, receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            gpui_kit::size(gpui_kit::px(900.), gpui_kit::px(740.)),
                            cx,
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|cx| super::Demo::new(sender, updates, cx)),
                )
                .unwrap()
            });
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    let bounds = (50, 40, 420, 600);
                    demo.windows = vec![WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: WindowSnapshot {
                            title: "Controls".into(),
                            bounds,
                            structure_bounds: Some((31, 39, 422, 602)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(0),
                        close_box: false,
                    }];
                    demo.controls = vec![ControlSnapshot {
                        guest_id: 22,
                        generation: 1,
                        owner_id: 7,
                        proc_id: 1,
                        local_bounds: (100, 100, 124, 220),
                        bounds: (150, 140, 174, 260),
                        owner_visible: true,
                        visible: true,
                        enabled: true,
                        hilite: 0,
                        value: 0,
                        minimum: 0,
                        maximum: 1,
                        title: "Checkbox".into(),
                    }];
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-control-checkbox-22-1", cx);
            })
            .unwrap();
            let inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .collect();
            assert_eq!(
                inputs
                    .iter()
                    .filter(|input| matches!(input, MacintoshInput::MouseDown { .. }))
                    .count(),
                1
            );
            assert_eq!(
                inputs
                    .iter()
                    .filter(|input| matches!(input, MacintoshInput::MouseUp { .. }))
                    .count(),
                1
            );
        }
    }
}
