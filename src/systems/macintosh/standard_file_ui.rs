//! Read-only presentation state for retained Standard File panels.
//!
//! Standard File owns its modal event loop and reply record. The host may
//! observe this state, but input must continue through the guest event path.
//! Inside Macintosh: Files (1992), pp. 3-3--3-13, 3-44--3-47.

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardFileKind {
    Get,
    Put,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileEntrySnapshot {
    pub name: String,
    pub directory_id: u32,
    pub is_directory: bool,
    pub file_type: u32,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFilePutLayout {
    pub directory_label: (i16, i16, i16, i16),
    pub list: (i16, i16, i16, i16),
    pub scroll: (i16, i16, i16, i16),
    pub prompt: (i16, i16, i16, i16),
    pub name: (i16, i16, i16, i16),
    pub new_folder: (i16, i16, i16, i16),
    pub desktop: (i16, i16, i16, i16),
    pub cancel: (i16, i16, i16, i16),
    pub save: (i16, i16, i16, i16),
    pub row_height: i16,
    pub first_visible: usize,
    pub visible_rows: usize,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileGetLayout {
    pub volume: (i16, i16, i16, i16),
    pub directory_label: (i16, i16, i16, i16),
    pub list: (i16, i16, i16, i16),
    pub scroll: (i16, i16, i16, i16),
    pub eject: (i16, i16, i16, i16),
    pub desktop: (i16, i16, i16, i16),
    pub cancel: (i16, i16, i16, i16),
    pub open: (i16, i16, i16, i16),
    pub row_height: i16,
    pub first_visible: usize,
    pub visible_rows: usize,
}

impl StandardFilePutLayout {
    pub(crate) fn global_rect(
        bounds: (i16, i16, i16, i16),
        rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        (
            bounds.0.saturating_add(rect.0),
            bounds.1.saturating_add(rect.1),
            bounds.0.saturating_add(rect.2),
            bounds.1.saturating_add(rect.3),
        )
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileSnapshot {
    /// Address of the caller's reply record; pair with generation.
    pub guest_id: u32,
    /// Changes for each retained Standard File invocation and modal transition.
    pub generation: u64,
    pub kind: StandardFileKind,
    pub confirming_replace: bool,
    pub new_folder: Option<StandardFileNewFolderSnapshot>,
    /// True only for the modern standard entry points. This alone does not
    /// qualify a panel for an overlay; its guest behavior must also be complete.
    pub standard_entry_point: bool,
    pub bounds: (i16, i16, i16, i16),
    pub directory_id: u32,
    /// `None` means the current guest panel has no exposed file list.
    pub entries: Option<Vec<StandardFileEntrySnapshot>>,
    pub selected: Option<usize>,
    pub prompt: Option<String>,
    pub name: Option<String>,
    pub name_selection: Option<(usize, usize)>,
    /// `None` for Open panels; Save reports where guest keyboard input goes.
    pub name_has_focus: Option<bool>,
    pub directory_label: Option<String>,
    pub get_layout: Option<StandardFileGetLayout>,
    pub put_layout: Option<StandardFilePutLayout>,
}

/// Geometry and event interpretation shared by both Standard File backends.
/// Files (1992), p. 3-7: a name conflict requires a subsidiary confirmation.
#[doc(hidden)]
pub struct StandardFileReplacementLayout {
    pub bounds: (i16, i16, i16, i16),
    pub message: (i16, i16, i16, i16),
    pub cancel: (i16, i16, i16, i16),
    pub replace: (i16, i16, i16, i16),
}

impl StandardFileReplacementLayout {
    pub fn new(parent: (i16, i16, i16, i16)) -> Self {
        let top = parent.0 + (parent.2 - parent.0 - 110) / 2;
        let left = parent.1 + (parent.3 - parent.1 - 300) / 2;
        Self {
            bounds: (top, left, top + 110, left + 300),
            message: (top + 14, left + 16, top + 64, left + 284),
            cancel: (top + 76, left + 104, top + 98, left + 184),
            replace: (top + 76, left + 202, top + 98, left + 282),
        }
    }

    /// None keeps the confirmation open; false returns to the parent Save panel.
    pub(crate) fn action(
        &self,
        what: u16,
        message: u32,
        modifiers: u16,
        v: i16,
        h: i16,
    ) -> Option<bool> {
        if what == 1 {
            let contains = |r: (i16, i16, i16, i16)| v >= r.0 && v < r.2 && h >= r.1 && h < r.3;
            if contains(self.cancel) {
                return Some(false);
            }
            if contains(self.replace) {
                return Some(true);
            }
        } else if what == 3 || what == 5 {
            let character = message as u8;
            if character == 27 || (character == b'.' && modifiers & 0x100 != 0) {
                return Some(false);
            }
            // Mac OS 8.1 replacement alerts default to Cancel (BasiliskII and
            // SheepShaver replay); Return/Enter invoke that default, not Replace.
            if character == 13 || character == 3 {
                return Some(false);
            }
        }
        None
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileNewFolderSnapshot {
    pub name: String,
    pub selection: (usize, usize),
    pub error: Option<i16>,
    pub layout: StandardFileNewFolderLayout,
}

/// Shared guest-coordinate geometry for the Standard File New Folder dialog.
/// Files (1992), pp. 3-6–3-7. Keep the subsidiary panel within the retained
/// parent's saved region so dismissing it restores the scene on either CPU.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileNewFolderLayout {
    pub bounds: (i16, i16, i16, i16),
    pub prompt: (i16, i16, i16, i16),
    pub name: (i16, i16, i16, i16),
    pub cancel: (i16, i16, i16, i16),
    pub create: (i16, i16, i16, i16),
}

impl StandardFileNewFolderLayout {
    pub fn new(parent: (i16, i16, i16, i16)) -> Self {
        let top = parent.0 + (parent.2 - parent.0 - 102) / 2;
        let left = parent.1 + (parent.3 - parent.1 - 216) / 2;
        Self {
            bounds: (top, left, top + 102, left + 216),
            prompt: (top + 12, left + 16, top + 30, left + 198),
            name: (top + 36, left + 16, top + 56, left + 198),
            cancel: (top + 66, left + 16, top + 88, left + 76),
            create: (top + 66, left + 136, top + 88, left + 198),
        }
    }

    pub(crate) fn mouse_action(&self, v: i16, h: i16) -> Option<StandardFileNewFolderAction> {
        let contains = |r: (i16, i16, i16, i16)| v >= r.0 && v < r.2 && h >= r.1 && h < r.3;
        if contains(self.cancel) {
            Some(StandardFileNewFolderAction::Cancel)
        } else if contains(self.create) {
            Some(StandardFileNewFolderAction::Create)
        } else {
            None
        }
    }
}

/// Guest-owned edit state for the subsidiary New Folder dialog.
/// Files (1992), pp. 3-6–3-7; Mac OS 8.1 oracle captures establish the initially
/// selected name and Create default. Bytes and selection offsets are Mac Roman.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StandardFileNewFolderState {
    pub(crate) edit: crate::text_edit::TextEditBuffer,
    pub(crate) error: Option<i16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StandardFileNewFolderAction {
    Create,
    Cancel,
}

impl Default for StandardFileNewFolderState {
    fn default() -> Self {
        let name = b"untitled folder".to_vec();
        let end = name.len();
        Self {
            edit: crate::text_edit::TextEditBuffer::new(name, 0, end),
            error: None,
        }
    }
}

impl StandardFileNewFolderState {
    pub(crate) fn snapshot(&self, parent: (i16, i16, i16, i16)) -> StandardFileNewFolderSnapshot {
        let selection = self.edit.selection();
        StandardFileNewFolderSnapshot {
            name: crate::trap::types::decode_mac_roman(self.edit.text()),
            selection: (selection.start, selection.end),
            error: self.error,
            layout: StandardFileNewFolderLayout::new(parent),
        }
    }

    /// Only the active subsidiary dialog may interpret these modal events.
    /// Empty names disable Create for pointer and keyboard activation equally.
    pub(crate) fn event(
        &mut self,
        layout: &StandardFileNewFolderLayout,
        what: u16,
        message: u32,
        modifiers: u16,
        point: (i16, i16),
        scrap: &mut Vec<u8>,
    ) -> Option<StandardFileNewFolderAction> {
        match what {
            1 => layout.mouse_action(point.0, point.1).filter(|action| {
                *action != StandardFileNewFolderAction::Create || !self.edit.text().is_empty()
            }),
            3 | 5 => self.key(message, modifiers, scrap),
            _ => None,
        }
    }

    /// Apply guest keyDown/autoKey input. The caller owns the process TextEdit
    /// scrap; no host clipboard or guest reply record is changed here.
    pub(crate) fn key(
        &mut self,
        message: u32,
        modifiers: u16,
        scrap: &mut Vec<u8>,
    ) -> Option<StandardFileNewFolderAction> {
        let character = message as u8;
        let key_code = (message >> 8) as u8;
        let command = modifiers & 0x0100 != 0;
        if character == 27 || key_code == 0x35 || (command && character == b'.') {
            return Some(StandardFileNewFolderAction::Cancel);
        }
        if crate::dialog_manager::is_dialog_default_key(character, key_code) {
            return (!self.edit.text().is_empty()).then_some(StandardFileNewFolderAction::Create);
        }
        if command {
            match character.to_ascii_lowercase() {
                b'a' => {
                    let text = self.edit.text().to_vec();
                    let end = text.len();
                    self.edit = crate::text_edit::TextEditBuffer::new(text, 0, end);
                }
                b'c' | b'x' => {
                    if !self.edit.selection().is_empty() {
                        *scrap = self.edit.selected_text().to_vec();
                        if character.eq_ignore_ascii_case(&b'x') {
                            self.edit.delete_selection();
                        }
                    }
                }
                b'v' => self.insert_name_bytes(scrap),
                _ => {}
            }
            return None;
        }
        match character {
            8 | 0x7f | 0x1c | 0x1d => self.edit.apply_key(character),
            0x20..=0xff => self.insert_name_bytes(&[character]),
            _ => {}
        }
        None
    }

    fn insert_name_bytes(&mut self, bytes: &[u8]) {
        // Files (1992), "Names and Pathnames": HFS names contain at most
        // 31 characters and cannot contain colons. Count Mac Roman bytes, not
        // UTF-8 bytes; do not reinterpret a slash as a directory separator here.
        let retained = self.edit.text().len() - self.edit.selection().len();
        let available = 31usize.saturating_sub(retained);
        let inserted: Vec<u8> = bytes
            .iter()
            .copied()
            .filter(|byte| *byte >= 0x20 && *byte != 0x7f && *byte != b':')
            .take(available)
            .collect();
        if !inserted.is_empty() {
            self.edit.replace_selection(&inserted);
        }
    }
}

#[cfg(test)]
mod new_folder_tests {
    use super::{StandardFileNewFolderAction as Action, StandardFileNewFolderState};

    #[test]
    fn new_folder_modal_geometry_and_pointer_defaults() {
        let parent = (90, 140, 350, 500);
        let layout = super::StandardFileNewFolderLayout::new(parent);
        assert!(layout.bounds.0 >= parent.0 && layout.bounds.1 >= parent.1);
        assert!(layout.bounds.2 <= parent.2 && layout.bounds.3 <= parent.3);
        let shifted = super::StandardFileNewFolderLayout::new((110, 170, 370, 530));
        assert_eq!(
            shifted.name,
            (
                layout.name.0 + 20,
                layout.name.1 + 30,
                layout.name.2 + 20,
                layout.name.3 + 30
            )
        );
        let mut state = StandardFileNewFolderState::default();
        let mut scrap = Vec::new();
        let create = (layout.create.0 + 1, layout.create.1 + 1);
        let cancel = (layout.cancel.0 + 1, layout.cancel.1 + 1);
        assert_eq!(
            state.event(&layout, 1, 0, 0, create, &mut scrap),
            Some(Action::Create)
        );
        assert_eq!(state.event(&layout, 2, 0, 0, create, &mut scrap), None);
        assert_eq!(
            state.event(&layout, 1, 0, 0, (parent.0, parent.1), &mut scrap),
            None
        );
        state.key(8, 0, &mut scrap);
        assert_eq!(state.event(&layout, 1, 0, 0, create, &mut scrap), None);
        assert_eq!(state.event(&layout, 5, 13, 0, create, &mut scrap), None);
        assert_eq!(
            state.event(&layout, 1, 0, 0, cancel, &mut scrap),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn new_folder_native_initial_selection_and_default() {
        let mut state = StandardFileNewFolderState::default();
        let mut scrap = Vec::new();
        assert_eq!(state.edit.selected_text(), b"untitled folder");
        for byte in b"gpui folder" {
            assert_eq!(state.key(u32::from(*byte), 0, &mut scrap), None);
        }
        assert_eq!(state.edit.text(), b"gpui folder");
        assert_eq!(state.edit.selection(), 11..11);
        assert_eq!(state.key(13, 0, &mut scrap), Some(Action::Create));
        assert_eq!(state.key(3, 0, &mut scrap), Some(Action::Create));
        assert_eq!(state.key(27, 0, &mut scrap), Some(Action::Cancel));
        assert_eq!(
            state.key(u32::from(b'.'), 0x100, &mut scrap),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn new_folder_mac_roman_selection_clipboard_and_name_limit() {
        let mut state = StandardFileNewFolderState::default();
        let mut scrap = vec![0x8e; 40];
        state.key(u32::from(b'v'), 0x100, &mut scrap);
        assert_eq!(state.edit.text(), &[0x8e; 31]);
        state.key(u32::from(b'x'), 0, &mut scrap);
        assert_eq!(state.edit.text().len(), 31);
        state.key(u32::from(b'a'), 0x100, &mut scrap);
        state.key(u32::from(b'x'), 0x100, &mut scrap);
        assert_eq!(scrap, vec![0x8e; 31]);
        assert!(state.edit.text().is_empty());
        assert_eq!(state.key(13, 0, &mut scrap), None);
        state.key(u32::from(b':'), 0, &mut scrap);
        assert!(state.edit.text().is_empty());
        state.key(u32::from(b'/'), 0, &mut scrap);
        assert_eq!(state.edit.text(), b"/");
        state.key(0x1c, 0, &mut scrap);
        state.key(0x8e, 0, &mut scrap);
        assert_eq!(state.edit.text(), &[0x8e, b'/']);
        state.key(8, 0, &mut scrap);
        assert_eq!(state.edit.text(), b"/");
    }
}
