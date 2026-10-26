//! Guest initialization, pacing, sound and saves, independent of host windows.
//! The driver is still called on the event thread during the initial extraction.

use super::runtime_protocol::{CursorWarp, DialogTextAck, GuiCommand, GuiState};
use super::{
    configure_realtime_execution_rate, debug_server, foreground_cpu_batch_instructions,
    service_pending_sound_work, DesktopSaveStore, FramePhaseTimer, HostMouseReleaseLatch,
    AUDIO_CALLBACK_CHUNK_SAMPLES, FRAME_DURATION, MAX_AUDIO_MIX_INTERVAL, MAX_RENDER_HEADROOM,
    MIN_RENDER_HEADROOM, RENDER_HEADROOM_MARGIN,
};
use std::path::PathBuf;
use systemless::systems::macintosh::game;
use systemless::systems::macintosh::runner::MenuBarPolicy;
use systemless::systems::macintosh::{runner::FixtureRunner, ui_theme::UiThemeId};

pub(super) struct GuiDriver {
    pub(super) gpu: bool,
    pub(super) workspace: bool,
    generation: u64,
    snapshot_sequence: u64,
    warp_serial: u64,
    file_reply: Option<(u64, u64, u64, bool)>,
    dialog_text_replies: std::collections::HashMap<u64, std::sync::Arc<DialogTextAck>>,
    /// An edit typed into a filtered dialog: (serial, dialog, item, quiet
    /// captures). Acknowledged once the guest has consumed the keystrokes.
    typed_text: Option<(u64, u64, i16, u8)>,
    pending_warp: Option<CursorWarp>,
    #[cfg(target_os = "macos")]
    identity: Option<std::sync::Arc<game::ApplicationIdentity>>,
    display_generation: u64,
    snapshot_screen_mode: Option<(u32, u32, u16, u16, u16)>,
    compact_cache: systemless::systems::macintosh::memory::CompactPresentationCache,
    compact_snapshot:
        Option<std::sync::Arc<systemless::systems::macintosh::memory::CompactPresentation>>,
    /// Snapshots retired by later frames, kept for reuse once the presenter
    /// and renderer have let go of them.
    compact_spares:
        Vec<std::sync::Arc<systemless::systems::macintosh::memory::CompactPresentation>>,
    pub(super) runner: Option<FixtureRunner>,
    pub(super) debug_server: Option<debug_server::DebugServer>,
    pub(super) save_store: Option<DesktopSaveStore>,
    pub(super) guest_exit_reported: bool,
    pub(super) initialized: bool,
    pub(super) total_instructions: u64,
    /// Wall-clock origin for deriving tick targets.
    pub(super) start_time: Option<std::time::Instant>,
    /// Next frame target for pacing.
    pub(super) next_frame_time: Option<std::time::Instant>,
    /// Adaptive CPU/render split for the single-threaded GUI loop.
    pub(super) render_headroom: std::time::Duration,
    /// Fractional host samples carried between GUI slices to preserve rate.
    pub(super) audio_sample_remainder: f64,
    /// Wall-clock instant represented by the most recently queued audio.
    /// Unlike video, audio cannot simply drop a late host frame without
    /// starving the device ring buffer.
    pub(super) last_audio_mix_time: Option<std::time::Instant>,
    pub(super) mouse_release_latch: HostMouseReleaseLatch,
    /// Frame counter for diagnostic screenshots
    pub(super) frame_count: u64,
    /// Guest tick last presented to the host window.
    pub(super) last_presented_guest_tick: Option<u32>,
    /// Force the next host present even if the guest tick has not advanced.
    pub(super) force_next_render: bool,
    game_path: PathBuf,
    arrows_as_numpad: bool,
    #[cfg(target_os = "macos")]
    native_integrations: bool,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    ui_theme: UiThemeId,
}

/// Retired compact snapshots kept for reuse: the presenter holds the previous
/// frame's and the renderer may still hold an older one.
const COMPACT_SPARES: usize = 3;

