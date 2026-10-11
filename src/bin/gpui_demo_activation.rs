//! Semantic control actions delivered through ordinary guest mouse tracking.

use super::frames::{control_activation_point, dialog_activation_point, Rect};
use systemless::systems::macintosh::session::{MacintoshInput, MacintoshSession};

pub struct ControlActivation {
    point: (i16, i16),
    origin: (i16, i16),
    released: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum FileAction {
    Accept,
    Cancel,
    Desktop,
    Replace,
    CancelReplacement,
    NewFolder,
    CreateFolder,
    CancelNewFolder,
    DismissFolderError,
}

impl ControlActivation {
    pub fn begin_file(
        session: &mut MacintoshSession,
        id: u32,
        generation: u64,
        action: FileAction,
    ) -> Option<Self> {
        use systemless::runner::StandardFileKind;
        if session.runner().guest_menu_tracking_active() {
            return None;
        }
        let panel = session.runner().standard_file_snapshot()?;
        if panel.guest_id != id || panel.generation != generation || !panel.standard_entry_point {
            return None;
        }
        // Standard File owns the modal loop and reply; semantic actions are clicks.
        // Inside Macintosh: Files (1992), pp. 3-3--3-13.
        let rect = if let Some(folder) = &panel.new_folder {
            match action {
                FileAction::DismissFolderError if folder.error.is_some() => folder.layout.create,
                FileAction::CreateFolder if folder.error.is_none() && !folder.name.is_empty() => folder.layout.create,
                FileAction::CancelNewFolder if folder.error.is_none() => folder.layout.cancel,
                _ => return None,
            }
        } else if panel.confirming_replace {
            let layout = systemless::runner::StandardFileReplacementLayout::new(panel.bounds);
            match action {
                FileAction::Replace => layout.replace,
                FileAction::CancelReplacement => layout.cancel,
                _ => return None,
            }
        } else {
            match panel.kind {
                StandardFileKind::Get => {
                    let layout = panel.get_layout.as_ref()?;
                    match action {
                        FileAction::Accept => {
                            let entry = panel.entries.as_ref()?.get(panel.selected?)?;
                            if !entry.is_directory && entry.file_type == 0 {
                                return None;
                            }
                            layout.open
                        }
                        FileAction::Cancel => layout.cancel,
                        FileAction::Desktop => layout.desktop,
                        _ => return None,
                    }
                }
                StandardFileKind::Put => {
                    let layout = panel.put_layout.as_ref()?;
                    match action {
                        FileAction::Accept => layout.save,
                        FileAction::Cancel => layout.cancel,
                        FileAction::Desktop => layout.desktop,
                        FileAction::NewFolder => layout.new_folder,
                        _ => return None,
                    }
                }
            }
        };
        let mode = session.runner().dispatcher().screen_mode;
        let rect = Rect::from(rect)
            .intersection(Rect::from(panel.bounds))?
            .intersection(Rect {
                top: 0,
                left: 0,
                bottom: i32::from(mode.3),
                right: i32::from(mode.2),
            })?;
        let point = (
            (rect.top + rect.height() / 2) as i16,
            (rect.left + rect.width() / 2) as i16,
        );
        let origin = session.runner().dispatcher().mouse_position();
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: point.0,
            horizontal: point.1,
        });
        Some(Self {
            point,
            origin,
            released: false,
        })
    }
    pub fn begin(session: &mut MacintoshSession, id: u32, generation: u64) -> Option<Self> {
        if session.runner().is_ui_tracking_active()
            || session.runner().guest_menu_tracking_active()
            || session
                .runner_mut()
                .dialog_snapshot()
                .iter()
                .any(|d| d.visible && d.active)
        {
            return None;
        }
        let controls = session.runner_mut().control_snapshot();
        let windows = session.runner_mut().window_frame_snapshot();
        let menus = session.runner_mut().guest_menu_snapshot();
        let mode = session.runner().dispatcher().screen_mode;
        let point = control_activation_point(
            id,
            generation,
            &controls,
            &menus,
            &windows,
            Rect {
                top: 0,
                left: 0,
                bottom: i32::from(mode.3),
                right: i32::from(mode.2),
            },
        )?;
        let origin = session.runner().dispatcher().mouse_position();
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: point.0,
            horizontal: point.1,
        });
        Some(Self {
            point,
            origin,
            released: false,
        })
    }

    pub fn begin_dialog(
        session: &mut MacintoshSession,
        id: u32,
        generation: u64,
        number: i16,
        identity: Option<(u32, u64)>,
    ) -> Option<Self> {
        // ModalDialog's waiting loop is the intended recipient, not a competing
        // gesture. The worker serializes presses; item tracking is checked below.
        if (session.runner().is_ui_tracking_active()
            && !session.runner().dispatcher().is_dialog_tracking())
            || session.runner().guest_menu_tracking_active()
        {
            return None;
        }
        let dialogs = session.runner_mut().dialog_snapshot();
        let windows = session.runner_mut().window_frame_snapshot();
        let mode = session.runner().dispatcher().screen_mode;
        let point = dialog_activation_point(
            id,
            generation,
            number,
            identity?,
            &dialogs,
            &windows,
            Rect {
                top: 0,
                left: 0,
                bottom: i32::from(mode.3),
                right: i32::from(mode.2),
            },
        )?;
        let origin = session.runner().dispatcher().mouse_position();
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: point.0,
            horizontal: point.1,
        });
        Some(Self {
            point,
            origin,
            released: false,
        })
    }

    // Each phase is separated by guest execution, just like physical input.
    // TrackControl owns callback invocation and the application owns values
    // (Macintosh Toolbox Essentials, pp. 5-57--5-59).
    pub fn advance(mut self, session: &mut MacintoshSession) -> Option<Self> {
        if !self.released {
            session.deliver_input(MacintoshInput::MouseUp {
                vertical: self.point.0,
                horizontal: self.point.1,
            });
            self.released = true;
            Some(self)
        } else {
            session.deliver_input(MacintoshInput::MouseMove {
                vertical: self.origin.0,
                horizontal: self.origin.1,
            });
            None
        }
    }
}
