//! Semantic control actions delivered through ordinary guest mouse tracking.

use super::frames::{control_activation_point, dialog_activation_point, Rect};
use systemless::systems::macintosh::session::{MacintoshInput, MacintoshSession};

pub struct ControlActivation {
    point: (i16, i16),
    origin: (i16, i16),
    released: bool,
    wait_logged: bool,
    dialog_target: Option<DialogActivationTarget>,
}

struct DialogActivationTarget {
    id: u32,
    generation: u64,
    number: i16,
    control_identity: (u32, u64),
    initial_value: Option<i16>,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    feature = "gpui-demo-test",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "gpui-demo-test", serde(rename_all = "snake_case"))]
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
    ScrollUp,
    ScrollDown,
    #[cfg_attr(feature = "gpui-demo-test", serde(skip))]
    Entry(usize),
}

fn file_scroll_arrow(rect: (i16, i16, i16, i16), action: FileAction) -> (i16, i16, i16, i16) {
    match action {
        FileAction::ScrollUp => (
            rect.0,
            rect.1,
            rect.0.saturating_add(16).min(rect.2),
            rect.3,
        ),
        _ => (
            rect.2.saturating_sub(16).max(rect.0),
            rect.1,
            rect.2,
            rect.3,
        ),
    }
}

fn file_entry_rect(
    list: (i16, i16, i16, i16),
    first: usize,
    visible: usize,
    height: i16,
    index: usize,
    count: usize,
) -> Option<(i16, i16, i16, i16)> {
    let row = index.checked_sub(first)?;
    if row >= visible || index >= count || height <= 0 {
        return None;
    }
    let top = i32::from(list.0)
        .checked_add(2)?
        .checked_add(i32::try_from(row).ok()?.checked_mul(i32::from(height))?)?;
    let bottom = top.checked_add(i32::from(height))?.min(i32::from(list.2));
    if top >= bottom {
        return None;
    }
    Some((
        i16::try_from(top).ok()?,
        list.1.saturating_add(2),
        i16::try_from(bottom).ok()?,
        list.3.saturating_sub(2),
    ))
}

