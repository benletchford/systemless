//! Host-owned egui workspace. Reads presentation snapshots; never accesses the runner.
#[path = "dialog_editors.rs"]
mod dialog_editors;
use systemless::systems::macintosh::host_ui::{
    FileDialogAction, FileDialogReply, FileDialogSnapshot, UiModality, UiOwner,
};
use systemless_ui::{
    egui,
    egui_dock::{DockState, NodeIndex, TabViewer},
    egui_wgpu, Appearance, Theme,
};
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    window::Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
enum Pane {
    Guest,
    Inspector,
}
fn initial_layout() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Guest]);
    dock.main_surface_mut()
        .split_right(NodeIndex::root(), 0.75, vec![Pane::Inspector]);
    dock
}
#[derive(Default)]
pub struct Inspection {
    pub sequence: u64,
    pub tick: u32,
    pub screen: (u32, u32),
    pub retained: bool,
}
struct ArtworkTexture {
    source: std::sync::Arc<systemless::systems::macintosh::host_ui::DialogArtwork>,
    texture: egui::TextureHandle,
}

/// Screen rectangle, guest content bounds and egui layer of a lifted dialog.
type LiftedSurface = (egui::Rect, (i16, i16, i16, i16), egui::LayerId);

pub struct WorkspaceUi {
    pub context: egui::Context,
    platform: Option<egui_winit::State>,
    accessibility_active: bool,
    renderer: egui_wgpu::Renderer,
    dock: DockState<Pane>,
    pane_layers: std::collections::HashMap<Pane, egui::LayerId>,
    theme: Theme,
    texture: Option<(wgpu::Texture, egui::TextureId)>,
    pub guest_rect: Option<egui::Rect>,
    guest_hovered: bool,
    guest_layer: Option<egui::LayerId>,
    guest_focused: bool,
    guest_drag: bool,
    ui_pointer_buttons: std::collections::HashSet<MouseButton>,
    keys: std::collections::HashSet<winit::keyboard::PhysicalKey>,
    pointer: egui::Pos2,
    pub crop: (u32, u32, u32, u32),
    pub inspection: Inspection,
    modality: UiModality,
    pub menus: Option<std::sync::Arc<systemless::systems::macintosh::menu_model::GuestMenuSnapshot>>,
    pub menu_actions: Vec<(i16, i16)>,
    dialog: Option<std::sync::Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>>,
    pub dialog_actions: Vec<systemless::systems::macintosh::host_ui::DialogReply>,
    dialog_editors: dialog_editors::DialogEditorSessions,
    dialog_surfaces: std::collections::HashMap<u64, systemless_ui::dialog::ModalState>,
    modeless: Vec<std::sync::Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>>,
    modeless_sessions: std::collections::HashMap<u64, ModelessSession>,
    pub modeless_actions: Vec<super::runtime_protocol::ModelessCommand>,
    popup_menu: Option<systemless::systems::macintosh::host_ui::PopupMenuSnapshot>,
    pub popup_action: Option<systemless::systems::macintosh::host_ui::PopupMenuReply>,
    system_dialog: Option<systemless::systems::macintosh::host_ui::SystemDialogSnapshot>,
    color_draft: egui::Color32,
    pub system_action: Option<systemless::systems::macintosh::host_ui::SystemDialogReply>,
    notifications: Vec<systemless::systems::macintosh::host_ui::NotificationSnapshot>,
    balloon: Option<systemless::systems::macintosh::host_ui::BalloonSnapshot>,
    balloon_texture: Option<ArtworkTexture>,
    pub menu_help: Option<std::sync::Arc<std::collections::HashMap<(i16, i16), String>>>,
    balloon_help: bool,
    pub balloon_help_action: Option<bool>,
    pub dismissed_notifications: Vec<u64>,
    hidden_notifications: std::collections::HashSet<u64>,
    stopped: Option<String>,
    guest_surface: Option<systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot>,
    lifted: Option<LiftedSurface>,
    /// Guest-drawn modeless dialogs framed in egui windows, with retained geometry.
    guest_windows: Vec<systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot>,
    guest_window_states: std::collections::HashMap<u64, systemless_ui::window::WindowState>,
    lifted_windows: Vec<LiftedSurface>,
    /// Identities already answered; hidden until the runtime retires them.
    answered: std::collections::HashSet<u64>,
    artwork_textures: std::collections::HashMap<i16, ArtworkTexture>,
    pub dialog_control_actions: Vec<systemless::systems::macintosh::host_ui::DialogControlReply>,
    pending_controls: std::collections::HashMap<u64, (i16, i16, i16)>,
    pub dialog_text_actions: Vec<systemless::systems::macintosh::host_ui::DialogTextReply>,
    file_dialog: Option<std::sync::Arc<FileDialogSnapshot>>,
    file_picker: systemless_ui::file_picker::FilePicker,
    file_pending: bool,
    file_reply: Option<(u64, u64, u64, bool)>,
    pub file_actions: Vec<FileDialogReply>,
    preferences: bool,
    about: bool,
    error: Option<String>,
    storage: Option<std::path::PathBuf>,
    pub repaint_at: std::time::Instant,
}
impl WorkspaceUi {
    pub fn set_modality(&mut self, modality: UiModality) {
        if self.modality != modality {
            self.modality = modality;
            self.context.memory_mut(|memory| {
                if let Some(id) = memory.focused() {
                    memory.surrender_focus(id);
                }
            });
            self.guest_focused = modality.owner == UiOwner::Guest;
            if modality.owner == UiOwner::Dialog {
                self.dialog_editors.active.restore_focus();
            }
            self.context.request_repaint();
        }
    }
    fn has_host_modal(&self) -> bool {
        self.modality.owner != UiOwner::Guest
    }

    pub fn set_dialog(
        &mut self,
        dialog: Option<std::sync::Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>>,
        live_dialogs: &[u64],
    ) {
        let dialog = dialog.filter(|d| host_renderable(d));
        self.pending_controls.retain(|id, _| live_dialogs.contains(id));
        self.dialog_surfaces.retain(|id, _| live_dialogs.contains(id));
        self.dialog_editors.sync(dialog.as_deref(), live_dialogs);
        if self.dialog != dialog {
            if self
                .dialog
                .as_deref()
                .zip(dialog.as_deref())
                .is_some_and(|(old, new)| old.id == new.id && old.busy && !new.busy)
            {
                self.dialog_editors.active.restore_focus();
            }
            self.dialog = dialog;
            self.sync_artwork_textures();
            self.context.request_repaint();
        }
    }

    fn sync_artwork_textures(&mut self) {
        sync_artwork(
            &self.context,
            &mut self.artwork_textures,
            self.dialog.as_deref(),
        );
    }

    /// Publish modeless windows in guest front-to-back order. Sessions follow
    /// live guest identities, including hidden or temporarily unprojected windows.
    pub fn set_modeless_dialogs(
        &mut self,
        dialogs: &[std::sync::Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>],
        live_dialogs: &[u64],
    ) {
        let dialogs: Vec<_> = dialogs
            .iter()
            .filter(|d| host_renderable(d))
            .cloned()
            .collect();
        if dialogs == self.modeless {
            return;
        }
        self.modeless_sessions
            .retain(|id, _| live_dialogs.contains(id));
        let old_front = self.modeless.first().map(|d| d.id);
        for dialog in &dialogs {
            let old = self.modeless.iter().find(|old| old.id == dialog.id);
            let session = self.modeless_sessions.entry(dialog.id).or_default();
            session.editors.sync(Some(dialog));
            if old.is_none() || old.is_some_and(|old| old.busy && !dialog.busy) {
                session.editors.restore_focus();
            }
            sync_artwork(&self.context, &mut session.artwork, Some(dialog));
            if session.pending_control.is_some_and(|(item, expected, _)| {
                dialog
                    .items
                    .iter()
                    .all(|i| i.number != item || i.value != expected)
            }) {
                session.pending_control = None;
            }
        }
        // Guest activation raises its window; host stacking alone never activates.
        if let Some(front) = dialogs.first().filter(|d| Some(d.id) != old_front) {
            if let Some(session) = self.modeless_sessions.get_mut(&front.id) {
                session.window.request_front();
            }
        }
        for (id, session) in &mut self.modeless_sessions {
            if !dialogs.iter().any(|d| d.id == *id) {
                session.artwork.clear();
            }
        }
        self.modeless = dialogs;
        self.context.request_repaint();
    }

    fn show_modeless(&mut self, context: &egui::Context) {
        use super::runtime_protocol::ModelessCommand;
        let blocked =
            self.has_host_modal() || self.preferences || self.about || self.error.is_some();
        let center = context.content_rect().center();
        // Paint back to front so the guest's front window starts on top.
        for (index, dialog) in self.modeless.clone().iter().enumerate().rev() {
            let Some(session) = self.modeless_sessions.get_mut(&dialog.id) else {
                continue;
            };
            let size = egui::vec2(
                (dialog.size.0 as f32 + 48.).clamp(360., 640.),
                (dialog.size.1 as f32 + 180.).clamp(200., 600.),
            );
            let offset = egui::vec2(28., 28.) * index as f32;
            let response = systemless_ui::window::show(
                context,
                egui::Id::new(("macintosh-modeless", dialog.id)),
                if dialog.title.is_empty() {
                    "Dialog"
                } else {
                    &dialog.title
                },
                &mut session.window,
                systemless_ui::window::WindowOptions {
                    initial_rect: egui::Rect::from_center_size(center + offset, size),
                    enabled: !blocked,
                    resizable: true,
                    closable: dialog.closable,
                },
                |ui| {
                    dialog_form(
                        ui,
                        &self.theme,
                        dialog,
                        &session.artwork,
                        &mut session.editors,
                        index == 0 && !self.guest_focused,
                        // The window title bar already names the dialog.
                        false,
                    )
                },
            );
            if blocked || dialog.busy {
                continue;
            }
            if response.activated {
                self.modeless_actions.push(ModelessCommand::Activate {
                    id: dialog.id,
                    revision: dialog.revision,
                });
            }
            if response.close_requested {
                session.close = true;
            }
            if let Some(form) = response.inner {
                for (id, _) in form.edits {
                    session.editors.edited(id);
                }
                for (item, value) in form.values {
                    if let Some(source) = dialog.items.iter().find(|i| i.number == item) {
                        session.pending_control = Some((item, source.value, value));
                        session.editors.focus_control(item);
                        session.editors.action = Some(item);
                    }
                }
                if let Some(item) = form.action {
                    session.pending_control = None;
                    session.editors.action = Some(item);
                }
            }
            let (text, action) = session.editors.flush(dialog);
            if let Some(text) = text {
                self.modeless_actions.push(ModelessCommand::Text(text));
            } else if let Some(action) = action {
                match session
                    .pending_control
                    .take()
                    .filter(|(item, _, _)| *item == action.item)
                {
                    Some((_, expected, value)) => {
                        self.modeless_actions.push(ModelessCommand::Control(
                            systemless::systems::macintosh::host_ui::DialogControlReply {
                                dialog: action,
                                expected,
                                value,
                            },
                        ))
                    }
                    None => self.modeless_actions.push(ModelessCommand::Item(action)),
                }
            } else if session.close && session.editors.synchronized() {
                // Text commits first, so the application reads final values.
                session.close = false;
                self.modeless_actions.push(ModelessCommand::Close {
                    id: dialog.id,
                    revision: dialog.revision,
                });
            }
        }
    }

    pub fn set_dialog_text_reply(
        &mut self,
        reply: Option<&super::runtime_protocol::DialogTextAck>,
    ) {
        if let Some(reply) = reply {
            if let Some(session) = self.modeless_sessions.get_mut(&reply.id) {
                let snapshot = self.modeless.iter().find(|d| d.id == reply.id);
                if session
                    .editors
                    .acknowledge(Some(reply), snapshot.map(|d| &**d))
                {
                    self.context.request_repaint();
                }
                return;
            }
        }
        if self
            .dialog_editors
            .acknowledge(reply, self.dialog.as_deref())
        {
            self.context.request_repaint();
        }
    }

    pub fn set_file_reply(&mut self, reply: Option<(u64, u64, u64, bool)>) {
        if self.file_reply == reply {
            return;
        }
        self.file_reply = reply;
        if let Some((_, id, revision, false)) = reply {
            if self
                .file_dialog
                .as_ref()
                .is_some_and(|d| (d.id, d.revision) == (id, revision))
            {
                self.file_pending = false;
                self.error = Some(
                    "The guest could not accept that file selection. Please try again.".into(),
                );
                self.context.request_repaint();
            }
        }
    }

    pub fn set_popup_menu(
        &mut self,
        popup: Option<systemless::systems::macintosh::host_ui::PopupMenuSnapshot>,
    ) {
        if self.popup_menu != popup {
            self.retain_answered(
                popup.as_ref().map(|p| p.id),
                self.system_dialog.as_ref().map(|d| d.id),
            );
            self.popup_menu = popup;
            self.context.request_repaint();
        }
    }

    pub fn set_system_dialog(
        &mut self,
        dialog: Option<systemless::systems::macintosh::host_ui::SystemDialogSnapshot>,
    ) {
        use systemless::systems::macintosh::host_ui::SystemDialogKind;
        if self.system_dialog == dialog {
            return;
        }
        self.retain_answered(
            self.popup_menu.as_ref().map(|p| p.id),
            dialog.as_ref().map(|d| d.id),
        );
        if let Some(SystemDialogKind::Color { color, .. }) = dialog.as_ref().map(|d| &d.kind) {
            let [r, g, b] = color.map(|component| (component >> 8) as u8);
            self.color_draft = egui::Color32::from_rgb(r, g, b);
        }
        self.system_dialog = dialog;
        self.context.request_repaint();
    }