impl GuiDriver {
    pub(super) fn new(
        game_path: PathBuf,
        arrows_as_numpad: bool,
        native_integrations: bool,
        addressing_24_bit: bool,
        screen_depth: Option<u16>,
        ui_theme: UiThemeId,
    ) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = native_integrations;
        static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let generation = NEXT_GENERATION
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |value| value.checked_add(1),
            )
            .expect("desktop runtime generation exhausted");
        Self {
            gpu: false,
            workspace: false,
            generation,
            snapshot_sequence: 0,
            warp_serial: 0,
            file_reply: None,
            dialog_text_replies: Default::default(),
            typed_text: None,
            pending_warp: None,
            #[cfg(target_os = "macos")]
            identity: None,
            display_generation: 0,
            snapshot_screen_mode: None,
            compact_cache: Default::default(),
            compact_snapshot: None,
            compact_spares: Vec::new(),
            runner: None,
            debug_server: None,
            save_store: None,
            guest_exit_reported: false,
            initialized: false,
            total_instructions: 0,
            start_time: None,
            next_frame_time: None,
            render_headroom: MIN_RENDER_HEADROOM,
            audio_sample_remainder: 0.0,
            last_audio_mix_time: None,
            mouse_release_latch: HostMouseReleaseLatch::default(),
            frame_count: 0,
            last_presented_guest_tick: None,
            force_next_render: true,
            game_path,
            arrows_as_numpad,
            addressing_24_bit,
            screen_depth,
            ui_theme,
            #[cfg(target_os = "macos")]
            native_integrations,
        }
    }

    pub(super) fn init_game(&mut self) {
        let _timing = FramePhaseTimer::new("guest initialization");
        if self.initialized {
            return;
        }

        let mut runner = match self.screen_depth {
            Some(screen_depth) => {
                game::new_runner_with_configuration(!self.addressing_24_bit, screen_depth)
            }
            None => game::new_runner_with_addressing(!self.addressing_24_bit),
        };
        runner.set_ui_theme(self.ui_theme);
        runner.set_host_ui_enabled(self.workspace);
        if self.workspace {
            runner.set_menu_bar_policy(MenuBarPolicy::ForceHidden);
        }
        if self.gpu {
            runner.set_q3_gpu_renderer(Box::new(super::wgpu_qd3d::Qd3dRenderer::new()));
            runner.set_quickdraw_gpu_renderer(Box::new(
                super::wgpu_quickdraw::QuickDrawRenderer::new(),
            ));
        }
        #[cfg(target_os = "macos")]
        if self.native_integrations {
            runner.set_menu_bar_policy(MenuBarPolicy::ForceHidden);
        }
        let app =
            game::load_game_from_path(&mut runner, &self.game_path).expect("Failed to load game");
        let mut save_store = DesktopSaveStore::for_loaded_archive(&self.game_path, &mut runner);
        eprintln!(
            "[SYSTEMLESS] Desktop save dir: {}",
            save_store.root().display()
        );
        let restored_saves = save_store.load_saved_files();
        for file in &restored_saves {
            runner.import_vfs_file(file);
        }
        if !restored_saves.is_empty() {
            eprintln!(
                "[SYSTEMLESS] Restored {} desktop save file(s)",
                restored_saves.len()
            );
        }
        let a5_base = app.a5_base;
        let entry_point = app.entry_point(a5_base);
        game::init_game_owned(&mut runner, app);
        runner.prepare_text_presentation();
        runner.set_arrows_as_numpad(self.arrows_as_numpad);

        let ipt = configure_realtime_execution_rate(&mut runner);
        eprintln!("[SYSTEMLESS] Instructions per tick: {}", ipt);

        // Initialize audio output.
        if let Some(audio) = systemless::systems::macintosh::audio::CpalAudioBackend::new() {
            runner.set_audio(Box::new(audio));
        } else {
            eprintln!("[SYSTEMLESS] Warning: could not initialize audio output");
        }

        eprintln!("[SYSTEMLESS] Game loaded: {}", self.game_path.display());
        eprintln!(
            "[SYSTEMLESS] A5=${:08X}, Entry=${:08X}",
            a5_base,
            entry_point
        );

        self.runner = Some(runner);
        self.save_store = Some(save_store);
        self.initialized = true;
    }

    pub(super) fn apply_command(&mut self, command: GuiCommand) {
        if let GuiCommand::AcknowledgeWarp { generation, serial } = command {
            if generation == self.generation
                && self.pending_warp.is_some_and(|warp| warp.serial == serial)
            {
                self.pending_warp = None;
            }
            return;
        }
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        self.force_next_render = true;
        if let GuiCommand::DialogText(reply) = command {
            let (serial, id) = (reply.serial, reply.id);
            let typed = runner
                .host_dialog()
                .is_some_and(|dialog| dialog.guest_filter);
            let accepted = runner.reply_host_dialog_text(&reply);
            if accepted && typed {
                self.typed_text = Some((serial, id, reply.item, 0));
                return;
            }
            let text = accepted
                .then(|| runner.host_dialog())
                .flatten()
                .filter(|dialog| dialog.id == id)
                .and_then(|dialog| {
                    dialog
                        .items
                        .into_iter()
                        .find(|item| item.number == reply.item)
                })
                .map(|item| item.text);
            self.dialog_text_replies.insert(
                id,
                std::sync::Arc::new(DialogTextAck {
                    serial,
                    id,
                    accepted,
                    text,
                }),
            );
            return;
        }
        if let GuiCommand::PopupMenu(reply) = command {
            runner.reply_host_popup_menu(reply);
            return;
        }
        if let GuiCommand::SystemDialog(reply) = command {
            runner.reply_host_system_dialog(reply);
            return;
        }
        if let GuiCommand::DismissNotification(id) = command {
            runner.dismiss_host_notification(id);
            return;
        }
        if let GuiCommand::SetBalloonHelp(enabled) = command {
            runner.set_balloon_help(enabled);
            return;
        }
        if let GuiCommand::Modeless(command) = command {
            use super::runtime_protocol::ModelessCommand;
            match command {
                ModelessCommand::Activate { id, revision } => {
                    runner.activate_host_modeless_dialog(id, revision);
                }
                ModelessCommand::Item(reply) => {
                    runner.reply_host_modeless_dialog(&reply);
                }
                ModelessCommand::Control(reply) => {
                    runner.reply_host_modeless_dialog_control(&reply);
                }
                ModelessCommand::Close { id, revision } => {
                    runner.close_host_modeless_dialog(id, revision);
                }
                ModelessCommand::Text(reply) => {
                    let (serial, id) = (reply.serial, reply.id);
                    let accepted = runner.reply_host_modeless_dialog_text(&reply);
                    let text = accepted
                        .then(|| runner.host_modeless_dialogs())
                        .and_then(|dialogs| dialogs.into_iter().find(|dialog| dialog.id == id))
                        .and_then(|dialog| {
                            dialog
                                .items
                                .into_iter()
                                .find(|item| item.number == reply.item)
                        })
                        .map(|item| item.text);
                    self.dialog_text_replies.insert(
                        id,
                        std::sync::Arc::new(DialogTextAck {
                            serial,
                            id,
                            accepted,
                            text,
                        }),
                    );
                }
            }
            return;
        }
        if let GuiCommand::FileDialog(reply) = command {
            let serial = self
                .file_reply
                .map_or(0, |r| r.0)
                .checked_add(1)
                .expect("dialog reply sequence exhausted");
            let (id, revision) = (reply.id, reply.revision);
            let accepted = runner.reply_host_file_dialog(reply);
            self.file_reply = Some((serial, id, revision, accepted));
            return;
        }
        Self::apply_guest_command(runner, &mut self.mouse_release_latch, command);
    }

    pub(super) fn apply_guest_command(
        runner: &mut FixtureRunner,
        mouse_release_latch: &mut HostMouseReleaseLatch,
        command: GuiCommand,
    ) {
        let modality = runner.ui_modality();
        if matches!(command, GuiCommand::Menu { .. }) && modality.menus_blocked {
            return;
        }
        if modality.owner != systemless::systems::macintosh::host_ui::UiOwner::Guest
            && matches!(
                command,
                GuiCommand::MouseMove { .. }
                    | GuiCommand::MouseDown { .. }
                    | GuiCommand::KeyDown { .. }
                    | GuiCommand::Menu { .. }
            )
        {
            return;
        }
        match command {
            GuiCommand::MouseMove { v, h } => {
                runner.set_mouse_position(v, h);
                runner.dispatcher_mut().show_cursor();
            }
            GuiCommand::MouseDown { v, h } => {
                // The pane shows a projected dialog's saved background, not
                // the hidden guest window still occupying that geometry.
                if runner.host_modeless_dialog_at(v, h) {
                    return;
                }
                runner.push_mouse_down(v, h);
                mouse_release_latch.press();
            }
            GuiCommand::MouseUp { v, h } => {
                if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                    runner.push_mouse_up(v, h);
                }
            }
            GuiCommand::KeyDown { key, character } => runner.push_key_down(key, character),
            GuiCommand::KeyUp { key, character } => runner.push_key_up(key, character),
            GuiCommand::Dialog(reply) => {
                let point = runner.host_dialog_item_position(&reply);
                if std::env::var_os("SYSTEMLESS_TRACE_ACCESSIBILITY").is_some() {
                    eprintln!("[ACCESSIBILITY] runtime dialog reply={reply:?} point={point:?}");
                }
                if let Some((v, h)) = point {
                    runner.push_mouse_down(v, h);
                    mouse_release_latch.press();
                    if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                        runner.push_mouse_up(v, h);
                    }
                }
            }
            GuiCommand::DialogPoint { reply, point } => {
                if let Some((v, h)) = runner.host_dialog_point_position(&reply, point) {
                    runner.push_mouse_down(v, h);
                    mouse_release_latch.press();
                    if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                        runner.push_mouse_up(v, h);
                    }
                }
            }
            GuiCommand::DialogControl(reply) => {
                if let Some((v, h)) = runner.reply_host_dialog_control(&reply) {
                    runner.push_mouse_down(v, h);
                    mouse_release_latch.press();
                    if let Some((v, h)) = mouse_release_latch.release((v, h)) {
                        runner.push_mouse_up(v, h);
                    }
                }
            }
            GuiCommand::DialogText(reply) => {
                runner.reply_host_dialog_text(&reply);
            }
            GuiCommand::FileDialog(reply) => {
                runner.reply_host_file_dialog(reply);
            }
            GuiCommand::Modeless(_)
            | GuiCommand::PopupMenu(_)
            | GuiCommand::SystemDialog(_)
            | GuiCommand::DismissNotification(_)
            | GuiCommand::SetBalloonHelp(_) => {}
            GuiCommand::Menu { menu, item } => {
                runner.select_guest_menu_item(menu, item);
            }
            GuiCommand::AcknowledgeWarp { .. } => unreachable!(),
        }
    }

    pub(super) fn capture_state(&mut self, output: &mut GuiState, native_models: bool) {
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if let Some(position) = runner.take_guest_cursor_warp() {
            self.warp_serial = self
                .warp_serial
                .checked_add(1)
                .expect("cursor warp serial exhausted");
            self.pending_warp = Some(CursorWarp {
                serial: self.warp_serial,
                position,
            });
        }
        if native_models && self.workspace {
            let menus = runner.guest_menu_snapshot();
            let changed = output.menus.as_ref().is_none_or(|old| **old != menus);
            if changed {
                output.menus = Some(std::sync::Arc::new(menus));
            }
            // Menu balloons load `hmnu` resources only when help is shown,
            // and again only when the menus change.
            let help = runner.balloon_help_enabled();
            if help && (changed || output.menu_help.is_none()) {
                output.menu_help = Some(std::sync::Arc::new(
                    runner.guest_menu_help().into_iter().collect(),
                ));
            } else if !help {
                output.menu_help = None;
            }
        }
        if self.workspace {
            output.live_dialogs = runner.host_dialog_ids();
            let general = runner.host_dialog();
            if output.dialog.as_deref() != general.as_ref() {
                output.dialog = general.map(std::sync::Arc::new);
            }
            let modeless = runner.host_modeless_dialogs();
            if output.modeless_dialogs.len() != modeless.len()
                || output
                    .modeless_dialogs
                    .iter()
                    .zip(&modeless)
                    .any(|(old, new)| **old != *new)
            {
                output.modeless_dialogs = modeless
                    .into_iter()
                    .map(|new| {
                        output
                            .modeless_dialogs
                            .iter()
                            .find(|old| ***old == new)
                            .cloned()
                            .unwrap_or_else(|| std::sync::Arc::new(new))
                    })
                    .collect();
            }
            output.popup_menu = runner.host_popup_menu();
            output.system_dialog = runner.host_system_dialog();
            output.notifications = runner.host_notifications();
            output.balloon = runner.host_balloon();
            output.balloon_help = runner.balloon_help_enabled();
            let window_help = runner.guest_window_help();
            if *output.window_help != window_help {
                output.window_help = std::sync::Arc::new(window_help);
            }
            output.guest_surface = runner.guest_modal_surface();
            output.guest_windows = runner.guest_modeless_surfaces();
            output.stopped = runner.unexpected_stop_reason();
            let dialog = runner.host_file_dialog();
            if output.file_dialog.as_deref() != dialog.as_ref() {
                output.file_dialog = dialog.map(std::sync::Arc::new);
            }
        }
        output.generation = self.generation;
        output.screen_mode = runner.dispatcher().screen_mode;
        output.cursor = runner.dispatcher().cursor().cloned();
        output.modality = runner.ui_modality();
        output.file_reply = self.file_reply;
        if let Some((serial, id, item, quiet)) = self.typed_text {
            if runner.host_typed_keys_pending() {
                self.typed_text = Some((serial, id, item, 0));
            } else if quiet < 1 {
                self.typed_text = Some((serial, id, item, quiet + 1));
            } else {
                self.typed_text = None;
                let text = runner
                    .host_dialog()
                    .filter(|dialog| dialog.id == id)
                    .and_then(|dialog| dialog.items.into_iter().find(|i| i.number == item))
                    .map(|item| item.text);
                self.dialog_text_replies.insert(
                    id,
                    std::sync::Arc::new(DialogTextAck {
                        serial,
                        id,
                        accepted: text.is_some(),
                        text,
                    }),
                );
            }
        }
        self.dialog_text_replies
            .retain(|id, _| output.live_dialogs.contains(id));
        output.dialog_text_replies = self.dialog_text_replies.values().cloned().collect();
        output
            .dialog_text_replies
            .sort_unstable_by_key(|reply| reply.id);
        output.warp = self.pending_warp;
        #[cfg(target_os = "macos")]
        {
            use systemless::systems::macintosh::memory::MemoryBus;
            output.dialog_bounds = runner
                .dispatcher()
                .visible_dialog_structure_bounds(runner.bus());
            output.hidden_menu_height = if self.native_integrations {
                runner
                    .bus()
                    .read_word(systemless::systems::macintosh::memory::globals::addr::MBAR_HEIGHT)
                    .into()
            } else {
                0
            };
            if native_models && self.native_integrations {
                let path = runner.dispatcher().launched_app_path();
                if self
                    .identity
                    .as_ref()
                    .map(|identity| identity.path.as_str())
                    != path
                {
                    self.identity =
                        game::loaded_application_identity(runner).map(std::sync::Arc::new);
                }
                output.identity = self.identity.clone();
                let menus = runner.guest_menu_snapshot();
                if output.menus.as_ref().is_none_or(|old| **old != menus) {
                    output.menus = Some(std::sync::Arc::new(menus));
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = native_models;
    }

    pub(super) fn pump_debugger(&mut self) {
        if let (Some(server), Some(runner)) = (self.debug_server.as_mut(), self.runner.as_mut()) {
            server.pump(runner);
        }
    }

    pub(super) fn finish_frame(&mut self) {
        if let Some(runner) = self.runner.as_mut() {
            runner.finish_gui_frame();
        }
        self.frame_count += 1;
    }

    pub(super) fn capture_frame(
        &mut self,
        output: &mut super::frame_snapshot::GuiFrame,
        debug: Option<systemless::systems::macintosh::debug_overlay::DebugOverlayFrameStats>,
        capture_crop: bool,
        learning_crop: bool,
    ) -> bool {
        let Some(runner) = self.runner.as_mut() else {
            return false;
        };
        {
            let _timing = FramePhaseTimer::new("outline palette preparation");
            runner.prepare_text_presentation();
        }
        {
            let _timing = FramePhaseTimer::new("window compositing");
            runner.composite_frame();
        }
        let _timing = FramePhaseTimer::new("owned frame export");
        let mode = runner.dispatcher().screen_mode;
        let learning_crop = learning_crop || self.snapshot_screen_mode != Some(mode);
        if self.snapshot_screen_mode != Some(mode) {
            self.display_generation = self
                .display_generation
                .checked_add(1)
                .expect("desktop display generation exhausted");
            self.snapshot_screen_mode = Some(mode);
        }
        self.snapshot_sequence = self
            .snapshot_sequence
            .checked_add(1)
            .expect("desktop frame sequence exhausted");
        output.generation = self.generation;
        output.sequence = self.snapshot_sequence;
        output.display_generation = self.display_generation;
        output.guest_tick = runner.guest_tick();
        output.screen.capture(
            runner.bus(),
            mode,
            &runner.dispatcher().device_clut,
            &runner.dispatcher().device_gamma(),
        );
        output.cursor = runner.dispatcher().cursor().cloned();
        output.mouse_position = runner.dispatcher().mouse_position();
        output.debug_lines = debug
            .map(|stats| runner.debug_overlay_snapshot(stats).lines())
            .unwrap_or_default();
        if runner.bus().has_visible_outline_detail() {
            match self
                .compact_cache
                .prepare_changed(runner.bus(), (mode.2.into(), mode.3.into()))
            {
                Some(changed) => {
                    if changed || self.compact_snapshot.is_none() {
                        let snapshot = Self::next_compact_snapshot(
                            self.compact_cache.frame(),
                            &mut self.compact_spares,
                        );
                        if let Some(retired) = self.compact_snapshot.replace(snapshot) {
                            if self.compact_spares.len() < COMPACT_SPARES {
                                self.compact_spares.push(retired);
                            }
                        }
                    }
                    output.retained = self.compact_snapshot.clone();
                }
                None => {
                    output.retained = None;
                    self.compact_snapshot = None;
                }
            }
        } else {
            output.retained = None;
            self.compact_snapshot = None;
        }
        #[cfg(target_os = "macos")]
        {
            use systemless::systems::macintosh::memory::MemoryBus;
            let dispatcher = runner.dispatcher();
            output.crop = super::frame_snapshot::CropObservations {
                dialog_bounds: dispatcher.visible_dialog_structure_bounds(runner.bus()),
                framed_rect: capture_crop
                    .then(|| dispatcher.framed_manual_cport_presentation_rect(runner.bus()))
                    .flatten(),
                manual_rect: (capture_crop && learning_crop)
                    .then(|| dispatcher.manual_cport_presentation_rect(runner.bus()))
                    .flatten(),
                declared_rect: (capture_crop && learning_crop)
                    .then(|| dispatcher.declared_centered_presentation_rect(runner.bus()))
                    .flatten(),
                copybits_count: dispatcher.copybits_screen_count,
                last_copybits_rect: dispatcher.last_screen_copybits_rect,
                hidden_menu_height: if self.native_integrations {
                    runner
                        .bus()
                        .read_word(
                            systemless::systems::macintosh::memory::globals::addr::MBAR_HEIGHT,
                        )
                        .into()
                } else {
                    0
                },
            };
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (capture_crop, learning_crop);
        true
    }

    /// A snapshot of the compact cache's frame. A spare no one else still
    /// holds takes the frame in its existing buffers: exporting a new
    /// multi-megabyte image every frame otherwise allocates it here and
    /// frees the previous one once presented, and each such free returns
    /// the pages to the system.
    fn next_compact_snapshot(
        frame: &systemless::systems::macintosh::memory::CompactPresentation,
        spares: &mut Vec<
            std::sync::Arc<systemless::systems::macintosh::memory::CompactPresentation>,
        >,
    ) -> std::sync::Arc<systemless::systems::macintosh::memory::CompactPresentation> {
        for index in 0..spares.len() {
            if let Some(spare) = std::sync::Arc::get_mut(&mut spares[index]) {
                spare.clone_from(frame);
                return spares.swap_remove(index);
            }
        }
        std::sync::Arc::new(frame.clone())
    }

    pub(super) fn sync_save_files(&mut self, force: bool) {
        let Some(save_store) = self.save_store.as_mut() else {
            return;
        };
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        if force {
            save_store.sync_save_files_now(runner);
        } else {
            save_store.sync_save_files(runner);
        }
    }

    pub(super) fn guest_requested_exit(&self) -> bool {
        self.runner
            .as_ref()
            .is_some_and(FixtureRunner::halted_by_exit_to_shell)
    }

    /// Wall-clock origin such that `tick_due_at(origin, now)` equals `guest_tick`.
    /// Shifts the origin back so a boot-seeded, non-zero TickCount does not make
    /// the pacer wait real time before running any guest CPU work.
    pub(super) fn wall_clock_origin_for_guest_tick(
        now: std::time::Instant,
        guest_tick: u32,
    ) -> std::time::Instant {
        // Add a half-tick of lead before flooring so `tick_due_at` reliably
        // maps `now` back to `guest_tick` (rather than `guest_tick - 1` after
        // float truncation), guaranteeing the first frame already has runnable
        // guest work. The half-tick (~8ms) lead is sub-frame and harmless.
        now.checked_sub(std::time::Duration::from_secs_f64(
            (guest_tick as f64 + 0.5) / systemless::systems::macintosh::runner::DEFAULT_VBL_HZ,
        ))
        .unwrap_or(now)
    }

    pub(super) fn tick_due_at(origin: std::time::Instant, at: std::time::Instant) -> u32 {
        at.checked_duration_since(origin)
            .unwrap_or_default()
            .as_secs_f64()
            .mul_add(systemless::systems::macintosh::runner::DEFAULT_VBL_HZ, 0.0)
            .floor() as u32
    }

    pub(super) fn audio_samples_for_duration(
        duration: std::time::Duration,
        remainder: &mut f64,
    ) -> usize {
        let total_samples = duration.as_secs_f64().mul_add(
            systemless::systems::macintosh::sound::OUTPUT_RATE as f64,
            *remainder,
        );
        let whole_samples = total_samples.floor();
        *remainder = total_samples - whole_samples;
        whole_samples as usize
    }

    pub(super) fn next_render_headroom(render_time: std::time::Duration) -> std::time::Duration {
        let target = render_time.saturating_add(RENDER_HEADROOM_MARGIN);
        target.clamp(MIN_RENDER_HEADROOM, MAX_RENDER_HEADROOM)
    }

    pub(super) fn next_frame_target(
        now: std::time::Instant,
        scheduled: std::time::Instant,
    ) -> (std::time::Instant, bool) {
        if now.saturating_duration_since(scheduled) >= FRAME_DURATION {
            (now + FRAME_DURATION, true)
        } else {
            (scheduled + FRAME_DURATION, false)
        }
    }

    pub(super) fn flush_ready_mouse_release(&mut self) {
        let Some((v, h)) = self.mouse_release_latch.take_ready_release() else {
            return;
        };
        if let Some(runner) = self.runner.as_mut() {
            runner.push_mouse_up(v, h);
        }
    }

    pub(super) fn step_frame(&mut self) {
        self.step_frame_with_clock(std::time::Instant::now);
    }

    pub(super) fn step_frame_with_clock(&mut self, host_now: impl FnMut() -> std::time::Instant) {
        self.step_frame_with_safe_points(host_now, |_, _| true);
    }

    /// The callback runs only between complete guest CPU/audio batches. A
    /// false result requests shutdown; it cannot preempt a long Toolbox call.
    pub(super) fn step_frame_with_safe_points(
        &mut self,
        mut host_now: impl FnMut() -> std::time::Instant,
        mut safe_point: impl FnMut(&mut FixtureRunner, &mut HostMouseReleaseLatch) -> bool,
    ) {
        let _timing = FramePhaseTimer::new("CPU and audio frame");
        let Some(runner) = self.runner.as_ref() else {
            return;
        };

        if runner.is_halted() {
            return;
        }

        let now = host_now();
        // Seed the wall-clock origin from the guest's current tick, not `now`.
        // The runner boots with a non-zero TickCount (DEFAULT_LAUNCH_TICKS ≈ 600
        // ≈ 10s of simulated post-boot time), so anchoring the origin at `now`
        // would leave the guest clock 600 ticks "ahead" of the wall clock. With
        // `ticks_behind` saturating to 0, the CPU loop would advance no work for
        // ~10 real seconds until the wall clock caught up — a launch stall. See
        // wall_clock_origin_for_guest_tick in systemless.org/src/emulator.rs.
        let start = *self.start_time.get_or_insert_with(|| {
            Self::wall_clock_origin_for_guest_tick(now, runner.guest_tick())
        });
        let scheduled_frame_end = self.next_frame_time.unwrap_or(now + FRAME_DURATION);

        // Wall-clock tick target: where the game clock should be right now.
        let target_tick = Self::tick_due_at(start, scheduled_frame_end);
        let current_tick = runner.guest_tick();

        // Cap ticks-to-advance at 2 per frame. If the game is behind,
        // we accept the lag rather than trying to catch up (which causes
        // the CPU to run for 100ms+ and drops frames further). When the
        // game is more than 2 ticks behind, we reset the wall-clock
        // origin so it can recover without a runaway spiral.
        let ticks_behind = target_tick.saturating_sub(current_tick);
        if ticks_behind > 4 {
            // Game fell too far behind — snap the wall-clock origin forward
            // so the target aligns with where the game actually is.
            // This prevents the death spiral where each frame tries to
            // catch up, takes too long, falls further behind, repeat.
            self.start_time = Some(
                now - std::time::Duration::from_secs_f64(
                    (current_tick + 2) as f64
                        / systemless::systems::macintosh::runner::DEFAULT_VBL_HZ,
                ),
            );
        }
        // Host input wakes the foreground application even when its TickCount
        // is ahead of the wall-clock target. Give each mouse transition one
        // bounded guest slice so a polling loop cannot be starved by pacing.
        let input_progress_ticks = u32::from(self.mouse_release_latch.requires_guest_progress());
        let effective_target =
            current_tick.saturating_add(ticks_behind.min(2).max(input_progress_ticks));

        // CPU budget: wall-clock time left in this frame, minus render headroom.
        // The CPU runs in small batches, checking the clock between batches.
        let cpu_deadline = scheduled_frame_end
            .checked_sub(self.render_headroom)
            .map(|d| d.max(now))
            .unwrap_or(now);

        let slice_budget = game::MAX_INSTRUCTIONS_PER_FRAME;
        let presentation_interval = self
            .last_audio_mix_time
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or(FRAME_DURATION);
        let audio_interval = presentation_interval.min(MAX_AUDIO_MIX_INTERVAL);
        let audio_samples =
            Self::audio_samples_for_duration(audio_interval, &mut self.audio_sample_remainder);
        if std::env::var_os("SYSTEMLESS_TRACE_AUDIO").is_some()
            && audio_interval > FRAME_DURATION + FRAME_DURATION / 2
        {
            eprintln!(
                "[AUDIO] recovering {:.1} ms of host time ({} source samples)",
                audio_interval.as_secs_f64() * 1000.0,
                audio_samples
            );
        }

        let runner = self.runner.as_mut().expect("runner checked above");
        runner.advance_menu_presentation_clock(presentation_interval);
        // A PPC HLE slice currently borrows its large mutable state by moving
        // collections into a dispatch closure and restoring them afterward.
        // Yield a few times per guest VBL rather than paying that boundary
        // thousands of times per second, so the wall-clock CPU deadline is
        // still rechecked within a tick. The interpreter stops at the tick cap.
        let foreground_batch_instructions = foreground_cpu_batch_instructions(
            runner.is_powerpc_app(),
            runner.instructions_per_tick(),
        );

        // Mix one host frame of audio per GUI frame. Sound Manager doubleback
        // callbacks run at interrupt time, including while menu/control
        // tracking keeps the application-visible TickCount fixed, so same-tick
        // frames still need audio. Do not catch up multiple late host frames at
        // once: that drains SndPlayDoubleBuffer queues faster than their
        // callbacks can refill them and turns low-rate effects into fragments.
        // Sound 1994, 2-72 and 2-146 to 2-148.
        let mut audio_mixed = 0usize;
        let mut total_steps = 0usize;
        let mut foreground_steps = 0usize;
        let mut reserved_sound_steps = 0usize;

        let mut cancelled = false;
        loop {
            if !safe_point(runner, &mut self.mouse_release_latch) {
                cancelled = true;
                break;
            }
            if runner.guest_tick() >= effective_target || runner.is_halted() {
                break;
            }
            if host_now() >= cpu_deadline {
                break;
            }

            let remaining = slice_budget.saturating_sub(total_steps);
            if remaining == 0 {
                break;
            }

            let batch_size = remaining.min(foreground_batch_instructions);
            let remaining_audio = audio_samples.saturating_sub(audio_mixed);
            let batches_left = remaining.div_ceil(foreground_batch_instructions).max(1);
            let batch_audio = if remaining_audio == 0 {
                0
            } else {
                remaining_audio.div_ceil(batches_left)
            };
            let (steps, running) = {
                let _timing = FramePhaseTimer::new("foreground CPU batch");
                runner.run_gui_cpu_slice(batch_size, effective_target)
            };
            total_steps += steps;
            foreground_steps += steps;
            audio_mixed += batch_audio;
            if batch_audio > 0 {
                // CPU batches share one presentation pass in render_frame.
                // Keep audio callbacks serviced without repainting every window.
                runner.mix_gui_audio_slice(batch_audio);
                if let Some(steps) = service_pending_sound_work(
                    runner,
                    cpu_deadline,
                    slice_budget,
                    total_steps,
                    &mut reserved_sound_steps,
                ) {
                    total_steps += steps;
                }
            }
            if !running || runner.is_ui_tracking_active() {
                break;
            }
        }

        if !cancelled && audio_mixed < audio_samples {
            if let Some(steps) = service_pending_sound_work(
                runner,
                cpu_deadline,
                slice_budget,
                total_steps,
                &mut reserved_sound_steps,
            ) {
                total_steps += steps;
            }
        }

        if !cancelled && audio_mixed < audio_samples {
            let mut remaining_audio = audio_samples - audio_mixed;
            while remaining_audio > 0 && !runner.is_halted() {
                if !safe_point(runner, &mut self.mouse_release_latch) {
                    cancelled = true;
                    break;
                }
                let chunk_audio = remaining_audio.min(AUDIO_CALLBACK_CHUNK_SAMPLES);
                runner.mix_gui_audio_slice(chunk_audio);
                remaining_audio -= chunk_audio;
                if let Some(steps) = service_pending_sound_work(
                    runner,
                    cpu_deadline,
                    slice_budget,
                    total_steps,
                    &mut reserved_sound_steps,
                ) {
                    total_steps += steps;
                }
            }
        }

        if !cancelled {
            if let Some(steps) = service_pending_sound_work(
                runner,
                cpu_deadline,
                slice_budget,
                total_steps,
                &mut reserved_sound_steps,
            ) {
                total_steps += steps;
            }
        }

        self.total_instructions += total_steps as u64;
        if foreground_steps > 0 {
            self.mouse_release_latch.observe_guest_progress();
        }
        if foreground_steps > 0 && runner.guest_tick() == current_tick {
            // Loading and animation code can draw substantial work before the
            // next VBL tick. Present that progress instead of batching it into
            // a later tick, which makes startup look choppy.
            self.force_next_render = true;
        }

        // Optional tick-lag instrumentation. Gate on
        // SYSTEMLESS_TRACE_TICK_LAG=1. Logs target/current tick counts and
        // CPU budget vs instructions actually executed each frame.
        //   - Logs EVERY frame when ticks_behind > 0 (lag event).
        //   - Also logs ONCE PER SECOND (every 60 frames) as a steady-
        //     state sample so the user sees baseline performance.
        // Interpretation: if cpu_used / slice_budget < 1.0 consistently,
        // the host CPU can't keep up with the 25 MHz target and
        // animations will lag.
        if std::env::var_os("SYSTEMLESS_TRACE_TICK_LAG").is_some() {
            let final_tick = runner.guest_tick();
            let advanced = final_tick.saturating_sub(current_tick);
            let steady_sample = self.frame_count.is_multiple_of(60);
            if ticks_behind > 0 || steady_sample {
                let tag = if ticks_behind > 0 { "LAG" } else { "OK " };
                eprintln!(
                    "[TICK_LAG {}] frame={} target={} current={} behind={} \
                     advanced={} budget={} used={}",
                    tag,
                    self.frame_count,
                    target_tick,
                    current_tick,
                    ticks_behind,
                    advanced,
                    slice_budget,
                    total_steps,
                );
            }
        }
    }

    pub(super) fn should_render_frame(&self, debug_overlay_visible: bool) -> bool {
        if self.force_next_render {
            return true;
        }
        if debug_overlay_visible {
            return true;
        }
        let Some(runner) = self.runner.as_ref() else {
            return false;
        };
        runner.is_halted()
            || runner.is_ui_tracking_active()
            || self.last_presented_guest_tick != Some(runner.guest_tick())
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::frame_snapshot::GuiFrame;
    use systemless::systems::macintosh::memory::MemoryBus;

    fn driver() -> GuiDriver {
        let mut driver = GuiDriver::new(
            "snapshot-test".into(),
            false,
            false,
            false,
            Some(8),
            UiThemeId::ClassicSystem7,
        );
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, Default::default());
        let address = runner.bus_mut().alloc(32 * 32);
        runner.dispatcher_mut().screen_mode = (address, 32, 32, 32, 8);
        runner.bus_mut().fill_bytes(address, 32 * 32, 1);
        driver.runner = Some(runner);
        driver
    }

    #[test]
    fn workspace_file_dialog_round_trips_through_execution_owner() {
        use systemless::systems::macintosh::cpu::Register;
        use systemless::systems::macintosh::host_ui::{FileDialogAction, FileDialogReply};
        let mut driver = driver();
        driver.workspace = true;
        let runner = driver.runner.as_mut().unwrap();
        runner.set_host_ui_enabled(true);
        let base = 0x10000;
        let sp = 0x100000;
        let result = 0x110000;
        runner.bus_mut().write_word(base, 0xA9EA);
        runner.bus_mut().write_word(sp, 6);
        runner.bus_mut().write_long(sp + 2, result);
        runner.bus_mut().write_long(sp + 6, 0);
        runner.bus_mut().write_word(sp + 10, 0);
        runner.bus_mut().write_long(sp + 12, 0);
        runner.cpu_mut().write_reg(Register::PC, base);
        runner.cpu_mut().write_reg(Register::A7, sp);
        runner.run_gui_slice_with_audio(1, 0, 0);
        let mut state = GuiState::default();
        driver.capture_state(&mut state, false);
        let dialog = state
            .file_dialog
            .clone()
            .expect("dialog crosses snapshot boundary");
        let reply = FileDialogReply {
            id: dialog.id,
            revision: dialog.revision,
            action: FileDialogAction::Cancel,
        };
        driver.apply_command(GuiCommand::FileDialog(reply.clone()));
        driver
            .runner
            .as_mut()
            .unwrap()
            .run_gui_slice_with_audio(1, 0, 0);
        driver.capture_state(&mut state, false);
        assert!(state.file_dialog.is_none());
        assert_eq!(
            state.file_reply,
            Some((1, dialog.id, dialog.revision, true))
        );
        assert_eq!(driver.runner.as_ref().unwrap().bus().read_byte(result), 0);
        driver.apply_command(GuiCommand::FileDialog(reply));
        driver.capture_state(&mut state, false);
        assert_eq!(
            state.file_reply,
            Some((2, dialog.id, dialog.revision, false))
        );
    }

    #[test]
    fn text_acknowledgements_survive_multiple_dialog_commands_before_a_snapshot() {
        use systemless::systems::macintosh::cpu::Register;
        use systemless::systems::macintosh::host_ui::DialogTextReply;
        let mut driver = driver();
        driver.workspace = true;
        let runner = driver.runner.as_mut().unwrap();
        runner.set_host_ui_enabled(true);
        let base = 0x10000;
        let sp = 0x100000;
        let bounds = runner.bus_mut().alloc(8);
        for (offset, value) in [(0, 5), (2, 5), (4, 25), (6, 25)] {
            runner.bus_mut().write_word(bounds + offset, value);
        }
        let mut windows = Vec::new();
        for _ in 0..2 {
            runner.bus_mut().write_word(base, 0xA97D); // NewDialog
            runner.bus_mut().fill_bytes(sp, 34, 0);
            runner.bus_mut().write_long(sp + 10, u32::MAX);
            runner.bus_mut().write_word(sp + 14, 1);
            runner.bus_mut().write_long(sp + 22, bounds);
            runner.cpu_mut().write_reg(Register::PC, base);
            runner.cpu_mut().write_reg(Register::A7, sp);
            runner.run_gui_slice_with_audio(1, 0, 0);
            let window = runner.bus().read_long(sp + 30);
            assert_ne!(window, 0);
            windows.push(window);
        }
        let ids = runner.host_dialog_ids();
        assert_eq!(ids.len(), 2);
        // Neither window is modal: both commands are rejected, but each live
        // editor must receive its own answer rather than waiting indefinitely.
        for id in &ids {
            driver.apply_command(GuiCommand::DialogText(DialogTextReply {
                serial: 1,
                id: *id,
                revision: 0,
                item: 1,
                expected: String::new(),
                text: "draft".into(),
                selection: (5, 5),
            }));
        }
        let mut state = GuiState::default();
        driver.capture_state(&mut state, false);
        assert_eq!(state.live_dialogs, ids);
        assert_eq!(state.dialog_text_replies.len(), 2);
        for (ack, id) in state.dialog_text_replies.iter().zip(&ids) {
            assert_eq!(ack.id, *id);
            assert_eq!(ack.serial, 1);
            assert!(!ack.accepted);
            assert!(ack.text.is_none());
        }
        driver.capture_state(&mut state, false);
        assert_eq!(
            state.dialog_text_replies.len(),
            2,
            "snapshot replacement preserves both answers"
        );
        let runner = driver.runner.as_mut().unwrap();
        runner.bus_mut().write_word(base, 0xA983); // DisposeDialog
        runner.bus_mut().write_long(sp, windows[1]);
        runner.cpu_mut().write_reg(Register::PC, base);
        runner.cpu_mut().write_reg(Register::A7, sp);
        runner.run_gui_slice_with_audio(1, 0, 0);
        driver.capture_state(&mut state, false);
        assert_eq!(state.live_dialogs.len(), 1);
        assert_eq!(state.dialog_text_replies.len(), 1);
        assert_eq!(state.dialog_text_replies[0].id, state.live_dialogs[0]);
    }

    /// Opt-in real-application smoke check. Set SYSTEMLESS_REAL_APPS to a
    /// colon-separated list of archives. Each runs with the host UI, answering
    /// any Systemless-owned interface it raises, and is compared with a run
    /// whose host UI is disabled: the host UI must not introduce a stop.
    #[test]
    #[ignore = "requires locally supplied application archives"]
    fn workspace_real_application_smoke() {
        use super::super::runtime_protocol::ModelessCommand;
        use systemless::systems::macintosh::host_ui::{
            DialogReply, FileDialogAction, FileDialogReply, PopupMenuReply, SystemDialogAnswer,
            SystemDialogReply,
        };
        let Some(list) = std::env::var_os("SYSTEMLESS_REAL_APPS") else {
            eprintln!("SYSTEMLESS_REAL_APPS is unset; nothing to check");
            return;
        };
        let frames: usize = std::env::var("SYSTEMLESS_REAL_APP_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1800);
        let mut failures = Vec::new();
        for archive in std::env::split_paths(&list) {
            let run = |host: bool| {
                let mut driver = driver();
                driver.workspace = host;
                let mut runner = game::new_runner_with_configuration(true, 8);
                runner.set_host_ui_enabled(host);
                runner.set_app_start_time(3_000_000_000);
                let app = match game::load_game_from_path(&mut runner, &archive) {
                    Ok(app) => app,
                    Err(error) => return Err(format!("load failed: {error}")),
                };
                game::init_game(&mut runner, &app);
                configure_realtime_execution_rate(&mut runner);
                driver.runner = Some(runner);
                let mut state = GuiState::default();
                let mut seen = std::collections::BTreeMap::<&str, usize>::new();
                for frame in 0..frames {
                    let runner = driver.runner.as_mut().unwrap();
                    let work = super::super::headless_time::frame(runner, 0);
                    // Deliver deferred releases as the desktop driver does.
                    if work.foreground > 0 {
                        driver.mouse_release_latch.observe_guest_progress();
                    }
                    if let Some((v, h)) = driver.mouse_release_latch.take_ready_release() {
                        runner.push_mouse_up(v, h);
                    }
                    if driver.runner.as_ref().unwrap().is_halted() {
                        break;
                    }
                    // Explore Systemless-owned UI the application exposes:
                    // About, Open… and Preferences…, when present.
                    if frame > 0 && frame % 400 == 0 {
                        let runner = driver.runner.as_mut().unwrap();
                        let menus = runner.guest_menu_snapshot();
                        let wanted = ["about", "open", "preferences"][(frame / 400 - 1) % 3];
                        let choice = menus.menus.iter().find_map(|menu| {
                            menu.items
                                .iter()
                                .find(|item| {
                                    item.enabled
                                        && item.submenu_id.is_none()
                                        && item.text.to_lowercase().starts_with(wanted)
                                })
                                .map(|item| (menu.id, item.number))
                        });
                        if let Some((menu, item)) = choice {
                            runner.select_guest_menu_item(menu, item);
                            *seen
                                .entry(
                                    ["menu:about", "menu:open", "menu:preferences"]
                                        [(frame / 400 - 1) % 3],
                                )
                                .or_default() += 1;
                        }
                    }
                    if !host || frame % 15 != 0 {
                        continue;
                    }
                    driver.capture_state(&mut state, true);
                    // Answer whatever Systemless-owned UI is waiting.
                    if let Some(dialog) = state.dialog.clone().filter(|d| !d.busy) {
                        *seen.entry("dialog").or_default() += 1;
                        if std::env::var_os("SYSTEMLESS_REAL_APP_TRACE").is_some() {
                            eprintln!(
                                "[REAL-APP-DIALOG] frame={frame} title={:?} default={} cancel={} items={:?}",
                                dialog.title,
                                dialog.default_item,
                                dialog.cancel_item,
                                dialog.items.iter().map(|i| (i.number, i.kind, i.enabled, i.text.clone())).collect::<Vec<_>>()
                            );
                        }
                        // Prefer buttons that continue without input, then
                        // the default and cancel items.
                        let preferred = dialog.items.iter().find(|i| {
                            i.kind == 4
                                && i.enabled
                                && ["demo", "continue", "skip", "ok", "done"]
                                    .iter()
                                    .any(|word| i.text.to_lowercase().starts_with(word))
                        });
                        let item = preferred.map(|i| i.number).or_else(|| {
                            [dialog.default_item, dialog.cancel_item]
                                .into_iter()
                                .find(|item| {
                                    dialog
                                        .items
                                        .iter()
                                        .any(|i| i.number == *item && i.enabled && i.kind != 0)
                                })
                        });
                        if let Some(item) = item {
                            driver.apply_command(GuiCommand::Dialog(DialogReply {
                                id: dialog.id,
                                revision: dialog.revision,
                                item,
                            }));
                        } else {
                            // No standard button: click an application-drawn
                            // control that reads like one, or anywhere in a
                            // dialog whose filter handles clicks itself.
                            let target = dialog
                                .items
                                .iter()
                                .filter(|i| i.kind == 0 && i.enabled && i.guest_rect.is_some())
                                .find(|i| {
                                    ["demo", "continue", "skip", "ok", "done", "cancel"]
                                        .iter()
                                        .any(|word| i.text.to_lowercase().starts_with(word))
                                })
                                .or_else(|| {
                                    dialog
                                        .items
                                        .iter()
                                        .filter(|_| dialog.guest_filter)
                                        .find(|i| i.guest_rect.is_some())
                                });
                            if let Some(target) = target {
                                *seen.entry("dialog-point").or_default() += 1;
                                let (top, left, bottom, right) = target.rect;
                                driver.apply_command(GuiCommand::DialogPoint {
                                    reply: DialogReply {
                                        id: dialog.id,
                                        revision: dialog.revision,
                                        item: target.number,
                                    },
                                    point: ((bottom - top) / 2, (right - left) / 2),
                                });
                            }
                        }
                    }
                    if let Some(dialog) = state.file_dialog.clone().filter(|d| !d.busy) {
                        *seen.entry("file").or_default() += 1;
                        driver.apply_command(GuiCommand::FileDialog(FileDialogReply {
                            id: dialog.id,
                            revision: dialog.revision,
                            action: FileDialogAction::Cancel,
                        }));
                    }
                    if let Some(popup) = state.popup_menu.clone() {
                        *seen.entry("popup").or_default() += 1;
                        driver.apply_command(GuiCommand::PopupMenu(PopupMenuReply {
                            id: popup.id,
                            choice: None,
                        }));
                    }
                    if let Some(dialog) = state.system_dialog.clone() {
                        *seen.entry("system").or_default() += 1;
                        driver.apply_command(GuiCommand::SystemDialog(SystemDialogReply {
                            id: dialog.id,
                            answer: SystemDialogAnswer::Cancel,
                        }));
                    }
                    for note in state.notifications.clone() {
                        *seen.entry("notification").or_default() += 1;
                        driver.apply_command(GuiCommand::DismissNotification(note.id));
                    }
                    if state.guest_surface.is_some() {
                        *seen.entry("guest-modal").or_default() += 1;
                    }
                    if !state.guest_windows.is_empty() {
                        *seen.entry("guest-window").or_default() += 1;
                    }
                    if let Some(window) = state.modeless_dialogs.first().filter(|d| !d.busy) {
                        *seen.entry("modeless").or_default() += 1;
                        if window.closable {
                            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Close {
                                id: window.id,
                                revision: window.revision,
                            }));
                        }
                    }
                }
                let runner = driver.runner.as_ref().unwrap();
                Ok((runner.unexpected_stop_reason(), seen))
            };
            let name = archive.display().to_string();
            match (run(false), run(true)) {
                (Ok((baseline, _)), Ok((hosted, seen))) => {
                    eprintln!("[REAL-APP] {name}: baseline={baseline:?} hosted={hosted:?} host-ui={seen:?}");
                    if hosted.is_some() && baseline.is_none() {
                        failures.push(format!("{name}: host UI stopped the guest: {hosted:?}"));
                    }
                }
                (Err(error), _) | (_, Err(error)) => {
                    eprintln!("[REAL-APP] {name}: skipped ({error})")
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// A dialog whose application filters events stays in egui: edits reach
    /// TextEdit as keystrokes through ModalDialog and the filter, and the
    /// acknowledgement waits for the guest's result.
    #[test]
    #[ignore = "runs the source-built Macintosh application"]
    fn workspace_showcase_filtered_dialog_types_edits_through_the_filter() {
        use systemless::systems::macintosh::memory::MemoryBus;
        use systemless::systems::macintosh::host_ui::DialogTextReply;
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        let mut driver = driver();
        driver.workspace = true;
        let mut runner = game::new_runner_with_configuration(true, 8);
        runner.set_prefer_powerpc_executables(false);
        runner.set_host_ui_enabled(true);
        runner.set_app_start_time(3_000_000_000);
        let app = game::load_game_from_path(&mut runner, &archive).unwrap();
        game::init_game(&mut runner, &app);
        configure_realtime_execution_rate(&mut runner);
        for _ in 0..60 {
            super::super::headless_time::frame(&mut runner, 0);
            if runner
                .guest_menu_snapshot()
                .menus
                .iter()
                .any(|m| m.id == 132)
            {
                break;
            }
        }
        assert!(runner.select_guest_menu_item(132, 6));
        for _ in 0..120 {
            super::super::headless_time::frame(&mut runner, 0);
            if runner.host_dialog().is_some() {
                break;
            }
        }
        // An application filter that declines every event, at an address of
        // the application's own: the standard filter's code, copied.
        let filter = runner.bus_mut().alloc(16);
        for (offset, word) in [
            (0, 0x4ef9u16),
            (6, 0x7000),
            (8, 0x426f),
            (10, 0x0010),
            (12, 0x4e74),
            (14, 0x000c),
        ] {
            runner.bus_mut().write_word(filter + offset, word);
        }
        runner.bus_mut().write_long(filter + 2, filter + 6);
        runner
            .dispatcher_mut()
            .dialog_tracking
            .as_mut()
            .unwrap()
            .filter_proc = filter;
        driver.runner = Some(runner);
        let mut state = GuiState::default();
        driver.capture_state(&mut state, false);
        let source = state.dialog.clone().expect("filtered dialogs stay in egui");
        assert!(source.guest_filter);
        assert!(source.items.iter().all(|item| item.guest_rect.is_some()));
        let field = source
            .items
            .iter()
            .find(|i| i.kind == 16 && i.enabled)
            .unwrap()
            .clone();
        driver.apply_command(GuiCommand::DialogText(DialogTextReply {
            id: source.id,
            revision: source.revision,
            serial: 1,
            expected: field.text.clone(),
            item: field.number,
            text: "Renée".into(),
            selection: (5, 5),
        }));
        driver.capture_state(&mut state, false);
        assert!(
            state.dialog_text_replies.is_empty(),
            "acknowledged only after the guest consumes the keystrokes"
        );
        let runner = driver.runner.as_mut().unwrap();
        assert!(runner.host_typed_keys_pending());
        for _ in 0..60 {
            super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
            driver.capture_state(&mut state, false);
            if !state.dialog_text_replies.is_empty() {
                break;
            }
        }
        let ack = state
            .dialog_text_replies
            .first()
            .expect("acknowledged")
            .clone();
        assert!(ack.accepted);
        assert_eq!(
            ack.text.as_deref(),
            Some("Renée"),
            "TextEdit received each keystroke"
        );
        // Edits inside the text arrive at the selection, as TEKey applies them.
        // The Showcase reenters ModalDialog without a filter after each item
        // hit, so the application filter is reinstated for each step.
        let refilter = |driver: &mut GuiDriver, state: &mut GuiState| {
            driver
                .runner
                .as_mut()
                .unwrap()
                .dispatcher_mut()
                .dialog_tracking
                .as_mut()
                .unwrap()
                .filter_proc = filter;
            driver.capture_state(state, false);
            assert!(state.dialog.as_ref().unwrap().guest_filter);
        };
        for (serial, (before, after)) in (2..).zip([("Renée", "Re-née"), ("Re-née", "Rnée")]) {
            refilter(&mut driver, &mut state);
            let dialog = state.dialog.clone().unwrap();
            driver.apply_command(GuiCommand::DialogText(DialogTextReply {
                id: dialog.id,
                revision: dialog.revision,
                serial,
                expected: before.into(),
                item: field.number,
                text: after.into(),
                selection: (0, 0),
            }));
            assert!(
                driver.runner.as_ref().unwrap().host_typed_keys_pending(),
                "delivered as keystrokes"
            );
            for _ in 0..60 {
                super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
                driver.capture_state(&mut state, false);
                if state
                    .dialog_text_replies
                    .iter()
                    .any(|ack| ack.serial == serial)
                {
                    break;
                }
            }
            let ack = state
                .dialog_text_replies
                .iter()
                .find(|ack| ack.serial == serial)
                .expect("acknowledged");
            assert_eq!(ack.text.as_deref(), Some(after));
        }
        let typed = state.dialog.clone().unwrap();
        assert_eq!(
            typed
                .items
                .iter()
                .find(|i| i.number == field.number)
                .unwrap()
                .text,
            "Rnée"
        );
        // A click anywhere reaches the guest as a mouse event at that point.
        refilter(&mut driver, &mut state);
        let typed = state.dialog.clone().unwrap();
        let label = typed.items.iter().find(|i| i.kind == 8).unwrap();
        let (top, left, ..) = label.guest_rect.unwrap();
        assert_eq!(
            driver.runner.as_mut().unwrap().host_dialog_point_position(
                &systemless::systems::macintosh::host_ui::DialogReply {
                    id: typed.id,
                    revision: typed.revision,
                    item: label.number,
                },
                (2, 3),
            ),
            Some((top + 2, left + 3))
        );
    }

    #[test]
    #[ignore = "runs the source-built Macintosh application on both CPU backends"]
    fn workspace_showcase_popup_control_round_trip() {
        use systemless::systems::macintosh::host_ui::{PopupMenuReply, UiOwner};
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        for powerpc in [false, true] {
            let mut driver = driver();
            driver.workspace = true;
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(powerpc);
            runner.set_host_ui_enabled(true);
            runner.set_app_start_time(3_000_000_000);
            let app = game::load_game_from_path(&mut runner, &archive).unwrap();
            game::init_game(&mut runner, &app);
            configure_realtime_execution_rate(&mut runner);
            for _ in 0..60 {
                super::super::headless_time::frame(&mut runner, 0);
                if runner
                    .guest_menu_snapshot()
                    .menus
                    .iter()
                    .any(|m| m.id == 129)
                {
                    break;
                }
            }
            assert!(
                runner.select_guest_menu_item(129, 16),
                "Popup & Dropdown page"
            );
            for _ in 0..30 {
                super::super::headless_time::frame(&mut runner, 0);
            }
            driver.runner = Some(runner);
            let mut state = GuiState::default();
            // Main window content starts at (50, 40); the control is local
            // (100, 190, 124, 400) with a 60-pixel title.
            let open = |driver: &mut GuiDriver, state: &mut GuiState| {
                let runner = driver.runner.as_mut().unwrap();
                let (v, h) = runner.presented_mouse_position(50 + 112, 40 + 320);
                driver.apply_command(GuiCommand::MouseDown { v, h });
                for _ in 0..60 {
                    super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
                    driver.capture_state(state, false);
                    if state.popup_menu.is_some() {
                        break;
                    }
                }
                driver.apply_command(GuiCommand::MouseUp { v, h });
                for _ in 0..10 {
                    super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
                }
                driver.capture_state(state, false);
                state
                    .popup_menu
                    .clone()
                    .expect("the popup stays open after release")
            };
            let popup = open(&mut driver, &mut state);
            assert_eq!(popup.menu, 143, "powerpc={powerpc}");
            assert_eq!(popup.item, 1);
            assert_eq!(state.modality.owner, UiOwner::PopupMenu);
            assert!(state.modality.menus_blocked);
            assert!(!driver
                .runner
                .as_mut()
                .unwrap()
                .select_guest_menu_item(129, 1));
            // A separator cannot be chosen; a real item becomes the value.
            driver.apply_command(GuiCommand::PopupMenu(PopupMenuReply {
                id: popup.id,
                choice: Some((143, 3)),
            }));
            driver.capture_state(&mut state, false);
            assert!(state.popup_menu.is_some());
            driver.apply_command(GuiCommand::PopupMenu(PopupMenuReply {
                id: popup.id,
                choice: Some((143, 4)),
            }));
            for _ in 0..30 {
                super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
            }
            driver.capture_state(&mut state, false);
            assert!(state.popup_menu.is_none());
            assert_eq!(state.modality.owner, UiOwner::Guest);
            let popup = open(&mut driver, &mut state);
            assert_eq!(popup.item, 4, "the control kept the host choice");
            driver.apply_command(GuiCommand::PopupMenu(PopupMenuReply {
                id: popup.id,
                choice: None,
            }));
            for _ in 0..30 {
                super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
            }
            driver.capture_state(&mut state, false);
            assert!(state.popup_menu.is_none());
            assert!(!driver.runner.as_ref().unwrap().is_halted());
        }
    }

    #[test]
    #[ignore = "runs the source-built Macintosh application on both CPU backends"]
    fn workspace_showcase_modeless_round_trip() {
        use super::super::runtime_protocol::ModelessCommand;
        use systemless::systems::macintosh::host_ui::{DialogReply, DialogTextReply};
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        for powerpc in [false, true] {
            let mut driver = driver();
            driver.workspace = true;
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(powerpc);
            runner.set_host_ui_enabled(true);
            runner.set_app_start_time(3_000_000_000);
            let app = game::load_game_from_path(&mut runner, &archive).unwrap();
            game::init_game(&mut runner, &app);
            assert_eq!(runner.is_powerpc_app(), powerpc);
            configure_realtime_execution_rate(&mut runner);
            for _ in 0..60 {
                super::super::headless_time::frame(&mut runner, 0);
                if runner
                    .guest_menu_snapshot()
                    .menus
                    .iter()
                    .any(|m| m.id == 129)
                {
                    break;
                }
            }
            assert!(
                runner.select_guest_menu_item(129, 6),
                "Dialogs & Alerts page"
            );
            for _ in 0..30 {
                super::super::headless_time::frame(&mut runner, 0);
            }
            runner.push_key_down(3, b'f');
            runner.push_key_up(3, b'f');
            for _ in 0..120 {
                super::super::headless_time::frame(&mut runner, 0);
                if !runner.host_modeless_dialogs().is_empty() {
                    break;
                }
            }
            driver.runner = Some(runner);
            let mut state = GuiState::default();
            // Runs until `done` accepts the published front-most Find view.
            let settle =
                |driver: &mut GuiDriver, state: &mut GuiState, done: &dyn Fn(&GuiState) -> bool| {
                    for _ in 0..180 {
                        driver.capture_state(state, false);
                        if done(state) {
                            return;
                        }
                        super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
                    }
                    driver.capture_state(state, false);
                    assert!(
                        done(state),
                        "powerpc={powerpc}: {:?}",
                        state.modeless_dialogs
                    );
                };
            settle(&mut driver, &mut state, &|s| {
                s.modeless_dialogs.first().is_some_and(|d| !d.busy)
            });
            let find = state.modeless_dialogs[0].clone();
            assert_eq!(find.title, "Find");
            assert!(find.closable && find.movable);
            assert!(state.dialog.is_none());
            assert_eq!(
                state.modality.owner,
                systemless::systems::macintosh::host_ui::UiOwner::Guest
            );
            assert!(
                !state.modality.menus_blocked,
                "modeless dialogs do not block menus"
            );

            // Text commits in place and is acknowledged canonically.
            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Text(
                DialogTextReply {
                    id: find.id,
                    revision: find.revision,
                    serial: 1,
                    expected: String::new(),
                    item: 4,
                    text: "needle".into(),
                    selection: (6, 6),
                },
            )));
            driver.capture_state(&mut state, false);
            let ack = state
                .dialog_text_replies
                .iter()
                .find(|ack| ack.id == find.id)
                .unwrap();
            assert!(ack.accepted, "powerpc={powerpc}");
            assert_eq!(ack.text.as_deref(), Some("needle"));
            assert_eq!(
                state.modeless_dialogs[0]
                    .items
                    .iter()
                    .find(|i| i.number == 4)
                    .unwrap()
                    .text,
                "needle"
            );
            // The application observes the committed edit through DialogSelect.
            settle(&mut driver, &mut state, &|s| {
                s.modeless_dialogs
                    .first()
                    .is_some_and(|d| d.items.iter().any(|i| i.number == 6 && i.text == "Editing"))
            });
            let view = state.modeless_dialogs[0].clone();
            assert_eq!(
                view.items.iter().find(|i| i.number == 4).unwrap().text,
                "needle"
            );

            // Find arrives through the application's DialogSelect exactly once.
            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Item(DialogReply {
                id: view.id,
                revision: view.revision,
                item: 1,
            })));
            settle(&mut driver, &mut state, &|s| {
                s.modeless_dialogs.first().is_some_and(|d| {
                    d.items
                        .iter()
                        .any(|i| i.number == 6 && i.text == "Found needle")
                })
            });
            // The checkbox toggles once in application code, not twice.
            let view = state.modeless_dialogs[0].clone();
            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Item(DialogReply {
                id: view.id,
                revision: view.revision,
                item: 5,
            })));
            settle(&mut driver, &mut state, &|s| {
                s.modeless_dialogs
                    .first()
                    .is_some_and(|d| d.items.iter().any(|i| i.number == 5 && i.value == 1))
            });
            for _ in 0..20 {
                super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
            }
            driver.capture_state(&mut state, false);
            let view = state.modeless_dialogs[0].clone();
            assert_eq!(view.items.iter().find(|i| i.number == 5).unwrap().value, 1);
            // A stale revision is rejected without reaching the guest.
            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Close {
                id: view.id,
                revision: view.revision + 100,
            }));
            for _ in 0..20 {
                super::super::headless_time::frame(driver.runner.as_mut().unwrap(), 0);
            }
            driver.capture_state(&mut state, false);
            assert_eq!(state.modeless_dialogs.len(), 1);
            // Closing is the application's TrackGoAway decision: it hides the
            // palette, retaining its identity for a later ShowWindow.
            let view = state.modeless_dialogs[0].clone();
            driver.apply_command(GuiCommand::Modeless(ModelessCommand::Close {
                id: view.id,
                revision: view.revision,
            }));
            settle(&mut driver, &mut state, &|s| s.modeless_dialogs.is_empty());
            assert!(state.live_dialogs.contains(&view.id));
            assert!(!driver.runner.as_ref().unwrap().is_halted());
        }
    }

    #[test]
    #[ignore = "runs the source-built Macintosh application on both CPU backends"]
    fn workspace_showcase_text_round_trip() {
        use systemless::systems::macintosh::host_ui::{DialogReply, DialogTextReply};
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        for powerpc in [false, true] {
            let mut driver = driver();
            driver.workspace = true;
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(powerpc);
            runner.set_host_ui_enabled(true);
            runner.set_app_start_time(3_000_000_000);
            let app = game::load_game_from_path(&mut runner, &archive).unwrap();
            game::init_game(&mut runner, &app);
            assert_eq!(runner.is_powerpc_app(), powerpc);
            configure_realtime_execution_rate(&mut runner);
            for _ in 0..60 {
                super::super::headless_time::frame(&mut runner, 0);
                if runner
                    .guest_menu_snapshot()
                    .menus
                    .iter()
                    .any(|m| m.id == 132)
                {
                    break;
                }
            }
            assert!(runner.select_guest_menu_item(132, 6));
            for _ in 0..120 {
                super::super::headless_time::frame(&mut runner, 0);
                if runner.host_dialog().is_some() {
                    break;
                }
            }
            driver.runner = Some(runner);
            let mut state = GuiState::default();
            driver.capture_state(&mut state, false);
            let source = state
                .dialog
                .clone()
                .expect("application preferences use host UI");
            assert_eq!(source.title, "Game Preferences");
            assert!(state.modality.menus_blocked);
            assert!(
                !driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .select_guest_menu_item(132, 6),
                "an enabled application menu cannot queue behind a modal"
            );
            if !powerpc {
                // An application filter keeps the dialog in egui; the filter
                // still sees every event in ModalDialog's loop.
                driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .dispatcher_mut()
                    .dialog_tracking
                    .as_mut()
                    .unwrap()
                    .filter_proc = 0x10000;
                driver.capture_state(&mut state, false);
                assert!(state
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.guest_filter));
                driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .dispatcher_mut()
                    .dialog_tracking
                    .as_mut()
                    .unwrap()
                    .filter_proc = 0;
                // A window definition Systemless cannot frame natively keeps
                // the guest's drawing, lifted into a host modal surface.
                let proc_id = driver
                    .runner
                    .as_ref()
                    .unwrap()
                    .dispatcher()
                    .dialog_tracking
                    .as_ref()
                    .unwrap()
                    .proc_id;
                driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .dispatcher_mut()
                    .dialog_tracking
                    .as_mut()
                    .unwrap()
                    .proc_id = 16;
                driver.capture_state(&mut state, false);
                assert!(state.dialog.is_none());
                // The guest-rendered fallback is framed by a host modal surface.
                let surface = state.guest_surface.clone().expect("guest dialog is lifted");
                assert_eq!(
                    (surface.id, surface.title.as_str()),
                    (source.id, "Game Preferences")
                );
                assert_eq!(
                    (
                        surface.bounds.2 - surface.bounds.0,
                        surface.bounds.3 - surface.bounds.1
                    ),
                    (source.size.1 as i16, source.size.0 as i16)
                );
                assert!(state.live_dialogs.contains(&source.id));
                assert_eq!(
                    state.modality.owner,
                    systemless::systems::macintosh::host_ui::UiOwner::Guest
                );
                assert!(state.modality.menus_blocked);
                assert!(!driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .select_guest_menu_item(132, 6));
                driver
                    .runner
                    .as_mut()
                    .unwrap()
                    .dispatcher_mut()
                    .dialog_tracking
                    .as_mut()
                    .unwrap()
                    .proc_id = proc_id;
                driver.capture_state(&mut state, false);
                assert!(
                    state.guest_surface.is_none(),
                    "host projection replaces the surface"
                );
            }
            // Modal checkboxes must retain the application's toggle after
            // the synthesized click and its deferred mouse-up have settled.
            for expected in [1, 0] {
                let view = state.dialog.clone().unwrap();
                driver.apply_command(GuiCommand::Dialog(DialogReply {
                    id: view.id,
                    revision: view.revision,
                    item: 4,
                }));
                for _ in 0..120 {
                    let runner = driver.runner.as_mut().unwrap();
                    let work = super::super::headless_time::frame(runner, 0);
                    if work.foreground > 0 {
                        driver.mouse_release_latch.observe_guest_progress();
                    }
                    if let Some((v, h)) = driver.mouse_release_latch.take_ready_release() {
                        runner.push_mouse_up(v, h);
                    }
                }
                driver.capture_state(&mut state, false);
                let view = state.dialog.as_ref().expect("checkbox keeps dialog open");
                assert_eq!(
                    view.items
                        .iter()
                        .find(|item| item.number == 4)
                        .unwrap()
                        .value,
                    expected,
                    "modal checkbox toggles once, powerpc={powerpc}"
                );
            }
            let source = state.dialog.clone().unwrap();
            let field = source
                .items
                .iter()
                .find(|i| i.kind == 16 && i.enabled)
                .unwrap();
            let text = DialogTextReply {
                id: source.id,
                revision: source.revision,
                serial: 1,
                expected: field.text.clone(),
                item: field.number,
                text: "Renée".into(),
                selection: (5, 5),
            };
            driver.apply_command(GuiCommand::DialogText(text));
            driver.capture_state(&mut state, false);
            assert_eq!(
                state
                    .dialog_text_replies
                    .first()
                    .map(|reply| reply.as_ref()),
                Some(&DialogTextAck {
                    serial: 1,
                    id: source.id,
                    accepted: true,
                    text: Some("Renée".into()),
                })
            );
            assert!(state.live_dialogs.contains(&source.id));
            let updated = state.dialog.clone().unwrap();
            assert_eq!(
                updated
                    .items
                    .iter()
                    .find(|i| i.number == field.number)
                    .unwrap()
                    .text,
                "Renée"
            );
            driver.apply_command(GuiCommand::Dialog(DialogReply {
                id: updated.id,
                revision: updated.revision,
                item: updated.default_item,
            }));
            for _ in 0..120 {
                let runner = driver.runner.as_mut().unwrap();
                let work = super::super::headless_time::frame(runner, 0);
                if work.foreground > 0 {
                    driver.mouse_release_latch.observe_guest_progress();
                }
                if let Some((v, h)) = driver.mouse_release_latch.take_ready_release() {
                    runner.push_mouse_up(v, h);
                }
                if !runner.host_dialog_ids().contains(&updated.id) {
                    break;
                }
            }
            driver.capture_state(&mut state, false);
            assert!(
                state.dialog.is_none(),
                "guest consumes Save and retires the host view"
            );
            assert!(
                !state.live_dialogs.contains(&updated.id),
                "disposed guest dialog retires its editor session"
            );
            assert!(!driver.runner.as_ref().unwrap().is_halted());
        }
    }

    /// A retired compact snapshot no one else holds takes the next frame in
    /// place; one still held by a presented frame is left alone.
    #[test]
    fn compact_snapshots_reuse_only_released_spares() {
        use std::sync::Arc;
        use systemless::systems::macintosh::memory::CompactPresentation;
        let mut driver = driver();
        let stale = || CompactPresentation {
            width: 9,
            height: 9,
            scale: 2,
            cells: vec![7; 81],
            detail: vec![5; 16],
        };
        *driver.compact_cache.frame_mut() = CompactPresentation {
            width: 4,
            height: 4,
            scale: 4,
            cells: (0..16).collect(),
            detail: vec![0x0012_3456; 16],
        };
        let expected = driver.compact_cache.frame().clone();

        let held = Arc::new(stale());
        let presented = Arc::clone(&held);
        driver.compact_spares.push(held);
        let fresh = GuiDriver::next_compact_snapshot(
            driver.compact_cache.frame(),
            &mut driver.compact_spares,
        );
        assert!(
            !Arc::ptr_eq(&fresh, &presented),
            "a held spare is not reused"
        );
        assert_eq!(*presented, stale(), "and is not overwritten");
        assert_eq!(*fresh, expected);
        assert_eq!(driver.compact_spares.len(), 1);

        drop(presented);
        let released = Arc::as_ptr(&driver.compact_spares[0]);
        let reused = GuiDriver::next_compact_snapshot(
            driver.compact_cache.frame(),
            &mut driver.compact_spares,
        );
        assert_eq!(Arc::as_ptr(&reused), released, "a released spare is reused");
        assert_eq!(*reused, expected, "holding exactly the new frame");
        assert!(driver.compact_spares.is_empty());
    }

    #[test]
    fn complete_frames_survive_frozen_ticks_guest_changes_and_owner_destruction() {
        let mut driver = driver();
        let mut first = GuiFrame::default();
        assert!(driver.capture_frame(&mut first, None, false, false));
        let original_bytes = first.screen.pixels.clone();
        let original_palette = first.screen.palette;
        let runner = driver.runner.as_mut().unwrap();
        let address = runner.dispatcher().screen_mode.0;
        runner.bus_mut().fill_bytes(address, 32 * 32, 2);
        runner.dispatcher_mut().set_mouse_position(19, 23);
        let mut second = GuiFrame::default();
        assert!(driver.capture_frame(&mut second, None, false, false));
        assert_eq!(first.guest_tick, second.guest_tick);
        assert_eq!(first.generation, second.generation);
        assert_eq!(second.sequence, first.sequence + 1);
        assert_eq!(first.display_generation, second.display_generation);
        assert_eq!(second.mouse_position, (19, 23));
        assert_eq!(first.screen.palette, second.screen.palette);
        assert_ne!(first.screen.pixels, second.screen.pixels);
        driver
            .runner
            .as_mut()
            .unwrap()
            .dispatcher_mut()
            .screen_mode
            .2 = 16;
        let mut third = GuiFrame::default();
        assert!(driver.capture_frame(&mut third, None, false, false));
        assert_eq!(third.display_generation, second.display_generation + 1);
        assert_eq!(third.sequence, second.sequence + 1);
        drop(driver);
        assert_eq!(first.screen.pixels, original_bytes);
        assert_eq!(first.screen.palette, original_palette);
        let mut pixels = Vec::new();
        first.screen.render_argb(&mut pixels);
        assert_eq!(pixels.len(), 32 * 32);
    }

    #[test]
    fn runtime_generations_do_not_reuse_frame_identifiers() {
        let mut first = driver();
        let mut second = driver();
        let mut a = GuiFrame::default();
        let mut b = GuiFrame::default();
        assert!(first.capture_frame(&mut a, None, false, false));
        assert!(second.capture_frame(&mut b, None, false, false));
        assert_eq!(a.sequence, b.sequence);
        assert_ne!(a.generation, b.generation);
    }
    fn guest_warp(driver: &mut GuiDriver, position: (i16, i16)) {
        use systemless::systems::macintosh::cpu::Register;
        use systemless::systems::macintosh::memory::globals::addr;
        let runner = driver.runner.as_mut().unwrap();
        runner.bus_mut().write_word(0x10000, 0x21fc); // MOVE.L #point,Mouse.W
        runner.bus_mut().write_long(
            0x10002,
            (u32::from(position.0 as u16) << 16) | u32::from(position.1 as u16),
        );
        runner
            .bus_mut()
            .write_word(0x10006, addr::MOUSE_LOC2 as u16);
        runner.cpu_mut().write_reg(Register::PC, 0x10000);
        runner.run_steps(1, None);
    }

    #[test]
    fn cursor_warps_survive_replaced_snapshots_until_matching_acknowledgement() {
        let mut driver = driver();
        guest_warp(&mut driver, (11, 13));
        let mut state = GuiState::default();
        driver.capture_state(&mut state, false);
        let first = state.warp.expect("guest instruction should request a warp");
        assert_eq!(first.position, (11, 13));
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(first));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation + 1,
            serial: first.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(first));
        guest_warp(&mut driver, (17, 19));
        driver.capture_state(&mut state, false);
        let second = state.warp.unwrap();
        assert!(second.serial > first.serial);
        assert_eq!(second.position, (17, 19));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation,
            serial: first.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, Some(second));
        driver.apply_command(GuiCommand::AcknowledgeWarp {
            generation: state.generation,
            serial: second.serial,
        });
        driver.capture_state(&mut state, false);
        assert_eq!(state.warp, None);
    }

    #[test]
    fn owned_mouse_commands_preserve_delayed_release_coordinates() {
        let mut driver = driver();
        driver.apply_command(GuiCommand::MouseDown { v: 3, h: 5 });
        driver.apply_command(GuiCommand::MouseUp { v: 7, h: 11 });
        driver.apply_command(GuiCommand::MouseMove { v: 13, h: 17 });
        assert_eq!(driver.mouse_release_latch.pending_release, Some((7, 11)));
        driver.mouse_release_latch.observe_guest_progress();
        driver.flush_ready_mouse_release();
        assert_eq!(driver.mouse_release_latch.pending_release, None);
        assert_eq!(
            driver
                .runner
                .as_ref()
                .unwrap()
                .dispatcher()
                .mouse_position(),
            (7, 11)
        );
    }
}
