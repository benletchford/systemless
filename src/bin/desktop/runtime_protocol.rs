//! Owned host/guest boundary. Commands are applied by the execution owner;
//! snapshots contain no references to the runner or Macintosh memory.

use std::sync::Arc;
use systemless::systems::macintosh::display::CursorImage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuiCommand {
    MouseMove { v: i16, h: i16 },
    MouseDown { v: i16, h: i16 },
    MouseUp { v: i16, h: i16 },
    KeyDown { key: u8, character: u8 },
    KeyUp { key: u8, character: u8 },
    Menu { menu: i16, item: i16 },
    Dialog(systemless::systems::macintosh::host_ui::DialogReply),
    DialogControl(systemless::systems::macintosh::host_ui::DialogControlReply),
    DialogText(systemless::systems::macintosh::host_ui::DialogTextReply),
    FileDialog(systemless::systems::macintosh::host_ui::FileDialogReply),
    Modeless(ModelessCommand),
    PopupMenu(systemless::systems::macintosh::host_ui::PopupMenuReply),
    SystemDialog(systemless::systems::macintosh::host_ui::SystemDialogReply),
    DismissNotification(u64),
    SetBalloonHelp(bool),
    AcknowledgeWarp { generation: u64, serial: u64 },
}

/// Intents for independent Macintosh dialogs. Each names an opaque identity
/// and the revision it was decided against; the runtime rejects stale ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ModelessCommand {
    Activate { id: u64, revision: u64 },
    Item(systemless::systems::macintosh::host_ui::DialogReply),
    Control(systemless::systems::macintosh::host_ui::DialogControlReply),
    Text(systemless::systems::macintosh::host_ui::DialogTextReply),
    Close { id: u64, revision: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CursorWarp {
    pub serial: u64,
    pub position: (i16, i16),
}

#[derive(Default)]
pub(super) struct GuiState {
    pub generation: u64,
    pub modality: systemless::systems::macintosh::host_ui::UiModality,
    pub screen_mode: (u32, u32, u16, u16, u16),
    pub cursor: Option<CursorImage>,
    pub warp: Option<CursorWarp>,
    #[cfg(target_os = "macos")]
    pub dialog_bounds: Option<(i16, i16, i16, i16)>,
    #[cfg(target_os = "macos")]
    pub hidden_menu_height: u32,
    #[cfg(target_os = "macos")]
    pub identity: Option<Arc<systemless::systems::macintosh::game::ApplicationIdentity>>,
    /// Command sequence, dialog identity, revision, accepted.
    pub file_reply: Option<(u64, u64, u64, bool)>,
    pub dialog_text_replies: Vec<Arc<DialogTextAck>>,
    /// Includes live parents obscured by a child or temporarily rendered by the guest.
    pub live_dialogs: Vec<u64>,
    pub dialog: Option<Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>>,
    /// Projected modeless dialogs in guest front-to-back order.
    pub modeless_dialogs: Vec<Arc<systemless::systems::macintosh::host_ui::DialogSnapshot>>,
    pub file_dialog: Option<Arc<systemless::systems::macintosh::host_ui::FileDialogSnapshot>>,
    pub popup_menu: Option<systemless::systems::macintosh::host_ui::PopupMenuSnapshot>,
    pub system_dialog: Option<systemless::systems::macintosh::host_ui::SystemDialogSnapshot>,
    pub notifications: Vec<systemless::systems::macintosh::host_ui::NotificationSnapshot>,
    pub balloon: Option<systemless::systems::macintosh::host_ui::BalloonSnapshot>,
    pub balloon_help: bool,
    pub guest_surface: Option<systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot>,
    /// Guest-drawn modeless dialogs framed by the host, front to back.
    pub guest_windows: Vec<systemless::systems::macintosh::host_ui::GuestSurfaceSnapshot>,
    /// Set once when the guest halts for any reason other than ExitToShell.
    pub stopped: Option<String>,
    pub menus: Option<Arc<systemless::systems::macintosh::menu_model::GuestMenuSnapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DialogTextAck {
    pub serial: u64,
    pub id: u64,
    pub accepted: bool,
    /// Canonical text immediately after acceptance, before the guest runs again.
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_snapshots_can_cross_the_owner_boundary() {
        fn send<T: Send>() {}
        send::<GuiCommand>();
        send::<GuiState>();
        send::<crate::frame_snapshot::GuiFrame>();
    }
}