    /// Color Picker and Printing Manager dialogs. Each answers once.
    fn show_system_dialog(&mut self, context: &egui::Context) {
        use systemless::systems::macintosh::host_ui::{
            SystemDialogAnswer, SystemDialogKind, SystemDialogReply,
        };
        use systemless_ui::components::{button, ButtonStyle};
        let Some(dialog) = self.system_dialog.clone() else {
            return;
        };
        if self.answered.contains(&dialog.id) {
            return;
        }
        let mut answer = None;
        let response = egui::Modal::new(egui::Id::new(("system-dialog", dialog.id))).show(context, |ui| {
            ui.set_min_width(340.);
            let confirm = match &dialog.kind {
                SystemDialogKind::Color { prompt, .. } => {
                    ui.heading(if prompt.is_empty() { "Choose a color" } else { prompt });
                    ui.add_space(8.);
                    // The picker sizes its gradients from the slider width.
                    ui.spacing_mut().slider_width = 220.;
                    egui::widgets::color_picker::color_picker_color32(
                        ui,
                        &mut self.color_draft,
                        egui::widgets::color_picker::Alpha::Opaque,
                    );
                    "Choose"
                }
                SystemDialogKind::PageSetup => {
                    ui.heading("Page Setup");
                    ui.add_space(8.);
                    ui.label("US Letter, portrait, 100%.");
                    ui.weak("Systemless has no printer. The application keeps its own page settings.");
                    "OK"
                }
                SystemDialogKind::ProgramLinking { prompt } => {
                    ui.heading(if prompt.is_empty() { "Program Linking" } else { prompt });
                    ui.add_space(8.);
                    ui.label("No other computers or programs are available.");
                    ui.weak("Systemless does not provide AppleTalk program linking.");
                    "OK"
                }
                SystemDialogKind::BadDisk => {
                    ui.heading("This disk is unreadable");
                    ui.add_space(8.);
                    ui.label("Systemless uses virtual volumes and cannot initialize disks.");
                    "Eject"
                }
                SystemDialogKind::Print => {
                    ui.heading("Print");
                    ui.add_space(8.);
                    ui.label("Systemless has no printer.");
                    ui.weak("Continuing lets the application finish printing; nothing is sent to a device.");
                    "Continue"
                }
            };
            ui.add_space(12.);
            ui.horizontal(|ui| {
                if button(ui, &self.theme, confirm, ButtonStyle::Primary).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    answer = Some(match dialog.kind {
                        SystemDialogKind::Color { .. } => {
                            let [r, g, b, _] = self.color_draft.to_array();
                            SystemDialogAnswer::Color([r, g, b].map(|c| u16::from(c) * 0x101))
                        }
                        // Nothing to choose without a network: dismissing cancels.
                        SystemDialogKind::ProgramLinking { .. } => SystemDialogAnswer::Cancel,
                        _ => SystemDialogAnswer::Confirm,
                    });
                }
                let informational = matches!(
                    dialog.kind,
                    SystemDialogKind::ProgramLinking { .. } | SystemDialogKind::BadDisk
                );
                if !informational && button(ui, &self.theme, "Cancel", ButtonStyle::Secondary).clicked() {
                    answer = Some(SystemDialogAnswer::Cancel);
                }
            });
        });
        if answer.is_none() && response.should_close() {
            answer = Some(SystemDialogAnswer::Cancel);
        }
        if let Some(answer) = answer {
            self.answered.insert(dialog.id);
            self.system_action = Some(SystemDialogReply {
                id: dialog.id,
                answer,
            });
            context.request_repaint();
        }
    }

    pub fn set_balloon(
        &mut self,
        balloon: Option<systemless::systems::macintosh::host_ui::BalloonSnapshot>,
        enabled: bool,
    ) {
        if self.balloon != balloon || self.balloon_help != enabled {
            let artwork = balloon
                .as_ref()
                .and_then(|b| b.artwork.clone())
                .filter(|image| image.is_valid());
            self.balloon_texture = artwork.map(|image| ArtworkTexture {
                texture: self.context.load_texture(
                    "balloon-art",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        &image.rgba,
                    ),
                    egui::TextureOptions::NEAREST,
                ),
                source: image,
            });
            self.balloon = balloon;
            self.balloon_help = enabled;
            self.context.request_repaint();
        }
    }

    /// A Help Manager balloon beside its guest hot spot.
    fn show_balloon(&mut self, context: &egui::Context) {
        let Some(balloon) = self.balloon.clone() else {
            return;
        };
        let Some(tip) = self.guest_to_points(balloon.tip.0, balloon.tip.1) else {
            return;
        };
        egui::Area::new(egui::Id::new(("balloon", balloon.id)))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .fixed_pos(tip + egui::vec2(12., 12.))
            .constrain(true)
            .show(context, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(10.)
                    .show(ui, |ui| {
                        ui.set_max_width(280.);
                        if let Some(art) = &self.balloon_texture {
                            let size =
                                egui::vec2(art.source.width as f32, art.source.height as f32);
                            ui.image((art.texture.id(), size));
                        }
                        if !balloon.text.is_empty() {
                            ui.label(&balloon.text);
                        }
                    });
            });
    }

    pub fn set_notifications(
        &mut self,
        notifications: &[systemless::systems::macintosh::host_ui::NotificationSnapshot],
    ) {
        if self.notifications != notifications {
            self.hidden_notifications
                .retain(|id| notifications.iter().any(|n| n.id == *id));
            self.notifications = notifications.to_vec();
            self.context.request_repaint();
        }
    }

    /// Report an unexpected guest stop once; a later stop reason replaces it.
    pub fn set_stopped(&mut self, reason: Option<String>) {
        if self.stopped != reason {
            if let Some(reason) = &reason {
                self.error = Some(format!(
                    "{reason} The guest is paused; you can inspect it or quit Systemless."
                ));
            }
            self.stopped = reason;
            self.context.request_repaint();
        }
    }

    pub fn set_guest_surface(
        &mut self,
        surface: Option<systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot>,
    ) {
        if self.guest_surface != surface {
            // A guest-drawn modal owns the keyboard as it would on the guest.
            if surface.as_ref().map(|s| s.id) != self.guest_surface.as_ref().map(|s| s.id)
                && surface.is_some()
            {
                self.guest_focused = true;
            }
            self.guest_surface = surface;
            self.context.request_repaint();
        }
    }

    pub fn set_guest_windows(
        &mut self,
        windows: &[systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot],
    ) {
        if self.guest_windows != windows {
            let old_front = self.guest_windows.first().map(|w| w.id);
            self.guest_windows = windows.to_vec();
            self.guest_window_states
                .retain(|id, _| windows.iter().any(|w| w.id == *id));
            // Guest activation raises the window it selected.
            if let Some(front) = windows.first().filter(|w| Some(w.id) != old_front) {
                self.guest_window_states
                    .entry(front.id)
                    .or_default()
                    .request_front();
            }
            self.context.request_repaint();
        }
    }

    /// Frame guest-drawn modeless dialogs in independent egui windows. Input
    /// over their pixels maps back to guest space, so the application's own
    /// event loop selects, tracks and closes them as usual.
    fn show_guest_windows(
        &mut self,
        context: &egui::Context,
        texture: Option<(egui::TextureId, egui::Vec2)>,
    ) {
        let (Some((texture, _)), Some(pane)) = (texture, self.guest_rect) else {
            return;
        };
        let (left, top, width, height) = self.crop;
        if width == 0 || height == 0 || !pane.is_positive() {
            return;
        }
        let zoom = (pane.width() / width as f32).max(0.25);
        let blocked =
            self.has_host_modal() || self.preferences || self.about || self.error.is_some();
        for (index, surface) in self.guest_windows.clone().iter().enumerate().rev() {
            let (t, l, b, r) = surface.bounds;
            let size = egui::vec2((r - l) as f32, (b - t) as f32) * zoom;
            let uv = egui::Rect::from_min_max(
                egui::pos2(
                    (l as f32 - left as f32) / width as f32,
                    (t as f32 - top as f32) / height as f32,
                ),
                egui::pos2(
                    (r as f32 - left as f32) / width as f32,
                    (b as f32 - top as f32) / height as f32,
                ),
            );
            let state = self.guest_window_states.entry(surface.id).or_default();
            let response = systemless_ui::window::show(
                context,
                egui::Id::new(("guest-window", surface.id)),
                if surface.title.is_empty() {
                    "Dialog"
                } else {
                    &surface.title
                },
                state,
                systemless_ui::window::WindowOptions {
                    initial_rect: egui::Rect::from_min_size(
                        pane.left_top() + egui::vec2(24., 24.) * (index as f32 + 1.),
                        size + egui::vec2(24., 48.),
                    ),
                    enabled: !blocked,
                    resizable: false,
                    closable: false,
                },
                |ui| {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                    ui.painter().image(texture, rect, uv, egui::Color32::WHITE);
                    rect
                },
            );
            if let Some(rect) = response.inner {
                self.lifted_windows
                    .push((rect, surface.bounds, response.response.layer_id));
            }
        }
    }

    /// Frame a guest-rendered modal dialog in a host modal surface. Its pixels
    /// remain the guest's; pointer input over them maps back to guest space.
    fn show_guest_surface(
        &mut self,
        context: &egui::Context,
        texture: Option<(egui::TextureId, egui::Vec2)>,
    ) {
        let (Some(surface), Some((texture, _)), Some(pane)) =
            (self.guest_surface.clone(), texture, self.guest_rect)
        else {
            return;
        };
        let (left, top, width, height) = self.crop;
        if width == 0 || height == 0 || !pane.is_positive() {
            return;
        }
        let (t, l, b, r) = surface.bounds;
        let zoom = (pane.width() / width as f32).max(0.25);
        let size = egui::vec2((r - l) as f32, (b - t) as f32) * zoom;
        let uv = egui::Rect::from_min_max(
            egui::pos2(
                (l as f32 - left as f32) / width as f32,
                (t as f32 - top as f32) / height as f32,
            ),
            egui::pos2(
                (r as f32 - left as f32) / width as f32,
                (b as f32 - top as f32) / height as f32,
            ),
        );
        let response =
            egui::Modal::new(egui::Id::new(("guest-surface", surface.id))).show(context, |ui| {
                if !surface.title.is_empty() {
                    ui.strong(&surface.title);
                    ui.add_space(6.);
                }
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                ui.painter().image(texture, rect, uv, egui::Color32::WHITE);
                ui.painter().rect_stroke(
                    rect,
                    0.,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                    egui::StrokeKind::Outside,
                );
                (rect, response.layer_id)
            });
        let (rect, layer) = response.inner;
        self.lifted = Some((rect, surface.bounds, layer));
    }

    /// Notification Manager alerts as non-blocking cards in the top-right.
    fn show_notifications(&mut self, context: &egui::Context) {
        let mut offset = 48.;
        for notification in self.notifications.clone() {
            if self.hidden_notifications.contains(&notification.id) {
                continue;
            }
            let response = egui::Area::new(egui::Id::new(("notification", notification.id)))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16., offset))
                .show(context, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(12.)
                        .show(ui, |ui| {
                            ui.set_min_width(260.);
                            ui.set_max_width(320.);
                            ui.strong("Notification");
                            ui.label(&notification.text);
                            ui.add_space(4.);
                            systemless_ui::components::button(
                                ui,
                                &self.theme,
                                "OK",
                                systemless_ui::components::ButtonStyle::Primary,
                            )
                            .clicked()
                        })
                        .inner
                });
            offset += response.response.rect.height() + 8.;
            if response.inner {
                self.hidden_notifications.insert(notification.id);
                self.dismissed_notifications.push(notification.id);
                context.request_repaint();
            }
        }
    }

    fn retain_answered(&mut self, popup: Option<u64>, system: Option<u64>) {
        self.answered
            .retain(|id| Some(*id) == popup || Some(*id) == system);
    }

    /// Guest presented-canvas coordinates to workspace points.
    fn guest_to_points(&self, v: i16, h: i16) -> Option<egui::Pos2> {
        let rect = self.guest_rect.filter(|rect| rect.is_positive())?;
        let (left, top, width, height) = self.crop;
        if width == 0 || height == 0 {
            return None;
        }
        Some(egui::pos2(
            rect.left() + (h as f32 - left as f32) / width as f32 * rect.width(),
            rect.top() + (v as f32 - top as f32) / height as f32 * rect.height(),
        ))
    }

    /// Present a parked PopUpMenuSelect call. Choosing an item or dismissing
    /// the menu replies once; the guest call resumes with the result.
    fn show_popup_menu(&mut self, context: &egui::Context) {
        let Some(popup) = self.popup_menu.clone() else {
            return;
        };
        if self.answered.contains(&popup.id) {
            return;
        }
        let rows = self
            .menus
            .as_deref()
            .map(|menus| {
                menu_rows(
                    menus,
                    self.menu_help.as_deref(),
                    popup.menu,
                    &mut Vec::new(),
                )
            })
            .unwrap_or_default();
        // A menu missing from the published list cannot be shown; dismiss it
        // rather than leave the guest call parked behind an invisible popup.
        let (choice, dismissed) = if rows.is_empty() {
            (None, true)
        } else {
            let anchor = self
                .guest_to_points(popup.anchor.0, popup.anchor.1)
                .unwrap_or_else(|| context.content_rect().center());
            let response = systemless_ui::menu::popup(
                context,
                egui::Id::new(("guest-popup-menu", popup.id)),
                anchor,
                popup.item.max(1) as usize - 1,
                &rows,
            );
            (response.chosen, response.dismissed)
        };
        if choice.is_some() || dismissed {
            self.answered.insert(popup.id);
            self.popup_action = Some(systemless::systems::macintosh::host_ui::PopupMenuReply {
                id: popup.id,
                choice,
            });
            context.request_repaint();
        }
    }

    pub fn set_file_dialog(&mut self, snapshot: Option<std::sync::Arc<FileDialogSnapshot>>) {
        if self.file_dialog == snapshot {
            return;
        }
        let old = self.file_dialog.as_ref().map(|d| (d.id, d.revision));
        let new = snapshot.as_ref().map(|d| (d.id, d.revision));
        if old != new {
            self.file_picker.reset_selection();
            self.file_pending = false;
            if old.map(|d| d.0) != new.map(|d| d.0) {
                self.file_picker.name = snapshot
                    .as_ref()
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
            }
        }
        self.file_dialog = snapshot;
        self.context.request_repaint();
    }

    pub fn init_accessibility(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: &Window,
        proxy: winit::event_loop::EventLoopProxy<super::DesktopEvent>,
    ) {
        self.platform
            .as_mut()
            .unwrap()
            .init_accesskit(event_loop, window, proxy);
    }
    pub fn accessibility_event(&mut self, event: egui_winit::accesskit_winit::WindowEvent) -> bool {
        use egui_winit::accesskit_winit::WindowEvent;
        match event {
            WindowEvent::InitialTreeRequested => {
                self.accessibility_active = true;
                self.context.enable_accesskit();
            }
            WindowEvent::ActionRequested(request) => {
                if let Some(platform) = self.platform.as_mut() {
                    platform.on_accesskit_action_request(request);
                }
            }
            WindowEvent::AccessibilityDeactivated => {
                self.accessibility_active = false;
                self.context.disable_accesskit();
                return false;
            }
        }
        self.context.request_repaint();
        true
    }
    pub fn new(window: &Window, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let mut ui = Self::headless(device, format);
        ui.platform = Some(egui_winit::State::new(
            ui.context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        ));
        ui
    }
    fn headless(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let context = egui::Context::default();
        let theme = Theme::default();
        theme.apply(&context);
        Self {
            context,
            platform: None,
            accessibility_active: false,
            renderer: egui_wgpu::Renderer::new(device, format, Default::default()),
            dock: initial_layout(),
            pane_layers: Default::default(),
            theme,
            texture: None,
            guest_rect: None,
            guest_hovered: false,
            guest_layer: None,
            guest_focused: false,
            guest_drag: false,
            ui_pointer_buttons: Default::default(),
            keys: Default::default(),
            pointer: egui::Pos2::ZERO,
            crop: (0, 0, 1, 1),
            inspection: Inspection::default(),
            modality: Default::default(),
            menus: None,
            menu_actions: Vec::new(),
            dialog: None,
            dialog_actions: Vec::new(),
            dialog_editors: Default::default(),
            dialog_surfaces: Default::default(),
            modeless: Vec::new(),
            modeless_sessions: Default::default(),
            modeless_actions: Vec::new(),
            popup_menu: None,
            popup_action: None,
            system_dialog: None,
            color_draft: egui::Color32::WHITE,
            system_action: None,
            answered: Default::default(),
            notifications: Vec::new(),
            balloon: None,
            balloon_texture: None,
            menu_help: None,
            balloon_help: false,
            balloon_help_action: None,
            dismissed_notifications: Vec::new(),
            hidden_notifications: Default::default(),
            stopped: None,
            guest_surface: None,
            lifted: None,
            guest_windows: Vec::new(),
            guest_window_states: Default::default(),
            lifted_windows: Vec::new(),
            artwork_textures: Default::default(),
            dialog_text_actions: Vec::new(),
            dialog_control_actions: Vec::new(),
            pending_controls: Default::default(),
            file_dialog: None,
            file_picker: Default::default(),
            file_pending: false,
            file_reply: None,
            file_actions: Vec::new(),
            preferences: false,
            about: false,
            error: None,
            storage: None,
            repaint_at: std::time::Instant::now(),
        }
    }

    pub fn configure_storage(&mut self, path: std::path::PathBuf, window: &Window) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            if bytes.len() > 1_048_576 {
                return Err("Workspace settings exceed size limit".into());
            }
            let saved: SavedWorkspace = serde_json::from_slice(&bytes)?;
            if saved.version != 1 {
                return Err("Unsupported workspace settings version".into());
            }
            let size = window.inner_size().to_logical::<f32>(window.scale_factor());
            self.dock = systemless_ui::workspace::restore(
                &saved.layout,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.width, size.height)),
            )?;
            let tabs: Vec<_> = self.dock.iter_all_tabs().map(|(_, t)| *t).collect();
            if tabs.len() != 2 || !tabs.contains(&Pane::Guest) || !tabs.contains(&Pane::Inspector) {
                self.dock = initial_layout();
                return Err("Workspace must contain Guest and Inspector panes".into());
            }
            self.theme = Theme::new(if saved.light {
                Appearance::Light
            } else {
                Appearance::Dark
            });
            self.theme.reduced_motion = saved.reduced_motion;
            self.theme.apply(&self.context);
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(format!("Could not restore workspace: {error}"));
        }
        self.storage = Some(path);
    }
    pub fn has_guest_texture(&self) -> bool {
        self.texture.is_some()
    }
    pub fn show_error(&mut self, error: String) {
        self.error = Some(error);
        self.context.request_repaint();
    }
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(path) = &self.storage else {
            return Ok(());
        };
        let saved = SavedWorkspace {
            version: 1,
            light: self.theme.appearance == Appearance::Light,
            reduced_motion: self.theme.reduced_motion,
            layout: systemless_ui::workspace::save_with_layers(
                &self.dock,
                &self.context,
                |pane| self.pane_layers.get(pane).copied(),
            )?,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, serde_json::to_vec_pretty(&saved)?)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }
    /// Feed all events to egui; return whether the guest owns this input event.
    pub fn event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self
            .platform
            .as_mut()
            .unwrap()
            .on_window_event(window, event);
        if response.repaint {
            window.request_redraw();
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = egui::pos2(
                    position.x as f32 / window.scale_factor() as f32,
                    position.y as f32 / window.scale_factor() as f32,
                );
                self.guest_drag || (self.pointer_over_guest() && self.guest_input_available())
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if *state == ElementState::Released {
                    self.ui_pointer_buttons.remove(button);
                    let owned = *button == MouseButton::Left && self.guest_drag;
                    if *button == MouseButton::Left {
                        self.guest_drag = false;
                    }
                    return owned;
                }
                let owned = *button == MouseButton::Left
                    && self.pointer_over_guest()
                    && self.guest_input_available();
                self.guest_focused = owned;
                if *button == MouseButton::Left {
                    self.guest_drag = owned;
                }
                if !owned {
                    self.ui_pointer_buttons.insert(*button);
                }
                owned
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Released {
                    return self.keys.remove(&event.physical_key);
                }
                if self.keys.contains(&event.physical_key) {
                    return true;
                }
                let owned = self.guest_focused
                    && self
                        .context
                        .memory(|m| m.focused())
                        .is_none_or(|id| id == egui::Id::new("guest-input"))
                    && self.guest_input_available();
                if owned {
                    self.keys.insert(event.physical_key);
                }
                owned
            }
            _ => false,
        }
    }
    fn guest_input_available(&self) -> bool {
        // UI-owned gestures keep their owner until release, even when a
        // splitter or floating pane crosses the guest image while moving.
        self.guest_rect.is_some_and(|rect| rect.is_positive())
            && self.guest_layer.is_some()
            && self.ui_pointer_buttons.is_empty()
            && !egui::Popup::is_any_open(&self.context)
            && !self.has_host_modal()
            && !self.preferences
            && !self.about
            && self.error.is_none()
    }
    /// The lifted guest surface under a workspace point. A lifted modal
    /// claims the whole window: elsewhere its backdrop blocks guest input.
    fn lifted_at(&self, pos: egui::Pos2) -> Option<Option<LiftedSurface>> {
        let hit = |surface: &LiftedSurface| {
            surface.0.contains(pos) && self.context.layer_id_at(pos) == Some(surface.2)
        };
        if let Some(modal) = self.lifted {
            return Some(hit(&modal).then_some(modal));
        }
        self.lifted_windows
            .iter()
            .find(|surface| hit(surface))
            .map(|surface| Some(*surface))
    }
    fn pointer_over_guest(&self) -> bool {
        if let Some(surface) = self.lifted_at(self.pointer) {
            return surface.is_some();
        }
        self.guest_rect.is_some_and(|r| r.contains(self.pointer))
            && self.context.layer_id_at(self.pointer) == self.guest_layer
    }
    pub fn release_input(&mut self) -> (bool, Vec<winit::keyboard::PhysicalKey>) {
        let mouse = std::mem::take(&mut self.guest_drag);
        self.ui_pointer_buttons.clear();
        self.guest_focused = false;
        (mouse, self.keys.drain().collect())
    }
    pub fn recreate_gpu(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) {
        // The native adapter belongs to the window, not the GPU. Recreating it
        // after the window is visible would violate AccessKit's lifecycle.
        let accesskit = self
            .platform
            .as_mut()
            .and_then(|platform| platform.accesskit.take());
        self.renderer = egui_wgpu::Renderer::new(device, format, Default::default());
        self.texture = None;
        self.reset_egui_context();
        self.platform = Some(egui_winit::State::new(
            self.context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        ));
        self.platform.as_mut().unwrap().accesskit = accesskit;
    }
    fn reset_egui_context(&mut self) {
        self.context = egui::Context::default();
        self.guest_rect = None;
        self.guest_layer = None;
        if self.accessibility_active {
            self.context.enable_accesskit();
        }
        self.artwork_textures.clear();
        self.sync_artwork_textures();
        self.dialog_editors.active.restore_focus();
        for dialog in &self.modeless {
            if let Some(session) = self.modeless_sessions.get_mut(&dialog.id) {
                session.artwork.clear();
                sync_artwork(&self.context, &mut session.artwork, Some(dialog));
                session.editors.restore_focus();
            }
        }
        self.theme.apply(&self.context);
    }
    pub fn guest_to_physical(&self, v: i16, h: i16, scale: f64) -> Option<(f64, f64)> {
        if !self.guest_focused || !self.guest_input_available() {
            return None;
        }
        let containing = self.lifted.or_else(|| {
            self.lifted_windows
                .iter()
                .find(|(_, (t, l, b, r), _)| (*t..*b).contains(&v) && (*l..*r).contains(&h))
                .copied()
        });
        if let Some((rect, (t, l, b, r), _)) = containing {
            let x = ((h as f64 - l as f64).clamp(0., (r - l - 1).max(0) as f64) + 0.5)
                / (r - l).max(1) as f64;
            let y = ((v as f64 - t as f64).clamp(0., (b - t - 1).max(0) as f64) + 0.5)
                / (b - t).max(1) as f64;
            return Some((
                (rect.left() as f64 + x * rect.width() as f64) * scale,
                (rect.top() as f64 + y * rect.height() as f64) * scale,
            ));
        }
        let rect = self.guest_rect?;
        let (left, top, width, height) = self.crop;
        if width == 0 || height == 0 || !rect.is_positive() {
            return None;
        }
        // Warp to the pixel cell's center, keeping edge pixels inside the
        // pane and making the inverse mapping exact for cropped images.
        let x = ((h as f64 - left as f64).clamp(0., width.saturating_sub(1) as f64) + 0.5)
            / width as f64;
        let y = ((v as f64 - top as f64).clamp(0., height.saturating_sub(1) as f64) + 0.5)
            / height as f64;
        let center = (
            (rect.left() as f64 + x * rect.width() as f64) * scale,
            (rect.top() as f64 + y * rect.height() as f64) * scale,
        );
        let expected = (
            (top as f64 + (v as f64 - top as f64).clamp(0., height.saturating_sub(1) as f64))
                as i16,
            (left as f64 + (h as f64 - left as f64).clamp(0., width.saturating_sub(1) as f64))
                as i16,
        );
        // Winit rounds OS warps to native pixels on some platforms. Prefer
        // a representable point inside this guest pixel's cell; retain the
        // fractional center when downscaling leaves no matching native pixel.
        for px in [center.0.round(), center.0.floor(), center.0.ceil()] {
            for py in [center.1.round(), center.1.floor(), center.1.ceil()] {
                if px >= rect.left() as f64 * scale
                    && px < rect.right() as f64 * scale
                    && py >= rect.top() as f64 * scale
                    && py < rect.bottom() as f64 * scale
                    && self.map_pointer(px, py, scale) == Some(expected)
                {
                    return Some((px, py));
                }
            }
        }
        Some(center)
    }
    #[cfg(feature = "test-support")]
    pub(super) fn native_probe_splitter(&self) -> Option<egui::Pos2> {
        let tree = self.dock.main_surface();
        if !matches!(
            tree.root_node()?,
            systemless_ui::egui_dock::Node::Horizontal(_)
        ) {
            return None;
        }
        let left = tree[NodeIndex::root().left()].rect()?;
        let right = tree[NodeIndex::root().right()].rect()?;
        Some(egui::pos2(
            (left.right() + right.left()) * 0.5,
            left.center().y,
        ))
    }
    #[cfg(feature = "test-support")]
    pub(super) fn native_probe_hide_guest(&mut self) {
        if let Some(path) = self.dock.find_tab(&Pane::Guest) {
            self.dock[path.node_path()].set_collapsed(true);
        }
        self.context.request_repaint();
    }
    pub fn map_pointer(&self, px: f64, py: f64, scale: f64) -> Option<(i16, i16)> {
        let pos = egui::pos2((px / scale) as f32, (py / scale) as f32);
        let surface = match self.lifted_at(pos) {
            Some(surface) => surface.or(self.lifted),
            None => None,
        };
        if let Some((rect, (t, l, b, r), _)) = surface {
            if !rect.is_positive() {
                return None;
            }
            let x = ((px / scale - rect.left() as f64) / rect.width() as f64).clamp(0., 1.);
            let y = ((py / scale - rect.top() as f64) / rect.height() as f64).clamp(0., 1.);
            return Some((
                (t as f64 + (y * (b - t) as f64).floor().min((b - t - 1).max(0) as f64)) as i16,
                (l as f64 + (x * (r - l) as f64).floor().min((r - l - 1).max(0) as f64)) as i16,
            ));
        }
        let r = self.guest_rect?;
        if !r.is_positive() {
            return None;
        }
        let x = ((px / scale - r.left() as f64) / r.width() as f64).clamp(0., 1.);
        let y = ((py / scale - r.top() as f64) / r.height() as f64).clamp(0., 1.);
        let (left, top, width, height) = self.crop;
        if width == 0 || height == 0 {
            return None;
        }
        Some((
            (top as f64
                + (y * height as f64)
                    .floor()
                    .min(height.saturating_sub(1) as f64)) as i16,
            (left as f64
                + (x * width as f64)
                    .floor()
                    .min(width.saturating_sub(1) as f64)) as i16,
        ))
    }
    pub fn target(&mut self, device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        if self
            .texture
            .as_ref()
            .is_none_or(|(t, _)| t.width() != size.0 || t.height() != size.1)
        {
            if let Some((_, id)) = self.texture.take() {
                self.renderer.free_texture(&id);
            }
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("workspace guest"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let id = self.renderer.register_native_texture(
                device,
                &texture.create_view(&Default::default()),
                wgpu::FilterMode::Nearest,
            );
            self.texture = Some((texture, id));
        }
        self.texture
            .as_ref()
            .unwrap()
            .0
            .create_view(&Default::default())
    }
    fn build_frame(&mut self, input: egui::RawInput) -> egui::FullOutput {
        let texture = self
            .texture
            .as_ref()
            .map(|(t, id)| (*id, egui::vec2(t.width() as f32, t.height() as f32)));
        self.guest_rect = None;
        self.guest_layer = None;
        self.lifted = None;
        self.lifted_windows.clear();
        self.pane_layers.clear();
        self.guest_hovered = false;
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| {
            egui::Panel::top("workspace-menu").show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    if !self.menus.as_ref().is_some_and(|menus| {
                        menus.menus.iter().any(|menu| {
                            menu.visible_in_menu_bar && !menu.hierarchical && menu.title == "Systemless"
                        })
                    }) {
                        ui.strong("Systemless");
                    }
                    if let Some(menus) = &self.menus {
                        for menu in menus
                            .menus
                            .iter()
                            .filter(|menu| menu.visible_in_menu_bar && !menu.hierarchical)
                        {
                            ui.add_enabled_ui(menu.enabled && !self.modality.menus_blocked, |ui| {
                                ui.menu_button(&menu.title, |ui| {
                                    let rows = menu_rows(menus, self.menu_help.as_deref(), menu.id, &mut Vec::new());
                                    if let Some(choice) = systemless_ui::menu::items(ui, &rows) {
                                        self.menu_actions.push(choice);
                                    }
                                });
                            });
                        }
                    }
                    ui.menu_button("Workspace", |ui| {
                        if ui.button("Reset layout").clicked() {
                            self.dock = initial_layout();
                            ui.close();
                        }
                        if ui.button("Preferences…").clicked() {
                            self.preferences = true;
                            ui.close();
                        }
                        if ui.button("About Systemless").clicked() {
                            self.about = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        let mut enabled = self.balloon_help;
                        let label = if enabled { "Hide Balloons" } else { "Show Balloons" };
                        if ui.button(label).clicked() {
                            enabled = !enabled;
                            self.balloon_help = enabled;
                            self.balloon_help_action = Some(enabled);
                            ui.close();
                        }
                    });
                });
            });
            egui::CentralPanel::default().show(ui, |ui| {
                systemless_ui::workspace::show(
                    ui,
                    &self.theme,
                    &mut self.dock,
                    &mut Viewer {
                        texture,
                        rect: &mut self.guest_rect,
                        hovered: &mut self.guest_hovered,
                        layer: &mut self.guest_layer,
                        inspection: &self.inspection,
                        layers: &mut self.pane_layers,
                    },
                );
            });
            self.show_guest_windows(ui.ctx(), texture);
            self.show_guest_surface(ui.ctx(), texture);
            self.show_modeless(ui.ctx());
            self.show_popup_menu(ui.ctx());
            self.show_system_dialog(ui.ctx());
            self.show_notifications(ui.ctx());
            self.show_balloon(ui.ctx());
            if let Some(dialog) = self.dialog.clone().filter(|_| self.modality.owner == UiOwner::Dialog) {
                let buttons: Vec<_> = dialog.items.iter().filter(|i| i.kind == 4).map(|i| i.number).collect();
                let response = systemless_ui::dialog::show_modal(
                    ui.ctx(),
                    egui::Id::new(("macintosh-dialog", dialog.id)),
                    dialog.movable,
                    self.dialog_surfaces.entry(dialog.id).or_default(),
                    |ui| {
                        dialog_form(
                            ui,
                            &self.theme,
                            &dialog,
                            &self.artwork_textures,
                            &mut self.dialog_editors.active,
                            true,
                            true,
                        )
                    },
                );
                let cancel = (!dialog.busy && response.should_close())
                    .then_some(dialog.cancel_item)
                    .filter(|id| {
                        buttons.contains(id)
                            && dialog.items.iter().any(|i| i.number == *id && i.enabled)
                    });
                for (id, _) in response.inner.edits {
                    self.dialog_editors.active.edited(id);
                }
                for (item, value) in response.inner.values {
                    if let Some(source) = dialog.items.iter().find(|source| source.number == item) {
                        self.pending_controls
                            .insert(dialog.id, (item, source.value, value));
                        self.dialog_editors.active.focus_control(item);
                        self.dialog_editors.active.action = Some(item);
                    }
                }
                if let Some(item) = response.inner.action.or(cancel) {
                    self.pending_controls.remove(&dialog.id);
                    self.dialog_editors.active.action = Some(item);
                }
                let (text, action) = self.dialog_editors.active.flush(&dialog);
                if let Some(text) = text {
                    self.dialog_text_actions.push(text);
                }
                if let Some(action) = action {
                    if let Some((_, expected, value)) = self
                        .pending_controls
                        .remove(&dialog.id)
                        .filter(|(item, _, _)| *item == action.item)
                    {
                        self.dialog_control_actions.push(
                            systemless::systems::macintosh::host_ui::DialogControlReply {
                                dialog: action,
                                expected,
                                value,
                            },
                        );
                    } else {
                        self.dialog_actions.push(action);
                    }
                }
            }
            if let Some(dialog) = self.file_dialog.clone().filter(|_| self.modality.owner == UiOwner::FileDialog) {
                use systemless_ui::file_picker::{Action, Entry, Listing};
                let entries: Vec<_> = dialog.entries.iter().map(|e| Entry { name: &e.name, directory: e.directory }).collect();
                let response = egui::Modal::new(egui::Id::new(("guest-file-dialog", dialog.id))).show(ui.ctx(), |ui| {
                    if dialog.busy {
                        ui.horizontal(|ui| { ui.spinner(); ui.label("The application is preparing this folder…"); });
                    }
                    ui.add_enabled_ui(!self.file_pending && !dialog.busy, |ui| self.file_picker.show(ui, &self.theme, Listing {
                        title: &dialog.prompt, directory: &dialog.directory, entries: &entries,
                        save: dialog.kind == systemless::systems::macintosh::host_ui::FileDialogKind::Save,
                        writable: dialog.writable,
                    })).inner
                });
                let close = !self.file_pending && !dialog.busy && response.should_close();
                let action = response.inner.or_else(|| close.then_some(Action::Cancel));
                if let Some(action) = action {
                    let action = match action {
                        Action::Open(i) => FileDialogAction::Open(i), Action::Save(name) => FileDialogAction::Save(name),
                        Action::Parent => FileDialogAction::Parent, Action::Root => FileDialogAction::Desktop, Action::Cancel => FileDialogAction::Cancel,
                    };
                    self.file_actions.push(FileDialogReply { id: dialog.id, revision: dialog.revision, action });
                    self.file_pending = true;
                }
            }
            if self.preferences {
                let response =
                    egui::Modal::new(egui::Id::new("preferences")).show(ui.ctx(), |ui| {
                        ui.set_min_width(320.);
                        ui.heading("Preferences");
                        let mut light = self.theme.appearance == Appearance::Light;
                        if systemless_ui::components::checkbox(ui, &self.theme, "Light appearance", &mut light).changed() {
                            let reduced_motion = self.theme.reduced_motion;
                            self.theme = Theme::new(if light {
                                Appearance::Light
                            } else {
                                Appearance::Dark
                            });
                            self.theme.reduced_motion = reduced_motion;
                            self.theme.apply(ui.ctx());
                        }
                        let mut reduced_motion = self.theme.reduced_motion;
                        if systemless_ui::components::checkbox(ui, &self.theme, "Reduce motion", &mut reduced_motion).changed() {
                            self.theme.reduced_motion = reduced_motion;
                            self.theme.apply(ui.ctx());
                        }
                        ui.separator();
                        ui.button("Done").clicked()
                    });
                if response.inner || response.should_close() {
                    self.preferences = false;
                }
            }
            if let Some(error) = self.error.clone() {
                let response =
                    egui::Modal::new(egui::Id::new("workspace-error")).show(ui.ctx(), |ui| {
                        ui.set_max_width(480.);
                        ui.heading("Systemless");
                        ui.label(&error);
                        ui.button("Dismiss").clicked()
                    });
                if response.inner || response.should_close() {
                    self.error = None;
                }
            }
            if self.about {
                let response = egui::Modal::new(egui::Id::new("about")).show(ui.ctx(), |ui| {
                    ui.heading("Systemless");
                    ui.label("A modern home for classic Macintosh applications.");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.button("Close").clicked()
                });
                if response.inner || response.should_close() {
                    self.about = false;
                }
            }
        });
        if !self.guest_rect.is_some_and(|rect| rect.is_positive()) {
            // Tab switches and collapsed panes revoke new input, while held
            // keys/buttons retain their owners until their release arrives.
            self.guest_focused = false;
            self.context.memory_mut(|memory| {
                memory.surrender_focus(egui::Id::new("guest-input"));
            });
        }
        output
    }

    pub fn draw(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
    ) {
        let input = self.platform.as_mut().unwrap().take_egui_input(window);
        let mut output = self.build_frame(input);
        self.platform
            .as_mut()
            .unwrap()
            .handle_platform_output(window, std::mem::take(&mut output.platform_output));
        window.set_cursor_visible(
            !(self.pointer_over_guest()
                && !self.has_host_modal()
                && !self.preferences
                && !self.about
                && self.error.is_none()),
        );
        let size = window.inner_size();
        self.paint(device, queue, target, [size.width, size.height], output);
    }
    fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        output: egui::FullOutput,
    ) {
        let jobs = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: output.pixels_per_point,
        };
        let mut delta = output.textures_delta;
        for (id, images) in &delta.set {
            for image in images {
                self.renderer.update_texture(device, queue, *id, image);
            }
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands = self
            .renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("workspace UI"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &screen);
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        for id in &delta.free {
            self.renderer.free_texture(id);
        }
        delta.clear();
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.repaint_at = std::time::Instant::now()
            .checked_add(delay)
            .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(86400));
    }
}
/// Render a projected Macintosh dialog's items through the standalone form.
/// Static labels adjacent to editors and controls become their accessible labels.
fn dialog_form(
    ui: &mut egui::Ui,
    theme: &Theme,
    dialog: &systemless::systems::macintosh::host_ui::DialogSnapshot,
    artwork: &std::collections::HashMap<i16, ArtworkTexture>,
    editors: &mut dialog_editors::DialogEditors,
    shortcuts: bool,
    heading: bool,
) -> systemless_ui::dialog::FormResponse {
    let mut body: Vec<_> = dialog.items.iter().filter(|i| i.kind != 4).collect();
    body.sort_by_key(|i| (i.rect.0, i.rect.1, i.number));
    let labels: std::collections::HashMap<_, _> = body
        .iter()
        .filter(|i| matches!(i.kind, 7 | 16 | 17))
        .filter_map(|edit| {
            body.iter()
                .filter(|label| {
                    label.kind == 8
                        && label.rect.3 <= edit.rect.1
                        && label.rect.0 < edit.rect.2
                        && label.rect.2 > edit.rect.0
                })
                .max_by_key(|label| label.rect.3)
                .map(|label| (edit.number, *label))
        })
        .collect();
    let choices: std::collections::HashMap<_, Vec<_>> = body
        .iter()
        .filter(|item| item.kind == 7)
        .map(|item| {
            (
                item.number,
                item.choices
                    .iter()
                    .map(|choice| systemless_ui::dialog::Choice {
                        value: choice.value,
                        text: &choice.text,
                        enabled: choice.enabled,
                    })
                    .collect(),
            )
        })
        .collect();
    let control_focus = editors.take_control_focus();
    let fields: Vec<_> = body
        .iter()
        .filter(|i| !labels.values().any(|label| label.number == i.number))
        .map(|i| systemless_ui::dialog::Field {
            id: i.number,
            text: if matches!(i.kind, 7 | 16 | 17) {
                labels.get(&i.number).map_or(
                    if matches!(i.kind, 7 | 17) {
                        i.text.as_str()
                    } else {
                        ""
                    },
                    |label| label.text.as_str(),
                )
            } else if i.kind == 32 {
                "Application icon"
            } else if i.kind == 64 {
                "Application picture"
            } else {
                &i.text
            },
            enabled: matches!(i.kind, 8 | 32 | 64) || i.enabled,
            kind: match i.kind {
                17 => {
                    let range = i.range.unwrap();
                    systemless_ui::dialog::FieldKind::Range {
                        value: i.value,
                        min: range.min,
                        max: range.max,
                        request_focus: control_focus == Some(i.number),
                    }
                }
                7 => systemless_ui::dialog::FieldKind::Popup {
                    value: i.value,
                    choices: &choices[&i.number],
                },
                5 => systemless_ui::dialog::FieldKind::Checkbox(i.value != 0),
                6 => systemless_ui::dialog::FieldKind::Radio(i.value != 0),
                16 => systemless_ui::dialog::FieldKind::Edit,
                32 | 64 => systemless_ui::dialog::FieldKind::Image {
                    texture: artwork[&i.number].texture.id(),
                    size: [
                        (i.rect.3 as i32 - i.rect.1 as i32).clamp(1, 1024) as u16,
                        (i.rect.2 as i32 - i.rect.0 as i32).clamp(1, 1024) as u16,
                    ],
                    monochrome: i.artwork.as_ref().unwrap().monochrome,
                    interactive: i.enabled,
                },
                _ => systemless_ui::dialog::FieldKind::Text,
            },
        })
        .collect();
    let buttons: Vec<_> = dialog
        .items
        .iter()
        .filter(|i| i.kind == 4)
        .map(|i| systemless_ui::dialog::Button {
            id: i.number,
            text: &i.text,
            enabled: i.enabled,
        })
        .collect();
    if let Some(error) = &editors.error {
        ui.colored_label(theme.colors.danger, error);
    }
    ui.add_enabled_ui(!dialog.busy && editors.action.is_none(), |ui| {
        systemless_ui::dialog::show_with_shortcuts(
            ui,
            theme,
            systemless_ui::dialog::Form {
                title: if heading { &dialog.title } else { "" },
                fields: &fields,
                buttons: &buttons,
                default: dialog.default_item,
                cancel: dialog.cancel_item,
            },
            &mut editors.fields,
            shortcuts,
        )
    })
    .inner
}

