//! Owned frontend projections for Systemless-supplied Macintosh UI.
//! No egui types or guest pointers cross this boundary. The runtime remains
//! responsible for validating replies and completing the guest ABI call.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogKind {
    Open,
    Save,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogEntry {
    pub name: String,
    pub directory: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogSnapshot {
    pub id: u64,
    pub revision: u64,
    pub kind: FileDialogKind,
    pub prompt: String,
    pub directory: String,
    pub entries: Vec<FileDialogEntry>,
    pub name: String,
    pub writable: bool,
    /// Guest callbacks are still preparing the listing; choices are unavailable.
    pub busy: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogAction {
    /// Open a file, or enter a directory, from this snapshot's entry list.
    Open(usize),
    /// Save in the current virtual directory. The backend applies MacRoman and ABI limits.
    Save(String),
    Parent,
    Desktop,
    Cancel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogReply {
    pub id: u64,
    pub revision: u64,
    pub action: FileDialogAction,
}

/// Current input surface. Guest-rendered modals still block projected menus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiOwner {
    #[default]
    Guest,
    Dialog,
    FileDialog,
    /// A Systemless-implemented popup menu awaits a host choice.
    PopupMenu,
    /// A Systemless-implemented package dialog (color picker, printing).
    SystemDialog,
}

/// A modal dialog the guest renders itself (application filters, custom
/// controls or drawing). The host frames this region of the guest image in
/// its own modal surface and routes input back to these guest coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestSurfaceSnapshot {
    pub id: u64,
    pub title: String,
    /// Content rectangle (top, left, bottom, right), presented coordinates.
    pub bounds: (i16, i16, i16, i16),
}

/// A Help Manager balloon. `tip` is the hot spot in presented coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalloonSnapshot {
    pub id: u64,
    pub text: String,
    pub tip: (i16, i16),
}

