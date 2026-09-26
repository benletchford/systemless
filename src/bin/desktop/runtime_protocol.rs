//! Owned host/guest boundary. Commands are applied by the execution owner;
//! snapshots contain no references to the runner or Macintosh memory.

#[cfg(target_os = "macos")]
use std::sync::Arc;
use systemless::display::CursorImage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuiCommand {
    MouseMove { v: i16, h: i16 },
    MouseDown { v: i16, h: i16 },
    MouseUp { v: i16, h: i16 },
    KeyDown { key: u8, character: u8 },
    KeyUp { key: u8, character: u8 },
    Menu { menu: i16, item: i16 },
    AcknowledgeWarp { generation: u64, serial: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CursorWarp {
    pub serial: u64,
    pub position: (i16, i16),
}

#[derive(Default)]
pub(super) struct GuiState {
    pub generation: u64,
    pub screen_mode: (u32, u32, u16, u16, u16),
    pub cursor: Option<CursorImage>,
    pub warp: Option<CursorWarp>,
    #[cfg(target_os = "macos")]
    pub dialog_bounds: Option<(i16, i16, i16, i16)>,
    #[cfg(target_os = "macos")]
    pub hidden_menu_height: u32,
    #[cfg(target_os = "macos")]
    pub identity: Option<Arc<systemless::game::ApplicationIdentity>>,
    #[cfg(target_os = "macos")]
    pub menus: Option<Arc<systemless::menu_model::GuestMenuSnapshot>>,
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