impl ControlActivation {
    pub fn begin_file_entry(
        session: &mut MacintoshSession,
        id: u32,
        generation: u64,
        index: usize,
        expected: &systemless::runner::StandardFileEntrySnapshot,
    ) -> Option<Self> {
        let panel = session.runner().standard_file_snapshot()?;
        if panel.guest_id != id
            || panel.generation != generation
            || panel.entries.as_ref()?.get(index)? != expected
        {
            return None;
        }
        Self::begin_file(session, id, generation, FileAction::Entry(index))
    }

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
        if panel.guest_id != id
            || panel.generation != generation
            || !panel.standard_entry_point
            || panel.volume_popup.is_some()
        {
            return None;
        }
        // Standard File owns the modal loop and reply; semantic actions are clicks.
        // Inside Macintosh: Files (1992), pp. 3-3--3-13.
        let rect = if let Some(folder) = &panel.new_folder {
            match action {
                FileAction::DismissFolderError if folder.error.is_some() => folder.layout.create,
                FileAction::CreateFolder if folder.error.is_none() && !folder.name.is_empty() => {
                    folder.layout.create
                }
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
                        FileAction::ScrollUp | FileAction::ScrollDown => {
                            file_scroll_arrow(layout.scroll, action)
                        }
                        FileAction::Entry(index) => file_entry_rect(
                            layout.list,
                            layout.first_visible,
                            layout.visible_rows,
                            layout.row_height,
                            index,
                            panel.entries.as_ref()?.len(),
                        )?,
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
                        FileAction::ScrollUp | FileAction::ScrollDown => {
                            file_scroll_arrow(layout.scroll, action)
                        }
                        FileAction::Entry(index) => file_entry_rect(
                            layout.list,
                            layout.first_visible,
                            layout.visible_rows,
                            layout.row_height,
                            index,
                            panel.entries.as_ref()?.len(),
                        )?,
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
            wait_logged: false,
            dialog_target: None,
        })
    }
    pub fn begin(session: &mut MacintoshSession, id: u32, generation: u64) -> Option<Self> {
        if session.runner().standard_file_snapshot().is_some()
            || session.runner().is_ui_tracking_active()
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
            wait_logged: false,
            dialog_target: None,
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
        if session.runner().standard_file_snapshot().is_some()
            || (session.runner().is_ui_tracking_active()
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
        );
        if std::env::var_os("SYSTEMLESS_GPUI_PROFILE").is_some() {
            eprintln!("[GPUI-DIALOG-ACTIVATION] id={id} generation={generation} item={number} point={point:?}");
        }
        let point = point?;
        let origin = session.runner().dispatcher().mouse_position();
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: point.0,
            horizontal: point.1,
        });
        Some(Self {
            point,
            origin,
            released: false,
            wait_logged: false,
            dialog_target: Some(DialogActivationTarget {
                id,
                generation,
                number,
                control_identity: identity?,
                initial_value: dialogs
                    .iter()
                    .find(|dialog| dialog.guest_id == id && dialog.generation == generation)
                    .and_then(|dialog| dialog.items.iter().find(|item| item.number == number))
                    .and_then(|item| item.value),
            }),
        })
    }

    pub fn is_released(&self) -> bool {
        self.released
    }

    // Each phase is separated by guest execution, just like physical input.
    // TrackControl owns callback invocation and the application owns values
    // (Macintosh Toolbox Essentials, pp. 5-57--5-59).
    pub fn advance(mut self, session: &mut MacintoshSession) -> Option<Self> {
        if !self.released {
            // Release even if the event remains queued. Classic applications
            // may wait for Button() to clear before they consume mouseDown.
            session.deliver_input(MacintoshInput::MouseUp {
                vertical: self.point.0,
                horizontal: self.point.1,
            });
            self.released = true;
            self.wait_logged = false;
            Some(self)
        } else {
            // Keep the release inside the painted item until the guest finishes
            // tracking it. Restoring the pointer on the next publication can
            // turn an unprocessed inside release into an outside cancellation.
            let dialogs = session.runner_mut().dialog_snapshot();
            let target = self.dialog_target.as_ref().and_then(|target| {
                dialogs
                    .iter()
                    .find(|dialog| {
                        dialog.visible
                            && dialog.active
                            && dialog.guest_id == target.id
                            && dialog.generation == target.generation
                    })
                    .and_then(|dialog| {
                        dialog.items.iter().find(|item| {
                            item.visible
                                && item.number == target.number
                                && item.control_identity == Some(target.control_identity)
                        })
                    })
            });
            let awaiting_release = target.is_some_and(|item| item.pressed)
                || target.is_some_and(|item| {
                    self.dialog_target
                        .as_ref()
                        .is_some_and(|target| item.value == target.initial_value)
                        && session
                            .runner()
                            .event_manager_snapshot()
                            .queued_event_types
                            .contains(&1)
                });
            if awaiting_release {
                if !self.wait_logged && std::env::var_os("SYSTEMLESS_GPUI_PROFILE").is_some() {
                    eprintln!(
                        "[GPUI-ACTIVATION] awaiting inside release at {:?} tick={} pc={:08X} events={:?} tracking={:?}",
                        self.point,
                        session.runner().guest_tick(),
                        session.runner().cpu().read_reg(systemless::cpu::Register::PC),
                        session.runner().event_manager_snapshot().queued_event_types,
                        session.runner().dispatcher().dialog_tracking.as_ref().map(|tracking|
                            (tracking.draw_procs_done, tracking.active_button.is_some(), tracking.flash_remaining, tracking.flash_delay))
                    );
                }
                self.wait_logged = true;
                return Some(self);
            }
            session.deliver_input(MacintoshInput::MouseMove {
                vertical: self.origin.0,
                horizontal: self.origin.1,
            });
            None
        }
    }
}