/// Text of an HMMessageRecord: khmmString (1), khmmStringRes (3, `STR#`
/// ID and index) or khmmSTRRes (7, `STR `). Other message kinds are not
/// presented. Inside Macintosh: More Macintosh Toolbox (1993), pp. 3-127--3-129.
pub(crate) fn balloon_message_text(
    message: &[u8],
    resource: impl Fn([u8; 4], i16) -> Option<Vec<u8>>,
) -> Option<String> {
    let word = |offset: usize| {
        message
            .get(offset..offset + 2)
            .map(|bytes| i16::from_be_bytes([bytes[0], bytes[1]]))
    };
    let pstring = |bytes: &[u8]| -> Option<String> {
        let length = usize::from(*bytes.first()?);
        Some(crate::trap::types::decode_mac_roman(
            bytes.get(1..1 + length)?,
        ))
    };
    let text = match word(0)? {
        1 => pstring(message.get(2..)?)?,
        3 => {
            let list = resource(*b"STR#", word(2)?)?;
            let index = usize::try_from(word(4)?).ok()?.checked_sub(1)?;
            let count =
                usize::try_from(i16::from_be_bytes([*list.first()?, *list.get(1)?])).ok()?;
            if index >= count {
                return None;
            }
            let mut offset = 2;
            for _ in 0..index {
                offset += 1 + usize::from(*list.get(offset)?);
            }
            pstring(list.get(offset..)?)?
        }
        7 => pstring(&resource(*b"STR ", word(2)?)?)?,
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// A Notification Manager alert string, shown once per installed request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub id: u64,
    pub text: String,
}

/// Package dialogs Systemless implements itself rather than through a DITL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemDialogKind {
    /// Color Picker Package GetColor; components are 16-bit RGB.
    Color { prompt: String, color: [u16; 3] },
    /// Printing Manager PrStlDialog.
    PageSetup,
    /// Printing Manager PrJobDialog.
    Print,
    /// Program Linking PPCBrowser; Systemless has no network ports.
    ProgramLinking { prompt: String },
    /// Disk Initialization DIBadMount; Systemless has no physical disks.
    BadDisk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemDialogSnapshot {
    pub id: u64,
    pub kind: SystemDialogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemDialogAnswer {
    Cancel,
    Confirm,
    Color([u16; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemDialogReply {
    pub id: u64,
    pub answer: SystemDialogAnswer,
}

/// A standard-MDEF PopUpMenuSelect call presented by the host. The menu's
/// contents come from the published menu list; `item` is the item placed
/// at `anchor` (presented-canvas coordinates of its top-left corner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopupMenuSnapshot {
    pub id: u64,
    pub menu: i16,
    pub item: i16,
    pub anchor: (i16, i16),
}

/// `None` dismisses the menu without a choice, as releasing outside it would.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupMenuReply {
    pub id: u64,
    pub choice: Option<(i16, i16)>,
}

/// Validate a popup choice: the chosen menu must be the popup itself or one
/// of its hierarchical descendants, and the item must be selectable.
pub(crate) fn popup_choice_result(
    menus: &crate::systems::macintosh::menu_model::GuestMenuSnapshot,
    root: i16,
    menu: i16,
    item: i16,
) -> Option<u32> {
    let mut pending = vec![root];
    let mut seen = Vec::new();
    while let Some(id) = pending.pop() {
        if seen.contains(&id) || seen.len() > 64 {
            continue;
        }
        seen.push(id);
        if let Some(found) = menus.menus.iter().find(|m| m.id == id) {
            pending.extend(found.items.iter().filter_map(|i| i.submenu_id));
        }
    }
    seen.contains(&menu)
        .then(|| menus.selectable_result(menu, item))
        .flatten()
}

/// Identity, title and content bounds of a guest-rendered dialog.
pub(crate) type GuestDialogSurface = (u64, String, (i16, i16, i16, i16));

/// A live modeless dialog's guest pointer with its current projection.
pub(crate) type ModelessTarget = (u32, DialogSnapshot);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostPopup {
    snapshot: PopupMenuSnapshot,
    result: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiModality {
    pub owner: UiOwner,
    pub menus_blocked: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ModalStatus {
    pub dialog: Option<u32>,
    pub dialog_order: u64,
    pub file: bool,
    pub file_order: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModalKey {
    Dialog(u32),
    File,
}

/// File-package state retained while a guest filter opens a nested file dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SuspendedFileUi {
    id: u64,
    revision: u64,
    active: bool,
    pending: Option<FileDialogAction>,
    file_order: u64,
    last_modal: Option<ModalKey>,
}

/// Where a staged semantic click lands once the guest event loop reaches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostClickTarget {
    Item(i16),
    GoAway,
}

/// A host decision delivered as an ordinary guest click. DialogSelect and
/// TrackGoAway consume it instead of re-tracking work the host completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HostClick {
    dialog: u32,
    identity: u64,
    epoch: u64,
    point: (i16, i16),
    target: HostClickTarget,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostUiState {
    pub enabled: bool,
    id: u64,
    revision: u64,
    active: bool,
    pending: Option<FileDialogAction>,
    dialogs: std::collections::HashMap<u32, u64>,
    projected_dialogs: std::collections::HashMap<u64, (u64, DialogSnapshot)>,
    presented_dialogs: std::collections::HashSet<u32>,
    modeless_dialogs: std::collections::HashSet<u32>,
    dialog_restore_pending: std::collections::HashSet<u32>,
    dialog_revision: u64,
    projection_epoch: u64,
    last_modal: Option<ModalKey>,
    dialog_orders: std::collections::HashMap<u32, u64>,
    file_order: u64,
    range_tracking: std::collections::HashMap<u32, (u32, u64, i16, i16, i16)>,
    host_clicks: std::collections::VecDeque<HostClick>,
    popup: Option<HostPopup>,
    system: Option<(SystemDialogSnapshot, Option<SystemDialogAnswer>)>,
    /// Classic Balloon Help setting (Help menu "Show Balloons").
    pub balloons_enabled: bool,
    balloon: Option<BalloonSnapshot>,
    /// Request record, identity, alert text and whether the user dismissed it.
    notifications: Vec<(u32, u64, String, bool)>,
    artwork: std::cell::RefCell<
        std::collections::HashMap<(u32, i16), (u64, std::sync::Arc<DialogArtwork>)>,
    >,
}
impl HostUiState {
    pub fn project_icon(
        &self,
        dialog: u32,
        item: i16,
        bytes: &[u8],
        color: bool,
    ) -> Option<std::sync::Arc<DialogArtwork>> {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        color.hash(&mut hash);
        bytes.hash(&mut hash);
        let hash = hash.finish();
        let key = (dialog, item);
        if let Some((_, image)) = self
            .artwork
            .borrow()
            .get(&key)
            .filter(|(old, _)| *old == hash)
        {
            return Some(image.clone());
        }
        let image = std::sync::Arc::new(DialogArtwork::decode(bytes, color)?);
        self.artwork.borrow_mut().insert(key, (hash, image.clone()));
        Some(image)
    }
    pub fn project_picture(
        &self,
        dialog: u32,
        item: i16,
        bytes: &[u8],
        size: (u16, u16),
    ) -> Option<std::sync::Arc<DialogArtwork>> {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        ("dialog-picture-v1", size, bytes).hash(&mut hash);
        let hash = hash.finish();
        let key = (dialog, item);
        if let Some((_, image)) = self
            .artwork
            .borrow()
            .get(&key)
            .filter(|(old, _)| *old == hash)
        {
            return Some(image.clone());
        }
        let image = std::sync::Arc::new(DialogArtwork::picture(bytes, size)?);
        self.artwork.borrow_mut().insert(key, (hash, image.clone()));
        Some(image)
    }

    pub fn project_alert_icon(
        &self,
        dialog: u32,
        item: i16,
        icon_id: i16,
    ) -> Option<std::sync::Arc<DialogArtwork>> {
        use std::hash::{Hash, Hasher};
        if !(0..=2).contains(&icon_id) {
            return None;
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        ("system-alert-icon-v1", icon_id).hash(&mut hash);
        let hash = hash.finish();
        let key = (dialog, item);
        if let Some((_, image)) = self
            .artwork
            .borrow()
            .get(&key)
            .filter(|(old, _)| *old == hash)
        {
            return Some(image.clone());
        }
        let image = std::sync::Arc::new(DialogArtwork::system_alert(icon_id)?);
        self.artwork.borrow_mut().insert(key, (hash, image.clone()));
        Some(image)
    }

    /// Consume a semantic control command even when it reselects the current value.
    pub fn invalidate_dialog_snapshot(&mut self, pointer: u32) {
        if let Some(id) = self.dialogs.get(&pointer) {
            self.projected_dialogs.remove(id);
        }
    }

    pub fn stage_range_tracking(
        &mut self,
        dialog: u32,
        control: u32,
        value: i16,
        min: i16,
        max: i16,
    ) {
        if let Some(id) = self.dialog_identity(dialog) {
            self.range_tracking
                .insert(control, (dialog, id, value, min, max));
        }
    }
    pub fn has_range_tracking(&self, control: u32) -> bool {
        self.range_tracking.contains_key(&control)
    }
    pub fn take_range_tracking(
        &mut self,
        dialog: u32,
        control: u32,
        value: i16,
        min: i16,
        max: i16,
    ) -> bool {
        self.range_tracking.remove(&control).is_some_and(|saved| {
            saved
                == (
                    dialog,
                    self.dialog_identity(dialog).unwrap_or(0),
                    value,
                    min,
                    max,
                )
                && self.dialog_presented(dialog)
        })
    }
    pub fn clear_range_tracking(&mut self, dialog: u32) {
        self.range_tracking.retain(|_, saved| saved.0 != dialog);
    }
    pub fn retire_control(&mut self, control: u32) {
        self.range_tracking.remove(&control);
    }

    /// HMShowBalloon: replaces any balloon already shown.
    pub fn show_balloon(&mut self, text: String, tip: (i16, i16)) -> bool {
        if !self.enabled || !self.balloons_enabled {
            return false;
        }
        self.balloon = Some(BalloonSnapshot {
            id: next_identity(),
            text,
            tip,
        });
        true
    }
    pub fn remove_balloon(&mut self) {
        self.balloon = None;
    }
    pub fn balloon(&self) -> Option<BalloonSnapshot> {
        self.balloon.clone()
    }

    /// NMInstall with an nmStr alert. Reinstalling the same record replaces it.
    pub fn note_notification(&mut self, record: u32, text: String) {
        if !self.enabled || text.is_empty() {
            return;
        }
        self.notifications.retain(|(rec, ..)| *rec != record);
        self.notifications
            .push((record, next_identity(), text, false));
    }
    /// NMRemove: the application withdrew the request.
    pub fn remove_notification(&mut self, record: u32) {
        self.notifications.retain(|(rec, ..)| *rec != record);
    }
    pub fn notifications(&self) -> Vec<NotificationSnapshot> {
        self.notifications
            .iter()
            .filter(|(.., dismissed)| !dismissed)
            .map(|(_, id, text, _)| NotificationSnapshot {
                id: *id,
                text: text.clone(),
            })
            .collect()
    }
    /// The alert closes; the request stays queued until the application
    /// removes it, as with the Notification Manager's own alert.
    pub fn dismiss_notification(&mut self, id: u64) -> bool {
        self.notifications
            .iter_mut()
            .find(|(_, identity, _, dismissed)| *identity == id && !dismissed)
            .map(|entry| entry.3 = true)
            .is_some()
    }

    /// Park a package call on a host dialog. Returns false when the host UI
    /// is disabled, in which case the caller keeps its headless behavior.
    pub fn begin_system_dialog(&mut self, kind: SystemDialogKind) -> bool {
        if !self.enabled {
            return false;
        }
        self.system = Some((
            SystemDialogSnapshot {
                id: next_identity(),
                kind,
            },
            None,
        ));
        true
    }
    pub fn system_dialog_active(&self) -> bool {
        self.system.is_some()
    }
    pub fn system_dialog(&self) -> Option<SystemDialogSnapshot> {
        self.system
            .as_ref()
            .filter(|(_, answer)| answer.is_none())
            .map(|(snapshot, _)| snapshot.clone())
    }
    /// Colors answer only color requests; Confirm answers printing dialogs.
    pub fn reply_system_dialog(&mut self, reply: SystemDialogReply) -> bool {
        let Some((snapshot, answer)) = self
            .system
            .as_mut()
            .filter(|(snapshot, answer)| snapshot.id == reply.id && answer.is_none())
        else {
            return false;
        };
        let valid = matches!(
            (&snapshot.kind, reply.answer),
            (_, SystemDialogAnswer::Cancel)
                | (SystemDialogKind::Color { .. }, SystemDialogAnswer::Color(_))
                | (
                    SystemDialogKind::PageSetup
                        | SystemDialogKind::Print
                        | SystemDialogKind::BadDisk,
                    SystemDialogAnswer::Confirm,
                )
        );
        if valid {
            *answer = Some(reply.answer);
        }
        valid
    }
    pub fn take_system_answer(&mut self) -> Option<SystemDialogAnswer> {
        let answer = self.system.as_ref()?.1?;
        self.system = None;
        Some(answer)
    }

    /// Begin presenting a popup menu; the guest call stays parked until a
    /// reply produces its PopUpMenuSelect result.
    pub fn begin_popup(&mut self, menu: i16, item: i16, anchor: (i16, i16)) {
        self.popup = Some(HostPopup {
            snapshot: PopupMenuSnapshot {
                id: next_identity(),
                menu,
                item,
                anchor,
            },
            result: None,
        });
    }
    pub fn popup_active(&self) -> bool {
        self.popup.is_some()
    }
    pub fn popup_snapshot(&self) -> Option<PopupMenuSnapshot> {
        self.popup
            .as_ref()
            .filter(|popup| popup.result.is_none())
            .map(|popup| popup.snapshot.clone())
    }
    /// `selectable` validates a choice against the live menu list and returns
    /// its packed result; rejected choices leave the menu open.
    pub fn reply_popup(
        &mut self,
        reply: PopupMenuReply,
        selectable: impl FnOnce(i16, i16) -> Option<u32>,
    ) -> bool {
        let Some(popup) = self
            .popup
            .as_mut()
            .filter(|popup| popup.snapshot.id == reply.id && popup.result.is_none())
        else {
            return false;
        };
        let result = match reply.choice {
            None => 0,
            Some((menu, item)) => match selectable(menu, item) {
                Some(result) => result,
                None => return false,
            },
        };
        popup.result = Some(result);
        true
    }
    /// Consume the chosen result once it exists, ending the presentation.
    pub fn take_popup_result(&mut self) -> Option<u32> {
        let result = self.popup.as_ref()?.result?;
        self.popup = None;
        Some(result)
    }
    pub fn clear_popup(&mut self) {
        self.popup = None;
    }

    /// Stage a click for a presented modeless dialog; the caller posts the
    /// matching mouse-down/up pair at `point` in canonical global coordinates.
    pub fn stage_host_click(&mut self, dialog: u32, point: (i16, i16), target: HostClickTarget) {
        let Some(identity) = self.dialog_identity(dialog) else {
            return;
        };
        if self.host_clicks.len() >= 8 {
            self.host_clicks.pop_front();
        }
        self.host_clicks.push_back(HostClick {
            dialog,
            identity,
            epoch: self.projection_epoch,
            point,
            target,
        });
    }
    /// Consume the oldest staged click for `dialog`. Earlier clicks the
    /// application handled without DialogSelect/TrackGoAway are discarded, so
    /// they can never be reinterpreted by a later ordinary guest click.
    /// Item and go-away clicks are matched independently.
    pub fn take_host_click(
        &mut self,
        dialog: u32,
        point: (i16, i16),
        go_away: bool,
    ) -> Option<HostClickTarget> {
        while let Some(index) = self
            .host_clicks
            .iter()
            .position(|c| c.dialog == dialog && (c.target == HostClickTarget::GoAway) == go_away)
        {
            let click = self.host_clicks.remove(index).unwrap();
            if click.point == point {
                return (click.epoch == self.projection_epoch
                    && self.dialog_identity(dialog) == Some(click.identity)
                    && self.dialog_is_modeless(dialog))
                .then_some(click.target);
            }
        }
        None
    }
    fn clear_host_clicks(&mut self, dialog: u32) {
        self.host_clicks.retain(|click| click.dialog != dialog);
    }

    pub fn note_dialog_modal(&mut self, pointer: u32) {
        if self.last_modal != Some(ModalKey::Dialog(pointer)) {
            self.projection_epoch = next_identity();
            self.dialog_orders.insert(pointer, next_identity());
            self.last_modal = Some(ModalKey::Dialog(pointer));
        }
    }
    pub fn dialog_order(&self, pointer: u32) -> u64 {
        self.dialog_orders
            .get(&pointer)
            .copied()
            .or_else(|| self.dialogs.get(&pointer).copied())
            .unwrap_or(0)
    }
    pub fn was_dialog_modal(&self, pointer: u32) -> bool {
        self.dialog_orders.contains_key(&pointer)
    }
    pub fn file_order(&self) -> u64 {
        self.file_order
    }
    pub fn register_dialog(&mut self, pointer: u32) {
        self.modeless_dialogs.remove(&pointer);
        if let Some(old) = self.dialogs.insert(pointer, next_identity()) {
            self.projected_dialogs.remove(&old);
        }
    }
    pub fn retire_dialog(&mut self, pointer: u32) {
        self.modeless_dialogs.remove(&pointer);
        self.clear_host_clicks(pointer);
        self.clear_range_tracking(pointer);
        self.dialog_orders.remove(&pointer);
        self.artwork
            .borrow_mut()
            .retain(|(dialog, _), _| *dialog != pointer);
        if self.last_modal == Some(ModalKey::Dialog(pointer)) {
            self.projection_epoch = next_identity();
            self.last_modal = None;
        }
        self.presented_dialogs.remove(&pointer);
        self.dialog_restore_pending.remove(&pointer);
        if let Some(id) = self.dialogs.remove(&pointer) {
            self.projected_dialogs.remove(&id);
        }
    }
    pub fn present_dialog(&mut self, pointer: u32) -> bool {
        if !self.enabled {
            return false;
        }
        self.modeless_dialogs.remove(&pointer);
        let first = self.presented_dialogs.insert(pointer);
        let refreshed = self.dialog_restore_pending.remove(&pointer);
        first || refreshed
    }
    pub fn present_modeless_dialog(&mut self, pointer: u32) -> bool {
        if !self.enabled {
            return false;
        }
        let restore = self.present_dialog(pointer);
        self.modeless_dialogs.insert(pointer);
        restore
    }
    pub fn dialog_is_modeless(&self, pointer: u32) -> bool {
        self.dialog_presented(pointer) && self.modeless_dialogs.contains(&pointer)
    }
    /// Hiding preserves logical ownership and the editor's lifetime. Showing
    /// captures a new background, which the next host projection must restore.
    pub fn invalidate_dialog_presentation(&mut self, pointer: u32) {
        if !self.dialog_presented(pointer) {
            return;
        }
        self.dialog_restore_pending.insert(pointer);
        self.invalidate_dialog_snapshot(pointer);
    }
    pub fn unpresent_dialog(&mut self, pointer: u32) -> bool {
        self.modeless_dialogs.remove(&pointer);
        self.clear_host_clicks(pointer);
        self.dialog_restore_pending.remove(&pointer);
        let removed = self.presented_dialogs.remove(&pointer);
        if removed {
            self.invalidate_dialog_snapshot(pointer);
        }
        removed
    }
    pub fn dialog_presented(&self, pointer: u32) -> bool {
        self.enabled && self.presented_dialogs.contains(&pointer)
    }
    pub fn dialog_identity(&self, pointer: u32) -> Option<u64> {
        self.enabled
            .then(|| self.dialogs.get(&pointer).copied())
            .flatten()
    }
    pub fn dialog_pointer(&self, id: u64) -> Option<u32> {
        self.enabled
            .then(|| {
                self.dialogs
                    .iter()
                    .find_map(|(pointer, identity)| (*identity == id).then_some(*pointer))
            })
            .flatten()
    }

    pub fn live_dialog_ids(&self) -> Vec<u64> {
        if !self.enabled {
            return Vec::new();
        }
        let mut ids: Vec<_> = self.dialogs.values().copied().collect();
        ids.sort_unstable();
        ids
    }
    pub fn project_dialog(&mut self, mut snapshot: DialogSnapshot) -> DialogSnapshot {
        // A modal transition invalidates old interactions, while independently
        // rendered modeless windows keep stable revisions in the same context.
        let context = self.projection_epoch;
        let cached = self.projected_dialogs.get(&snapshot.id);
        snapshot.revision = cached.map_or(0, |(_, old)| old.revision);
        if cached.is_none_or(|(old_context, old)| *old_context != context || *old != snapshot) {
            self.dialog_revision = self
                .dialog_revision
                .checked_add(1)
                .expect("dialog revision exhausted");
            snapshot.revision = self.dialog_revision;
            self.projected_dialogs
                .insert(snapshot.id, (context, snapshot.clone()));
        }
        snapshot
    }
    pub fn suspend_file(&mut self) -> SuspendedFileUi {
        self.projection_epoch = next_identity();
        let saved = SuspendedFileUi {
            id: self.id,
            revision: self.revision,
            active: self.active,
            pending: self.pending.take(),
            file_order: self.file_order,
            last_modal: self.last_modal,
        };
        self.active = false;
        saved
    }
    pub fn resume_file(&mut self, saved: SuspendedFileUi) {
        self.projection_epoch = next_identity();
        self.id = saved.id;
        self.revision = saved.revision;
        self.active = saved.active;
        self.pending = saved.pending;
        self.file_order = saved.file_order;
        self.last_modal = saved.last_modal;
    }
    pub fn begin(&mut self) {
        self.projection_epoch = next_identity();
        self.file_order = next_identity();
        self.last_modal = Some(ModalKey::File);
        self.id = next_identity();
        self.revision = 0;
        self.active = true;
        self.pending = None;
    }
    pub fn finish(&mut self) {
        if self.active {
            self.projection_epoch = next_identity();
        }
        self.active = false;
        self.pending = None;
    }
    pub fn identity(&self) -> Option<(u64, u64)> {
        (self.enabled && self.active).then_some((self.id, self.revision))
    }
    pub fn reply(&mut self, reply: FileDialogReply) -> bool {
        if self.identity() != Some((reply.id, reply.revision)) || self.pending.is_some() {
            return false;
        }
        self.pending = Some(reply.action);
        true
    }
    pub fn take(&mut self) -> Option<FileDialogAction> {
        let action = self.pending.take()?;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("host dialog revision exhausted");
        Some(action)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_stale_and_retired_dialog_replies() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        state.begin();
        let reply = FileDialogReply {
            id: state.identity().unwrap().0,
            revision: 0,
            action: FileDialogAction::Desktop,
        };
        assert!(state.reply(reply.clone()));
        assert!(!state.reply(reply.clone()));
        assert_eq!(state.take(), Some(FileDialogAction::Desktop));
        assert!(!state.reply(reply.clone()));
        state.finish();
        assert!(state.identity().is_none());
        state.begin();
        assert_ne!(state.identity().unwrap().0, reply.id);
        assert_eq!(state.identity().unwrap().1, 0);
        assert!(!state.reply(reply));
    }
    #[test]
    fn disabled_projection_never_accepts_frontend_input() {
        let mut state = HostUiState::default();
        state.begin();
        assert!(state.identity().is_none());
        assert!(!state.reply(FileDialogReply {
            id: 1,
            revision: 0,
            action: FileDialogAction::Cancel
        }));
    }
    #[test]
    fn dialog_lifetimes_survive_obscuring_and_retire_before_pointer_reuse() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        state.register_dialog(100);
        let parent = state.dialog_identity(100).unwrap();
        state.present_dialog(100);
        state.register_dialog(200);
        let child = state.dialog_identity(200).unwrap();
        state.note_dialog_modal(200);
        assert_eq!(state.live_dialog_ids(), vec![parent, child]);
        state.unpresent_dialog(100);
        assert!(state.live_dialog_ids().contains(&parent));
        state.retire_dialog(100);
        assert_eq!(state.live_dialog_ids(), vec![child]);
        state.register_dialog(100);
        let replacement = state.dialog_identity(100).unwrap();
        assert_ne!(replacement, parent);
        assert_eq!(state.live_dialog_ids(), vec![child, replacement]);
        state.enabled = false;
        assert!(state.live_dialog_ids().is_empty());
    }
}

/// Normalize standard CNTL-backed controls to the same semantic kinds as
/// inline DITL controls. Resource controls require a materialized standard CDEF;
/// custom CDEFs retain guest drawing and interaction until their adapter exists.
pub(crate) fn standard_dialog_item_kind(kind: u8, control_proc: Option<i16>) -> Option<u8> {
    match kind & 0x7f {
        4..=6 if control_proc.is_none_or(|proc| matches!(proc, 0..=2)) => Some(kind & 0x7f),
        7 => control_proc.and_then(|proc| match proc {
            0..=2 => Some(4 + proc as u8),
            1008..=1023 => Some(7),
            16 => Some(17),
            _ => None,
        }),
        8 | 16 | 32 | 64 => Some(kind & 0x7f),
        _ => None,
    }
}

/// An owned image projection; no renderer or guest-memory handles cross the UI boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogArtwork {
    pub width: u16,
    pub height: u16,
    pub rgba: std::sync::Arc<[u8]>,
    /// Monochrome source bits use the design system's foreground color.
    pub monochrome: bool,
}
impl DialogArtwork {
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= 1024
            && self.height <= 1024
            && self.rgba.len() == usize::from(self.width) * usize::from(self.height) * 4
    }
    fn picture(bytes: &[u8], (width, height): (u16, u16)) -> Option<Self> {
        use crate::memory::{MacMemoryBus, MemoryBus};
        use crate::trap::pict;
        if width == 0
            || height == 0
            || width > 1024
            || height > 1024
            || bytes.len() > 16 * 1024 * 1024
        {
            return None;
        }
        pict::picture_stream_len(bytes)?;
        let picture_base = 0x10000u32;
        let screen_base = (picture_base + bytes.len() as u32 + 4095) & !4095;
        let row_bytes = u32::from(width) * 4;
        let length = row_bytes * u32::from(height);
        let mut bus = MacMemoryBus::new((screen_base + length + 4096) as usize);
        bus.write_bytes(picture_base, bytes);
        bus.fill_bytes(screen_base, length, 255);
        bus.begin_uncapped_write_probe();
        let clip = pict::DstClip::new((0, 0, i32::from(height), i32::from(width)), Vec::new());
        let (rendered, _) = pict::draw_picture(
            &mut bus,
            picture_base,
            0,
            0,
            height as i16,
            width as i16,
            (screen_base, row_bytes, width, height, 32),
            &crate::trap::dispatch::TrapDispatcher::standard_mac_8bpp_clut(),
            0,
            Some(&clip),
        );
        if !rendered {
            return None;
        }
        let mut rgba = vec![0; length as usize];
        for range in bus.finish_write_probe_ranges() {
            let start = range.start.max(screen_base);
            let end = range.end.min(screen_base + length);
            if start >= end {
                continue;
            }
            for pixel in ((start - screen_base) / 4)..(end - screen_base).div_ceil(4) {
                let address = screen_base + pixel * 4;
                let offset = pixel as usize * 4;
                rgba[offset..offset + 4].copy_from_slice(&[
                    bus.read_byte(address + 1),
                    bus.read_byte(address + 2),
                    bus.read_byte(address + 3),
                    255,
                ]);
            }
        }
        Some(Self {
            width,
            height,
            rgba: rgba.into(),
            monochrome: false,
        })
    }

    /// Theme-tinted fallback for built-in Macintosh stop, note and caution icons.
    pub fn system_alert(icon_id: i16) -> Option<Self> {
        if !(0..=2).contains(&icon_id) {
            return None;
        }
        // High-resolution, theme-tinted symbols for absent system resources.
        // Application ICON/cicn artwork always takes precedence in the adapters.
        let inside = |x: f32, y: f32| match icon_id {
            0 => {
                let outer = x.abs().max(y.abs()) <= 0.82 && x.abs() + y.abs() <= 1.16;
                let inner = x.abs().max(y.abs()) < 0.69 && x.abs() + y.abs() < 0.98;
                (outer && !inner)
                    || (x.abs() < 0.39 && y.abs() < 0.39 && (x.abs() - y.abs()).abs() < 0.1)
            }
            1 => {
                let r = x * x + y * y;
                (0.69 * 0.69..=0.84 * 0.84).contains(&r)
                    || (x.abs() <= 0.09 && (-0.06..=0.45).contains(&y))
                    || x * x + (y + 0.34) * (y + 0.34) <= 0.11 * 0.11
            }
            _ => {
                let outer = y >= -0.85 && y <= 0.78 && x.abs() <= (y + 0.85) * 0.55;
                let inner = y >= -0.57 && y < 0.64 && x.abs() < (y + 0.57) * 0.55;
                (outer && !inner)
                    || (x.abs() <= 0.08 && (-0.24..=0.26).contains(&y))
                    || x * x + (y - 0.46) * (y - 0.46) <= 0.09 * 0.09
            }
        };
        let mut rgba = Vec::with_capacity(128 * 128 * 4);
        for row in 0..128 {
            for col in 0..128 {
                let mut coverage = 0;
                for dy in [0.25, 0.75] {
                    for dx in [0.25, 0.75] {
                        coverage += u16::from(inside(
                            (col as f32 + dx) / 64.0 - 1.0,
                            (row as f32 + dy) / 64.0 - 1.0,
                        ));
                    }
                }
                rgba.extend_from_slice(&[255, 255, 255, (coverage * 255 / 4) as u8]);
            }
        }
        Some(Self {
            width: 128,
            height: 128,
            rgba: rgba.into(),
            monochrome: true,
        })
    }

    fn decode(bytes: &[u8], color: bool) -> Option<Self> {
        if !color {
            if bytes.len() < 128 {
                return None;
            }
            let rgba: Vec<_> = (0..1024)
                .flat_map(|pixel| {
                    let alpha = if bytes[pixel / 8] & (0x80 >> (pixel % 8)) != 0 {
                        255
                    } else {
                        0
                    };
                    [255, 255, 255, alpha]
                })
                .collect();
            return Some(Self {
                width: 32,
                height: 32,
                rgba: rgba.into(),
                monochrome: true,
            });
        }
        let layout = crate::menu_manager::ColorIconLayout::decode(bytes)?;
        if !matches!(layout.pixel_size, 1 | 2 | 4 | 8 | 16 | 32) {
            return None;
        }
        let width = u16::try_from(layout.width).ok()?;
        let height = u16::try_from(layout.height).ok()?;
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return None;
        }
        let mut rgba = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
        let mut colors = [None; 256];
        for y in 0..usize::from(height) {
            for x in 0..usize::from(width) {
                let read = |offset| bytes.get(offset).copied();
                if !layout.mask_bit_with(read, x, y)? {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let index = if layout.pixel_size <= 8 {
                    let bit = x * usize::from(layout.pixel_size);
                    let byte = *bytes
                        .get(layout.pixel_data_offset + y * layout.pixel_row_bytes + bit / 8)?;
                    Some(usize::from(
                        (byte >> (8 - usize::from(layout.pixel_size) - bit % 8))
                            & ((1u16 << layout.pixel_size) - 1) as u8,
                    ))
                } else {
                    None
                };
                let rgb = match index.and_then(|index| colors[index]) {
                    Some(rgb) => rgb,
                    None => {
                        let rgb = layout
                            .rgb_with(read, x, y)?
                            .map(|component| (component >> 8) as u8);
                        if let Some(index) = index {
                            colors[index] = Some(rgb);
                        }
                        rgb
                    }
                };
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        Some(Self {
            width,
            height,
            rgba: rgba.into(),
            monochrome: false,
        })
    }
}

/// A Systemless-owned Dialog Manager view. Item numbers retain Macintosh's
/// one-based numbering; opaque identities never expose a guest address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogSnapshot {
    /// Host surface may move independently of the guest layout.
    pub movable: bool,
    /// The guest window has a standard close box; closing is an application decision.
    pub closable: bool,
    pub id: u64,
    pub revision: u64,
    pub title: String,
    pub size: (u16, u16),
    pub items: Vec<DialogItemSnapshot>,
    /// One-based active TextEdit item, or zero.
    pub edit_item: i16,
    pub default_item: i16,
    pub cancel_item: i16,
    pub busy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogRange {
    pub min: i16,
    pub max: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogChoice {
    pub value: i16,
    pub text: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogControlReply {
    pub dialog: DialogReply,
    pub expected: i16,
    pub value: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogItemSnapshot {
    pub range: Option<DialogRange>,
    pub choices: Vec<DialogChoice>,
    pub number: i16,
    pub kind: u8,
    pub enabled: bool,
    pub rect: (i16, i16, i16, i16),
    pub text: String,
    pub value: i16,
    /// Normalized character offsets, only for editable text items.
    pub selection: Option<(u16, u16)>,
    pub artwork: Option<std::sync::Arc<DialogArtwork>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogReply {
    pub id: u64,
    pub revision: u64,
    pub item: i16,
}

/// One acknowledged edit transaction. Frontends retain newer drafts until this
/// command is acknowledged, then submit them against the next snapshot revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogTextReply {
    pub id: u64,
    pub revision: u64,
    pub serial: u64,
    /// Text observed before this draft began; prevents overwriting guest changes.
    pub expected: String,
    pub item: i16,
    pub text: String,
    pub selection: (u16, u16),
}

fn next_identity() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_ID
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |id| id.checked_add(1),
        )
        .expect("host dialog identity exhausted")
}

#[cfg(test)]
pub(crate) mod artwork_tests {
    use super::*;
    pub(crate) fn color_icon() -> Vec<u8> {
        let mut bytes = vec![0; 82];
        for (offset, value) in [
            (4, 0x8001u16),
            (10, 1),
            (12, 2),
            (32, 1),
            (54, 1),
            (60, 1),
            (62, 2),
            (68, 1),
            (74, 1),
            (76, 2),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&[0x80, 0x80]); // mask, monochrome fallback
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]); // two palette entries
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 1, 255, 255, 0, 0, 0, 0]);
        bytes.push(0x80); // red first pixel, transparent second pixel
        bytes
    }
    #[test]
    fn icon_projection_preserves_masks_palette_changes_and_cache_lifetime() {
        let mut state = HostUiState::default();
        state.register_dialog(100);
        let mut mono = [0; 128];
        mono[0] = 0x80;
        let image = state.project_icon(100, 1, &mono, false).unwrap();
        assert_eq!(&image.rgba[..8], &[255, 255, 255, 255, 255, 255, 255, 0]);
        assert!(image.monochrome);
        assert!(std::sync::Arc::ptr_eq(
            &image,
            &state.project_icon(100, 1, &mono, false).unwrap()
        ));
        let mut color = color_icon();
        let image = state.project_icon(100, 1, &color, true).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(&*image.rgba, &[255, 0, 0, 255, 0, 0, 0, 0]);
        assert!(!image.monochrome);
        assert!(image.is_valid());
        color[102..104].copy_from_slice(&0u16.to_be_bytes());
        color[104..106].copy_from_slice(&u16::MAX.to_be_bytes());
        let changed = state.project_icon(100, 1, &color, true).unwrap();
        assert_eq!(&changed.rgba[..4], &[0, 255, 0, 255]);
        assert!(!std::sync::Arc::ptr_eq(&image, &changed));
        assert!(state.project_icon(100, 1, &color[..84], true).is_none());
        state.retire_dialog(100);
        assert!(state.artwork.borrow().is_empty());
    }
}

#[cfg(test)]
mod alert_artwork_tests {
    use super::*;
    #[test]
    fn built_in_alert_artwork_is_distinct_cached_and_replaced_by_application_artwork() {
        let mut state = HostUiState::default();
        state.register_dialog(100);
        let images: Vec<_> = (0..=2)
            .map(|id| state.project_alert_icon(100, 1, id).unwrap())
            .collect();
        for image in &images {
            assert!(image.is_valid());
            assert!(image.monochrome);
            assert!(image.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert!(image.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
            assert!(image
                .rgba
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255));
        }
        assert_ne!(images[0], images[1]);
        assert_ne!(images[1], images[2]);
        assert_ne!(images[0], images[2]);
        assert!(std::sync::Arc::ptr_eq(
            &images[2],
            &state.project_alert_icon(100, 1, 2).unwrap()
        ));
        assert!(state.project_alert_icon(100, 1, 300).is_none());
        let actual = state.project_icon(100, 1, &[0x80; 128], false).unwrap();
        assert_eq!((actual.width, actual.height), (32, 32));
        assert_ne!(actual, images[2]);
        state.retire_dialog(100);
        assert!(state.artwork.borrow().is_empty());
    }
}

#[cfg(test)]
pub(crate) mod picture_artwork_tests {
    use super::*;
    pub(crate) fn picture() -> Vec<u8> {
        let mut bytes = vec![0, 0];
        for n in [0_i16, 0, 32, 32] {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.extend_from_slice(&[0x11, 1, 0x1a, 255, 255, 0, 0, 0, 0, 0x0a]);
        bytes.extend_from_slice(&[255; 8]);
        bytes.push(0x34);
        for n in [0_i16, 0, 32, 16] {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.push(255);
        let len = bytes.len() as u16;
        bytes[..2].copy_from_slice(&len.to_be_bytes());
        bytes
    }
    fn picture_v2() -> Vec<u8> {
        let mut bytes = vec![0, 0];
        for n in [0_i16, 0, 32, 32] {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.extend_from_slice(&[0, 0x11, 2, 255, 0, 0x1a, 255, 255, 0, 0, 0, 0, 0, 0x0a]);
        bytes.extend_from_slice(&[255; 8]);
        bytes.extend_from_slice(&[0, 0x34]);
        for n in [0_i16, 0, 32, 16] {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.extend_from_slice(&[0, 255]);
        let len = bytes.len() as u16;
        bytes[..2].copy_from_slice(&len.to_be_bytes());
        bytes
    }
    #[test]
    fn picture_projection_preserves_colour_transparency_scaling_and_cache_changes() {
        let mut state = HostUiState::default();
        state.register_dialog(100);
        let mut bytes = picture();
        let image = state.project_picture(100, 1, &bytes, (8, 4)).unwrap();
        assert_eq!(&image.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&image.rgba[16..20], &[0, 0, 0, 0]);
        assert!(!image.monochrome);
        let v2 = state
            .project_picture(100, 2, &picture_v2(), (8, 4))
            .unwrap();
        assert_eq!(image.rgba, v2.rgba);
        assert!(std::sync::Arc::ptr_eq(
            &image,
            &state.project_picture(100, 1, &bytes, (8, 4)).unwrap()
        ));
        let resized = state.project_picture(100, 1, &bytes, (4, 8)).unwrap();
        assert_eq!((resized.width, resized.height), (4, 8));
        assert!(!std::sync::Arc::ptr_eq(&image, &resized));
        bytes[13..17].copy_from_slice(&[0, 0, 255, 255]);
        let changed = state.project_picture(100, 1, &bytes, (8, 4)).unwrap();
        assert_eq!(&changed.rgba[..4], &[0, 255, 0, 255]);
        let mut white = picture();
        white[13..19].fill(255);
        let painted_white = state.project_picture(100, 3, &white, (8, 4)).unwrap();
        assert_eq!(&painted_white.rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(&painted_white.rgba[16..20], &[0, 0, 0, 0]);
        bytes.pop();
        assert!(state.project_picture(100, 1, &bytes, (8, 4)).is_none());
        assert!(state
            .project_picture(100, 1, &picture(), (1025, 4))
            .is_none());
        assert!(state.project_picture(100, 1, &picture(), (0, 4)).is_none());
        state.retire_dialog(100);
        assert!(state.artwork.borrow().is_empty());
    }
}

#[cfg(test)]
pub(crate) mod popup_tests {
    pub(crate) fn resources() -> (Vec<u8>, Vec<u8>) {
        let mut control = vec![0; 23];
        control[4..6].copy_from_slice(&20_i16.to_be_bytes());
        control[6..8].copy_from_slice(&120_i16.to_be_bytes());
        control[8..10].copy_from_slice(&1_i16.to_be_bytes());
        control[10] = 1;
        control[14..16].copy_from_slice(&300_i16.to_be_bytes());
        control[16..18].copy_from_slice(&1008_i16.to_be_bytes());
        let mut menu = vec![0; 15];
        menu[..2].copy_from_slice(&300_i16.to_be_bytes());
        menu[10..14].copy_from_slice(&19_u32.to_be_bytes());
        for text in ["Low", "Unavailable", "-", "High"] {
            menu.push(text.len() as u8);
            menu.extend_from_slice(text.as_bytes());
            menu.extend_from_slice(&[0; 4]);
        }
        menu.push(0);
        (control, menu)
    }
    #[test]
    fn accepting_the_current_popup_value_still_consumes_the_revision() {
        use super::*;
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        state.register_dialog(100);
        let snapshot = DialogSnapshot {
            movable: false,
            closable: false,
            id: state.dialog_identity(100).unwrap(),
            revision: 0,
            title: "Preferences".into(),
            size: (200, 100),
            items: Vec::new(),
            edit_item: 0,
            default_item: 1,
            cancel_item: 0,
            busy: false,
        };
        let before = state.project_dialog(snapshot.clone());
        state.invalidate_dialog_snapshot(100);
        let after = state.project_dialog(snapshot);
        assert_eq!(after.id, before.id);
        assert!(after.revision > before.revision);
    }
}

#[cfg(test)]
pub(crate) mod range_tests {
    use super::*;
    pub(crate) fn resource() -> Vec<u8> {
        let mut bytes = vec![0; 23];
        for (offset, value) in [(4, 16_i16), (6, 160), (12, 100), (14, -100), (16, 16)] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes[10] = 1;
        bytes
    }
    #[test]
    fn range_tracking_is_one_shot_and_expires_with_lifecycle_or_changed_source() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        state.register_dialog(100);
        state.present_dialog(100);
        state.stage_range_tracking(100, 200, 75, -100, 100);
        assert!(state.take_range_tracking(100, 200, 75, -100, 100));
        assert!(!state.take_range_tracking(100, 200, 75, -100, 100));
        state.stage_range_tracking(100, 200, 75, -100, 100);
        assert!(!state.take_range_tracking(100, 200, 76, -100, 100));
        state.stage_range_tracking(100, 200, 75, -100, 100);
        state.clear_range_tracking(100);
        assert!(!state.take_range_tracking(100, 200, 75, -100, 100));
        state.stage_range_tracking(100, 200, 75, -100, 100);
        state.retire_control(200);
        assert!(!state.take_range_tracking(100, 200, 75, -100, 100));
        state.stage_range_tracking(100, 200, 75, -100, 100);
        state.retire_dialog(100);
        state.register_dialog(100);
        state.present_dialog(100);
        assert!(!state.take_range_tracking(100, 200, 75, -100, 100));
    }
}

#[cfg(test)]
mod dialog_projection_tests {
    use super::*;
    fn view(id: u64) -> DialogSnapshot {
        DialogSnapshot {
            id,
            revision: 0,
            movable: true,
            closable: true,
            title: "Preferences".into(),
            size: (240, 160),
            items: Vec::new(),
            edit_item: 0,
            default_item: 1,
            cancel_item: 2,
            busy: false,
        }
    }
    #[test]
    fn interleaved_windows_keep_independent_revisions_and_retirement() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        let windows: Vec<_> = (100..110)
            .map(|pointer| {
                state.register_dialog(pointer);
                let id = state.dialog_identity(pointer).unwrap();
                (pointer, state.project_dialog(view(id)))
            })
            .collect();
        for _ in 0..20 {
            for (_, saved) in windows.iter().rev() {
                assert_eq!(state.project_dialog(saved.clone()), *saved);
            }
        }
        let (changed, before) = &windows[3];
        let mut edited = before.clone();
        edited.title = "Updated preferences".into();
        let edited = state.project_dialog(edited);
        assert!(edited.revision > before.revision);
        for (_, saved) in windows.iter().filter(|(pointer, _)| pointer != changed) {
            assert_eq!(state.project_dialog(saved.clone()), *saved);
        }
        state.invalidate_dialog_snapshot(*changed);
        assert!(state.project_dialog(edited.clone()).revision > edited.revision);
        let (retired, old) = &windows[5];
        state.retire_dialog(*retired);
        assert!(!state.projected_dialogs.contains_key(&old.id));
        state.register_dialog(*retired);
        assert_ne!(state.dialog_identity(*retired), Some(old.id));
        assert_eq!(state.projected_dialogs.len(), 9);
    }
    #[test]
    fn unprojected_modal_round_trips_invalidate_old_window_interactions() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        state.register_dialog(100);
        let id = state.dialog_identity(100).unwrap();
        let before = state.project_dialog(view(id));
        // Even a guest-rendered child, which publishes no host snapshot, must
        // expire interactions captured before the child owned the event loop.
        state.register_dialog(200);
        state.note_dialog_modal(200);
        state.retire_dialog(200);
        let after = state.project_dialog(before.clone());
        assert!(after.revision > before.revision);
        state.begin();
        state.finish();
        let file_return = state.project_dialog(after.clone());
        assert!(file_return.revision > after.revision);
        assert_eq!(state.project_dialog(file_return.clone()), file_return);
    }
    #[test]
    fn hiding_one_window_preserves_siblings_and_nested_file_restore_expires_replies() {
        let mut state = HostUiState {
            enabled: true,
            ..Default::default()
        };
        for pointer in [100, 200] {
            state.register_dialog(pointer);
            state.present_dialog(pointer);
        }
        let first = state.project_dialog(view(state.dialog_identity(100).unwrap()));
        let second = state.project_dialog(view(state.dialog_identity(200).unwrap()));
        state.invalidate_dialog_presentation(100);
        assert_eq!(state.project_dialog(second.clone()), second);
        let shown = state.project_dialog(first.clone());
        assert!(shown.revision > first.revision);
        state.unpresent_dialog(100);
        assert_eq!(state.project_dialog(second.clone()), second);
        state.begin();
        let parent = state.project_dialog(second.clone());
        let suspended = state.suspend_file();
        state.begin();
        state.finish();
        state.resume_file(suspended);
        assert!(state.project_dialog(parent.clone()).revision > parent.revision);
        state.register_dialog(200);
        assert!(!state.projected_dialogs.contains_key(&second.id));
    }
}