/// Rich/custom items retain guest rendering until their adapters exist.
fn host_renderable(dialog: &systemless::systems::macintosh::host_ui::DialogSnapshot) -> bool {
    !dialog.items.is_empty()
        && dialog.items.iter().all(|i| {
            matches!(i.kind, 4..=7 | 8 | 16)
                || (i.kind == 17
                    && i.range
                        .is_some_and(|range| range.min <= i.value && i.value <= range.max))
                || (matches!(i.kind, 32 | 64)
                    && i.artwork.as_ref().is_some_and(|image| image.is_valid()))
        })
}

/// Upload changed item artwork, reusing each item's texture slot.
fn sync_artwork(
    context: &egui::Context,
    cache: &mut std::collections::HashMap<i16, ArtworkTexture>,
    dialog: Option<&systemless::systems::macintosh::host_ui::DialogSnapshot>,
) {
    cache.retain(|number, _| {
        dialog.is_some_and(|d| {
            d.items
                .iter()
                .any(|i| i.number == *number && i.artwork.is_some())
        })
    });
    let Some(dialog) = dialog else {
        return;
    };
    for item in &dialog.items {
        let Some(image) = item.artwork.as_ref().filter(|image| image.is_valid()) else {
            continue;
        };
        if cache
            .get(&item.number)
            .is_some_and(|cached| cached.source == *image)
        {
            continue;
        }
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        if let Some(cached) = cache.get_mut(&item.number) {
            cached.texture.set(pixels, egui::TextureOptions::NEAREST);
            cached.source = image.clone();
        } else {
            let texture = context.load_texture(
                format!("dialog-art-{}-{}", dialog.id, item.number),
                pixels,
                egui::TextureOptions::NEAREST,
            );
            cache.insert(
                item.number,
                ArtworkTexture {
                    source: image.clone(),
                    texture,
                },
            );
        }
    }
}

