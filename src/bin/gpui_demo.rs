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
        collections::HashMap,
        path::PathBuf,
        sync::{mpsc, Arc, Mutex},
        time::{Duration, Instant},
    };

    use clap::Parser;
    use gpui_kit::{
        component::{
            button::{Button, ButtonVariants},
            checkbox::Checkbox,
            menu::{PopupMenu, PopupMenuItem},
            popover::Popover,
            radio::Radio,
            ActiveTheme, Disableable, Sizable,
        },
        prelude::*,
        *,
    };
    use systemless::{
        memory::{globals::addr::MBAR_HEIGHT, MemoryBus},
        menu_model::{GuestMenu, GuestMenuSnapshot},
        runner::{ControlSnapshot, DialogItemKind, DialogSnapshot, ListManagerSnapshot, StandardFileKind, StandardFileSnapshot, TextEditSnapshot, WindowFrameSnapshot},
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
        capture_modal_dialog: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_checked: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modeless_dialog: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_nested_modal_dialog: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls_changed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls_dragged: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_controls_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_edited: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_save: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_save_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_open_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_save_edited_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_custom_menu_fallback: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modeless_dialog_layout: Option<PathBuf>,
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

    fn save_name_segments(
        name: &str,
        selection: (usize, usize),
        focused: bool,
    ) -> (String, String, String) {
        if !focused {
            return (name.to_string(), String::new(), String::new());
        }
        // TextEdit selections use source-buffer byte offsets. In this Roman
        // field each source byte becomes one character, even when its host
        // UTF-8 encoding uses multiple bytes (Inside Macintosh: Text, TESetSelect).
        let chars: Vec<char> = name.chars().collect();
        let start = selection.0.min(chars.len());
        let end = selection.1.max(start).min(chars.len());
        (
            chars[..start].iter().collect(),
            chars[start..end].iter().collect(),
            chars[end..].iter().collect(),
        )
    }

    enum Command {
        Menu(i16, i16, u32, u64),
        Input(MacintoshInput),
    }

    #[derive(Default)]
    struct LiveMenuState {
        menu: Option<Entity<PopupMenu>>,
        rendered: Option<Vec<GuestMenu>>,
    }

    #[derive(Default)]
    struct Update {
        menus: GuestMenuSnapshot,
        guest_menu_tracking: bool,
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
        lists: Vec<ListManagerSnapshot>,
        text_edits: Vec<TextEditSnapshot>,
        standard_file: Option<StandardFileSnapshot>,
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
                        Ok(Command::Menu(menu, item, guest_id, generation)) => {
                            if session
                                .runner_mut()
                                .guest_menu_snapshot()
                                .selectable_result_for_guest(menu, item, guest_id, generation)
                                .is_some()
                            {
                                session.runner_mut().select_guest_menu_item(menu, item);
                            }
                        }
                        Ok(Command::Input(input)) => {
                            session.deliver_input(input);
                            // Tracking and autoKey must observe a held input
                            // before a queued release clears it (IM:I, I-246).
                            if matches!(input, MacintoshInput::MouseDown { .. })
                                || matches!(input, MacintoshInput::KeyDown { mac_key, .. } if mac_key != 0x37)
                            {
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
                let guest_menu_fallback = menus.requires_guest_menu_rendering();
                let guest_menu_tracking = session.runner().guest_menu_tracking_active();
                let frame = session.video_frame().map(|frame| {
                    // Preserve guest MBarHeight and crop only the displayed image,
                    // as the native menu frontend does (Inside Macintosh V, V-245).
                    let top = if guest_menu_fallback {
                        0
                    } else {
                        u32::from(session.runner().bus().read_word(MBAR_HEIGHT))
                    };
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
                let lists = session.runner_mut().list_manager_snapshot();
                let text_edits = session.runner_mut().text_edit_snapshot().records;
                let standard_file = session.runner_mut().standard_file_snapshot();
                *updates.lock().unwrap() = Some(Update {
                    menus,
                    guest_menu_tracking,
                    windows,
                    dialogs,
                    controls,
                    lists,
                    text_edits,
                    standard_file,
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
        guest_menu_tracking: bool,
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
        lists: Vec<ListManagerSnapshot>,
        text_edits: Vec<TextEditSnapshot>,
        standard_file: Option<StandardFileSnapshot>,
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
        popup_tracking: Option<(u32, u64)>,
        command_down: bool,
        held_keys: HashMap<u8, u8>,
        _focus_out: Option<Subscription>,
        _focus_lost: Option<Subscription>,
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
                            this.guest_menu_tracking = update.guest_menu_tracking;
                            this.windows = update.windows;
                            this.dialogs = update.dialogs;
                            this.controls = update.controls;
                            this.lists = update.lists;
                            this.text_edits = update.text_edits;
                            this.standard_file = update.standard_file;
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
                guest_menu_tracking: false,
                windows: Vec::new(),
                dialogs: Vec::new(),
                controls: Vec::new(),
                lists: Vec::new(),
                text_edits: Vec::new(),
                standard_file: None,
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
                popup_tracking: None,
                command_down: false,
                held_keys: HashMap::new(),
                _focus_out: None,
                _focus_lost: None,
                _poll: poll,
            }
        }

        fn pointer(&self, position: Point<Pixels>) -> (i16, i16) {
            // The framebuffer is displayed at 1:1; custom MDEFs retain the
            // guest menu bar and therefore have no host-bar offset.
            let x = f32::from(position.x).clamp(0., self.width.saturating_sub(1) as f32);
            let bar_height = if self.guest_menu_fallback() { 0. } else { 36. };
            let y = (f32::from(position.y) - bar_height)
                .clamp(0., self.height.saturating_sub(1) as f32);
            ((y as u32 + self.crop_top) as i16, x as i16)
        }

        fn inside_guest_pane(&self, position: Point<Pixels>) -> bool {
            let x = f32::from(position.x);
            let y = f32::from(position.y);
            let bar_height = if self.guest_menu_fallback() { 0. } else { 36. };
            x >= 0.
                && x < self.width as f32
                && y >= bar_height
                && y < bar_height + self.height as f32
        }

        fn guest_menu_fallback(&self) -> bool {
            self.menus.requires_guest_menu_rendering()
        }

        fn sync_command_key(&mut self, down: bool) {
            if self.command_down == down {
                return;
            }
            self.command_down = down;
            let input = if down {
                MacintoshInput::KeyDown { mac_key: 0x37, character: 0 }
            } else {
                MacintoshInput::KeyUp { mac_key: 0x37, character: 0 }
            };
            let _ = self.commands.send(Command::Input(input));
        }

        fn press_host_key(&mut self, mac_key: u8, character: u8) {
            if let std::collections::hash_map::Entry::Vacant(entry) = self.held_keys.entry(mac_key) {
                entry.insert(character);
                let _ = self.commands.send(Command::Input(MacintoshInput::KeyDown {
                    mac_key,
                    character,
                }));
            }
        }

        fn release_host_key(&mut self, mac_key: u8) {
            if let Some(character) = self.held_keys.remove(&mac_key) {
                let _ = self.commands.send(Command::Input(MacintoshInput::KeyUp {
                    mac_key,
                    character,
                }));
            }
        }

        fn release_host_input(&mut self) {
            if self.mouse_down {
                self.mouse_down = false;
                self.scrollbar_drag = None;
                self.popup_tracking = None;
                let (vertical, horizontal) = self.mouse_position;
                let _ = self.commands.send(Command::Input(MacintoshInput::MouseUp {
                    vertical,
                    horizontal,
                }));
            }
            let mut keys: Vec<u8> = self.held_keys.keys().copied().collect();
            keys.sort_unstable();
            for mac_key in keys {
                self.release_host_key(mac_key);
            }
            self.sync_command_key(false);
        }

        fn scrollbar_at(&self, point: (i16, i16)) -> Option<(u32, u64, (i16, i16))> {
            let viewport = super::frames::Rect {
                top: self.crop_top as i32,
                left: 0,
                bottom: (self.crop_top + self.height) as i32,
                right: self.width as i32,
            };
            super::frames::control_pieces(&self.controls, &self.menus, &self.windows, viewport)
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

        fn popup_at(&self, point: (i16, i16)) -> Option<(u32, u64)> {
            let viewport = super::frames::Rect {
                top: self.crop_top as i32,
                left: 0,
                bottom: (self.crop_top + self.height) as i32,
                right: self.width as i32,
            };
            super::frames::control_pieces(&self.controls, &self.menus, &self.windows, viewport)
                .into_iter()
                .find_map(|piece| {
                    let control = &self.controls[piece.control];
                    let p = (i32::from(point.0), i32::from(point.1));
                    (control.enabled
                        && super::frames::popup_control_label(control, &self.menus).is_some()
                        && p.0 >= piece.clip.top
                        && p.0 < piece.clip.bottom
                        && p.1 >= piece.clip.left
                        && p.1 < piece.clip.right)
                        .then_some((control.guest_id, control.generation))
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
                let guest_id = menu.guest_id;
                let generation = menu.generation;
                let label = match item.key_equivalent {
                    Some(key) => format!("{}    ⌘{}", item.text, key.to_uppercase()),
                    None => item.text.clone(),
                };
                popup = popup.item(
                    PopupMenuItem::new(label)
                        .checked(item.checked)
                        .disabled(!menu.enabled || !item.enabled)
                        .on_click(move |_, _, _| {
                            let _ = commands.send(Command::Menu(id, number, guest_id, generation));
                        }),
                );
            }
        }
        popup
    }

    fn rendered_menu_tree(snapshot: &GuestMenuSnapshot, id: i16) -> Vec<GuestMenu> {
        fn visit(snapshot: &GuestMenuSnapshot, id: i16, tree: &mut Vec<GuestMenu>) {
            if tree.iter().any(|menu| menu.id == id) {
                return;
            }
            let Some(menu) = snapshot.menus.iter().find(|menu| menu.id == id) else {
                return;
            };
            tree.push(menu.clone());
            for submenu_id in menu.items.iter().filter_map(|item| item.submenu_id) {
                visit(snapshot, submenu_id, tree);
            }
        }

        let mut tree = Vec::new();
        visit(snapshot, id, &mut tree);
        tree
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
                && dialog.items.iter().all(|item| match item.kind {
                    DialogItemKind::Button | DialogItemKind::StaticText | DialogItemKind::EditText => true,
                    DialogItemKind::Checkbox | DialogItemKind::RadioButton => item.value.is_some(),
                    _ => false,
                })
        })
    }

    impl Render for Demo {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            if self._focus_out.is_none() {
                self._focus_out = Some(cx.on_focus_out(&self.focus, window, |this, _, _, cx| {
                    this.release_host_input();
                    cx.notify();
                }));
            }
            if self._focus_lost.is_none() {
                self._focus_lost = Some(cx.on_focus_lost(window, |this, _, cx| {
                    this.release_host_input();
                    cx.notify();
                }));
            }
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
                let identity = format!("guest-menu-{}-{}", menu.guest_id, menu.generation);
                let state = window.use_keyed_state(format!("live-{identity}"), cx, |_, _| {
                    LiveMenuState::default()
                });
                bar = bar.child(
                    Popover::new(format!("popover-{identity}"))
                        .appearance(false)
                        .overlay_closable(false)
                        .trigger(
                            Button::new(identity)
                                .label(menu.title.clone())
                                .ghost()
                                .small()
                                .disabled(!menu.enabled),
                        )
                        .content(move |_, window, cx| {
                            let menu_tree = rendered_menu_tree(&snapshot, id);
                            if let Some(open_menu) = state.read(cx).menu.clone() {
                                if state.read(cx).rendered.as_ref() != Some(&menu_tree) {
                                    open_menu.update(cx, |popup, cx| {
                                        popup.rebuild(window, cx, |popup, window, cx| {
                                            populate_menu(
                                                popup.scrollable(true).max_h(px(420.)),
                                                &snapshot,
                                                id,
                                                &commands,
                                                &[],
                                                window,
                                                cx,
                                            )
                                        });
                                    });
                                    state.update(cx, |state, _| {
                                        state.rendered = Some(menu_tree);
                                    });
                                }
                                return open_menu;
                            }
                            let open_menu = PopupMenu::build(window, cx, |popup, window, cx| {
                                populate_menu(
                                    popup.scrollable(true).max_h(px(420.)),
                                    &snapshot,
                                    id,
                                    &commands,
                                    &[],
                                    window,
                                    cx,
                                )
                            });
                            state.update(cx, |state, _| {
                                state.menu = Some(open_menu.clone());
                                state.rendered = Some(menu_tree);
                            });
                            open_menu.focus_handle(cx).focus(window, cx);
                            let popover = cx.entity().downgrade();
                            window
                                .subscribe(&open_menu, cx, {
                                    let state = state.downgrade();
                                    move |_, _: &DismissEvent, window, cx| {
                                        if let Some(popover) = popover.upgrade() {
                                            popover.update(cx, |popover, cx| {
                                                popover.dismiss(window, cx);
                                            });
                                        }
                                        _ = state.update(cx, |state, _| {
                                            state.menu = None;
                                            state.rendered = None;
                                        });
                                    }
                                })
                                .detach();
                            open_menu
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
                        this.popup_tracking = this.popup_at((vertical, horizontal));
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
                        this.popup_tracking = None;
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
                            this.popup_tracking = None;
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
            // A tracking custom MDEF may draw its dropdown over any window.
            // Keep those pixels, but present standard windows and dialogs
            // while the custom menu is closed.
            // Macintosh Toolbox Essentials (1992), pp. 3-3, 3-87.
            if !self.guest_menu_fallback() || !self.guest_menu_tracking {
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
                // A standard LDEF's unstyled text rows can use Kit list items.
                // The list's guest-visible cells, selection, and view origin remain
                // authoritative, and unknown LDEFs retain their framebuffer pixels.
                // More Macintosh Toolbox (1993), pp. 4-70--4-76.
                for piece in super::frames::list_pieces(
                    &self.lists,
                    &self.controls,
                    &self.windows,
                    viewport,
                ) {
                    let list = &self.lists[piece.list];
                    let Some(cells) = list.text_cells.as_ref() else {
                        continue;
                    };
                    let source = piece.source;
                    let clip = piece.clip;
                    let mut overlay = div()
                        .absolute()
                        .left(px((source.left - clip.left) as f32))
                        .top(px((source.top - clip.top) as f32))
                        .w(px(source.width() as f32))
                        .h(px(source.height() as f32))
                        .bg(cx.theme().background);
                    for (&(row, column), text) in cells {
                        if row < list.visible.0
                            || row >= list.visible.2
                            || column < list.visible.1
                            || column >= list.visible.3
                        {
                            continue;
                        }
                        let top = i32::from(row - list.visible.0) * i32::from(list.cell_size.0.max(1));
                        let left =
                            i32::from(column - list.visible.1) * i32::from(list.cell_size.1.max(1));
                        let selected = list.selected.contains(&(row, column));
                        overlay = overlay.child(
                            div()
                            .id(format!(
                                "guest-list-cell-{}-{}-{}-{}",
                                list.guest_id, list.generation, row, column
                            ))
                            .test_support()
                            .role(Role::ListItem)
                            .aria_label(text.clone())
                            .aria_selected(selected)
                            .absolute()
                            .left(px(left as f32))
                            .top(px(top as f32))
                            .w(px(f32::from(list.cell_size.1.max(1))))
                            .h(px(f32::from(list.cell_size.0.max(1))))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .bg(if selected && list.active {
                                cx.theme().accent
                            } else {
                                cx.theme().background
                            })
                            .px_1()
                            .text_size(px(13.))
                            .child(text.clone()),
                        );
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
                // TextEdit supplies the guest line breaks and scroll origin. Keep
                // keyboard and pointer events on the normal guest path.
                // Inside Macintosh: Text (1993), pp. 2-64--2-69.
                for piece in super::frames::text_edit_pieces(
                    &self.text_edits,
                    &self.dialogs,
                    &self.controls,
                    &self.windows,
                    viewport,
                ) {
                    let record = &self.text_edits[piece.record];
                    let Some(lines) = record.display_lines() else {
                        continue;
                    };
                    let Some(dest) = record.global_dest_rect.map(super::frames::Rect::from) else {
                        continue;
                    };
                    let source = piece.source;
                    let clip = piece.clip;
                    let mut overlay = div()
                        .absolute()
                        .left(px((source.left - clip.left) as f32))
                        .top(px((source.top - clip.top) as f32))
                        .w(px(source.width() as f32))
                        .h(px(source.height() as f32))
                        .bg(cx.theme().background);
                    for (index, line) in lines.into_iter().enumerate() {
                        let top = dest.top + index as i32 * i32::from(record.line_height) - source.top;
                        if top >= source.height() || top + i32::from(record.line_height) <= 0 {
                            continue;
                        }
                        let starts = record.line_starts.as_ref().unwrap();
                        let line_start = starts[index];
                        let line_end = line_start + line.chars().count();
                        let selection = (
                            record.selection.0.saturating_sub(line_start).min(line_end - line_start),
                            record.selection.1.saturating_sub(line_start).min(line_end - line_start),
                        );
                        let (before, selected, after) = save_name_segments(&line, selection, true);
                        let soft_wrap_end = index + 2 < starts.len() && starts[index + 1] == line_end;
                        let caret = record.active && record.selection.0 == record.selection.1
                            && record.selection.0 >= line_start
                            && (record.selection.0 < line_end
                                || record.selection.0 == line_end && !soft_wrap_end);
                        overlay = overlay.child(
                            div()
                                .id(format!("guest-text-edit-line-{}-{}-{index}", record.guest_id, record.generation))
                                .absolute()
                                .left(px((dest.left - source.left) as f32))
                                .top(px(top as f32))
                                .w(px(dest.width().max(1) as f32))
                                .h(px(f32::from(record.line_height)))
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .text_size(px(f32::from(record.size.clamp(9, 18))))
                                .child(before)
                                .when(caret, |row| row.child(
                                    div().w(px(1.)).h(px(f32::from(record.line_height.max(1)))).bg(cx.theme().foreground)
                                ))
                                .when(!selected.is_empty(), |row| row.child(
                                    div().bg(cx.theme().selection).child(selected)
                                ))
                                .child(after),
                        );
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
                // CDEF-owned standard controls can use Kit components while their
                // ControlRecord state and tracking remain guest-owned.
                // Macintosh Toolbox Essentials (1992), pp. 5-58--5-64.
                for piece in super::frames::control_pieces(&self.controls, &self.menus, &self.windows, viewport) {
                    let control = &self.controls[piece.control];
                    if (1008..=1023).contains(&control.proc_id)
                        && (self.popup_tracking == Some((control.guest_id, control.generation))
                            || control.hilite == 1)
                    {
                        // Let the guest's live popup tracking and MDEF paint the
                        // open state until release. MTE (1992), pp. 3-34--3-35.
                        continue;
                    }
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
                        proc_id if (1008..=1023).contains(&proc_id) => {
                            let Some(selected) = super::frames::popup_control_label(control, &self.menus) else {
                                continue;
                            };
                            let title_width = i32::from(control.popup_title_width.unwrap_or(0))
                                .clamp(0, source.width().saturating_sub(20));
                            overlay = overlay
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(title_width as f32))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(if control.enabled { cx.theme().foreground } else { cx.theme().muted_foreground })
                                        .child(control.title.clone()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .min_w(px(1.))
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .bg(cx.theme().secondary)
                                        .flex()
                                        .items_center()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(1.))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .px_1()
                                                .text_size(px(12.))
                                                .text_color(if control.enabled { cx.theme().foreground } else { cx.theme().muted_foreground })
                                                .child(selected.to_owned()),
                                        )
                                        .child(div().w(px(18.)).flex().items_center().justify_center().child("▾")),
                                );
                        }
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
                // Standard DITL items use guest geometry and live guest state.
                // The single-line edit field is a read-only GPUI presentation;
                // pointer and keyboard events still enter the guest Dialog Manager.
                // Macintosh Toolbox Essentials (1992), pp. 6-13--6-15, 6-79--6-80.
                for piece in
                    super::frames::dialog_item_pieces(&self.dialogs, &self.windows, viewport)
                {
                    let dialog = &self.dialogs[piece.dialog];
                    let item = &dialog.items[piece.item];
                    let item_rect = super::frames::Rect::from(item.bounds);
                    let source = piece.source;
                    let clip = piece.clip;
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
                        DialogItemKind::EditText => {
                            let focused = dialog.active
                                && dialog.edit_field == Some(item.number)
                                && item.enabled
                                && item.selection.is_some();
                            let selection = item.selection.unwrap_or((0, 0));
                            let (prefix, selected, suffix) = save_name_segments(
                                &item.text,
                                (selection.0.max(0) as usize, selection.1.max(0) as usize),
                                focused,
                            );
                            let mut field = div()
                                .id(format!(
                                    "guest-dialog-edit-{}-{}-{}",
                                    dialog.guest_id, dialog.generation, item.number
                                ))
                                .test_support()
                                .absolute()
                                .left(px((item_rect.left - source.left) as f32))
                                .top(px((item_rect.top - source.top) as f32))
                                .w(px(item_rect.width() as f32))
                                .h(px(item_rect.height() as f32))
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .px_1()
                                .border_1()
                                .border_color(if focused {
                                    cx.theme().accent
                                } else {
                                    cx.theme().border
                                })
                                .bg(cx.theme().background)
                                .text_size(px(13.))
                                .text_color(if item.enabled {
                                    cx.theme().foreground
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .child(prefix);
                            if focused && !selected.is_empty() {
                                field = field.child(
                                    div()
                                        .bg(cx.theme().selection)
                                        .text_color(cx.theme().foreground)
                                        .child(selected),
                                );
                            } else if focused {
                                field = field.child(
                                    div().w(px(1.)).h(px(14.)).bg(cx.theme().foreground),
                                );
                            }
                            overlay.child(field.child(suffix))
                        }
                        DialogItemKind::Checkbox => overlay.child(
                            Checkbox::new(format!(
                                "guest-dialog-checkbox-{}-{}-{}",
                                dialog.guest_id, dialog.generation, item.number
                            ))
                            .label(item.text.clone())
                            .checked(item.value.unwrap() != 0)
                            .disabled(!item.enabled)
                            .tab_stop(false)
                            .small()
                            .w_full()
                            .h_full(),
                        ),
                        DialogItemKind::RadioButton => overlay.child(
                            Radio::new(format!(
                                "guest-dialog-radio-{}-{}-{}",
                                dialog.guest_id, dialog.generation, item.number
                            ))
                            .label(item.text.clone())
                            .checked(item.value.unwrap() != 0)
                            .disabled(!item.enabled)
                            .tab_stop(false)
                            .small()
                            .w_full()
                            .h_full(),
                        ),
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
                if let Some(panel) = self.standard_file.as_ref().filter(|panel| {
                    panel.kind == StandardFileKind::Get
                        && panel.standard_entry_point
                        && panel.get_layout.is_some()
                        && panel.entries.is_some()
                }) {
                    let layout = panel.get_layout.as_ref().unwrap();
                    let bounds = super::frames::Rect::from(panel.bounds);
                    if bounds.intersection(viewport).is_some() {
                        let at = |rect: (i16, i16, i16, i16)| {
                            let rect = super::frames::Rect::from(rect);
                            div()
                                .absolute()
                                .top(px((rect.top - bounds.top) as f32))
                                .left(px((rect.left - bounds.left) as f32))
                                .w(px(rect.width() as f32))
                                .h(px(rect.height() as f32))
                        };
                        let mut overlay = div()
                            .id(format!("guest-standard-open-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .absolute()
                            .top(px((bounds.top - self.crop_top as i32) as f32))
                            .left(px(bounds.left as f32))
                            .w(px(bounds.width() as f32))
                            .h(px(bounds.height() as f32))
                            .bg(cx.theme().background)
                            .border_2()
                            .border_color(cx.theme().border)
                            .text_color(cx.theme().foreground)
                            .text_size(px(13.));
                        let volume_abbreviation: String = panel
                            .directory_label
                            .as_deref()
                            .unwrap_or_default()
                            .chars()
                            .take(4)
                            .collect();
                        overlay = overlay.child(
                            at(layout.volume)
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .text_ellipsis()
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(format!("{volume_abbreviation}…")),
                        );
                        overlay = overlay.child(
                            at(layout.directory_label)
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(panel.directory_label.clone().unwrap_or_default()),
                        );
                        let entries = panel.entries.as_ref().unwrap();
                        let list_width = i32::from(layout.list.3 - layout.list.1);
                        let mut list = at(layout.list)
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border);
                        for (row, (index, entry)) in entries
                            .iter()
                            .enumerate()
                            .skip(layout.first_visible)
                            .take(layout.visible_rows)
                            .enumerate()
                        {
                            let selected = panel.selected == Some(index);
                            list = list.child(
                                div()
                                    .id(format!(
                                        "guest-standard-open-entry-{}-{}-{}",
                                        panel.guest_id, panel.generation, index
                                    ))
                                    .test_support()
                                    .role(Role::ListItem)
                                    .aria_label(entry.name.clone())
                                    .aria_selected(selected)
                                    .absolute()
                                    .top(px(2. + row as f32 * f32::from(layout.row_height)))
                                    .left(px(2.))
                                    .w(px((list_width - 4).max(1) as f32))
                                    .h(px(f32::from(layout.row_height)))
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .px_1()
                                    .bg(if selected {
                                        cx.theme().accent
                                    } else {
                                        cx.theme().background
                                    })
                                    .child(if entry.is_directory {
                                        format!("{} ▸", entry.name)
                                    } else {
                                        entry.name.clone()
                                    }),
                            );
                        }
                        overlay = overlay.child(list);
                        let scroll_max = entries.len().saturating_sub(layout.visible_rows);
                        let scroll_height = i32::from(layout.scroll.2 - layout.scroll.0);
                        let track_height = (scroll_height - 32).max(0);
                        let thumb_height = if scroll_max == 0 {
                            track_height
                        } else {
                            16.min(track_height)
                        };
                        let thumb_top = 16
                            + if scroll_max == 0 {
                                0
                            } else {
                                ((track_height - thumb_height) as i64
                                    * layout.first_visible.min(scroll_max) as i64
                                    / scroll_max as i64) as i32
                            };
                        overlay = overlay.child(
                            at(layout.scroll)
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .w_full()
                                        .h(px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▴"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .bottom_0()
                                        .w_full()
                                        .h(px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▾"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(thumb_top as f32))
                                        .w_full()
                                        .h(px(thumb_height as f32))
                                        .bg(cx.theme().accent)
                                        .border_1()
                                        .border_color(cx.theme().border),
                                ),
                        );
                        for (label, rect, enabled) in [
                            ("Eject", layout.eject, false),
                            ("Desktop", layout.desktop, true),
                            ("Cancel", layout.cancel, true),
                            (
                                "Open",
                                layout.open,
                                panel.selected.and_then(|index| entries.get(index)).is_some_and(
                                    |entry| entry.is_directory || entry.file_type != 0,
                                ),
                            ),
                        ] {
                            overlay = overlay.child(
                                at(rect).child(
                                    Button::new(format!(
                                        "guest-standard-open-{}-{}-{}",
                                        panel.guest_id, panel.generation, label
                                    ))
                                    .label(label)
                                    .small()
                                    .compact()
                                    .disabled(!enabled)
                                    .tab_stop(false)
                                    .w_full()
                                    .h_full(),
                                ),
                            );
                        }
                        screen = screen.child(overlay);
                    }
                }
                if let Some(panel) = self.standard_file.as_ref().filter(|panel| {
                    panel.kind == StandardFileKind::Put
                        && panel.standard_entry_point
                        && panel.put_layout.is_some()
                        && panel.entries.is_some()
                        && panel.name.is_some()
                }) {
                    let layout = panel.put_layout.as_ref().unwrap();
                    let bounds = super::frames::Rect::from(panel.bounds);
                    if bounds.intersection(viewport).is_some() {
                        let at = |rect: (i16, i16, i16, i16)| {
                            let rect = super::frames::Rect::from(rect);
                            div()
                                .absolute()
                                .top(px((rect.top - bounds.top) as f32))
                                .left(px((rect.left - bounds.left) as f32))
                                .w(px(rect.width() as f32))
                                .h(px(rect.height() as f32))
                        };
                        let mut overlay = div()
                            .id(format!("guest-standard-save-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .absolute()
                            .top(px((bounds.top - self.crop_top as i32) as f32))
                            .left(px(bounds.left as f32))
                            .w(px(bounds.width() as f32))
                            .h(px(bounds.height() as f32))
                            .bg(cx.theme().background)
                            .border_2()
                            .border_color(cx.theme().border)
                            .text_color(cx.theme().foreground)
                            .text_size(px(13.));
                        overlay = overlay.child(
                            at(layout.directory_label)
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(panel.directory_label.clone().unwrap_or_default()),
                        );
                        let entries = panel.entries.as_ref().unwrap();
                        let list_width = i32::from(layout.list.3 - layout.list.1);
                        let mut list = at(layout.list)
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border);
                        for (row, (index, entry)) in entries
                            .iter()
                            .enumerate()
                            .skip(layout.first_visible)
                            .take(layout.visible_rows)
                            .enumerate()
                        {
                            let selected = panel.selected == Some(index);
                            list = list.child(
                                div()
                                    .id(format!(
                                        "guest-standard-save-entry-{}-{}-{}",
                                        panel.guest_id, panel.generation, index
                                    ))
                                    .test_support()
                                    .role(Role::ListItem)
                                    .aria_label(entry.name.clone())
                                    .aria_selected(selected)
                                    .absolute()
                                    .top(px(2. + row as f32 * f32::from(layout.row_height)))
                                    .left(px(2.))
                                    .w(px((list_width - 4).max(1) as f32))
                                    .h(px(f32::from(layout.row_height)))
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .px_1()
                                    .bg(if selected {
                                        cx.theme().accent
                                    } else {
                                        cx.theme().background
                                    })
                                    .child(if entry.is_directory {
                                        format!("{} ▸", entry.name)
                                    } else {
                                        entry.name.clone()
                                    }),
                            );
                        }
                        overlay = overlay.child(list);
                        let scroll_max = entries.len().saturating_sub(layout.visible_rows);
                        let scroll_height = i32::from(layout.scroll.2 - layout.scroll.0);
                        let track_height = (scroll_height - 32).max(0);
                        let thumb_height = if scroll_max == 0 {
                            track_height
                        } else {
                            16.min(track_height)
                        };
                        let thumb_top = 16
                            + if scroll_max == 0 {
                                0
                            } else {
                                ((track_height - thumb_height) as i64
                                    * layout.first_visible.min(scroll_max) as i64
                                    / scroll_max as i64) as i32
                            };
                        overlay = overlay.child(
                            at(layout.scroll)
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .w_full()
                                        .h(px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▴"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .bottom_0()
                                        .w_full()
                                        .h(px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▾"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(thumb_top as f32))
                                        .w_full()
                                        .h(px(thumb_height as f32))
                                        .bg(cx.theme().accent)
                                        .border_1()
                                        .border_color(cx.theme().border),
                                ),
                        );
                        overlay = overlay.child(
                            at(layout.prompt)
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .child(panel.prompt.clone().unwrap_or_default()),
                        );
                        let name = panel.name.as_deref().unwrap_or_default();
                        let focused = panel.name_has_focus == Some(true);
                        let (prefix, selected, suffix) = save_name_segments(
                            name,
                            panel.name_selection.unwrap_or((0, 0)),
                            focused,
                        );
                        let mut name_field = at(layout.name)
                            .id(format!(
                                "guest-standard-save-name-{}-{}",
                                panel.guest_id, panel.generation
                            ))
                            .test_support()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .px_1()
                            .border_1()
                            .border_color(if focused {
                                cx.theme().accent
                            } else {
                                cx.theme().border
                            })
                            .bg(cx.theme().background)
                            .child(prefix);
                        if focused && !selected.is_empty() {
                            name_field = name_field.child(
                                div()
                                    .bg(cx.theme().selection)
                                    .text_color(cx.theme().foreground)
                                    .child(selected),
                            );
                        } else if focused {
                            name_field = name_field.child(
                                div().w(px(1.)).h(px(14.)).bg(cx.theme().foreground),
                            );
                        }
                        overlay = overlay.child(name_field.child(suffix));
                        for (label, rect) in [
                            ("Desktop", layout.desktop),
                            ("Cancel", layout.cancel),
                            ("Save", layout.save),
                        ] {
                            overlay = overlay.child(
                                at(rect).child(
                                    Button::new(format!(
                                        "guest-standard-save-{}-{}-{}",
                                        panel.guest_id, panel.generation, label
                                    ))
                                    .label(label)
                                    .small()
                                    .compact()
                                    .tab_stop(false)
                                    .w_full()
                                    .h_full(),
                                ),
                            );
                        }
                        screen = screen.child(overlay);
                    }
                }
            }
            let guest_menu_fallback = self.guest_menu_fallback();
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .track_focus(&self.focus)
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    if !this.mouse_down || this.inside_guest_pane(event.position) {
                        return;
                    }
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
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    this.sync_command_key(event.keystroke.modifiers.platform);
                    if event.keystroke.modifiers.platform {
                        if let Some((mac_key, character)) = guest_key(&event.keystroke) {
                            this.press_host_key(mac_key, character);
                            cx.stop_propagation();
                        }
                        return;
                    }
                    if let Some((mac_key, character)) = guest_key(&event.keystroke) {
                        this.press_host_key(mac_key, character);
                    }
                }))
                .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                    if let Some((mac_key, _)) = guest_key(&event.keystroke) {
                        this.release_host_key(mac_key);
                    }
                    this.sync_command_key(event.keystroke.modifiers.platform);
                }))
                .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, _| {
                    this.sync_command_key(event.modifiers.platform);
                }))
                .when(!guest_menu_fallback, |root| root.child(bar))
                .child(screen)
                .child(div().px_3().py_1().text_xs().child(self.status.clone()))
        }
    }

    // GPUI reports the printed key separately from its typed character.
    // Keep the Macintosh virtual code tied to the key and pass the typed
    // character through the existing guest event queue. Inside Macintosh
    // Volume V (1986), V-191, key-code assignments; Text (1993), pp. 2-32--2-37.
    fn guest_key(keystroke: &Keystroke) -> Option<(u8, u8)> {
        let key = keystroke.key.to_ascii_lowercase();
        let control = match key.as_str() {
            "enter" => Some((0x24, 13)),
            "escape" => Some((0x35, 27)),
            "space" => Some((0x31, 32)),
            "tab" => Some((0x30, 9)),
            "backspace" => Some((0x33, 8)),
            "left" | "arrowleft" => Some((0x7b, 28)),
            "right" | "arrowright" => Some((0x7c, 29)),
            "down" | "arrowdown" => Some((0x7d, 31)),
            "up" | "arrowup" => Some((0x7e, 30)),
            _ => None,
        };
        if control.is_some() {
            return control;
        }
        let virtual_key = match key.as_str() {
            "a" => 0x00,
            "s" => 0x01,
            "d" => 0x02,
            "f" => 0x03,
            "h" => 0x04,
            "g" => 0x05,
            "z" => 0x06,
            "x" => 0x07,
            "c" => 0x08,
            "v" => 0x09,
            "b" => 0x0b,
            "q" => 0x0c,
            "w" => 0x0d,
            "e" => 0x0e,
            "r" => 0x0f,
            "y" => 0x10,
            "t" => 0x11,
            "1" => 0x12,
            "2" => 0x13,
            "3" => 0x14,
            "4" => 0x15,
            "6" => 0x16,
            "5" => 0x17,
            "=" => 0x18,
            "9" => 0x19,
            "7" => 0x1a,
            "-" => 0x1b,
            "8" => 0x1c,
            "0" => 0x1d,
            "]" => 0x1e,
            "o" => 0x1f,
            "u" => 0x20,
            "[" => 0x21,
            "i" => 0x22,
            "p" => 0x23,
            "l" => 0x25,
            "j" => 0x26,
            "'" => 0x27,
            "k" => 0x28,
            ";" => 0x29,
            "\\" => 0x2a,
            "," => 0x2b,
            "/" => 0x2c,
            "n" => 0x2d,
            "m" => 0x2e,
            "." => 0x2f,
            "`" => 0x32,
            _ => return None,
        };
        let character = if let Some(text) = keystroke.key_char.as_deref() {
            let mut chars = text.chars();
            let character = chars.next()?;
            if !character.is_ascii() || chars.next().is_some() {
                return None;
            }
            character as u8
        } else {
            let character = key.as_bytes()[0];
            if keystroke.modifiers.shift && character.is_ascii_alphabetic() {
                character.to_ascii_uppercase()
            } else {
                character
            }
        };
        Some((virtual_key, character))
    }

    #[cfg(feature = "gpui-demo-test")]
    #[derive(Clone, Copy)]
    enum CaptureCase {
        Alert,
        ModalDialog,
        ModalDialogChecked,
        ModelessDialog,
        NestedModalDialog,
        Controls,
        ControlsChanged,
        ControlsDragged,
        ControlsHeld,
        Lists,
        ListsSelected,
        TextEdit,
        TextEditSelected,
        TextEditEdited,
        PopupControls,
        PopupControlsSelected,
        StandardFileSave,
        StandardFileSaveComposed,
        StandardFileOpenComposed,
        StandardFileSaveEditedComposed,
    }

    #[cfg(feature = "gpui-demo-test")]
    fn select_showcase_resource_popup_long(session: &mut MacintoshSession) {
        let runner = session.runner_mut();
        let (window_top, window_left, _, _) = runner.window_bounds();
        let (vertical, horizontal) = (window_top + 112, window_left + 280);
        runner.set_mouse_position(vertical, horizontal);
        runner.push_mouse_down(vertical, horizontal);
        for _ in 0..20 {
            runner.run_steps(50_000, None);
        }
        runner.set_mouse_position(window_top + 146, horizontal);
        for _ in 0..20 {
            runner.run_steps(50_000, None);
        }
        runner.push_mouse_up(window_top + 146, horizontal);
        assert!(
            (0..300).any(|_| {
                runner.run_steps(50_000, None);
                runner.control_snapshot().iter().any(|control| {
                    control.visible && control.popup_menu_id == Some(143) && control.value == 4
                })
            }),
            "guest should select the popup's long item"
        );
    }

    #[cfg(feature = "gpui-demo-test")]
    fn capture_fixture_screen(
        game: &std::path::Path,
        output: &std::path::Path,
        prefer_powerpc: bool,
        screen_depth: Option<u16>,
        capture: CaptureCase,
    ) {
        use gpui_kit::{platform, HeadlessAppContext};

        let controls_page = matches!(
            capture,
            CaptureCase::Controls
                | CaptureCase::ControlsChanged
                | CaptureCase::ControlsDragged
                | CaptureCase::ControlsHeld
        );
        let lists_page = matches!(capture, CaptureCase::Lists | CaptureCase::ListsSelected);
        let text_edit_page = matches!(
            capture,
            CaptureCase::TextEdit | CaptureCase::TextEditSelected | CaptureCase::TextEditEdited
        );
        let popup_page = matches!(
            capture,
            CaptureCase::PopupControls | CaptureCase::PopupControlsSelected
        );
        let standard_file_save = matches!(
            capture,
            CaptureCase::StandardFileSave
                | CaptureCase::StandardFileSaveComposed
                | CaptureCase::StandardFileSaveEditedComposed
        );
        let standard_file_open = matches!(capture, CaptureCase::StandardFileOpenComposed);
        let standard_file_page = standard_file_save || standard_file_open;
        let controls_changed = matches!(capture, CaptureCase::ControlsChanged);
        let controls_dragged = matches!(capture, CaptureCase::ControlsDragged);
        let controls_held = matches!(capture, CaptureCase::ControlsHeld);

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
        let (menu_id, item) = if standard_file_page {
            (129, 12)
        } else if matches!(
            capture,
            CaptureCase::ModelessDialog | CaptureCase::NestedModalDialog
        ) {
            (132, 7)
        } else if matches!(capture, CaptureCase::ModalDialog | CaptureCase::ModalDialogChecked) {
            (129, 6)
        } else if lists_page {
            (129, 9)
        } else if text_edit_page {
            (129, 7)
        } else if popup_page {
            (129, 16)
        } else if controls_page {
            (129, 2)
        } else {
            (128, 1)
        };
        assert!(session.runner_mut().select_guest_menu_item(menu_id, item));
        let dialogs = if matches!(
            capture,
            CaptureCase::ModelessDialog | CaptureCase::NestedModalDialog
        ) {
            let modeless = (0..300)
                .find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let dialogs = session.runner_mut().dialog_snapshot();
                    dialogs.iter().any(|dialog| {
                        dialog.visible && dialog.items.len() == 4
                    }).then_some(dialogs)
                })
                .expect("modeless dialog should become visible");
            if matches!(capture, CaptureCase::NestedModalDialog) {
                let modeless_id = modeless
                    .iter()
                    .find(|dialog| dialog.visible && dialog.items.len() == 4)
                    .unwrap()
                    .guest_id;
                assert!(session.runner_mut().select_guest_menu_item(132, 6));
                (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        let dialogs = session.runner_mut().dialog_snapshot();
                        (dialogs.iter().any(|dialog| {
                            dialog.visible && dialog.active && dialog.items.len() == 10
                        }) && dialogs.iter().any(|dialog| {
                            dialog.guest_id == modeless_id && dialog.visible
                        }))
                        .then_some(dialogs)
                    })
                    .expect("nested modal should occlude a visible modeless dialog")
            } else {
                modeless
            }
        } else if matches!(capture, CaptureCase::ModalDialog | CaptureCase::ModalDialogChecked) {
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu| {
                    menu.id == 129
                        && menu.items.iter().any(|item| item.number == 6 && item.checked)
                })
            }));
            for input in [
                MacintoshInput::MouseDown { vertical: 367, horizontal: 170 },
                MacintoshInput::MouseUp { vertical: 367, horizontal: 170 },
            ] {
                session.deliver_input(input);
                session.runner_mut().run_steps(100_000, None);
            }
            let mut dialogs = (0..300)
                .find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let dialogs = session.runner_mut().dialog_snapshot();
                    dialogs.iter().any(|dialog| {
                        dialog.visible
                            && dialog.active
                            && dialog.items.iter().any(|item| {
                                item.kind == DialogItemKind::Checkbox && item.value.is_some()
                            })
                    }).then_some(dialogs)
                })
                .expect("modal preferences dialog should expose a live checkbox");
            if matches!(capture, CaptureCase::ModalDialogChecked) {
                for input in [
                    MacintoshInput::MouseDown { vertical: 155, horizontal: 300 },
                    MacintoshInput::MouseUp { vertical: 155, horizontal: 300 },
                ] {
                    session.deliver_input(input);
                    session.runner_mut().run_steps(100_000, None);
                }
                dialogs = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        let current = session.runner_mut().dialog_snapshot();
                        current.iter().any(|dialog| {
                            dialog.visible && dialog.items.iter().any(|item| {
                                item.number == 4 && item.value == Some(1)
                            })
                        }).then_some(current)
                    })
                    .expect("guest should check the modal dialog control");
            }
            dialogs
        } else if standard_file_page {
            for _ in 0..300 {
                session.runner_mut().run_steps(100_000, None);
                if session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu| {
                    menu.id == 129
                        && menu.items.iter().any(|item| item.number == 12 && item.checked)
                }) {
                    break;
                }
            }
            let tick = session.runner().guest_tick().saturating_add(1);
            session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
            for input in [
                MacintoshInput::MouseDown {
                    vertical: 266,
                    horizontal: if standard_file_open { 126 } else { 400 },
                },
                MacintoshInput::MouseUp {
                    vertical: 266,
                    horizontal: if standard_file_open { 126 } else { 400 },
                },
            ] {
                session.deliver_input(input);
            }
            assert!((0..100).any(|_| {
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                session.runner_mut().standard_file_snapshot().is_some_and(|panel| {
                    panel.kind == if standard_file_open {
                        systemless::runner::StandardFileKind::Get
                    } else {
                        systemless::runner::StandardFileKind::Put
                    }
                        && panel.entries.as_ref().is_some_and(|entries| !entries.is_empty())
                })
            }));
            if matches!(capture, CaptureCase::StandardFileSaveEditedComposed) {
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x00,
                    character: b'S',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x00,
                    character: b'S',
                });
                assert!((0..100).any(|_| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    session
                        .runner_mut()
                        .standard_file_snapshot()
                        .is_some_and(|panel| panel.name.as_deref() == Some("S"))
                }));
            }
            Vec::new()
        } else if popup_page {
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().control_snapshot().iter().any(|control| {
                    control.visible && control.popup_menu_id == Some(143)
                })
            }));
            Vec::new()
        } else if text_edit_page {
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().text_edit_snapshot().records.iter().any(|record| {
                    record.view_rect == (76, 34, 211, 326) && record.line_starts.is_some()
                })
            }));
            Vec::new()
        } else if lists_page {
            for _ in 0..300 {
                session.runner_mut().run_steps(100_000, None);
                if session
                    .runner_mut()
                    .list_manager_snapshot()
                    .iter()
                    .any(|list| list.draw_enabled && list.definition_id == 0)
                {
                    break;
                }
            }
            assert!(session
                .runner_mut()
                .list_manager_snapshot()
                .iter()
                .any(|list| list.draw_enabled && list.definition_id == 0));
            Vec::new()
        } else if controls_page {
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
        if matches!(
            capture,
            CaptureCase::ModelessDialog | CaptureCase::NestedModalDialog
        ) {
            assert!(!super::frames::dialog_item_pieces(
                &dialogs,
                &windows,
                super::frames::Rect::from((0, 0, 600, 800)),
            )
            .is_empty());
        } else if !controls_page && !lists_page && !text_edit_page && !popup_page && !standard_file_page {
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
        if matches!(capture, CaptureCase::ListsSelected) {
            let list = session
                .runner_mut()
                .list_manager_snapshot()
                .into_iter()
                .find(|list| list.draw_enabled && list.definition_id == 0)
                .unwrap();
            let bounds = list.global_view_rect.unwrap();
            let list_id = list.guest_id;
            let point = (bounds.0 + 7 * list.cell_size.0 + list.cell_size.0 / 2, bounds.1 + 480);
            for input in [
                MacintoshInput::MouseDown {
                    vertical: point.0,
                    horizontal: point.1,
                },
                MacintoshInput::MouseUp {
                    vertical: point.0,
                    horizontal: point.1,
                },
            ] {
                session.deliver_input(input);
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            assert!(session
                .runner_mut()
                .list_manager_snapshot()
                .iter()
                .any(|list| list.guest_id == list_id && list.selected.contains(&(7, 0))));
        }
        if matches!(capture, CaptureCase::PopupControlsSelected) {
            select_showcase_resource_popup_long(&mut session);
            let controls = session.runner_mut().control_snapshot();
            let menus = session.runner_mut().guest_menu_snapshot();
            let control = controls
                .iter()
                .find(|control| control.visible && control.popup_menu_id == Some(143))
                .expect("resource-backed popup should remain visible");
            assert_eq!(control.value, 4);
            assert_eq!(
                super::frames::popup_control_label(control, &menus),
                Some("Long-range Expedition Loadout")
            );
        }
        let mut held_drag = None;
        if controls_dragged || controls_held {
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
            let original_value = bar.value;
            let mut inputs = vec![
                MacintoshInput::MouseDown {
                    vertical: from.0,
                    horizontal: from.1,
                },
                MacintoshInput::MouseMove {
                    vertical: to.0,
                    horizontal: to.1,
                },
            ];
            if controls_dragged {
                inputs.push(MacintoshInput::MouseUp {
                    vertical: to.0,
                    horizontal: to.1,
                });
            }
            for input in inputs {
                session.deliver_input(input);
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            let value = session
                .runner_mut()
                .control_snapshot()
                .iter()
                .find(|control| control.guest_id == bar.guest_id)
                .unwrap()
                .value;
            if controls_held {
                assert_eq!(value, original_value);
                held_drag = Some((bar.guest_id, bar.generation, from, to));
            } else {
                assert!(value >= 8);
            }
        }
        let controls = session.runner_mut().control_snapshot();
        let lists = session.runner_mut().list_manager_snapshot();
        if matches!(capture, CaptureCase::TextEditSelected | CaptureCase::TextEditEdited) {
            // The showcase Reset control invokes TESetText then
            // TESetSelect(0, 14); send a real guest click through TrackControl.
            // Inside Macintosh: Text (1993), pp. 2-75--2-78.
            for input in [
                MacintoshInput::MouseDown { vertical: 318, horizontal: 337 },
                MacintoshInput::MouseUp { vertical: 318, horizontal: 337 },
            ] {
                session.deliver_input(input);
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            assert!((0..100).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().text_edit_snapshot().records.iter().any(|record| {
                    record.view_rect == (76, 34, 211, 326) && record.selection == (0, 14)
                })
            }));
            if matches!(capture, CaptureCase::TextEditEdited) {
                for input in [
                    MacintoshInput::KeyDown { mac_key: 0x00, character: b'Z' },
                    MacintoshInput::KeyUp { mac_key: 0x00, character: b'Z' },
                ] {
                    session.deliver_input(input);
                }
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().text_edit_snapshot().records.iter().any(|record| {
                        record.view_rect == (76, 34, 211, 326)
                            && record.text.first() == Some(&b'Z')
                            && record.selection == (1, 1)
                    })
                }));
            }
        }
        let text_edits = session.runner_mut().text_edit_snapshot().records;
        let standard_file = session.runner_mut().standard_file_snapshot();
        let menus = session.runner_mut().guest_menu_snapshot();
        let frame = session.video_frame().unwrap();
        let guest_menu_tracking = session.runner().guest_menu_tracking_active();
        if matches!(capture, CaptureCase::NestedModalDialog) {
            assert!(menus.requires_guest_menu_rendering());
            assert!(!guest_menu_tracking, "nested dialog capture must compose GPUI overlays");
        }
        let top = if menus.requires_guest_menu_rendering() {
            0
        } else {
            u32::from(session.runner().bus().read_word(MBAR_HEIGHT))
        };
        let mut pixels = frame.pixels[(top * frame.width * 4) as usize..].to_vec();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let frame_height = frame.height - top;
        if matches!(capture, CaptureCase::StandardFileSave) {
            image::RgbaImage::from_raw(frame.width, frame_height, pixels)
                .unwrap()
                .save(output)
                .unwrap();
            eprintln!("saved guest Standard File capture to {}", output.display());
            return;
        }

        let mut visual = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            platform::current_headless_renderer,
        );
        visual.update(gpui_kit::init);
        let (sender, _receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let mut view = None;
        let window = visual
            .open_window(size(px(900.), px(740.)), |_, cx| {
                let entity = cx.new(|cx| Demo::new(sender, updates, cx));
                view = Some(entity.clone());
                entity
            })
            .unwrap();
        let view = view.unwrap();
        visual.update(|cx| {
            view.update(cx, |demo, cx| {
                demo.menus = menus;
                demo.guest_menu_tracking = guest_menu_tracking;
                demo.windows = windows;
                demo.dialogs = dialogs;
                demo.controls = controls;
                demo.lists = lists;
                demo.text_edits = text_edits;
                demo.standard_file = standard_file;
                if let Some((id, generation, from, to)) = held_drag {
                    demo.mouse_down = true;
                    demo.mouse_position = to;
                    demo.scrollbar_drag = Some((id, generation, from));
                }
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
        if let Some(output) = args.capture_custom_menu_fallback.as_ref() {
            capture_custom_menu_fallback(output);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modeless_dialog_layout.as_ref() {
            capture_modeless_dialog_layout(output);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_about_alert.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::Alert,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::ModalDialog,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_checked.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::ModalDialogChecked,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modeless_dialog.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::ModelessDialog,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_nested_modal_dialog.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::NestedModalDialog,
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
                CaptureCase::Controls,
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
                CaptureCase::ControlsChanged,
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
                CaptureCase::ControlsDragged,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_controls_held.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::ControlsHeld,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_lists.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::Lists,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_lists_selected.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::ListsSelected,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::TextEdit,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_selected.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::TextEditSelected,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_edited.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::TextEditEdited,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::PopupControls,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls_selected.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::PopupControlsSelected,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_save.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::StandardFileSave,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_save_composed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::StandardFileSaveComposed,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_open_composed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::StandardFileOpenComposed,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_save_edited_composed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::StandardFileSaveEditedComposed,
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

    #[cfg(feature = "gpui-demo-test")]
    fn capture_modeless_dialog_layout(output: &std::path::Path) {
        use gpui_kit::{platform, AppContext, HeadlessAppContext, RenderImage};
        use std::sync::{mpsc, Arc, Mutex};
        use systemless::runner::{DialogItemSnapshot, WindowSnapshot};

        let mut visual = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            platform::current_headless_renderer,
        );
        visual.update(gpui_kit::init);
        let (sender, _receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let mut view = None;
        let window = visual
            .open_window(size(px(360.), px(320.)), |_, cx| {
                let entity = cx.new(|cx| Demo::new(sender, updates, cx));
                view = Some(entity.clone());
                entity
            })
            .unwrap();
        let view = view.unwrap();
        visual.update(|cx| {
            view.update(cx, |demo, cx| {
                demo.width = 300;
                demo.height = 220;
                demo.crop_top = 0;
                demo.windows = vec![
                    WindowFrameSnapshot {
                        guest_id: 2,
                        generation: 1,
                        window: WindowSnapshot {
                            title: "Front".into(),
                            bounds: (90, 90, 150, 160),
                            structure_bounds: Some((70, 85, 155, 165)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(0),
                        close_box: true,
                    },
                    WindowFrameSnapshot {
                        guest_id: 1,
                        generation: 1,
                        window: WindowSnapshot {
                            title: "Modeless".into(),
                            bounds: (50, 50, 180, 240),
                            structure_bounds: Some((30, 45, 185, 245)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: false,
                        },
                        definition_id: Some(4),
                        close_box: false,
                    },
                ];
                demo.dialogs = vec![DialogSnapshot {
                    guest_id: 1,
                    generation: 1,
                    bounds: (50, 50, 180, 240),
                    visible: true,
                    active: false,
                    default_item: None,
                    cancel_item: None,
                    edit_field: None,
                    items: vec![DialogItemSnapshot {
                        number: 1,
                        kind: DialogItemKind::StaticText,
                        bounds: (105, 100, 135, 210),
                        text: "Modeless item".into(),
                        enabled: false,
                        visible: true,
                        value: None,
                        selection: None,
                    }],
                }];
                let mut pixels = image::RgbaImage::new(300, 220);
                for (x, y, pixel) in pixels.enumerate_pixels_mut() {
                    *pixel = if (90..150).contains(&y) && (90..160).contains(&x) {
                        image::Rgba([20, 20, 220, 255])
                    } else {
                        image::Rgba([220, 20, 20, 255])
                    };
                }
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])));
                cx.notify();
            });
        });
        visual.run_until_parked();
        let capture = visual.capture_screenshot(window.into()).unwrap();
        capture.save(output).unwrap();
        let guest_front = *capture.get_pixel(240, 262);
        assert_eq!(*capture.get_pixel(240, 312), guest_front);
        let guest_background = *capture.get_pixel(450, 312);
        assert_ne!(*capture.get_pixel(410, 312), guest_background);
    }

    #[cfg(feature = "gpui-demo-test")]
    fn capture_custom_menu_fallback(output: &std::path::Path) {
        use gpui_kit::{platform, AppContext, HeadlessAppContext, RenderImage};
        use std::sync::{mpsc, Arc, Mutex};
        use systemless::menu_model::{GuestMenu, GuestMenuSnapshot};
        use systemless::runner::WindowSnapshot;

        let mut visual = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            platform::current_headless_renderer,
        );
        visual.update(gpui_kit::init);
        let (sender, _receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let mut view = None;
        let window = visual
            .open_window(
                gpui_kit::size(gpui_kit::px(160.), gpui_kit::px(160.)),
                |_, cx| {
                    let entity = cx.new(|cx| Demo::new(sender, updates, cx));
                    view = Some(entity.clone());
                    entity
                },
            )
            .unwrap();
        let view = view.unwrap();
        visual.update(|cx| {
            view.update(cx, |demo, cx| {
                demo.menus = GuestMenuSnapshot {
                    custom_bar_definition: false,
                    menus: vec![GuestMenu {
                        guest_id: 128,
                        generation: 1,
                        id: 128,
                        title: "Custom".into(),
                        enabled: true,
                        standard_definition: false,
                        hierarchical: false,
                        visible_in_menu_bar: true,
                        items: Vec::new(),
                    }],
                };
                demo.guest_menu_tracking = true;
                demo.width = 64;
                demo.height = 64;
                demo.crop_top = 0;
                // This standard frame crosses the guest menu/content boundary.
                // Fallback must keep its GPUI overlay off the guest pixels.
                demo.windows = vec![WindowFrameSnapshot {
                    guest_id: 7,
                    generation: 1,
                    window: WindowSnapshot {
                        title: "Overlay".into(),
                        bounds: (35, 5, 55, 55),
                        structure_bounds: Some((16, 0, 60, 60)),
                        visible_region: None,
                        update_region: None,
                        visible: true,
                        active: true,
                    },
                    definition_id: Some(0),
                    close_box: true,
                }];
                let mut pixels = image::RgbaImage::new(64, 64);
                for (index, pixel) in pixels.pixels_mut().enumerate() {
                    // Mark the guest's menu rows separately from its content rows.
                    // The session returns RGBA; GPUI's macOS image upload consumes BGRA.
                    *pixel = if index < 64 * 20 {
                        image::Rgba([20, 20, 220, 255])
                    } else {
                        image::Rgba([220, 20, 20, 255])
                    };
                }
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])));
                assert_eq!(
                    demo.pointer(gpui_kit::point(gpui_kit::px(5.), gpui_kit::px(5.))),
                    (5, 5)
                );
                assert!(
                    demo.inside_guest_pane(gpui_kit::point(gpui_kit::px(5.), gpui_kit::px(5.),))
                );
                cx.notify();
            });
        });
        visual.run_until_parked();
        let capture = visual.capture_screenshot(window.into()).unwrap();
        let pixel = capture.get_pixel(5, 5);
        assert_eq!(*pixel, image::Rgba([220, 20, 20, 255]));
        // The headless macOS renderer captures at 2x physical scale.
        assert_eq!(*capture.get_pixel(5, 45), image::Rgba([20, 20, 220, 255]));
        capture.save(output).unwrap();
    }

    #[cfg(test)]
    mod tests {
        use super::{MacintoshInput, MacintoshSession, PathBuf};

        #[test]
        fn open_menu_rebuilds_only_for_its_own_tree() {
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

            let item = |submenu_id| GuestMenuItem {
                number: 1,
                text: "Item".into(),
                enabled: true,
                checked: false,
                key_equivalent: None,
                submenu_id,
                separator: false,
            };
            let menu = |id: i16, submenu_id: Option<i16>| GuestMenu {
                guest_id: id as u32,
                generation: 1,
                id,
                title: format!("Menu {id}"),
                enabled: true,
                standard_definition: true,
                hierarchical: submenu_id.is_none(),
                visible_in_menu_bar: submenu_id.is_some(),
                items: vec![item(submenu_id)],
            };
            let mut snapshot = GuestMenuSnapshot {
                menus: vec![menu(1, Some(3)), menu(2, None), menu(3, None)],
                custom_bar_definition: false,
            };
            let original = super::rendered_menu_tree(&snapshot, 1);
            snapshot.menus[1].items[0].checked = true;
            assert_eq!(super::rendered_menu_tree(&snapshot, 1), original);
            snapshot.menus[2].items[0].checked = true;
            assert_ne!(super::rendered_menu_tree(&snapshot, 1), original);
            snapshot.menus[2].items[0].submenu_id = Some(1);
            assert_eq!(super::rendered_menu_tree(&snapshot, 1).len(), 2);
        }

        #[test]
        fn save_selection_offsets_follow_mac_roman_characters_after_decoding() {
            assert_eq!(
                super::save_name_segments("Résumé", (1, 4), true),
                ("R".into(), "ésu".into(), "mé".into())
            );
            assert_eq!(
                super::save_name_segments("Résumé", (1, 4), false),
                ("Résumé".into(), String::new(), String::new())
            );
        }

        #[test]
        fn worker_routes_command_shortcut_and_rapid_close_button_press() {
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
                        capture_modal_dialog: None,
                        capture_modal_dialog_checked: None,
                        capture_modeless_dialog: None,
                        capture_nested_modal_dialog: None,
                        capture_controls: None,
                        capture_controls_changed: None,
                        capture_controls_dragged: None,
                        capture_controls_held: None,
                        capture_lists: None,
                        capture_lists_selected: None,
                        capture_text_edit: None,
                        capture_text_edit_selected: None,
                        capture_text_edit_edited: None,
                        capture_popup_controls: None,
                        capture_popup_controls_selected: None,
                        capture_standard_file_save: None,
                        capture_standard_file_save_composed: None,
                        capture_standard_file_open_composed: None,
                        capture_standard_file_save_edited_composed: None,
                        capture_custom_menu_fallback: None,
                        capture_modeless_dialog_layout: None,
                    },
                    rx,
                    worker_updates,
                )
            });
            wait(&updates, |u| u.menus.menus.iter().any(|m| m.id == 129));
            tx.send(Command::Input(MacintoshInput::KeyDown {
                mac_key: 0x37,
                character: 0,
            }))
            .unwrap();
            tx.send(Command::Input(MacintoshInput::KeyDown {
                mac_key: 0x23,
                character: b'p',
            }))
            .unwrap();
            let update = wait(&updates, |u| {
                u.menus.menus.iter().any(|menu| {
                    menu.id == 129
                        && menu.items.iter().any(|item| item.number == 5 && item.checked)
                })
            });
            tx.send(Command::Input(MacintoshInput::KeyUp {
                mac_key: 0x23,
                character: b'p',
            }))
            .unwrap();
            tx.send(Command::Input(MacintoshInput::KeyUp {
                mac_key: 0x37,
                character: 0,
            }))
            .unwrap();
            let menu = update.menus.menus.iter().find(|menu| menu.id == 129).unwrap();
            tx.send(Command::Menu(129, 3, menu.guest_id, menu.generation))
                .unwrap();
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
        fn guest_command_p_stays_held_until_release_on_both_cpus() {
            use systemless::memory::{globals::addr::KEY_MAP_LM, MemoryBus};

            let key_is_down = |session: &MacintoshSession, key: u8| {
                session.runner().bus().read_byte(KEY_MAP_LM + u32::from(key / 8))
                    & (1 << (key % 8))
                    != 0
            };
            for powerpc in [false, true] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(8) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x37, character: 0 });
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x23, character: b'p' });
                wait_for_menu(&mut session, 129, 5, true);
                assert!(
                    key_is_down(&session, 0x37),
                    "Command released while held on powerpc={powerpc}"
                );
                assert!(
                    key_is_down(&session, 0x23),
                    "P released while held on powerpc={powerpc}"
                );
                session.runner_mut().run_steps(100_000, None);
                assert!(key_is_down(&session, 0x37));
                assert!(key_is_down(&session, 0x23));
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x23, character: b'p' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x37, character: 0 });
                assert!(!key_is_down(&session, 0x23));
                assert!(!key_is_down(&session, 0x37));
            }
        }

        #[test]
        fn held_shortcut_posts_autokey_at_guest_tick_threshold_on_both_cpus() {
            for powerpc in [false, true] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(8) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x37, character: 0 });
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x23, character: b'p' });

                // The Event Manager posts autoKey after the initial key-repeat
                // threshold, then at the repeat rate. Inside Macintosh Volume I,
                // I-246; Macintosh Toolbox Essentials (1992), pp. 2-29, 2-38.
                for _ in 0..15 {
                    session.runner_mut().force_advance_guest_tick();
                }
                let repeat_count = |session: &MacintoshSession| {
                    session
                        .runner()
                        .event_manager_snapshot()
                        .queued_event_types
                        .iter()
                        .filter(|&&what| what == 5)
                        .count()
                };
                assert_eq!(repeat_count(&session), 0, "early autoKey on powerpc={powerpc}");
                session.runner_mut().force_advance_guest_tick();
                assert_eq!(repeat_count(&session), 1, "missing autoKey on powerpc={powerpc}");
                for _ in 0..3 {
                    session.runner_mut().force_advance_guest_tick();
                }
                assert_eq!(repeat_count(&session), 1, "early repeat on powerpc={powerpc}");
                session.runner_mut().force_advance_guest_tick();
                assert_eq!(repeat_count(&session), 2, "missing repeat on powerpc={powerpc}");
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x23, character: b'p' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x37, character: 0 });
                for _ in 0..4 {
                    session.runner_mut().force_advance_guest_tick();
                }
                assert_eq!(repeat_count(&session), 2, "released key repeated on powerpc={powerpc}");
            }
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
        fn popup_controls_link_live_guest_menus_on_both_cpus() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/toolbox-showcase/toolbox-showcase.sit"))
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 16));
                wait_for_menu(&mut session, 129, 16, true);
                let controls = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        let controls = session.runner_mut().control_snapshot();
                        controls.iter().any(|control| control.visible && (1008..=1023).contains(&control.proc_id))
                            .then_some(controls)
                    })
                    .expect("popup page should expose a standard CDEF control");
                let menus = session.runner_mut().guest_menu_snapshot();
                for (id, title, width) in [(143, "Loadout:", 60), (144, "Theme:", 52)] {
                    let control = controls.iter().find(|control| {
                        control.visible && control.popup_menu_id == Some(id)
                    }).unwrap_or_else(|| panic!("standard popup menu {id} should be visible"));
                    assert!((1008..=1023).contains(&control.proc_id));
                    assert_eq!(control.title, title);
                    assert_eq!(control.popup_title_width, Some(width));
                    assert_eq!(control.value, 1);
                    let menu = menus.menus.iter().find(|menu| menu.id == id)
                        .expect("popup should reference its live guest menu");
                    assert!(!menu.visible_in_menu_bar);
                    assert!(!menu.items.is_empty());
                    assert!(menu.items[0].enabled);
                    assert_eq!(
                        super::super::frames::popup_control_label(control, &menus),
                        Some(menu.items[0].text.as_str()),
                    );
                }
                #[cfg(feature = "gpui-demo-test")]
                {
                    super::select_showcase_resource_popup_long(&mut session);
                    let selected = session.runner_mut().control_snapshot();
                    let menus = session.runner_mut().guest_menu_snapshot();
                    let control = selected
                        .iter()
                        .find(|control| control.visible && control.popup_menu_id == Some(143))
                        .expect("selected resource popup should remain visible");
                    assert_eq!(control.value, 4);
                    assert_eq!(
                        super::super::frames::popup_control_label(control, &menus),
                        Some("Long-range Expedition Loadout"),
                    );
                }
            }
        }

        #[test]
        fn standard_file_snapshots_follow_modal_guest_state_on_both_cpus() {
            use systemless::runner::StandardFileKind;

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
                assert!(session.runner_mut().select_guest_menu_item(129, 12));
                wait_for_menu(&mut session, 129, 12, true);
                settle(&mut session);
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: 266,
                    horizontal: 126,
                });
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: 266,
                    horizontal: 126,
                });
                let opened = (0..100)
                    .find_map(|_| {
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                        session.runner_mut().standard_file_snapshot()
                    })
                    .expect("StandardGetFile should retain its modal panel state");
                assert_eq!(opened.kind, StandardFileKind::Get);
                assert!(opened.standard_entry_point);
                assert_ne!(opened.guest_id, 0);
                assert_ne!(opened.generation, 0);
                assert!(opened.bounds.2 > opened.bounds.0 && opened.bounds.3 > opened.bounds.1);
                assert!(opened.entries.as_ref().is_some_and(|entries| !entries.is_empty()));
                assert!(opened.directory_label.as_ref().is_some_and(|label| !label.is_empty()));
                let layout = opened.get_layout.as_ref().expect("standard Open geometry");
                for rect in [
                    layout.volume,
                    layout.directory_label,
                    layout.list,
                    layout.scroll,
                    layout.eject,
                    layout.desktop,
                    layout.cancel,
                    layout.open,
                ] {
                    assert!(rect.0 >= opened.bounds.0 && rect.2 <= opened.bounds.2);
                    assert!(rect.1 >= opened.bounds.1 && rect.3 <= opened.bounds.3);
                    assert!(rect.2 > rect.0 && rect.3 > rect.1);
                }
                assert!(layout.row_height > 0 && layout.visible_rows > 0);
                let stable = session.runner_mut().standard_file_snapshot().unwrap();
                assert_eq!((stable.guest_id, stable.generation), (opened.guest_id, opened.generation));
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x35,
                    character: 27,
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x35,
                    character: 27,
                });
                assert!((0..100).any(|_| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    session.runner_mut().standard_file_snapshot().is_none()
                }));
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: 266,
                    horizontal: 400,
                });
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: 266,
                    horizontal: 400,
                });
                let saving = (0..100)
                    .find_map(|_| {
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                        session.runner_mut().standard_file_snapshot()
                    })
                    .expect("StandardPutFile should retain its modal panel state");
                assert_eq!(saving.kind, StandardFileKind::Put);
                assert!(saving.standard_entry_point);
                assert!(saving.generation > opened.generation);
                assert!(saving.entries.as_ref().is_some_and(|entries| !entries.is_empty()));
                assert!(saving.name.as_ref().is_some_and(|name| !name.is_empty()));
                assert_eq!(saving.name_selection, Some((0, saving.name.as_ref().unwrap().len())));
                assert_eq!(saving.name_has_focus, Some(true));
                assert!(saving.directory_label.as_ref().is_some_and(|label| !label.is_empty()));
                let layout = saving.put_layout.as_ref().expect("standard Save geometry");
                for rect in [
                    layout.directory_label,
                    layout.list,
                    layout.scroll,
                    layout.prompt,
                    layout.name,
                    layout.desktop,
                    layout.cancel,
                    layout.save,
                ] {
                    assert!(rect.0 >= saving.bounds.0 && rect.2 <= saving.bounds.2);
                    assert!(rect.1 >= saving.bounds.1 && rect.3 <= saving.bounds.3);
                    assert!(rect.2 > rect.0 && rect.3 > rect.1);
                }
                assert!(layout.row_height > 0 && layout.visible_rows > 0);
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x00,
                    character: b'S',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x00,
                    character: b'S',
                });
                let edited = (0..100)
                    .find_map(|_| {
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                        session.runner_mut().standard_file_snapshot()
                            .filter(|panel| panel.name.as_deref() == Some("S"))
                    })
                    .expect("guest StandardPutFile should own save-name editing");
                assert_eq!(edited.generation, saving.generation);
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x35,
                    character: 27,
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x35,
                    character: 27,
                });
                assert!((0..100).any(|_| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    session.runner_mut().standard_file_snapshot().is_none()
                }));
            }
        }

        #[test]
        fn text_edit_snapshots_resolve_guest_port_geometry_on_both_cpus() {
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
                assert!(session.runner_mut().select_guest_menu_item(129, 7));
                wait_for_menu(&mut session, 129, 7, true);
                let first = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .text_edit_snapshot()
                            .records
                            .into_iter()
                            .find(|record| record.view_rect == (76, 34, 211, 326))
                    })
                    .expect("TextEdit page should create its guest TEHandle");
                assert_ne!(first.guest_id, 0);
                assert_ne!(first.generation, 0);
                assert_ne!(first.owner_port, 0);
                assert_eq!(first.global_view_rect, Some((126, 74, 261, 366)));
                assert!(first.global_dest_rect.is_some());
                assert!(!first.styled);
                assert!(first.line_height > 0 && first.size > 0);
                let starts = first.line_starts.as_ref().expect("TextEdit should expose guest lines");
                assert_eq!(starts.first(), Some(&0));
                assert_eq!(starts.last(), Some(&first.text.len()));
                assert_eq!(starts.len(), first.line_count + 1);
                assert_eq!(first.display_lines().unwrap().len(), first.line_count);
                settle(&mut session);
                let next = session
                    .runner_mut()
                    .text_edit_snapshot()
                    .records
                    .into_iter()
                    .find(|record| record.guest_id == first.guest_id)
                    .unwrap();
                assert_eq!(next.generation, first.generation);
                assert_eq!(next.global_view_rect, first.global_view_rect);
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x00,
                    character: b'a',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x00,
                    character: b'a',
                });
                let typed = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .text_edit_snapshot()
                            .records
                            .into_iter()
                            .find(|record| record.guest_id == first.guest_id && record.text != first.text)
                    })
                    .expect("guest TextEdit should receive the typed character");
                assert_eq!(typed.text.len(), first.text.len() + 1);
                assert!(typed.text.contains(&b'a'));
                // Reset invokes guest TESetText/TESetSelect. The next key
                // replaces the selected bytes through the normal TEKey path.
                // Inside Macintosh: Text (1993), pp. 2-75--2-78.
                for input in [
                    MacintoshInput::MouseDown { vertical: 318, horizontal: 337 },
                    MacintoshInput::MouseUp { vertical: 318, horizontal: 337 },
                ] {
                    session.deliver_input(input);
                    for _ in 0..20 {
                        session.runner_mut().run_steps(10_000, None);
                    }
                }
                let selected = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter().find(|record| {
                            record.guest_id == first.guest_id && record.selection == (0, 14)
                        })
                    })
                    .expect("guest Reset should select the first fourteen bytes");
                assert_eq!(selected.text, first.text);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x00, character: b'Z' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x00, character: b'Z' });
                let replaced = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter().find(|record| {
                            record.guest_id == first.guest_id
                                && record.text.first() == Some(&b'Z')
                                && record.selection == (1, 1)
                        })
                    })
                    .expect("guest TEKey should replace the selection and move the caret");
                assert_eq!(replaced.text.len(), first.text.len() - 13);
            }
        }

        #[test]
        fn gpui_printable_keys_keep_mac_virtual_and_typed_character() {
            let key = gpui_kit::Keystroke {
                key: "a".into(),
                key_char: Some("A".into()),
                ..Default::default()
            };
            assert_eq!(super::guest_key(&key), Some((0x00, b'A')));
            let shifted_number = gpui_kit::Keystroke {
                key: "1".into(),
                key_char: Some("!".into()),
                ..Default::default()
            };
            assert_eq!(super::guest_key(&shifted_number), Some((0x12, b'!')));
            let arrow = gpui_kit::Keystroke {
                key: "left".into(),
                ..Default::default()
            };
            assert_eq!(super::guest_key(&arrow), Some((0x7b, 28)));
            let non_roman = gpui_kit::Keystroke {
                key: "a".into(),
                key_char: Some("あ".into()),
                ..Default::default()
            };
            assert_eq!(super::guest_key(&non_roman), None);
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        async fn open_menu_rebuilds_from_live_guest_snapshot(cx: &mut gpui_kit::TestAppContext) {
            use std::time::Duration;
            use gpui_kit::{
                test::{TestAppContextExt, TestWindowExt},
                AppContext, Bounds, WindowBounds, WindowOptions,
            };
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

            let (sender, _receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
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
                    demo.menus = GuestMenuSnapshot {
                        custom_bar_definition: false,
                        menus: vec![GuestMenu {
                            guest_id: 0x1000,
                            generation: 1,
                            id: 129,
                            title: "File".into(),
                            enabled: true,
                            standard_definition: true,
                            hierarchical: false,
                            visible_in_menu_bar: true,
                            items: vec![GuestMenuItem {
                                number: 1,
                                text: "Open".into(),
                                enabled: true,
                                checked: false,
                                key_equivalent: None,
                                submenu_id: None,
                                separator: false,
                            }],
                        }],
                    };
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("guest-menu-4096-1", cx);
                assert_eq!(window.within("popup-menu").find(0usize).label(), Some("Open"));
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus[0].items[0].text = "Open recent".into();
                    demo.menus.menus[0].items[0].checked = true;
                    demo.menus.menus[0].items[0].enabled = false;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let item = window.within("popup-menu").find(0usize);
                assert_eq!(item.label(), Some("Open recent"));
                window.within("popup-menu").press("escape", cx);
            })
            .unwrap();
            cx.wait_for(window.into(), Duration::from_secs(1), |window, _| {
                window.try_find("popup-menu").is_none()
            })
            .await;
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-menu-4096-1", cx);
                assert_eq!(
                    window.within("popup-menu").find(0usize).label(),
                    Some("Open recent")
                );
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus.clear();
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("popup-menu").is_none());
            })
            .unwrap();
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn gpui_key_event_reaches_guest_queue(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, InputEvent, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, WindowBounds, WindowOptions};
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

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
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| demo.focus.focus(window, cx));
                window.render_frame(cx);
                let key = gpui_kit::Keystroke {
                    key: "a".into(),
                    key_char: Some("A".into()),
                    ..Default::default()
                };
                window.dispatch_event(KeyDownEvent {
                    keystroke: key.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }.to_platform_input(), cx);
                window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
            }).unwrap();
            let inputs: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                _ => None,
            }).collect();
            assert!(matches!(inputs.as_slice(), [
                MacintoshInput::KeyDown { mac_key: 0x00, character: b'A' },
                MacintoshInput::KeyUp { mac_key: 0x00, character: b'A' },
            ]));
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus = GuestMenuSnapshot {
                        custom_bar_definition: false,
                        menus: vec![GuestMenu {
                            guest_id: 0x1000,
                            generation: 1,
                            id: 131,
                            title: "File".into(),
                            enabled: false,
                            standard_definition: true,
                            hierarchical: false,
                            visible_in_menu_bar: true,
                            items: vec![GuestMenuItem {
                                number: 1,
                                text: "Preferences".into(),
                                enabled: false,
                                checked: false,
                                key_equivalent: Some('p'),
                                submenu_id: None,
                                separator: false,
                            }],
                        }],
                    };
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| demo.focus.focus(window, cx));
                window.render_frame(cx);
                window.dispatch_event(ModifiersChangedEvent {
                    modifiers: gpui_kit::Modifiers {
                        platform: true,
                        ..Default::default()
                    },
                    capslock: gpui_kit::Capslock { on: false },
                }.to_platform_input(), cx);
                window.dispatch_event(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        key: "p".into(),
                        modifiers: gpui_kit::Modifiers {
                            platform: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    is_held: false,
                    prefer_character_input: false,
                }.to_platform_input(), cx);
                window.dispatch_event(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        key: "p".into(),
                        modifiers: gpui_kit::Modifiers {
                            platform: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    is_held: true,
                    prefer_character_input: false,
                }.to_platform_input(), cx);
            }).unwrap();
            assert!(matches!(receiver.try_recv(), Ok(super::Command::Input(
                MacintoshInput::KeyDown { mac_key: 0x37, character: 0 }
            ))));
            assert!(matches!(receiver.try_recv(), Ok(super::Command::Input(
                MacintoshInput::KeyDown { mac_key: 0x23, character: b'p' }
            ))));
            assert!(receiver.try_recv().is_err());
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(KeyUpEvent {
                    keystroke: gpui_kit::Keystroke {
                        key: "p".into(),
                        modifiers: gpui_kit::Modifiers {
                            platform: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                }.to_platform_input(), cx);
                window.dispatch_event(ModifiersChangedEvent {
                    modifiers: gpui_kit::Modifiers::default(),
                    capslock: gpui_kit::Capslock { on: false },
                }.to_platform_input(), cx);
            }).unwrap();
            assert!(matches!(receiver.try_recv(), Ok(super::Command::Input(
                MacintoshInput::KeyUp { mac_key: 0x23, character: b'p' }
            ))));
            assert!(matches!(receiver.try_recv(), Ok(super::Command::Input(
                MacintoshInput::KeyUp { mac_key: 0x37, character: 0 }
            ))));
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| demo.focus.focus(window, cx));
                window.render_frame(cx);
                window.dispatch_event(KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        key: "p".into(),
                        modifiers: gpui_kit::Modifiers {
                            platform: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    is_held: false,
                    prefer_character_input: false,
                }.to_platform_input(), cx);
                window.blur(cx);
            }).unwrap();
            cx.update_window(window.into(), |_, window, cx| window.render_frame(cx)).unwrap();
            cx.run_until_parked();
            let remaining: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                _ => None,
            }).collect();
            assert!(matches!(remaining.as_slice(), [
                MacintoshInput::KeyDown { mac_key: 0x37, character: 0 },
                MacintoshInput::KeyDown { mac_key: 0x23, character: b'p' },
                MacintoshInput::KeyUp { mac_key: 0x23, character: b'p' },
                MacintoshInput::KeyUp { mac_key: 0x37, character: 0 },
            ]), "{remaining:?}");
        }

        #[test]
        fn list_snapshots_preserve_identity_and_guest_visibility_on_both_cpus() {
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
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                let initial = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().list_manager_snapshot().into_iter().next()
                    })
                    .expect("Lists page should create a guest ListHandle");
                assert_ne!(initial.guest_id, 0);
                assert_ne!(initial.generation, 0);
                assert_eq!(initial.definition_id, 0);
                assert_ne!(initial.owner_port, 0);
                assert_eq!(initial.view_rect, (78, 24, 228, 528));
                assert_eq!(initial.global_view_rect, Some((128, 64, 278, 568)));
                assert_eq!(initial.cells.len(), 12);
                assert_eq!(
                    initial.text_cells.as_ref().unwrap()[&(0, 0)].trim(),
                    "Phase Shifter       01  equipped"
                );
                let row_seven = (initial.global_view_rect.unwrap().0 + 7 * 18 + 9,
                    initial.global_view_rect.unwrap().1 + 480);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: row_seven.0,
                    horizontal: row_seven.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: row_seven.0,
                    horizontal: row_seven.1,
                });
                settle(&mut session);
                assert_eq!(
                    session.runner_mut().list_manager_snapshot()[0].selected,
                    [(7, 0)].into(),
                    "guest List Manager must own selection: {powerpc:?}"
                );
                settle(&mut session);
                let updated = session.runner_mut().list_manager_snapshot().remove(0);
                assert_eq!(
                    (updated.guest_id, updated.generation, updated.owner_port),
                    (initial.guest_id, initial.generation, initial.owner_port),
                    "list identity must survive presentation updates: {powerpc:?}"
                );
                assert!(session.runner_mut().select_guest_menu_item(129, 1));
                wait_for_menu(&mut session, 129, 1, true);
                let off_page = session.runner_mut().list_manager_snapshot().remove(0);
                assert_eq!(off_page.guest_id, initial.guest_id);
                assert_eq!(off_page.generation, initial.generation);
                assert_eq!(off_page.global_view_rect, initial.global_view_rect);
                assert!(!off_page.draw_enabled && !off_page.active);
                assert_eq!(off_page.vertical_scrollbar, Some((false, 254)));
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                let restored = (0..100)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .list_manager_snapshot()
                            .into_iter()
                            .find(|list| list.draw_enabled && list.active)
                    })
                    .expect("Lists page should show the retained guest list");
                assert_eq!(restored.guest_id, initial.guest_id);
                assert_eq!(restored.generation, initial.generation);
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
                let mut mixed = dialog.clone();
                mixed.items[1].kind = DialogItemKind::Checkbox;
                mixed.items[1].value = Some(1);
                mixed.items.push(systemless::runner::DialogItemSnapshot {
                    number: 3,
                    kind: DialogItemKind::EditText,
                    bounds: (250, 320, 270, 430),
                    text: "Pilot".into(),
                    enabled: true,
                    visible: true,
                    value: None,
                    selection: None,
                });
                assert!(super::standard_dbox_dialog(&[mixed.clone()], &windows).is_some());
                mixed.items[1].value = None;
                assert!(super::standard_dbox_dialog(&[mixed.clone()], &windows).is_none());
                mixed.items[1].kind = DialogItemKind::UserItem;
                assert!(super::standard_dbox_dialog(&[mixed], &windows).is_none());
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

        #[test]
        fn modal_dialog_checkbox_tracks_guest_value_across_guest_modes() {
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
                wait_for_menu(&mut session, 129, 6, false);
                assert!(session.runner_mut().select_guest_menu_item(129, 6));
                wait_for_menu(&mut session, 129, 6, true);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: 367,
                    horizontal: 170,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: 367,
                    horizontal: 170,
                });
                let dialog = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        let dialogs = session.runner_mut().dialog_snapshot();
                        dialogs.into_iter().find(|dialog| {
                            dialog.visible
                                && dialog.active
                                && dialog.items.iter().any(|item| {
                                    item.kind == DialogItemKind::Checkbox && item.value == Some(0)
                                })
                        })
                    })
                    .expect("preferences dialog should expose unchecked guest controls");
                assert_eq!(dialog.edit_field, Some(7));
                assert_eq!(dialog.items[6].selection, Some((0, 0)));
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x07,
                    character: b'X',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x07,
                    character: b'X',
                });
                assert!(
                    (0..20).any(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().dialog_snapshot().iter().any(|current| {
                            current.guest_id == dialog.guest_id
                                && current.items[6].text == "XCade Connelly"
                                && current.items[6].selection == Some((1, 1))
                        })
                    }),
                    "guest should insert before the dialog's initial text on {powerpc:?}"
                );
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x33,
                    character: 0x08,
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x33,
                    character: 0x08,
                });
                assert!((0..20).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id
                            && current.items[6].text == "Cade Connelly"
                            && current.items[6].selection == Some((0, 0))
                    })
                }));
                let windows = session.runner_mut().window_frame_snapshot();
                assert!(super::standard_dbox_dialog(&[dialog.clone()], &windows).is_some());
                let checkbox = dialog.items.iter().find(|item| item.number == 4).unwrap();
                let point = (
                    (checkbox.bounds.0 + checkbox.bounds.2) / 2,
                    checkbox.bounds.1 + 12,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: point.0,
                    horizontal: point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: point.0,
                    horizontal: point.1,
                });
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id
                            && current.generation == dialog.generation
                            && current.items.iter().any(|item| {
                                item.number == checkbox.number && item.value == Some(1)
                            })
                        })
                }));
                let second_edit = dialog.items.iter().find(|item| item.number == 9).unwrap();
                let second_point = (
                    (second_edit.bounds.0 + second_edit.bounds.2) / 2,
                    (second_edit.bounds.1 + second_edit.bounds.3) / 2,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: second_point.0,
                    horizontal: second_point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: second_point.0,
                    horizontal: second_point.1,
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id && current.edit_field == Some(9)
                    })
                }));
            }
        }

        #[test]
        fn modeless_dialog_tracks_guest_lifecycle_on_both_cpus() {
            for powerpc in [false, true] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(8) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(132, 7));
                let dialog = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .dialog_snapshot()
                            .into_iter()
                            .find(|dialog| dialog.visible && dialog.items.len() == 4)
                    })
                    .expect("modeless guest dialog should open");
                let windows = session.runner_mut().window_frame_snapshot();
                assert_eq!(windows[0].guest_id, dialog.guest_id);
                assert_eq!(windows[0].definition_id, Some(4));
                assert!(dialog.active);
                assert!(!crate::frames::dialog_item_pieces(
                    &[dialog.clone()],
                    &windows,
                    crate::frames::Rect::from((0, 0, 600, 800)),
                )
                .is_empty());

                let checkbox = &dialog.items[1];
                let point = (
                    (checkbox.bounds.0 + checkbox.bounds.2) / 2,
                    (checkbox.bounds.1 + checkbox.bounds.3) / 2,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: point.0,
                    horizontal: point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: point.0,
                    horizontal: point.1,
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id && current.items[1].value == Some(1)
                    })
                }));

                let edit = &dialog.items[3];
                let edit_point = (
                    (edit.bounds.0 + edit.bounds.2) / 2,
                    (edit.bounds.1 + edit.bounds.3) / 2,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: edit_point.0,
                    horizontal: edit_point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: edit_point.0,
                    horizontal: edit_point.1,
                });
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x06,
                    character: b'z',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x06,
                    character: b'z',
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id
                            && current.items[3].text.contains('z')
                    })
                }), "modeless edit field should accept guest key input on {powerpc:?}");

                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: 70,
                    horizontal: 70,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: 70,
                    horizontal: 70,
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id && current.visible && !current.active
                    })
                }));

                assert!(session.runner_mut().select_guest_menu_item(132, 7));
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id
                            && current.active
                            && current.items[3].text.contains('z')
                    })
                }));
                let close = &dialog.items[0];
                let close_point = (
                    (close.bounds.0 + close.bounds.2) / 2,
                    (close.bounds.1 + close.bounds.3) / 2,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: close_point.0,
                    horizontal: close_point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: close_point.0,
                    horizontal: close_point.1,
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session
                        .runner_mut()
                        .dialog_snapshot()
                        .iter()
                        .all(|current| current.guest_id != dialog.guest_id)
                }));
            }
        }

        #[test]
        fn nested_modal_dialog_keeps_modeless_text_out_of_its_key_path() {
            for powerpc in [false, true] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(8) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(132, 7));
                let modeless = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .dialog_snapshot()
                            .into_iter()
                            .find(|dialog| dialog.visible && dialog.items.len() == 4)
                    })
                    .expect("modeless dialog should open");
                assert_eq!(modeless.items[3].text, "Pilot");

                assert!(session.runner_mut().select_guest_menu_item(132, 6));
                let modal = (0..300)
                    .find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session
                            .runner_mut()
                            .dialog_snapshot()
                            .into_iter()
                            .find(|dialog| dialog.visible && dialog.items.len() == 10)
                    })
                    .expect("nested modal dialog should open");
                assert!(modal.active);
                assert!(!session.runner().guest_menu_tracking_active());
                assert!(session.runner_mut().dialog_snapshot().iter().any(|current| {
                    current.guest_id == modeless.guest_id && current.visible && !current.active
                }), "covered modeless dialog must not present focus on {powerpc:?}");
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x07,
                    character: b'X',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x07,
                    character: b'X',
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let dialogs = session.runner_mut().dialog_snapshot();
                    dialogs.iter().any(|current| {
                        current.guest_id == modal.guest_id
                            && current.items[6].text == "XCade Connelly"
                    }) && dialogs.iter().any(|current| {
                        current.guest_id == modeless.guest_id
                            && current.items[3].text == "Pilot"
                    })
                }), "nested modal key input reached the wrong dialog on {powerpc:?}");

                let cancel = &modal.items[1];
                let cancel_point = (
                    (cancel.bounds.0 + cancel.bounds.2) / 2,
                    (cancel.bounds.1 + cancel.bounds.3) / 2,
                );
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: cancel_point.0,
                    horizontal: cancel_point.1,
                });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp {
                    vertical: cancel_point.0,
                    horizontal: cancel_point.1,
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let dialogs = session.runner_mut().dialog_snapshot();
                    dialogs.iter().all(|current| current.guest_id != modal.guest_id)
                        && dialogs.iter().any(|current| {
                            current.guest_id == modeless.guest_id
                                && current.visible
                                && current.active
                                && current.items[3].text == "Pilot"
                        })
                }), "modeless dialog should regain focus after nested modal dismissal on {powerpc:?}");
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x06,
                    character: b'z',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x06,
                    character: b'z',
                });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == modeless.guest_id
                            && current.active
                            && current.items[3].text == "zPilot"
                    })
                }), "modeless edit should accept input after nested modal dismissal on {powerpc:?}");
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn held_pointer_routes_across_guest_pane_boundary(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};

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
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.drag(
                    gpui_kit::point(gpui_kit::px(780.), gpui_kit::px(100.)),
                    gpui_kit::point(gpui_kit::px(780.), gpui_kit::px(20.)),
                    cx,
                );
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
            assert!(inputs.iter().any(|input| matches!(
                input,
                MacintoshInput::MouseMove {
                    vertical: 20,
                    horizontal: 780
                }
            )));
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn off_window_release_clears_guest_held_button(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};

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
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.drag(
                    gpui_kit::point(gpui_kit::px(780.), gpui_kit::px(100.)),
                    gpui_kit::point(gpui_kit::px(-50.), gpui_kit::px(-50.)),
                    cx,
                );
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
            assert!(matches!(
                inputs.last(),
                Some(MacintoshInput::MouseUp {
                    vertical: 20,
                    horizontal: 0
                })
            ));
            assert!(!view.read_with(cx, |demo, _| demo.mouse_down));
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn focus_loss_releases_guest_button_once(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::AppContext;

            let (sender, receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let view = cx.update(|cx| cx.new(|cx| super::Demo::new(sender, updates, cx)));
            cx.update(|cx| {
                view.update(cx, |demo, _| {
                    demo.mouse_down = true;
                    demo.mouse_position = (92, 137);
                    demo.scrollbar_drag = Some((1, 1, (92, 137)));
                    demo.popup_tracking = Some((2, 1));
                    demo.release_host_input();
                    demo.release_host_input();
                    assert!(!demo.mouse_down);
                    assert!(demo.scrollbar_drag.is_none());
                    assert!(demo.popup_tracking.is_none());
                });
            });
            let inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .collect();
            assert!(matches!(
                inputs.as_slice(),
                [MacintoshInput::MouseUp {
                    vertical: 92,
                    horizontal: 137
                }]
            ));
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
        fn themed_dialog_checkbox_and_edit_field_forward_guest_clicks(
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
                    let bounds = (100, 130, 315, 470);
                    demo.windows = vec![WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: WindowSnapshot {
                            title: "Preferences".into(),
                            bounds,
                            structure_bounds: Some((92, 122, 323, 478)),
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
                        default_item: None,
                        cancel_item: None,
                        edit_field: Some(2),
                        items: vec![
                            DialogItemSnapshot {
                                number: 1,
                                kind: super::DialogItemKind::Checkbox,
                                bounds: (145, 150, 165, 450),
                                text: "Enable 3D".into(),
                                enabled: true,
                                visible: true,
                                value: Some(0),
                                selection: None,
                            },
                            DialogItemSnapshot {
                                number: 2,
                                kind: super::DialogItemKind::EditText,
                                bounds: (200, 235, 220, 430),
                                text: "Pilot".into(),
                                enabled: true,
                                visible: true,
                                value: None,
                                selection: Some((0, 0)),
                            },
                        ],
                    }];
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-dialog-checkbox-7-1-1", cx);
            })
            .unwrap();
            let inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .filter(|input| {
                    matches!(
                        input,
                        MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }
                    )
                })
                .collect();
            assert_eq!(inputs.len(), 2, "{inputs:?}");
            assert!(matches!(
                inputs.as_slice(),
                [
                    MacintoshInput::MouseDown {
                        vertical: 145..=164,
                        horizontal: 150..=449,
                    },
                    MacintoshInput::MouseUp {
                        vertical: 145..=164,
                        horizontal: 150..=449,
                    },
                ]
            ));
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-dialog-edit-7-1-2", cx);
            })
            .unwrap();
            let edit_inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .filter(|input| {
                    matches!(
                        input,
                        MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }
                    )
                })
                .collect();
            assert!(matches!(
                edit_inputs.as_slice(),
                [
                    MacintoshInput::MouseDown {
                        vertical: 200..=219,
                        horizontal: 235..=429,
                    },
                    MacintoshInput::MouseUp {
                        vertical: 200..=219,
                        horizontal: 235..=429,
                    },
                ]
            ));
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn themed_standard_file_actions_forward_guest_clicks(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::runner::{
                StandardFileEntrySnapshot, StandardFileGetLayout, StandardFileKind,
                StandardFilePutLayout, StandardFileSnapshot,
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
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    demo.standard_file = Some(StandardFileSnapshot {
                        guest_id: 7,
                        generation: 1,
                        kind: StandardFileKind::Get,
                        standard_entry_point: true,
                        bounds: (100, 100, 278, 456),
                        directory_id: 2,
                        entries: Some(vec![StandardFileEntrySnapshot {
                            name: "Documents".into(),
                            directory_id: 3,
                            is_directory: true,
                            file_type: 0,
                        }]),
                        selected: Some(0),
                        prompt: None,
                        name: None,
                        name_selection: None,
                        name_has_focus: None,
                        directory_label: Some("MacintoshHD".into()),
                        get_layout: Some(StandardFileGetLayout {
                            volume: (112, 190, 131, 264),
                            directory_label: (112, 368, 131, 436),
                            list: (135, 118, 263, 336),
                            scroll: (135, 335, 263, 351),
                            eject: (138, 358, 159, 438),
                            desktop: (166, 358, 187, 438),
                            cancel: (210, 358, 231, 438),
                            open: (238, 358, 259, 438),
                            row_height: 14,
                            first_visible: 0,
                            visible_rows: 8,
                        }),
                        put_layout: None,
                    });
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("guest-standard-open-7-1-Open", cx);
            })
            .unwrap();
            let open_inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .collect();
            let open_presses: Vec<_> = open_inputs
                .iter()
                .filter(|input| matches!(input, MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }))
                .collect();
            assert!(matches!(open_presses.as_slice(), [
                MacintoshInput::MouseDown { vertical: 238..=258, horizontal: 358..=437 },
                MacintoshInput::MouseUp { vertical: 238..=258, horizontal: 358..=437 },
            ]), "{open_inputs:?}");

            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.standard_file = Some(StandardFileSnapshot {
                        guest_id: 8,
                        generation: 2,
                        kind: StandardFileKind::Put,
                        standard_entry_point: true,
                        bounds: (100, 100, 360, 460),
                        directory_id: 2,
                        entries: Some(Vec::new()),
                        selected: None,
                        prompt: Some("Save as:".into()),
                        name: Some("Untitled".into()),
                        name_selection: Some((0, 8)),
                        name_has_focus: Some(true),
                        directory_label: Some("MacintoshHD".into()),
                        get_layout: None,
                        put_layout: Some(StandardFilePutLayout {
                            directory_label: (112, 224, 131, 436),
                            list: (138, 118, 256, 416),
                            scroll: (138, 415, 256, 431),
                            prompt: (266, 124, 284, 430),
                            name: (288, 124, 308, 430),
                            desktop: (320, 124, 342, 204),
                            cancel: (320, 266, 342, 346),
                            save: (320, 358, 342, 438),
                            row_height: 14,
                            first_visible: 0,
                            visible_rows: 8,
                        }),
                    });
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("guest-standard-save-8-2-Save", cx);
            })
            .unwrap();
            let save_inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    _ => None,
                })
                .collect();
            let save_presses: Vec<_> = save_inputs
                .iter()
                .filter(|input| matches!(input, MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }))
                .collect();
            assert!(matches!(save_presses.as_slice(), [
                MacintoshInput::MouseDown { vertical: 320..=341, horizontal: 358..=437 },
                MacintoshInput::MouseUp { vertical: 320..=341, horizontal: 358..=437 },
            ]), "{save_inputs:?}");
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn themed_list_row_forwards_guest_click(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::runner::{ListManagerSnapshot, WindowFrameSnapshot, WindowSnapshot};

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
                    demo.windows = vec![WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: WindowSnapshot {
                            title: "List".into(),
                            bounds: (50, 40, 420, 600),
                            structure_bounds: Some((31, 39, 422, 602)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(0),
                        close_box: false,
                    }];
                    demo.lists = vec![ListManagerSnapshot {
                        guest_id: 10,
                        generation: 1,
                        definition_id: 0,
                        owner_port: 7,
                        global_view_rect: Some((128, 64, 278, 568)),
                        view_rect: (78, 24, 228, 528),
                        data_bounds: (0, 0, 12, 1),
                        cell_size: (18, 504),
                        visible: (0, 0, 9, 1),
                        draw_enabled: true,
                        active: true,
                        cells: [((7, 0), b"Signal Beacon".to_vec())].into(),
                        text_cells: Some([((7, 0), "Signal Beacon".into())].into()),
                        selected: Default::default(),
                        vertical_scrollbar: None,
                        horizontal_scrollbar: None,
                    }];
                    demo.width = 800;
                    demo.height = 580;
                    demo.crop_top = 20;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("guest-list-cell-10-1-7-0", cx);
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
            assert!(inputs.iter().any(|input| matches!(
                input,
                MacintoshInput::MouseDown {
                    vertical: 263,
                    horizontal: 316
                }
            )));
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
                        popup_menu_id: None,
                        popup_title_width: None,
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
