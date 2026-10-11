//! Shared macOS GPUI frontend and headless presentation harness.

pub(crate) struct LaunchOptions {
    pub game: std::path::PathBuf,
    pub prefer_powerpc: bool,
    pub screen_depth: Option<u16>,
    pub addressing_24_bit: bool,
    pub arrows_as_numpad: bool,
    pub display_scale: Option<u32>,
    pub ui_theme: systemless::ui_theme::UiThemeId,
    pub fullscreen: bool,
    pub debug_socket: Option<std::path::PathBuf>,
    pub native_integrations: bool,
}

#[cfg(target_os = "macos")]
pub(crate) fn launch(options: LaunchOptions) {
    desktop::launch(options);
}

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
#[path = "desktop/cpu_frame.rs"]
mod cpu_frame;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_text.rs"]
mod text;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_input.rs"]
mod input;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_scroll.rs"]
mod scroll;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_frames.rs"]
mod frames;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_a11y.rs"]
mod a11y;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_metrics.rs"]
mod metrics;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_popup.rs"]
mod popup;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_choices.rs"]
mod choices;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_activation.rs"]
mod activation;

#[cfg(target_os = "macos")]
#[path = "gpui_demo_clipboard.rs"]
mod clipboard;

#[cfg(target_os = "macos")]
#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;

#[cfg(target_os = "macos")]
#[path = "desktop/native_application.rs"]
mod native_application;

#[cfg(all(feature = "debug-server", unix))]
#[path = "desktop/debug_server.rs"]
mod debug_server;

#[cfg(target_os = "macos")]
mod desktop {
    //! Opt-in GPUI Kit presentation experiment for live guest menus.

    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
        sync::{mpsc, Arc, Mutex},
        time::{Duration, Instant},
    };

    use clap::Parser;
    use gpui_kit::{
        component::{
            button::{Button, ButtonVariants},
            popover::Popover,
            ActiveTheme, Disableable, Sizable,
        },
        prelude::*,
        *,
    };
    use systemless::{
        memory::{globals::addr::MBAR_HEIGHT, MemoryBus},
        menu_model::{GuestMenu, GuestMenuSnapshot},
        runner::{default_realtime_instructions_per_tick, ControlSnapshot, DialogItemKind, DialogSnapshot, ListManagerSnapshot, StandardFileKind, StandardFileSnapshot, TextEditSnapshot, WindowFrameSnapshot},
        systems::macintosh::{game, session::{MacintoshInput, MacintoshSession}},
    };

    use super::input::{guest_key, guest_virtual_key};

    include!("gpui_demo_menu.rs");

    #[derive(Parser)]
    #[command(about = "GPUI Kit Macintosh runner")]
    struct Args {
        game: PathBuf,
        #[arg(long)]
        prefer_powerpc: bool,
        #[arg(long, value_parser = parse_depth)]
        screen_depth: Option<u16>,
        #[arg(skip)]
        options: Option<super::LaunchOptions>,
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
        capture_modal_dialog_selection: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_selection_inactive: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_caret_visible: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_caret_hidden: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_button_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_button_outside: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_checkbox_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_checkbox_checked_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modal_dialog_checkbox_outside: Option<PathBuf>,
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
        capture_radio_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_radio_outside: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_radio_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_retain_native_source: bool,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_held: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_cancelled: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_lists_transition: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true, default_value = "scrolled", value_parser = ["scrolled", "inactive", "reactivated", "mutated", "resized"])]
        capture_list_transition: String,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true, value_parser = parse_capture_scale)]
        capture_scale: Option<f32>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_ink: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_multiline: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_caret: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true, default_value_t = 26)]
        capture_styled_caret_offset: usize,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true, default_value = "visible",
            value_parser = ["visible", "blink-off", "suspended", "resumed"])]
        capture_styled_caret_state: String,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_selected_suspended: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_styled_text_edit_selected_resumed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_edited: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_inactive: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_reactivated: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_host_suspended: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_text_edit_host_resumed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_selected: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_host_suspended: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_disabled: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_open: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_popup_controls_scrolled: Option<PathBuf>,
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
        capture_standard_file_save_caret_hidden_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_replace_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_new_folder_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_new_folder_error_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_new_folder_selected_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_new_folder_long_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_file_new_folder_caret_hidden_composed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_custom_menu_fallback: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_standard_menu: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true, default_value_t = 129)]
        capture_standard_menu_id: i16,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_moved: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_activated: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_grown: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_zoomed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_zoom_restored: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_custom_zoomed: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_custom_zoom_restored: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_promoted: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_windows_main_promoted: Option<PathBuf>,
        #[cfg(feature = "gpui-demo-test")]
        #[arg(long, hide = true)]
        capture_modeless_dialog_layout: Option<PathBuf>,
    }

    #[cfg(feature = "gpui-demo-test")]
    fn parse_capture_scale(value: &str) -> Result<f32, String> {
        let scale: f32 = value.parse().map_err(|_| "capture scale must be a number")?;
        if !scale.is_finite() || !(0.25..=4.).contains(&scale) {
            return Err("capture scale must be finite and between 0.25 and 4".into());
        }
        Ok(scale)
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
        ImportClipboard(Vec<u8>),
        Foreground(bool, u64),
        Menu(i16, i16, u32, u64),
        Input(MacintoshInput),
        Wheel(super::scroll::WheelRequest),
        ActivateControl(u32, u64),
        ActivateDialog(u32, u64, i16, Option<(u32, u64)>),
        ActivateFile(u32, u64, super::activation::FileAction),
        CancelWheel,
        Shutdown,
    }

    #[derive(Default)]
    struct LiveMenuState {
        menu: Option<Entity<GuestMenuPopup>>,
        rendered: Option<Vec<GuestMenu>>,
    }

    struct PreparedButtonImage {
        source: Arc<RenderImage>,
        clips: Vec<super::frames::Rect>,
        background: [u8; 4],
        image: Arc<RenderImage>,
    }

    #[derive(Default)]
    struct Update {
        identity: Option<Arc<game::ApplicationIdentity>>,
        menus: GuestMenuSnapshot,
        menu_presented: bool,
        menu_height: u16,
        guest_menu_tracking: bool,
        guest_popup: Option<systemless::menu_model::GuestPopupSnapshot>,
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
        lists: Vec<ListManagerSnapshot>,
        list_text_plans: Vec<std::collections::BTreeMap<(i16, i16), super::text::ClassicListCellPaintPlan>>,
        text_edits: Vec<TextEditSnapshot>,
        styled_text_plans: Vec<Option<super::text::StyledTextEditPaintPlan>>,
        standard_file: Option<StandardFileSnapshot>,
        frame: Option<(u32, u32, Vec<u8>)>,
        clipboard_export: Option<(u64, Vec<u8>)>,
        status: String,
    }

    // Both live rendering and composed captures preserve the complete guest
    // coordinate space. GPUI RenderImage consumes BGRA pixels.
    fn gpui_pixels(mut pixels: Vec<u8>) -> Vec<u8> {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        pixels
    }

    fn configure_realtime_execution(session: &mut MacintoshSession) -> u32 {
        let instructions_per_tick =
            default_realtime_instructions_per_tick(session.runner().is_powerpc_app());
        session
            .runner_mut()
            .set_instructions_per_tick(instructions_per_tick);
        instructions_per_tick
    }

    fn run_guest(
        args: Args,
        commands: mpsc::Receiver<Command>,
        updates: Arc<Mutex<Option<Update>>>,
        host_services: bool,
    ) {
        let result = std::panic::catch_unwind(|| {
            let mut session = MacintoshSession::new(
                !args.options.as_ref().is_some_and(|options| options.addressing_24_bit),
                args.screen_depth,
            );
            if let Some(options) = args.options.as_ref() {
                session.runner_mut().set_ui_theme(options.ui_theme);
            }
            session
                .runner_mut()
                .set_prefer_powerpc_executables(args.prefer_powerpc);
            let app = session.load_path(&args.game)?;
            let mut save_store = host_services.then(|| {
                let mut store = super::desktop_save_store::DesktopSaveStore::for_loaded_archive(
                    &args.game, session.runner_mut(),
                );
                for file in store.load_saved_files() {
                    session.runner_mut().import_vfs_file(&file);
                }
                store
            });
            session.initialize(&app);
            let identity = host_services.then(|| game::loaded_application_identity(session.runner()))
                .flatten().filter(|_| args.options.as_ref().is_none_or(|options| options.native_integrations)).map(Arc::new);
            #[cfg(feature = "debug-server")]
            let mut debug_server = args.options.as_ref().and_then(|options| options.debug_socket.as_ref())
                .map(|path| super::debug_server::DebugServer::bind(path).expect("bind debugger socket"));
            // Keep the device and its stream on the guest worker, matching
            // the ordinary desktop runner's stereo delivery and lifetime.
            if host_services {
                if let Some(audio) = systemless::systems::macintosh::audio::CpalAudioBackend::new() {
                    session.runner_mut().set_audio(Box::new(audio));
                } else {
                    eprintln!("[GPUI] Could not initialize audio output");
                }
            }
            let instructions_per_tick = configure_realtime_execution(&mut session);
            let architecture = if session.status().powerpc_application {
                "PowerPC"
            } else {
                "68k"
            };
            let mut epoch = Instant::now();
            let mut initial_tick = session.runner().guest_tick();
            let mut previous = epoch;
            let mut queued = std::collections::VecDeque::new();
            let mut wheel: Option<super::scroll::WheelClick> = None;
            let mut activation: Option<super::activation::ControlActivation> = None;
            let mut pointer_down = false;
            let mut clipboard = super::clipboard::GuestClipboard::default();
            loop {
                let start = Instant::now();
                #[cfg(feature = "debug-server")]
                if let Some(server) = debug_server.as_mut() {
                    server.pump(session.runner_mut());
                }
                loop {
                    match commands.try_recv() {
                        Ok(Command::CancelWheel) => {
                            queued.retain(|command| !matches!(command, Command::Wheel(_)));
                            if let Some(click) = wheel.as_mut() { click.stop_repeating(); }
                        }
                        Ok(Command::Wheel(request)) => {
                            if let Some(Command::Wheel(previous)) = queued.back_mut() {
                                if previous.target == request.target && previous.origin == request.origin {
                                    previous.steps = previous.steps.saturating_add(request.steps).clamp(-4, 4);
                                    continue;
                                }
                            }
                            queued.push_back(Command::Wheel(request));
                        }
                        Ok(command) => queued.push_back(command),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            queued.push_back(Command::Shutdown);
                            break;
                        }
                    }
                }
                if let Some(mut click) = wheel.take() {
                    if !queued.is_empty() { click.stop_repeating(); }
                    wheel = click.advance(&mut session);
                }
                if let Some(click) = activation.take() {
                    activation = click.advance(&mut session);
                }
                while wheel.is_none() && activation.is_none() {
                    match queued.pop_front().ok_or(mpsc::TryRecvError::Empty) {
                        Ok(Command::CancelWheel) => {}
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
                        Ok(Command::Wheel(request)) => {
                            if !pointer_down && !session.runner().is_ui_tracking_active() {
                                wheel = super::scroll::WheelClick::begin(&mut session, request);
                            }
                        }
                        Ok(Command::ActivateControl(id, generation)) => {
                            if !pointer_down {
                                activation = super::activation::ControlActivation::begin(&mut session, id, generation);
                            }
                        }
                        Ok(Command::ActivateDialog(id, generation, number, identity)) => {
                            if !pointer_down {
                                activation = super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, identity);
                            }
                        }
                        Ok(Command::ActivateFile(id, generation, action)) => {
                            if !pointer_down {
                                activation = super::activation::ControlActivation::begin_file(&mut session, id, generation, action);
                            }
                        }
                        Ok(Command::ImportClipboard(text)) => {
                            clipboard.imported(&text);
                            session.import_clipboard_text(text);
                        }
                        Ok(Command::Foreground(active, generation)) => {
                            clipboard.foreground(active, generation);
                            session.request_foreground(active);
                        }
                        Ok(Command::Input(input)) => {
                            match input {
                                MacintoshInput::MouseDown { .. } => pointer_down = true,
                                MacintoshInput::MouseUp { .. } => pointer_down = false,
                                _ => {}
                            }
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
                        Ok(Command::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                            if let Some(store) = save_store.as_mut() {
                                store.sync_save_files_now(session.runner_mut());
                            }
                            return Ok::<(), String>(());
                        }
                    }
                }
                session
                    .runner_mut()
                    .advance_menu_presentation_clock(start.duration_since(previous));
                previous = start;
                let current_tick = session.runner().guest_tick();
                let mut due_tick = initial_tick
                    .saturating_add((epoch.elapsed().as_secs_f64() * 60.) as u32)
                    .saturating_add(1);
                if due_tick.saturating_sub(current_tick) > 4 {
                    // A late frame must not make the worker chase an ever-growing
                    // backlog. The ordinary desktop runner also rebases here.
                    epoch = start;
                    initial_tick = current_tick;
                    due_tick = current_tick.saturating_add(1);
                }
                let deadline = due_tick.min(current_tick.saturating_add(2));
                let batch = if session.runner().is_powerpc_app() {
                    (instructions_per_tick as usize).div_ceil(16)
                } else {
                    10_000
                };
                let cpu_deadline = start + Duration::from_millis(12);
                super::cpu_frame::advance(session.runner_mut(), batch, deadline, cpu_deadline);
                session.runner_mut().mix_gui_audio_slice(367);
                if session.runner().has_pending_sound_work()
                    && Instant::now() < cpu_deadline
                {
                    session.runner_mut().run_gui_pending_sound_work(10_000);
                }
                session.runner_mut().finish_gui_frame();
                clipboard.observe(&session);
                session.drain_audio();
                if let Some(store) = save_store.as_mut() {
                    store.sync_save_files(session.runner_mut());
                }
                let menus = session.runner_mut().guest_menu_snapshot();
                let menu_presented = session.runner().guest_menu_bar_presented();
                let menu_height = session.runner().bus().read_word(MBAR_HEIGHT);
                let guest_menu_tracking = session.runner().guest_menu_tracking_active();
                let guest_popup = session.runner_mut().guest_popup_snapshot();
                let frame = session.video_frame();
                let running = session.status().running;
                let windows = session.runner_mut().window_frame_snapshot();
                let dialogs = session.runner_mut().dialog_snapshot();
                let controls = session.runner_mut().control_snapshot();
                let lists = session.runner_mut().list_manager_snapshot();
                let text_edits = session.runner_mut().text_edit_snapshot().records;
                let standard_file = session.runner_mut().standard_file_snapshot();
                let list_text_plans = frame.as_ref().map(|frame|
                    qualify_list_text_fields(&lists, &frame.pixels, frame.width, frame.height)).unwrap_or_default();
                let styled_text_plans = frame.as_ref().map(|frame|
                    qualify_styled_text_fields(&text_edits, &frame.pixels, frame.width, frame.height))
                    .unwrap_or_default();
                let frame = frame.map(|frame| (frame.width, frame.height, gpui_pixels(frame.pixels)));
                *updates.lock().unwrap() = Some(Update {
                    identity: identity.clone(),
                    clipboard_export: clipboard.export.clone(),
                    menus,
                    menu_presented,
                    menu_height,
                    guest_menu_tracking,
                    guest_popup,
                    windows,
                    dialogs,
                    controls,
                    lists,
                    list_text_plans,
                    text_edits,
                    styled_text_plans,
                    standard_file,
                    frame,
                    status: format!(
                        "{architecture} · {}",
                        if running { "Running" } else { "Guest stopped" }
                    ),
                });
                if !running {
                    if let Some(store) = save_store.as_mut() {
                        store.sync_save_files_now(session.runner_mut());
                    }
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
        menu_presented: bool,
        menu_height: u16,
        menu_hovered: bool,
        open_menus: HashSet<String>,
        guest_menu_tracking: bool,
        guest_popup: Option<systemless::menu_model::GuestPopupSnapshot>,
        windows: Vec<WindowFrameSnapshot>,
        dialogs: Vec<DialogSnapshot>,
        controls: Vec<ControlSnapshot>,
        lists: Vec<ListManagerSnapshot>,
        list_text_plans: Vec<std::collections::BTreeMap<(i16, i16), super::text::ClassicListCellPaintPlan>>,
        text_edits: Vec<TextEditSnapshot>,
        styled_text_plans: Vec<Option<super::text::StyledTextEditPaintPlan>>,
        standard_file: Option<StandardFileSnapshot>,
        text_pointer_map: std::rc::Rc<std::cell::RefCell<Option<super::text::TextPointerMap>>>,
        text_pointer_capture: Option<(u32, u64)>,
        image: Option<Arc<RenderImage>>,
        prepared_buttons: Option<PreparedButtonImage>,
        logo: Arc<Image>,
        width: u32,
        height: u32,
        display_origin: (f32, f32),
        display_scale: f32,
        display_size: (f32, f32),
        status: String,
        focus: FocusHandle,
        mouse_down: bool,
        mouse_position: (i16, i16),
        scrollbar_drag: Option<(u32, u64, (i16, i16))>,
        popup_tracking: Option<(u32, u64)>,
        keyboard: super::input::KeyboardState,
        wheel: super::scroll::WheelAccumulator,
        _window_activation: Option<Subscription>,
        host_active: Option<bool>,
        host_generation: u64,
        host_clipboard: super::clipboard::HostClipboard,
        arrows_as_numpad: bool,
        application_identity: Option<Arc<game::ApplicationIdentity>>,
        _focus_out: Option<Subscription>,
        _focus_lost: Option<Subscription>,
        _poll: Task<()>,
    }

    impl Demo {
        fn map_arrow(&self, key: u8, character: u8) -> (u8, u8) {
            if self.arrows_as_numpad {
                match key {
                    0x7b => (0x56, b'4'),
                    0x7c => (0x58, b'6'),
                    0x7d => (0x54, b'2'),
                    0x7e => (0x5b, b'8'),
                    _ => (key, character),
                }
            } else {
                (key, character)
            }
        }

        fn read_host_clipboard(cx: &App) -> super::clipboard::HostSample {
            // GPUI does not expose the native pasteboard change count. Use it
            // to detect a new copy with identical text and unknown formats.
            let stamp = || {
                if cx.is_test() {
                    return None;
                }
                // SAFETY: invoked only on the macOS UI thread; these AppKit
                // accessors return retained/read-only pasteboard state.
                let board = unsafe { objc2_app_kit::NSPasteboard::generalPasteboard() };
                Some(unsafe {
                    (board.changeCount(), board.types().is_some_and(|types| types.count() != 0))
                })
            };
            for _ in 0..3 {
                let before = stamp();
                let mut sample = super::clipboard::HostSample {
                    text: None,
                    text_only: before.is_none_or(|(_, has_types)| !has_types),
                    revision: before.map(|(count, _)| count),
                };
                if let Some(item) = cx.read_from_clipboard() {
                    sample.text_only = true;
                    for entry in item.entries() {
                        match entry {
                            ClipboardEntry::String(value) => {
                                sample.text.get_or_insert_with(String::new).push_str(value.text());
                            }
                            _ => sample.text_only = false,
                        }
                    }
                }
                if stamp() == before {
                    return sample;
                }
            }
            super::clipboard::HostSample {
                text: None,
                text_only: false,
                revision: stamp().map(|(count, _)| count),
            }
        }

        fn import_host_clipboard(&mut self, cx: &App) {
            let sample = Self::read_host_clipboard(cx);
            if let Some(text) = self.host_clipboard.changed_sample(sample) {
                let _ = self.commands.send(Command::ImportClipboard(text));
            }
        }

        fn export_host_clipboard(&mut self, generation: u64, text: &[u8], cx: &App) {
            if self.host_active != Some(false) {
                return;
            }
            let sample = Self::read_host_clipboard(cx);
            if let Some(text) = self.host_clipboard.export(generation, text, sample) {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.host_clipboard.record_export(Self::read_host_clipboard(cx));
            }
        }

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
                            if let Some((generation, text)) = update.clipboard_export {
                                this.export_host_clipboard(generation, &text, cx);
                            }
                            let same_identity = match (&this.application_identity, &update.identity) {
                                (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                                (None, None) => true,
                                _ => false,
                            };
                            if !same_identity {
                                super::native_application::set_application_icon(update.identity.as_ref().and_then(|identity| identity.icon.as_ref()));
                                this.application_identity = update.identity;
                            }
                            this.menus = update.menus;
                            this.menu_presented = update.menu_presented;
                            this.menu_height = update.menu_height;
                            this.guest_menu_tracking = update.guest_menu_tracking;
                            this.guest_popup = update.guest_popup;
                            this.windows = update.windows;
                            this.dialogs = update.dialogs;
                            this.controls = update.controls;
                            this.lists = update.lists;
                            this.list_text_plans = update.list_text_plans;
                            this.text_edits = update.text_edits;
                            this.styled_text_plans = update.styled_text_plans;
                            this.standard_file = update.standard_file;
                            this.status = update.status;
                            if let Some((width, height, pixels)) = update.frame {
                                this.width = width;
                                this.height = height;
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
                menu_presented: true,
                menu_height: 20,
                menu_hovered: false,
                open_menus: HashSet::new(),
                guest_menu_tracking: false,
                guest_popup: None,
                windows: Vec::new(),
                dialogs: Vec::new(),
                controls: Vec::new(),
                lists: Vec::new(),
                list_text_plans: Vec::new(),
                text_edits: Vec::new(),
                styled_text_plans: Vec::new(),
                standard_file: None,
                image: None,
                prepared_buttons: None,
                logo: Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!("assets/systemless-logo.svg").to_vec(),
                )),
                width: 640,
                height: 460,
                display_origin: (0., 0.),
                display_scale: 1.,
                display_size: (640., 460.),
                status: "Loading guest…".into(),
                focus: cx.focus_handle(),
                text_pointer_map: Default::default(),
                text_pointer_capture: None,
                mouse_down: false,
                mouse_position: (0, 0),
                scrollbar_drag: None,
                popup_tracking: None,
                keyboard: super::input::KeyboardState::default(),
                wheel: super::scroll::WheelAccumulator::default(),
                _window_activation: None,
                host_active: None,
                host_generation: 0,
                host_clipboard: Default::default(),
                arrows_as_numpad: false,
                application_identity: None,
                _focus_out: None,
                _focus_lost: None,
                _poll: poll,
            }
        }

        fn pointer(&self, position: Point<Pixels>) -> (i16, i16) {
            // Convert the aspect-fit host position back to guest coordinates.
            let x = ((f32::from(position.x) - self.display_origin.0) / self.display_scale)
                .clamp(0., self.width.saturating_sub(1) as f32);
            let y = ((f32::from(position.y) - self.display_origin.1)
                / self.display_scale)
                .clamp(0., self.height.saturating_sub(1) as f32);
            (y as i16, x as i16)
        }

        fn text_pointer(&mut self, position: Point<Pixels>, begin: bool) -> (i16, i16) {
            let point = self.pointer(position);
            let map = self.text_pointer_map.borrow();
            let Some(map) = map.as_ref() else { return point; };
            let Some(panel) = self.standard_file.as_ref() else { return point; };
            let Some(folder) = panel.new_folder.as_ref().filter(|folder| folder.error.is_none()) else { return point; };
            if (panel.guest_id, panel.generation) != map.identity || folder.name != map.text {
                return point;
            }
            let r = map.guest_bounds;
            let inside = point.0 >= r.0 && point.0 < r.2 && point.1 >= r.1 && point.1 < r.3;
            if begin && inside { self.text_pointer_capture = Some(map.identity); }
            if self.text_pointer_capture != Some(map.identity) { return point; }
            (point.0, map.horizontal(f32::from(position.x)).unwrap_or(point.1))
        }

        fn inside_guest_pane(&self, position: Point<Pixels>) -> bool {
            let x = f32::from(position.x);
            let y = f32::from(position.y);
            x >= self.display_origin.0
                && x < self.display_origin.0 + self.display_size.0
                && y >= self.display_origin.1
                && y < self.display_origin.1 + self.display_size.1
        }

        fn guest_menu_fallback(&self) -> bool {
            self.menus.requires_guest_menu_rendering()
        }

        fn sync_caps_lock(&mut self, on: bool) {
            for input in self.keyboard.sync_caps_lock(on) {
                let _ = self.commands.send(Command::Input(input));
            }
        }

        fn sync_host_modifiers(&mut self, modifiers: Modifiers) {
            for input in self.keyboard.sync_host_modifiers(modifiers) {
                let _ = self.commands.send(Command::Input(input));
            }
        }

        fn press_host_key(&mut self, mac_key: u8, character: u8) {
            if let Some(input) = self.keyboard.press_host_key(mac_key, character) {
                let _ = self.commands.send(Command::Input(input));
            }
        }

        fn release_host_key(&mut self, mac_key: u8) {
            if let Some(input) = self.keyboard.release_host_key(mac_key) {
                let _ = self.commands.send(Command::Input(input));
            }
        }

        fn release_host_input(&mut self) {
            self.text_pointer_capture = None;
            self.wheel.reset();
            let _ = self.commands.send(Command::CancelWheel);
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
            for input in self.keyboard.release_all() {
                let _ = self.commands.send(Command::Input(input));
            }
        }

        fn scrollbar_at(&self, point: (i16, i16)) -> Option<(u32, u64, (i16, i16))> {
            let viewport = super::frames::Rect {
                top: 0,
                left: 0,
                bottom: self.height as i32,
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
                top: 0,
                left: 0,
                bottom: self.height as i32,
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
                        && frame.presentation_definition_id() == Some(1)
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
            if self._window_activation.is_none() {
                self.host_active = Some(window.is_window_active());
                if window.is_window_active() {
                    self.import_host_clipboard(cx);
                }
                if !window.is_window_active() {
                    self.host_clipboard.suspend(self.host_generation, Self::read_host_clipboard(cx));
                    let _ = self.commands.send(Command::Foreground(false, self.host_generation));
                }
                self._window_activation = Some(cx.observe_window_activation(window, |this, window, cx| {
                    let active = window.is_window_active();
                    if this.host_active.replace(active) != Some(active) {
                        this.host_generation = this.host_generation.wrapping_add(1);
                        if !active {
                            this.release_host_input();
                            this.host_clipboard.suspend(this.host_generation, Self::read_host_clipboard(cx));
                        } else {
                            this.host_clipboard.resume();
                            this.import_host_clipboard(cx);
                        }
                        let _ = this.commands.send(Command::Foreground(active, this.host_generation));
                        cx.notify();
                    }
                }));
            }
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
            let had_open_menu = !self.open_menus.is_empty();
            self.open_menus.retain(|identity| {
                self.menus.menus.iter().any(|menu| {
                    menu.visible_in_menu_bar
                        && *identity == format!("guest-menu-{}-{}", menu.guest_id, menu.generation)
                })
            });
            if had_open_menu && self.open_menus.is_empty() {
                self.focus.focus(window, cx);
            }
            let guest_menu_fallback = self.guest_menu_fallback();
            let fill_display = self.image.is_some();
            let (screen_width, screen_height) = if fill_display {
                let viewport = window.viewport_size();
                let available_width = f32::from(viewport.width);
                let available_height = f32::from(viewport.height);
                let scale = (available_width / self.width as f32)
                    .min(available_height / self.height as f32);
                self.display_scale = scale;
                self.display_size = (self.width as f32 * scale, self.height as f32 * scale);
                self.display_origin = (
                    (available_width - self.display_size.0) / 2.,
                    (available_height - self.display_size.1) / 2.,
                );
                self.display_size
            } else {
                self.display_scale = 1.;
                self.display_size = (self.width as f32, self.height as f32);
                self.display_origin = (0., 0.);
                self.display_size
            };
            // The guest reserves MBarHeight, not a fixed 20-pixel strip.
            // Inside Macintosh V, Menu Manager: menu-bar height and MBarHeight.
            let bar_height = if self.menu_presented {
                f32::from(self.menu_height.max(1)) * self.display_scale
            } else {
                36.
            };
            let mut bar = div()
                .id("guest-menu-bar")
                .test_support()
                .occlude()
                .flex()
                .items_center()
                .h(px(bar_height))
                .w_full()
                .when(self.menu_presented, |bar| bar.w(px(self.display_size.0)))
                .flex_shrink_0()
                .bg(cx.theme().background)
                .border_b_1()
                .border_color(cx.theme().border);
            bar = bar.child(
                div()
                    .ml(px(10.))
                    .mr(px(2.))
                    .w(px((bar_height - 4.).clamp(1., 20.)))
                    .h(px((bar_height - 4.).clamp(1., 20.)))
                    .child(img(self.logo.clone()).size_full()),
            );
            for menu in self.menus.menus.iter().filter(|m| m.visible_in_menu_bar) {
                let snapshot = self.menus.clone();
                let commands = self.commands.clone();
                let id = menu.id;
                let identity = format!("guest-menu-{}-{}", menu.guest_id, menu.generation);
                let demo = cx.entity().downgrade();
                let state = window.use_keyed_state(format!("live-{identity}"), cx, |_, _| {
                    LiveMenuState::default()
                });
                bar = bar.child(
                    Popover::new(format!("popover-{identity}"))
                        .appearance(false)
                        .overlay_closable(true)
                        .on_open_change({
                            let state = state.downgrade();
                            let identity = identity.clone();
                            move |open, window, cx| {
                                if let Some(demo) = demo.upgrade() {
                                    demo.update(cx, |demo, cx| {
                                        if *open {
                                            demo.open_menus.insert(identity.clone());
                                        } else {
                                            demo.open_menus.remove(&identity);
                                            if demo.open_menus.is_empty() {
                                                demo.focus.focus(window, cx);
                                            }
                                            demo.menu_hovered = f32::from(window.mouse_position().y) < 36.;
                                        }
                                        cx.notify();
                                    });
                                }
                                if !*open {
                                    _ = state.update(cx, |state, _| {
                                        state.menu = None;
                                        state.rendered = None;
                                    });
                                }
                            }
                        })
                        .trigger(
                            Button::new(identity)
                                .accessibility_label(menu.title.clone())
                                .child(super::text::classic_menu_label(&menu.title, 0,
                                    if self.menu_presented { self.display_scale } else { 1. },
                                    if menu.enabled { cx.theme().foreground } else { cx.theme().muted_foreground }))
                                .ghost()
                                .small()
                                .compact()
                                .h(px(bar_height))
                                .disabled(!menu.enabled),
                        )
                        .content(move |_, window, cx| {
                            let menu_tree = rendered_menu_tree(&snapshot, id);
                            if let Some(open_menu) = state.read(cx).menu.clone() {
                                if state.read(cx).rendered.as_ref() != Some(&menu_tree) {
                                    open_menu.update(cx, |popup, cx| {
                                        popup.update_snapshot(id, snapshot.clone(), cx);
                                    });
                                    state.update(cx, |state, _| {
                                        state.rendered = Some(menu_tree);
                                    });
                                }
                                return open_menu;
                            }
                            let open_menu = cx.new(|cx| {
                                GuestMenuPopup::new(id, snapshot.clone(), commands.clone(), cx)
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
                .test_support()
                .relative()
                .overflow_hidden()
                .w(px(screen_width))
                .h(px(screen_height))
                .flex_shrink_0()
                .when(fill_display, |screen| {
                    screen.ml(px(self.display_origin.0)).mt(px(self.display_origin.1))
                })
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, _| {
                    if event.touch_phase == TouchPhase::Cancelled {
                        this.wheel.reset();
                        let _ = this.commands.send(Command::CancelWheel);
                        return;
                    }
                    if event.touch_phase == TouchPhase::Started { this.wheel.reset(); }
                    if this.mouse_down || !this.open_menus.is_empty() || this.guest_menu_tracking {
                        this.wheel.reset();
                        return;
                    }
                    let origin = this.pointer(event.position);
                    let (x, y) = match event.delta {
                        ScrollDelta::Lines(delta) => (delta.x, delta.y),
                        ScrollDelta::Pixels(delta) => (f32::from(delta.x) / 40., f32::from(delta.y) / 40.),
                    };
                    let vertical = y.abs() >= x.abs();
                    let viewport = super::frames::Rect {
                        top: 0, left: 0, bottom: this.height as i32, right: this.width as i32,
                    };
                    let Some(target) = super::scroll::target(
                        &this.controls, &this.menus, &this.windows, viewport, origin, vertical,
                    ) else { this.wheel.reset(); return; };
                    let steps = this.wheel.push(target, -(if vertical { y } else { x }));
                    if steps != 0 {
                        let _ = this.commands.send(Command::Wheel(super::scroll::WheelRequest {
                            target, origin, steps,
                        }));
                    }
                }))
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    let (vertical, horizontal) = this.text_pointer(event.position, false);
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
                        let (vertical, horizontal) = this.text_pointer(event.position, true);
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
                        let (vertical, horizontal) = this.text_pointer(event.position, false);
                        this.text_pointer_capture = None;
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
                            let (vertical, horizontal) = this.text_pointer(event.position, false);
                            this.text_pointer_capture = None;
                            let _ = this.commands.send(Command::Input(MacintoshInput::MouseUp {
                                vertical,
                                horizontal,
                            }));
                        }
                    }),
                );
            if let Some(source) = self.image.clone() {
                let viewport = super::frames::Rect {
                    top: 0, left: 0, bottom: self.height as i32, right: self.width as i32,
                };
                let mut clips = Vec::new();
                if !self.guest_menu_fallback() || !self.guest_menu_tracking {
                    clips.extend(super::frames::dialog_item_pieces(&self.dialogs, &self.windows, viewport)
                        .into_iter().filter(|piece| self.dialogs[piece.dialog].items[piece.item].kind == DialogItemKind::Button)
                        .map(|piece| piece.clip));
                    clips.extend(super::frames::control_pieces(&self.controls, &self.menus, &self.windows, viewport)
                        .into_iter().filter(|piece| self.controls[piece.control].proc_id == 0)
                        .map(|piece| piece.clip));
                }
                if let Some(popup) = self.guest_popup.as_ref() {
                    clips.extend(super::popup::owned_rects(popup.bounds));
                }
                let color = cx.theme().background.to_rgb();
                let background = [color.b, color.g, color.r, color.a].map(|channel| (channel * 255.).round() as u8);
                let image = if clips.is_empty() {
                    if let Some(old) = self.prepared_buttons.take() { cx.drop_image(old.image, None); }
                    source
                } else {
                    let current = self.prepared_buttons.as_ref().is_some_and(|prepared| {
                        Arc::ptr_eq(&prepared.source, &source) && prepared.clips == clips
                            && prepared.background == background
                    });
                    if !current {
                        let mut pixels = source.as_bytes(0).expect("guest frame pixels").to_vec();
                        // Remove only pixels already owned by opaque GPUI button overlays
                        // before linear texture filtering can blend them beyond their edges.
                        super::frames::fill_texture_clips(&mut pixels, self.width, self.height, &clips, background);
                        let buffer = image::RgbaImage::from_raw(self.width, self.height, pixels).unwrap();
                        let prepared = PreparedButtonImage {
                            source, clips, background,
                            image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
                        };
                        if let Some(old) = self.prepared_buttons.replace(prepared) { cx.drop_image(old.image, None); }
                    }
                    self.prepared_buttons.as_ref().unwrap().image.clone()
                };
                screen = screen.child(img(image).absolute().top_0().left_0().size_full());
            }
            // A tracking custom MDEF may draw its dropdown over any window.
            // Keep those pixels, but present standard windows and dialogs
            // while the custom menu is closed.
            // Macintosh Toolbox Essentials (1992), pp. 3-3, 3-87.
            let scene_scale = self.display_scale;
            let guest_px = |value: f32| gpui_kit::px(value * scene_scale);
            if !self.guest_menu_fallback() || !self.guest_menu_tracking {
                let viewport = super::frames::Rect {
                    top: 0,
                    left: 0,
                    bottom: self.height as i32,
                    right: self.width as i32,
                };
                for piece in super::frames::frame_pieces(&self.windows, viewport) {
                    let frame = &self.windows[piece.window];
                    let source = piece.source;
                    let clip = piece.clip;
                    let mut strip = div()
                        .absolute()
                        .left(guest_px((source.left - clip.left) as f32))
                        .top(guest_px((source.top - clip.top) as f32))
                        .w(guest_px(source.width() as f32))
                        .h(guest_px(source.height() as f32))
                        .bg(cx.theme().border);
                    if piece.title {
                        let Some(title) = frame.title_layout(self.menu_height as i16) else { continue; };
                        let Some(bytes) = frame.window.title.chars()
                            .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
                            .collect::<Option<Vec<_>>>() else { continue; };
                        let glyphs = super::text::ClassicLine::plain(&bytes, 0, 12);
                        let title_clip = super::frames::Rect::from(title.clip);
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
                            // Paint the border without insetting the containing
                            // block: WDEF title coordinates start at the outer edge.
                            .child(div().absolute().top_0().left_0().size_full()
                                .border_1().border_color(cx.theme().border))
                            .child(div()
                                .absolute()
                                .left(guest_px((title_clip.left - source.left) as f32))
                                .top(guest_px((title_clip.top - source.top) as f32))
                                .w(guest_px(title_clip.width() as f32))
                                .h(guest_px(title_clip.height() as f32))
                                .overflow_hidden()
                                .child(div()
                                    .absolute()
                                    .left(guest_px((i32::from(title.horizontal) - title_clip.left) as f32))
                                    .top(guest_px((i32::from(title.baseline - title.ascent) - title_clip.top) as f32))
                                    .w(guest_px(f32::from(title.width.max(1))))
                                    .h(guest_px(f32::from(title.ascent + title.descent)))
                                    .child(super::text::classic_line(glyphs, title.ascent,
                                        title.ascent + title.descent, (0, 0), None,
                                        super::text::ClassicLineGeometry::Title, scene_scale, foreground, cx.theme().selection))));
                        // Keep controls over the standard WDEF hit cells. Input still
                        // reaches FindWindow/TrackGoAway/DragWindow in the guest.
                        // Inside Macintosh I, I-287--I-289.
                        if frame.close_box
                            && frame.window.active
                            && matches!(
                                frame.presentation_definition_id(),
                                Some(0 | 4 | 8 | 12 | 16)
                            )
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
                                    .left(guest_px((i32::from(frame.window.bounds.1) - source.left) as f32))
                                    .top_0()
                                    .w(guest_px(18.))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(close_pressed, |control| control.bg(cx.theme().accent))
                                    .child("×"),
                            );
                        }
                        if frame.window.active
                            && matches!(frame.presentation_definition_id(), Some(8 | 12))
                        {
                            strip = strip.child(
                                div()
                                    .absolute()
                                    .left(guest_px(
                                        (i32::from(frame.window.bounds.3) - 15 - source.left) as f32
                                    ))
                                    .top_0()
                                    .w(guest_px(15.))
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
                            .left(guest_px(clip.left as f32))
                            .top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32))
                            .h(guest_px(clip.height() as f32))
                            .child(strip),
                    );
                }
                let gutters =
                    super::frames::gutter_pieces(&self.windows, &self.controls, viewport);
                for piece in gutters {
                    let source = piece.source;
                    let clip = piece.clip;
                    let mut gutter = div()
                        .absolute()
                        .left(guest_px((source.left - clip.left) as f32))
                        .top(guest_px((source.top - clip.top) as f32))
                        .w(guest_px(source.width() as f32))
                        .h(guest_px(source.height() as f32))
                        .bg(cx.theme().border)
                        .border_color(cx.theme().border);
                    gutter = match piece.kind {
                        super::frames::GutterKind::Vertical
                        | super::frames::GutterKind::Horizontal => gutter,
                        super::frames::GutterKind::GrowBox => {
                            let mut corner = gutter
                                .bg(cx.theme().secondary)
                                .border_l_1()
                                .border_t_1();
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
                                            .left(guest_px(left))
                                            .top(guest_px(top))
                                            .w(guest_px(2.))
                                            .h(guest_px(2.))
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
                            .left(guest_px(clip.left as f32))
                            .top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32))
                            .h(guest_px(clip.height() as f32))
                            .child(gutter),
                    );
                }
                // Preserve standard list cell semantics, but paint only the
                // native-qualified guest recipe. Unknown/modified cells keep
                // guest pixels and input continues through LClick.
                for piece in super::frames::list_pieces(&self.lists, &self.controls, &self.windows, viewport) {
                    let list = &self.lists[piece.list];
                    let Some(cells) = list.text_cells.as_ref() else { continue; };
                    let Some(global) = list.global_view_rect else { continue; };
                    for (&cell, text) in cells {
                        let (row, column) = cell;
                        if row < list.visible.0 || row >= list.visible.2 || column < list.visible.1 || column >= list.visible.3 { continue; }
                        let top = piece.source.top + i32::from(row - list.visible.0) * i32::from(list.cell_size.0.max(1));
                        let left = piece.source.left + i32::from(column - list.visible.1) * i32::from(list.cell_size.1.max(1));
                        let bounds = super::frames::Rect { top, left,
                            bottom: top + i32::from(list.cell_size.0.max(1)), right: left + i32::from(list.cell_size.1.max(1)) };
                        let Some(clip) = bounds.intersection(piece.clip) else { continue; };
                        screen = screen.child(div().absolute()
                            .id(format!("guest-list-cell-{}-{}-{row}-{column}", list.guest_id, list.generation)).test_support()
                            .role(Role::ListItem).aria_label(text.clone()).aria_selected(list.selected.contains(&cell))
                            .left(guest_px(clip.left as f32)).top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32)).h(guest_px(clip.height() as f32)));
                        let Some(plan) = self.list_text_plans.get(piece.list).and_then(|plans| plans.get(&cell)) else { continue; };
                        let Some(paint) = list.standard_cell_paint.get(&cell) else { continue; };
                        let origin = (self.display_origin.0 + (i32::from(global.1) - i32::from(list.view_rect.1)) as f32 * scene_scale,
                            self.display_origin.1 + (i32::from(global.0) - i32::from(list.view_rect.0)) as f32 * scene_scale);
                        for painted in &paint.painted_regions {
                            let Some(clip) = clip.intersection(super::frames::Rect::from(*painted)) else { continue; };
                            let Some(ink) = super::text::classic_list_cell(plan.clone(), scene_scale, origin) else { continue; };
                            // Canvas glyphs and backgrounds snap global edges to
                            // device pixels. Match those edges for the scissor,
                            // including centered scenes and half-pixel ties.
                            let dpi = window.scale_factor();
                            let snap = |value: f32, origin: f32| {
                                let device = (origin + value * scene_scale) * dpi;
                                (device.abs() - 0.5).ceil().copysign(device) / dpi - origin
                            };
                            let left = snap(clip.left as f32, self.display_origin.0);
                            let right = snap(clip.right as f32, self.display_origin.0);
                            let top = snap(clip.top as f32, self.display_origin.1);
                            let bottom = snap(clip.bottom as f32, self.display_origin.1);
                            screen = screen.child(div().absolute().overflow_hidden()
                                .left(px(left)).top(px(top))
                                .w(px(right - left)).h(px(bottom - top)).child(ink));
                        }
                    }
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
                        .left(guest_px((source.left - clip.left) as f32))
                        .top(guest_px((source.top - clip.top) as f32))
                        .w(guest_px(source.width() as f32))
                        .h(guest_px(source.height() as f32))
                        .bg(cx.theme().background);
                    let caret_line = record.caret_line();
                    for (index, _line) in lines.into_iter().enumerate() {
                        // Reuse the owning CPU's line anchors. Styled records
                        // remain guest-owned until run ink/selection is qualified.
                        let Some(geometry) = record.line_geometry(index, 0) else { continue; };
                        let top = dest.top - i32::from(record.dest_rect.0) + i32::from(geometry.top) - source.top;
                        if top >= source.height() || top + i32::from(geometry.height) <= 0 {
                            continue;
                        }
                        let starts = record.line_starts.as_ref().unwrap();
                        let line_start = starts[index];
                        let line_end = starts[index + 1];
                        let mut visible_end = line_end;
                        while visible_end > line_start && matches!(record.text[visible_end - 1], b' ' | b'\r' | b'\n') {
                            visible_end -= 1;
                        }
                        let measured_end = if record.clips_line_offsets_to_visible_text { visible_end } else { line_end };
                        let selection = (
                            record.selection.0.saturating_sub(line_start).min(measured_end - line_start),
                            record.selection.1.saturating_sub(line_start).min(measured_end - line_start),
                        );
                        let mut glyphs = super::text::ClassicLine::plain(&record.text[line_start..visible_end], record.font, record.size);
                        // Measure canonical byte spans independently of visible
                        // ink; CR and trailing spaces remain guest offsets.
                        glyphs.positions = super::text::ClassicLine::plain(&record.text[line_start..line_end], record.font, record.size).positions;
                        let caret = caret_line.filter(|&(line, _)| line == index).map(|(_, offset)| offset);
                        overlay = overlay.child(
                            div()
                                .id(format!("guest-text-edit-line-{}-{}-{index}", record.guest_id, record.generation))
                                .absolute()
                                .left(guest_px((dest.left - i32::from(record.dest_rect.1) + i32::from(geometry.left) - 1 - source.left) as f32))
                                .top(guest_px(top as f32))
                                .w(guest_px(dest.width().max(1) as f32))
                                .h(guest_px(f32::from(geometry.height)))
                                .overflow_hidden()
                                .child(super::text::classic_line(
                                    glyphs, geometry.ascent, geometry.height,
                                    if record.active { selection } else { (0, 0) }, caret,
                                    super::text::ClassicLineGeometry::TextEdit, self.display_scale, cx.theme().foreground, cx.theme().selection,
                                )),
                        );
                    }
                    screen = screen.child(
                        div()
                            .absolute()
                            .overflow_hidden()
                            .left(guest_px(clip.left as f32))
                            .top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32))
                            .h(guest_px(clip.height() as f32))
                            .child(overlay),
                    );
                }
                // Styled fields use the owning CPU's strikes and paint order.
                // Visibility establishes the standard owner; exact native pixels
                // establish paint fidelity. Input continues through guest events.
                for piece in super::frames::styled_text_edit_candidates(
                    &self.text_edits, &self.dialogs, &self.controls, &self.windows, viewport,
                ) {
                    let Some(Some(plan)) = self.styled_text_plans.get(piece.record) else { continue; };
                    let record = &self.text_edits[piece.record];
                    let Some(dest) = record.global_dest_rect else { continue; };
                    let origin = (
                        self.display_origin.0 + (i32::from(dest.1) - i32::from(record.dest_rect.1)) as f32 * scene_scale,
                        self.display_origin.1 + (i32::from(dest.0) - i32::from(record.dest_rect.0)) as f32 * scene_scale,
                    );
                    let Some(ink) = super::text::classic_styled_text_edit_field(
                        plan.clone(), scene_scale, origin,
                    ) else { continue; };
                    let clip = piece.clip;
                    screen = screen.child(div().absolute().overflow_hidden()
                        .left(guest_px(clip.left as f32)).top(guest_px(clip.top as f32))
                        .w(guest_px(clip.width() as f32)).h(guest_px(clip.height() as f32))
                        .child(ink));
                }
                // CDEF-owned standard controls can use Kit components while their
                // ControlRecord state and tracking remain guest-owned.
                // Macintosh Toolbox Essentials (1992), pp. 5-58--5-64.
                for piece in super::frames::control_pieces(&self.controls, &self.menus, &self.windows, viewport) {
                    let control = &self.controls[piece.control];
                    let semantic_enabled = self.standard_file.is_none() && control.enabled && self.windows.iter().any(|frame| {
                        frame.guest_id == control.owner_id && frame.window.visible && frame.window.active
                    });
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
                        .left(guest_px((source.left - clip.left) as f32))
                        .top(guest_px((source.top - clip.top) as f32))
                        .w(guest_px(source.width() as f32))
                        .h(guest_px(source.height() as f32))
                        .bg(cx.theme().background);
                    match control.proc_id {
                        proc_id if (1008..=1023).contains(&proc_id) => {
                            let Some(selected) = super::frames::popup_control_label(control, &self.menus) else {
                                continue;
                            };
                            let popup_ink = control.popup_ink.unwrap_or(systemless::runner::ControlTextInk::Solid([0; 3]));
                            let popup_font = control.popup_font.unwrap_or_default();
                            let Some((box_top, box_left, box_bottom, box_right)) = control.popup_box_bounds else {
                                continue;
                            };
                            let title_width = i32::from(box_left) - source.left;
                            let box_y = i32::from(box_top) - source.top;
                            let box_width = i32::from(box_right) - i32::from(box_left);
                            let box_height = i32::from(box_bottom) - i32::from(box_top);
                            // Keep text in the guest ControlRecord coordinate system.
                            // Host borders must not inset the CDEF text canvas.
                            overlay = overlay
                                .child(
                                    div().absolute().left(guest_px(0.)).top(guest_px(0.))
                                        .w(guest_px(title_width as f32)).h_full().overflow_hidden()
                                        .child(super::text::classic_popup_control_label(
                                            &control.title, popup_font, true, control.popup_text_inset, scene_scale,
                                            popup_ink, (source.top, source.left, source.bottom, i32::from(box_left)), self.display_origin)),
                                )
                                .child(
                                    div().absolute().left(guest_px(title_width as f32)).top(guest_px(box_y as f32))
                                        .w(guest_px(box_width as f32)).h(guest_px(box_height as f32))
                                        .border_1().border_color(cx.theme().border).bg(cx.theme().secondary),
                                )
                                .child(
                                    div().absolute().left(guest_px(title_width as f32)).top(guest_px(box_y as f32))
                                        .w(guest_px((box_width - 19).max(0) as f32))
                                        .h(guest_px(box_height as f32)).overflow_hidden()
                                        .child(super::text::classic_popup_control_label(
                                            selected, popup_font, false, control.popup_text_inset, scene_scale,
                                            popup_ink, (i32::from(box_top), i32::from(box_left), i32::from(box_bottom), i32::from(box_right) - 19), self.display_origin)),
                                )
                                .child(
                                    div().absolute().left(guest_px((title_width + box_width - 18) as f32)).top(guest_px(box_y as f32))
                                        .w(guest_px(18.)).h(guest_px(box_height as f32)).flex().items_center().justify_center()
                                        .child(if let Some(indicator) = control.popup_indicator.clone() {
                                            super::text::classic_popup_indicator(indicator, scene_scale, self.display_origin).into_any_element()
                                        } else {
                                            div().child("▾").into_any_element()
                                        }),
                                );
                        }
                        0 => {
                            overlay = overlay.child(
                                super::a11y::AccessibleComponent::new(super::choices::guest_button(
                                    format!("guest-control-button-{}-{}", control.guest_id, control.generation),
                                    control.title.clone(), control.enabled, semantic_enabled,
                                    semantic_enabled && control.hilite == 10, false, scene_scale, cx,
                                )
                                    .w_full()
                                    .h_full()
                                    .on_click({
                                        let sender = self.commands.clone();
                                        let (id, generation) = (control.guest_id, control.generation);
                                        move |event, _, _| {
                                            if matches!(event, ClickEvent::Keyboard(_)) {
                                                let _ = sender.send(Command::ActivateControl(id, generation));
                                            }
                                        }
                                    })
                                    .when(semantic_enabled, |button| {
                                        let sender = self.commands.clone();
                                        let (id, generation) = (control.guest_id, control.generation);
                                        button.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                            let _ = sender.send(Command::ActivateControl(id, generation));
                                        })
                                    }), !semantic_enabled),
                            );
                        }
                        1 => {
                            overlay = overlay.child(
                                super::a11y::AccessibleComponent::new(super::choices::guest_checkbox(
                                    format!("guest-control-checkbox-{}-{}", control.guest_id, control.generation),
                                    control.title.clone(), control.value != 0, control.enabled,
                                    control.hilite == 11, scene_scale, cx,
                                ).disabled(!semantic_enabled).on_change({
                                    let sender = self.commands.clone();
                                    let (id, generation) = (control.guest_id, control.generation);
                                    move |_, event, _, _| {
                                        if matches!(event, ClickEvent::Keyboard(_)) {
                                            let _ = sender.send(Command::ActivateControl(id, generation));
                                        }
                                    }
                                }).when(semantic_enabled, |choice| {
                                    let sender = self.commands.clone();
                                    let (id, generation) = (control.guest_id, control.generation);
                                    choice.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                        let _ = sender.send(Command::ActivateControl(id, generation));
                                    })
                                }), !semantic_enabled),
                            );
                        }
                        2 => {
                            overlay = overlay.child(
                                super::a11y::AccessibleComponent::new(super::choices::guest_radio(
                                    format!("guest-control-radio-{}-{}", control.guest_id, control.generation),
                                    control.title.clone(), control.value != 0, control.enabled,
                                    control.hilite == 11, scene_scale, cx,
                                ).disabled(!semantic_enabled).on_change({
                                    let sender = self.commands.clone();
                                    let (id, generation) = (control.guest_id, control.generation);
                                    move |_, event, _, _| {
                                        if matches!(event, ClickEvent::Keyboard(_)) {
                                            let _ = sender.send(Command::ActivateControl(id, generation));
                                        }
                                    }
                                }).when(semantic_enabled, |choice| {
                                    let sender = self.commands.clone();
                                    let (id, generation) = (control.guest_id, control.generation);
                                    choice.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                        let _ = sender.send(Command::ActivateControl(id, generation));
                                    })
                                }), !semantic_enabled),
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
                                // Paint the frame without insetting the guest-coordinate children.
                                .child(div().absolute().top_0().left_0().w_full().h_full()
                                    .border_1().border_color(cx.theme().border))
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .w(guest_px(arrow_width))
                                        .h(guest_px(arrow_height))
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
                                        .left(guest_px(end_left))
                                        .top(guest_px(end_top))
                                        .w(guest_px(arrow_width))
                                        .h(guest_px(arrow_height))
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
                                        .left(guest_px(thumb_left))
                                        .top(guest_px(thumb_top))
                                        .w(guest_px(thumb_width))
                                        .h(guest_px(thumb_height))
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
                                                    .left(guest_px(left))
                                                    .top(guest_px(top))
                                                    .w(guest_px(thumb_width))
                                                    .h(guest_px(thumb_height))
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
                            .left(guest_px(clip.left as f32))
                            .top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32))
                            .h(guest_px(clip.height() as f32))
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
                    let semantic_active = dialog.active && self.standard_file.is_none();
                    let item = &dialog.items[piece.item];
                    let item_rect = super::frames::Rect::from(item.bounds);
                    let source = piece.source;
                    let clip = piece.clip;
                    let mut overlay = div()
                        .absolute()
                        .left(guest_px((source.left - clip.left) as f32))
                        .top(guest_px((source.top - clip.top) as f32))
                        .w(guest_px(source.width() as f32))
                        .h(guest_px(source.height() as f32))
                        .bg(cx.theme().background);
                    overlay = match item.kind {
                        DialogItemKind::Button => overlay.child(
                            super::a11y::AccessibleComponent::new(super::choices::guest_button(
                                format!("guest-dialog-button-{}-{}-{}", dialog.guest_id, dialog.generation, item.number),
                                item.text.clone(), item.enabled, semantic_active,
                                semantic_active && item.enabled && item.pressed,
                                semantic_active && item.enabled && dialog.default_item == Some(item.number), scene_scale, cx,
                            )
                            .absolute()
                            .left(guest_px((item_rect.left - source.left) as f32))
                            .top(guest_px((item_rect.top - source.top) as f32))
                            .w(guest_px(item_rect.width() as f32))
                            .h(guest_px(item_rect.height() as f32))
                            .on_click({
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                move |event, _, _| {
                                    if matches!(event, ClickEvent::Keyboard(_)) {
                                        let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                    }
                                }
                            })
                            .when(item.enabled && semantic_active, |button| {
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                button.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                    let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                })
                            }), !item.enabled || !semantic_active),
                        ),
                        DialogItemKind::StaticText => overlay.child(super::text::classic_dialog_static_text(
                            &item.text, item.static_text_layout.as_ref().unwrap(), scene_scale, cx.theme().foreground,
                        )),
                        DialogItemKind::EditText => {
                            let focused = semantic_active
                                && dialog.edit_field == Some(item.number)
                                && item.enabled
                                && item.selection.is_some();
                            let selection = item.selection.unwrap_or((0, 0));
                            let foreground = if item.enabled { cx.theme().foreground } else { cx.theme().muted_foreground };
                            let field = div()
                                .id(format!("guest-dialog-edit-{}-{}-{}", dialog.guest_id, dialog.generation, item.number))
                                .test_support()
                                .absolute()
                                .left(guest_px((item_rect.left - source.left - 3) as f32))
                                .top(guest_px((item_rect.top - source.top - 3) as f32))
                                .w(guest_px((item_rect.width() + 6) as f32))
                                .h(guest_px((item_rect.height() + 6) as f32))
                                .border(guest_px(1.))
                                .border_color(if focused { cx.theme().accent } else { cx.theme().border })
                                .bg(cx.theme().background)
                                .child(div().absolute().left(guest_px(3.)).top(guest_px(3.))
                                    .w(guest_px(item_rect.width() as f32)).h(guest_px(item_rect.height() as f32))
                                    .overflow_hidden()
                                    .child(super::text::classic_dialog_edit_text(
                                        &item.text, item.edit_text_layout.as_ref().unwrap(),
                                        (selection.0.max(0) as usize, selection.1.max(0) as usize),
                                        focused, item.caret_visible == Some(true), scene_scale,
                                        foreground, cx.theme().selection)));
                            overlay.child(field)
                        }
                        DialogItemKind::Checkbox => overlay.child(
                            super::a11y::AccessibleComponent::new(super::choices::guest_checkbox(
                                format!("guest-dialog-checkbox-{}-{}-{}", dialog.guest_id, dialog.generation, item.number),
                                item.text.clone(), item.value.unwrap() != 0, item.enabled,
                                semantic_active && item.pressed, scene_scale, cx,
                            ).disabled(!item.enabled || !semantic_active).on_change({
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                move |_, event, _, _| {
                                    if matches!(event, ClickEvent::Keyboard(_)) {
                                        let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                    }
                                }
                            }).when(semantic_active && item.enabled, |choice| {
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                choice.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                    let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                })
                            }), !item.enabled || !semantic_active),
                        ),
                        DialogItemKind::RadioButton => overlay.child(
                            super::a11y::AccessibleComponent::new(super::choices::guest_radio(
                                format!("guest-dialog-radio-{}-{}-{}", dialog.guest_id, dialog.generation, item.number),
                                item.text.clone(), item.value.unwrap() != 0, item.enabled,
                                semantic_active && item.pressed, scene_scale, cx,
                            ).disabled(!item.enabled || !semantic_active).on_change({
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                move |_, event, _, _| {
                                    if matches!(event, ClickEvent::Keyboard(_)) {
                                        let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                    }
                                }
                            }).when(semantic_active && item.enabled, |choice| {
                                let sender = self.commands.clone();
                                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                                let identity = item.control_identity;
                                choice.on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                    let _ = sender.send(Command::ActivateDialog(id, generation, number, identity));
                                })
                            }), !item.enabled || !semantic_active),
                        ),
                        _ => unreachable!(),
                    };
                    screen = screen.child(
                        div()
                            .absolute()
                            .overflow_hidden()
                            .left(guest_px(clip.left as f32))
                            .top(guest_px(clip.top as f32))
                            .w(guest_px(clip.width() as f32))
                            .h(guest_px(clip.height() as f32))
                            .child(overlay),
                    );
                }
                if let Some(panel) = self.standard_file.as_ref().filter(|panel| {
                    panel.kind == StandardFileKind::Get
                        && panel.standard_entry_point
                        && panel.get_layout.is_some()
                        && panel.entries.is_some()
                        && panel.directory_font.2 == 0
                        && systemless::quickdraw::fonts::get_font_face_or_default(panel.directory_font.0, panel.directory_font.1).size
                            == if panel.directory_font.1 == 0 { 12 } else { panel.directory_font.1 }
                }) {
                    let layout = panel.get_layout.as_ref().unwrap();
                    let bounds = super::frames::Rect::from(panel.bounds);
                    if bounds.intersection(viewport).is_some() {
                        let at = |rect: (i16, i16, i16, i16)| {
                            let rect = super::frames::Rect::from(rect);
                            div()
                                .absolute()
                                .top(guest_px((rect.top - bounds.top) as f32))
                                .left(guest_px((rect.left - bounds.left) as f32))
                                .w(guest_px(rect.width() as f32))
                                .h(guest_px(rect.height() as f32))
                        };
                        let mut overlay = div()
                            .id(format!("guest-standard-open-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .absolute()
                            .top(guest_px(bounds.top as f32))
                            .left(guest_px(bounds.left as f32))
                            .w(guest_px(bounds.width() as f32))
                            .h(guest_px(bounds.height() as f32))
                            .bg(cx.theme().background)
                            .border_2()
                            .border_color(cx.theme().border)
                            .text_color(cx.theme().foreground)
                            .text_size(guest_px(13.));
                        if let Some((text, origin)) = &panel.volume_text {
                            overlay = overlay.child(
                                at(layout.volume)
                                    .overflow_hidden()
                                    .bg(cx.theme().secondary)
                                    .child(div().absolute().size_full().border_1().border_color(cx.theme().border))
                                    .child(super::text::classic_file_row(text, *origin, scene_scale, cx.theme().foreground)),
                            );
                        }
                        overlay = overlay.child(
                            at(layout.directory_label)
                                .overflow_hidden()
                                .child(super::text::classic_directory_label(
                                    panel.directory_label.as_deref().unwrap_or_default(),
                                    panel.directory_font, panel.directory_text_layout,
                                    scene_scale, cx.theme().foreground,
                                )),
                        );
                        let entries = panel.entries.as_ref().unwrap();
                        let list_width = i32::from(layout.list.3 - layout.list.1);
                        let mut list = at(layout.list)
                            .id(format!("guest-standard-open-list-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .role(Role::ListBox)
                            .aria_label("Files")
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .child(div().absolute().size_full().border_1().border_color(cx.theme().border));
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
                                    .role(Role::ListBoxOption)
                                    .aria_label(entry.name.clone())
                                    .aria_selected(selected)
                                    .absolute()
                                    .top(guest_px(2. + row as f32 * f32::from(layout.row_height)))
                                    .left(guest_px(2.))
                                    .w(guest_px((list_width - 4).max(1) as f32))
                                    .h(guest_px(f32::from(layout.row_height)))
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .bg(if selected {
                                        cx.theme().accent
                                    } else {
                                        cx.theme().background
                                    })
                                    .child(super::text::classic_file_row(&if entry.is_directory {
                                        format!("{} {}", super::text::file_row_name(&entry.name, panel.list_name_limit), panel.directory_marker)
                                    } else { super::text::file_row_name(&entry.name, panel.list_name_limit) }, panel.list_text_origin, scene_scale,
                                    cx.theme().foreground)),
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
                                        .h(guest_px(16.))
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
                                        .h(guest_px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▾"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(guest_px(thumb_top as f32))
                                        .w_full()
                                        .h(guest_px(thumb_height as f32))
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
                                    super::a11y::AccessibleComponent::new(super::choices::guest_button(
                                        format!("guest-standard-open-{}-{}-{}", panel.guest_id, panel.generation, label),
                                        label.into(), enabled, true, false, false, scene_scale, cx,
                                    ).w_full().h_full()
                                    .when(label != "Eject", |button| {
                                        let action = match label {
                                            "Cancel" => super::activation::FileAction::Cancel,
                                            "Desktop" => super::activation::FileAction::Desktop,
                                            _ => super::activation::FileAction::Accept,
                                        };
                                        let (id, generation) = (panel.guest_id, panel.generation);
                                        let keyboard_sender = self.commands.clone();
                                        let accessibility_sender = self.commands.clone();
                                        button.on_click(move |event, _, _| {
                                            if matches!(event, ClickEvent::Keyboard(_)) {
                                                let _ = keyboard_sender.send(Command::ActivateFile(id, generation, action));
                                            }
                                        }).on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                            let _ = accessibility_sender.send(Command::ActivateFile(id, generation, action));
                                        })
                                    }), !enabled),
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
                        && panel.directory_font.2 == 0
                        && systemless::quickdraw::fonts::get_font_face_or_default(panel.directory_font.0, panel.directory_font.1).size
                            == if panel.directory_font.1 == 0 { 12 } else { panel.directory_font.1 }
                        && panel.name_text_layout.as_ref().is_some_and(|layout| {
                            layout.font.2 == 0
                                && systemless::quickdraw::fonts::get_font_face_or_default(layout.font.0, layout.font.1).size
                                    == if layout.font.1 == 0 { 12 } else { layout.font.1 }
                        })
                        && panel.name.is_some()
                }) {
                    let layout = panel.put_layout.as_ref().unwrap();
                    let bounds = super::frames::Rect::from(panel.bounds);
                    if bounds.intersection(viewport).is_some() {
                        let at = |rect: (i16, i16, i16, i16)| {
                            let rect = super::frames::Rect::from(rect);
                            div()
                                .absolute()
                                .top(guest_px((rect.top - bounds.top) as f32))
                                .left(guest_px((rect.left - bounds.left) as f32))
                                .w(guest_px(rect.width() as f32))
                                .h(guest_px(rect.height() as f32))
                        };
                        let mut overlay = div()
                            .id(format!("guest-standard-save-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .role(gpui_kit::Role::Group).aria_label("Save file")
                            .absolute()
                            .top(guest_px(bounds.top as f32))
                            .left(guest_px(bounds.left as f32))
                            .w(guest_px(bounds.width() as f32))
                            .h(guest_px(bounds.height() as f32))
                            .bg(cx.theme().background)
                            .border_2()
                            .border_color(cx.theme().border)
                            .text_color(cx.theme().foreground)
                            .text_size(guest_px(13.));
                        overlay = overlay.child(
                            at(layout.directory_label)
                                .overflow_hidden()
                                .child(super::text::classic_directory_label(
                                    panel.directory_label.as_deref().unwrap_or_default(),
                                    panel.directory_font, panel.directory_text_layout,
                                    scene_scale, cx.theme().foreground,
                                )),
                        );
                        let entries = panel.entries.as_ref().unwrap();
                        let list_width = i32::from(layout.list.3 - layout.list.1);
                        let mut list = at(layout.list)
                            .id(format!("guest-standard-save-list-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .role(Role::ListBox)
                            .aria_label("Files")
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .child(div().absolute().size_full().border_1().border_color(cx.theme().border));
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
                                    .role(Role::ListBoxOption)
                                    .aria_label(entry.name.clone())
                                    .aria_selected(selected)
                                    .absolute()
                                    .top(guest_px(2. + row as f32 * f32::from(layout.row_height)))
                                    .left(guest_px(2.))
                                    .w(guest_px((list_width - 4).max(1) as f32))
                                    .h(guest_px(f32::from(layout.row_height)))
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .bg(if selected {
                                        cx.theme().accent
                                    } else {
                                        cx.theme().background
                                    })
                                    .child(super::text::classic_file_row(&if entry.is_directory {
                                        format!("{} {}", super::text::file_row_name(&entry.name, panel.list_name_limit), panel.directory_marker)
                                    } else { super::text::file_row_name(&entry.name, panel.list_name_limit) }, panel.list_text_origin, scene_scale,
                                    cx.theme().foreground)),
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
                                        .h(guest_px(16.))
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
                                        .h(guest_px(16.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child("▾"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(guest_px(thumb_top as f32))
                                        .w_full()
                                        .h(guest_px(thumb_height as f32))
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
                                .child(super::text::classic_file_prompt(panel.prompt.as_deref().unwrap_or_default(), scene_scale, cx.theme().foreground)),
                        );
                        let name = panel.name.as_deref().unwrap_or_default();
                        let focused = panel.name_has_focus == Some(true);
                        let name_field = at(layout.name)
                            .id(format!("guest-standard-save-name-{}-{}", panel.guest_id, panel.generation))
                            .test_support()
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .child(div().absolute().size_full().border_1().border_color(if focused {
                                cx.theme().accent
                            } else { cx.theme().border }))
                            .child(super::text::classic_save_name(
                                name, panel.name_selection.unwrap_or((0, 0)), focused, panel.name_caret_visible == Some(true),
                                panel.name_text_layout.as_ref().unwrap(), scene_scale,
                                cx.theme().foreground, cx.theme().selection,
                            ));
                        overlay = overlay.child(name_field);
                        for (label, rect) in [
                            ("Desktop", layout.desktop),
                            ("New", layout.new_folder),
                            ("Cancel", layout.cancel),
                            ("Save", layout.save),
                        ] {
                            overlay = overlay.child(
                                at(rect).child(
                                    super::a11y::AccessibleComponent::new(super::choices::guest_button(
                                        format!("guest-standard-save-{}-{}-{}", panel.guest_id, panel.generation, label),
                                        label.into(), true, !(panel.confirming_replace || panel.new_folder.is_some()), false, false, scene_scale, cx,
                                    ).w_full().h_full()
                                    .when(!(panel.confirming_replace || panel.new_folder.is_some()), |button| {
                                        let action = match label {
                                            "Cancel" => super::activation::FileAction::Cancel,
                                            "Desktop" => super::activation::FileAction::Desktop,
                                            "New" => super::activation::FileAction::NewFolder,
                                            _ => super::activation::FileAction::Accept,
                                        };
                                        let (id, generation) = (panel.guest_id, panel.generation);
                                        let keyboard_sender = self.commands.clone();
                                        let accessibility_sender = self.commands.clone();
                                        button.on_click(move |event, _, _| {
                                            if matches!(event, ClickEvent::Keyboard(_)) {
                                                let _ = keyboard_sender.send(Command::ActivateFile(id, generation, action));
                                            }
                                        }).on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                                            let _ = accessibility_sender.send(Command::ActivateFile(id, generation, action));
                                        })
                                    }), panel.confirming_replace || panel.new_folder.is_some()),
                                ),
                            );
                        }
                        screen = screen.child(super::a11y::AccessibleState::new(overlay, false).hidden(panel.confirming_replace || panel.new_folder.is_some()));
                    }
                }
                if let Some((panel, folder)) = self.standard_file.as_ref()
                    .filter(|panel| panel.standard_entry_point)
                    .and_then(|panel| panel.new_folder.as_ref().map(|folder| (panel, folder)))
                {
                    let layout = &folder.layout;
                    let bounds = super::frames::Rect::from(layout.bounds);
                    let at = |rect: (i16, i16, i16, i16)| {
                        let rect = super::frames::Rect::from(rect);
                        div().absolute().top(guest_px((rect.top - bounds.top) as f32))
                            .left(guest_px((rect.left - bounds.left) as f32))
                            .w(guest_px(rect.width() as f32)).h(guest_px(rect.height() as f32))
                    };
                    let prompt = folder.prompt();
                    let is_error = folder.error.is_some();
                    let mut overlay = div().id("guest-standard-new-folder").test_support()
                        .role(if is_error { gpui_kit::Role::AlertDialog } else { gpui_kit::Role::Dialog }).aria_label(if is_error { "Folder creation error" } else { "New Folder" })
                        .absolute().top(guest_px(bounds.top as f32)).left(guest_px(bounds.left as f32))
                        .w(guest_px(bounds.width() as f32)).h(guest_px(bounds.height() as f32))
                        .bg(cx.theme().background).border_2().border_color(cx.theme().border)
                        .text_color(cx.theme().foreground).text_size(guest_px(13.))
                        .child(at(if is_error { layout.error_message() } else { layout.prompt }).overflow_hidden().child(super::text::classic_file_prompt(prompt, scene_scale, cx.theme().foreground)));
                    if !is_error {
                        let name = at(folder.layout.name).id("guest-standard-new-folder-name").test_support()
                            .aria_label("Name of new folder").overflow_hidden().flex().items_center().px_1()
                            .border_1().border_color(cx.theme().accent).bg(cx.theme().background)
                            .child(super::text::single_line(folder, (panel.guest_id, panel.generation),
                                self.text_pointer_map.clone(), scene_scale, cx.theme().foreground, cx.theme().selection));
                        overlay = overlay.child(name);
                    }
                    let actions = if is_error {
                        vec![(layout.create, "OK", super::activation::FileAction::DismissFolderError)]
                    } else { vec![
                        (layout.cancel, "Cancel", super::activation::FileAction::CancelNewFolder),
                        (layout.create, "Create", super::activation::FileAction::CreateFolder),
                    ] };
                    for (rect, label, action) in actions {
                        let enabled = is_error || label == "Cancel" || !folder.name.is_empty();
                        let (id, generation) = (panel.guest_id, panel.generation);
                        let keyboard_sender = self.commands.clone();
                        let accessibility_sender = self.commands.clone();
                        overlay = overlay.child(at(rect).child(super::a11y::AccessibleComponent::new(super::choices::guest_button(
                            format!("guest-standard-new-folder-{id}-{generation}-{label}"), label.into(),
                            true, enabled, false, label == "Create" || label == "OK", scene_scale, cx,
                        ).w_full().h_full().when(enabled, |button| button.on_click(move |event, _, _| {
                            if matches!(event, ClickEvent::Keyboard(_)) {
                                let _ = keyboard_sender.send(Command::ActivateFile(id, generation, action));
                            }
                        }).on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                            let _ = accessibility_sender.send(Command::ActivateFile(id, generation, action));
                        })), !enabled)));
                    }
                    screen = screen.child(overlay);
                }
                if let Some(panel) = self.standard_file.as_ref().filter(|p| p.standard_entry_point && p.confirming_replace) {
                    let layout = systemless::runner::StandardFileReplacementLayout::new(panel.bounds);
                    let bounds = super::frames::Rect::from(layout.bounds);
                    let at = |rect: (i16, i16, i16, i16)| {
                        let rect = super::frames::Rect::from(rect);
                        div().absolute().top(guest_px((rect.top - bounds.top) as f32))
                            .left(guest_px((rect.left - bounds.left) as f32))
                            .w(guest_px(rect.width() as f32)).h(guest_px(rect.height() as f32))
                    };
                    let mut overlay = div().id("guest-standard-replace").test_support()
                        .role(gpui_kit::Role::AlertDialog).aria_label("Replace existing file")
                        .absolute().top(guest_px(bounds.top as f32)).left(guest_px(bounds.left as f32))
                        .w(guest_px(bounds.width() as f32)).h(guest_px(bounds.height() as f32))
                        .bg(cx.theme().background).border_2().border_color(cx.theme().border)
                        .text_color(cx.theme().foreground).text_size(guest_px(13.))
                        .child(at(layout.message).overflow_hidden().child(super::text::classic_file_prompt(
                            &format!("Replace existing \"{}\"?", panel.name.as_deref().unwrap_or("")),
                            scene_scale, cx.theme().foreground,
                        )));
                    for (rect, label, action) in [
                        (layout.cancel, "Cancel", super::activation::FileAction::CancelReplacement),
                        (layout.replace, "Replace", super::activation::FileAction::Replace),
                    ] {
                        let (id, generation) = (panel.guest_id, panel.generation);
                        let keyboard_sender = self.commands.clone();
                        let accessibility_sender = self.commands.clone();
                        overlay = overlay.child(at(rect).child(super::choices::guest_button(
                            format!("guest-standard-replace-{id}-{generation}-{label}"), label.into(),
                            true, true, false, label == "Cancel", scene_scale, cx,
                        ).w_full().h_full().on_click(move |event, _, _| {
                            if matches!(event, ClickEvent::Keyboard(_)) {
                                let _ = keyboard_sender.send(Command::ActivateFile(id, generation, action));
                            }
                        }).on_a11y_action(gpui_kit::accesskit::Action::Click, move |_, _, _| {
                            let _ = accessibility_sender.send(Command::ActivateFile(id, generation, action));
                        })));
                    }
                    screen = screen.child(overlay);
                }
            }
            let mut bar = Some(bar);
            if let Some(popup) = self.guest_popup.as_ref() {
                // A screen-height guest popup may cover the menu bar. Keep it
                // above that bar without moving it out of the scene input layer.
                if !guest_menu_fallback && self.menu_presented {
                    screen = screen.child(super::metrics::SceneMetrics::new(
                        bar.take().unwrap().absolute().top_0().left_0(), window.rem_size(),
                    ));
                }
                screen = screen.child(super::popup::popup(popup, scene_scale, cx));
            }
            let menu_hovered = (self.menu_hovered || !self.open_menus.is_empty())
                && !self.menu_presented;
            div()
                .flex()
                .flex_col()
                .size_full()
                .relative()
                .bg(rgb(0x000000))
                .text_color(cx.theme().foreground)
                .track_focus(&self.focus)
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    let hover = !this.menu_presented
                        && !this.guest_menu_fallback()
                        && this.menus.menus.iter().any(|menu| menu.visible_in_menu_bar)
                        && f32::from(event.position.y) < 36.;
                    if this.menu_hovered != hover {
                        this.menu_hovered = hover;
                        cx.notify();
                    }
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
                .on_mouse_exit(cx.listener(|this, _: &MouseExitEvent, _, cx| {
                    if this.menu_hovered {
                        this.menu_hovered = false;
                        cx.notify();
                    }
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.sync_caps_lock(window.capslock().on);
                    this.sync_host_modifiers(event.keystroke.modifiers);
                    if event.keystroke.modifiers.platform {
                        if let Some((mac_key, character)) = guest_key(&event.keystroke) {
                            let (mac_key, character) = this.map_arrow(mac_key, character);
                            this.press_host_key(mac_key, character);
                            cx.stop_propagation();
                        }
                        return;
                    }
                    // A focused host control owns its ordinary keys. Its semantic
                    // callback queues guest input separately after validation.
                    if this.focus.is_focused(window) {
                        if let Some((mac_key, character)) = guest_key(&event.keystroke) {
                            let (mac_key, character) = this.map_arrow(mac_key, character);
                            this.press_host_key(mac_key, character);
                        }
                    }
                }))
                .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                    if let Some(mac_key) = guest_virtual_key(&event.keystroke.key.to_ascii_lowercase()) {
                        let (mac_key, _) = this.map_arrow(mac_key, 0);
                        this.release_host_key(mac_key);
                    }
                    this.sync_host_modifiers(event.keystroke.modifiers);
                }))
                .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, _| {
                    this.sync_caps_lock(event.capslock.on);
                    this.sync_host_modifiers(event.modifiers);
                }))
                .child(super::metrics::SceneMetrics::new(screen, window.rem_size() * scene_scale))
                .when(!guest_menu_fallback && self.guest_popup.is_none() && (self.menu_presented || menu_hovered), |root| {
                    root.child(bar.unwrap().absolute().top_0().left_0().when(self.menu_presented, |bar| {
                        bar.top(px(self.display_origin.1)).left(px(self.display_origin.0))
                    }))
                })
                .when(self.image.is_none(), |root| {
                    root.child(
                        div()
                            .id("guest-status")
                            .test_support()
                            .px_3()
                            .py_1()
                            .text_xs()
                            .child(self.status.clone()),
                    )
                })
        }
    }

    #[cfg(feature = "gpui-demo-test")]
    #[derive(Clone, Copy, Debug)]
    enum CaptureCase {
        Alert,
        Windows,
        WindowsMoved,
        WindowsActivated,
        WindowsGrown,
        WindowsZoomed,
        WindowsZoomRestored,
        WindowsCustomZoomed,
        WindowsCustomZoomRestored,
        WindowsPromoted,
        WindowsMainPromoted,
        ModalDialog,
        ModalDialogChecked,
        ModalDialogSelection,
        ModalDialogSelectionInactive,
        ModalDialogCaretVisible,
        ModalDialogCaretHidden,
        ModalDialogButtonHeld,
        ModalDialogButtonOutside,
        ModalDialogCheckboxHeld,
        ModalDialogCheckboxCheckedHeld,
        ModalDialogCheckboxOutside,
        ModelessDialog,
        NestedModalDialog,
        Controls,
        ControlsChanged,
        ControlsDragged,
        ControlsHeld,
        RadioHeld,
        RadioOutside,
        RadioSelected,
        Lists,
        ListsSelected,
        ListsSelectedNativeSource,
        ListsHeld,
        ListsCancelled,
        ListsScrolled,
        ListsInactive,
        ListsReactivated,
        ListsMutated,
        ListsResized,
        TextEdit,
        TextEditSelected,
        TextEditEdited,
        TextEditInactive,
        TextEditReactivated,
        TextEditHostSuspended,
        TextEditHostResumed,
        PopupControls,
        PopupControlsSelected,
        PopupControlsHostSuspended,
        PopupControlsDisabled,
        PopupControlsOpen,
        PopupControlsScrolled,
        StandardFileSave,
        StandardFileSaveComposed,
        StandardFileOpenComposed,
        StandardFileSaveEditedComposed,
        StandardFileSaveCaretHiddenComposed,
        StandardFileReplaceComposed,
        StandardFileNewFolderComposed,
        StandardFileNewFolderErrorComposed,
        StandardFileNewFolderSelectedComposed,
        StandardFileNewFolderLongComposed,
        StandardFileNewFolderCaretHiddenComposed,
    }

    #[cfg(feature = "gpui-demo-test")]
    fn select_showcase_resource_popup_long(session: &mut MacintoshSession) {
        let runner = session.runner_mut();
        let (window_top, window_left, _, _) = runner.window_bounds();
        let (vertical, horizontal) = (window_top + 112, window_left + 280);
        let original_value = runner.control_snapshot().iter().find(|control|
            control.visible && control.popup_menu_id == Some(143)).unwrap().value;
        runner.set_mouse_position(vertical, horizontal);
        runner.push_mouse_down(vertical, horizontal);
        for _ in 0..20 {
            runner.run_steps(50_000, None);
        }
        let opened = runner.guest_popup_snapshot().expect("open standard popup geometry");
        assert_eq!(opened.menu.id, 143);
        assert_eq!(
            opened.bounds.1, window_left + 190 + 60,
            "popup must open at the selection box after its title"
        );
        assert_eq!(opened.row_heights.len(), opened.menu.items.len());
        assert!(opened.row_heights.iter().all(|height| *height > 0));
        assert!(opened.bounds.0 <= vertical && vertical < opened.bounds.2);
        assert!(opened.bounds.1 <= horizontal && horizontal < opened.bounds.3);
        assert_eq!(runner.control_snapshot().iter().find(|control| {
            control.visible && control.popup_menu_id == Some(143)
        }).unwrap().value, original_value, "opening must not commit a value");
        let selected_vertical = opened.content_top + opened.row_heights[..3].iter().sum::<i16>() + opened.row_heights[3] / 2;
        runner.set_mouse_position(selected_vertical, horizontal);
        for _ in 0..20 {
            runner.run_steps(50_000, None);
        }
        let highlighted = runner.guest_popup_snapshot().expect("tracked popup geometry");
        assert_eq!(highlighted.menu.guest_id, opened.menu.guest_id);
        assert_eq!(highlighted.menu.generation, opened.menu.generation);
        assert_eq!(highlighted.highlighted_item, 4);
        assert_eq!(highlighted.bounds, opened.bounds);
        runner.push_mouse_up(selected_vertical, horizontal);
        assert!(
            (0..300).any(|_| {
                runner.run_steps(50_000, None);
                runner.control_snapshot().iter().any(|control| {
                    control.visible && control.popup_menu_id == Some(143) && control.value == 4
                }) && runner.guest_popup_snapshot().is_none()
            }),
            "guest should select the popup's long item"
        );
        assert!(runner.guest_popup_snapshot().is_none(), "completed popup must disappear");
    }

    #[cfg(feature = "gpui-demo-test")]
    fn scroll_showcase_theme_popup(session: &mut MacintoshSession) -> (i16, i16) {
        let runner = session.runner_mut();
        let (top, left, _, _) = runner.window_bounds();
        runner.push_mouse_down(top + 148, left + 280);
        let opened = (0..100)
            .find_map(|_| {
                runner.run_steps(50_000, None);
                runner.guest_popup_snapshot()
            })
            .expect("long Theme popup should open");
        assert_eq!(opened.menu.id, 144);
        assert_eq!(opened.menu.items.len(), 55);
        assert_eq!(opened.font.point_size(), 9, "popupUseWFont must retain the owner size");
        assert_eq!(opened.row_heights[0], 12, "Geneva 9 must use native 12-pixel menu rows");
        assert_eq!(opened.bounds.3 - opened.bounds.1, 140,
            "fixed popup must exclude its title and arrow area");
        assert!(opened.scroll_indicators().1);
        runner.set_mouse_position(opened.bounds.2 - 4, opened.bounds.1 + 30);
        let scrolled = (0..100)
            .find_map(|_| {
                runner.run_steps(50_000, None);
                runner
                    .guest_popup_snapshot()
                    .filter(|popup| !popup.scroll_indicators().1)
            })
            .expect("held down arrow should reveal the end of the popup");
        assert!(scrolled.scroll_indicators().0);
        assert!(scrolled.content_top < opened.content_top);
        let last = scrolled.row_heights.len() - 1;
        let point = (
            scrolled.content_top
                + scrolled.row_heights[..last].iter().sum::<i16>()
                + scrolled.row_heights[last] / 2,
            scrolled.bounds.1 + 30,
        );
        runner.set_mouse_position(point.0, point.1);
        for _ in 0..20 {
            runner.run_steps(50_000, None);
        }
        assert_eq!(runner.guest_popup_snapshot().unwrap().highlighted_item, 55);
        point
    }

    #[cfg(feature = "gpui-demo-test")]
    fn set_showcase_popups_enabled(session: &mut MacintoshSession, enabled: bool) {
        let (mac_key, character) = if enabled { (0x0e, b'e') } else { (0x02, b'd') };
        session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
        session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
        assert!((0..300).any(|_| {
            session.runner_mut().run_steps(10_000, None);
            let controls = session.runner_mut().control_snapshot();
            [143, 144].into_iter().all(|id| controls.iter().any(|control|
                control.visible && control.popup_menu_id == Some(id) && control.enabled == enabled))
        }), "guest HiliteControl must change both popup controls");
        assert!((0..300).any(|_| {
            session.runner_mut().run_steps(10_000, None);
            session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0)
        }), "guest must finish repainting disabled popup text");
    }

    #[cfg(feature = "gpui-demo-test")]
    fn capture_fixture_screen(
        game: &std::path::Path,
        output: &std::path::Path,
        prefer_powerpc: bool,
        screen_depth: Option<u16>,
        capture: CaptureCase,
        capture_scale: Option<f32>,
    ) {
        use gpui_kit::{platform, HeadlessAppContext};

        let controls_page = matches!(
            capture,
            CaptureCase::Controls
                | CaptureCase::ControlsChanged
                | CaptureCase::ControlsDragged
                | CaptureCase::ControlsHeld
        );
        let windows_page = matches!(
            capture,
            CaptureCase::Windows
                | CaptureCase::WindowsMoved
                | CaptureCase::WindowsActivated
                | CaptureCase::WindowsGrown
                | CaptureCase::WindowsZoomed
                | CaptureCase::WindowsZoomRestored
                | CaptureCase::WindowsCustomZoomed
                | CaptureCase::WindowsCustomZoomRestored
                | CaptureCase::WindowsPromoted
                | CaptureCase::WindowsMainPromoted
        );
        let lists_page = matches!(capture, CaptureCase::Lists | CaptureCase::ListsSelected | CaptureCase::ListsSelectedNativeSource | CaptureCase::ListsHeld | CaptureCase::ListsCancelled
            | CaptureCase::ListsScrolled | CaptureCase::ListsInactive | CaptureCase::ListsReactivated | CaptureCase::ListsMutated | CaptureCase::ListsResized);
        let text_edit_page = matches!(
            capture,
            CaptureCase::TextEdit | CaptureCase::TextEditSelected | CaptureCase::TextEditEdited | CaptureCase::TextEditInactive | CaptureCase::TextEditReactivated | CaptureCase::TextEditHostSuspended | CaptureCase::TextEditHostResumed
        );
        let popup_page = matches!(
            capture,
            CaptureCase::PopupControls | CaptureCase::PopupControlsSelected | CaptureCase::PopupControlsHostSuspended | CaptureCase::PopupControlsDisabled | CaptureCase::PopupControlsOpen | CaptureCase::PopupControlsScrolled
        );
        let standard_file_save = matches!(
            capture,
            CaptureCase::StandardFileSave
                | CaptureCase::StandardFileSaveComposed
                | CaptureCase::StandardFileSaveEditedComposed
                | CaptureCase::StandardFileSaveCaretHiddenComposed
                | CaptureCase::StandardFileReplaceComposed
                | CaptureCase::StandardFileNewFolderComposed
                | CaptureCase::StandardFileNewFolderErrorComposed
                | CaptureCase::StandardFileNewFolderSelectedComposed
                | CaptureCase::StandardFileNewFolderLongComposed | CaptureCase::StandardFileNewFolderCaretHiddenComposed
        );
        let standard_file_open = matches!(capture, CaptureCase::StandardFileOpenComposed);
        let standard_file_page = standard_file_save || standard_file_open;
        let radio_page = matches!(capture, CaptureCase::RadioHeld | CaptureCase::RadioOutside | CaptureCase::RadioSelected);
        let controls_changed = matches!(capture, CaptureCase::ControlsChanged);
        let controls_dragged = matches!(capture, CaptureCase::ControlsDragged);
        let controls_held = matches!(capture, CaptureCase::ControlsHeld);

        let mut session = MacintoshSession::new(true,
            if prefer_powerpc { Some(8) } else { screen_depth.or(Some(8)) });
        session
            .runner_mut()
            .set_prefer_powerpc_executables(prefer_powerpc);
        if prefer_powerpc {
            session.runner_mut().set_powerpc_screen_depth(screen_depth.unwrap_or(16)).unwrap();
        }
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
        } else if windows_page {
            (129, 3)
        } else if matches!(
            capture,
            CaptureCase::ModelessDialog | CaptureCase::NestedModalDialog
        ) {
            (132, 7)
        } else if matches!(capture, CaptureCase::ModalDialog | CaptureCase::ModalDialogChecked | CaptureCase::ModalDialogSelection | CaptureCase::ModalDialogSelectionInactive | CaptureCase::ModalDialogCaretVisible | CaptureCase::ModalDialogCaretHidden | CaptureCase::ModalDialogButtonHeld | CaptureCase::ModalDialogButtonOutside | CaptureCase::ModalDialogCheckboxHeld | CaptureCase::ModalDialogCheckboxCheckedHeld | CaptureCase::ModalDialogCheckboxOutside) {
            (129, 6)
        } else if lists_page {
            (129, 9)
        } else if text_edit_page {
            (129, 7)
        } else if popup_page {
            (129, 16)
        } else if radio_page {
            (129, 5)
        } else if controls_page {
            (129, 2)
        } else {
            (128, 1)
        };
        assert!(session.runner_mut().select_guest_menu_item(menu_id, item));
        let dialogs = if windows_page {
            let before = (0..300)
                .find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let frames = session.runner_mut().window_frame_snapshot();
                    (frames.len() == 3).then_some(frames)
                })
                .expect("three showcase windows should become visible");
            if matches!(
                capture,
                CaptureCase::WindowsCustomZoomed | CaptureCase::WindowsCustomZoomRestored
            ) {
                let original = before[0].window.bounds;
                let zoom = (original.0 - 9, original.3 - 7);
                for input in [
                    MacintoshInput::MouseDown {
                        vertical: zoom.0,
                        horizontal: zoom.1,
                    },
                    MacintoshInput::MouseUp {
                        vertical: zoom.0,
                        horizontal: zoom.1,
                    },
                ] {
                    session.deliver_input(input);
                    let start = session.runner().guest_tick();
                    assert!(
                        (0..100).any(|_| {
                            session.runner_mut().run_steps(10_000, None);
                            session.runner().guest_tick().wrapping_sub(start) >= 2
                        }),
                        "guest should advance during custom window zoom"
                    );
                }
                let zoomed = session.runner_mut().window_frame_snapshot();
                assert_eq!(zoomed[0].guest_id, before[0].guest_id);
                assert_eq!(zoomed[0].generation, before[0].generation);
                assert_eq!(zoomed[0].window.bounds, (100, 100, 520, 700));
                if matches!(capture, CaptureCase::WindowsCustomZoomRestored) {
                    let bounds = zoomed[0].window.bounds;
                    let restore = (bounds.0 - 9, bounds.3 - 7);
                    for input in [
                        MacintoshInput::MouseDown {
                            vertical: restore.0,
                            horizontal: restore.1,
                        },
                        MacintoshInput::MouseUp {
                            vertical: restore.0,
                            horizontal: restore.1,
                        },
                    ] {
                        session.deliver_input(input);
                        let start = session.runner().guest_tick();
                        assert!(
                            (0..100).any(|_| {
                                session.runner_mut().run_steps(10_000, None);
                                session.runner().guest_tick().wrapping_sub(start) >= 2
                            }),
                            "guest should advance during custom window zoom restore"
                        );
                    }
                    let restored = session.runner_mut().window_frame_snapshot();
                    assert_eq!(restored[0].guest_id, before[0].guest_id);
                    assert_eq!(restored[0].generation, before[0].generation);
                    assert_eq!(restored[0].window.bounds, original);
                }
            } else if matches!(capture, CaptureCase::WindowsMoved) {
                let (top, left, bottom, right) = before[0].window.bounds;
                let from = (top - 9, (left + right) / 2);
                for input in [
                    MacintoshInput::MouseDown {
                        vertical: from.0,
                        horizontal: from.1,
                    },
                    MacintoshInput::MouseMove {
                        vertical: from.0 + 12,
                        horizontal: from.1 + 16,
                    },
                    MacintoshInput::MouseUp {
                        vertical: from.0 + 12,
                        horizontal: from.1 + 16,
                    },
                ] {
                    session.deliver_input(input);
                    let start = session.runner().guest_tick();
                    assert!(
                        (0..100).any(|_| {
                            session.runner_mut().run_steps(10_000, None);
                            session.runner().guest_tick().wrapping_sub(start) >= 2
                        }),
                        "guest should advance during window tracking"
                    );
                }
                let moved = session.runner_mut().window_frame_snapshot();
                assert_eq!(moved[0].guest_id, before[0].guest_id);
                assert_eq!(moved[0].generation, before[0].generation);
                assert_eq!(
                    moved[0].window.bounds,
                    (top + 12, left + 16, bottom + 12, right + 16)
                );
            } else if matches!(
                capture,
                CaptureCase::WindowsActivated
                    | CaptureCase::WindowsGrown
                    | CaptureCase::WindowsZoomed
                    | CaptureCase::WindowsZoomRestored
            ) {
                // tests/toolbox-showcase/oracle/windows.json activates the
                // exposed auxiliary content at this guest coordinate.
                for input in [
                    MacintoshInput::MouseDown {
                        vertical: 240,
                        horizontal: 210,
                    },
                    MacintoshInput::MouseUp {
                        vertical: 240,
                        horizontal: 210,
                    },
                ] {
                    session.deliver_input(input);
                    let start = session.runner().guest_tick();
                    assert!(
                        (0..100).any(|_| {
                            session.runner_mut().run_steps(10_000, None);
                            session.runner().guest_tick().wrapping_sub(start) >= 2
                        }),
                        "guest should advance while activating rear window"
                    );
                }
                let activated = session.runner_mut().window_frame_snapshot();
                assert_eq!(activated[0].guest_id, before[1].guest_id);
                assert!(activated[0].window.active);
                assert!(!activated[1].window.active);
                if matches!(
                    capture,
                    CaptureCase::WindowsGrown
                        | CaptureCase::WindowsZoomed
                        | CaptureCase::WindowsZoomRestored
                ) {
                    let (top, left, bottom, right) = activated[0].window.bounds;
                    let from = (bottom - 5, right - 10);
                    for input in [
                        MacintoshInput::MouseDown {
                            vertical: from.0,
                            horizontal: from.1,
                        },
                        MacintoshInput::MouseMove {
                            vertical: from.0 + 25,
                            horizontal: from.1 + 25,
                        },
                        MacintoshInput::MouseUp {
                            vertical: from.0 + 25,
                            horizontal: from.1 + 25,
                        },
                    ] {
                        session.deliver_input(input);
                        let start = session.runner().guest_tick();
                        assert!(
                            (0..100).any(|_| {
                                session.runner_mut().run_steps(10_000, None);
                                session.runner().guest_tick().wrapping_sub(start) >= 2
                            }),
                            "guest should advance during window growth"
                        );
                    }
                    let grown = session.runner_mut().window_frame_snapshot();
                    assert_eq!(grown[0].guest_id, activated[0].guest_id);
                    assert_eq!(grown[0].generation, activated[0].generation);
                    assert_eq!(
                        grown[0].window.bounds,
                        (top, left, bottom + 25, right + 25)
                    );
                    if matches!(
                        capture,
                        CaptureCase::WindowsZoomed | CaptureCase::WindowsZoomRestored
                    ) {
                        let (grown_top, _, _, grown_right) = grown[0].window.bounds;
                        let zoom = (grown_top - 9, grown_right - 7);
                        for input in [
                            MacintoshInput::MouseDown {
                                vertical: zoom.0,
                                horizontal: zoom.1,
                            },
                            MacintoshInput::MouseUp {
                                vertical: zoom.0,
                                horizontal: zoom.1,
                            },
                        ] {
                            session.deliver_input(input);
                            let start = session.runner().guest_tick();
                            assert!(
                                (0..100).any(|_| {
                                    session.runner_mut().run_steps(10_000, None);
                                    session.runner().guest_tick().wrapping_sub(start) >= 2
                                }),
                                "guest should advance during window zoom"
                            );
                        }
                        let zoomed = session.runner_mut().window_frame_snapshot();
                        assert_eq!(zoomed[0].guest_id, grown[0].guest_id);
                        assert_eq!(zoomed[0].generation, grown[0].generation);
                        assert_ne!(zoomed[0].window.bounds, grown[0].window.bounds);
                        assert!(zoomed[0].window.bounds.0 >= 40);
                        if matches!(capture, CaptureCase::WindowsZoomRestored) {
                            let (zoomed_top, _, _, zoomed_right) = zoomed[0].window.bounds;
                            let restore = (zoomed_top - 9, zoomed_right - 7);
                            for input in [
                                MacintoshInput::MouseDown {
                                    vertical: restore.0,
                                    horizontal: restore.1,
                                },
                                MacintoshInput::MouseUp {
                                    vertical: restore.0,
                                    horizontal: restore.1,
                                },
                            ] {
                                session.deliver_input(input);
                                let start = session.runner().guest_tick();
                                assert!(
                                    (0..100).any(|_| {
                                        session.runner_mut().run_steps(10_000, None);
                                        session.runner().guest_tick().wrapping_sub(start) >= 2
                                    }),
                                    "guest should advance during window zoom restore"
                                );
                            }
                            let restored = session.runner_mut().window_frame_snapshot();
                            assert_eq!(restored[0].guest_id, grown[0].guest_id);
                            assert_eq!(restored[0].generation, grown[0].generation);
                            assert_eq!(restored[0].window.bounds, grown[0].window.bounds);
                        }
                    }
                }
            } else if matches!(
                capture,
                CaptureCase::WindowsPromoted | CaptureCase::WindowsMainPromoted
            ) {
                let mut expected_front = before[1].guest_id;
                let mut expected_count = 2;
                let close_count = if matches!(capture, CaptureCase::WindowsMainPromoted) {
                    2
                } else {
                    1
                };
                for close_index in 0..close_count {
                    let frames = session.runner_mut().window_frame_snapshot();
                    let (top, left, _, _) = frames[0].window.bounds;
                    let close = (top - 9, left + 9);
                    for input in [
                        MacintoshInput::MouseDown {
                            vertical: close.0,
                            horizontal: close.1,
                        },
                        MacintoshInput::MouseUp {
                            vertical: close.0,
                            horizontal: close.1,
                        },
                    ] {
                        session.deliver_input(input);
                        let start = session.runner().guest_tick();
                        assert!(
                            (0..100).any(|_| {
                                session.runner_mut().run_steps(10_000, None);
                                session.runner().guest_tick().wrapping_sub(start) >= 2
                            }),
                            "guest should advance during close tracking"
                        );
                    }
                    let promoted = session.runner_mut().window_frame_snapshot();
                    assert_eq!(promoted.len(), expected_count);
                    assert_eq!(promoted[0].guest_id, expected_front);
                    assert!(promoted[0].window.active);
                    if close_index == 0 {
                        expected_front = before[2].guest_id;
                        expected_count = 1;
                    }
                }
            }
            Vec::new()
        } else if matches!(
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
        } else if matches!(capture, CaptureCase::ModalDialog | CaptureCase::ModalDialogChecked | CaptureCase::ModalDialogSelection | CaptureCase::ModalDialogSelectionInactive | CaptureCase::ModalDialogCaretVisible | CaptureCase::ModalDialogCaretHidden | CaptureCase::ModalDialogButtonHeld | CaptureCase::ModalDialogButtonOutside | CaptureCase::ModalDialogCheckboxHeld | CaptureCase::ModalDialogCheckboxCheckedHeld | CaptureCase::ModalDialogCheckboxOutside) {
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
            if matches!(capture, CaptureCase::ModalDialogChecked | CaptureCase::ModalDialogCheckboxCheckedHeld) {
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
            if matches!(capture, CaptureCase::ModalDialogSelection | CaptureCase::ModalDialogSelectionInactive) {
                let field = &dialogs.iter().find(|dialog| dialog.visible && dialog.active).unwrap().items[8];
                let bounds = field.bounds;
                let layout = field.edit_text_layout.as_ref().unwrap();
                let line = super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
                for input in [
                    MacintoshInput::MouseDown { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 + line.positions[3] as i16 },
                    MacintoshInput::MouseMove { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 + line.positions[6] as i16 },
                    MacintoshInput::MouseUp { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 + line.positions[6] as i16 },
                ] {
                    session.deliver_input(input);
                    for _ in 0..10 {
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    }
                }
                dialogs = session.runner_mut().dialog_snapshot();
                assert!(dialogs.iter().any(|dialog| dialog.visible && dialog.active
                    && dialog.items[8].selection == Some((3, 6))));
                if matches!(capture, CaptureCase::ModalDialogSelectionInactive) {
                    let bounds = dialogs.iter().find(|dialog| dialog.visible && dialog.active).unwrap().items[6].bounds;
                    for input in [
                        MacintoshInput::MouseDown { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 },
                        MacintoshInput::MouseUp { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 },
                    ] {
                        session.deliver_input(input);
                        for _ in 0..10 {
                            let tick = session.runner().guest_tick().saturating_add(1);
                            session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                        }
                    }
                    dialogs = session.runner_mut().dialog_snapshot();
                    assert!(dialogs.iter().any(|dialog| dialog.visible && dialog.active
                        && dialog.edit_field == Some(7) && dialog.items[6].selection == Some((0, 0))
                        && dialog.items[8].selection == if prefer_powerpc { None } else { Some((3, 6)) }
                        && dialog.items[8].text == "Maverick"),
                        "switching focus must preserve text and expose only guest-owned selection");
                }
            }
            if matches!(capture, CaptureCase::ModalDialogCaretVisible | CaptureCase::ModalDialogCaretHidden) {
                let visible = matches!(capture, CaptureCase::ModalDialogCaretVisible);
                dialogs = (0..65).find_map(|_| {
                    let current = session.runner_mut().dialog_snapshot();
                    if current.iter().any(|dialog| {
                        dialog.visible && dialog.active
                            && dialog.items.iter().any(|item| {
                                item.number == 7 && item.selection == Some((0, 0))
                                    && item.caret_visible == Some(visible)
                            })
                    }) {
                        return Some(current);
                    }
                    session.runner_mut().force_advance_guest_tick();
                    session.runner_mut().run_steps(1_000, None);
                    None
                }).expect("modal edit field should reach the requested guest caret phase");
            }
            if matches!(capture, CaptureCase::ModalDialogButtonHeld | CaptureCase::ModalDialogButtonOutside | CaptureCase::ModalDialogCheckboxHeld | CaptureCase::ModalDialogCheckboxCheckedHeld | CaptureCase::ModalDialogCheckboxOutside) {
                let dialog = dialogs.iter().find(|dialog| dialog.visible && dialog.active).unwrap();
                let checkbox = matches!(capture, CaptureCase::ModalDialogCheckboxHeld | CaptureCase::ModalDialogCheckboxCheckedHeld | CaptureCase::ModalDialogCheckboxOutside);
                let target = dialog.items.iter().find(|item| if checkbox {
                    item.kind == DialogItemKind::Checkbox
                } else { item.kind == DialogItemKind::Button && item.text == "Cancel" }).unwrap();
                let (dialog_id, item_number, rect) = (dialog.guest_id, target.number, target.bounds);
                session.deliver_input(MacintoshInput::MouseDown {
                    vertical: (rect.0 + rect.2) / 2, horizontal: (rect.1 + rect.3) / 2,
                });
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                let outside = matches!(capture, CaptureCase::ModalDialogButtonOutside | CaptureCase::ModalDialogCheckboxOutside);
                if outside {
                    session.deliver_input(MacintoshInput::MouseMove { vertical: rect.0 - 10, horizontal: rect.1 - 10 });
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                dialogs = session.runner_mut().dialog_snapshot();
                let current = dialogs.iter().find(|dialog| dialog.guest_id == dialog_id && dialog.visible).unwrap();
                assert_eq!(current.items.iter().find(|item| item.number == item_number).unwrap().pressed, !outside);
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
            if matches!(capture, CaptureCase::StandardFileNewFolderComposed | CaptureCase::StandardFileNewFolderErrorComposed | CaptureCase::StandardFileNewFolderSelectedComposed | CaptureCase::StandardFileNewFolderLongComposed | CaptureCase::StandardFileNewFolderCaretHiddenComposed) {
                let panel = session.runner().standard_file_snapshot().unwrap();
                let click = super::activation::ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, super::activation::FileAction::NewFolder).unwrap();
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                let click = click.advance(&mut session).unwrap();
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                assert!(click.advance(&mut session).is_none());
                let panel = session.runner().standard_file_snapshot().unwrap();
                let folder = panel.new_folder.as_ref().unwrap();
                assert_eq!(folder.name, "untitled folder");
                assert_eq!(folder.selection, (0, 15));
                if matches!(capture, CaptureCase::StandardFileNewFolderLongComposed | CaptureCase::StandardFileNewFolderCaretHiddenComposed) {
                    for character in b"wwwwwwwwwwwwwwwwwwwwwwwwwwwabcd" {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: *character });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: *character });
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    }
                    let folder = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                    assert_eq!(folder.selection, (31, 31));
                    assert_eq!(folder.visible_offset, 31);
                    assert!(folder.caret_visible);
                    if matches!(capture, CaptureCase::StandardFileNewFolderCaretHiddenComposed) {
                        let initial_tick = session.runner().guest_tick();
                        for _ in 0..40 {
                            let tick = session.runner().guest_tick().saturating_add(1);
                            session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                            if !session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().caret_visible {
                                break;
                            }
                        }
                        let folder = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                        assert!(!folder.caret_visible, "caret did not enter its hidden phase");
                        assert_eq!(session.runner().guest_tick().wrapping_sub(initial_tick), 32);
                        assert_eq!(folder.selection, (31, 31));
                        assert_eq!(folder.visible_offset, 31);
                    }
                }
                if matches!(capture, CaptureCase::StandardFileNewFolderSelectedComposed) {
                    let field = folder.layout.name;
                    let vertical = (field.0 + field.2) / 2;
                    for input in [
                        MacintoshInput::MouseDown { vertical, horizontal: field.1 + 1 },
                        MacintoshInput::MouseMove { vertical, horizontal: field.1 + 50 },
                        MacintoshInput::MouseUp { vertical, horizontal: field.1 + 50 },
                    ] {
                        session.deliver_input(input);
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    }
                    let selected = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                    assert_eq!(selected.selection.0, 0);
                    assert!(selected.selection.1 > 0 && selected.selection.1 < 15);
                }
                if matches!(capture, CaptureCase::StandardFileNewFolderErrorComposed) {
                    let name = panel.entries.as_ref().unwrap().iter().find(|entry| entry.is_directory).unwrap().name.clone();
                    for character in name.bytes() {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    }
                    let click = super::activation::ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, super::activation::FileAction::CreateFolder).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    let click = click.advance(&mut session).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    assert!(click.advance(&mut session).is_none());
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().error, Some(-48));
                }
            }
            if matches!(capture, CaptureCase::StandardFileReplaceComposed) {
                let panel = session.runner().standard_file_snapshot().unwrap();
                let name = panel.entries.as_ref().unwrap().iter().find(|entry| !entry.is_directory).unwrap().name.clone();
                for character in name.bytes() {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                }
                let click = super::activation::ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, super::activation::FileAction::Accept).unwrap();
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                let click = click.advance(&mut session).unwrap();
                let tick = session.runner().guest_tick().saturating_add(1);
                session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                assert!(click.advance(&mut session).is_none());
                assert!(session.runner().standard_file_snapshot().unwrap().confirming_replace);
            }
            if matches!(capture, CaptureCase::StandardFileSaveEditedComposed | CaptureCase::StandardFileSaveCaretHiddenComposed) {
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
                let visible = matches!(capture, CaptureCase::StandardFileSaveEditedComposed);
                assert!((0..120).any(|_| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    session.runner().standard_file_snapshot().is_some_and(|panel| {
                        panel.name.as_deref() == Some("S") && panel.name_selection == Some((1, 1))
                            && panel.name_caret_visible == Some(visible)
                    })
                }), "guest Save caret capture phase");
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
        } else if radio_page {
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().control_snapshot().iter().any(|c| c.visible && c.title == "Recruit (Easy)")
            }));
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
        if matches!(
            capture,
            CaptureCase::WindowsZoomed
                | CaptureCase::WindowsZoomRestored
                | CaptureCase::WindowsCustomZoomed
                | CaptureCase::WindowsCustomZoomRestored
        ) {
            for _ in 0..20 {
                session.runner_mut().run_steps(100_000, None);
            }
        }
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
        } else if !windows_page
            && !radio_page
            && !controls_page
            && !lists_page
            && !text_edit_page
            && !popup_page
            && !standard_file_page
        {
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
        if matches!(capture, CaptureCase::ListsSelected | CaptureCase::ListsSelectedNativeSource | CaptureCase::ListsScrolled | CaptureCase::ListsInactive | CaptureCase::ListsReactivated | CaptureCase::ListsMutated | CaptureCase::ListsResized) {
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
        if matches!(capture, CaptureCase::ListsScrolled | CaptureCase::ListsInactive | CaptureCase::ListsReactivated | CaptureCase::ListsMutated | CaptureCase::ListsResized) {
            let before = session.runner_mut().list_manager_snapshot().into_iter()
                .find(|list| list.draw_enabled && list.definition_id == 0).unwrap();
            let title = match capture {
                CaptureCase::ListsScrolled => "Scroll Four Rows",
                CaptureCase::ListsMutated => "Update Selected Row",
                CaptureCase::ListsResized => "Resize List",
                _ => "Toggle Activation",
            };
            let repetitions = if matches!(capture, CaptureCase::ListsReactivated) { 2 } else { 1 };
            for _ in 0..repetitions {
                let control = session.runner_mut().control_snapshot().into_iter()
                    .find(|control| control.visible && control.title == title).expect("guest transition button");
                let point = ((control.bounds.0 + control.bounds.2) / 2, (control.bounds.1 + control.bounds.3) / 2);
                for input in [MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 },
                    MacintoshInput::MouseUp { vertical: point.0, horizontal: point.1 }] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
            }
            let after = session.runner_mut().list_manager_snapshot().into_iter()
                .find(|list| list.draw_enabled && list.definition_id == 0).unwrap();
            assert_eq!((after.guest_id, after.generation, after.owner_port),
                (before.guest_id, before.generation, before.owner_port), "transition preserves list identity");
            assert_eq!(after.selected, before.selected, "transition preserves guest selection");
            if matches!(capture, CaptureCase::ListsScrolled) { assert_eq!(after.visible.0, 4); }
            else if matches!(capture, CaptureCase::ListsMutated) {
                let mut expected = before.cells[&(7, 0)].clone();
                expected.extend_from_slice(b"  * updated");
                assert_eq!(after.cells[&(7, 0)], expected);
                assert!(after.cells.iter().filter(|(cell, _)| **cell != (7, 0))
                    .all(|(cell, bytes)| before.cells.get(cell) == Some(bytes)));
                assert!(after.active);
            } else if matches!(capture, CaptureCase::ListsResized) {
                assert_eq!(after.view_rect, (78, 24, 192, 474));
                assert_eq!(after.cells, before.cells);
                assert!(after.active);
            } else { assert_eq!(after.active, matches!(capture, CaptureCase::ListsReactivated)); }
        }
        if matches!(capture, CaptureCase::PopupControlsScrolled) {
            scroll_showcase_theme_popup(&mut session);
        }
        if matches!(capture, CaptureCase::PopupControlsOpen) {
            let runner = session.runner_mut();
            let (top, left, _, _) = runner.window_bounds();
            runner.push_mouse_down(top + 112, left + 280);
            for _ in 0..20 { runner.run_steps(50_000, None); }
            runner.set_mouse_position(top + 146, left + 280);
            for _ in 0..20 { runner.run_steps(50_000, None); }
            assert_eq!(runner.guest_popup_snapshot().unwrap().highlighted_item, 4);
        }
        if matches!(capture, CaptureCase::PopupControlsSelected | CaptureCase::PopupControlsHostSuspended | CaptureCase::PopupControlsDisabled) {
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
        if matches!(capture, CaptureCase::ListsHeld | CaptureCase::ListsCancelled) {
            let list = session.runner_mut().list_manager_snapshot().into_iter()
                .find(|list| list.draw_enabled && list.definition_id == 0).unwrap();
            let bounds = list.global_view_rect.unwrap();
            let bar = session.runner_mut().control_snapshot().into_iter()
                .find(|control| control.visible && control.proc_id == 16
                    && control.bounds.1 == bounds.3
                    && super::frames::scrollbar_geometry(control).vertical).unwrap();
            let thumb = super::frames::scrollbar_geometry(&bar);
            assert!(bar.maximum > bar.minimum);
            let from = (bar.bounds.0 + thumb.thumb_start as i16 + 8,
                (bar.bounds.1 + bar.bounds.3) / 2);
            let to = (bar.bounds.2 - 24, from.1);
            let cancelled = matches!(capture, CaptureCase::ListsCancelled);
            let mut inputs = vec![
                MacintoshInput::MouseDown { vertical: from.0, horizontal: from.1 },
                MacintoshInput::MouseMove { vertical: to.0, horizontal: to.1 },
            ];
            if cancelled {
                let outside = bar.bounds.3 + 40;
                inputs.push(MacintoshInput::MouseMove { vertical: to.0, horizontal: outside });
                inputs.push(MacintoshInput::MouseUp { vertical: to.0, horizontal: outside });
            }
            for input in inputs {
                session.deliver_input(input);
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
            }
            let after = session.runner_mut().list_manager_snapshot().into_iter()
                .find(|after| after.guest_id == list.guest_id).unwrap();
            assert_eq!(after.visible, list.visible);
            assert_eq!(after.selected, list.selected);
            if !cancelled {
                held_drag = Some((bar.guest_id, bar.generation, from, to));
            }
        }

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
        if radio_page {
            let target = session.runner_mut().control_snapshot().into_iter()
                .find(|c| c.visible && c.title == "Recruit (Easy)").unwrap();
            let rect = target.bounds;
            let inside = ((rect.0 + rect.2) / 2, (rect.1 + rect.3) / 2);
            let mut inputs = vec![MacintoshInput::MouseDown { vertical: inside.0, horizontal: inside.1 }];
            if matches!(capture, CaptureCase::RadioOutside) {
                inputs.push(MacintoshInput::MouseMove { vertical: rect.0 - 10, horizontal: rect.1 - 10 });
            } else if matches!(capture, CaptureCase::RadioSelected) {
                inputs.push(MacintoshInput::MouseUp { vertical: inside.0, horizontal: inside.1 });
            }
            for input in inputs {
                session.deliver_input(input);
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
            }
            let controls = session.runner_mut().control_snapshot();
            let current = controls.iter().find(|c| c.guest_id == target.guest_id).unwrap();
            assert_eq!(current.hilite, if matches!(capture, CaptureCase::RadioHeld) { 11 } else { 0 });
            assert_eq!(current.value, if matches!(capture, CaptureCase::RadioSelected) { 1 } else { 0 });
        }
        if matches!(capture, CaptureCase::PopupControlsDisabled) {
            set_showcase_popups_enabled(&mut session, false);
        }
        let controls = session.runner_mut().control_snapshot();
        if matches!(capture, CaptureCase::PopupControlsDisabled) {
            assert!(controls.iter().filter(|c| c.visible && c.popup_menu_id.is_some()).all(|c| !c.enabled));
            eprintln!("disabled popup ink: {:?}", controls.iter().filter(|c| c.visible && c.popup_menu_id.is_some())
                .map(|c| (c.popup_menu_id, c.hilite, c.popup_ink)).collect::<Vec<_>>());
        }
        let lists = session.runner_mut().list_manager_snapshot();
        if matches!(capture, CaptureCase::TextEditSelected | CaptureCase::TextEditEdited | CaptureCase::TextEditInactive | CaptureCase::TextEditReactivated | CaptureCase::TextEditHostSuspended | CaptureCase::TextEditHostResumed) {
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
        let activation_capture = matches!(capture, CaptureCase::TextEditInactive | CaptureCase::TextEditReactivated);
        if activation_capture {
            assert!(session.runner_mut().select_guest_menu_item(132, 7));
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(100_000, None);
                session.runner_mut().text_edit_snapshot().records.iter().any(|record|
                    record.view_rect == (76, 34, 211, 326) && !record.active && record.selection == (0, 14))
            }), "document selection must become inactive");
            // Let the dialog consume its activation and initial update events.
            for _ in 0..30 { session.runner_mut().run_steps(100_000, None); }
            if matches!(capture, CaptureCase::TextEditReactivated) {
                for input in [
                    MacintoshInput::MouseDown { vertical: 100, horizontal: 70 },
                    MacintoshInput::MouseUp { vertical: 100, horizontal: 70 },
                ] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().text_edit_snapshot().records.iter().any(|record|
                        record.view_rect == (76, 34, 211, 326) && record.active && record.selection == (0, 14))
                }), "document activation must preserve the selection");
                // Activation precedes the newly exposed document's updateEvt.
                for _ in 0..30 { session.runner_mut().run_steps(100_000, None); }
            }
        }
        let host_activation_capture = matches!(capture, CaptureCase::TextEditHostSuspended | CaptureCase::TextEditHostResumed);
        if host_activation_capture {
            let states: &[bool] = if matches!(capture, CaptureCase::TextEditHostResumed) { &[false, true] } else { &[false] };
            for &active in states {
                session.request_foreground(active);
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(10_000, None);
                    session.runner_mut().text_edit_snapshot().records.iter().any(|record|
                        record.view_rect == (76, 34, 211, 326) && record.active == active && record.selection == (0, 14))
                }), "host switch must preserve the guest selection");
                // TEActivate/TEDeactivate changes active before completing
                // its drawing. Capture only after the guest finishes handling
                // the notification and returns to the event loop.
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(10_000, None);
                    session.runner().event_manager_snapshot().last_record
                        .is_some_and(|event| event.what == 0)
                }), "guest must finish painting the host transition");
            }
        }
        let popup_host_suspended = matches!(capture, CaptureCase::PopupControlsHostSuspended);
        if popup_host_suspended {
            let owner = session.runner_mut().control_snapshot().iter()
                .find(|control| control.visible && control.popup_menu_id == Some(143))
                .expect("selected popup must retain its owner").owner_id;
            session.request_foreground(false);
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(10_000, None);
                session.runner_mut().window_frame_snapshot().iter()
                    .any(|frame| frame.guest_id == owner && frame.window.visible && !frame.window.active)
            }), "popup owner must become inactive through guest suspend events");
            assert!((0..300).any(|_| {
                session.runner_mut().run_steps(10_000, None);
                session.runner().event_manager_snapshot().last_record
                    .is_some_and(|event| event.what == 0)
            }), "guest must finish its popup-owner suspend handling");
            let current = session.runner_mut().control_snapshot();
            assert!(current.iter().any(|control| control.visible
                && control.popup_menu_id == Some(143) && control.value == 4));
            assert!(session.runner_mut().guest_popup_snapshot().is_none());
        }
        let activation_capture = activation_capture || host_activation_capture || popup_host_suspended;
        let windows = if activation_capture { session.runner_mut().window_frame_snapshot() } else { windows };
        let dialogs = if activation_capture { session.runner_mut().dialog_snapshot() } else { dialogs };
        if popup_host_suspended {
            for frame in &windows {
                eprintln!("suspended popup owner: id={:#x}, title={:?}, active={}, bounds={:?}, title_layout={:?}",
                    frame.guest_id, frame.window.title, frame.window.active, frame.window.bounds,
                    frame.title_layout(session.runner().bus().read_word(MBAR_HEIGHT) as i16));
            }
        }
        let text_edits = session.runner_mut().text_edit_snapshot().records;
        let standard_file = session.runner_mut().standard_file_snapshot();
        let menus = session.runner_mut().guest_menu_snapshot();
        let guest_popup = session.runner_mut().guest_popup_snapshot();
        let frame = session.video_frame().unwrap();
        let styled_text_plans = qualify_styled_text_fields(&text_edits, &frame.pixels, frame.width, frame.height);
        let list_text_plans = qualify_list_text_fields(&lists, &frame.pixels, frame.width, frame.height);
        if matches!(capture, CaptureCase::WindowsZoomRestored) {
            // This exposed main-window point used to retain the zoomed
            // auxiliary window's blue pixels after the 68K zoom-back.
            let offset = ((100 * frame.width + 100) * 4) as usize;
            assert_eq!(&frame.pixels[offset..offset + 3], &[255, 255, 255]);
        }
        let guest_menu_tracking = session.runner().guest_menu_tracking_active();
        if matches!(capture, CaptureCase::NestedModalDialog) {
            assert!(!menus.requires_guest_menu_rendering());
            assert!(!guest_menu_tracking, "nested dialog capture must compose GPUI overlays");
        }
        let menu_presented = session.runner().guest_menu_bar_presented();
        let menu_height = session.runner().bus().read_word(MBAR_HEIGHT);
        if !matches!(capture, CaptureCase::StandardFileSave) {
            // Preserve the exact pre-compositor frame alongside the themed
            // capture so an overlay cannot conceal a guest-painting defect.
            let guest_output = output.with_extension("guest.png");
            image::save_buffer(
                &guest_output,
                &frame.pixels,
                frame.width,
                frame.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            eprintln!("saved guest frame to {}", guest_output.display());
        }
        if matches!(capture, CaptureCase::StandardFileReplaceComposed) {
            let panel = session.runner().standard_file_snapshot().unwrap();
            let layout = systemless::runner::StandardFileReplacementLayout::new(panel.bounds);
            std::fs::write(output.with_extension("json"), serde_json::to_vec_pretty(&serde_json::json!({
                "compositor": "shared Demo renderer", "prefer_powerpc": prefer_powerpc,
                "requested_depth": screen_depth, "scale": capture_scale,
                "actual_depth": session.runner().presented_screen_depth(),
                "confirming_replace": panel.confirming_replace, "name": panel.name,
                "message_bounds": layout.message, "panel_bounds": layout.bounds,
                "guest_tick": session.runner().guest_tick(),
                "scope": "Replacement prompt fixture; source native frame retained, no native Macintosh oracle qualification",
            })).unwrap()).unwrap();
        }
        let actual_depth = session.runner().presented_screen_depth();
        let mut source_pixels = frame.pixels;
        if lists_page {
            let retain_native_source = matches!(capture, CaptureCase::ListsSelectedNativeSource);
            // Remove only visible, qualified ownership from the source texture.
            // A composed capture must prove the shared renderer supplies these
            // pixels; application borders and declined cells stay untouched.
            let viewport = super::frames::Rect { top: 0, left: 0,
                bottom: frame.height as i32, right: frame.width as i32 };
            let mut erased_regions = Vec::new();
            for piece in super::frames::list_pieces(&lists, &controls, &windows, viewport) {
                let list = &lists[piece.list];
                for (&cell, _) in &list_text_plans[piece.list] {
                    let paint = &list.standard_cell_paint[&cell];
                    for &region in &paint.painted_regions {
                        let Some(region) = super::frames::Rect::from(region).intersection(piece.clip) else { continue; };
                        for y in region.top..region.bottom {
                            for x in region.left..region.right {
                                let offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
                                if !retain_native_source {
                                    source_pixels[offset..offset + 4].copy_from_slice(&[255, 0, 255, 255]);
                                }
                            }
                        }
                        erased_regions.push((piece.list, cell, (region.top, region.left, region.bottom, region.right)));
                    }
                }
            }
            assert!(!erased_regions.is_empty(), "list capture requires visible GPUI text ownership");
            std::fs::write(output.with_extension("json"), serde_json::to_vec_pretty(&serde_json::json!({
                "compositor": "shared Demo renderer", "source_mask": if retain_native_source { "retained native source; appearance only" } else { "magenta qualified visible regions" },
                "prefer_powerpc": prefer_powerpc, "requested_depth": screen_depth,
                "paint_depths": lists.iter().flat_map(|list| list.standard_cell_paint.values()
                    .map(|paint| paint.depth)).collect::<std::collections::BTreeSet<_>>(),
                "scale": capture_scale, "guest_tick": session.runner().guest_tick(),
                "erased_regions": if retain_native_source { None } else { Some(&erased_regions) },
                "owned_regions": &erased_regions,
                "list_state": lists.iter().map(|list| serde_json::json!({
                    "id": list.guest_id, "generation": list.generation, "active": list.active,
                    "visible": list.visible, "selected": list.selected,
                    "view_rect": list.view_rect, "cells": list.cells.iter().map(|(cell, bytes)|
                        serde_json::json!({"cell": cell, "bytes": bytes})).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "transition": match capture {
                    CaptureCase::ListsScrolled => "scrolled",
                    CaptureCase::ListsInactive => "inactive",
                    CaptureCase::ListsReactivated => "reactivated",
                    CaptureCase::ListsMutated => "mutated",
                    CaptureCase::ListsResized => "resized",
                    _ => "none",
                },
            })).unwrap()).unwrap();
        }
        let pixels = gpui_pixels(source_pixels);
        let frame_height = frame.height;
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
        let capture_size = capture_scale.map_or(size(px(900.), px(740.)), |scale| {
            size(px(frame.width as f32 * scale), px(frame_height as f32 * scale))
        });
        let window = visual
            .open_window(capture_size, |_, cx| {
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
                demo.guest_popup = guest_popup;
                demo.windows = windows;
                demo.dialogs = dialogs;
                demo.controls = controls;
                demo.lists = lists;
                demo.list_text_plans = list_text_plans;
                demo.text_edits = text_edits;
                demo.styled_text_plans = styled_text_plans;
                demo.standard_file = standard_file;
                if let Some((id, generation, from, to)) = held_drag {
                    demo.mouse_down = true;
                    demo.mouse_position = to;
                    demo.scrollbar_drag = Some((id, generation, from));
                }
                demo.width = frame.width;
                demo.height = frame_height;

                demo.menu_presented = menu_presented;
                demo.menu_height = menu_height;
                demo.status = format!(
                    "{} · Running",
                    if prefer_powerpc { "PowerPC" } else { "68k" }
                );
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::from_raw(frame.width, frame_height, pixels).unwrap(),
                )])));
                cx.notify();
            });
        });
        visual.run_until_parked();
        let composed = visual.capture_screenshot(window.into()).unwrap();
        let (scene_scale, scene_origin) = visual.update(|cx| {
            view.update(cx, |demo, _| (demo.display_scale, demo.display_origin))
        });
        std::fs::write(output.with_extension("capture.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "compositor": "shared Demo renderer", "case": format!("{capture:?}"),
                "prefer_powerpc": prefer_powerpc, "requested_depth": screen_depth,
                "actual_depth": actual_depth, "requested_scale": capture_scale,
                "scene_scale": scene_scale, "scene_origin": scene_origin,
                "guest_dimensions": [frame.width, frame_height],
                "viewport_dimensions": [f32::from(capture_size.width), f32::from(capture_size.height)],
                "composed_dimensions": [composed.width(), composed.height()],
                "typography": "resolved outline coverage for supported plain system text and qualified standard list cells; binary fallback for unsupported sources and styled recipes",
                "scope": "Capture provenance only; no automatic smooth visual, font fidelity or performance qualification",
            })).unwrap()).unwrap();
        composed.save(output).unwrap();
        eprintln!("saved composed GPUI capture to {}", output.display());
    }

    fn qualify_list_text_fields(
        lists: &[ListManagerSnapshot], native: &[u8], width: u32, height: u32,
    ) -> Vec<std::collections::BTreeMap<(i16, i16), super::text::ClassicListCellPaintPlan>> {
        lists.iter().map(|list| {
            let mut plans = std::collections::BTreeMap::new();
            if list.definition_id != 0 || !list.draw_enabled { return plans; }
            let Some(global) = list.global_view_rect else { return plans; };
            for (&cell, paint) in &list.standard_cell_paint {
                let offset_y = i32::from(global.0) - i32::from(list.view_rect.0);
                let offset_x = i32::from(global.1) - i32::from(list.view_rect.1);
                let bounds = [i32::from(paint.clip.0) + offset_y, i32::from(paint.clip.1) + offset_x,
                    i32::from(paint.clip.2) + offset_y, i32::from(paint.clip.3) + offset_x];
                let Some(bounds) = bounds.into_iter().map(|value| i16::try_from(value).ok()).collect::<Option<Vec<_>>>() else { continue; };
                if let Some(plan) = super::text::ClassicListCellPaintPlan::from_guest(paint,
                    (bounds[0], bounds[1], bounds[2], bounds[3]), native, width, height) {
                    plans.insert(cell, plan);
                }
            }
            plans
        }).collect()
    }

    // White is an explicit classic-field candidate, not inferred host theme
    // paint. Every field pixel must match before the recipe can be presented.
    fn qualify_styled_text_fields(
        records: &[TextEditSnapshot], native: &[u8], width: u32, height: u32,
    ) -> Vec<Option<super::text::StyledTextEditPaintPlan>> {
        records.iter().map(|record| {
            if !record.styled { return None; }
            let background = systemless::runner::TextEditInkSnapshot {
                pixel: if record.paint.as_ref()?.depth == 16 { 0x7fff } else { 0 },
                rgb: [255, 255, 255], inverted_rgb: [0, 0, 0],
            };
            let caret = (0..record.line_count).find_map(|index| styled_caret_paint(record, index));
            super::text::StyledTextEditPaintPlan::qualify(record, &background, caret, native, width, height)
        }).collect()
    }

    /// Classic PPC uses the insertion style; 68k uses retained native solid
    /// paint evidence, independently of the restored caller foreground.
    /// This is a classic-theme candidate; full native pixel qualification
    /// rejects a changed theme or unsupported caret paint.
    fn styled_caret_paint(
        record: &systemless::runner::TextEditSnapshot, index: usize,
    ) -> Option<((i16, i16, i16, i16), systemless::runner::TextEditInkSnapshot)> {
        if record.line_layout_policy == systemless::runner::TextEditLineLayoutPolicy::CumulativeGuestMetrics {
            if !record.active || !record.caret_visible || record.selection.0 != record.selection.1 { return None; }
            let paint = record.paint.as_ref()?.solid_caret.as_ref()?;
            let (owner, _) = record.caret_line()?;
            let width = paint.0.3.checked_sub(paint.0.1)?;
            return (owner == index && record.guest_styled_caret_rect(index, width)?? == paint.0)
                .then(|| paint.clone());
        }
        let rect = record.guest_styled_caret_rect(index, 1)??;
        let (owner, offset) = record.caret_line()?;
        if owner != index { return None; }
        let insertion = record.line_starts.as_ref()?.get(index)?.checked_add(offset)?;
        let run = record.style_runs.as_ref()?.iter().rposition(|run| run.start <= insertion)?;
        Some((rect, record.paint.as_ref()?.style_ink.get(run)?.clone()))
    }

    #[cfg(feature = "gpui-demo-test")]
    fn capture_styled_text_edit_ink(
        game: &std::path::Path, output: &std::path::Path,
        prefer_powerpc: bool, depth: Option<u16>, scale: f32, selected: bool, caret_offset: Option<usize>, activation: &[bool], caret_state: &str, multiline: bool,
    ) {
        use gpui_kit::{platform, HeadlessAppContext};
        let caret = caret_offset.is_some();
        let insertion = caret_offset.unwrap_or(26);
        let activation = if caret { match caret_state {
            "suspended" => &[false][..], "resumed" => &[false, true][..],
            "visible" | "blink-off" => &[][..], _ => panic!("unknown caret capture state"),
        } } else { activation };
        let mut session = MacintoshSession::new(true, if prefer_powerpc { Some(8) } else { depth });
        session.runner_mut().set_prefer_powerpc_executables(prefer_powerpc);
        if prefer_powerpc { session.runner_mut().set_powerpc_screen_depth(depth.unwrap_or(16)).unwrap(); }
        let app = session.load_path(game).unwrap();
        session.initialize(&app);
        assert!((0..300).any(|_| {
            session.runner_mut().run_steps(100_000, None);
            session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu| menu.id == 129 && !menu.items.is_empty())
        }));
        assert!(session.runner_mut().select_guest_menu_item(129, 11));
        let mut record = (0..300).find_map(|_| {
            session.runner_mut().run_steps(100_000, None);
            let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
            session.runner_mut().text_edit_snapshot().records.into_iter().find(|record|
                settled && record.drawing_intact && record.styled && record.style_runs.as_ref()
                    .is_some_and(|runs| runs.iter().any(|run| run.start == 26)))
        }).expect("settled showcase styled field");
        assert!(!record.active, "styled fixture starts inactive");
        if multiline {
            let before = record.clone();
            let dest = before.global_dest_rect.unwrap();
            let geometry = before.guest_styled_line_geometry(0).unwrap().0;
            let origin = (dest.0 - before.dest_rect.0, dest.1 - before.dest_rect.1);
            let horizontal = origin.1 + geometry.left + before.guest_styled_range_width(0..26).unwrap();
            let vertical = origin.0 + geometry.top + geometry.ascent;
            for input in [MacintoshInput::MouseDown { vertical, horizontal }, MacintoshInput::MouseUp { vertical, horizontal }] {
                session.deliver_input(input);
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
            }
            session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x24, character: b'\r' });
            session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x24, character: b'\r' });
            let split = (0..300).find_map(|_| {
                session.runner_mut().run_steps(10_000, None);
                let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                session.runner_mut().text_edit_snapshot().records.into_iter().find(|record|
                    settled && record.guest_id == before.guest_id && record.line_count == 2 && record.drawing_intact)
            }).expect("guest Return creates two mixed-metric styled lines");
            assert_eq!(split.text[26], b'\r');
            let first = split.guest_styled_line_geometry(0).unwrap().0;
            let second = split.guest_styled_line_geometry(1).unwrap().0;
            let start = (origin.0 + first.top + first.ascent, origin.1 + first.left);
            let line_start = split.line_starts.as_ref().unwrap()[1];
            let end = (origin.0 + second.top + second.ascent,
                origin.1 + second.left + split.guest_styled_range_width(line_start..split.text.len()).unwrap());
            for input in [MacintoshInput::MouseDown { vertical: start.0, horizontal: start.1 },
                MacintoshInput::MouseMove { vertical: end.0, horizontal: end.1 },
                MacintoshInput::MouseUp { vertical: end.0, horizontal: end.1 }] {
                session.deliver_input(input);
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
            }
            let selected = (0..300).find_map(|_| {
                session.runner_mut().run_steps(10_000, None);
                let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                session.runner_mut().text_edit_snapshot().records.into_iter().find(|next|
                    settled && next.guest_id == split.guest_id && next.active && next.drawing_intact
                        && next.selection == (0, split.text.len()))
            }).expect("guest drag selects both styled lines");
            if prefer_powerpc {
                let first = selected.guest_styled_selection_rect(0).unwrap().unwrap();
                let second = selected.guest_styled_selection_rect(1).unwrap().unwrap();
                assert!(first.0 < second.2 && second.0 < first.2, "overlapping PPC highlights");
            }
            record = selected;
        }
        if !multiline && (selected || caret) {
            assert!(insertion <= record.text.len(), "caret offset is a guest byte boundary");
            let before = record.clone();
            let dest = record.global_dest_rect.unwrap();
            let geometry = record.guest_styled_line_geometry(0).unwrap().0;
            let vertical = dest.0 - record.dest_rect.0 + geometry.top + geometry.ascent;
            let left = dest.1 - record.dest_rect.1 + geometry.left;
            let right = left + record.guest_styled_range_width(0..insertion).unwrap();
            let left = if caret { right } else { left };
            for input in [MacintoshInput::MouseDown { vertical, horizontal: left },
                MacintoshInput::MouseMove { vertical, horizontal: right },
                MacintoshInput::MouseUp { vertical, horizontal: right }] {
                session.deliver_input(input);
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
            }
            record = (0..300).find_map(|_| {
                session.runner_mut().run_steps(10_000, None);
                let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                session.runner_mut().text_edit_snapshot().records.into_iter().find(|next|
                    settled && next.guest_id == record.guest_id && next.active
                        && next.selection == (if caret { (insertion, insertion) } else { (0, 26) })
                        && (!caret || next.caret_visible && next.drawing_intact))
            }).expect("guest styled drag or click reaches the measured insertion boundary");
            assert_eq!(record.text, before.text, "pointer input preserves guest text");
            assert_eq!(record.style_runs, before.style_runs, "pointer input preserves style intent");
            assert_eq!(record.generation, before.generation, "pointer input preserves owner identity");
        }
        let original = record.clone();
        for &active in activation {
            session.request_foreground(active);
            record = (0..300).find_map(|_| {
                session.runner_mut().run_steps(10_000, None);
                let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                session.runner_mut().text_edit_snapshot().records.into_iter().find(|next|
                    settled && next.guest_id == original.guest_id && next.active == active)
            }).expect("guest finishes styled suspend/resume repaint");
            assert_eq!(record.selection, original.selection);
            assert_eq!(record.text, original.text);
            assert_eq!(record.style_runs, original.style_runs);
        }
        if caret && record.active {
            let visible = caret_state != "blink-off";
            record = (0..300).find_map(|_| {
                // Advance the guest clock and let the application's TEIdle
                // decide the phase. Never write caretState or paint host blink.
                if !visible { session.runner_mut().force_advance_guest_tick(); }
                session.runner_mut().run_steps(10_000, None);
                let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                session.runner_mut().text_edit_snapshot().records.into_iter().find(|next|
                    settled && next.guest_id == original.guest_id && next.active
                        && next.caret_visible == visible && next.drawing_intact)
            }).expect("guest completes requested caret blink phase and repaint");
        }
        assert_eq!(record.selection, original.selection);
        assert_eq!(record.text, original.text);
        assert_eq!(record.style_runs, original.style_runs);
        assert_eq!(record.generation, original.generation);
        let evidence = serde_json::json!({
            "caret_state": if caret { caret_state } else { "not-requested" },
            "insertion_offset": caret_offset, "multiline": multiline, "selection": record.selection,
            "compositor": "shared Demo renderer",
            "active": record.active, "caret_visible": record.caret_visible,
            "drawing_intact": record.drawing_intact, "generation": record.generation,
            "guest_tick": session.runner().guest_tick(), "view": record.global_view_rect,
            "scale": scale, "depth": record.paint.as_ref().map(|paint| paint.depth),
        });
        let mut frame = session.video_frame().unwrap();
        image::save_buffer(output.with_extension("guest.png"), &frame.pixels, frame.width, frame.height,
            image::ColorType::Rgba8).unwrap();
        let view = record.global_view_rect.unwrap();
        let background = systemless::runner::TextEditInkSnapshot {
            pixel: if record.paint.as_ref().unwrap().depth == 16 { 0x7fff } else { 0 },
            rgb: [255; 3], inverted_rgb: [0; 3],
        };
        let caret_paint = (0..record.line_count).find_map(|index| styled_caret_paint(&record, index));
        let plan = super::text::StyledTextEditPaintPlan::qualify(
            &record, &background, caret_paint, &frame.pixels, frame.width, frame.height,
        ).expect("whole-field native styled recipe, background and caret");
        for y in view.0..view.2 { for x in view.1..view.3 {
            let at = ((y as u32 * frame.width + x as u32) * 4) as usize;
            frame.pixels[at..at + 4].copy_from_slice(&[255; 4]);
        } }
        let mut visual = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets), platform::current_headless_renderer);
        visual.update(gpui_kit::init);
        let windows = session.runner_mut().window_frame_snapshot();
        let dialogs = session.runner_mut().dialog_snapshot();
        let controls = session.runner_mut().control_snapshot();
        let viewport = super::frames::Rect { top: 0, left: 0,
            bottom: frame.height as i32, right: frame.width as i32 };
        assert!(!super::frames::styled_text_edit_candidates(std::slice::from_ref(&record),
            &dialogs, &controls, &windows, viewport).is_empty(), "standard styled field visibility");
        let (sender, _receiver) = mpsc::channel();
        let window = visual.open_window(size(px(frame.width as f32 * scale), px(frame.height as f32 * scale)), |_, cx| {
            cx.new(|cx| {
                let mut demo = Demo::new(sender, Default::default(), cx);
                demo.width = frame.width;
                demo.height = frame.height;
                demo.menu_presented = session.runner().guest_menu_bar_presented();
                demo.menu_height = session.runner().bus().read_word(MBAR_HEIGHT);
                demo.menus = session.runner_mut().guest_menu_snapshot();
                demo.windows = windows;
                demo.dialogs = dialogs;
                demo.controls = controls;
                demo.text_edits = vec![record];
                demo.styled_text_plans = vec![Some(plan)];
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::from_raw(frame.width, frame.height, gpui_pixels(frame.pixels)).unwrap())])));
                demo
            })
        }).unwrap();
        visual.run_until_parked();
        visual.capture_screenshot(window.into()).unwrap().save(output).unwrap();
        std::fs::write(output.with_extension("json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        eprintln!("saved GPUI styled ink capture to {}", output.display());
    }

    pub(super) fn main() {
        run(Args::parse());
    }

    pub(super) fn launch(options: super::LaunchOptions) {
        let mut args = Args::parse_from([std::ffi::OsString::from("systemless"), options.game.clone().into_os_string()]);
        args.prefer_powerpc = options.prefer_powerpc;
        args.screen_depth = options.screen_depth;
        args.options = Some(options);
        run(args);
    }

    fn run(args: Args) {
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_custom_menu_fallback.as_ref() {
            capture_custom_menu_fallback(output);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_menu.as_ref() {
            capture_standard_menu(&args.game, output, args.prefer_powerpc, args.screen_depth, args.capture_standard_menu_id);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::Windows,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_moved.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsMoved,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_activated.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsActivated,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_grown.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsGrown,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_zoomed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsZoomed,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_zoom_restored.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsZoomRestored,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_custom_zoomed.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsCustomZoomed,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_custom_zoom_restored.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsCustomZoomRestored,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_promoted.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsPromoted,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_windows_main_promoted.as_ref() {
            capture_fixture_screen(
                &args.game,
                output,
                args.prefer_powerpc,
                args.screen_depth,
                CaptureCase::WindowsMainPromoted,
                args.capture_scale,
            );
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
                args.capture_scale,
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
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_selection.as_ref() {
            capture_fixture_screen(
                &args.game, output, args.prefer_powerpc, args.screen_depth,
                CaptureCase::ModalDialogSelection, args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_selection_inactive.as_ref() {
            capture_fixture_screen(
                &args.game, output, args.prefer_powerpc, args.screen_depth,
                CaptureCase::ModalDialogSelectionInactive, args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_caret_visible.as_ref() {
            capture_fixture_screen(
                &args.game, output, args.prefer_powerpc, args.screen_depth,
                CaptureCase::ModalDialogCaretVisible,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_caret_hidden.as_ref() {
            capture_fixture_screen(
                &args.game, output, args.prefer_powerpc, args.screen_depth,
                CaptureCase::ModalDialogCaretHidden,
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_checkbox_checked_held.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ModalDialogCheckboxCheckedHeld, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_checkbox_held.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ModalDialogCheckboxHeld, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_checkbox_outside.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ModalDialogCheckboxOutside, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_button_held.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ModalDialogButtonHeld, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_modal_dialog_button_outside.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ModalDialogButtonOutside, args.capture_scale);
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_radio_held.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::RadioHeld, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_radio_outside.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::RadioOutside, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_radio_selected.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::RadioSelected, args.capture_scale);
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
                args.capture_scale,
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
                args.capture_scale,
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
                if args.capture_lists_retain_native_source { CaptureCase::ListsSelectedNativeSource } else { CaptureCase::ListsSelected },
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_lists_held.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ListsHeld, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_lists_cancelled.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::ListsCancelled, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_lists_transition.as_ref() {
            let state = match args.capture_list_transition.as_str() {
                "scrolled" => CaptureCase::ListsScrolled,
                "inactive" => CaptureCase::ListsInactive,
                "reactivated" => CaptureCase::ListsReactivated,
                "mutated" => CaptureCase::ListsMutated,
                "resized" => CaptureCase::ListsResized,
                _ => unreachable!(),
            };
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, state, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_multiline.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), true, None, &[], "visible", true);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_selected_suspended.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), true, None, &[false], "visible", false);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_selected_resumed.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), true, None, &[false, true], "visible", false);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_selected.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), true, None, &[], "visible", false);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_caret.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), false, Some(args.capture_styled_caret_offset), &[], &args.capture_styled_caret_state, false);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_styled_text_edit_ink.as_ref() {
            capture_styled_text_edit_ink(&args.game, output, args.prefer_powerpc,
                args.screen_depth, args.capture_scale.unwrap_or(1.), false, None, &[], "visible", false);
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_host_suspended.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::TextEditHostSuspended, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_host_resumed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::TextEditHostResumed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_inactive.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::TextEditInactive, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_text_edit_reactivated.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::TextEditReactivated, args.capture_scale);
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
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls_scrolled.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::PopupControlsScrolled, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls_open.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::PopupControlsOpen, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls_disabled.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::PopupControlsDisabled, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_popup_controls_host_suspended.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::PopupControlsHostSuspended, args.capture_scale);
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
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
                args.capture_scale,
            );
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_save_caret_hidden_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc,
                args.screen_depth, CaptureCase::StandardFileSaveCaretHiddenComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_replace_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileReplaceComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_new_folder_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileNewFolderComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_new_folder_error_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileNewFolderErrorComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_new_folder_caret_hidden_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileNewFolderCaretHiddenComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_new_folder_long_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileNewFolderLongComposed, args.capture_scale);
            return;
        }
        #[cfg(feature = "gpui-demo-test")]
        if let Some(output) = args.capture_standard_file_new_folder_selected_composed.as_ref() {
            capture_fixture_screen(&args.game, output, args.prefer_powerpc, args.screen_depth, CaptureCase::StandardFileNewFolderSelectedComposed, args.capture_scale);
            return;
        }
        let fullscreen = args.options.as_ref().is_some_and(|options| options.fullscreen);
        let arrows_as_numpad = args.options.as_ref().is_some_and(|options| options.arrows_as_numpad);
        let display_scale = args.options.as_ref().and_then(|options| options.display_scale);
        let profile = systemless::machine_profile::reference_machine_profile();
        let window_size = (f32::from(profile.screen_width) * display_scale.unwrap_or(1) as f32,
            f32::from(profile.screen_height) * display_scale.unwrap_or(1) as f32);
        let (commands, receiver) = mpsc::channel();
        let updates = Arc::new(Mutex::new(None));
        let worker_updates = updates.clone();
        let mut worker = Some(std::thread::spawn(move || run_guest(args, receiver, worker_updates, true)));
        let shutdown_commands = commands.clone();
        gpui_kit::application()
            .with_assets(gpui_kit::assets::Assets)
            .run(move |cx| {
                gpui_kit::init(cx);
                cx.on_app_quit(move |_| {
                    let _ = shutdown_commands.send(Command::Shutdown);
                    if let Some(worker) = worker.take() {
                        if worker.join().is_err() {
                            eprintln!("[GPUI] Guest worker failed during shutdown");
                        }
                    }
                    async {}
                }).detach();
                cx.on_window_closed(|cx, _| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(if fullscreen {
                            WindowBounds::Fullscreen(Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx))
                        } else {
                            WindowBounds::Windowed(Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx))
                        }),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Systemless".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        if display_scale.is_some() && !fullscreen {
                            let dpi = window.scale_factor().max(1.);
                            window.resize(size(px(window_size.0 / dpi), px(window_size.1 / dpi)));
                        }
                        let view = cx.new(|cx| {
                            let mut view = Demo::new(commands, updates, cx);
                            view.arrows_as_numpad = arrows_as_numpad;
                            view
                        });
                        let focus = view.read(cx).focus.clone();
                        focus.focus(window, cx);
                        view
                    },
                )
                .expect("open GPUI window");
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![(90, 90, 150, 160)]),
                        close_box: true,
                        grow_icon_drawn: false,
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![(50, 50, 180, 240)]),
                        close_box: false,
                        grow_icon_drawn: false,
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
                        static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                        edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                        control_identity: None,
                        pressed: false,
                        number: 1,
                        kind: DialogItemKind::StaticText,
                        bounds: (105, 100, 135, 210),
                        text: "Modeless item".into(),
                        enabled: false,
                        visible: true,
                        value: None,
                        selection: None,
                        caret_visible: Some(true),
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
    fn capture_standard_menu(
        game: &std::path::Path,
        output: &std::path::Path,
        prefer_powerpc: bool,
        screen_depth: Option<u16>,
        menu_id: i16,
    ) {
        use gpui_kit::{platform, test::TestWindowExt, HeadlessAppContext};

        let mut session = MacintoshSession::new(true, screen_depth.or(Some(8)));
        session.runner_mut().set_prefer_powerpc_executables(prefer_powerpc);
        let app = session.load_path(game).unwrap();
        session.initialize(&app);
        let menus = (0..300)
            .find_map(|_| {
                session.runner_mut().run_steps(100_000, None);
                let menus = session.runner_mut().guest_menu_snapshot();
                menus.menus.iter().any(|menu| menu.id == menu_id && !menu.items.is_empty())
                    .then_some(menus)
            })
            .expect("showcase standard menu should become available");
        assert!(!menus.requires_guest_menu_rendering());
        let menu = menus.menus.iter().find(|menu| menu.id == menu_id).unwrap();
        let trigger = format!("guest-menu-{}-{}", menu.guest_id, menu.generation);
        let menu_presented = session.runner().guest_menu_bar_presented();
        let menu_height = session.runner().bus().read_word(MBAR_HEIGHT);
        let frame = session.video_frame().expect("showcase video frame");
        let pixels = gpui_pixels(frame.pixels);
        let (frame_width, frame_height) = (frame.width, frame.height);
        let windows = session.runner_mut().window_frame_snapshot();
        let dialogs = session.runner_mut().dialog_snapshot();
        let controls = session.runner_mut().control_snapshot();
        let lists = session.runner_mut().list_manager_snapshot();
        let text_edits = session.runner_mut().text_edit_snapshot().records;
        let standard_file = session.runner_mut().standard_file_snapshot();

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
        visual.update(|cx| {
            view.unwrap().update(cx, |demo, cx| {
                demo.menus = menus;
                demo.windows = windows;
                demo.dialogs = dialogs;
                demo.controls = controls;
                demo.lists = lists;
                demo.text_edits = text_edits;
                demo.standard_file = standard_file;
                demo.width = frame_width;
                demo.height = frame_height;

                demo.menu_presented = menu_presented;
                demo.menu_height = menu_height;
                demo.status = format!(
                    "{} · Running",
                    if prefer_powerpc { "PowerPC" } else { "68k" }
                );
                demo.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::from_raw(frame_width, frame_height, pixels).unwrap(),
                )])));
                cx.notify();
            });
        });
        visual.run_until_parked();
        visual.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click(trigger, cx);
        }).unwrap();
        visual.run_until_parked();
        visual.capture_screenshot(window.into()).unwrap().save(output).unwrap();
        eprintln!("saved composed GPUI menu capture to {}", output.display());
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
                    rectangular_regions: true,
                    visible_content_rects: Some(vec![(35, 5, 55, 55)]),
                    close_box: true,
                    grow_icon_drawn: false,
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
                mark: 0,
                style: 0,
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
        fn gpui_uses_realtime_guest_rate_on_both_cpus() {
            let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/toolbox-showcase/toolbox-showcase.sit");
            for powerpc in [false, true] {
                let mut session = MacintoshSession::new(true, Some(8));
                session
                    .runner_mut()
                    .set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&fixture).unwrap();
                session.initialize(&app);
                assert_eq!(session.runner().is_powerpc_app(), powerpc);
                let expected = super::default_realtime_instructions_per_tick(powerpc);
                assert_eq!(super::configure_realtime_execution(&mut session), expected);
                assert_eq!(session.runner().instructions_per_tick(), expected);
            }
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
            for powerpc in [false, true] {
            let updates = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            let worker_updates = updates.clone();
            let worker = std::thread::spawn(move || {
                run_guest(
                    Args {
                        game: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                        prefer_powerpc: powerpc,
                        screen_depth: Some(8),
                        options: None,
                        capture_about_alert: None,
                        capture_modal_dialog: None,
                        capture_modal_dialog_checked: None,
                        capture_modal_dialog_selection: None,
                        capture_modal_dialog_selection_inactive: None,
                        capture_modal_dialog_caret_visible: None,
                        capture_modal_dialog_caret_hidden: None,
                        capture_modal_dialog_button_held: None,
                        capture_modal_dialog_button_outside: None,
                        capture_modal_dialog_checkbox_held: None,
                        capture_modal_dialog_checkbox_checked_held: None,
                        capture_modal_dialog_checkbox_outside: None,
                        capture_modeless_dialog: None,
                        capture_nested_modal_dialog: None,
                        capture_controls: None,
                        capture_controls_changed: None,
                        capture_controls_dragged: None,
                        capture_controls_held: None,
                        capture_radio_held: None,
                        capture_radio_outside: None,
                        capture_radio_selected: None,
                        capture_lists: None,
                        capture_lists_selected: None,
                        capture_lists_retain_native_source: false,
                        capture_lists_held: None,
                        capture_lists_cancelled: None,
                        capture_lists_transition: None,
                        capture_list_transition: "scrolled".into(),
                        capture_scale: None,
                        capture_text_edit: None,
                        capture_styled_text_edit_ink: None,
                        capture_styled_text_edit_multiline: None,
                        capture_styled_text_edit_caret: None,
                        capture_styled_caret_offset: 26,
                        capture_styled_caret_state: "visible".into(),
                        capture_styled_text_edit_selected: None,
                        capture_styled_text_edit_selected_suspended: None,
                        capture_styled_text_edit_selected_resumed: None,
                        capture_text_edit_selected: None,
                        capture_text_edit_edited: None,
                        capture_text_edit_inactive: None,
                        capture_text_edit_reactivated: None,
                        capture_text_edit_host_suspended: None,
                        capture_text_edit_host_resumed: None,
                        capture_popup_controls: None,
                        capture_popup_controls_selected: None,
                        capture_popup_controls_host_suspended: None,
                        capture_popup_controls_disabled: None,
                        capture_popup_controls_open: None,
                        capture_popup_controls_scrolled: None,
                        capture_standard_file_save: None,
                        capture_standard_file_save_composed: None,
                        capture_standard_file_open_composed: None,
                        capture_standard_file_save_edited_composed: None,
                        capture_standard_file_save_caret_hidden_composed: None,
                        capture_standard_file_replace_composed: None,
                        capture_standard_file_new_folder_composed: None,
                        capture_standard_file_new_folder_error_composed: None,
                        capture_standard_file_new_folder_selected_composed: None,
                        capture_standard_file_new_folder_long_composed: None,
                        capture_standard_file_new_folder_caret_hidden_composed: None,
                        capture_custom_menu_fallback: None,
                        capture_standard_menu: None,
                        capture_standard_menu_id: 129,
                        capture_windows: None,
                        capture_windows_moved: None,
                        capture_windows_activated: None,
                        capture_windows_grown: None,
                        capture_windows_zoomed: None,
                        capture_windows_zoom_restored: None,
                        capture_windows_custom_zoomed: None,
                        capture_windows_custom_zoom_restored: None,
                        capture_windows_promoted: None,
                        capture_windows_main_promoted: None,
                        capture_modeless_dialog_layout: None,
                    },
                    rx,
                    worker_updates,
                    false,
                )
            });
            let initial = wait(&updates, |u| u.menus.menus.iter().any(|m| m.id == 129));
            let menu = initial.menus.menus.iter().find(|m| m.id == 129).unwrap();
            tx.send(Command::Menu(129, 2, menu.guest_id, menu.generation)).unwrap();
            let controls = wait(&updates, |u| u.controls.iter().any(|c| c.visible && c.proc_id == 16));
            let bar = controls.controls.iter().find(|c| c.visible && c.proc_id == 16).unwrap();
            tx.send(Command::Wheel(super::super::scroll::WheelRequest {
                target: super::super::scroll::ScrollTarget {
                    id: bar.guest_id, generation: bar.generation, vertical: false,
                },
                origin: (368, 300), steps: 1,
            })).unwrap();
            let changed = wait(&updates, |u| u.controls.iter().any(|c| c.guest_id == bar.guest_id && c.value > 0));
            let menu = changed.menus.menus.iter().find(|m| m.id == 129).unwrap();
            tx.send(Command::Menu(129, 9, menu.guest_id, menu.generation)).unwrap();
            let list = wait(&updates, |u| u.lists.iter().any(|list| list.active && list.draw_enabled));
            let bar = list.controls.iter().find(|c| c.visible && c.proc_id == 16
                && c.bounds.2 - c.bounds.0 > c.bounds.3 - c.bounds.1).unwrap();
            tx.send(Command::Wheel(super::super::scroll::WheelRequest {
                target: super::super::scroll::ScrollTarget {
                    id: bar.guest_id, generation: bar.generation, vertical: true,
                }, origin: (150, 100), steps: 1,
            })).unwrap();
            wait(&updates, |u| u.lists.iter().any(|list| list.active && list.visible.0 > 0));
            let held_point = (bar.bounds.2 - 8, bar.bounds.1 + 8);
            tx.send(Command::Input(MacintoshInput::MouseDown {
                vertical: held_point.0, horizontal: held_point.1,
            })).unwrap();
            wait(&updates, |u| u.lists.iter().any(|list| list.active && list.visible.0 == 4));
            tx.send(Command::Input(MacintoshInput::MouseUp {
                vertical: held_point.0, horizontal: held_point.1,
            })).unwrap();
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
            tx.send(Command::Shutdown).unwrap();
            drop(tx);
            worker.join().unwrap();
            }
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
            let title = before[0].title_layout(20).expect("recognized WDEF title");
            let bytes = before[0].window.title.chars()
                .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
                .collect::<Option<Vec<_>>>().unwrap();
            let glyphs = super::super::text::ClassicLine::plain(&bytes, 0, 12);
            assert_eq!(i32::from(title.width), *glyphs.positions.last().unwrap());
            assert!(title.baseline > title.clip.0 && title.baseline < title.clip.2);
            let mut inactive = before[0].clone();
            inactive.window.active = false;
            assert_eq!(inactive.title_layout(20), Some(title), "activation must not move glyphs");
            inactive.definition_id = Some(128);
            assert_eq!(inactive.title_layout(20), None, "custom WDEF owns its text");
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
                for on in [true, false] {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x39, character: 0 });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x39, character: 0 });
                    session.runner_mut().run_steps(100_000, None);
                    assert_eq!(key_is_down(&session, 0x39), on, "Caps Lock latch, powerpc={powerpc}");
                }
                for mac_key in [0x38, 0x3a, 0x3b] {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key, character: 0 });
                    session.runner_mut().run_steps(100_000, None);
                    assert!(key_is_down(&session, mac_key), "modifier={mac_key:x}, powerpc={powerpc}");
                    session.deliver_input(MacintoshInput::KeyUp { mac_key, character: 0 });
                    assert!(!key_is_down(&session, mac_key));
                }
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
        fn wheel_scroll_uses_guest_controls_on_all_cpu_modes() {
            use super::super::scroll::{arrow, ScrollTarget, WheelClick, WheelRequest};
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 2));
                wait_for_menu(&mut session, 129, 2, true);
                let bar = session.runner_mut().control_snapshot().into_iter()
                    .find(|c| c.visible && c.proc_id == 16).unwrap();
                let request = WheelRequest {
                    target: ScrollTarget { id: bar.guest_id, generation: bar.generation, vertical: false },
                    origin: (368, 300), steps: 1,
                };
                let viewport = super::super::frames::Rect { top: 0, left: 0, bottom: 600, right: 800 };
                let controls = session.runner_mut().control_snapshot();
                let menus = session.runner_mut().guest_menu_snapshot();
                let windows = session.runner_mut().window_frame_snapshot();
                assert_eq!(super::super::scroll::target(&controls, &menus, &windows, viewport,
                    request.origin, false), Some(request.target));
                let mut obscured = windows.clone();
                obscured[0].visible_content_rects = Some(Vec::new());
                assert!(super::super::scroll::target(&controls, &menus, &obscured, viewport,
                    request.origin, false).is_none());
                let mut disabled = controls.clone();
                disabled.iter_mut().find(|c| c.guest_id == bar.guest_id).unwrap().enabled = false;
                assert!(super::super::scroll::target(&disabled, &menus, &windows, viewport,
                    request.origin, false).is_none());
                let mut inactive = windows.clone();
                for window in &mut inactive { window.window.active = false; }
                assert!(super::super::scroll::target(&controls, &menus, &inactive, viewport,
                    request.origin, false).is_none());
                let mut stale = request;
                stale.target.generation += 1;
                assert!(arrow(&mut session, stale).is_none());
                assert!(arrow(&mut session, WheelRequest { steps: -1, ..request }).is_none());
                assert!(arrow(&mut session, request).is_some(), "powerpc={powerpc}");
                let click = WheelClick::begin(&mut session, request).unwrap();
                settle(&mut session);
                let click = click.advance(&mut session).unwrap();
                settle(&mut session);
                assert!(click.advance(&mut session).is_none());
                let changed = session.runner_mut().control_snapshot().into_iter()
                    .find(|c| c.guest_id == bar.guest_id).unwrap();
                assert!(changed.value > bar.value, "powerpc={powerpc}");
                assert_eq!(changed.generation, bar.generation);
                assert_eq!(session.runner().dispatcher().mouse_position(), request.origin);
                assert!(session.runner_mut().select_guest_menu_item(129, 1));
                wait_for_menu(&mut session, 129, 1, true);
                assert!(arrow(&mut session, request).is_none(), "hidden control");
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                let initial = session.runner_mut().list_manager_snapshot().remove(0);
                let bounds = initial.global_view_rect.unwrap();
                let origin = (bounds.0 + 20, bounds.1 + 20);
                let controls = session.runner_mut().control_snapshot();
                let menus = session.runner_mut().guest_menu_snapshot();
                let windows = session.runner_mut().window_frame_snapshot();
                let target = super::super::scroll::target(&controls, &menus, &windows, viewport,
                    origin, true).expect("list vertical scrollbar must be a standard control");
                let request = WheelRequest { target, origin, steps: 1 };
                let click = WheelClick::begin(&mut session, request).unwrap();
                settle(&mut session);
                let click = click.advance(&mut session).unwrap();
                settle(&mut session);
                assert!(click.advance(&mut session).is_none());
                let scrolled = session.runner_mut().list_manager_snapshot().remove(0);
                assert!(scrolled.visible.0 > initial.visible.0, "vertical wheel did not scroll list: powerpc={powerpc}");
                assert_eq!(scrolled.selected, initial.selected);
                assert_eq!(scrolled.generation, initial.generation);
                let down = super::super::scroll::arrow(&mut session, request).unwrap();
                session.deliver_input(MacintoshInput::MouseDown { vertical: down.0, horizontal: down.1 });
                settle(&mut session);
                assert!(session.runner().is_ui_tracking_active(), "LClick returned before release: {powerpc}");
                session.deliver_input(MacintoshInput::MouseMove { vertical: 550, horizontal: 760 });
                let outside = session.runner_mut().list_manager_snapshot()[0].visible;
                for _ in 0..4 { settle(&mut session); }
                assert_eq!(session.runner_mut().list_manager_snapshot()[0].visible, outside,
                    "arrow must not repeat outside its hit region");
                session.deliver_input(MacintoshInput::MouseMove { vertical: down.0, horizontal: down.1 });
                for _ in 0..12 { settle(&mut session); }
                let held = session.runner_mut().list_manager_snapshot().remove(0);
                assert_eq!(held.visible.0, 4, "held arrow must reach final full page: {powerpc}");
                assert_eq!(held.selected, initial.selected);
                session.deliver_input(MacintoshInput::MouseUp { vertical: down.0, horizontal: down.1 });
                settle(&mut session);
                assert!(!session.runner().is_ui_tracking_active(), "LClick did not return after release: {powerpc}");
                session.deliver_input(MacintoshInput::MouseDown { vertical: 304, horizontal: 496 });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp { vertical: 304, horizontal: 496 });
                settle(&mut session);
                // The compact view fits six whole rows plus a clipped row;
                // paging retains one row of overlap, including at the last page.
                for (point, expected) in [((160, 522), 0), ((208, 522), 6), ((160, 522), 0)] {
                    session.deliver_input(MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 });
                    for _ in 0..4 { settle(&mut session); }
                    assert!(session.runner().is_ui_tracking_active());
                    session.deliver_input(MacintoshInput::MouseUp { vertical: point.0, horizontal: point.1 });
                    settle(&mut session);
                    let page = session.runner_mut().list_manager_snapshot().remove(0);
                    assert_eq!(page.visible.0, expected, "list page click {point:?}, powerpc={powerpc}");
                    assert_eq!(page.visible.2, expected + 7, "visible extent must retain the clipped row: powerpc={powerpc}");
                    assert_eq!(page.selected, initial.selected);
                }
                let outline_pixels = |session: &mut MacintoshSession| {
                    let frame = session.video_frame().unwrap();
                    (210..226).flat_map(|y| {
                        let start = ((y * frame.width + 514) * 4) as usize;
                        frame.pixels[start..start + 16 * 4].to_vec()
                    }).collect::<Vec<_>>()
                };
                let content_pixels = |session: &mut MacintoshSession| {
                    let frame = session.video_frame().unwrap();
                    (128..242).flat_map(|y| {
                        let start = ((y * frame.width + 64) * 4) as usize;
                        frame.pixels[start..start + 450 * 4].to_vec()
                    }).collect::<Vec<_>>()
                };
                let clean_content = content_pixels(&mut session);
                let clean_track = outline_pixels(&mut session);
                session.deliver_input(MacintoshInput::MouseDown { vertical: 150, horizontal: 522 });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseMove { vertical: 218, horizontal: 522 });
                settle(&mut session);
                assert_eq!(content_pixels(&mut session), clean_content, "thumb feedback repainted list content: powerpc={powerpc}, depth={depth:?}");
                assert_ne!(outline_pixels(&mut session), clean_track, "missing guest thumb outline: powerpc={powerpc}, depth={depth:?}");
                session.deliver_input(MacintoshInput::MouseMove { vertical: 218, horizontal: 650 });
                settle(&mut session);
                assert_eq!(outline_pixels(&mut session), clean_track, "cancelled outline left stale pixels: powerpc={powerpc}, depth={depth:?}");
                session.deliver_input(MacintoshInput::MouseUp { vertical: 218, horizontal: 650 });
                settle(&mut session);
                // Native PPC LClick leaves content stationary while dragging,
                // commits on release, and cancels beyond the drag allowance.
                for (start, end, release_h, expected) in [(150, 218, 522, 6), (218, 150, 522, 0), (150, 218, 650, 0), (150, 300, 522, 0)] {
                    let before = session.runner_mut().list_manager_snapshot().remove(0);
                    session.deliver_input(MacintoshInput::MouseDown { vertical: start, horizontal: 522 });
                    settle(&mut session);
                    assert!(session.runner().is_ui_tracking_active());
                    session.deliver_input(MacintoshInput::MouseMove { vertical: end, horizontal: release_h });
                    for _ in 0..4 { settle(&mut session); }
                    assert_eq!(session.runner_mut().list_manager_snapshot().remove(0).visible, before.visible);
                    session.deliver_input(MacintoshInput::MouseUp { vertical: end, horizontal: release_h });
                    settle(&mut session);
                    assert!(!session.runner().is_ui_tracking_active());
                    let after = session.runner_mut().list_manager_snapshot().remove(0);
                    assert_eq!(after.visible.0, expected, "thumb release on powerpc={powerpc}");
                    assert_eq!(after.selected, initial.selected);
                }
            }
        }

        #[test]
        fn semantic_radio_activation_preserves_guest_tracking_across_cpus() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 5));
                wait_for_menu(&mut session, 129, 5, true);
                let controls = session.runner_mut().control_snapshot();
                let target = controls.iter().find(|c| c.visible && c.title == "Recruit (Easy)").unwrap();
                let (id, generation) = (target.guest_id, target.generation);
                let origin = session.runner().dispatcher().mouse_position();
                assert!(super::super::activation::ControlActivation::begin(&mut session, id, generation + 1).is_none());
                let click = super::super::activation::ControlActivation::begin(&mut session, id, generation).unwrap();
                settle(&mut session);
                let held = session.runner_mut().control_snapshot();
                let target = held.iter().find(|c| c.guest_id == id).unwrap();
                assert_eq!((target.value, target.hilite), (0, 11));
                assert!(super::super::activation::ControlActivation::begin(&mut session, id, generation).is_none());
                let click = click.advance(&mut session).unwrap();
                settle(&mut session);
                assert!(click.advance(&mut session).is_none());
                assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                let updated = session.runner_mut().control_snapshot();
                assert_eq!(updated.iter().find(|c| c.guest_id == id).unwrap().value, 1);
                assert_eq!(updated.iter().find(|c| c.visible && c.title == "Veteran (Normal)").unwrap().value, 0);
            }
        }

        #[test]
        fn radio_tracking_preserves_guest_group_until_release() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 5));
                wait_for_menu(&mut session, 129, 5, true);
                let controls = session.runner_mut().control_snapshot();
                let target = controls.iter().find(|c| c.visible && c.title == "Recruit (Easy)").unwrap();
                assert_eq!((target.proc_id, target.value), (2, 0));
                let id = target.guest_id;
                let rect = target.bounds;
                let inside = ((rect.0 + rect.2) / 2, (rect.1 + rect.3) / 2);
                let outside = (rect.0 - 10, rect.1 - 10);
                // HIG (1992), p. 205: track inside/outside; cancel an outside release.
                for cancel in [true, false] {
                    session.deliver_input(MacintoshInput::MouseDown { vertical: inside.0, horizontal: inside.1 });
                    settle(&mut session);
                    let held = session.runner_mut().control_snapshot();
                    let target = held.iter().find(|c| c.guest_id == id).unwrap();
                    assert_eq!((target.value, target.hilite), (0, 11), "held: powerpc={powerpc}, depth={depth:?}");
                    assert_eq!(held.iter().find(|c| c.visible && c.title == "Veteran (Normal)").unwrap().value, 1);
                    let release = if cancel { outside } else { inside };
                    session.deliver_input(MacintoshInput::MouseMove { vertical: release.0, horizontal: release.1 });
                    settle(&mut session);
                    assert_eq!(session.runner_mut().control_snapshot().iter().find(|c| c.guest_id == id).unwrap().hilite,
                        if cancel { 0 } else { 11 });
                    session.deliver_input(MacintoshInput::MouseUp { vertical: release.0, horizontal: release.1 });
                    settle(&mut session);
                    let released = session.runner_mut().control_snapshot();
                    let target = released.iter().find(|c| c.guest_id == id).unwrap();
                    assert_eq!((target.value, target.hilite), (if cancel { 0 } else { 1 }, 0));
                    assert_eq!(released.iter().find(|c| c.visible && c.title == "Veteran (Normal)").unwrap().value,
                        if cancel { 1 } else { 0 });
                }
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
                let held = session.runner_mut().control_snapshot();
                let held_checkbox = held.iter().find(|control| control.guest_id == checkbox_id).unwrap();
                assert_eq!((held_checkbox.value, held_checkbox.hilite), (0, 11));
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
                    assert_eq!(control.popup_text_inset, if powerpc { 5 } else { 15 });
                    let (top, left, bottom, right) = control.bounds;
                    let box_bounds = control.popup_box_bounds.expect("popup CDEF must expose its resolved box");
                    assert_eq!((box_bounds.0, box_bounds.1, box_bounds.2), (top + 1, left + width, bottom - 2));
                    assert!(box_bounds.3 <= right - 1);
                    if powerpc || (control.proc_id - 1008) & 1 != 0 {
                        assert_eq!(box_bounds.3, right - 1);
                    }
                    assert_eq!(control.popup_font.unwrap().point_size(), if id == 144 { 9 } else { 12 });
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
                    super::set_showcase_popups_enabled(&mut session, false);
                    let disabled = session.runner_mut().control_snapshot();
                    let control = disabled.iter().find(|c| c.popup_menu_id == Some(143)).unwrap();
                    assert!(!control.enabled);
                    assert_eq!(control.value, 4);
                    let ink = control.popup_ink.expect("disabled popup must expose guest ink");
                    if !powerpc && depth == Some(1) {
                        assert_eq!(ink, systemless::runner::ControlTextInk::Checker);
                    } else {
                        assert!(matches!(ink, systemless::runner::ControlTextInk::Solid(_)));
                    }
                    let (top, left, bottom, right) = control.popup_box_bounds.unwrap();
                    let (vertical, horizontal) = ((top + bottom) / 2, (left + right) / 2);
                    session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                    settle(&mut session);
                    assert!(session.runner_mut().guest_popup_snapshot().is_none(),
                        "disabled guest popup must reject tracking on mouse-down");
                    session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                    settle(&mut session);
                    assert_eq!(session.runner_mut().control_snapshot().iter()
                        .find(|c| c.popup_menu_id == Some(143)).unwrap().value, 4);
                    super::set_showcase_popups_enabled(&mut session, true);
                    super::select_showcase_resource_popup_long(&mut session);
                    assert_eq!(session.runner_mut().control_snapshot().iter()
                        .find(|c| c.popup_menu_id == Some(143)).unwrap().value, 4);
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[test]
        fn popup_scrolling_reaches_last_item_across_cpu_modes() {
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
                assert!(session.runner_mut().select_guest_menu_item(129, 16));
                wait_for_menu(&mut session, 129, 16, true);
                settle(&mut session);
                let point = super::scroll_showcase_theme_popup(&mut session);
                assert_eq!(
                    session
                        .runner_mut()
                        .control_snapshot()
                        .iter()
                        .find(|control| { control.visible && control.popup_menu_id == Some(144) })
                        .unwrap()
                        .value,
                    1,
                    "scrolling must not commit the highlighted value"
                );
                session.runner_mut().push_mouse_up(point.0, point.1);
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(50_000, None);
                    session
                        .runner_mut()
                        .control_snapshot()
                        .iter()
                        .any(|control| {
                            control.visible && control.popup_menu_id == Some(144) && control.value == 55
                        })
                }));
                assert!(session.runner_mut().guest_popup_snapshot().is_none());
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[test]
        fn popup_reverse_scrolling_and_arrow_release_preserve_value_across_cpu_modes() {
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
                settle(&mut session);
                super::scroll_showcase_theme_popup(&mut session);
                let runner = session.runner_mut();
                let bottom = runner.guest_popup_snapshot().unwrap();
                let up_point = (bottom.bounds.0 + 4, bottom.bounds.1 + 30);
                runner.set_mouse_position(up_point.0, up_point.1);
                let top = (0..100).find_map(|_| {
                    runner.run_steps(50_000, None);
                    runner.guest_popup_snapshot()
                        .filter(|popup| !popup.scroll_indicators().0)
                }).expect("held up arrow must return to the start of the menu");
                assert_eq!(top.menu.guest_id, bottom.menu.guest_id);
                assert_eq!(top.menu.generation, bottom.menu.generation);
                assert_eq!(top.bounds, bottom.bounds);
                assert!(top.content_top > bottom.content_top);
                assert_eq!(top.scroll_indicators(), (false, true));

                // Stop on a scrolling indicator, not a selectable row. The
                // standard MDEF owns scrolling; it must not commit a value.
                // Macintosh Toolbox Essentials (1992), Menu Manager, "Menus".
                let down_point = (top.bounds.2 - 4, top.bounds.1 + 30);
                runner.set_mouse_position(down_point.0, down_point.1);
                runner.push_mouse_up(down_point.0, down_point.1);
                assert!((0..100).any(|_| {
                    runner.run_steps(50_000, None);
                    runner.guest_popup_snapshot().is_none()
                }), "release on the scroll indicator must close tracking");
                for _ in 0..20 {
                    runner.run_steps(50_000, None);
                }
                assert_eq!(runner.control_snapshot().iter().find(|control| {
                    control.visible && control.popup_menu_id == Some(144)
                }).unwrap().value, 1, "arrow release must preserve the original selection");

                // A subsequent interaction must be usable after cancellation.
                let point = super::scroll_showcase_theme_popup(&mut session);
                session.runner_mut().push_mouse_up(point.0, point.1);
                assert!((0..100).any(|_| {
                    let runner = session.runner_mut();
                    runner.run_steps(50_000, None);
                    runner.control_snapshot().iter().any(|control| {
                        control.visible && control.popup_menu_id == Some(144) && control.value == 55
                    })
                }));
                assert!(session.runner_mut().guest_popup_snapshot().is_none());
            }
        }

        #[test]
        fn popup_cancellation_preserves_guest_value_across_cpu_modes() {
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
                assert!(session.runner_mut().select_guest_menu_item(129, 16));
                wait_for_menu(&mut session, 129, 16, true);
                settle(&mut session);
                let runner = session.runner_mut();
                let (top, left, _, _) = runner.window_bounds();
                for target in ["outside", "disabled", "separator"] {
                    runner.push_mouse_down(top + 112, left + 280);
                    let opened = (0..100)
                        .find_map(|_| {
                            runner.run_steps(50_000, None);
                            runner.guest_popup_snapshot()
                        })
                        .expect("popup should open after a cancelled selection");
                    let point = if target == "outside" {
                        (opened.bounds.2 + 10, opened.bounds.3 + 10)
                    } else {
                        let index = opened
                            .menu
                            .items
                            .iter()
                            .position(|item| {
                                if target == "disabled" {
                                    !item.enabled && !item.separator
                                } else {
                                    item.separator
                                }
                            })
                            .expect("fixture must contain the target row");
                        let y = opened.content_top
                            + opened.row_heights[..index].iter().sum::<i16>()
                            + opened.row_heights[index] / 2;
                        (y, opened.bounds.1 + 30)
                    };
                    runner.set_mouse_position(point.0, point.1);
                    for _ in 0..20 {
                        runner.run_steps(50_000, None);
                    }
                    let tracking = runner
                        .guest_popup_snapshot()
                        .expect("held popup remains open");
                    assert_eq!(
                        tracking.highlighted_item, 0,
                        "{powerpc:?}/{depth:?}/{target}"
                    );
                    assert_eq!(tracking.menu.guest_id, opened.menu.guest_id);
                    assert_eq!(tracking.menu.generation, opened.menu.generation);
                    runner.push_mouse_up(point.0, point.1);
                    assert!(
                        (0..100).any(|_| {
                            runner.run_steps(50_000, None);
                            runner.guest_popup_snapshot().is_none()
                        }),
                        "cancelled popup must close: {powerpc:?}/{depth:?}/{target}"
                    );
                    for _ in 0..20 {
                        runner.run_steps(50_000, None);
                    }
                    let control = runner
                        .control_snapshot()
                        .into_iter()
                        .find(|control| control.visible && control.popup_menu_id == Some(143))
                        .unwrap();
                    assert_eq!(
                        control.value, 1,
                        "cancellation must preserve value: {powerpc:?}/{depth:?}/{target}"
                    );
                    let menus = runner.guest_menu_snapshot();
                    let menu = menus.menus.iter().find(|menu| menu.id == 143).unwrap();
                    assert_eq!(
                        menu.items
                            .iter()
                            .filter(|item| item.checked)
                            .map(|item| item.number)
                            .collect::<Vec<_>>(),
                        vec![1]
                    );
                }
            }
        }

        #[test]
        fn standard_file_snapshots_follow_modal_guest_state_on_both_cpus() {
            use systemless::runner::StandardFileKind;

            for (powerpc, depth, semantic) in [(false, Some(1)), (false, Some(8)), (true, None)].into_iter()
                .flat_map(|(cpu, depth)| [false, true].map(move |semantic| (cpu, depth, semantic))) {
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
                assert_eq!(opened.list_name_limit, if powerpc { None } else { Some(36) });
                assert_eq!(opened.list_text_origin, if powerpc { (3, 13) } else { (4, 11) });
                assert_eq!(opened.directory_marker, if powerpc { ">" } else { "▸" });
                assert!(opened.standard_entry_point);
                assert_ne!(opened.guest_id, 0);
                assert_ne!(opened.generation, 0);
                assert!(opened.bounds.2 > opened.bounds.0 && opened.bounds.3 > opened.bounds.1);
                assert!(opened.entries.as_ref().is_some_and(|entries| !entries.is_empty()));
                assert!(opened.directory_label.as_ref().is_some_and(|label| !label.is_empty()));
                let layout = opened.get_layout.as_ref().expect("standard Open geometry");
                if powerpc {
                    assert_eq!(opened.directory_font, (0, 0, 0));
                    assert_eq!(opened.directory_text_layout, (0, 12, 16));
                } else {
                    let metrics = systemless::quickdraw::text::get_font_metrics(opened.directory_font.0, opened.directory_font.1);
                    assert_eq!(opened.directory_text_layout.0, 1);
                    assert_eq!(opened.directory_text_layout.1, metrics.ascent.min(layout.directory_label.2 - layout.directory_label.0 - 1));
                    assert_eq!(opened.directory_text_layout.2, metrics.ascent + metrics.descent + metrics.leading);
                }
                let (volume_text, origin) = opened.volume_text.as_ref().expect("guest volume typography");
                if powerpc {
                    assert_eq!(volume_text, "Maci...");
                    assert_eq!(*origin, (0, 12));
                } else {
                    assert_eq!(origin.0, 15);
                    let line = super::super::text::ClassicLine::unicode(volume_text, 0, 12);
                    assert!(line.positions.last().copied().unwrap_or(0) <= i32::from(layout.volume.3 - layout.volume.1 - 34));
                    assert!(volume_text.ends_with("..."));
                }
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
                if semantic {
                    use super::super::activation::{ControlActivation, FileAction};
                    let origin = session.runner().dispatcher().mouse_position();
                    assert!(ControlActivation::begin_file(&mut session, opened.guest_id, opened.generation + 1, FileAction::Cancel).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                    let click = ControlActivation::begin_file(&mut session, opened.guest_id, opened.generation, FileAction::Cancel).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    let click = click.advance(&mut session).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    assert!(click.advance(&mut session).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                } else {
                    session.deliver_input(MacintoshInput::KeyDown {
                        mac_key: 0x35,
                        character: 27,
                    });
                    session.deliver_input(MacintoshInput::KeyUp {
                        mac_key: 0x35,
                        character: 27,
                    });
                }
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
                // A queued action from the dismissed Open panel must not affect Save.
                let origin = session.runner().dispatcher().mouse_position();
                assert!(super::super::activation::ControlActivation::begin_file(
                    &mut session, opened.guest_id, opened.generation,
                    super::super::activation::FileAction::Cancel,
                ).is_none());
                assert_eq!(session.runner().dispatcher().mouse_position(), origin);
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
                if semantic {
                    use super::super::activation::{ControlActivation, FileAction};
                    let origin = session.runner().dispatcher().mouse_position();
                    assert!(ControlActivation::begin_file(&mut session, saving.guest_id, saving.generation + 1, FileAction::Cancel).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                    let click = ControlActivation::begin_file(&mut session, saving.guest_id, saving.generation, FileAction::Cancel).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    let click = click.advance(&mut session).unwrap();
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    assert!(click.advance(&mut session).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                } else {
                    session.deliver_input(MacintoshInput::KeyDown {
                        mac_key: 0x35,
                        character: 27,
                    });
                    session.deliver_input(MacintoshInput::KeyUp {
                        mac_key: 0x35,
                        character: 27,
                    });
                }
                assert!((0..100).any(|_| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    session.runner_mut().standard_file_snapshot().is_none()
                }));
                let origin = session.runner().dispatcher().mouse_position();
                assert!(super::super::activation::ControlActivation::begin_file(
                    &mut session, saving.guest_id, saving.generation,
                    super::super::activation::FileAction::Cancel,
                ).is_none());
                assert_eq!(session.runner().dispatcher().mouse_position(), origin);
            }
        }

        #[test]
        fn semantic_new_folder_create_cancel_and_stale_actions_across_modes() {
            use super::super::activation::{ControlActivation, FileAction};
            use systemless::memory::{globals::addr::CARET_TIME, MemoryBus};
            for (powerpc, depth, duplicate_directory) in [(false, Some(1)), (false, Some(8)), (true, None)].into_iter().flat_map(|(cpu, depth)| [false, true].map(move |directory| (cpu, depth, directory))) {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 12));
                wait_for_menu(&mut session, 129, 12, true);
                settle(&mut session);
                let step = |session: &mut MacintoshSession| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                };
                let activate = |session: &mut MacintoshSession, action| {
                    let panel = session.runner().standard_file_snapshot().unwrap();
                    let origin = session.runner().dispatcher().mouse_position();
                    let click = ControlActivation::begin_file(session, panel.guest_id, panel.generation, action).unwrap();
                    step(session);
                    let click = click.advance(session).unwrap();
                    step(session);
                    assert!(click.advance(session).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                };
                session.deliver_input(MacintoshInput::MouseDown { vertical: 266, horizontal: 400 });
                session.deliver_input(MacintoshInput::MouseUp { vertical: 266, horizontal: 400 });
                let original = (0..100).find_map(|_| { step(&mut session); session.runner().standard_file_snapshot() }).unwrap();
                activate(&mut session, FileAction::NewFolder);
                let child = session.runner().standard_file_snapshot().unwrap();
                assert_eq!(child.new_folder.as_ref().unwrap().selection, (0, 15));
                let folder = child.new_folder.as_ref().unwrap();
                assert_eq!(folder.insertion_positions.len(), 16);
                let vertical = (folder.layout.name.0 + folder.layout.name.2) / 2;
                for offset in [0, 1, 7, 15] {
                    let horizontal = folder.insertion_positions[offset];
                    session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                    step(&mut session);
                    session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection,
                        (offset, offset), "snapshot insertion position: PPC={powerpc}, depth={depth:?}");
                }
                for (mac_key, character, selection) in [(0x7e, 0x1e, (0, 0)), (0x7d, 0x1f, (15, 15))] {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection, selection);
                }
                let field = child.new_folder.as_ref().unwrap().layout.name;
                let v = (field.0 + field.2) / 2;
                let end = field.3 - 2;
                let start = field.1 - 12;
                session.deliver_input(MacintoshInput::MouseDown { vertical: v, horizontal: end });
                step(&mut session);
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection, (15, 15), "end mouse-down: PPC={powerpc}, depth={depth:?}");
                session.deliver_input(MacintoshInput::MouseUp { vertical: v, horizontal: end });
                step(&mut session);
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection, (15, 15), "end release: PPC={powerpc}, depth={depth:?}");
                let caret_tick = session.runner().guest_tick();
                for (interval, elapsed, visible) in [(32, 31, true), (32, 32, false),
                    (32, 64, true), (64, 127, true), (64, 128, false), (5, 132, false), (5, 133, true)] {
                    // Simulate the guest's General Controls preference changing
                    // while this same edit field remains active.
                    session.runner_mut().bus_mut().write_long(CARET_TIME, interval);
                    // A GUI deadline caps time; PPC returns at each VBL boundary.
                    // Exercise every guest tick rather than treating one slice as
                    // an instruction to jump directly to the checkpoint.
                    for _ in 0..100 {
                        if session.runner().guest_tick() >= caret_tick.saturating_add(elapsed) { break; }
                        step(&mut session);
                    }
                    assert_eq!(session.runner().guest_tick(), caret_tick.saturating_add(elapsed),
                        "caret checkpoint clock: PPC={powerpc}, depth={depth:?}, elapsed={elapsed}");
                    let folder = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                    assert_eq!(folder.caret_visible, visible,
                        "New Folder idle caret: PPC={powerpc}, depth={depth:?}, elapsed={elapsed}");
                }
                session.runner_mut().bus_mut().write_long(CARET_TIME, 32);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: b'x' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: b'x' });
                step(&mut session);
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().name, "untitled folderx");
                session.deliver_input(MacintoshInput::MouseDown { vertical: v, horizontal: end });
                step(&mut session);
                session.deliver_input(MacintoshInput::MouseMove { vertical: v, horizontal: start });
                step(&mut session);
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection, (0, 16));
                session.deliver_input(MacintoshInput::MouseUp { vertical: v, horizontal: start });
                step(&mut session);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: b'a' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: b'a' });
                step(&mut session);
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().name, "a");
                assert!(ControlActivation::begin_file(&mut session, original.guest_id, original.generation, FileAction::Accept).is_none());
                assert!(ControlActivation::begin_file(&mut session, child.guest_id, child.generation, FileAction::Cancel).is_none());
                activate(&mut session, FileAction::CancelNewFolder);
                let parent = session.runner().standard_file_snapshot().unwrap();
                assert!(parent.new_folder.is_none());
                assert_eq!(parent.name, original.name);
                assert!(ControlActivation::begin_file(&mut session, child.guest_id, child.generation, FileAction::CreateFolder).is_none());
                activate(&mut session, FileAction::NewFolder);
                let duplicate = original.entries.as_ref().unwrap().iter()
                    .find(|entry| entry.is_directory == duplicate_directory).unwrap().name.clone();
                for character in duplicate.bytes() {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                    step(&mut session);
                }
                let before_error = session.runner().standard_file_snapshot().unwrap();
                activate(&mut session, FileAction::CreateFolder);
                let failed = session.runner().standard_file_snapshot().unwrap();
                assert_eq!(failed.directory_id, original.directory_id);
                assert_eq!(failed.entries, original.entries);
                assert_eq!(failed.new_folder.as_ref().unwrap().error, Some(-48));
                assert_eq!(failed.new_folder.as_ref().unwrap().name, duplicate);
                assert_eq!(failed.name, original.name);
                assert_ne!(failed.generation, before_error.generation);
                assert!(ControlActivation::begin_file(&mut session, before_error.guest_id, before_error.generation, FileAction::CreateFolder).is_none());
                assert!(ControlActivation::begin_file(&mut session, failed.guest_id, failed.generation, FileAction::CreateFolder).is_none());
                activate(&mut session, FileAction::DismissFolderError);
                let restored = session.runner().standard_file_snapshot().unwrap();
                assert!(restored.new_folder.is_none());
                assert_eq!(restored.name, original.name);
                activate(&mut session, FileAction::NewFolder);
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x33, character: 8 });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x33, character: 8 });
                step(&mut session);
                let empty = session.runner().standard_file_snapshot().unwrap();
                assert!(empty.new_folder.as_ref().unwrap().name.is_empty());
                assert!(ControlActivation::begin_file(&mut session, empty.guest_id, empty.generation, FileAction::CreateFolder).is_none());
                for character in b"gpui folder" {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: *character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: *character });
                    step(&mut session);
                }
                assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().name, "gpui folder");
                activate(&mut session, FileAction::CreateFolder);
                let parent = session.runner().standard_file_snapshot().unwrap();
                assert!(parent.new_folder.is_none());
                assert_ne!(parent.directory_id, original.directory_id);
                assert_eq!(parent.name, original.name);
                assert_eq!(parent.name_has_focus, Some(true));
                assert!(parent.entries.unwrap().is_empty());
                activate(&mut session, FileAction::Cancel);
                assert!((0..100).any(|_| { step(&mut session); session.runner().standard_file_snapshot().is_none() }));
            }
        }

        #[test]
        fn semantic_standard_file_save_returns_guest_reply_across_modes() {
            use super::super::activation::{ControlActivation, FileAction};
            use systemless::systems::macintosh::debug::{handle_debug_request, DebugReply, DebugRequest, M68K_SPACE, PPC_SPACE};

            for (powerpc, depth, replacing, keyboard) in [(false, Some(1)), (false, Some(8)), (true, None)].into_iter()
                .flat_map(|(cpu, depth)| [(false, 0), (true, 0), (true, 1), (true, 2)].map(move |(replacing, keyboard)| (cpu, depth, replacing, keyboard))) {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 12));
                wait_for_menu(&mut session, 129, 12, true);
                settle(&mut session);
                let step = |session: &mut MacintoshSession| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                };
                step(&mut session);
                session.deliver_input(MacintoshInput::MouseDown { vertical: 266, horizontal: 400 });
                session.deliver_input(MacintoshInput::MouseUp { vertical: 266, horizontal: 400 });
                let panel = (0..100).find_map(|_| {
                    step(&mut session);
                    session.runner().standard_file_snapshot()
                }).expect("Save panel");
                let name = if replacing {
                    panel.entries.as_ref().unwrap().iter().find(|entry| !entry.is_directory)
                        .expect("existing file in Save directory").name.clone()
                } else { "é£S".to_string() };
                let name_bytes: Vec<_> = name.chars().map(|ch| systemless::systems::macintosh::mac_roman::encode_mac_roman_char(ch).unwrap()).collect();
                for character in name_bytes.iter().copied() {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                    step(&mut session);
                }
                assert!((0..100).any(|_| {
                    step(&mut session);
                    session.runner().standard_file_snapshot().is_some_and(|p| p.name.as_deref() == Some(name.as_str()))
                }));
                if !replacing {
                    for _ in 0..2 {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x33, character: 8 });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x33, character: 8 });
                        step(&mut session);
                    }
                    let edited = session.runner().standard_file_snapshot().unwrap();
                    assert_eq!(edited.name.as_deref(), Some("é"));
                    assert_eq!(edited.name_selection, Some((1, 1)), "selection offsets are Mac Roman bytes");
                    let mut saw_on = false;
                    let mut saw_off = false;
                    for _ in 0..90 {
                        step(&mut session);
                        let blinking = session.runner().standard_file_snapshot().unwrap();
                        assert_eq!(blinking.name.as_deref(), Some("é"));
                        assert_eq!(blinking.name_selection, Some((1, 1)));
                        saw_on |= blinking.name_caret_visible == Some(true);
                        saw_off |= blinking.name_caret_visible == Some(false);
                    }
                    assert!(saw_on && saw_off, "guest CaretTime must blink the Save insertion point, powerpc={powerpc}");

                    for character in [0xa3, b'S'] {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                        step(&mut session);
                    }
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name.as_deref(), Some(name.as_str()));
                }
                if !replacing {
                    let current = session.runner().standard_file_snapshot().unwrap();
                    let rect = current.put_layout.as_ref().unwrap().name;
                    let (font, size, _) = current.directory_font;
                    let glyphs = super::super::text::ClassicLine::plain(&name_bytes, font, size);
                    for (offset, advance) in glyphs.positions.iter().enumerate() {
                        let horizontal = rect.1 + if powerpc { 0 } else { 1 } + *advance as i16;
                        session.deliver_input(MacintoshInput::MouseDown { vertical: rect.0 + 10, horizontal });
                        session.deliver_input(MacintoshInput::MouseUp { vertical: rect.0 + 10, horizontal });
                        step(&mut session);
                        step(&mut session);
                        assert_eq!(session.runner().standard_file_snapshot().unwrap().name_selection,
                            Some((offset, offset)), "Save glyph insertion boundary, powerpc={powerpc}");
                    }
                }
                if !replacing {
                    let current = session.runner().standard_file_snapshot().unwrap();
                    let rect = current.put_layout.as_ref().unwrap().name;
                    let (font, size, _) = current.directory_font;
                    let glyphs = super::super::text::ClassicLine::plain(&name_bytes, font, size);
                    let h = |offset: usize| rect.1 + if powerpc { 0 } else { 1 } + glyphs.positions[offset] as i16;
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x38, character: 0 });
                    session.deliver_input(MacintoshInput::MouseDown { vertical: rect.0 + 10, horizontal: h(1) });
                    session.deliver_input(MacintoshInput::MouseUp { vertical: rect.0 + 10, horizontal: h(1) });
                    for _ in 0..4 { step(&mut session); }
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_selection, Some((1, 3)), "Shift extends guest Save selection");
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_caret_visible, Some(false));
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x38, character: 0 });
                    step(&mut session);
                    session.deliver_input(MacintoshInput::MouseDown { vertical: rect.0 + 10, horizontal: h(1) });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_caret_visible, Some(false), "held insertion capture suppresses the caret");
                    session.deliver_input(MacintoshInput::MouseMove { vertical: rect.0 + 10, horizontal: h(3) });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_selection, Some((1, 3)), "held selection follows moving glyph boundary");
                    session.deliver_input(MacintoshInput::MouseMove { vertical: rect.0 - 20, horizontal: rect.1 - 20 });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_selection, Some((0, 1)), "capture survives leaving the Save field");
                    session.deliver_input(MacintoshInput::MouseUp { vertical: rect.0 - 20, horizontal: rect.1 - 20 });
                    step(&mut session);
                    session.deliver_input(MacintoshInput::MouseMove { vertical: rect.0 + 10, horizontal: h(3) });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name_selection, Some((0, 1)), "release ends selection capture");
                }
                let origin = session.runner().dispatcher().mouse_position();
                let click = ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, FileAction::Accept).unwrap();
                step(&mut session);
                let click = click.advance(&mut session).unwrap();
                step(&mut session);
                assert!(click.advance(&mut session).is_none());
                assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                if replacing {
                    assert!(session.runner().standard_file_snapshot().unwrap().confirming_replace);
                    assert!(ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, FileAction::Accept).is_none());
                    let confirmation = session.runner().standard_file_snapshot().unwrap();
                    assert_ne!(confirmation.generation, panel.generation);
                    assert_eq!(confirmation.name_has_focus, Some(false));
                    // Ordinary typing must not leak through the subsidiary modal loop.
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: b'X' });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: b'X' });
                    step(&mut session);
                    assert_eq!(session.runner().standard_file_snapshot().unwrap().name, confirmation.name);
                    if keyboard != 0 {
                        // Files (1992), p. 3-7: Escape/Command-period cancel; Return/Enter invoke the default.
                        if keyboard == 2 {
                            session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x37, character: 0 });
                        }
                        let (mac_key, character) = if keyboard == 1 { (0x35, 27) } else { (0x2f, b'.') };
                        session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
                        if keyboard == 2 {
                            session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x37, character: 0 });
                        }
                        step(&mut session);
                        step(&mut session);
                    } else {
                        let click = ControlActivation::begin_file(&mut session, confirmation.guest_id, confirmation.generation, FileAction::CancelReplacement).unwrap();
                        step(&mut session);
                        let click = click.advance(&mut session).unwrap();
                        step(&mut session);
                        assert!(click.advance(&mut session).is_none());
                    }
                    let parent = session.runner().standard_file_snapshot().unwrap();
                    assert!(!parent.confirming_replace);
                    assert_eq!(parent.name_has_focus, Some(true));
                    assert_eq!(parent.name.as_deref(), Some(name.as_str()));
                    assert_eq!(parent.name_selection, Some((0, name.len())));
                    assert!(ControlActivation::begin_file(&mut session, confirmation.guest_id, confirmation.generation, FileAction::CancelReplacement).is_none());
                    for action in [FileAction::Accept, FileAction::Replace] {
                        if keyboard != 0 && matches!(action, FileAction::Replace) {
                            let (mac_key, character) = if keyboard == 1 { (0x24, 13) } else { (0x4c, 3) };
                            session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
                            session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
                            step(&mut session);
                            step(&mut session);
                            let parent = session.runner().standard_file_snapshot().unwrap();
                            assert!(!parent.confirming_replace, "Return/Enter must invoke Cancel");
                            assert_eq!(parent.name_selection, Some((0, name.len())));
                            let click = ControlActivation::begin_file(&mut session, parent.guest_id, parent.generation, FileAction::Accept).unwrap();
                            step(&mut session);
                            let click = click.advance(&mut session).unwrap();
                            step(&mut session);
                            assert!(click.advance(&mut session).is_none());
                        }
                        let current = session.runner().standard_file_snapshot().unwrap();
                        let click = ControlActivation::begin_file(&mut session, current.guest_id, current.generation, action).unwrap();
                        step(&mut session);
                        let click = click.advance(&mut session).unwrap();
                        step(&mut session);
                        assert!(click.advance(&mut session).is_none());
                    }
                }
                assert!((0..100).any(|_| {
                    step(&mut session);
                    session.runner().standard_file_snapshot().is_none()
                }));
                let DebugReply::Memory(reply) = handle_debug_request(session.runner_mut(), DebugRequest::ReadMemory {
                    space: if powerpc { PPC_SPACE } else { M68K_SPACE },
                    address: u64::from(panel.guest_id), length: 88,
                }).unwrap() else { panic!("StandardFileReply memory"); };
                // StandardFileReply layout: Files (1992), p. 3-69; FSSpec name
                // follows its volume reference and directory ID at reply + 12.
                assert_eq!(reply.bytes.len(), 88);
                assert_eq!(reply.bytes[0], 1, "sfGood, PPC={powerpc}, depth={depth:?}");
                assert_eq!(reply.bytes[1], u8::from(replacing), "replacement result");
                assert_eq!(usize::from(reply.bytes[12]), name_bytes.len());
                assert_eq!(&reply.bytes[13..13 + name_bytes.len()], name_bytes.as_slice(), "guest-edited FSSpec name");
            }
        }

        #[test]
        fn styled_text_edit_snapshots_preserve_guest_runs_across_cpu_modes() {
            for (powerpc, depth, ppc_depth) in [(false, Some(1), None), (false, Some(8), None), (true, None, None), (true, None, Some(8))] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if let Some(depth) = ppc_depth { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 11));
                wait_for_menu(&mut session, 129, 11, true);
                let record = (0..100).find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                    session.runner_mut().text_edit_snapshot().records.into_iter()
                        .find(|record| settled && record.drawing_intact && record.styled
                            && record.style_runs.as_ref().is_some_and(|runs| runs.iter().any(|run| run.start == 26)))
                }).expect("styled page must expose canonical mixed style runs");
                let runs = record.style_runs.as_ref().unwrap();
                assert_eq!(runs[0].start, 0);
                assert!(runs.windows(2).all(|pair| pair[0].start < pair[1].start));
                assert!(runs.iter().all(|run| run.start <= record.text.len()));
                assert!(runs.iter().any(|run| run.face != 0));
                assert!(runs.iter().any(|run| run.font == 3));
                let bold = runs.iter().find(|run| run.start == 7).unwrap();
                assert_eq!((bold.font, bold.face, bold.size, bold.color), (3, 1, 12, (0x1111, 0x2222, 0xdddd)));
                let italic = runs.iter().find(|run| run.start == 20).unwrap();
                assert_eq!((italic.font, italic.face, italic.size, italic.color), (4, 2, 14, (0x1111, 0x9999, 0x2222)));
                assert_eq!(runs.iter().find(|run| run.start == 26).unwrap().face, 4);
                let paint = record.paint.as_ref().unwrap_or_else(|| panic!("styled screen paint inputs: {record:?}"));
                assert_eq!(paint.depth, if powerpc { ppc_depth.unwrap_or(16) } else { depth.unwrap() });
                assert!(matches!(paint.mode, 0 | 1));
                assert_eq!(paint.style_ink.len(), runs.len());
                assert_eq!(paint.char_extra, if powerpc {
                    systemless::runner::TextEditCharExtraSnapshot::PpcPacked(0)
                } else { systemless::runner::TextEditCharExtraSnapshot::ClassicFixed(0) });
                let frame = session.video_frame().unwrap();
                let view = record.global_view_rect.unwrap();
                for ink in &paint.style_ink {
                    if paint.depth == 1 { assert_eq!(ink.rgb, [0; 3]); assert_eq!(ink.inverted_rgb, [255; 3]); }
                    let present = (view.0..view.2).any(|y| (view.1..view.3).any(|x| {
                        let at = ((y as u32 * frame.width + x as u32) * 4) as usize;
                        frame.pixels.get(at..at + 3) == Some(ink.rgb.as_slice())
                    }));
                    assert!(present, "resolved run ink {:?} appears in guest field, PPC={powerpc}, depth={depth:?}", ink);
                }
                // Compare complete ordered styled ink against the native
                // field, including empty background pixels. This fixture has
                // a white erased view and no active selection/caret.
                assert!(!record.active);
                let mut styled_pixels = std::collections::BTreeMap::new();
                for index in 0..record.line_count {
                    let line = super::super::text::StyledTextEditLine::from_guest(&record, index)
                        .expect("resolved zero-spacing srcOr styled line");
                    assert!(line.runs.windows(2).all(|pair| pair[0].bytes.end == pair[1].bytes.start));
                    for run in &line.runs {
                        assert_eq!(run.measured_positions.len(), run.bytes.len() + 1);
                    }
                    styled_pixels.extend(line.pixels().unwrap());
                }
                let dest = record.global_dest_rect.unwrap();
                let port_origin = (dest.0 - record.dest_rect.0, dest.1 - record.dest_rect.1);
                for y in view.0..view.2 {
                    for x in view.1..view.3 {
                        let expected = styled_pixels.get(&(x - port_origin.1, y - port_origin.0))
                            .copied().unwrap_or([255; 3]);
                        let at = ((y as u32 * frame.width + x as u32) * 4) as usize;
                        assert_eq!(&frame.pixels[at..at + 3], expected.as_slice(),
                            "ordered styled ink at ({x},{y}), PPC={powerpc}, depth={depth:?}, override={ppc_depth:?}");
                    }
                }
                let background = systemless::runner::TextEditInkSnapshot {
                    pixel: if paint.depth == 16 { 0x7fff } else { 0 },
                    rgb: [255; 3], inverted_rgb: [0; 3],
                };
                let plan = super::super::text::StyledTextEditPaintPlan::qualify(
                    &record, &background, None, &frame.pixels, frame.width, frame.height,
                ).expect("whole-field styled recipe matches native ink and background");
                let live_plans = super::qualify_styled_text_fields(std::slice::from_ref(&record),
                    &frame.pixels, frame.width, frame.height);
                assert_eq!(live_plans.len(), 1);
                assert_eq!(live_plans[0].as_ref().expect("worker qualifies native styled field").pixels, plan.pixels);
                assert_eq!(plan.background, [255; 3]);
                assert!(!plan.pixels.is_empty());
                assert_eq!(plan.view, record.view_rect);
                assert!(super::super::text::classic_styled_text_edit_field(plan.clone(), 1., (0., 0.)).is_some());
                for scale in [0., -1., f32::NAN, f32::INFINITY] {
                    assert!(super::super::text::classic_styled_text_edit_field(plan.clone(), scale, (0., 0.)).is_none());
                }
                assert!(super::super::text::classic_styled_text_edit_field(plan.clone(), 1., (f32::NAN, 0.)).is_none());
                let mut changed = frame.pixels.clone();
                let at = ((view.0 as u32 * frame.width + view.1 as u32) * 4) as usize;
                changed[at] ^= 1;
                assert!(super::qualify_styled_text_fields(std::slice::from_ref(&record),
                    &changed, frame.width, frame.height)[0].is_none());
                assert!(super::super::text::StyledTextEditPaintPlan::qualify(
                    &record, &background, None, &changed, frame.width, frame.height,
                ).is_none(), "modified application pixels refuse replacement");
                let mut stale = record.clone();
                stale.drawing_intact = false;
                assert!(super::super::text::StyledTextEditPaintPlan::qualify(
                    &stale, &background, None, &frame.pixels, frame.width, frame.height,
                ).is_none(), "pixel equality cannot replace missing drawing ownership evidence");
                changed = frame.pixels.clone();
                changed[0] ^= 1;
                assert!(super::super::text::StyledTextEditPaintPlan::qualify(
                    &record, &background, None, &changed, frame.width, frame.height,
                ).is_some(), "unrelated guest drawing outside the field remains independent");
                let metrics = record.line_metrics.as_ref().unwrap();
                assert_eq!(metrics.len(), record.line_count);
                assert!(metrics.iter().all(|&(height, ascent)| height > 0 && ascent > 0 && ascent <= height));
                assert!(record.display_lines().is_some());
                assert_eq!(record.line_layout_policy, if powerpc {
                    systemless::runner::TextEditLineLayoutPolicy::PpcRunMetrics
                } else {
                    systemless::runner::TextEditLineLayoutPolicy::CumulativeGuestMetrics
                });
                for index in 0..record.line_count {
                    let geometry = record.line_geometry(index, 0).expect("styled guest line anchors");
                    assert!(geometry.height > 0 && geometry.ascent >= 0);
                    let projected = record.visible_style_runs(index).expect("canonical visible style spans");
                    let starts = record.line_starts.as_ref().unwrap();
                    let mut visible_end = starts[index + 1];
                    while visible_end > starts[index]
                        && matches!(record.text[visible_end - 1], b' ' | b'\r' | b'\n')
                    { visible_end -= 1; }
                    let mut cursor = starts[index];
                    for (range, style) in projected {
                        assert_eq!(range.start, cursor, "no gaps or duplicated bytes in guest style projection");
                        assert!(range.end <= visible_end);
                        assert!(runs.iter().any(|canonical| canonical == style));
                        cursor = range.end;
                    }
                    assert_eq!(cursor, visible_end, "cover all and only guest visible text bytes");
                }
                // Mixed run presentation remains guest-owned until the GPUI
                // renderer preserves its line metrics and editing boundaries.
                let windows = session.runner_mut().window_frame_snapshot();
                assert!(super::super::frames::text_edit_pieces(&[record], &[], &[], &windows,
                    super::super::frames::Rect::from((0, 0, 600, 800))).is_empty());
            }
        }

        #[test]
        fn standard_list_first_row_qualification_preserves_native_cpu_pixels() {
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                settle(&mut session);
                assert_eq!(session.runner().presented_screen_depth(), Some(u32::from(depth)));
                let list = session.runner_mut().list_manager_snapshot().into_iter()
                    .find(|list| list.definition_id == 0 && list.draw_enabled && !list.cells.is_empty()).unwrap();
                let local = list.view_rect;
                let global = list.global_view_rect.unwrap();
                let cell = (list.visible.0, list.visible.1);
                assert!(!list.selected.contains(&cell));
                let bottom = local.0.saturating_add(list.cell_size.0).min(local.2);
                let right = local.1.saturating_add(list.cell_size.1).min(local.3);
                let metrics = systemless::quickdraw::text::get_font_metrics(1, 9);
                let baseline = if powerpc { local.0 + metrics.ascent } else {
                    local.0 + (bottom - local.0 - metrics.ascent - metrics.descent).max(0) / 2 + metrics.ascent
                };
                let retained = list.standard_cell_paint.get(&cell).expect("native standard painter retained this cell");
                assert_eq!((retained.font, retained.size, retained.left, retained.baseline),
                    (1, 9, local.1 + if powerpc { 1 } else { 3 }, baseline));
                assert_eq!(retained.bytes, list.cells[&cell]);
                assert!(!retained.selected);
                let layout = super::super::text::ClassicListCellLayout {
                    font: 1, size: 9, left: local.1 + if powerpc { 1 } else { 3 }, baseline,
                    clip: (local.0 + 1, local.1 + 1, bottom, right - 1),
                    stop_before: if powerpc { None } else { Some(right - 3) }, char_extra: 0,
                };
                let frame = session.video_frame().unwrap();
                let qualified = super::super::text::ClassicListCellPaintPlan::qualify(
                    layout, &list.cells[&cell], true, [0; 3], [255; 3],
                    (global.0 + 1, global.1 + 1, global.0 + bottom - local.0, global.1 + right - local.1 - 1),
                    &frame.pixels, frame.width, frame.height,
                );
                if !powerpc && depth == 1 {
                    assert!(qualified.is_none(), "native one-bit black custom drawing stays guest-owned");
                    for y in global.0 + 1..global.0 + bottom - local.0 {
                        for x in global.1 + 1..global.1 + right - local.1 - 1 {
                            let at = ((y as u32 * frame.width + x as u32) * 4) as usize;
                            assert_eq!(&frame.pixels[at..at + 3], &[0; 3]);
                        }
                    }
                } else {
                    assert!(qualified.is_some(), "native standard list row: PPC={powerpc}, depth={depth}");
                }
                let vertical = global.0 + list.cell_size.0 / 2;
                let horizontal = global.1 + 40;
                for input in [MacintoshInput::MouseDown { vertical, horizontal },
                    MacintoshInput::MouseUp { vertical, horizontal }] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                settle(&mut session);
                let selected = session.runner_mut().list_manager_snapshot().into_iter()
                    .find(|next| next.guest_id == list.guest_id).unwrap();
                assert!(selected.active && selected.selected.contains(&cell));
                let retained = selected.standard_cell_paint.get(&cell).expect("selected native painter evidence");
                assert!(retained.selected);
                assert_eq!((retained.font, retained.size, retained.baseline), (1, 9, baseline));
                assert_eq!(selected.cells, list.cells);
                assert_eq!(selected.generation, list.generation);
                let frame = session.video_frame().unwrap();
                // The known empty left inset samples the actual physical native
                // highlight; it does not infer a host theme or grant ownership.
                let at = (((global.0 + 1) as u32 * frame.width + (global.1 + 1) as u32) * 4) as usize;
                let background: [u8; 3] = frame.pixels[at..at + 3].try_into().unwrap();
                let foreground = if 299 * u32::from(background[0]) + 587 * u32::from(background[1])
                    + 114 * u32::from(background[2]) < 127_500 { [255; 3] } else { [0; 3] };
                let qualified = super::super::text::ClassicListCellPaintPlan::qualify(
                    layout, &selected.cells[&cell], true, foreground, background,
                    (global.0 + 1, global.1 + 1, global.0 + bottom - local.0, global.1 + right - local.1 - 1),
                    &frame.pixels, frame.width, frame.height,
                );
                assert!(qualified.is_some(), "native selected row: PPC={powerpc}, depth={depth}, background={background:?}");
                let plans = super::qualify_list_text_fields(std::slice::from_ref(&selected),
                    &frame.pixels, frame.width, frame.height);
                assert!(!plans[0].is_empty(), "production list qualification needs an owned native cell: PPC={powerpc}, depth={depth}, paint={:?}", selected.standard_cell_paint);

            }
        }

        #[test]
        fn multiline_styled_selection_plan_matches_native_paint_order() {
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 11));
                wait_for_menu(&mut session, 129, 11, true);
                settle(&mut session);
                let original = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.styled).unwrap();
                let dest = original.global_dest_rect.unwrap();
                let geometry = original.guest_styled_line_geometry(0).unwrap().0;
                let origin = (dest.0 - original.dest_rect.0, dest.1 - original.dest_rect.1);
                let horizontal = origin.1 + geometry.left + original.guest_styled_range_width(0..26).unwrap();
                let vertical = origin.0 + geometry.top + geometry.ascent;
                for input in [MacintoshInput::MouseDown { vertical, horizontal }, MacintoshInput::MouseUp { vertical, horizontal }] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x24, character: b'\r' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x24, character: b'\r' });
                let record = (0..300).find_map(|_| {
                    session.runner_mut().run_steps(10_000, None);
                    let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                    session.runner_mut().text_edit_snapshot().records.into_iter().find(|record|
                        settled && record.guest_id == original.guest_id && record.line_count == 2 && record.drawing_intact)
                }).expect("guest Return creates two mixed-metric styled lines");
                assert_eq!(record.text[26], b'\r');
                let first = record.guest_styled_line_geometry(0).unwrap().0;
                let second = record.guest_styled_line_geometry(1).unwrap().0;
                let start = (origin.0 + first.top + first.ascent, origin.1 + first.left);
                let line_start = record.line_starts.as_ref().unwrap()[1];
                let end = (origin.0 + second.top + second.ascent,
                    origin.1 + second.left + record.guest_styled_range_width(line_start..record.text.len()).unwrap());
                for input in [MacintoshInput::MouseDown { vertical: start.0, horizontal: start.1 },
                    MacintoshInput::MouseMove { vertical: end.0, horizontal: end.1 },
                    MacintoshInput::MouseUp { vertical: end.0, horizontal: end.1 }] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                let selected = (0..300).find_map(|_| {
                    session.runner_mut().run_steps(10_000, None);
                    let settled = session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0);
                    session.runner_mut().text_edit_snapshot().records.into_iter().find(|next|
                        settled && next.guest_id == record.guest_id && next.active && next.drawing_intact
                            && next.selection == (0, record.text.len()))
                }).expect("guest drag selects both styled lines");
                if powerpc {
                    let first = selected.guest_styled_selection_rect(0).unwrap().unwrap();
                    let second = selected.guest_styled_selection_rect(1).unwrap().unwrap();
                    assert!(first.0 < second.2 && second.0 < first.2,
                        "mixed PPC line heights must exercise overlapping highlight boxes");
                }
                let frame = session.video_frame().unwrap();
                let background = systemless::runner::TextEditInkSnapshot {
                    pixel: if depth == 16 { 0x7fff } else { 0 }, rgb: [255; 3], inverted_rgb: [0; 3],
                };
                assert!(super::super::text::StyledTextEditPaintPlan::qualify(
                    &selected, &background, None, &frame.pixels, frame.width, frame.height,
                ).is_some(), "CPU-native line/selection order: PPC={powerpc}, depth={depth}");
            }
        }

        #[test]
        fn styled_guest_idle_blink_repaints_the_native_caret() {
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 11));
                wait_for_menu(&mut session, 129, 11, true);
                settle(&mut session);
                let original = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.styled).unwrap();
                let dest = original.global_dest_rect.unwrap();
                let geometry = original.guest_styled_line_geometry(0).unwrap().0;
                let x = dest.1 - original.dest_rect.1 + geometry.left;
                let y = dest.0 - original.dest_rect.0 + geometry.top;
                let pixel = |session: &mut MacintoshSession| {
                    let frame = session.video_frame().unwrap();
                    let at = ((y as u32 * frame.width + x as u32) * 4) as usize;
                    frame.pixels[at..at + 3].to_vec()
                };
                let background = pixel(&mut session);
                for input in [MacintoshInput::MouseDown { vertical: y + geometry.ascent, horizontal: x },
                    MacintoshInput::MouseUp { vertical: y + geometry.ascent, horizontal: x }] {
                    session.deliver_input(input);
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                }
                let mut painted = None;
                for visible in [true, false, true] {
                    let record = (0..300).find_map(|_| {
                        session.runner_mut().force_advance_guest_tick();
                        session.runner_mut().run_steps(10_000, None);
                        let settled = session.runner().event_manager_snapshot().last_record
                            .is_some_and(|event| event.what == 0);
                        session.runner_mut().text_edit_snapshot().records.into_iter().find(|record|
                            settled && record.guest_id == original.guest_id && record.active
                                && record.caret_visible == visible && record.drawing_intact)
                    }).expect("guest TEIdle completes native caret repaint");
                    assert_eq!(record.selection, (0, 0));
                    assert_eq!(record.text, original.text);
                    assert_eq!(record.style_runs, original.style_runs);
                    let actual = pixel(&mut session);
                    if visible {
                        assert_ne!(actual, background, "visible native caret: PPC={powerpc}, depth={depth}");
                        if let Some(ref previous) = painted { assert_eq!(&actual, previous); }
                        painted = Some(actual);
                    } else {
                        assert_eq!(actual, background, "hidden native caret must restore its pixel: PPC={powerpc}, depth={depth}");
                    }
                }
            }
        }

        #[test]
        fn styled_text_edit_guest_click_typing_and_activation_preserve_runs() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 11));
                wait_for_menu(&mut session, 129, 11, true);
                settle(&mut session);
                let original = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.styled).expect("styled guest record");
                assert!(!original.active, "sample remains unfocused until clicked");
                for insertion in [0, 9] {
                    let dest = original.global_dest_rect.unwrap();
                    let geometry = original.line_geometry(0, 0).unwrap();
                    let vertical = dest.0 - original.dest_rect.0 + geometry.top + geometry.ascent;
                    let horizontal = dest.1 - original.dest_rect.1 + geometry.left;
                    session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                    for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                    session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                    settle(&mut session);
                    let focused = session.runner_mut().text_edit_snapshot().records.into_iter()
                        .find(|record| record.guest_id == original.guest_id).unwrap();
                    assert!(focused.active);
                    assert_eq!(focused.selection, (0, 0));
                    for _ in 0..insertion {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x7c, character: 0x1d });
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x7c, character: 0x1d });
                        session.runner_mut().run_steps(100_000, None);
                    }
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x06, character: b'z' });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x06, character: b'z' });
                    let _inserted = (0..100).find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|record| record.guest_id == original.guest_id
                                && record.text.len() == original.text.len() + 1)
                    }).expect("guest TEKey must insert into styled field");
                    assert!((0..300).any(|_| {
                        session.runner_mut().run_steps(10_000, None);
                        session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0)
                    }), "guest must finish styled insertion and redraw");
                    let inserted = session.runner_mut().text_edit_snapshot().records.into_iter()
                        .find(|record| record.guest_id == original.guest_id).unwrap();
                    assert_eq!(inserted.text[insertion], b'z');
                    assert_eq!(&inserted.text[..insertion], &original.text[..insertion]);
                    assert_eq!(&inserted.text[insertion + 1..], &original.text[insertion..]);
                    assert_eq!(inserted.selection, (insertion + 1, insertion + 1));
                    let shifted: Vec<_> = original.style_runs.as_ref().unwrap().iter().cloned()
                        .map(|mut run| { if run.start > insertion { run.start += 1; } run }).collect();
                    assert_eq!(inserted.style_runs.as_ref().unwrap(), &shifted,
                        "typing must preserve all existing font, face and RGB16 run attributes: PPC={powerpc}, depth={depth:?}, insertion={insertion}");
                    session.request_foreground(false);
                    let suspended = (0..100).find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|record| record.guest_id == original.guest_id && !record.active)
                    }).expect("guest styled TEDeactivate on suspend");
                    assert_eq!(suspended.text, inserted.text);
                    assert_eq!(suspended.selection, inserted.selection);
                    assert_eq!(suspended.style_runs, inserted.style_runs);
                    session.request_foreground(true);
                    let resumed = (0..100).find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|record| record.guest_id == original.guest_id && record.active)
                    }).expect("guest styled TEActivate on resume");
                    assert_eq!(resumed.style_runs, inserted.style_runs);
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x33, character: 0x08 });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x33, character: 0x08 });
                    let _restored = (0..100).find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|record| record.guest_id == original.guest_id && record.text == original.text)
                    }).expect("guest backspace after styled resume");
                    assert!((0..300).any(|_| {
                        session.runner_mut().run_steps(10_000, None);
                        session.runner().event_manager_snapshot().last_record.is_some_and(|event| event.what == 0)
                    }), "guest must finish styled deletion and redraw");
                    let restored = session.runner_mut().text_edit_snapshot().records.into_iter()
                        .find(|record| record.guest_id == original.guest_id).unwrap();
                    assert_eq!(restored.selection, (insertion, insertion));
                    assert_eq!(restored.style_runs, original.style_runs);
                }
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
                if depth == Some(1) {
                    assert_eq!(session.runner().dispatcher().device_clut[1], [0; 3], "initial monochrome black");
                }
                wait_for_menu(&mut session, 129, 1, true);
                if depth == Some(1) {
                    assert_eq!(session.runner().dispatcher().device_clut[1], [0; 3], "monochrome black after showing the window");
                }
                let records = session.runner_mut().text_edit_snapshot().records;
                let windows = session.runner_mut().window_frame_snapshot();
                let dialogs = session.runner_mut().dialog_snapshot();
                let controls = session.runner_mut().control_snapshot();
                assert!(super::super::frames::text_edit_pieces(
                    &records, &dialogs, &controls, &windows,
                    super::super::frames::Rect { top: 0, left: 0, bottom: 600, right: 800 },
                ).is_empty(), "allocated text must not overlay the graphics page; powerpc={powerpc}");
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
                assert!(first.font_ascent > 0 && first.font_ascent <= first.line_height);
                let starts = first.line_starts.as_ref().expect("TextEdit should expose guest lines");
                assert_eq!(starts.first(), Some(&0));
                assert_eq!(starts.last(), Some(&first.text.len()));
                assert_eq!(starts.len(), first.line_count + 1);
                assert_eq!(first.display_lines().unwrap().len(), first.line_count);
                // The GPUI binary-ink layout must agree with the guest TEClick
                // insertion geometry, not merely its own paint calculations.
                let end = starts[1].min(first.text.len());
                let line = super::super::text::ClassicLine::plain(&first.text[..end], first.font, first.size);
                let dest = first.global_dest_rect.unwrap();
                let geometry = first.line_geometry(0, 0).expect("guest first-line geometry");
                assert_eq!(first.line_layout_policy, if powerpc {
                    systemless::runner::TextEditLineLayoutPolicy::PpcRunMetrics
                } else {
                    systemless::runner::TextEditLineLayoutPolicy::CumulativeGuestMetrics
                });
                let horizontal = dest.1 - first.dest_rect.1 + geometry.left + line.positions[3] as i16;
                let vertical = dest.0 - first.dest_rect.0 + geometry.top + geometry.ascent;
                session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                settle(&mut session);
                let clicked = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.guest_id == first.guest_id).unwrap();
                assert_eq!(clicked.selection, (3, 3), "painted guest glyph boundary, PPC={powerpc}, depth={depth:?}");
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
                assert!(next.drawing_intact, "painted text must remain eligible; powerpc={powerpc}, depth={depth:?}");
                // Compare the actual guest first-line pixels with the GPUI
                // binary spans, including its one-pixel inset and caret.
                let end = next.line_starts.as_ref().unwrap()[1];
                let mut visible_end = end;
                while visible_end > 0 && matches!(next.text[visible_end - 1], b' ' | b'\r' | b'\n') {
                    visible_end -= 1;
                }
                let glyphs = super::super::text::ClassicLine::plain(&next.text[..visible_end], next.font, next.size);
                let geometry = super::super::text::ClassicLineGeometry::TextEdit;
                let mut expected = std::collections::HashSet::new();
                for &(x, y, width) in &glyphs.ink {
                    for px in x..x + width {
                        expected.insert((i32::from(dest.1) + geometry.ink_x(px),
                            i32::from(dest.0 + next.font_ascent) + y));
                    }
                }
                let guest_ink = expected.clone();
                if next.caret_visible {
                    let x = i32::from(dest.1) + geometry.caret_x(glyphs.positions[3], 3);
                    expected.extend((i32::from(dest.0)..i32::from(dest.0 + next.line_height)).map(|y| (x, y)));
                }
                let frame = session.video_frame().unwrap();
                for y in i32::from(dest.0)..i32::from(dest.0 + next.line_height) {
                    for x in i32::from(dest.1)..i32::from(dest.3) {
                        let offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
                        let ink = frame.pixels[offset..offset + 3].iter().all(|value| *value < 128);
                        assert_eq!(ink, expected.contains(&(x, y)),
                            "TE line ink at {x},{y}, PPC={powerpc}, depth={depth:?}");
                    }
                }
                // Reach the shared soft-wrap byte boundary through TEKey,
                // then compare the first-line caret against guest pixels.
                for _ in 3..end {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x7c, character: 0x1d });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x7c, character: 0x1d });
                    session.runner_mut().run_steps(100_000, None);
                }
                let boundary = (0..100).find_map(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().text_edit_snapshot().records.into_iter().find(|record| {
                        record.guest_id == first.guest_id && record.selection == (end, end) && record.caret_visible
                    })
                }).expect("guest Right arrow must reach the first wrap boundary");
                assert_eq!(boundary.clips_line_offsets_to_visible_text, powerpc);
                let (owner, offset) = boundary.caret_line().unwrap();
                assert_eq!(owner, 0, "guest boundary caret belongs to the first matching line");
                assert_eq!(offset, if powerpc { visible_end } else { end });
                let measured = super::super::text::ClassicLine::plain(&boundary.text[..end], boundary.font, boundary.size);
                let caret_x = i32::from(dest.1) + geometry.caret_x(measured.positions[offset], offset);
                let mut boundary_expected = guest_ink;
                boundary_expected.extend((i32::from(dest.0)..i32::from(dest.0 + boundary.line_height)).map(|y| (caret_x, y)));
                let frame = session.video_frame().unwrap();
                for y in i32::from(dest.0)..i32::from(dest.0 + boundary.line_height) {
                    for x in i32::from(dest.1)..i32::from(dest.3) {
                        let pixel = ((y as u32 * frame.width + x as u32) * 4) as usize;
                        assert_eq!(frame.pixels[pixel..pixel + 3].iter().all(|value| *value < 128),
                            boundary_expected.contains(&(x, y)), "wrap caret ink at {x},{y}, PPC={powerpc}, depth={depth:?}");
                    }
                }
                session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                settle(&mut session);
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
                let mut expected = replaced.text.clone();
                for (offset, text) in ["é", "£", "π"].into_iter().enumerate() {
                    let key = gpui_kit::Keystroke {
                        key: "e".into(), key_char: Some(text.into()),
                        ..Default::default()
                    };
                    let (mac_key, character) = super::guest_key(&key).unwrap();
                    session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
                    expected.insert(offset + 1, character);
                    let typed = (0..100).find_map(|_| {
                        session.runner_mut().run_steps(100_000, None);
                        session.runner_mut().text_edit_snapshot().records.into_iter().find(|record| {
                            record.guest_id == first.guest_id && record.text == expected
                        })
                    }).expect("guest TextEdit should retain the exact Mac Roman byte");
                    assert_eq!(typed.selection, (offset + 2, offset + 2));
                }
                assert!(session.runner_mut().select_guest_menu_item(129, 1));
                wait_for_menu(&mut session, 129, 1, true);
                settle(&mut session);
                let retained = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.guest_id == first.guest_id).unwrap();
                assert!(!retained.drawing_intact,
                    "retained text must not overlay a subsequently painted page; powerpc={powerpc}, depth={depth:?}");

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
            for (text, expected) in [("é", 0x8e), ("£", 0xa3), ("π", 0xb9)] {
                let key = gpui_kit::Keystroke {
                    key: "e".into(), key_char: Some(text.into()),
                    ..Default::default()
                };
                assert_eq!(super::guest_key(&key), Some((0x0e, expected)));
            }
            let non_roman = gpui_kit::Keystroke {
                key: "a".into(),
                key_char: Some("あ".into()),
                ..Default::default()
            };
            assert_eq!(super::guest_key(&non_roman), None);
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn fullscreen_guest_menu_reveals_only_at_top_edge(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

            let (sender, receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(900.)),
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
                    demo.menu_presented = false;
                    demo.width = 800;
                    demo.height = 600;

                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                        image::Frame::new(image::RgbaImage::new(800, 600)),
                    ])));
                    demo.menus = GuestMenuSnapshot {
                        menus: vec![GuestMenu {
                            guest_id: 1,
                            generation: 1,
                            id: 128,
                            title: "File".into(),
                            enabled: true,
                            standard_definition: true,
                            hierarchical: false,
                            visible_in_menu_bar: true,
                            items: vec![GuestMenuItem {
                                mark: 0,
                                style: 0,
                                number: 1,
                                text: "Open".into(),
                                enabled: true,
                                checked: false,
                                key_equivalent: None,
                                submenu_id: None,
                                separator: false,
                            }],
                        }],
                        ..Default::default()
                    };
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("guest-menu-1-1").is_none());
                assert!(window.try_find("guest-status").is_none());
                let screen = window.find("guest-screen");
                assert_eq!(screen.bounds().origin.x, gpui_kit::px(0.));
                assert_eq!(screen.bounds().origin.y, gpui_kit::px(0.));
                assert_eq!(screen.bounds().size.width, gpui_kit::px(1200.));
                assert_eq!(screen.bounds().size.height, gpui_kit::px(900.));
                window.dispatch_event(gpui_kit::PlatformInput::MouseMove(
                    gpui_kit::MouseMoveEvent {
                        position: gpui_kit::point(gpui_kit::px(100.), gpui_kit::px(2.)),
                        ..Default::default()
                    },
                ), cx);
                window.render_frame(cx);
                assert!(window.try_find("guest-menu-1-1").is_some());
                window.click("guest-menu-1-1", cx);
                assert!(window.try_find("guest-popup-item-128-1").is_some());
                window.dispatch_event(gpui_kit::PlatformInput::MouseMove(
                    gpui_kit::MouseMoveEvent {
                        position: gpui_kit::point(gpui_kit::px(100.), gpui_kit::px(100.)),
                        ..Default::default()
                    },
                ), cx);
                window.render_frame(cx);
                assert!(window.try_find("guest-menu-1-1").is_some());
                assert!(window.try_find("guest-popup-item-128-1").is_some());
                window.hover("guest-popup-item-128-1", cx);
                assert_eq!(window.find("guest-popup-item-128-1").selected(), Some(true));
                window.click("guest-popup-item-128-1", cx);
            }).unwrap();
            assert!(std::iter::from_fn(|| receiver.try_recv().ok()).any(|command| {
                matches!(command, super::Command::Menu(128, 1, 1, 1))
            }));
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("guest-popup-item-128-1").is_none());
                assert!(window.try_find("guest-menu-1-1").is_none());
            }).unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menu_presented = true;
                    demo.menu_height = 24;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("guest-menu-1-1").is_some());
                assert_eq!(window.find("guest-menu-bar").bounds().size.height, gpui_kit::px(36.));
                let screen = window.find("guest-screen");
                assert_eq!(screen.bounds().origin.x, gpui_kit::px(0.));
                assert_eq!(screen.bounds().origin.y, gpui_kit::px(0.));
                assert_eq!(screen.bounds().size.width, gpui_kit::px(1200.));
                assert_eq!(screen.bounds().size.height, gpui_kit::px(900.));
            }).unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.windows.push(systemless::runner::WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: systemless::runner::WindowSnapshot {
                            title: "Dialog over scene".into(),
                            bounds: (100, 100, 300, 500),
                            structure_bounds: Some((80, 99, 302, 502)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(0),
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![(100, 100, 300, 500)]),
                        close_box: true,
                        grow_icon_drawn: false,
                    });
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let screen = window.find("guest-screen");
                assert_eq!(screen.bounds().size.width, gpui_kit::px(1200.));
                assert_eq!(screen.bounds().size.height, gpui_kit::px(900.));
            }).unwrap();
            assert_eq!(view.read_with(cx, |demo, _| {
                demo.pointer(gpui_kit::point(gpui_kit::px(450.), gpui_kit::px(300.)))
            }), (200, 300));
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
                    demo.status = "Ready".into();
                    demo.width = 320;
                    demo.height = 240;
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
                                mark: 0,
                                style: 0,
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
                assert_eq!(window.find("guest-popup-item-129-1").label(), Some("Open"));
                window.hover("guest-popup-item-129-1", cx);
                assert_eq!(window.find("guest-popup-item-129-1").selected(), Some(true));
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus[0].items[0].text = "Open recent".into();
                    demo.menus.menus[0].items[0].checked = true;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let item = window.find("guest-popup-item-129-1");
                assert_eq!(item.label(), Some("Open recent"));
                assert_eq!(item.selected(), Some(true));
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus[0].items[0].enabled = false;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(window.find("guest-popup-item-129-1").selected(), Some(false));
                window.within("guest-popup-menu").press("escape", cx);
            })
            .unwrap();
            cx.wait_for(window.into(), Duration::from_secs(1), |window, _| {
                window.try_find("guest-popup-menu").is_none()
            })
            .await;
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-menu-4096-1", cx);
                assert_eq!(
                    window.find("guest-popup-item-129-1").label(),
                    Some("Open recent")
                );
                window.click("guest-status", cx);
                assert!(window.try_find("guest-popup-menu").is_none());
                window.click("guest-menu-4096-1", cx);
                assert_eq!(window.find("guest-popup-item-129-1").selected(), Some(false));
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus[0].items = (1..=40)
                        .map(|number| GuestMenuItem {
                            mark: 0,
                            style: 0,
                            number,
                            text: format!("Item {number}"),
                            enabled: true,
                            checked: false,
                            key_equivalent: None,
                            submenu_id: None,
                            separator: false,
                        })
                        .collect();
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                for _ in 0..40 {
                    window.within("guest-popup-menu").press("down", cx);
                }
                let last = window.find("guest-popup-item-129-40");
                assert_eq!(last.selected(), Some(true));
                assert!(
                    last.visible(),
                    "keyboard selection must scroll into view: {:?}",
                    last.bounds()
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
                assert!(window.try_find("guest-popup-menu").is_none());
            })
            .unwrap();
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn live_menu_update_preserves_keyboard_selection(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

            let (sender, receiver) = std::sync::mpsc::channel();
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
                            items: ["Open", "Save"]
                                .into_iter()
                                .enumerate()
                                .map(|(index, text)| GuestMenuItem {
                                    mark: 0,
                                    style: 0,
                                    number: index as i16 + 1,
                                    text: text.into(),
                                    enabled: true,
                                    checked: false,
                                    key_equivalent: None,
                                    submenu_id: None,
                                    separator: false,
                                })
                                .collect(),
                        }],
                    };
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                for command in receiver.try_iter() {
                    assert!(matches!(command, super::Command::Foreground(false, _)), "only initial window activation may precede menu input");
                }
                window.click("guest-menu-4096-1", cx);
                window.within("guest-popup-menu").press("down", cx);
                window.within("guest-popup-menu").press("down", cx);
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    let mut other = demo.menus.menus[0].clone();
                    other.guest_id = 0x2000;
                    other.id = 130;
                    other.title = "Edit".into();
                    other.items[0].text = "Undo".into();
                    demo.menus.menus.push(other);
                    demo.menus.menus[0].items[0].text = "Open recent".into();
                    demo.menus.menus[0].items[0].checked = true;
                    demo.menus.menus[0].items[0].enabled = false;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let disabled = window.find("guest-popup-item-129-1");
                assert_eq!(disabled.disabled(), Some(true));
                assert_eq!(disabled.checked(), Some(true));
                window.click("guest-popup-item-129-1", cx);
                window.within("guest-popup-menu").press("enter", cx);
            })
            .unwrap();
            let received = receiver.try_recv().expect("menu selection should queue a command");
            assert!(matches!(
                received,
                super::Command::Menu(129, 2, 0x1000, 1)
            ));
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("guest-menu-4096-1", cx);
                window.within("guest-popup-menu").press("down", cx);
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    let replacement = &mut demo.menus.menus[0];
                    replacement.generation = 2;
                    replacement.items[0].enabled = true;
                    replacement.items[0].text = "Replacement command".into();
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("guest-menu-4096-1").is_none());
                assert!(window.try_find("guest-popup-menu").is_none());
                assert!(view.read(cx).focus.is_focused(window), "replaced menus must restore guest focus");
                assert!(
                    view.read(cx).open_menus.is_empty(),
                    "disposed menus must not pin the auto-revealed bar"
                );
                assert!(
                    !receiver.try_iter().any(|command| matches!(command, super::Command::Menu(..))),
                    "disposal must not select a command"
                );
                window.click("guest-menu-4096-2", cx);
                window.within("guest-popup-menu").press("down", cx);
                window.within("guest-popup-menu").press("enter", cx);
            })
            .unwrap();
            assert!(matches!(
                receiver
                    .try_iter()
                    .find(|command| matches!(command, super::Command::Menu(..)))
                    .expect("replacement menu command"),
                super::Command::Menu(129, 1, 0x1000, 2)
            ));
            assert!(
                !receiver.try_iter().any(|command| matches!(command, super::Command::Menu(..)))
            );
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn hierarchical_menu_keyboard_selection_reaches_guest(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};

            let (sender, receiver) = std::sync::mpsc::channel();
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
            let item = |number, text: &str, submenu_id| GuestMenuItem {
                mark: 0,
                style: 0,
                number,
                text: text.into(),
                enabled: true,
                checked: false,
                key_equivalent: None,
                submenu_id,
                separator: false,
            };
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus = GuestMenuSnapshot {
                        custom_bar_definition: false,
                        menus: vec![
                            GuestMenu {
                                guest_id: 0x1000,
                                generation: 1,
                                id: 129,
                                title: "File".into(),
                                enabled: true,
                                standard_definition: true,
                                hierarchical: false,
                                visible_in_menu_bar: true,
                                items: vec![item(1, "Recent", Some(130)), item(2, "Save", None)],
                            },
                            GuestMenu {
                                guest_id: 0x2000,
                                generation: 1,
                                id: 130,
                                title: "Recent".into(),
                                enabled: true,
                                standard_definition: true,
                                hierarchical: true,
                                visible_in_menu_bar: false,
                                items: vec![item(1, "Example", None)],
                            },
                        ],
                    };
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                for command in receiver.try_iter() {
                    assert!(matches!(command, super::Command::Foreground(false, _)), "only initial window activation may precede menu input");
                }
                window.click("guest-menu-4096-1", cx);
                window.render_frame(cx);
                assert_eq!(window.find("guest-popup-item-129-1").expanded(), Some(false));
                window.within("guest-popup-menu").press("z", cx);
                assert!(receiver.try_recv().is_err(), "host-focused menu must not leak ordinary keys to the guest");
                window.within("guest-popup-menu").press("down", cx);
                window.within("guest-popup-menu").press("right", cx);
                window.render_frame(cx);
                assert_eq!(window.find("guest-popup-item-129-1").expanded(), Some(true));
                assert_eq!(window.find("guest-popup-item-130-1").label(), Some("Example"));
            })
            .unwrap();
            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.menus.menus[1].generation = 2;
                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.within("guest-popup-menu").press("enter", cx);
                assert!(receiver.try_recv().is_err(), "replaced submenu must not run stale item");
                window.within("guest-popup-menu").press("enter", cx);
            })
            .unwrap();
            assert!(matches!(
                receiver.try_recv(),
                Ok(super::Command::Menu(130, 1, 0x2000, 2))
            ));
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.press("z", cx);
            }).unwrap();
            assert!(receiver.try_iter().any(|command| matches!(command,
                super::Command::Input(MacintoshInput::KeyDown { character: b'z', .. })
            )), "closing a menu must restore guest keyboard input");
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-menu-4096-1", cx);
                window.render_frame(cx);
                window.within("guest-popup-menu").press("escape", cx);
            }).unwrap();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                view.update(cx, |demo, _| {
                    assert!(demo.open_menus.is_empty(), "Escape left menus open: {:?}", demo.open_menus);
                    assert!(demo.focus.is_focused(window), "Escape did not restore scene focus");
                });
                window.press("x", cx);
            }).unwrap();
            let commands: Vec<_> = receiver.try_iter().collect();
            assert!(!commands.iter().any(|command| matches!(command, super::Command::Menu(..))));
            assert!(commands.iter().any(|command| matches!(command,
                super::Command::Input(MacintoshInput::KeyDown { character: b'x', .. })
            )), "cancelled menus must restore guest keyboard input");
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn classic_document_glyph_clicks_reach_guest_at_scene_scales(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
            cx.update(gpui_kit::init);
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth.unwrap_or(16)).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 7));
                wait_for_menu(&mut session, 129, 7, true);
                settle(&mut session);
                assert_eq!(session.runner().presented_screen_depth(), Some(u32::from(depth.unwrap_or(16))),
                    "interaction matrix must exercise the intended framebuffer depth");

                for scale in [0.75, 1., 1.5, 2.] {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let (window, view) = cx.update(|cx| {
                        gpui_kit::open_window(gpui_kit::WindowOptions {
                            // Explicit test bounds: Bounds::centered clamps to
                            // the mock display and would silently reduce 2x.
                            window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                                gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                                gpui_kit::size(gpui_kit::px(800. * scale), gpui_kit::px(600. * scale))))),
                            ..Default::default()
                        }, cx, |_, cx| cx.new(|cx| super::Demo::new(sender, Default::default(), cx))).unwrap()
                    });
                    for (line_index, offset, drag_end) in [(0, 3, None), (1, 2, None), (0, 3, Some(6))] {
                        let records = session.runner_mut().text_edit_snapshot().records;
                        let record = records.iter().find(|record| record.view_rect == (76, 34, 211, 326)).unwrap();
                        let starts = record.line_starts.as_ref().unwrap();
                        let layout = super::super::text::ClassicLine::plain(
                            &record.text[starts[line_index]..starts[line_index + 1]], record.font, record.size);
                        let dest = record.global_dest_rect.unwrap();
                        // TextEdit paints glyphs one guest pixel inside destRect.
                        // Exercise the displayed boundary, including that inset.
                        let guest_y = i32::from(dest.0) + line_index as i32 * i32::from(record.line_height)
                            + i32::from(record.font_ascent);
                        let mut events = vec![(0, offset)];
                        if let Some(end) = drag_end { events.push((1, end)); }
                        events.push((2, drag_end.unwrap_or(offset)));
                        for (phase, pointer_offset) in events {
                            let guest_x = i32::from(dest.1) + 1 + layout.positions[pointer_offset];
                            cx.update_window(window.into(), |_, window, cx| {
                                view.update(cx, |demo, cx| {
                                    demo.width = 800;
                                    demo.height = 600;
                                    demo.windows = session.runner_mut().window_frame_snapshot();
                                    demo.controls = session.runner_mut().control_snapshot();
                                    demo.text_edits = records.clone();
                                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                                        image::Frame::new(image::RgbaImage::new(800, 600))
                                    ])));
                                    cx.notify();
                                });
                                window.render_frame(cx);
                                let position = view.update(cx, |demo, _| {
                                    assert!((demo.display_scale - scale).abs() < 0.01,
                                        "expected scale {scale}, got {}, viewport {:?}", demo.display_scale, window.viewport_size());
                                    gpui_kit::point(
                                        gpui_kit::px(demo.display_origin.0 + (guest_x as f32 + 0.25) * scale),
                                        gpui_kit::px(demo.display_origin.1 + (guest_y as f32 + 0.25) * scale))
                                });
                                let event = match phase {
                                    0 => MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                    1 => MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() }.to_platform_input(),
                                    _ => MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                };
                                window.dispatch_event(event, cx);
                            }).unwrap();
                            let mut delivered = 0;
                            for command in receiver.try_iter() {
                                if let super::Command::Input(input) = command {
                                    session.deliver_input(input);
                                    delivered += 1;
                                }
                            }
                            assert_eq!(delivered, 1);
                            for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                        }
                        settle(&mut session);
                        let clicked = session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|next| next.guest_id == record.guest_id).unwrap();
                        let expected = starts[line_index] + offset;
                        let expected_end = starts[line_index] + drag_end.unwrap_or(offset);
                        assert_eq!(clicked.selection, (expected, expected_end),
                            "PPC={powerpc}, depth={depth:?}, scale={scale}, line={line_index}, drag_end={drag_end:?}");
                    }
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn qualified_list_clicks_reach_guest_at_centered_scene_scales(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, InputEvent, MouseButton, MouseDownEvent, MouseUpEvent};
            cx.update(gpui_kit::init);
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, Some(if powerpc { 8 } else { depth }));
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                settle(&mut session);
                for scale in [0.75, 1., 1.5, 2.] {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let (window, view) = cx.update(|cx| {
                        gpui_kit::open_window(gpui_kit::WindowOptions {
                            window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                                gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                                gpui_kit::size(gpui_kit::px(880. * scale), gpui_kit::px(600. * scale))))),
                            ..Default::default()
                        }, cx, |_, cx| cx.new(|cx| super::Demo::new(sender, Default::default(), cx))).unwrap()
                    });
                    for row in [0, 7] {
                        let initial = session.runner_mut().list_manager_snapshot().into_iter()
                            .find(|list| list.draw_enabled && list.definition_id == 0).unwrap();
                        let global = initial.global_view_rect.unwrap();
                        let paint = initial.standard_cell_paint.get(&(row, 0)).expect("retained row paint");
                        assert_eq!(paint.depth, depth);
                        let guest_x = i32::from(global.1) - i32::from(initial.view_rect.1) + i32::from(paint.left) + 12;
                        let guest_y = i32::from(global.0) - i32::from(initial.view_rect.0) + i32::from(paint.baseline) - 3;
                        for down in [true, false] {
                            let lists = session.runner_mut().list_manager_snapshot();
                            let frame = session.video_frame().unwrap();
                            let plans = super::qualify_list_text_fields(&lists, &frame.pixels, frame.width, frame.height);
                            if down { assert!(plans.iter().any(|plans| plans.contains_key(&(row, 0))), "displayed row must qualify: PPC={powerpc}, depth={depth}, row={row}, scale={scale}"); }
                            cx.update_window(window.into(), |_, window, cx| {
                                view.update(cx, |demo, cx| {
                                    demo.width = frame.width;
                                    demo.height = frame.height;
                                    demo.windows = session.runner_mut().window_frame_snapshot();
                                    demo.controls = session.runner_mut().control_snapshot();
                                    demo.lists = lists.clone();
                                    demo.list_text_plans = plans.clone();
                                    assert!(!super::super::frames::list_pieces(&demo.lists, &demo.controls,
                                        &demo.windows, super::super::frames::Rect { top: 0, left: 0, bottom: 600, right: 800 }).is_empty());
                                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                                        image::Frame::new(image::RgbaImage::from_raw(frame.width, frame.height,
                                            super::gpui_pixels(frame.pixels.clone())).unwrap())
                                    ])));
                                    cx.notify();
                                });
                                window.render_frame(cx);
                                let position = view.update(cx, |demo, _| {
                                    assert!((demo.display_scale - scale).abs() < 0.01);
                                    assert!(demo.display_origin.0 > 0., "exercise centered scene origin");
                                    gpui_kit::point(gpui_kit::px(demo.display_origin.0 + (guest_x as f32 + 0.25) * scale),
                                        gpui_kit::px(demo.display_origin.1 + (guest_y as f32 + 0.25) * scale))
                                });
                                let event = if down {
                                    MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input()
                                } else {
                                    MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input()
                                };
                                window.dispatch_event(event, cx);
                            }).unwrap();
                            let mut delivered = 0;
                            for command in receiver.try_iter() {
                                if let super::Command::Input(input) = command { session.deliver_input(input); delivered += 1; }
                            }
                            assert_eq!(delivered, 1, "one guest event per GPUI pointer event");
                            for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                        }
                        settle(&mut session);
                        let clicked = session.runner_mut().list_manager_snapshot().into_iter()
                            .find(|list| list.guest_id == initial.guest_id && list.generation == initial.generation).unwrap();
                        assert_eq!(clicked.selected, [(row, 0)].into(), "PPC={powerpc}, depth={depth}, row={row}, scale={scale}");
                    }
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn styled_document_glyph_clicks_reach_guest_at_centered_scene_scales(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
            cx.update(gpui_kit::init);
            for (powerpc, depth, ppc_depth) in [(false, Some(1), None), (false, Some(8), None), (true, None, Some(16)), (true, None, Some(8))] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if let Some(depth) = ppc_depth { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 11));
                wait_for_menu(&mut session, 129, 11, true);
                settle(&mut session);
                for scale in [0.75, 1., 1.5, 2.] {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let (window, view) = cx.update(|cx| {
                        gpui_kit::open_window(gpui_kit::WindowOptions {
                            // Explicit test bounds: Bounds::centered clamps to
                            // the mock display and would silently reduce 2x.
                            window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                                gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                                gpui_kit::size(gpui_kit::px(880. * scale), gpui_kit::px(600. * scale))))),
                            ..Default::default()
                        }, cx, |_, cx| cx.new(|cx| super::Demo::new(sender, Default::default(), cx))).unwrap()
                    });
                    for (line_index, offset, drag_end) in [(0, 3, None), (0, 20, None), (0, 9, Some(26))] {
                        let records = session.runner_mut().text_edit_snapshot().records;
                        let record = records.iter().find(|record| record.styled).unwrap();
                        let starts = record.line_starts.as_ref().unwrap();
                        let (geometry, runs) = record.guest_styled_line_geometry(line_index).unwrap();
                        let dest = record.global_dest_rect.unwrap();
                        let guest_y = i32::from(dest.0) - i32::from(record.dest_rect.0)
                            + i32::from(geometry.top) + i32::from(geometry.ascent);
                        let mut events = vec![(0, offset)];
                        if let Some(end) = drag_end { events.push((1, end)); }
                        events.push((2, drag_end.unwrap_or(offset)));
                        for (phase, pointer_offset) in events {
                            let local_x = runs.iter().find_map(|(range, positions)|
                                (range.start <= pointer_offset && pointer_offset <= range.end)
                                    .then(|| positions[pointer_offset - range.start])).unwrap();
                            let guest_x = i32::from(dest.1) - i32::from(record.dest_rect.1) + i32::from(local_x);
                            let current_records = session.runner_mut().text_edit_snapshot().records;
                            let frame = session.video_frame().unwrap();
                            let plans = super::qualify_styled_text_fields(&current_records, &frame.pixels, frame.width, frame.height);
                            if phase == 0 { assert!(plans.iter().any(Option::is_some), "styled paint must qualify before pointer interaction: PPC={powerpc}, depth={depth:?}, override={ppc_depth:?}, scale={scale}, offset={offset}, active={}, intact={}", record.active, record.drawing_intact); }
                            cx.update_window(window.into(), |_, window, cx| {
                                view.update(cx, |demo, cx| {
                                    demo.width = 800;
                                    demo.height = 600;
                                    demo.windows = session.runner_mut().window_frame_snapshot();
                                    demo.controls = session.runner_mut().control_snapshot();
                                    demo.text_edits = current_records.clone();
                                    demo.styled_text_plans = plans.clone();
                                    if phase == 0 {
                                        assert!(!super::super::frames::styled_text_edit_candidates(
                                            &demo.text_edits, &demo.dialogs, &demo.controls, &demo.windows,
                                            super::super::frames::Rect { top: 0, left: 0, bottom: 600, right: 800 },
                                        ).is_empty(), "pointer test requires a GPUI-owned visible field");
                                    }
                                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                                        image::Frame::new(image::RgbaImage::from_raw(frame.width, frame.height, super::gpui_pixels(frame.pixels.clone())).unwrap())
                                    ])));
                                    cx.notify();
                                });
                                window.render_frame(cx);
                                let position = view.update(cx, |demo, _| {
                                    assert!((demo.display_scale - scale).abs() < 0.01,
                                        "expected scale {scale}, got {}, viewport {:?}", demo.display_scale, window.viewport_size());
                                    assert!(demo.display_origin.0 > 0., "exercise centered scene origin");
                                    gpui_kit::point(
                                        gpui_kit::px(demo.display_origin.0 + (guest_x as f32 + 0.25) * scale),
                                        gpui_kit::px(demo.display_origin.1 + (guest_y as f32 + 0.25) * scale))
                                });
                                let event = match phase {
                                    0 => MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                    1 => MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() }.to_platform_input(),
                                    _ => MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                };
                                window.dispatch_event(event, cx);
                            }).unwrap();
                            let mut delivered = 0;
                            for command in receiver.try_iter() {
                                if let super::Command::Input(input) = command {
                                    session.deliver_input(input);
                                    delivered += 1;
                                }
                            }
                            assert_eq!(delivered, 1);
                            for _ in 0..20 { session.runner_mut().run_steps(10_000, None); }
                        }
                        settle(&mut session);
                        let clicked = session.runner_mut().text_edit_snapshot().records.into_iter()
                            .find(|next| next.guest_id == record.guest_id).unwrap();
                        let expected = starts[line_index] + offset;
                        let expected_end = starts[line_index] + drag_end.unwrap_or(offset);
                        assert_eq!(clicked.selection, (expected, expected_end),
                            "PPC={powerpc}, depth={depth:?}, scale={scale}, line={line_index}, drag_end={drag_end:?}");
                    }
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn painted_new_folder_pointer_events_reach_guest_selection(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
            use super::super::activation::{ControlActivation, FileAction};
            cx.update(gpui_kit::init);
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth.unwrap_or(16)).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 12));
                wait_for_menu(&mut session, 129, 12, true);
                settle(&mut session);
                assert_eq!(session.runner().presented_screen_depth(), Some(u32::from(depth.unwrap_or(16))),
                    "interaction matrix must exercise the intended framebuffer depth");

                let step = |session: &mut MacintoshSession| {
                    let tick = session.runner().guest_tick().saturating_add(1);
                    session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                };
                session.deliver_input(MacintoshInput::MouseDown { vertical: 266, horizontal: 400 });
                session.deliver_input(MacintoshInput::MouseUp { vertical: 266, horizontal: 400 });
                let panel = (0..100).find_map(|_| { step(&mut session); session.runner().standard_file_snapshot() }).unwrap();
                let click = ControlActivation::begin_file(&mut session, panel.guest_id, panel.generation, FileAction::NewFolder).unwrap();
                step(&mut session);
                let click = click.advance(&mut session).unwrap();
                step(&mut session);
                assert!(click.advance(&mut session).is_none());
                for name in ["untitled folder", "wwwwwwwwwwwwwwwwwwwwwwwwwwwabcd"] {
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x37, character: 0 });
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character: b'a' });
                    step(&mut session);
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character: b'a' });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x37, character: 0 });
                    for character in name.bytes() {
                        session.deliver_input(MacintoshInput::KeyDown { mac_key: 0, character });
                        step(&mut session);
                        session.deliver_input(MacintoshInput::KeyUp { mac_key: 0, character });
                    }
                    let folder = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                    assert_eq!(folder.name, name);
                    let glyphs = super::super::text::ClassicLine::plain(name.as_bytes(), 0, 12);
                    let origin = folder.insertion_positions[0];
                    for (guest, advance) in folder.insertion_positions.iter().zip(&glyphs.positions) {
                        assert_eq!(i32::from(*guest) - i32::from(origin), *advance,
                            "displayed glyph advance must equal guest insertion advance");
                    }
                for scale in [0.75, 1., 1.5, 2.] {
                    // Restore the tail before opening each viewport; the previous
                    // drag intentionally left a different selection endpoint visible.
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x7d, character: 0x1f });
                    step(&mut session);
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x7d, character: 0x1f });
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let (window, view) = cx.update(|cx| {
                        gpui_kit::open_window(gpui_kit::WindowOptions {
                            // Centered bounds clamp to the mock display, reducing 2x.
                            window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                                gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                                gpui_kit::size(gpui_kit::px(800. * scale), gpui_kit::px(600. * scale))))),
                            ..Default::default()
                        }, cx, |_, cx| cx.new(|cx| super::Demo::new(sender, Default::default(), cx))).unwrap()
                    });
                    // Point clicks followed by a held drag; every coordinate comes
                    // from painted host glyphs, never a guessed guest text width.
                    let short = [
                        (0, 0, (0, 0)), (2, 0, (0, 0)),
                        (0, 1, (1, 1)), (2, 1, (1, 1)),
                        (0, 7, (7, 7)), (2, 7, (7, 7)),
                        (0, 15, (15, 15)), (2, 15, (15, 15)),
                        (0, 6, (6, 6)), (1, 1, (1, 6)), (2, 1, (1, 6)),
                    ];
                    let long = [(0, 31, (31, 31)), (2, 31, (31, 31)),
                        (0, 30, (30, 30)), (2, 30, (30, 30)),
                        (0, 29, (29, 29)), (1, 27, (27, 29)), (2, 27, (27, 29)),
                        (3, 0, (0, 0)), (0, 0, (0, 0)), (2, 0, (0, 0)),
                        (4, 31, (31, 31)), (0, 31, (31, 31)), (2, 31, (31, 31))];
                    for &(phase, offset, expected) in if name.len() == 31 { long.as_slice() } else { short.as_slice() } {
                        cx.update_window(window.into(), |_, window, cx| {
                            view.update(cx, |demo, cx| {
                                demo.width = 800;
                                demo.height = 600;
                                demo.standard_file = session.runner().standard_file_snapshot();
                                if demo.image.is_none() {
                                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                                        image::Frame::new(image::RgbaImage::new(800, 600))
                                    ])));
                                }
                                cx.notify();
                            });
                            window.render_frame(cx);
                            let position = view.update(cx, |demo, _| {
                                assert!((demo.display_scale - scale).abs() < 0.01);
                                let map = demo.text_pointer_map.borrow();
                                let map = map.as_ref().expect("painted name");
                                if phase < 3 {
                                    let left = demo.display_origin.0 + f32::from(map.guest_bounds.1) * scale;
                                    let right = demo.display_origin.0 + f32::from(map.guest_bounds.3) * scale;
                                    assert!(map.positions[offset].0 >= left && map.positions[offset].0 < right,
                                        "target offset {offset} must be painted inside field at scale {scale}");
                                }
                                let y = demo.display_origin.1 + f32::from((map.guest_bounds.0 + map.guest_bounds.2) / 2) * demo.display_scale;
                                gpui_kit::point(gpui_kit::px(map.positions[offset].0), gpui_kit::px(y))
                            });
                            let event = match phase {
                                0 => MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                1 => MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() }.to_platform_input(),
                                3 | 4 => gpui_kit::KeyDownEvent {
                                    keystroke: gpui_kit::Keystroke { key: if phase == 3 { "up" } else { "down" }.into(), ..Default::default() },
                                    is_held: false, prefer_character_input: false,
                                }.to_platform_input(),
                                _ => MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                            };
                            window.dispatch_event(event, cx);
                            if phase >= 3 {
                                window.dispatch_event(gpui_kit::KeyUpEvent {
                                    keystroke: gpui_kit::Keystroke { key: if phase == 3 { "up" } else { "down" }.into(), ..Default::default() },
                                }.to_platform_input(), cx);
                            }
                        }).unwrap();
                        let mut delivered = 0;
                        for command in receiver.try_iter() {
                            if let super::Command::Input(input) = command {
                                session.deliver_input(input);
                                delivered += 1;
                            }
                        }
                        assert_eq!(delivered, if phase >= 3 { 2 } else { 1 });
                        step(&mut session);
                        assert_eq!(session.runner().standard_file_snapshot().unwrap().new_folder.unwrap().selection,
                            expected, "PPC={powerpc}, depth={depth:?}, scale={scale}, phase={phase}, offset={offset}");
                    }
                    if name.len() == 31 {
                        for toward_end in [false, true] {
                            let (mac_key, character, anchor) = if toward_end { (0x7e, 0x1e, 0) } else { (0x7d, 0x1f, 31) };
                            session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
                            step(&mut session);
                            session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
                            for phase in 0..3 {
                                cx.update_window(window.into(), |_, window, cx| {
                                    view.update(cx, |demo, cx| {
                                        demo.standard_file = session.runner().standard_file_snapshot();
                                        cx.notify();
                                    });
                                    window.render_frame(cx);
                                    let position = view.update(cx, |demo, _| {
                                        let map = demo.text_pointer_map.borrow();
                                        let map = map.as_ref().unwrap();
                                        let x = if phase == 0 { map.positions[anchor].0 } else {
                                            demo.display_origin.0 + f32::from(if toward_end {
                                                map.guest_bounds.3 + 20
                                            } else { map.guest_bounds.1 - 20 }) * scale
                                        };
                                        let y = demo.display_origin.1 + f32::from((map.guest_bounds.0 + map.guest_bounds.2) / 2) * scale;
                                        gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y))
                                    });
                                    let event = match phase {
                                        0 => MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                        1 => MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() }.to_platform_input(),
                                        _ => MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }.to_platform_input(),
                                    };
                                    window.dispatch_event(event, cx);
                                }).unwrap();
                                let mut delivered = 0;
                                for command in receiver.try_iter() {
                                    if let super::Command::Input(input) = command {
                                        session.deliver_input(input);
                                        delivered += 1;
                                    }
                                }
                                assert_eq!(delivered, 1);
                                // No additional motion: the guest's retained click loop
                                // must continue scrolling under a stationary held pointer.
                                for _ in 0..if phase == 1 { 20 } else { 1 } { step(&mut session); }
                                let folder = session.runner().standard_file_snapshot().unwrap().new_folder.unwrap();
                                assert_eq!(folder.selection, if phase == 0 { (anchor, anchor) } else { (0, 31) },
                                    "held scroll: PPC={powerpc}, depth={depth:?}, scale={scale}, toward_end={toward_end}, phase={phase}");
                            }
                        }
                    }
                }
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn gpui_popup_pointer_events_reach_guest_tracker(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{
                test::TestWindowExt, AppContext, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
                MouseUpEvent,
            };
            cx.update(gpui_kit::init);
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session
                    .load_path(
                        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/toolbox-showcase/toolbox-showcase.sit"),
                    )
                    .unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 16));
                wait_for_menu(&mut session, 129, 16, true);
                settle(&mut session);
                assert_eq!(session.runner().presented_screen_depth(), Some(u32::from(depth)));
                for scale in [0.75, 1., 1.5, 2.] {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let (window, view) = cx.update(|cx| {
                        gpui_kit::open_window(gpui_kit::WindowOptions {
                            window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                                gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                                gpui_kit::size(gpui_kit::px(800. * scale), gpui_kit::px(600. * scale))))),
                            ..Default::default()
                        }, cx, |_, cx| {
                            cx.new(|cx| super::Demo::new(sender, Default::default(), cx))
                        })
                        .unwrap()
                    });
                    let (top, left, _, _) = session.runner_mut().window_bounds();
                    let mut point = (top + 112, left + 280);
                    for phase in 0..3 {
                        let popup = session.runner_mut().guest_popup_snapshot();
                        if phase > 0 {
                            let popup = popup
                                .as_ref()
                                .expect("guest popup should remain open while held");
                            point = (
                                popup.content_top
                                    + popup.row_heights[..3].iter().sum::<i16>()
                                    + popup.row_heights[3] / 2,
                                popup.bounds.1 + 30,
                            );
                        }
                        cx.update_window(window.into(), |_, window, cx| {
                            view.update(cx, |demo, _| {
                                demo.width = 800;
                                demo.height = 600;
                                if demo.image.is_none() {
                                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                                        image::Frame::new(image::RgbaImage::new(800, 600))
                                    ])));
                                }
                                demo.controls = session.runner_mut().control_snapshot();
                                demo.windows = session.runner_mut().window_frame_snapshot();
                                demo.menus = session.runner_mut().guest_menu_snapshot();
                                demo.guest_popup = popup;
                            });
                            window.render_frame(cx);
                            let position = view.update(cx, |demo, _| {
                                assert!((demo.display_scale - scale).abs() < 0.001, "popup test must render at requested scale");
                                gpui_kit::point(
                                    gpui_kit::px(
                                        demo.display_origin.0
                                            + (f32::from(point.1) + 0.25) * demo.display_scale,
                                    ),
                                    gpui_kit::px(
                                        demo.display_origin.1
                                            + (f32::from(point.0) + 0.25) * demo.display_scale,
                                    ),
                                )
                            });
                            let event = match phase {
                                0 => MouseDownEvent {
                                    position,
                                    button: MouseButton::Left,
                                    click_count: 1,
                                    ..Default::default()
                                }
                                .to_platform_input(),
                                1 => MouseMoveEvent {
                                    position,
                                    pressed_button: Some(MouseButton::Left),
                                    ..Default::default()
                                }
                                .to_platform_input(),
                                _ => MouseUpEvent {
                                    position,
                                    button: MouseButton::Left,
                                    click_count: 1,
                                    ..Default::default()
                                }
                                .to_platform_input(),
                            };
                            window.dispatch_event(event, cx);
                        })
                        .unwrap();
                        let inputs: Vec<_> = receiver
                            .try_iter()
                            .filter_map(|command| match command {
                                super::Command::Input(input) => Some(input),
                                _ => None,
                            })
                            .collect();
                        assert!(
                            inputs.iter().any(|input| match input {
                                MacintoshInput::MouseDown {
                                    vertical,
                                    horizontal,
                                } if phase == 0 => (*vertical, *horizontal) == point,
                                MacintoshInput::MouseMove {
                                    vertical,
                                    horizontal,
                                } if phase == 1 => (*vertical, *horizontal) == point,
                                MacintoshInput::MouseUp {
                                    vertical,
                                    horizontal,
                                } if phase == 2 => (*vertical, *horizontal) == point,
                                _ => false,
                            }),
                            "GPUI phase {phase} must reach guest coordinates on {powerpc:?}/{depth:?}"
                        );
                        for input in inputs {
                            session.deliver_input(input);
                        }
                        for _ in 0..20 {
                            session.runner_mut().run_steps(50_000, None);
                        }
                        if phase == 1 {
                            assert_eq!(
                                session
                                    .runner_mut()
                                    .guest_popup_snapshot()
                                    .unwrap()
                                    .highlighted_item,
                                4
                            );
                        }
                    }
                    assert!((0..100).any(|_| {
                        session.runner_mut().run_steps(50_000, None);
                        session
                            .runner_mut()
                            .control_snapshot()
                            .iter()
                            .any(|control| {
                                control.visible && control.popup_menu_id == Some(143) && control.value == 4
                            })
                    }));
                    assert!(session.runner_mut().guest_popup_snapshot().is_none());
                }
            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn gpui_wheel_event_reaches_guest_scrollbar(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{test::TestWindowExt, AppContext, InputEvent, ScrollDelta, ScrollWheelEvent, TouchPhase};
            let mut session = MacintoshSession::new(true, Some(8));
            let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
            session.initialize(&app);
            wait_for_menu(&mut session, 129, 1, true);
            assert!(session.runner_mut().select_guest_menu_item(129, 2));
            wait_for_menu(&mut session, 129, 2, true);
            let (sender, receiver) = std::sync::mpsc::channel();
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(Default::default(), cx, |_, cx| {
                    cx.new(|cx| super::Demo::new(sender, Default::default(), cx))
                }).unwrap()
            });
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, _| {
                    demo.width = 800;
                    demo.height = 600;
                    demo.controls = session.runner_mut().control_snapshot();
                    demo.windows = session.runner_mut().window_frame_snapshot();
                    demo.menus = session.runner_mut().guest_menu_snapshot();
                });
                window.render_frame(cx);
                let position = view.update(cx, |demo, _| gpui_kit::point(
                    gpui_kit::px(demo.display_origin.0 + 300. * demo.display_scale),
                    gpui_kit::px(demo.display_origin.1 + 368. * demo.display_scale),
                ));
                let event = ScrollWheelEvent {
                    position, delta: ScrollDelta::Lines(gpui_kit::point(-0.5, 0.)),
                    modifiers: Default::default(), touch_phase: TouchPhase::Moved,
                };
                window.dispatch_event(event.clone().to_platform_input(), cx);
                window.dispatch_event(event.clone().to_platform_input(), cx);
                let requests: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                    super::Command::Wheel(request) => Some(request), _ => None,
                }).collect();
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].steps, 1);
                assert_eq!(requests[0].origin, (368, 300));
                view.update(cx, |demo, _| demo.mouse_down = true);
                window.dispatch_event(event.to_platform_input(), cx);
                assert!(!receiver.try_iter().any(|command| matches!(command, super::Command::Wheel(_))));
                view.update(cx, |demo, _| { demo.mouse_down = false; demo.release_host_input(); });
                assert!(receiver.try_iter().any(|command| matches!(command, super::Command::CancelWheel)));
            }).unwrap();
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
                let mut released = key;
                released.key_char = Some("あ".into());
                window.dispatch_event(KeyUpEvent { keystroke: released }.to_platform_input(), cx);
            }).unwrap();
            let inputs: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                _ => None,
            }).collect();
            assert!(matches!(inputs.as_slice(), [
                MacintoshInput::KeyDown { mac_key: 0x00, character: b'A' },
                MacintoshInput::KeyUp { mac_key: 0x00, character: b'A' },
            ]));
            cx.update_window(window.into(), |_, window, cx| {
                for _ in 0..2 {
                    window.dispatch_event(ModifiersChangedEvent {
                        modifiers: gpui_kit::Modifiers {
                            shift: true, alt: true, control: true,
                            ..Default::default()
                        },
                        capslock: gpui_kit::Capslock { on: false },
                    }.to_platform_input(), cx);
                }
                view.update(cx, |demo, _| {
                    demo.release_host_input();
                    demo.release_host_input();
                });
            }).unwrap();
            let modifiers: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                _ => None,
            }).collect();
            assert!(matches!(modifiers.as_slice(), [
                MacintoshInput::KeyDown { mac_key: 0x38, character: 0 },
                MacintoshInput::KeyDown { mac_key: 0x3a, character: 0 },
                MacintoshInput::KeyDown { mac_key: 0x3b, character: 0 },
                MacintoshInput::KeyUp { mac_key: 0x38, character: 0 },
                MacintoshInput::KeyUp { mac_key: 0x3a, character: 0 },
                MacintoshInput::KeyUp { mac_key: 0x3b, character: 0 },
            ]));
            cx.update_window(window.into(), |_, window, cx| {
                for on in [true, true, false] {
                    window.dispatch_event(ModifiersChangedEvent {
                        modifiers: Default::default(),
                        capslock: gpui_kit::Capslock { on },
                    }.to_platform_input(), cx);
                    view.update(cx, |demo, _| demo.release_host_input());
                }
            }).unwrap();
            let caps: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input), _ => None,
            }).collect();
            assert!(matches!(caps.as_slice(), [
                MacintoshInput::KeyDown { mac_key: 0x39, character: 0 },
                MacintoshInput::KeyUp { mac_key: 0x39, character: 0 },
                MacintoshInput::KeyDown { mac_key: 0x39, character: 0 },
                MacintoshInput::KeyUp { mac_key: 0x39, character: 0 },
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
                                mark: 0,
                                style: 0,
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
        fn guest_list_mutation_and_resize_refresh_qualified_paint() {
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                settle(&mut session);
                let initial = session.runner_mut().list_manager_snapshot().remove(0);
                let global = initial.global_view_rect.unwrap();
                for input in [MacintoshInput::MouseDown { vertical: global.0 + 9, horizontal: global.1 + 12 },
                    MacintoshInput::MouseUp { vertical: global.0 + 9, horizontal: global.1 + 12 }] {
                    session.deliver_input(input);
                    settle(&mut session);
                }
                let selected = session.runner_mut().list_manager_snapshot().remove(0);
                assert_eq!(selected.selected, [(0, 0)].into());
                let mut expected = selected.cells[&(0, 0)].clone();
                expected.extend_from_slice(b"  * updated");
                for (title, bounds) in [("Update Selected Row", (78, 24, 228, 528)),
                    ("Resize List", (78, 24, 192, 474)), ("Resize List", (78, 24, 228, 528))] {
                    let control = session.runner_mut().control_snapshot().into_iter()
                        .find(|control| control.visible && control.title == title).unwrap();
                    let point = ((control.bounds.0 + control.bounds.2) / 2,
                        (control.bounds.1 + control.bounds.3) / 2);
                    for input in [MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 },
                        MacintoshInput::MouseUp { vertical: point.0, horizontal: point.1 }] {
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                    let current = session.runner_mut().list_manager_snapshot().remove(0);
                    assert_eq!((current.guest_id, current.generation, current.owner_port),
                        (selected.guest_id, selected.generation, selected.owner_port));
                    assert_eq!(current.selected, selected.selected);
                    assert_eq!(current.view_rect, bounds);
                    assert_eq!(current.cells[&(0, 0)], expected);
                    assert_eq!(current.standard_cell_paint[&(0, 0)].bytes, expected);
                    let frame = session.video_frame().unwrap();
                    let plans = super::qualify_list_text_fields(std::slice::from_ref(&current),
                        &frame.pixels, frame.width, frame.height);
                    assert!(plans[0].contains_key(&(0, 0)),
                        "updated native row must qualify: {title}, PPC={powerpc}, depth={depth}");
                }
            }
        }

        #[test]
        fn guest_quit_removes_list_presentation_on_all_display_modes() {
            for (powerpc, depth) in [(false, 1), (false, 8), (true, 8), (true, 16)] {
                let mut session = MacintoshSession::new(true, if powerpc { None } else { Some(depth) });
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 9));
                wait_for_menu(&mut session, 129, 9, true);
                settle(&mut session);
                let lists = session.runner_mut().list_manager_snapshot();
                assert_eq!(lists.len(), 1);
                assert!(lists[0].draw_enabled);
                assert!(!lists[0].standard_cell_paint.is_empty());
                let controls: Vec<_> = session.runner_mut().control_snapshot().into_iter()
                    .filter(|control| control.proc_id == 16 && control.visible && control.owner_id == lists[0].owner_port)
                    .collect();
                assert!(!controls.is_empty());
                assert!(session.runner_mut().select_guest_menu_item(131, 4));
                let removed = (0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().list_manager_snapshot().is_empty()
                });
                assert!(removed, "guest Quit removes lists: PPC={powerpc}, depth={depth}");
                let remaining = session.runner_mut().control_snapshot();
                assert!(remaining.iter().all(|current| controls.iter().all(|old|
                    current.guest_id != old.guest_id || current.generation != old.generation)),
                    "disposed list controls cannot remain in presentation snapshots");
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
                let layout = dialog.items[1].static_text_layout.as_ref().expect("standard alert text has guest layout");
                assert_eq!(layout.origin.0, 1);
                assert_eq!(layout.inclusive_bottom, !powerpc);
                let metrics = systemless::quickdraw::text::get_font_metrics(layout.font.0, layout.font.1);
                assert_eq!(layout.line_height, if powerpc { 16 } else { metrics.ascent + metrics.descent + metrics.leading });
                assert_eq!(layout.origin.1, if powerpc { 12 } else { metrics.ascent });

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
                    static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                    edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                    control_identity: None,
                    pressed: false,
                    number: 3,
                    kind: DialogItemKind::EditText,
                    bounds: (250, 320, 270, 430),
                    text: "Pilot".into(),
                    enabled: true,
                    visible: true,
                    value: None,
                    selection: None,
                    caret_visible: Some(true),
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
        fn semantic_dialog_activation_tracks_guest_checkbox_across_modes() {
            use systemless::memory::MemoryBus;
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
                let item = dialog.items.iter().find(|item| item.kind == DialogItemKind::Checkbox).unwrap();
                let control_identity = item.control_identity.expect("dialog controls require a guest lifetime");
                assert_ne!(control_identity.0, 0);
                assert_ne!(control_identity.1, 0);
                let (id, generation, number) = (dialog.guest_id, dialog.generation, item.number);
                let identity = item.control_identity;
                if !powerpc {
                    let pointer = session.runner().bus().read_long(control_identity.0);
                    session.runner_mut().bus_mut().write_long(control_identity.0, 0);
                    let invalid = session.runner_mut().dialog_snapshot();
                    assert_eq!(invalid.iter().find(|d| d.guest_id == id).unwrap().items.iter()
                        .find(|i| i.number == number).unwrap().control_identity, None);
                    assert!(super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, identity).is_none());
                    session.runner_mut().bus_mut().write_long(control_identity.0, pointer);
                    // Exercise a changed live DITL slot without changing the dialog lifetime.
                    // Macintosh Toolbox Essentials (1992), pp. 6-122--6-123.
                    let replacement = dialog.items.iter().find(|other|
                        other.number != number && other.kind == DialogItemKind::Checkbox
                    ).unwrap().control_identity.unwrap();
                    let bus = session.runner().bus();
                    let ditl = bus.read_long(bus.read_long(id + 156));
                    let mut entry = ditl + 2;
                    for _ in 1..number {
                        let length = u32::from(bus.read_byte(entry + 13));
                        entry += 14 + ((length + 1) & !1);
                    }
                    assert_eq!(bus.read_long(entry), control_identity.0);
                    let origin = session.runner().dispatcher().mouse_position();
                    session.runner_mut().bus_mut().write_long(entry, replacement.0);
                    let replaced = session.runner_mut().dialog_snapshot();
                    assert_eq!(replaced.iter().find(|d| d.guest_id == id).unwrap().items.iter()
                        .find(|i| i.number == number).unwrap().control_identity, Some(replacement));
                    assert!(super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, identity).is_none());
                    assert_eq!(session.runner().dispatcher().mouse_position(), origin);
                    session.runner_mut().bus_mut().write_long(entry, control_identity.0);
                }
                let before = session.runner().dispatcher().mouse_position();
                assert!(super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, None).is_none());
                assert!(super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, Some((control_identity.0, control_identity.1 + 1))).is_none());
                assert_eq!(session.runner().dispatcher().mouse_position(), before);
                assert!(super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation + 1, number, identity).is_none());
                let click = super::super::activation::ControlActivation::begin_dialog(&mut session, id, generation, number, identity).unwrap();
                settle(&mut session);
                let held = session.runner_mut().dialog_snapshot();
                let item = held.iter().find(|d| d.guest_id == id).unwrap().items.iter().find(|i| i.number == number).unwrap();
                assert_eq!(item.value, Some(0));
                assert!(item.pressed);
                let click = click.advance(&mut session).unwrap();
                settle(&mut session);
                assert!(click.advance(&mut session).is_none());
                let updated = session.runner_mut().dialog_snapshot();
                let item = updated.iter().find(|d| d.guest_id == id).unwrap().items.iter().find(|i| i.number == number).unwrap();
                assert_eq!(item.value, Some(1));
                assert_eq!(item.control_identity, identity);
                assert!(!item.pressed);
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
                assert!(dialog.items[6].caret_visible.is_some(), "active dialog TERec must expose its blink phase");
                let initial_phase = dialog.items[6].caret_visible;
                let before_blink = session.video_frame().unwrap();
                assert!((0..65).any(|_| {
                    session.runner_mut().force_advance_guest_tick();
                    session.runner_mut().run_steps(1_000, None);
                    let current = session.runner_mut().dialog_snapshot().into_iter()
                        .find(|current| current.guest_id == dialog.guest_id).unwrap();
                    assert_eq!(current.items[6].selection, Some((0, 0)));
                    assert_eq!(current.items[6].text, dialog.items[6].text);
                    current.items[6].caret_visible != initial_phase
                }), "dialog caret did not blink: PPC={powerpc}, depth={depth:?}");
                let after_blink = session.video_frame().unwrap();
                assert_eq!((before_blink.width, before_blink.height), (after_blink.width, after_blink.height));
                let (top, left, bottom, right) = dialog.items[6].bounds;
                let mut changed = 0;
                for (index, (before, after)) in before_blink.pixels.chunks_exact(4)
                    .zip(after_blink.pixels.chunks_exact(4)).enumerate() {
                    if before != after {
                        changed += 1;
                        let y = (index as u32 / before_blink.width) as i16;
                        let x = (index as u32 % before_blink.width) as i16;
                        assert!(y >= top && y < bottom && x >= left && x < right,
                            "blink changed unrelated pixel ({x},{y}): PPC={powerpc}, depth={depth:?}");
                    }
                }
                assert!(changed > 0, "caret phase changed without pixels: PPC={powerpc}, depth={depth:?}");
                assert!((0..65).any(|_| {
                    session.runner_mut().force_advance_guest_tick();
                    session.runner_mut().run_steps(1_000, None);
                    session.runner_mut().dialog_snapshot().into_iter()
                        .find(|current| current.guest_id == dialog.guest_id).unwrap()
                        .items[6].caret_visible == initial_phase
                }), "dialog caret failed to return to its original phase");
                assert!(session.video_frame().unwrap().pixels == before_blink.pixels,
                    "blink cycle did not restore original pixels: PPC={powerpc}, depth={depth:?}");


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
                let held = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|current| current.guest_id == dialog.guest_id).unwrap();
                let held_checkbox = held.items.iter().find(|item| item.number == checkbox.number).unwrap();
                assert_eq!(held_checkbox.value, Some(0), "checkbox changed before release: PPC={powerpc}, depth={depth:?}");
                assert!(held_checkbox.pressed, "held checkbox lacks guest highlight: PPC={powerpc}, depth={depth:?}");
                let outside_checkbox = (checkbox.bounds.0 - 5, checkbox.bounds.1 - 5);
                session.deliver_input(MacintoshInput::MouseMove { vertical: outside_checkbox.0, horizontal: outside_checkbox.1 });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp { vertical: outside_checkbox.0, horizontal: outside_checkbox.1 });
                settle(&mut session);
                let cancelled = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|current| current.guest_id == dialog.guest_id).unwrap();
                let cancelled_checkbox = cancelled.items.iter().find(|item| item.number == checkbox.number).unwrap();
                assert_eq!(cancelled_checkbox.value, Some(0), "outside release toggled checkbox: PPC={powerpc}, depth={depth:?}");
                assert!(!cancelled_checkbox.pressed);
                session.deliver_input(MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 });
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
                // HIG (1992), p. 205: a held button tracks the pointer;
                // releasing outside cancels the action.
                let cancel = dialog.items.iter().find(|item| item.kind == DialogItemKind::Button && item.text == "Cancel").unwrap();
                let point = ((cancel.bounds.0 + cancel.bounds.2) / 2,
                    (cancel.bounds.1 + cancel.bounds.3) / 2);
                session.deliver_input(MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 });
                settle(&mut session);
                assert!(session.runner_mut().dialog_snapshot().iter().any(|current| {
                    current.guest_id == dialog.guest_id && current.generation == dialog.generation && current.visible
                }), "modal Cancel fired before release: PPC={powerpc}, depth={depth:?}");
                assert!(session.runner_mut().dialog_snapshot().iter()
                    .find(|current| current.guest_id == dialog.guest_id).unwrap()
                    .items.iter().find(|item| item.number == cancel.number).unwrap().pressed,
                    "held modal button must expose its guest highlight: PPC={powerpc}");
                let outside = (cancel.bounds.0 - 10, cancel.bounds.1 - 10);
                session.deliver_input(MacintoshInput::MouseMove { vertical: outside.0, horizontal: outside.1 });
                settle(&mut session);
                assert!(!session.runner_mut().dialog_snapshot().iter()
                    .find(|current| current.guest_id == dialog.guest_id).unwrap()
                    .items.iter().find(|item| item.number == cancel.number).unwrap().pressed,
                    "modal highlight must clear outside the button: PPC={powerpc}");
                session.deliver_input(MacintoshInput::MouseUp { vertical: outside.0, horizontal: outside.1 });
                settle(&mut session);
                assert!(session.runner_mut().dialog_snapshot().iter().any(|current| {
                    current.guest_id == dialog.guest_id && current.generation == dialog.generation && current.visible
                }), "modal Cancel ignored outside release: PPC={powerpc}, depth={depth:?}");
                session.deliver_input(MacintoshInput::MouseDown { vertical: point.0, horizontal: point.1 });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp { vertical: point.0, horizontal: point.1 });
                assert!((0..100).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    !session.runner_mut().dialog_snapshot().iter().any(|current| {
                        current.guest_id == dialog.guest_id && current.generation == dialog.generation && current.visible
                    })
                }), "modal Cancel failed after inside release: PPC={powerpc}, depth={depth:?}");
            }
        }

        #[test]
        fn modal_dialog_edit_fields_switch_and_keep_independent_text_across_modes() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(132, 6));
                settle(&mut session);
                let dialog = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.visible && dialog.items.len() == 10).unwrap();
                let id = dialog.guest_id;
                let mut expected = ["Cade Connelly".to_string(), "Maverick".to_string()];
                for (index, slot) in [(8, 1), (6, 0), (8, 1)] {
                    let current = session.runner_mut().dialog_snapshot().into_iter()
                        .find(|dialog| dialog.guest_id == id).unwrap();
                    let bounds = current.items[index].bounds;
                    for input in [
                        MacintoshInput::MouseDown { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 },
                        MacintoshInput::MouseUp { vertical: bounds.0 + 5, horizontal: bounds.1 + 1 },
                    ] {
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                    let current = session.runner_mut().dialog_snapshot().into_iter()
                        .find(|dialog| dialog.guest_id == id).unwrap();
                    assert_eq!(current.items[index].selection, Some((0, 0)), "mode={powerpc}/{depth:?} field={index}");
                    assert!(current.items[index].edit_text_layout.is_some(), "active field must retain renderable guest geometry");
                    session.deliver_input(MacintoshInput::KeyDown { mac_key: 6, character: b'z' });
                    session.deliver_input(MacintoshInput::KeyUp { mac_key: 6, character: b'z' });
                    settle(&mut session);
                    expected[slot].insert(0, 'z');
                    let current = session.runner_mut().dialog_snapshot().into_iter()
                        .find(|dialog| dialog.guest_id == id).unwrap();
                    assert_eq!(current.items[6].text, expected[0]);
                    assert_eq!(current.items[8].text, expected[1]);
                }
                let current = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.guest_id == id).unwrap();
                let field = &current.items[8];
                let layout = field.edit_text_layout.as_ref().unwrap();
                let line = super::super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
                let x = field.bounds.1 + 1 + line.positions[3] as i16;
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x38, character: 0 });
                for input in [
                    MacintoshInput::MouseDown { vertical: field.bounds.0 + 5, horizontal: x },
                    MacintoshInput::MouseUp { vertical: field.bounds.0 + 5, horizontal: x },
                ] {
                    session.deliver_input(input);
                    settle(&mut session);
                }
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x38, character: 0 });
                settle(&mut session);
                let current = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.guest_id == id).unwrap();
                assert_eq!(current.items[8].selection, Some((1, 3)), "Shift click mode={powerpc}/{depth:?}");
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x38, character: 0 });
                for input in [
                    MacintoshInput::MouseDown { vertical: field.bounds.0 + 5, horizontal: field.bounds.1 + 1 },
                    MacintoshInput::MouseUp { vertical: field.bounds.0 + 5, horizontal: field.bounds.1 + 1 },
                ] {
                    session.deliver_input(input);
                    settle(&mut session);
                }
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x38, character: 0 });
                settle(&mut session);
                let current = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.guest_id == id).unwrap();
                assert_eq!(current.items[8].selection, Some((0, 3)), "reverse Shift click mode={powerpc}/{depth:?}");
                for input in [
                    MacintoshInput::MouseDown { vertical: field.bounds.0 + 5, horizontal: field.bounds.1 + 1 + line.positions[1] as i16 },
                    MacintoshInput::MouseMove { vertical: field.bounds.0 + 5, horizontal: field.bounds.1 + 1 + line.positions[4] as i16 },
                    MacintoshInput::MouseUp { vertical: field.bounds.0 + 5, horizontal: field.bounds.1 + 1 + line.positions[4] as i16 },
                ] {
                    session.deliver_input(input);
                    for _ in 0..10 {
                        let tick = session.runner().guest_tick().saturating_add(1);
                        session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                    }
                }
                settle(&mut session);
                let current = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.guest_id == id).unwrap();
                assert_eq!(current.items[8].selection, Some((1, 4)), "held drag mode={powerpc}/{depth:?}");
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 7, character: b'X' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 7, character: b'X' });
                settle(&mut session);
                expected[1].replace_range(1..4, "X");
                let current = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.guest_id == id).unwrap();
                assert_eq!(current.items[8].text, expected[1], "typing must resume after drag release");
                assert_eq!(current.items[6].text, expected[0]);
            }
        }

        #[test]
        fn dialog_field_highlight_geometry_matches_guest_pixels() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(132, 7));
                settle(&mut session);
                let dialog = session.runner_mut().dialog_snapshot().into_iter()
                    .find(|dialog| dialog.visible && dialog.items.len() == 4).unwrap();
                let bounds = dialog.items[3].bounds;
                let layout = dialog.items[3].edit_text_layout.as_ref().unwrap();
                let line = super::super::text::ClassicLine::unicode("Pilot", layout.font.0, layout.font.1);
                for (anchor, endpoint, extend, expected_selection) in [
                    (0, 5, false, (0, 5)), (1, 3, false, (1, 3)),
                    (3, 1, false, (1, 3)), (0, 0, true, (0, 3)),
                ] {
                    if extend { session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x38, character: 0 }); }
                    let start_x = bounds.1 + 1 + line.positions[anchor] as i16;
                    let end_x = bounds.1 + 1 + line.positions[endpoint] as i16;
                    for input in [
                        MacintoshInput::MouseDown { vertical: bounds.0 + 5, horizontal: start_x },
                        MacintoshInput::MouseMove { vertical: bounds.0 + 5, horizontal: end_x },
                        MacintoshInput::MouseUp { vertical: bounds.0 + 5, horizontal: end_x },
                    ] {
                        session.deliver_input(input);
                        for _ in 0..10 {
                            let tick = session.runner().guest_tick().saturating_add(1);
                            session.runner_mut().run_gui_slice_with_audio(100_000, tick, 0);
                        }
                    }
                    if extend { session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x38, character: 0 }); }
                    settle(&mut session);
                    let selected = session.runner_mut().dialog_snapshot().into_iter()
                        .find(|next| next.guest_id == dialog.guest_id).unwrap();
                    let item = &selected.items[3];
                    assert_eq!(item.text, "Pilot");
                    assert_eq!(item.selection, Some((expected_selection.0 as i16, expected_selection.1 as i16)), "PPC={powerpc}, depth={depth:?}");
                    let layout = item.edit_text_layout.as_ref().unwrap();
                    let line = super::super::text::ClassicLine::unicode(&item.text, layout.font.0, layout.font.1);
                    let geometry = super::super::text::dialog_field_geometry(&line, 5, layout, expected_selection,
                        true, true, i32::from(bounds.3 - bounds.1), i32::from(bounds.2 - bounds.0));
                    assert!(geometry.caret.is_none());
                    let highlight = geometry.selection.unwrap();
                    let mut ink = std::collections::HashSet::new();
                    for &(x, y, width) in &line.ink {
                        ink.extend((x..x + width).map(|px| (px + 1, y + i32::from(layout.baseline))));
                    }
                    let frame = session.video_frame().unwrap();
                    for y in 0..i32::from(bounds.2 - bounds.0) {
                        for x in 0..i32::from(bounds.3 - bounds.1) {
                            let selected = y >= highlight.0 && y < highlight.2 && x >= highlight.1 && x < highlight.3;
                            let expected = ink.contains(&(x, y)) ^ selected;
                            let offset = (((y + i32::from(bounds.0)) as u32 * frame.width
                                + (x + i32::from(bounds.1)) as u32) * 4) as usize;
                            let actual = frame.pixels[offset..offset + 3].iter().all(|value| *value < 128);
                            assert_eq!(actual, expected, "dialog field pixel {x},{y}, PPC={powerpc}, depth={depth:?}");
                        }
                    }
                }
                session.deliver_input(MacintoshInput::KeyDown { mac_key: 0x06, character: b'z' });
                session.deliver_input(MacintoshInput::KeyUp { mac_key: 0x06, character: b'z' });
                settle(&mut session);
                assert!(session.runner_mut().dialog_snapshot().iter().any(|next| {
                    next.guest_id == dialog.guest_id && next.items[3].text == "zot"
                }), "release must restore guest editing, PPC={powerpc}, depth={depth:?}");
            }
        }

        #[test]
        fn modeless_dialog_tracks_guest_lifecycle_on_both_cpus() {
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
                let edit_layout = dialog.items[3].edit_text_layout.as_ref()
                    .expect("ordinary dialog field must expose guest drawing geometry");
                assert!(edit_layout.line_height > 0 && edit_layout.baseline > 0);
                if powerpc {
                    assert!(edit_layout.text_edit_geometry,
                        "active PPC dialog field must use the live TERec geometry");
                } else {
                    let metrics = systemless::quickdraw::text::get_font_metrics(edit_layout.font.0, edit_layout.font.1);
                    assert_eq!(edit_layout.baseline, metrics.ascent);
                    assert_eq!(edit_layout.line_height, metrics.ascent + metrics.descent + metrics.leading);
                    assert!(!edit_layout.text_edit_geometry && !edit_layout.wrap);
                }
                assert!(!super::super::frames::dialog_item_pieces(
                    &[dialog.clone()],
                    &windows,
                    super::super::frames::Rect::from((0, 0, 600, 800)),
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
        fn showcase_text_selection_survives_host_suspend_resume() {
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
                settle(&mut session);
                for input in [
                    MacintoshInput::MouseDown {
                        vertical: 132,
                        horizontal: 75,
                    },
                    MacintoshInput::MouseMove {
                        vertical: 132,
                        horizontal: 150,
                    },
                    MacintoshInput::MouseUp {
                        vertical: 132,
                        horizontal: 150,
                    },
                ] {
                    session.deliver_input(input);
                    settle(&mut session);
                }
                let text = session
                    .runner_mut()
                    .text_edit_snapshot()
                    .records
                    .into_iter()
                    .find(|record| record.view_rect == (76, 34, 211, 326))
                    .unwrap();
                assert!(text.active);
                assert_ne!(text.selection.0, text.selection.1);
                if !powerpc {
                    assert_eq!(
                        app.size_resource.unwrap().flags & 0x4800,
                        0x4800,
                        "the fixture must handle its own activation from osEvt"
                    );
                }
                for foreground in [false, true, false, true] {
                    session.request_foreground(foreground);
                    session.request_foreground(foreground);
                    let message = 0x0100_0000 | u32::from(foreground);
                    let mut saw_os_event = false;
                    assert!((0..10_000).any(|_| {
                                session.runner_mut().run_steps(100, None);
                                saw_os_event |= session.runner().event_manager_snapshot().last_record
                                    .is_some_and(|event| event.what == 15 && event.message == message);
                                saw_os_event && session.runner_mut().text_edit_snapshot().records.iter()
                                    .any(|record| record.guest_id == text.guest_id && record.active == foreground)
                            }), "host transition was not handled: PPC={powerpc}, depth={depth:?}, foreground={foreground}, osEvt={saw_os_event}");
                    let frames = session.runner_mut().window_frame_snapshot();
                    assert_eq!(frames.iter().any(|frame| frame.window.visible && frame.window.active),
                        foreground, "presentation activation must follow guest HiliteWindow: PPC={powerpc}, depth={depth:?}");
                    let current = session
                        .runner_mut()
                        .text_edit_snapshot()
                        .records
                        .into_iter()
                        .find(|record| record.guest_id == text.guest_id)
                        .unwrap();
                    assert_eq!(current.selection, text.selection);
                    assert_eq!(current.text, text.text);
                }
                let mut expected = text.text.clone();
                expected.splice(text.selection.0 as usize..text.selection.1 as usize, [b'z']);
                session.deliver_input(MacintoshInput::KeyDown {
                    mac_key: 0x06,
                    character: b'z',
                });
                session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: 0x06,
                    character: b'z',
                });
                assert!(
                    (0..100).any(|_| {
                        session.runner_mut().run_steps(10_000, None);
                        session
                            .runner_mut()
                            .text_edit_snapshot()
                            .records
                            .iter()
                            .any(|record| record.guest_id == text.guest_id && record.text == expected)
                    }),
                    "typing must resume at the retained selection: PPC={powerpc}, depth={depth:?}, expected={expected:?}, actual={:?}, event={:?}", session.runner_mut().text_edit_snapshot(), session.runner().event_manager_snapshot()
                );
            }
        }

        #[test]
        fn showcase_host_clipboard_resume_converts_private_scrap() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 7));
                wait_for_menu(&mut session, 129, 7, true);
                settle(&mut session);
                let before = session.runner_mut().text_edit_snapshot();
                let mut clipboard = super::super::clipboard::HostClipboard::default();
                let host = Some("café\r\nsecond line".to_owned());
                let expected = b"caf\x8e\rsecond line".to_vec();
                for changed in [true, false] {
                    session.request_foreground(false);
                    let mut saw_suspend = false;
                    assert!((0..10_000).any(|_| {
                        session.runner_mut().run_steps(100, None);
                        saw_suspend |= session.runner().event_manager_snapshot().last_record
                            .is_some_and(|event| event.what == 15 && event.message == 0x0100_0000);
                        saw_suspend && session.runner_mut().text_edit_snapshot().records.iter()
                            .all(|record| !record.active)
                    }), "suspend: PPC={powerpc}, depth={depth:?}");
                    // Advance to the next guest yield, where suspend handling
                    // (including any private-to-global conversion) is done.
                    for _ in 0..100 {
                        session.runner_mut().run_steps(100, None);
                    }
                    let imported = clipboard.changed_text(host.clone());
                    assert_eq!(imported.is_some(), changed);
                    if let Some(text) = imported {
                        session.import_clipboard_text(text);
                    }
                    session.request_foreground(true);
                    let message = if changed { 0x0100_0003 } else { 0x0100_0001 };
                    let mut saw_resume = false;
                    assert!((0..10_000).any(|_| {
                        session.runner_mut().run_steps(100, None);
                        saw_resume |= session.runner().event_manager_snapshot().last_record
                            .is_some_and(|event| event.what == 15 && event.message == message);
                        let snapshot = session.runner_mut().text_edit_snapshot();
                        saw_resume && snapshot.private_scrap == expected
                            && snapshot.records.iter().any(|record| record.active)
                    }), "resume conversion: PPC={powerpc}, depth={depth:?}, changed={changed}");
                    let after = session.runner_mut().text_edit_snapshot();
                    assert_eq!(after.records.len(), before.records.len());
                    for (old, new) in before.records.iter().zip(&after.records) {
                        assert_eq!(old.text, new.text);
                        assert_eq!(old.selection, new.selection);
                    }
                }
            }
        }

        #[test]
        fn showcase_clipboard_export_follows_guest_private_scrap_conversion() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 7));
                wait_for_menu(&mut session, 129, 7, true);
                settle(&mut session);
                for title in ["Reset", "Copy"] {
                    let control = session.runner_mut().control_snapshot().into_iter()
                        .find(|control| control.title == title && control.visible).unwrap();
                    let (top, left, bottom, right) = control.bounds;
                    let vertical = (top + bottom) / 2;
                    let horizontal = (left + right) / 2;
                    session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
                    settle(&mut session);
                    session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
                    settle(&mut session);
                }
                let before = session.runner_mut().text_edit_snapshot();
                let expected = before.private_scrap.clone();
                assert_eq!(expected.len(), 14, "fixture Copy must execute through guest tracking");
                assert_eq!(session.clipboard_text_after_suspend(), None);
                let mut worker = super::super::clipboard::GuestClipboard::default();
                worker.foreground(false, 1);
                session.request_foreground(false);
                worker.observe(&session);
                assert!(worker.export.is_none(), "host blur alone must not export private scrap");
                assert!((0..10_000).any(|_| {
                    session.runner_mut().run_steps(100, None);
                    worker.observe(&session);
                    worker.export.is_some()
                }), "guest suspend conversion never reached export: PPC={powerpc}, depth={depth:?}");
                assert_eq!(worker.export, Some((1, expected.clone())));
                assert_eq!(session.clipboard_text_after_suspend(), Some(Some(expected.clone())));
                let after = session.runner_mut().text_edit_snapshot();
                assert_eq!(after.private_scrap, expected);
                for (old, new) in before.records.iter().zip(&after.records) {
                    assert_eq!(old.text, new.text);
                    assert_eq!(old.selection, new.selection);
                }
                let sample = super::super::clipboard::HostSample { text: Some("old host".into()), text_only: true, revision: None };
                let mut host = super::super::clipboard::HostClipboard::default();
                host.suspend(1, sample.clone());
                let exported = host.export(1, &expected, sample).unwrap();
                assert_eq!(host.changed_text(Some(exported)), None, "guest export must not cause a resume import");
                host.resume();
                worker.foreground(true, 2);
                session.request_foreground(true);
                assert!(worker.export.is_none(), "resume clears retained export updates");
            }
        }

        #[test]
        fn showcase_text_selection_survives_modeless_window_activation() {
            for (powerpc, depth) in [(false, Some(1)), (false, Some(8)), (true, None)] {
                let mut session = MacintoshSession::new(true, depth);
                session.runner_mut().set_prefer_powerpc_executables(powerpc);
                let app = session.load_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit")).unwrap();
                session.initialize(&app);
                wait_for_menu(&mut session, 129, 1, true);
                assert!(session.runner_mut().select_guest_menu_item(129, 7));
                wait_for_menu(&mut session, 129, 7, true);
                settle(&mut session);
                for input in [
                    MacintoshInput::MouseDown { vertical: 132, horizontal: 75 },
                    MacintoshInput::MouseMove { vertical: 132, horizontal: 150 },
                    MacintoshInput::MouseUp { vertical: 132, horizontal: 150 },
                ] {
                    session.deliver_input(input);
                    settle(&mut session);
                }
                let text = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.view_rect == (76, 34, 211, 326)).unwrap();
                assert!(text.active);
                assert_ne!(text.selection.0, text.selection.1);
                assert!(session.runner_mut().select_guest_menu_item(132, 7));
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().text_edit_snapshot().records.iter().any(|record|
                        record.guest_id == text.guest_id && !record.active)
                }), "document must deactivate behind modeless dialog: PPC={powerpc}, depth={depth:?}");
                let inactive = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.guest_id == text.guest_id).unwrap();
                assert_eq!(inactive.selection, text.selection);
                assert_eq!(inactive.text, text.text);
                // caret_visible is the raw guest blink phase, not effective
                // visibility; TEDeactivate preserves it and clears active.
                assert!(!inactive.active);
                // Exposed document content: the activation click must not edit
                // or relocate the selection in its inactive TextEdit record.
                session.deliver_input(MacintoshInput::MouseDown { vertical: 100, horizontal: 70 });
                settle(&mut session);
                session.deliver_input(MacintoshInput::MouseUp { vertical: 100, horizontal: 70 });
                assert!((0..300).any(|_| {
                    session.runner_mut().run_steps(100_000, None);
                    session.runner_mut().text_edit_snapshot().records.iter().any(|record|
                        record.guest_id == text.guest_id && record.active)
                }), "document must reactivate: PPC={powerpc}, depth={depth:?}, windows={:?}, text={:?}", session.runner_mut().window_stack_snapshot(), session.runner_mut().text_edit_snapshot());
                let active = session.runner_mut().text_edit_snapshot().records.into_iter()
                    .find(|record| record.guest_id == text.guest_id).unwrap();
                assert_eq!(active.selection, text.selection);
                assert_eq!(active.text, text.text);
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
        fn themed_window_drag_and_activation_follow_guest_on_both_cpus(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};

            cx.update(gpui_kit::init);
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
                assert!(session.runner_mut().select_guest_menu_item(129, 3));
                let before = (0..100)
                    .find_map(|_| {
                        settle(&mut session);
                        let frames = session.runner_mut().window_frame_snapshot();
                        (frames.len() == 3).then_some(frames)
                    })
                    .expect("showcase window stack should appear");
                let (top, left, bottom, right) = before[0].window.bounds;
                let from = (top - 9, (left + right) / 2);
                let to = (from.0 + 12, from.1 + 16);
                let frame = session.video_frame().expect("showcase framebuffer");
                let width = frame.width;
                let height = frame.height;
                let host = |point: (i16, i16)| {
                    gpui_kit::point(
                        gpui_kit::px(f32::from(point.1)),
                        gpui_kit::px(f32::from(point.0)),
                    )
                };
                let (sender, receiver) = std::sync::mpsc::channel();
                let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
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
                        demo.windows = before.clone();
                        demo.width = width;
                        demo.height = height;

                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(host(from), host(to), cx);
                })
                .unwrap();
                let mut pressed = 0;
                let mut released = 0;
                for command in receiver.try_iter() {
                    let super::Command::Input(input) = command else {
                        continue;
                    };
                    match input {
                        MacintoshInput::MouseDown { .. } => pressed += 1,
                        MacintoshInput::MouseUp { .. } => released += 1,
                        _ => {}
                    }
                    session.deliver_input(input);
                    if matches!(
                        input,
                        MacintoshInput::MouseDown { .. }
                            | MacintoshInput::MouseMove { .. }
                            | MacintoshInput::MouseUp { .. }
                    ) {
                        settle(&mut session);
                    }
                }
                assert_eq!((pressed, released), (1, 1), "powerpc={powerpc}");
                let moved = session.runner_mut().window_frame_snapshot();
                assert_eq!(moved[0].guest_id, before[0].guest_id, "powerpc={powerpc}");
                assert_eq!(
                    moved[0].generation,
                    before[0].generation,
                    "powerpc={powerpc}"
                );
                assert_eq!(
                    moved[0].window.bounds,
                    (top + 12, left + 16, bottom + 12, right + 16),
                    "powerpc={powerpc}"
                );

                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = moved;
                        cx.notify();
                    });
                });
                // tests/toolbox-showcase/oracle/windows.json uses this exposed
                // auxiliary content point to exercise FindWindow/SelectWindow.
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(host((240, 210)), host((240, 210)), cx);
                })
                .unwrap();
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                let mut activated = session.runner_mut().window_frame_snapshot();
                assert_eq!(activated[0].guest_id, before[1].guest_id, "powerpc={powerpc}");
                assert!(activated[0].window.active, "powerpc={powerpc}");
                assert!(!activated[1].window.active, "powerpc={powerpc}");

                let grow_bounds = activated[0].window.bounds;
                let grow_from = (grow_bounds.2 - 5, grow_bounds.3 - 10);
                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = activated.clone();
                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(
                        host(grow_from),
                        host((grow_from.0 + 25, grow_from.1 + 25)),
                        cx,
                    );
                })
                .unwrap();
                let mut grow_presses = 0;
                let mut grow_releases = 0;
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        match input {
                            MacintoshInput::MouseDown { .. } => grow_presses += 1,
                            MacintoshInput::MouseUp { .. } => grow_releases += 1,
                            _ => {}
                        }
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                assert_eq!((grow_presses, grow_releases), (1, 1), "powerpc={powerpc}");
                activated = session.runner_mut().window_frame_snapshot();
                assert_eq!(activated[0].guest_id, before[1].guest_id, "powerpc={powerpc}");
                assert_eq!(activated[0].generation, before[1].generation, "powerpc={powerpc}");
                assert_eq!(
                    activated[0].window.bounds,
                    (grow_bounds.0, grow_bounds.1, grow_bounds.2 + 25, grow_bounds.3 + 25),
                    "powerpc={powerpc}"
                );

                let aux_bounds = activated[0].window.bounds;
                let aux_title = (aux_bounds.0 - 9, (aux_bounds.1 + aux_bounds.3) / 2);
                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = activated.clone();
                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(
                        host(aux_title),
                        gpui_kit::point(gpui_kit::px(850.), host(aux_title).y),
                        cx,
                    );
                })
                .unwrap();
                let mut presses = 0;
                let mut releases = 0;
                let mut release_at = None;
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        match input {
                            MacintoshInput::MouseDown { .. } => presses += 1,
                            MacintoshInput::MouseUp {
                                vertical,
                                horizontal,
                            } => {
                                releases += 1;
                                release_at = Some((vertical, horizontal));
                            }
                            _ => {}
                        }
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                assert_eq!((presses, releases), (1, 1), "powerpc={powerpc}");
                assert_eq!(release_at, Some((aux_title.0, 799)), "powerpc={powerpc}");
                let off_pane_moved = session.runner_mut().window_frame_snapshot();
                assert_eq!(off_pane_moved[0].guest_id, activated[0].guest_id);
                assert!(
                    off_pane_moved[0].window.bounds.1 > aux_bounds.1,
                    "off-pane drag should move guest window on powerpc={powerpc}: {:?}",
                    off_pane_moved[0].window.bounds
                );

                let edge_bounds = off_pane_moved[0].window.bounds;
                let edge_title = (edge_bounds.0 - 9, (edge_bounds.1 + edge_bounds.3) / 2);
                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = off_pane_moved.clone();
                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(
                        host(edge_title),
                        gpui_kit::point(gpui_kit::px(-50.), host(edge_title).y),
                        cx,
                    );
                })
                .unwrap();
                let mut presses = 0;
                let mut release_at = None;
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        match input {
                            MacintoshInput::MouseDown { .. } => presses += 1,
                            MacintoshInput::MouseUp {
                                vertical,
                                horizontal,
                            } => {
                                assert!(release_at.is_none(), "duplicate release on {powerpc}");
                                release_at = Some((vertical, horizontal));
                            }
                            _ => {}
                        }
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                assert_eq!(presses, 1, "powerpc={powerpc}");
                assert_eq!(release_at, Some((edge_title.0, 0)), "powerpc={powerpc}");
                let outside_moved = session.runner_mut().window_frame_snapshot();
                assert_eq!(outside_moved[0].guest_id, off_pane_moved[0].guest_id);
                assert!(
                    outside_moved[0].window.bounds.1 < edge_bounds.1,
                    "outer-window drag should move guest window on powerpc={powerpc}: {:?}",
                    outside_moved[0].window.bounds
                );

                let zoom_bounds = outside_moved[0].window.bounds;
                let zoom_point = (zoom_bounds.0 - 9, zoom_bounds.3 - 7);
                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = outside_moved.clone();
                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(host(zoom_point), host((zoom_point.0, zoom_point.1 + 1)), cx);
                })
                .unwrap();
                let mut zoom_presses = 0;
                let mut zoom_releases = 0;
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        match input {
                            MacintoshInput::MouseDown { .. } => zoom_presses += 1,
                            MacintoshInput::MouseUp { .. } => zoom_releases += 1,
                            _ => {}
                        }
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                assert_eq!((zoom_presses, zoom_releases), (1, 1), "powerpc={powerpc}");
                let zoomed = session.runner_mut().window_frame_snapshot();
                assert_eq!(zoomed[0].guest_id, outside_moved[0].guest_id, "powerpc={powerpc}");
                assert_eq!(zoomed[0].generation, outside_moved[0].generation, "powerpc={powerpc}");
                assert_ne!(zoomed[0].window.bounds, zoom_bounds, "powerpc={powerpc}");
                assert!(zoomed[0].window.bounds.0 >= 40, "powerpc={powerpc}");

                let restore_point = (zoomed[0].window.bounds.0 - 9, zoomed[0].window.bounds.3 - 7);
                cx.update(|cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows = zoomed.clone();
                        cx.notify();
                    });
                });
                cx.update_window(window.into(), |_, window, cx| {
                    window.drag(host(restore_point), host((restore_point.0, restore_point.1 + 1)), cx);
                })
                .unwrap();
                let mut restore_presses = 0;
                let mut restore_releases = 0;
                for command in receiver.try_iter() {
                    if let super::Command::Input(input) = command {
                        match input {
                            MacintoshInput::MouseDown { .. } => restore_presses += 1,
                            MacintoshInput::MouseUp { .. } => restore_releases += 1,
                            _ => {}
                        }
                        session.deliver_input(input);
                        settle(&mut session);
                    }
                }
                assert_eq!((restore_presses, restore_releases), (1, 1), "powerpc={powerpc}");
                let restored = session.runner_mut().window_frame_snapshot();
                assert_eq!(restored[0].guest_id, outside_moved[0].guest_id, "powerpc={powerpc}");
                assert_eq!(restored[0].generation, outside_moved[0].generation, "powerpc={powerpc}");
                assert_eq!(restored[0].window.bounds, zoom_bounds, "powerpc={powerpc}");


            }
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn themed_window_title_drag_forwards_guest_coordinates(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, WindowBounds, WindowOptions};
            use systemless::runner::{WindowFrameSnapshot, WindowSnapshot};

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
                            title: "Inspector".into(),
                            bounds: (50, 40, 420, 600),
                            structure_bounds: Some((31, 39, 422, 602)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(8),
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![(50, 40, 420, 600)]),
                        close_box: true,
                        grow_icon_drawn: false,
                    }];
                    demo.width = 800;
                    demo.height = 600;

                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.drag(
                    gpui_kit::point(gpui_kit::px(300.), gpui_kit::px(41.)),
                    gpui_kit::point(gpui_kit::px(316.), gpui_kit::px(53.)),
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
            let presses: Vec<_> = inputs
                .iter()
                .filter(|input| {
                    matches!(
                        input,
                        MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }
                    )
                })
                .collect();
            assert!(matches!(
                presses.as_slice(),
                [
                    MacintoshInput::MouseDown {
                        vertical: 41,
                        horizontal: 300
                    },
                    MacintoshInput::MouseUp {
                        vertical: 53,
                        horizontal: 316
                    }
                ]
            ), "{inputs:?}");
            assert!(inputs.iter().any(|input| matches!(
                input,
                MacintoshInput::MouseMove {
                    vertical: 53,
                    horizontal: 316
                }
            )), "{inputs:?}");
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
                    demo.height = 600;

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
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
                    demo.height = 600;

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
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
                    vertical: 0,
                    horizontal: 0
                })
            ));
            assert!(!view.read_with(cx, |demo, _| demo.mouse_down));
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn host_clipboard_export_respects_activation_and_new_host_contents(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::AppContext;
            let (sender, _receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let view = cx.update(|cx| cx.new(|cx| super::Demo::new(sender, updates, cx)));
            cx.update(|cx| {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("old".into()));
                view.update(cx, |demo, cx| {
                    demo.host_active = Some(false);
                    demo.host_clipboard.suspend(1, super::Demo::read_host_clipboard(cx));
                    demo.export_host_clipboard(0, b"stale", cx);
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "old");
                    demo.export_host_clipboard(1, b"caf\x8e\rnext", cx);
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "café\nnext");
                    demo.host_clipboard.suspend(2, super::Demo::read_host_clipboard(cx));
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("new host copy".into()));
                    demo.export_host_clipboard(2, b"delayed guest copy", cx);
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "new host copy");
                    demo.host_clipboard.suspend(3, super::Demo::read_host_clipboard(cx));
                    demo.host_active = Some(true);
                    demo.export_host_clipboard(3, b"late", cx);
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "new host copy");
                });
            });
        }

        #[cfg(feature = "gpui-demo-test")]
        #[gpui_kit::test]
        fn host_window_deactivation_releases_held_input(cx: &mut gpui_kit::TestAppContext) {
            use gpui_kit::{AppContext, test::TestWindowExt};

            let (sender, receiver) = std::sync::mpsc::channel();
            let updates = std::sync::Arc::new(std::sync::Mutex::new(None));
            cx.update(gpui_kit::init);
            let (window, view) = cx.update(|cx| {
                gpui_kit::open_window(Default::default(), cx, |_, cx| {
                    cx.new(|cx| super::Demo::new(sender, updates, cx))
                }).unwrap()
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.activate_window();
                view.update(cx, |demo, cx| demo.focus.focus(window, cx));
                window.render_frame(cx);
            }).unwrap();
            let mut visual = gpui_kit::VisualTestContext::from_window(window.into(), cx);
            visual.run_until_parked();
            view.update(cx, |demo, _| {
                demo.mouse_down = true;
                demo.mouse_position = (92, 137);
                demo.press_host_key(0x00, b'a');
            });
            receiver.try_iter().for_each(drop);
            visual.deactivate_window();
            visual.deactivate_window();
            let commands: Vec<_> = receiver.try_iter().collect();
            let foreground: Vec<_> = commands.iter().filter_map(|command| match command {
                super::Command::Foreground(active, _) => Some(*active),
                _ => None,
            }).collect();
            assert_eq!(foreground, vec![false]);
            let inputs: Vec<_> = commands.into_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                _ => None,
            }).collect();
            assert!(matches!(inputs.as_slice(), [
                MacintoshInput::MouseUp { vertical: 92, horizontal: 137 },
                MacintoshInput::KeyUp { mac_key: 0x00, character: b'a' },
            ]));
            assert!(!view.read_with(cx, |demo, _| demo.mouse_down));
            cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("café\nnext".into())));
            cx.update_window(window.into(), |_, window, _| window.activate_window()).unwrap();
            visual.run_until_parked();
            let commands: Vec<_> = receiver.try_iter().collect();
            assert!(matches!(commands.as_slice(), [
                super::Command::ImportClipboard(text),
                super::Command::Foreground(true, _),
            ] if text == b"caf\x8e\rnext"));
            visual.deactivate_window();
            receiver.try_iter().for_each(drop);
            cx.update_window(window.into(), |_, window, _| window.activate_window()).unwrap();
            visual.run_until_parked();
            assert!(matches!(receiver.try_iter().collect::<Vec<_>>().as_slice(), [
                super::Command::Foreground(true, _),
            ]), "unchanged clipboard must not overwrite newer guest scrap");
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
                    super::Command::Foreground(..) => panic!("input focus loss must not switch the application"),
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![bounds]),
                        close_box: false,
                        grow_icon_drawn: false,
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
                            static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                            edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                            control_identity: None,
                            pressed: false,
                            number: 1,
                            kind: super::DialogItemKind::Button,
                            bounds: (220, 360, 240, 430),
                            text: "OK".into(),
                            enabled: true,
                            visible: true,
                            value: None,
                            selection: None,
                            caret_visible: Some(true),
                        }],
                    }];
                    demo.width = 800;
                    demo.height = 600;

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
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
            for (active, enabled) in [(false, true), (true, false), (true, true)] {
                cx.update_window(window.into(), |_, window, cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows[0].window.active = active;
                        demo.dialogs[0].active = active;
                        demo.dialogs[0].items[0].enabled = enabled;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert_eq!(window.find("guest-dialog-button-7-1-1").focused(), (active && enabled).then_some(false));
                    window.click("guest-dialog-button-7-1-1", cx);
                }).unwrap();
                let commands: Vec<_> = receiver.try_iter().collect();
                assert!(!commands.iter().any(|command| matches!(command,
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..))));
                for down in [true, false] {
                    assert_eq!(commands.iter().filter(|command| match command {
                        super::Command::Input(MacintoshInput::MouseDown { .. }) => down,
                        super::Command::Input(MacintoshInput::MouseUp { .. }) => !down,
                        _ => false,
                    }).count(), 1, "guest must receive exactly one press/release in every state");
                }
            }
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![bounds]),
                        close_box: false,
                        grow_icon_drawn: false,
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
                                static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                                edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                                control_identity: None,
                                pressed: false,
                                number: 1,
                                kind: super::DialogItemKind::Checkbox,
                                bounds: (145, 150, 165, 450),
                                text: "Enable 3D".into(),
                                enabled: true,
                                visible: true,
                                value: Some(0),
                                selection: None,
                                caret_visible: Some(true),
                            },
                            DialogItemSnapshot {
                                static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                                edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                                control_identity: None,
                                pressed: false,
                                number: 2,
                                kind: super::DialogItemKind::EditText,
                                bounds: (200, 235, 220, 430),
                                text: "Pilot".into(),
                                enabled: true,
                                visible: true,
                                value: None,
                                selection: Some((0, 0)),
                                caret_visible: Some(true),
                            },
                        ],
                    }];
                    demo.width = 800;
                    demo.height = 600;

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
            use gpui_kit::{test::TestWindowExt, AppContext, Bounds, InputEvent, WindowBounds, WindowOptions};
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
                    demo.height = 600;

                    let bounds = (50, 40, 420, 600);
                    demo.windows = vec![systemless::runner::WindowFrameSnapshot {
                        guest_id: 7,
                        generation: 1,
                        window: systemless::runner::WindowSnapshot {
                            title: "Controls".into(),
                            bounds,
                            structure_bounds: Some((31, 39, 422, 602)),
                            visible_region: None,
                            update_region: None,
                            visible: true,
                            active: true,
                        },
                        definition_id: Some(0),
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![bounds]),
                        close_box: false,
                        grow_icon_drawn: false,
                    }];
                    demo.controls = vec![systemless::runner::ControlSnapshot {
                        guest_id: 22,
                        generation: 1,
                        owner_id: 7,
                        proc_id: 0,
                        local_bounds: (20, 20, 44, 140),
                        bounds: (70, 60, 94, 180),
                        owner_visible: true,
                        visible: true,
                        enabled: true,
                        hilite: 0,
                        value: 0,
                        minimum: 0,
                        maximum: 1,
                        title: "Button".into(),
                        popup_menu_id: None,
                        popup_title_width: None,
                        popup_text_inset: 15,
                        popup_ink: None,
                        popup_indicator: None,
                        popup_box_bounds: None,
                        popup_font: None,
                        font_style: None,
                    }];
                    demo.width = 800;
                    demo.height = 600;
                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                        image::Frame::new(image::RgbaImage::new(800, 600)),
                    ])));

                    demo.standard_file = Some(StandardFileSnapshot {
                        guest_id: 7,
                        generation: 1,
                        kind: StandardFileKind::Get,
                list_text_origin: (4, 11),
                directory_marker: "▸",
                list_name_limit: Some(36),
                volume_text: Some(("Maci...".into(), (15, 11))),
                        confirming_replace: false,
                        new_folder: None,
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
                        name_text_layout: None,
                        name_caret_visible: None,
                        name_has_focus: None,
                        directory_font: (0, 0, 0),
                        directory_text_layout: (1, 12, 16),
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
                assert_eq!(window.find("guest-control-button-22-1").focused(), None, "Standard File must remove background controls from keyboard focus");
                assert_eq!(window.find("guest-standard-open-list-7-1").role(), Some(gpui_kit::Role::ListBox));
                let row = window.find("guest-standard-open-entry-7-1-0");
                assert_eq!(row.role(), Some(gpui_kit::Role::ListBoxOption));
                assert_eq!(row.selected(), Some(true));
                window.click("guest-standard-open-7-1-Open", cx);
            })
            .unwrap();
            let open_inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    super::Command::ActivateFile(..) => panic!("pointer click duplicated as semantic file activation"),
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

            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| {
                    demo.standard_file.as_mut().unwrap().selected = None;
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-standard-open-7-1-Open").focused(), None);
                window.click("guest-standard-open-7-1-Open", cx);
            }).unwrap();
            let disabled_inputs: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                super::Command::ActivateFile(..) => panic!("pointer click duplicated as semantic file activation"),
                _ => None,
            }).collect();
            assert_eq!(disabled_inputs.iter().filter(|input| matches!(input, MacintoshInput::MouseDown { .. })).count(), 1);
            assert_eq!(disabled_inputs.iter().filter(|input| matches!(input, MacintoshInput::MouseUp { .. })).count(), 1);

            cx.update(|cx| {
                view.update(cx, |demo, cx| {
                    demo.standard_file = Some(StandardFileSnapshot {
                        guest_id: 8,
                        generation: 2,
                        kind: StandardFileKind::Put,
                list_text_origin: (4, 11),
                directory_marker: "▸",
                list_name_limit: Some(36),
                volume_text: None,
                        confirming_replace: false,
                        new_folder: None,
                        standard_entry_point: true,
                        bounds: (100, 100, 360, 460),
                        directory_id: 2,
                        entries: Some(Vec::new()),
                        selected: None,
                        prompt: Some("Save as:".into()),
                        name: Some("Untitled".into()),
                        name_selection: Some((0, 8)),
                        name_text_layout: Some(systemless::runner::StandardFileNameTextLayout { font: (0, 0, 0), origin: (1, 12), selection_top: 0, selection_height: 16, selection_to_edge: true, wraps: false }),
                        name_caret_visible: Some(false),
                        name_has_focus: Some(true),
                        directory_font: (0, 0, 0),
                        directory_text_layout: (1, 12, 16),
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
                            new_folder: (320, 210, 342, 260),
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
                assert_eq!(window.find("guest-standard-save-list-8-2").role(), Some(gpui_kit::Role::ListBox));
                window.click("guest-standard-save-8-2-Save", cx);
            })
            .unwrap();
            let save_inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    super::Command::ActivateFile(..) => panic!("pointer click duplicated as semantic file activation"),
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
            cx.update_window(window.into(), |_, window, cx| {
                for font in [(0, 0, 1), (0, 0, 2), (0, 97, 0)] {
                    view.update(cx, |demo, cx| {
                        demo.standard_file.as_mut().unwrap().name_text_layout.as_mut().unwrap().font = font;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert!(window.try_find("guest-standard-save-name-8-2").is_none(),
                        "unsupported filename typography must retain the guest panel: {font:?}");
                }
                view.update(cx, |demo, cx| {
                    demo.standard_file.as_mut().unwrap().name_text_layout.as_mut().unwrap().font = (0, 0, 0);
                    cx.notify();
                });
                for font in [(0, 0, 1), (0, 0, 2), (0, 97, 0)] {
                    view.update(cx, |demo, cx| {
                        demo.standard_file.as_mut().unwrap().directory_font = font;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert!(window.try_find("guest-standard-save-list-8-2").is_none(),
                        "unsupported directory typography must retain the guest panel: {font:?}");
                }
                view.update(cx, |demo, cx| {
                    demo.standard_file.as_mut().unwrap().directory_font = (0, 0, 0);
                    demo.standard_file.as_mut().unwrap().confirming_replace = true;
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-standard-replace").role(), Some(gpui_kit::Role::AlertDialog));
                for label in ["Desktop", "Cancel", "Save"] {
                    assert_eq!(window.find(format!("guest-standard-save-8-2-{label}")).focused(), None,
                        "modal background must not accept host focus");
                }
                window.click("guest-standard-replace-8-2-Replace", cx);
            }).unwrap();
            let confirmation_inputs: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input) => Some(input),
                super::Command::ActivateFile(..) => panic!("pointer click duplicated as semantic replacement"),
                _ => None,
            }).collect();
            let layout = systemless::runner::StandardFileReplacementLayout::new((100, 100, 360, 460));
            let presses: Vec<_> = confirmation_inputs.iter().filter(|input| matches!(input, MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. })).collect();
            assert_eq!(presses.len(), 2);
            for input in presses {
                let (MacintoshInput::MouseDown { vertical, horizontal } | MacintoshInput::MouseUp { vertical, horizontal }) = input else { unreachable!() };
                assert!(*vertical >= layout.replace.0 && *vertical < layout.replace.2);
                assert!(*horizontal >= layout.replace.1 && *horizontal < layout.replace.3);
            }
            cx.update(|cx| view.update(cx, |demo, cx| {
                demo.standard_file.as_mut().unwrap().confirming_replace = false;
                cx.notify();
            }));
            for selected in [None, Some(0), None] {
                cx.update_window(window.into(), |_, window, cx| {
                    view.update(cx, |demo, cx| {
                        let panel = demo.standard_file.as_mut().unwrap();
                        panel.entries = Some(vec![StandardFileEntrySnapshot {
                            name: "Documents".into(), directory_id: 3,
                            is_directory: true, file_type: 0,
                        }]);
                        panel.selected = selected;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert!(window.find("guest-standard-save-8-2-Save").focused().is_some(),
                        "closing confirmation restores the Save button focus handle");
                    let row = window.find("guest-standard-save-entry-8-2-0");
                    assert_eq!(row.role(), Some(gpui_kit::Role::ListBoxOption));
                    assert_eq!(row.selected(), Some(selected == Some(0)));
                }).unwrap();
            }
            assert!(!receiver.try_iter().any(|command| matches!(command, super::Command::Input(_) | super::Command::ActivateFile(..))),
                "presenting guest selection changes must not emit guest input");
            let folder_layout = systemless::runner::StandardFileNewFolderLayout::new((100, 100, 360, 460));
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| {
                    let panel = demo.standard_file.as_mut().unwrap();
                    panel.new_folder = Some(systemless::runner::StandardFileNewFolderSnapshot {
                        name: "untitled folder".into(), selection: (0, 15), error: None,
                        visible_offset: 0,
                        caret_visible: false,
                        insertion_positions: (0..=15).map(|i| folder_layout.name.1 + 2 + i * 7).collect(),
                        layout: folder_layout.clone(),
                    });
                    panel.name_has_focus = Some(false);
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-standard-new-folder").role(), Some(gpui_kit::Role::Dialog));
                view.update(cx, |demo, _| {
                    let map = demo.text_pointer_map.borrow().clone().expect("painted glyph positions");
                    assert_eq!(map.positions.len(), 16);
                    let y = demo.display_origin.1 + f32::from((folder_layout.name.0 + folder_layout.name.2) / 2) * demo.display_scale;
                    for (x, guest) in &map.positions {
                        demo.text_pointer_capture = None;
                        assert_eq!(demo.text_pointer(gpui_kit::point(gpui_kit::px(*x), gpui_kit::px(y)), true).1, *guest);
                    }
                    // Drag ownership continues beyond the field, including mouse-up.
                    assert_eq!(demo.text_pointer(gpui_kit::point(gpui_kit::px(map.positions[0].0 - 40.), gpui_kit::px(y)), false).1, map.positions[0].1);
                    demo.text_pointer_capture = None;
                });
                for label in ["Desktop", "New", "Cancel", "Save"] {
                    assert_eq!(window.find(format!("guest-standard-save-8-2-{label}")).focused(), None);
                }
                let (start, outside, first, selected) = view.update(cx, |demo, _| {
                    let map = demo.text_pointer_map.borrow();
                    let map = map.as_ref().unwrap();
                    let y = demo.display_origin.1 + f32::from((folder_layout.name.0 + folder_layout.name.2) / 2) * demo.display_scale;
                    (gpui_kit::point(gpui_kit::px(map.positions[6].0), gpui_kit::px(y)),
                     gpui_kit::point(gpui_kit::px(map.positions[0].0 - 30.), gpui_kit::px(y)),
                     map.positions[0].1, map.positions[6].1)
                });
                window.dispatch_event(gpui_kit::MouseDownEvent {
                    position: start, button: gpui_kit::MouseButton::Left, click_count: 1, ..Default::default()
                }.to_platform_input(), cx);
                window.dispatch_event(gpui_kit::MouseMoveEvent {
                    position: outside, pressed_button: Some(gpui_kit::MouseButton::Left), ..Default::default()
                }.to_platform_input(), cx);
                window.dispatch_event(gpui_kit::MouseUpEvent {
                    position: outside, button: gpui_kit::MouseButton::Left, click_count: 1, ..Default::default()
                }.to_platform_input(), cx);
                let inputs: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                    super::Command::Input(input) => Some(input), _ => None,
                }).collect();
                assert!(matches!(inputs.as_slice(), [
                    MacintoshInput::MouseDown { horizontal: down, .. },
                    MacintoshInput::MouseMove { horizontal: moved, .. },
                    MacintoshInput::MouseUp { horizontal: up, .. },
                ] if *down == selected && *moved == first && *up == first), "{inputs:?}");
                for name in ["iéW", "", "wwwwwwwwwwwwwwwwwwwwwwwwwwwabcd", "untitled folder"] {
                    view.update(cx, |demo, cx| {
                        let folder = demo.standard_file.as_mut().unwrap().new_folder.as_mut().unwrap();
                        folder.name = name.into();
                        let count = name.chars().count();
                        folder.selection = (count, count);
                        folder.visible_offset = count;
                        folder.caret_visible = true;
                        folder.insertion_positions = (0..=count).map(|i| folder.layout.name.1 + 2 + i as i16 * 7).collect();
                        cx.notify();
                    });
                    window.render_frame(cx);
                    view.update(cx, |demo, _| {
                        let map = demo.text_pointer_map.borrow().clone().unwrap();
                        assert_eq!(map.positions.len(), name.chars().count() + 1);
                        let left = demo.display_origin.0 + f32::from(folder_layout.name.1) * demo.display_scale;
                        let right = demo.display_origin.0 + f32::from(folder_layout.name.3) * demo.display_scale;
                        let caret_x = map.positions.last().unwrap().0;
                        assert!(caret_x >= left && caret_x < right,
                            "name caret must remain visible: name={name}, caret={caret_x}, field={left}..{right}");
                        let y = demo.display_origin.1 + f32::from((folder_layout.name.0 + folder_layout.name.2) / 2) * demo.display_scale;
                        for (x, guest) in map.positions.iter().filter(|(x, _)| *x >= left && *x < right) {
                            demo.text_pointer_capture = None;
                            assert_eq!(demo.text_pointer(gpui_kit::point(gpui_kit::px(*x), gpui_kit::px(y)), true).1, *guest, "{name}");
                        }
                        demo.text_pointer_capture = None;
                    });
                }
                window.click("guest-standard-new-folder-8-2-Create", cx);
            }).unwrap();
            let presses: Vec<_> = receiver.try_iter().filter_map(|command| match command {
                super::Command::Input(input @ (MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. })) => Some(input),
                super::Command::ActivateFile(..) => panic!("pointer click duplicated as semantic folder creation"),
                _ => None,
            }).collect();
            assert_eq!(presses.len(), 2);
            for input in presses {
                let (MacintoshInput::MouseDown { vertical, horizontal } | MacintoshInput::MouseUp { vertical, horizontal }) = input else { unreachable!() };
                assert!(vertical >= folder_layout.create.0 && vertical < folder_layout.create.2);
                assert!(horizontal >= folder_layout.create.1 && horizontal < folder_layout.create.3);
            }
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| {
                    demo.standard_file.as_mut().unwrap().new_folder.as_mut().unwrap().error = Some(-48);
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-standard-new-folder").role(), Some(gpui_kit::Role::AlertDialog));
                for label in ["Desktop", "New", "Cancel", "Save"] {
                    assert_eq!(window.find(format!("guest-standard-save-8-2-{label}")).focused(), None);
                }
                window.click("guest-standard-new-folder-8-2-OK", cx);
            }).unwrap();
            let presses = receiver.try_iter().filter(|command| matches!(command,
                super::Command::Input(MacintoshInput::MouseDown { .. } | MacintoshInput::MouseUp { .. }))).count();
            assert_eq!(presses, 2);
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| {
                    demo.standard_file = None;
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-control-button-22-1").focused(), Some(false),
                    "closing Standard File restores background control focus eligibility");
            }).unwrap();
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![(50, 40, 420, 600)]),
                        close_box: false,
                        grow_icon_drawn: false,
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
                        standard_cell_paint: Default::default(),
                    }];
                    demo.width = 800;
                    demo.height = 600;

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
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
        fn themed_document_button_forwards_one_guest_press_and_release(
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![bounds]),
                        close_box: false,
                        grow_icon_drawn: false,
                    }];
                    demo.controls = vec![ControlSnapshot {
                        guest_id: 22,
                        generation: 1,
                        owner_id: 7,
                        proc_id: 0,
                        local_bounds: (100, 100, 124, 220),
                        bounds: (150, 140, 174, 260),
                        owner_visible: true,
                        visible: true,
                        enabled: true,
                        hilite: 0,
                        value: 0,
                        minimum: 0,
                        maximum: 1,
                        title: "Button".into(),
                        popup_menu_id: None,
                        popup_title_width: None,
                        popup_text_inset: 15,
                        popup_ink: None,
                        popup_indicator: None,
                        popup_box_bounds: None,
                        popup_font: None,
                        font_style: None,
                    }];
                    demo.width = 800;
                    demo.height = 600;
                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                        image::Frame::new(image::RgbaImage::new(800, 600)),
                    ])));

                    cx.notify();
                });
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.click("guest-control-button-22-1", cx);
            })
            .unwrap();
            let inputs: Vec<_> = receiver
                .try_iter()
                .filter_map(|command| match command {
                    super::Command::Input(input) => Some(input),
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
            for (active, enabled) in [(false, true), (true, false), (true, true)] {
                cx.update_window(window.into(), |_, window, cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows[0].window.active = active;
                        demo.controls[0].enabled = enabled;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert_eq!(window.find("guest-control-button-22-1").focused(), (active && enabled).then_some(false));
                    let button = super::super::choices::guest_button(
                        "button-state-probe".into(), "Activate".into(), enabled, active,
                        false, false, 1., cx,
                    ).track_focus(&cx.focus_handle());
                    let component = super::super::a11y::AccessibleComponent::new(button, !active || !enabled);
                    let rendered = gpui_kit::IntoElement::into_element(gpui_kit::RenderOnce::render(component, window, cx));
                    let mut node = gpui_kit::accesskit::Node::new(gpui_kit::Element::a11y_role(&rendered).unwrap());
                    gpui_kit::Element::write_a11y_info(&rendered, &mut node);
                    assert_eq!(node.role(), gpui_kit::Role::Button);
                    assert_eq!(node.label(), Some("Activate"));
                    assert_eq!(node.is_disabled(), !active || !enabled);
                    window.click("guest-control-button-22-1", cx);
                }).unwrap();
                let commands: Vec<_> = receiver.try_iter().collect();
                assert!(!commands.iter().any(|command| matches!(command,
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..))));
                for down in [true, false] {
                    assert_eq!(commands.iter().filter(|command| match command {
                        super::Command::Input(MacintoshInput::MouseDown { .. }) => down,
                        super::Command::Input(MacintoshInput::MouseUp { .. }) => !down,
                        _ => false,
                    }).count(), 1, "guest must receive exactly one press/release in every state");
                }
            }
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
                        rectangular_regions: true,
                        visible_content_rects: Some(vec![bounds]),
                        close_box: false,
                        grow_icon_drawn: false,
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
                        popup_text_inset: 15,
                        popup_ink: None,
                        popup_indicator: None,
                        popup_box_bounds: None,
                        popup_font: None,
                        font_style: None,
                    }];
                    demo.width = 800;
                    demo.height = 600;
                    demo.image = Some(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![
                        image::Frame::new(image::RgbaImage::new(800, 600)),
                    ])));

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
                    super::Command::ActivateControl(..) | super::Command::ActivateDialog(..) => panic!("pointer click duplicated as semantic activation"),
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
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |demo, cx| {
                    demo.windows[0].window.active = false;
                    cx.notify();
                });
                window.render_frame(cx);
                assert_eq!(window.find("guest-control-checkbox-22-1").focused(), None,
                    "inactive guest controls must not expose host focus");
                window.click("guest-control-checkbox-22-1", cx);
            }).unwrap();
            let commands: Vec<_> = receiver.try_iter().collect();
            assert!(!commands.iter().any(|command| matches!(command, super::Command::ActivateControl(..))));
            assert_eq!(commands.iter().filter(|command| matches!(command,
                super::Command::Input(MacintoshInput::MouseDown { .. })
            )).count(), 1, "inactive window activation still belongs to the guest");
            for enabled in [true, false, true] {
                cx.update_window(window.into(), |_, window, cx| {
                    view.update(cx, |demo, cx| {
                        demo.windows[0].window.active = true;
                        demo.controls[0].enabled = enabled;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    assert_eq!(window.find("guest-control-checkbox-22-1").focused(),
                        enabled.then_some(false), "host focus availability must follow live guest state");
                    let choice = super::super::choices::guest_checkbox(
                        "a11y-state-probe".into(), "Sound".into(), true, enabled, false, 1., cx,
                    ).track_focus(&cx.focus_handle());
                    let component = super::super::a11y::AccessibleComponent::new(choice, !enabled);
                    let rendered = gpui_kit::IntoElement::into_element(gpui_kit::RenderOnce::render(component, window, cx));
                    let mut node = gpui_kit::accesskit::Node::new(gpui_kit::Element::a11y_role(&rendered).unwrap());
                    gpui_kit::Element::write_a11y_info(&rendered, &mut node);
                    assert_eq!(node.role(), gpui_kit::Role::CheckBox);
                    assert_eq!(node.toggled(), Some(gpui_kit::Toggled::True));
                    assert_eq!(node.is_disabled(), !enabled);
                    for checked in [false, true] {
                        let radio = super::super::choices::guest_radio(
                            "a11y-radio-probe".into(), "Music".into(), checked, enabled, false, 1., cx,
                        ).track_focus(&cx.focus_handle());
                        let component = super::super::a11y::AccessibleComponent::new(radio, !enabled);
                        let rendered = gpui_kit::IntoElement::into_element(gpui_kit::RenderOnce::render(component, window, cx));
                        let mut node = gpui_kit::accesskit::Node::new(gpui_kit::Element::a11y_role(&rendered).unwrap());
                        gpui_kit::Element::write_a11y_info(&rendered, &mut node);
                        assert_eq!(node.role(), gpui_kit::Role::RadioButton);
                        assert_eq!(node.toggled(), Some(if checked { gpui_kit::Toggled::True } else { gpui_kit::Toggled::False }));
                        assert_eq!(node.is_disabled(), !enabled);
                    }
                }).unwrap();
            }
        }
    }
}