/// Frontend state for one projected modeless dialog. It follows the guest
/// identity, so geometry and drafts survive hiding and context recreation.
#[derive(Default)]
struct ModelessSession {
    editors: dialog_editors::DialogEditors,
    window: systemless_ui::window::WindowState,
    artwork: std::collections::HashMap<i16, ArtworkTexture>,
    pending_control: Option<(i16, i16, i16)>,
    close: bool,
}

struct Viewer<'a> {
    layers: &'a mut std::collections::HashMap<Pane, egui::LayerId>,
    texture: Option<(egui::TextureId, egui::Vec2)>,
    rect: &'a mut Option<egui::Rect>,
    hovered: &'a mut bool,
    layer: &'a mut Option<egui::LayerId>,
    inspection: &'a Inspection,
}
impl TabViewer for Viewer<'_> {
    type Tab = Pane;
    fn on_tab_button(&mut self, tab: &mut Pane, response: &egui::Response) {
        self.layers.insert(*tab, response.layer_id);
    }
    fn id(&mut self, tab: &mut Pane) -> egui::Id {
        egui::Id::new(*tab)
    }
    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        match tab {
            Pane::Guest => "Guest",
            Pane::Inspector => "Inspector",
        }
        .into()
    }
    fn is_closeable(&self, _: &Pane) -> bool {
        false
    }
    fn scroll_bars(&self, _: &Pane) -> [bool; 2] {
        [false, false]
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Pane) {
        match tab {
            Pane::Guest => {
                if let Some((texture, size)) = self.texture {
                    let available = ui.available_size();
                    let scale = (available.x / size.x).min(available.y / size.y).max(0.);
                    let rect = egui::Rect::from_center_size(
                        ui.available_rect_before_wrap().center(),
                        size * scale,
                    );
                    ui.painter().image(
                        texture,
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        egui::Color32::WHITE,
                    );
                    let response = ui.interact(
                        rect,
                        egui::Id::new("guest-input"),
                        egui::Sense::click_and_drag(),
                    );
                    if response.clicked() {
                        response.request_focus();
                    }
                    *self.layer = Some(response.layer_id);
                    *self.rect = Some(response.rect);
                    *self.hovered = response.contains_pointer();
                } else {
                    ui.label("Waiting for guest frame…");
                }
            }
            Pane::Inspector => {
                ui.heading("Session inspector");
                ui.label("Live presentation snapshot");
                ui.separator();
                egui::Grid::new("inspection")
                    .spacing([16., 12.])
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                "Guest size",
                                format!(
                                    "{} × {}",
                                    self.inspection.screen.0, self.inspection.screen.1
                                ),
                            ),
                            ("Frame", self.inspection.sequence.to_string()),
                            ("Guest tick", self.inspection.tick.to_string()),
                            (
                                "Presentation",
                                if self.inspection.retained {
                                    "Retained GPU"
                                } else {
                                    "Raster upload"
                                }
                                .into(),
                            ),
                        ] {
                            ui.weak(label);
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                ui.add_space(16.);
                ui.label("Drag a tab to float it or dock it beside another pane.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn workspace_gpu_composes_guest_and_modal_at_both_dpi_scales() {
        use super::super::runtime_protocol::DialogTextAck;
        use systemless::systems::macintosh::host_ui::{DialogSnapshot, DialogTextReply};
        fn accept_text(ui: &mut WorkspaceUi, dialog: &mut DialogSnapshot, reply: DialogTextReply) {
            let field = dialog
                .items
                .iter_mut()
                .find(|item| item.number == reply.item)
                .unwrap();
            field.text = reply.text.clone();
            field.selection = Some(reply.selection);
            dialog.revision += 1;
            ui.set_dialog(Some(std::sync::Arc::new(dialog.clone())), &[dialog.id]);
            ui.set_dialog_text_reply(Some(&DialogTextAck {
                serial: reply.serial,
                id: dialog.id,
                accepted: true,
                text: Some(reply.text),
            }));
        }
        if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
            std::fs::create_dir_all(directory).unwrap();
        }
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut time = 0.;
                let mut test_input = || {
                    time += 0.2;
                    let mut input = egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    input
                };
                let mut ui = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                ui.theme = Theme::new(appearance);
                ui.theme.apply(&ui.context);
                ui.crop = (10, 20, 640, 480);
                let guest = ui.target(&device, (640, 480));
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &guest,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.25,
                                    g: 0.5,
                                    b: 0.75,
                                    a: 1.,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                }
                queue.submit([encoder.finish()]);
                let size = [(1000. * scale) as u32, (700. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                for _ in 0..3 {
                    let mut input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..test_input()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                let rect = ui.guest_rect.unwrap();
                assert!(
                    rect.left() > 0. && rect.right() < 800. && rect.width() / rect.height() > 1.3
                );
                let center = rect.center();
                let (v, h) = ui
                    .map_pointer(
                        center.x as f64 * scale as f64,
                        center.y as f64 * scale as f64,
                        scale as f64,
                    )
                    .unwrap();
                assert!((h - 329).abs() <= 1 && (v - 259).abs() <= 1);
                ui.guest_focused = true;
                for point in [(20, 10), (42, 32), (259, 329), (499, 649)] {
                    let physical = ui
                        .guest_to_physical(point.0, point.1, scale as f64)
                        .unwrap();
                    assert_eq!(
                        ui.map_pointer(physical.0, physical.1, scale as f64),
                        Some(point),
                        "guest cursor warp must round trip through a cropped pane at {scale}x"
                    );
                }
                let original_crop = ui.crop;
                for crop in [(10, 20, 0, 480), (10, 20, 640, 0)] {
                    ui.crop = crop;
                    assert!(ui.guest_to_physical(20, 10, scale as f64).is_none());
                    assert!(ui
                        .map_pointer(center.x as f64, center.y as f64, scale as f64)
                        .is_none());
                }
                ui.crop = (10, 20, 1, 1);
                let physical = ui.guest_to_physical(20, 10, scale as f64).unwrap();
                assert_eq!(
                    ui.map_pointer(physical.0, physical.1, scale as f64),
                    Some((20, 10))
                );
                ui.crop = original_crop;
                let original_rect = ui.guest_rect;
                ui.guest_rect = Some(egui::Rect::from_min_size(
                    egui::pos2(10., 20.),
                    egui::vec2(640., 480.),
                ));
                for point in [(20, 10), (42, 32), (259, 329), (499, 649)] {
                    let physical = ui
                        .guest_to_physical(point.0, point.1, scale as f64)
                        .unwrap();
                    assert_eq!(
                        ui.map_pointer(physical.0.round(), physical.1.round(), scale as f64),
                        Some(point),
                        "native pixel quantization must preserve 1:1 guest warps"
                    );
                }
                ui.guest_rect = original_rect;
                ui.guest_focused = false;
                let pixels = readback(&device, &queue, &target, size);
                let offset = (((center.y * scale) as u32 * size[0] + (center.x * scale) as u32) * 4)
                    as usize;
                for (actual, expected) in pixels[offset..offset + 3].iter().zip([64i16, 128, 191]) {
                    assert!(
                        (*actual as i16 - expected).abs() <= 2,
                        "guest color changed: {:?}",
                        &pixels[offset..offset + 4]
                    );
                }
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let path =
                        std::path::PathBuf::from(directory).join(format!("workspace-{scale}x.png"));
                    image::save_buffer(path, &pixels, size[0], size[1], image::ColorType::Rgba8)
                        .unwrap();
                }
                // A hidden guest tab must stop accepting new input and OS
                // warps, without losing releases for gestures it already owns.
                let saved_dock =
                    systemless_ui::workspace::save_with_layers(&ui.dock, &ui.context, |pane| {
                        ui.pane_layers.get(pane).copied()
                    })
                    .unwrap();
                ui.dock = DockState::new(vec![Pane::Guest, Pane::Inspector]);
                ui.dock.main_surface_mut()[systemless_ui::egui_dock::NodeIndex::root()]
                    .get_leaf_mut()
                    .unwrap()
                    .active = systemless_ui::egui_dock::TabIndex(1);
                ui.guest_focused = true;
                ui.guest_drag = true;
                let held = winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA);
                ui.keys.insert(held);
                let output = ui.build_frame(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    ..test_input()
                });
                ui.paint(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    output,
                );
                assert!(ui.guest_rect.is_none());
                assert!(ui.guest_layer.is_none());
                assert!(!ui.guest_focused);
                assert!(!ui.guest_input_available());
                assert!(ui.guest_to_physical(259, 329, scale as f64).is_none());
                assert!(ui.guest_drag && ui.keys.contains(&held));
                let (mouse, keys) = ui.release_input();
                assert!(mouse);
                assert_eq!(keys, vec![held]);
                ui.dock = DockState::new(vec![Pane::Guest]);
                ui.dock.main_surface_mut()[systemless_ui::egui_dock::NodeIndex::root()]
                    .set_collapsed(true);
                ui.guest_focused = true;
                let output = ui.build_frame(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    ..test_input()
                });
                ui.paint(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    output,
                );
                assert!(
                    ui.guest_rect.is_none(),
                    "collapsed guest must not expose an input rectangle"
                );
                assert!(!ui.guest_focused && !ui.guest_input_available());
                ui.dock = systemless_ui::workspace::restore(
                    &saved_dock,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.)),
                )
                .unwrap();
                for _ in 0..3 {
                    let output = ui.build_frame(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..test_input()
                    });
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                assert!(ui.guest_input_available());
                assert!(
                    !ui.guest_focused,
                    "showing a pane must not steal keyboard focus"
                );
                ui.guest_focused = true;
                ui.ui_pointer_buttons.insert(MouseButton::Left);
                assert!(
                    ui.guest_to_physical(259, 329, scale as f64).is_none(),
                    "UI gestures must not be interrupted by guest warps"
                );
                ui.ui_pointer_buttons.clear();
                assert!(ui.guest_to_physical(259, 329, scale as f64).is_some());
                ui.guest_focused = false;
                ui.preferences = true;
                for _ in 0..3 {
                    let output = ui.build_frame(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..test_input()
                    });
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                ui.pointer = center;
                assert!(
                    !ui.pointer_over_guest(),
                    "modal must cover guest input layer"
                );
                ui.preferences = false;
                ui.set_modality(UiModality {
                    owner: UiOwner::FileDialog,
                    menus_blocked: true,
                });
                ui.set_file_dialog(Some(std::sync::Arc::new(FileDialogSnapshot {
                    id: 42,
                    revision: 0,
                    kind: systemless::systems::macintosh::host_ui::FileDialogKind::Open,
                    prompt: "Open a file".into(),
                    directory: "Pilots".into(),
                    entries: vec![systemless::systems::macintosh::host_ui::FileDialogEntry {
                        name: "Rick Hardslab".into(),
                        directory: false,
                    }],
                    name: String::new(),
                    writable: true,
                    busy: false,
                })));
                for _ in 0..3 {
                    let output = ui.build_frame(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..test_input()
                    });
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                assert!(!ui.pointer_over_guest());
                let dialog_pixels = readback(&device, &queue, &target, size);
                let corner = (((10. * scale) as u32 * size[0] + size[0] - (20. * scale) as u32) * 4)
                    as usize;
                assert_ne!(
                    &dialog_pixels[corner..corner + 3],
                    &[0, 0, 0],
                    "modal frames must retain the viewport DPI and cover the target"
                );
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("file-dialog-{scale}x.png")),
                        &dialog_pixels,
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                let output = ui.build_frame(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..test_input()
                });
                ui.paint(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    output,
                );
                assert_eq!(
                    ui.file_actions,
                    vec![FileDialogReply {
                        id: 42,
                        revision: 0,
                        action: FileDialogAction::Cancel
                    }]
                );
                assert!(ui.file_pending);
                ui.set_file_reply(Some((1, 42, 0, false)));
                assert!(!ui.file_pending, "a rejected reply must allow retry");
                let file_parent = ui.file_dialog.clone().unwrap();
                ui.set_file_dialog(None);
                ui.error = None;
                ui.set_modality(UiModality {
                    owner: UiOwner::Dialog,
                    menus_blocked: true,
                });
                ui.set_dialog(
                    Some(std::sync::Arc::new(
                        systemless::systems::macintosh::host_ui::DialogSnapshot {
                            movable: true,
                            closable: false,
                            id: 43,
                            revision: 1,
                            title: "Preferences".into(),
                            size: (300, 120),
                            edit_item: 0,
                            default_item: 1,
                            cancel_item: 2,
                            busy: false,
                            items: vec![
                                systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                                    range: None,
                                    choices: Vec::new(),
                                    artwork: None,
                                    number: 1,
                                    kind: 4,
                                    enabled: true,
                                    rect: (80, 200, 100, 280),
                                    text: "Save".into(),
                                    selection: None,
                                    value: 0,
                                },
                                systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                                    range: None,
                                    choices: Vec::new(),
                                    artwork: None,
                                    number: 2,
                                    kind: 4,
                                    enabled: true,
                                    rect: (80, 100, 100, 180),
                                    text: "Cancel".into(),
                                    selection: None,
                                    value: 0,
                                },
                                systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                                    range: None,
                                    choices: Vec::new(),
                                    artwork: None,
                                    number: 3,
                                    kind: 8,
                                    enabled: true,
                                    rect: (10, 10, 60, 280),
                                    text: "Your changes will be saved in the guest application."
                                        .into(),
                                    selection: None,
                                    value: 0,
                                },
                                systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                                    range: None,
                                    choices: Vec::new(),
                                    artwork: None,
                                    number: 4,
                                    kind: 5,
                                    enabled: true,
                                    rect: (40, 10, 60, 280),
                                    text: "Sound Effects".into(),
                                    selection: None,
                                    value: 1,
                                },
                                systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                                    range: None,
                                    choices: Vec::new(),
                                    artwork: None,
                                    number: 5,
                                    kind: 6,
                                    enabled: true,
                                    rect: (65, 10, 85, 280),
                                    text: "High Quality".into(),
                                    selection: None,
                                    value: 1,
                                },
                            ],
                        },
                    )),
                    &[43],
                );
                for _ in 0..3 {
                    let mut input = test_input();
                    input.screen_rect = Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    ));
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                assert!(!ui.pointer_over_guest());
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("message-dialog-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                let mut input = test_input();
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                ));
                input.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                });
                let output = ui.build_frame(input);
                ui.paint(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    output,
                );
                assert_eq!(
                    ui.dialog_actions,
                    vec![systemless::systems::macintosh::host_ui::DialogReply {
                        id: 43,
                        revision: 1,
                        item: 1
                    }]
                );
                ui.dialog_actions.clear();
                let mut editable = (**ui.dialog.as_ref().unwrap()).clone();
                editable.id = 44;
                editable.edit_item = 6;
                editable.items.push(
                    systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                        range: None,
                        choices: Vec::new(),
                        artwork: None,
                        number: 6,
                        kind: 16,
                        enabled: true,
                        rect: (90, 90, 110, 280),
                        text: "Guest".into(),
                        selection: Some((0, 5)),
                        value: 0,
                    },
                );
                editable.items.push(
                    systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                        range: None,
                        choices: Vec::new(),
                        artwork: None,
                        number: 7,
                        kind: 8,
                        enabled: true,
                        rect: (90, 10, 110, 80),
                        text: "Player name".into(),
                        selection: None,
                        value: 0,
                    },
                );
                editable.items.push(
                    systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                        range: None,
                        choices: Vec::new(),
                        artwork: Some(std::sync::Arc::new(
                            systemless::systems::macintosh::host_ui::DialogArtwork {
                                width: 2,
                                height: 1,
                                rgba: std::sync::Arc::from([255, 0, 0, 255, 0, 0, 0, 0]),
                                monochrome: false,
                            },
                        )),
                        number: 8,
                        kind: 32,
                        enabled: false,
                        rect: (0, 10, 32, 74),
                        text: String::new(),
                        selection: None,
                        value: 0,
                    },
                );
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                let image_texture = ui.artwork_textures[&8].texture.id();
                let mut draw = |ui: &mut WorkspaceUi, events| {
                    let mut input = test_input();
                    input.screen_rect = Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    ));
                    input.events = events;
                    let output = ui.build_frame(input);
                    fn collect_labels(shape: &egui::Shape, labels: &mut Vec<String>) {
                        match shape {
                            egui::Shape::Text(text) => labels.push(text.galley.job.text.clone()),
                            egui::Shape::Vec(shapes) => {
                                for shape in shapes {
                                    collect_labels(shape, labels);
                                }
                            }
                            _ => {}
                        }
                    }
                    let mut labels = Vec::new();
                    for shape in &output.shapes {
                        collect_labels(&shape.shape, &mut labels);
                    }
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                    labels
                };
                draw(&mut ui, Vec::new());
                if let Some(initial) = ui.dialog_text_actions.pop() {
                    accept_text(&mut ui, &mut editable, initial);
                }
                draw(
                    &mut ui,
                    vec![
                        egui::Event::Text("Renée".into()),
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        },
                    ],
                );
                assert!(
                    ui.dialog_actions.is_empty(),
                    "Save waits for the text acknowledgement"
                );
                let text = ui
                    .dialog_text_actions
                    .pop()
                    .expect("text edit is sent to the owner");
                assert_eq!(text.text, "Renée");
                accept_text(&mut ui, &mut editable, text);
                draw(&mut ui, Vec::new());
                assert_eq!(
                    ui.dialog_actions,
                    vec![systemless::systems::macintosh::host_ui::DialogReply {
                        id: editable.id,
                        revision: editable.revision,
                        item: 1
                    }]
                );
                draw(&mut ui, Vec::new());
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("text-dialog-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                assert_eq!(
                    ui.artwork_textures[&8].texture.id(),
                    image_texture,
                    "text edits do not replace artwork textures"
                );
                editable.revision += 1;
                editable
                    .items
                    .iter_mut()
                    .find(|i| i.number == 8)
                    .unwrap()
                    .artwork = Some(std::sync::Arc::new(
                    systemless::systems::macintosh::host_ui::DialogArtwork {
                        width: 2,
                        height: 1,
                        rgba: std::sync::Arc::from([0, 255, 0, 255, 0, 0, 0, 0]),
                        monochrome: false,
                    },
                ));
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                assert_eq!(
                    ui.artwork_textures[&8].texture.id(),
                    image_texture,
                    "changed pixels update the existing texture"
                );
                let pixels = readback(&device, &queue, &target, size);
                assert!(
                    pixels
                        .chunks_exact(4)
                        .filter(|p| *p == [0, 255, 0, 255])
                        .count()
                        >= 256 * scale as usize * scale as usize,
                    "owned artwork reaches the GPU at both DPI scales"
                );
                let mut previous_pixels = pixels;
                for icon in 0..=2 {
                    editable.revision += 1;
                    editable
                        .items
                        .iter_mut()
                        .find(|item| item.number == 8)
                        .unwrap()
                        .kind = if icon == 2 { 64 } else { 32 };
                    editable
                        .items
                        .iter_mut()
                        .find(|item| item.number == 8)
                        .unwrap()
                        .artwork = Some(std::sync::Arc::new(
                        systemless::systems::macintosh::host_ui::DialogArtwork::system_alert(icon)
                            .unwrap(),
                    ));
                    ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                    draw(&mut ui, Vec::new());
                    assert_eq!(ui.artwork_textures[&8].texture.id(), image_texture);
                    assert_eq!(ui.artwork_textures[&8].texture.size(), [128, 128]);
                    let pixels = readback(&device, &queue, &target, size);
                    assert!(
                        pixels != previous_pixels,
                        "each built-in alert symbol reaches the GPU"
                    );
                    previous_pixels = pixels;
                }
                editable.items.push(
                    systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                        range: None,
                        choices: vec![
                            systemless::systems::macintosh::host_ui::DialogChoice {
                                value: 1,
                                text: "Low".into(),
                                enabled: true,
                            },
                            systemless::systems::macintosh::host_ui::DialogChoice {
                                value: 4,
                                text: "High".into(),
                                enabled: true,
                            },
                        ],
                        number: 9,
                        kind: 7,
                        enabled: true,
                        rect: (120, 10, 140, 200),
                        text: "Quality".into(),
                        value: 1,
                        artwork: None,
                        selection: None,
                    },
                );
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                let labels = draw(&mut ui, Vec::new());
                assert!(labels.iter().any(|label| label == "Quality"));
                assert!(labels.iter().any(|label| label == "Low"));
                let original = editable
                    .items
                    .iter()
                    .find(|item| item.number == 6)
                    .unwrap()
                    .text
                    .clone();
                ui.dialog_editors.active.fields[0].editor.text = "Popup draft".into();
                ui.dialog_editors.active.edited(6);
                ui.pending_controls.insert(editable.id, (9, 1, 4));
                ui.dialog_editors.active.action = Some(9);
                draw(&mut ui, Vec::new());
                assert!(
                    ui.dialog_control_actions.is_empty(),
                    "popup waits for text acknowledgement"
                );
                let reply = ui.dialog_text_actions.pop().unwrap();
                assert_eq!(reply.text, "Popup draft");
                accept_text(&mut ui, &mut editable, reply);
                draw(&mut ui, Vec::new());
                let reply = ui.dialog_control_actions.pop().unwrap();
                assert_eq!((reply.dialog.item, reply.expected, reply.value), (9, 1, 4));
                assert_eq!(reply.dialog.revision, editable.revision);
                let editor = editable
                    .items
                    .iter_mut()
                    .find(|item| item.number == 6)
                    .unwrap();
                editor.text = original;
                editor.selection = Some((5, 5));
                editable
                    .items
                    .iter_mut()
                    .find(|item| item.number == 9)
                    .unwrap()
                    .value = 4;
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                let labels = draw(&mut ui, Vec::new());
                assert!(labels.iter().any(|label| label == "High"));
                editable.items.push(
                    systemless::systems::macintosh::host_ui::DialogItemSnapshot {
                        range: Some(systemless::systems::macintosh::host_ui::DialogRange {
                            min: -100,
                            max: 100,
                        }),
                        choices: Vec::new(),
                        artwork: None,
                        number: 10,
                        kind: 17,
                        enabled: true,
                        rect: (150, 10, 166, 170),
                        text: "Level".into(),
                        value: 0,
                        selection: None,
                    },
                );
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                assert!(draw(&mut ui, Vec::new())
                    .iter()
                    .any(|label| label == "Level"));
                ui.pending_controls.insert(editable.id, (10, 0, 75));
                ui.dialog_editors.active.focus_control(10);
                ui.dialog_editors.active.action = Some(10);
                draw(&mut ui, Vec::new());
                let reply = ui.dialog_control_actions.pop().unwrap();
                assert_eq!(
                    (reply.dialog.item, reply.expected, reply.value),
                    (10, 0, 75)
                );
                editable
                    .items
                    .iter_mut()
                    .find(|item| item.number == 10)
                    .unwrap()
                    .value = 75;
                editable.busy = true;
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                editable.busy = false;
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                assert!(draw(&mut ui, Vec::new()).iter().any(|label| label == "75"));
                draw(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::ArrowLeft,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                let reply = ui.dialog_control_actions.pop().unwrap();
                assert_eq!((reply.dialog.item, reply.expected), (10, 75));
                assert!(reply.value < 75);
                editable
                    .items
                    .iter_mut()
                    .find(|item| item.number == 10)
                    .unwrap()
                    .value = reply.value;
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                ui.dialog_editors.active.focus_control(6);
                ui.dialog_editors.active.restore_focus();
                draw(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::ArrowLeft,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                if let Some(focus) = ui.dialog_text_actions.pop() {
                    assert_eq!(focus.text, "Renée");
                    accept_text(&mut ui, &mut editable, focus);
                    draw(&mut ui, Vec::new());
                }
                editable.title = "Updated preferences".into();
                editable.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                let labels = draw(&mut ui, Vec::new());
                assert!(labels.iter().any(|label| label == "Updated preferences"));
                assert!(!labels.iter().any(|label| label == "Preferences"));
                assert_eq!(ui.dialog_editors.active.fields[0].editor.text, "Renée");
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(std::path::PathBuf::from(directory).join(format!("range-dialog-{scale}x.png")),
                        &readback(&device, &queue, &target, size), size[0], size[1], image::ColorType::Rgba8).unwrap();
                }

                let mut handling = editable.clone();
                handling.busy = true;
                handling.revision += 1;
                ui.set_dialog(Some(std::sync::Arc::new(handling)), &[editable.id]);
                ui.dialog_actions.clear();
                ui.dialog_text_actions.clear();
                draw(&mut ui, Vec::new());
                let labels = draw(
                    &mut ui,
                    vec![
                        egui::Event::Text("ignored while handling".into()),
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        },
                    ],
                );
                assert!(labels.iter().any(|label| label == "Player name"));
                assert!(labels.iter().any(|label| label == "Renée"));
                assert!(ui.dialog_actions.is_empty());
                assert!(ui.dialog_text_actions.is_empty());
                assert!(ui.guest_to_physical(10, 10, scale as f64).is_none());
                editable.revision += 2;
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                if let Some(focus) = ui.dialog_text_actions.pop() {
                    assert_eq!(focus.text, "Renée");
                    accept_text(&mut ui, &mut editable, focus);
                }
                draw(&mut ui, vec![egui::Event::Text(" Junior".into())]);
                let parent_edit = ui.dialog_text_actions.pop().unwrap();
                assert_eq!(parent_edit.text, "Renée Junior");
                let mut child = editable.clone();
                child.id = editable.id + 1;
                child.title = "Nested confirmation".into();
                child.edit_item = 0;
                child.items.retain(|item| item.kind == 4);
                ui.set_dialog(
                    Some(std::sync::Arc::new(child.clone())),
                    &[editable.id, child.id],
                );
                draw(&mut ui, Vec::new());
                let labels = draw(&mut ui, Vec::new());
                assert!(labels.iter().any(|label| label == "Nested confirmation"));
                assert!(!labels.iter().any(|label| label == "Player name"));
                // The owner's parent acknowledgement arrives while only the
                // child is projected. It must not release or reset that child.
                ui.set_dialog_text_reply(Some(&super::super::runtime_protocol::DialogTextAck {
                    serial: parent_edit.serial,
                    id: editable.id,
                    accepted: true,
                    text: Some(parent_edit.text.clone()),
                }));
                editable.revision += 1;
                let field = editable
                    .items
                    .iter_mut()
                    .find(|item| item.number == 6)
                    .unwrap();
                field.text = parent_edit.text;
                field.selection = Some((12, 12));
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                if let Some(focus) = ui.dialog_text_actions.pop() {
                    assert_eq!(focus.text, "Renée Junior");
                    accept_text(&mut ui, &mut editable, focus);
                }
                draw(&mut ui, vec![egui::Event::Text("!".into())]);
                let resumed = ui
                    .dialog_text_actions
                    .pop()
                    .expect("resumed parent restores keyboard focus");
                assert_eq!(resumed.text, "Renée Junior!");
                assert_eq!(resumed.expected, "Renée Junior");
                assert_eq!(resumed.id, editable.id);
                accept_text(&mut ui, &mut editable, resumed);
                ui.set_modality(UiModality {
                    owner: UiOwner::Guest,
                    menus_blocked: true,
                });
                ui.set_dialog(None, &[editable.id]);
                assert!(ui.dialog_surfaces.contains_key(&editable.id));
                draw(&mut ui, Vec::new());
                let labels = draw(&mut ui, Vec::new());
                assert!(!labels.iter().any(|label| label == "Player name"));
                editable.revision += 1;
                ui.set_modality(UiModality {
                    owner: UiOwner::Dialog,
                    menus_blocked: true,
                });
                ui.set_dialog(Some(std::sync::Arc::new(editable.clone())), &[editable.id]);
                draw(&mut ui, Vec::new());
                if let Some(focus) = ui.dialog_text_actions.pop() {
                    accept_text(&mut ui, &mut editable, focus);
                }
                draw(&mut ui, vec![egui::Event::Text("?".into())]);
                let shown = ui
                    .dialog_text_actions
                    .pop()
                    .expect("shown editor restores focus and selection");
                assert_eq!(shown.id, editable.id);
                assert_eq!(shown.text, "Renée Junior!?");
                assert_eq!(shown.expected, "Renée Junior!");
                ui.dialog_actions.clear();
                ui.file_actions.clear();
                let mut parent = (*file_parent).clone();
                parent.busy = true;
                ui.set_file_dialog(Some(std::sync::Arc::new(parent.clone())));
                let labels = draw(&mut ui, Vec::new());
                assert_eq!(ui.dialog_editors.active.fields.len(), 1);
                assert!(
                    labels.iter().any(|s| s == "Player name"),
                    "visible dialog labels: {labels:?}"
                );
                assert!(
                    !labels.iter().any(|s| s.contains("preparing this folder")),
                    "a child dialog must not be covered by the busy parent picker"
                );
                ui.set_modality(UiModality {
                    owner: UiOwner::Guest,
                    menus_blocked: true,
                });
                draw(&mut ui, Vec::new());
                let labels = draw(&mut ui, Vec::new());
                assert!(!labels
                    .iter()
                    .any(|s| s == "Player name" || s.contains("preparing this folder")));
                assert!(ui.guest_to_physical(10, 10, scale as f64).is_some(), "a guest-rendered child retains guest input despite the retained parent snapshots");
                assert!(ui.dialog_actions.is_empty());
                assert!(ui.file_actions.is_empty());
                parent.busy = false;
                ui.set_file_dialog(Some(std::sync::Arc::new(parent)));
                ui.set_modality(UiModality {
                    owner: UiOwner::FileDialog,
                    menus_blocked: true,
                });
                draw(&mut ui, Vec::new());
                draw(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                assert!(ui.dialog_actions.is_empty());
                assert_eq!(ui.file_actions.last().unwrap().id, file_parent.id);
                ui.set_dialog(None, &[]);
                assert!(ui.dialog_surfaces.is_empty());
                ui.set_file_dialog(None);
                ui.set_modality(UiModality::default());
                use egui_winit::accesskit_winit::WindowEvent as AccessibilityEvent;
                assert!(ui.accessibility_event(AccessibilityEvent::InitialTreeRequested));
                let mut output = ui.build_frame(test_input());
                output.textures_delta.clear();
                assert!(output.platform_output.accesskit_update.is_some());
                // GPU recovery uses this same context reset while retaining
                // the window's native adapter and its activation state.
                ui.guest_focused = true;
                ui.reset_egui_context();
                assert!(ui.guest_rect.is_none() && ui.guest_layer.is_none());
                assert!(ui.guest_focused && !ui.guest_input_available());
                let mut output = ui.build_frame(test_input());
                output.textures_delta.clear();
                assert!(output.platform_output.accesskit_update.is_some());
                assert!(!ui.accessibility_event(AccessibilityEvent::AccessibilityDeactivated));
                let mut output = ui.build_frame(test_input());
                output.textures_delta.clear();
                assert!(output.platform_output.accesskit_update.is_none());
                let mut floating = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                floating.theme = Theme::new(appearance);
                floating.theme.apply(&floating.context);
                let guest = floating.target(&device, (640, 480));
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &guest,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.25,
                                    g: 0.5,
                                    b: 0.75,
                                    a: 1.,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                queue.submit([encoder.finish()]);
                floating.dock = DockState::new(vec![]);
                for (pane, position, size) in [
                    (Pane::Guest, egui::pos2(100., 140.), egui::vec2(550., 420.)),
                    (
                        Pane::Inspector,
                        egui::pos2(500., 80.),
                        egui::vec2(300., 320.),
                    ),
                ] {
                    let surface = floating.dock.add_window(vec![pane]);
                    floating
                        .dock
                        .get_window_state_mut(surface)
                        .unwrap()
                        .set_position(position)
                        .set_size(size);
                }
                let mut floating_input = || {
                    let mut input = test_input();
                    input.screen_rect = Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    ));
                    input
                };
                for _ in 0..3 {
                    let output = floating.build_frame(floating_input());
                    floating.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                let inspector_layer = floating.pane_layers[&Pane::Inspector];
                let inspector = floating
                    .context
                    .memory(|memory| memory.area_rect(inspector_layer.id).unwrap());
                let overlap = floating.guest_rect.unwrap().intersect(inspector);
                assert!(overlap.is_positive());
                floating.pointer = overlap.center();
                assert!(
                    !floating.pointer_over_guest(),
                    "an overlapping inspector must block guest pointer input"
                );
                for _ in 0..2 {
                    let output = floating.build_frame(floating_input());
                    floating
                        .context
                        .move_to_top(floating.pane_layers[&Pane::Guest]);
                    floating.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                assert!(
                    floating.pointer_over_guest(),
                    "raising the guest must restore pointer ownership"
                );
                // Render once more after egui applies the pending layer order.
                let output = floating.build_frame(floating_input());
                floating.paint(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    output,
                );
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        directory.join(format!("floating-workspace-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }

            }
        });
    }
    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn nonmodal_windows_gpu_render_at_both_dpi_scales() {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut presenter = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                presenter.theme = Theme::new(appearance);
                presenter.theme.apply(&presenter.context);
                let size = [(800. * scale) as u32, (600. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("nonmodal-capture"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut states = [
                    systemless_ui::window::WindowState::default(),
                    Default::default(),
                ];
                for frame in 0..3 {
                    let mut input = egui::RawInput {
                        time: Some(frame as f64 * 0.2),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 600.),
                        )),
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = presenter.context.run_ui(input, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.heading("Systemless UI");
                            ui.label("Independent nonmodal surfaces");
                        });
                        for (index, state) in states.iter_mut().enumerate() {
                            let response = systemless_ui::window::show(
                                ui.ctx(), egui::Id::new(("capture", index)),
                                ["Session preferences", "Display preferences"][index], state,
                                systemless_ui::window::WindowOptions {
                                    initial_rect: egui::Rect::from_min_size(
                                        egui::pos2(40. + index as f32 * 270., 110. + index as f32 * 140.),
                                        egui::vec2(400., 280.)),
                                    enabled: true, resizable: true, closable: true,
                                },
                                |ui| {
                                    use systemless_ui::dialog::{Button, Field, FieldKind, Form};
                                    systemless_ui::dialog::show_with_shortcuts(ui, &presenter.theme, Form {
                                        title: "",
                                        fields: &[
                                            Field { id: 3, text: "A reusable window with independent geometry and application-owned actions.", enabled: true, kind: FieldKind::Text },
                                            Field { id: 4, text: "Retain window layout", enabled: true, kind: FieldKind::Checkbox(true) },
                                        ],
                                        buttons: &[Button { id: 1, text: "Apply", enabled: true }, Button { id: 2, text: "Cancel", enabled: true }],
                                        default: 1, cancel: 2,
                                    }, &mut [], index == 1)
                                },
                            );
                            assert!(!response.activated && !response.close_requested);
                        }
                    });
                    presenter.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                }
                let pixels = readback(&device, &queue, &target, size);
                match appearance {
                    Appearance::Dark => assert!(pixels[0] < 80),
                    Appearance::Light => assert!(pixels[0] > 200),
                }
                assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
                assert!(
                    pixels
                        .chunks_exact(4)
                        .filter(|pixel| pixel[0].abs_diff(pixel[2]) > 12)
                        .count()
                        > 100
                );
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        directory.join(format!("nonmodal-windows-{scale}x.png")),
                        &pixels,
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        });
    }

    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn workspace_modeless_dialogs_publish_intents_at_both_dpi_scales() {
        use super::super::runtime_protocol::{DialogTextAck, ModelessCommand};
        use systemless::systems::macintosh::host_ui::{
            DialogItemSnapshot, DialogReply, DialogSnapshot,
        };
        fn item(
            number: i16,
            kind: u8,
            rect: (i16, i16, i16, i16),
            text: &str,
        ) -> DialogItemSnapshot {
            DialogItemSnapshot {
                range: None,
                choices: Vec::new(),
                artwork: None,
                number,
                kind,
                enabled: true,
                rect,
                text: text.into(),
                value: 0,
                selection: (kind == 16).then_some((0, 0)),
            }
        }
        fn dialog(id: u64, title: &str, edit: bool) -> DialogSnapshot {
            let mut items = vec![
                item(1, 4, (90, 200, 110, 270), "Find"),
                item(2, 4, (90, 120, 110, 190), "Cancel"),
                item(3, 5, (60, 20, 76, 200), "Match case"),
            ];
            if edit {
                items.push(item(4, 8, (20, 10, 36, 60), "Find:"));
                items.push(item(5, 16, (20, 70, 36, 270), ""));
            }
            DialogSnapshot {
                movable: true,
                closable: edit,
                id,
                revision: 1,
                title: title.into(),
                size: (280, 120),
                items,
                edit_item: if edit { 5 } else { 0 },
                default_item: 1,
                cancel_item: 2,
                busy: false,
            }
        }
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut ui = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                ui.theme = Theme::new(appearance);
                ui.theme.apply(&ui.context);
                let size = [(1000. * scale) as u32, (700. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("modeless-capture"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut time = 0.;
                let mut frame = |ui: &mut WorkspaceUi, events: Vec<egui::Event>| {
                    time += 0.2;
                    let mut input = egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        events,
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                    std::mem::take(&mut ui.modeless_actions)
                };
                let mut front = dialog(7, "Find", true);
                let back = dialog(8, "Replace", false);
                let live = [7, 8];
                ui.set_modeless_dialogs(
                    &[
                        std::sync::Arc::new(front.clone()),
                        std::sync::Arc::new(back.clone()),
                    ],
                    &live,
                );
                // Focusing the initial editor synchronizes the guest's active field.
                for _ in 0..3 {
                    for action in frame(&mut ui, Vec::new()) {
                        let ModelessCommand::Text(text) = action else {
                            panic!("unexpected {action:?}");
                        };
                        assert_eq!((text.id, text.item, text.text.as_str()), (7, 5, ""));
                        ui.set_dialog_text_reply(Some(&DialogTextAck {
                            serial: text.serial,
                            id: 7,
                            accepted: true,
                            text: Some(String::new()),
                        }));
                    }
                }
                let rect = |ui: &WorkspaceUi, id: u64| {
                    ui.context
                        .memory(|m| m.area_rect(egui::Id::new(("macintosh-modeless", id))))
                        .unwrap()
                };
                let (front_rect, back_rect) = (rect(&ui, 7), rect(&ui, 8));
                assert!(front_rect.intersects(back_rect));
                let layer = |id: u64| {
                    egui::LayerId::new(
                        egui::Order::Middle,
                        egui::Id::new(("macintosh-modeless", id)),
                    )
                };
                assert_eq!(
                    ui.context
                        .layer_id_at(front_rect.intersect(back_rect).center()),
                    Some(layer(7))
                );

                // Typing edits the focused front editor; the draft flushes as text.
                let actions = frame(&mut ui, vec![egui::Event::Text("needle".into())]);
                let ModelessCommand::Text(text) = &actions[..].first().unwrap().clone() else {
                    panic!("expected a text commit, got {actions:?}");
                };
                assert_eq!((text.id, text.item, text.text.as_str()), (7, 5, "needle"));
                // Return waits for the acknowledgement and then uses the new revision.
                let actions = frame(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                assert!(actions.is_empty(), "{actions:?}");
                front.revision = 2;
                front.items[4].text = "needle".into();
                front.items[4].selection = Some((6, 6));
                ui.set_modeless_dialogs(
                    &[
                        std::sync::Arc::new(front.clone()),
                        std::sync::Arc::new(back.clone()),
                    ],
                    &live,
                );
                ui.set_dialog_text_reply(Some(&DialogTextAck {
                    serial: text.serial,
                    id: 7,
                    accepted: true,
                    text: Some("needle".into()),
                }));
                assert_eq!(
                    frame(&mut ui, Vec::new()),
                    vec![ModelessCommand::Item(DialogReply {
                        id: 7,
                        revision: 2,
                        item: 1
                    })]
                );

                // A press on the obscured window activates it before any action.
                let exposed = egui::pos2(back_rect.right() - 6., back_rect.bottom() - 6.);
                assert!(!front_rect.contains(exposed));
                let press = |pressed| egui::Event::PointerButton {
                    pos: exposed,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                let actions = frame(
                    &mut ui,
                    vec![egui::Event::PointerMoved(exposed), press(true)],
                );
                assert_eq!(
                    actions,
                    vec![ModelessCommand::Activate { id: 8, revision: 1 }]
                );
                frame(&mut ui, vec![press(false)]);
                // Host stacking alone does not fabricate guest activation or shortcuts.
                assert!(frame(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }]
                )
                .iter()
                .all(|a| !matches!(a, ModelessCommand::Item(r) if r.id == 8)));

                // Guest activation raises the window it selected.
                ui.set_modeless_dialogs(
                    &[
                        std::sync::Arc::new(back.clone()),
                        std::sync::Arc::new(front.clone()),
                    ],
                    &live,
                );
                frame(&mut ui, Vec::new());
                frame(&mut ui, Vec::new());
                assert_eq!(ui.context.layer_id_at(exposed), Some(layer(8)));
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        directory.join(format!("modeless-workspace-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }

                // Close is an intent, emitted only once drafts are synchronized.
                ui.modeless_sessions.get_mut(&7).unwrap().editors.fields[0]
                    .editor
                    .text = "late".into();
                ui.modeless_sessions.get_mut(&7).unwrap().editors.edited(5);
                ui.modeless_sessions.get_mut(&7).unwrap().close = true;
                let actions = frame(&mut ui, Vec::new());
                assert!(
                    matches!(&actions[..], [ModelessCommand::Text(t)] if t.text == "late"),
                    "{actions:?}"
                );
                let serial = match &actions[0] {
                    ModelessCommand::Text(t) => t.serial,
                    _ => unreachable!(),
                };
                front.revision = 3;
                front.items[4].text = "late".into();
                front.items[4].selection = Some((4, 4));
                ui.set_modeless_dialogs(
                    &[
                        std::sync::Arc::new(back.clone()),
                        std::sync::Arc::new(front.clone()),
                    ],
                    &live,
                );
                ui.set_dialog_text_reply(Some(&DialogTextAck {
                    serial,
                    id: 7,
                    accepted: true,
                    text: Some("late".into()),
                }));
                assert_eq!(
                    frame(&mut ui, Vec::new()),
                    vec![ModelessCommand::Close { id: 7, revision: 3 }]
                );

                // Host modality disables every modeless surface.
                ui.set_modality(UiModality {
                    owner: UiOwner::Dialog,
                    menus_blocked: true,
                });
                assert!(frame(
                    &mut ui,
                    vec![egui::Event::PointerMoved(exposed), press(true)]
                )
                .is_empty());
                frame(&mut ui, vec![press(false)]);
                ui.set_modality(UiModality::default());

                // Geometry survives context recreation; retirement drops sessions.
                let moved = rect(&ui, 8);
                ui.reset_egui_context();
                ui.set_modeless_dialogs(&[], &live);
                ui.set_modeless_dialogs(
                    &[
                        std::sync::Arc::new(back.clone()),
                        std::sync::Arc::new(front.clone()),
                    ],
                    &live,
                );
                frame(&mut ui, Vec::new());
                frame(&mut ui, Vec::new());
                assert!((rect(&ui, 8).min - moved.min).length() < 1.);
                ui.set_modeless_dialogs(&[], &[]);
                assert!(ui.modeless_sessions.is_empty());
            }
        });
    }

    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn workspace_popup_menu_replies_at_both_dpi_scales() {
        use systemless::systems::macintosh::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};
        use systemless::systems::macintosh::host_ui::{PopupMenuReply, PopupMenuSnapshot};
        let item = |number: i16, text: &str, enabled: bool| GuestMenuItem {
            number,
            text: text.into(),
            enabled,
            checked: number == 2,
            key_equivalent: None,
            submenu_id: None,
            separator: text == "-",
        };
        let menus = std::sync::Arc::new(GuestMenuSnapshot {
            menus: vec![GuestMenu {
                id: 143,
                title: "Loadout".into(),
                enabled: true,
                hierarchical: true,
                visible_in_menu_bar: false,
                items: vec![
                    item(1, "Scout Kit", true),
                    item(2, "Veteran Kit", true),
                    item(3, "-", false),
                    item(4, "Long-range Expedition Loadout", true),
                    item(5, "Locked Prototype", false),
                ],
            }],
        });
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut ui = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                ui.theme = Theme::new(appearance);
                ui.theme.apply(&ui.context);
                ui.crop = (0, 0, 640, 480);
                let _guest = ui.target(&device, (640, 480));
                let size = [(1000. * scale) as u32, (700. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("popup-capture"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut time = 0.;
                let mut frame = |ui: &mut WorkspaceUi, events: Vec<egui::Event>| {
                    time += 0.2;
                    let mut input = egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        events,
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                };
                ui.menus = Some(menus.clone());
                ui.set_modality(UiModality {
                    owner: UiOwner::PopupMenu,
                    menus_blocked: true,
                });
                let popup = PopupMenuSnapshot {
                    id: 9,
                    menu: 143,
                    item: 2,
                    anchor: (150, 290),
                };
                ui.set_popup_menu(Some(popup.clone()));
                for _ in 0..3 {
                    frame(&mut ui, Vec::new());
                }
                assert!(ui.popup_action.is_none());
                let area = ui
                    .context
                    .memory(|m| m.area_rect(egui::Id::new(("guest-popup-menu", 9u64))))
                    .unwrap();
                // The current item is placed over the guest anchor.
                let anchor = ui.guest_to_points(150, 290).unwrap();
                assert!(
                    area.contains(anchor + egui::vec2(4., 4.)),
                    "{area:?} {anchor:?}"
                );
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        directory.join(format!("popup-menu-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                // Choosing a row replies with that item.
                let row = area.top() + (area.height() / 5.) * 3.5;
                let pos = egui::pos2(area.center().x, row);
                let click = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                frame(&mut ui, vec![egui::Event::PointerMoved(pos), click(true)]);
                frame(&mut ui, vec![click(false)]);
                assert_eq!(
                    ui.popup_action,
                    Some(PopupMenuReply {
                        id: 9,
                        choice: Some((143, 4))
                    })
                );
                // Outside presses and Escape dismiss without a choice.
                ui.popup_action = None;
                ui.set_popup_menu(Some(PopupMenuSnapshot {
                    id: 10,
                    ..popup.clone()
                }));
                frame(&mut ui, Vec::new());
                let outside = egui::pos2(5., 690.);
                frame(
                    &mut ui,
                    vec![
                        egui::Event::PointerMoved(outside),
                        egui::Event::PointerButton {
                            pos: outside,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                assert_eq!(
                    ui.popup_action,
                    Some(PopupMenuReply {
                        id: 10,
                        choice: None
                    })
                );
                ui.popup_action = None;
                ui.set_popup_menu(Some(PopupMenuSnapshot { id: 11, ..popup }));
                frame(&mut ui, Vec::new());
                frame(
                    &mut ui,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                assert_eq!(
                    ui.popup_action,
                    Some(PopupMenuReply {
                        id: 11,
                        choice: None
                    })
                );
            }
        });
    }

    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn workspace_system_dialogs_reply_at_both_dpi_scales() {
        use systemless::systems::macintosh::host_ui::{
            SystemDialogAnswer, SystemDialogKind, SystemDialogReply, SystemDialogSnapshot,
        };
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut ui = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                ui.theme = Theme::new(appearance);
                ui.theme.apply(&ui.context);
                let size = [(1000. * scale) as u32, (700. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("system-dialog-capture"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut time = 0.;
                let mut frame = |ui: &mut WorkspaceUi, events: Vec<egui::Event>| {
                    time += 0.2;
                    let mut input = egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        events,
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                };
                let key = |key| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                };
                ui.set_modality(UiModality {
                    owner: UiOwner::SystemDialog,
                    menus_blocked: true,
                });
                ui.set_system_dialog(Some(SystemDialogSnapshot {
                    id: 4,
                    kind: SystemDialogKind::Color {
                        prompt: "Choose a highlight color".into(),
                        color: [0xffff, 0x8000, 0x0000],
                    },
                }));
                for _ in 0..3 {
                    frame(&mut ui, Vec::new());
                }
                assert!(ui.system_action.is_none());
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        directory.join(format!("color-picker-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                frame(&mut ui, vec![key(egui::Key::Enter)]);
                assert_eq!(
                    ui.system_action,
                    Some(SystemDialogReply {
                        id: 4,
                        answer: SystemDialogAnswer::Color([0xffff, 0x8080, 0]),
                    })
                );
                // One answer per dialog, even if frames continue.
                let answered = ui.system_action.take();
                frame(&mut ui, vec![key(egui::Key::Enter)]);
                assert!(answered.is_some() && ui.system_action.is_none());
                ui.set_system_dialog(Some(SystemDialogSnapshot {
                    id: 5,
                    kind: SystemDialogKind::Print,
                }));
                frame(&mut ui, Vec::new());
                frame(&mut ui, vec![key(egui::Key::Escape)]);
                assert_eq!(
                    ui.system_action,
                    Some(SystemDialogReply {
                        id: 5,
                        answer: SystemDialogAnswer::Cancel,
                    })
                );

                // Notification Manager alerts are non-blocking cards.
                ui.set_system_dialog(None);
                ui.set_modality(UiModality::default());
                let note = systemless::systems::macintosh::host_ui::NotificationSnapshot {
                    id: 12,
                    text: "Download complete.".into(),
                };
                ui.set_notifications(std::slice::from_ref(&note));
                frame(&mut ui, Vec::new());
                frame(&mut ui, Vec::new());
                let card = ui
                    .context
                    .memory(|m| m.area_rect(egui::Id::new(("notification", 12u64))))
                    .unwrap();
                assert!(card.right() > 900.);
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("notification-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                let ok = egui::pos2(card.left() + 28., card.bottom() - 22.);
                let press = |pressed| egui::Event::PointerButton {
                    pos: ok,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                frame(&mut ui, vec![egui::Event::PointerMoved(ok), press(true)]);
                frame(&mut ui, vec![press(false)]);
                assert_eq!(std::mem::take(&mut ui.dismissed_notifications), vec![12]);
                frame(&mut ui, Vec::new());
                assert!(ui.dismissed_notifications.is_empty(), "dismissed once");

                // Program linking and bad-disk alerts are informational.
                for (id, kind, answer) in [
                    (
                        20u64,
                        SystemDialogKind::ProgramLinking {
                            prompt: String::new(),
                        },
                        SystemDialogAnswer::Cancel,
                    ),
                    (21, SystemDialogKind::BadDisk, SystemDialogAnswer::Confirm),
                ] {
                    ui.set_system_dialog(Some(SystemDialogSnapshot { id, kind }));
                    frame(&mut ui, Vec::new());
                    frame(&mut ui, vec![key(egui::Key::Enter)]);
                    assert_eq!(
                        ui.system_action.take(),
                        Some(SystemDialogReply { id, answer })
                    );
                }
                ui.set_system_dialog(None);

                // Balloon Help shows the guest's balloon beside its hot spot.
                ui.crop = (0, 0, 640, 480);
                let _guest = ui.target(&device, (640, 480));
                ui.set_balloon(
                    Some(systemless::systems::macintosh::host_ui::BalloonSnapshot {
                        id: 30,
                        text: "Saves the current document.".into(),
                        artwork: None,
                        tip: (100, 200),
                    }),
                    true,
                );
                frame(&mut ui, Vec::new());
                frame(&mut ui, Vec::new());
                let balloon = ui
                    .context
                    .memory(|m| m.area_rect(egui::Id::new(("balloon", 30u64))))
                    .unwrap();
                let tip = ui.guest_to_points(100, 200).unwrap();
                assert!((balloon.left_top() - tip - egui::vec2(12., 12.)).length() < 1.);
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::PathBuf::from(directory).join(format!("balloon-{scale}x.png")),
                        &readback(&device, &queue, &target, size),
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                ui.set_balloon(None, true);

                // An unexpected guest stop is reported once.
                ui.set_stopped(Some("The application reported system error 25.".into()));
                assert!(ui.error.as_deref().unwrap().contains("system error 25"));
                ui.error = None;
                ui.set_stopped(Some("The application reported system error 25.".into()));
                assert!(ui.error.is_none());
            }
        });
    }

    #[test]
    #[ignore = "requires a real wgpu adapter"]
    fn workspace_lifts_guest_rendered_modals_at_both_dpi_scales() {
        use systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot;
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            for (scale, appearance) in [(1., Appearance::Dark), (2., Appearance::Light)] {
                let mut ui = WorkspaceUi::headless(&device, wgpu::TextureFormat::Rgba8Unorm);
                ui.theme = Theme::new(appearance);
                ui.theme.apply(&ui.context);
                ui.crop = (0, 0, 640, 480);
                let guest = ui.target(&device, (640, 480));
                // A vivid guest image: the lifted region stays undimmed while
                // the modal backdrop darkens the rest of the pane.
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &guest,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.,
                                g: 0.,
                                b: 1.,
                                a: 1.,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                queue.submit([encoder.finish()]);
                let size = [(1000. * scale) as u32, (700. * scale) as u32];
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("lifted-capture"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let mut time = 0.;
                let mut frame = |ui: &mut WorkspaceUi| {
                    time += 0.2;
                    let mut input = egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..Default::default()
                    };
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(scale);
                    let output = ui.build_frame(input);
                    ui.paint(
                        &device,
                        &queue,
                        &target.create_view(&Default::default()),
                        size,
                        output,
                    );
                };
                ui.set_modality(UiModality {
                    owner: UiOwner::Guest,
                    menus_blocked: true,
                });
                ui.set_guest_surface(Some(GuestSurfaceSnapshot {
                    id: 3,
                    title: "Custom Options".into(),
                    bounds: (120, 200, 240, 400),
                }));
                assert!(ui.guest_focused, "a guest modal owns the keyboard");
                for _ in 0..3 {
                    frame(&mut ui);
                }
                let (rect, bounds, _) = ui.lifted.expect("surface is lifted");
                assert_eq!(bounds, (120, 200, 240, 400));
                let pane = ui.guest_rect.unwrap();
                let zoom = pane.width() / 640.;
                assert!((rect.width() - 200. * zoom).abs() < 0.5);
                // Pointer input over the surface maps into guest coordinates.
                ui.pointer = rect.center();
                assert!(ui.pointer_over_guest());
                let center = ui
                    .map_pointer(
                        rect.center().x as f64 * scale as f64,
                        rect.center().y as f64 * scale as f64,
                        scale as f64,
                    )
                    .unwrap();
                assert!(
                    (center.0 - 180).abs() <= 1 && (center.1 - 300).abs() <= 1,
                    "{center:?}"
                );
                let corner = ui
                    .map_pointer(
                        rect.left() as f64 * scale as f64,
                        rect.top() as f64 * scale as f64,
                        scale as f64,
                    )
                    .unwrap();
                assert_eq!(corner, (120, 200));
                // Elsewhere, the backdrop blocks guest input.
                ui.pointer = egui::pos2(pane.left() + 4., pane.bottom() - 4.);
                assert!(!ui.pointer_over_guest());
                let pixels = readback(&device, &queue, &target, size);
                let sample = |p: egui::Pos2| {
                    let (x, y) = ((p.x * scale) as usize, (p.y * scale) as usize);
                    let i = (y * size[0] as usize + x) * 4;
                    [pixels[i], pixels[i + 1], pixels[i + 2]]
                };
                let inside = sample(rect.center());
                let outside = sample(egui::pos2(pane.left() + 8., pane.bottom() - 8.));
                assert!(
                    inside[0] > 200 && inside[1] < 60 && inside[2] > 200,
                    "{inside:?}"
                );
                assert!(
                    outside[0] < inside[0] - 40,
                    "backdrop dims the pane: {outside:?}"
                );
                if let Some(directory) = std::env::var_os("SYSTEMLESS_UI_CAPTURE_DIR") {
                    std::fs::create_dir_all(&directory).unwrap();
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("lifted-guest-dialog-{scale}x.png")),
                        &pixels,
                        size[0],
                        size[1],
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                ui.set_guest_surface(None);
                frame(&mut ui);
                assert!(ui.lifted.is_none());

                // Guest-drawn modeless dialogs are framed in movable windows.
                ui.set_modality(UiModality::default());
                ui.set_guest_windows(&[GuestSurfaceSnapshot {
                    id: 9,
                    title: "Palette".into(),
                    bounds: (300, 40, 380, 200),
                }]);
                for _ in 0..3 {
                    frame(&mut ui);
                }
                let (rect, bounds, _) = ui.lifted_windows[0];
                assert_eq!(bounds, (300, 40, 380, 200));
                ui.pointer = rect.center();
                assert!(ui.pointer_over_guest());
                let corner = ui
                    .map_pointer(
                        rect.left() as f64 * scale as f64,
                        rect.top() as f64 * scale as f64,
                        scale as f64,
                    )
                    .unwrap();
                assert_eq!(corner, (300, 40));
                // Cursor warps into the framed region land on the frame.
                ui.guest_focused = true;
                let (x, y) = ui.guest_to_physical(340, 120, scale as f64).unwrap();
                assert!(rect.contains(egui::pos2(
                    (x / scale as f64) as f32,
                    (y / scale as f64) as f32
                )));
                // The window's title bar is host chrome, not guest input.
                ui.pointer = egui::pos2(rect.center().x, rect.top() - 12.);
                assert!(!ui.pointer_over_guest());
                ui.set_guest_windows(&[]);
                frame(&mut ui);
                assert!(ui.lifted_windows.is_empty() && ui.guest_window_states.is_empty());
            }
        });
    }

    fn readback(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        size: [u32; 2],
    ) -> Vec<u8> {
        let stride = (size[0] * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (stride * size[1]) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(size[1]),
                },
            },
            texture.size(),
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        mapped
            .chunks(stride as usize)
            .flat_map(|row| row[..size[0] as usize * 4].iter().copied())
            .collect()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedWorkspace {
    version: u32,
    light: bool,
    reduced_motion: bool,
    layout: String,
}

/// Convert a guest menu into design-system rows identified by (menu, item).
/// Hierarchical cycles and excessive nesting are cut off.
fn menu_rows(
    snapshot: &systemless::systems::macintosh::menu_model::GuestMenuSnapshot,
    help: Option<&std::collections::HashMap<(i16, i16), String>>,
    id: i16,
    ancestors: &mut Vec<i16>,
) -> Vec<systemless_ui::menu::Item<(i16, i16)>> {
    if ancestors.contains(&id) || ancestors.len() >= 16 {
        return Vec::new();
    }
    let Some(menu) = snapshot.menus.iter().find(|m| m.id == id) else {
        return Vec::new();
    };
    ancestors.push(id);
    let rows = menu
        .items
        .iter()
        .map(|item| {
            if item.separator {
                return systemless_ui::menu::Item::separator((id, item.number));
            }
            let mut row = systemless_ui::menu::Item::new((id, item.number), item.text.clone());
            row.enabled = menu.enabled && item.enabled;
            row.checked = item.checked;
            row.shortcut = item.key_equivalent.map(|key| format!("⌘{key}"));
            row.help = help.and_then(|help| help.get(&(id, item.number)).cloned());
            if let Some(submenu) = item.submenu_id {
                row.children = menu_rows(snapshot, help, submenu, ancestors);
            }
            row
        })
        .collect();
    ancestors.pop();
    rows
}
