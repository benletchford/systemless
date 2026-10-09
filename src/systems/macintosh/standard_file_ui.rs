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
    /// Horizontal inset and baseline relative to each guest list row.
    pub list_text_origin: (i16, i16),
    pub directory_marker: &'static str,
    /// Guest painter character limit; full entry names remain canonical.
    pub list_name_limit: Option<usize>,
    /// Guest Open volume text and origin relative to its popup rectangle.
    pub volume_text: Option<(String, (i16, i16))>,
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
    pub name_text_layout: Option<StandardFileNameTextLayout>,
    pub name_caret_visible: Option<bool>,
    /// `None` for Open panels; Save reports where guest keyboard input goes.
    pub name_has_focus: Option<bool>,
    pub directory_label: Option<String>,
    /// Directory statText font family, raw size and QuickDraw face.
    pub directory_font: (i16, i16, u8),
    /// Horizontal inset, first baseline and line spacing within its rectangle.
    pub directory_text_layout: (i16, i16, i16),
    pub get_layout: Option<StandardFileGetLayout>,
    pub put_layout: Option<StandardFilePutLayout>,
}

/// Insertion blink state driven by guest ticks and CaretTime, shared by Pack3 gateways.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StandardFileCaret {
    tick: Option<u32>,
    pub(crate) on: bool,
}
impl StandardFileCaret {
    pub(crate) fn reset(&mut self, tick: u32) {
        self.tick = Some(tick);
        self.on = true;
    }
    pub(crate) fn idle(&mut self, tick: u32, interval: u32, active: bool) -> bool {
        if !active {
            let changed = self.on;
            self.on = false;
            self.tick = None;
            return changed;
        }
        let Some(previous) = self.tick else {
            self.reset(tick);
            return true;
        };
        if tick.wrapping_sub(previous) < interval {
            return false;
        }
        self.tick = Some(tick);
        self.on = !self.on;
        true
    }
}

/// Guest filename painter geometry, relative to the Save name rectangle.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileNameTextLayout {
    pub font: (i16, i16, u8),
    pub origin: (i16, i16),
    pub selection_top: i16,
    pub selection_height: i16,
    pub selection_to_edge: bool,
    pub wraps: bool,
}

impl StandardFileNameTextLayout {
    pub(crate) fn powerpc() -> Self {
        Self { font: (0, 0, 0), origin: (0, 14), selection_top: 2,
            selection_height: 16, selection_to_edge: false, wraps: true }
    }
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
    /// Global guest x positions for each Mac Roman insertion offset, including EOF.
    pub insertion_positions: Vec<i16>,
    /// Byte offset whose caret or moving selection endpoint should remain visible.
    pub visible_offset: usize,
    pub caret_visible: bool,
    pub selection: (usize, usize),
    pub error: Option<i16>,
    pub layout: StandardFileNewFolderLayout,
}

impl StandardFileNewFolderSnapshot {
    pub fn prompt(&self) -> &'static str {
        new_folder_prompt(self.error)
    }
}

fn new_folder_prompt(error: Option<i16>) -> &'static str {
    match error {
        Some(-48) => "That name is already taken; please use another name.",
        Some(-44 | -46) => "The disk is locked.",
        Some(-37) => "The folder name is invalid.",
        Some(_) => "The folder could not be created.",
        None => "Name of new folder:",
    }
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

    pub fn error_message(&self) -> (i16, i16, i16, i16) {
        (self.prompt.0, self.prompt.1, self.name.2, self.prompt.3)
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
    pointer_anchor: Option<usize>,
    pub(crate) scroll_x: i16,
    visible_offset: usize,
    caret_tick: Option<u32>,
    caret_on: bool,
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
            pointer_anchor: None,
            scroll_x: 0,
            visible_offset: 0,
            caret_tick: None,
            caret_on: true,
        }
    }
}

/// TEClick chooses the nearest Mac Roman insertion boundary using guest advances.
pub(crate) fn classic_text_offset_at_x(text: &[u8], x: i32, measure: impl Fn(&[u8]) -> i32) -> usize {
    let mut left = 0;
    for end in 1..=text.len() {
        let right = measure(&text[..end]);
        if i64::from(x) * 2 < i64::from(left) + i64::from(right) {
            return end - 1;
        }
        left = right;
    }
    text.len()
}

impl StandardFileNewFolderState {
    pub(crate) fn caret_visible(&self) -> bool {
        self.error.is_none() && self.edit.selection().is_empty() && self.caret_on
    }

    pub(crate) fn reset_caret(&mut self, tick: u32) {
        self.caret_tick = Some(tick);
        self.caret_on = true;
    }

    /// TEIdle, Text (1993), p. 2-84: only active insertion points blink,
    /// using the guest CaretTime interval (Toolbox Essentials, p. 2-113).
    pub(crate) fn idle(&mut self, tick: u32, caret_time: u32) -> bool {
        if self.error.is_some() || !self.edit.selection().is_empty() || self.is_selecting() {
            return false;
        }
        let Some(previous) = self.caret_tick else {
            self.reset_caret(tick);
            return false;
        };
        if tick.wrapping_sub(previous) < caret_time { return false; }
        self.caret_tick = Some(tick);
        self.caret_on = !self.caret_on;
        true
    }

    pub(crate) fn offset_at_x(&self, x: i32, measure: impl Fn(&[u8]) -> i32) -> usize {
        classic_text_offset_at_x(self.edit.text(), x.saturating_add(i32::from(self.scroll_x)), measure)
    }

    /// TESelView keeps the selection start visible; held selection tracking
    /// reveals its moving endpoint (Text 1993, TEAutoView/TESelView, pp. 2-99–2-100).
    pub(crate) fn reveal_offset(&mut self, offset: usize, width: i16, measure: impl Fn(&[u8]) -> i16) -> bool {
        self.visible_offset = offset.min(self.edit.text().len());
        let width = width.max(1);
        let x = measure(&self.edit.text()[..offset.min(self.edit.text().len())]);
        let max_scroll = measure(self.edit.text()).saturating_sub(width - 1).max(0);
        let previous = self.scroll_x;
        self.scroll_x = self.scroll_x.min(x).max(x.saturating_sub(width - 1)).clamp(0, max_scroll);
        self.scroll_x != previous
    }

    /// The CPU adapter resolves guest font metrics to a Mac Roman byte offset.
    /// TEClick retains an anchor until release (Text, 1993, p. 2-85).
    pub(crate) fn begin_selection(&mut self, offset: usize, extend: bool) {
        if self.error.is_some() {
            return;
        }
        let offset = offset.min(self.edit.text().len());
        let selection = self.edit.selection();
        let anchor = if extend {
            if offset < selection.start { selection.end } else { selection.start }
        } else {
            offset
        };
        self.pointer_anchor = Some(anchor);
        self.track_selection(offset, true);
    }

    /// Returns whether selection changed; release preserves the final range.
    pub(crate) fn track_selection(&mut self, offset: usize, button_down: bool) -> bool {
        let Some(anchor) = self.pointer_anchor else { return false; };
        let offset = offset.min(self.edit.text().len());
        let old = self.edit.selection();
        let selection = anchor.min(offset)..anchor.max(offset);
        if old != selection {
            self.edit = crate::text_edit::TextEditBuffer::new(
                self.edit.text().to_vec(), selection.start, selection.end,
            );
        }
        if !button_down {
            self.pointer_anchor = None;
        }
        old != selection
    }

    pub(crate) fn is_selecting(&self) -> bool {
        self.pointer_anchor.is_some()
    }

    pub(crate) fn prompt(&self) -> &'static str {
        new_folder_prompt(self.error)
    }

    pub(crate) fn snapshot(
        &self, parent: (i16, i16, i16, i16), inset: i16,
        measure: impl Fn(&[u8]) -> i16,
    ) -> StandardFileNewFolderSnapshot {
        let selection = self.edit.selection();
        let layout = StandardFileNewFolderLayout::new(parent);
        // Text (1993), CharToPixel: insertion positions correspond to source
        // buffer offsets. Preserve guest metrics for themed pointer translation.
        let insertion_positions = (0..=self.edit.text().len())
            .map(|offset| layout.name.1.saturating_add(inset)
                .saturating_add(measure(&self.edit.text()[..offset]))
                .saturating_sub(self.scroll_x))
            .collect();
        StandardFileNewFolderSnapshot {
            insertion_positions,
            visible_offset: self.visible_offset,
            caret_visible: self.caret_visible(),
            name: crate::trap::types::decode_mac_roman(self.edit.text()),
            selection: (selection.start, selection.end),
            error: self.error,
            layout,
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
        // Native Mac OS 8.1 Standard File closes the editor on failure and
        // shows a one-button alert. Its OK returns to Save, not the editor.
        if self.error.is_some() {
            return match what {
                1 if layout.mouse_action(point.0, point.1)
                    == Some(StandardFileNewFolderAction::Create) =>
                {
                    Some(StandardFileNewFolderAction::Cancel)
                }
                3 | 5 => self.key(message, modifiers, scrap),
                _ => None,
            };
        }
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
        if self.error.is_some() {
            return crate::dialog_manager::is_dialog_default_key(character, key_code)
                .then_some(StandardFileNewFolderAction::Cancel);
        }
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
            // Text (1993), "Caret Position and Movement": on the first/last
            // line Up/Down moves to the beginning/end. This field is one line.
            0x1e | 0x1f => {
                let text = self.edit.text().to_vec();
                let caret = if character == 0x1e { 0 } else { text.len() };
                self.edit = crate::text_edit::TextEditBuffer::new(text, caret, caret);
            }
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
    fn new_folder_hit_testing_uses_guest_glyph_midpoints() {
        let mut state = StandardFileNewFolderState::default();
        state.edit = crate::text_edit::TextEditBuffer::new(vec![b'i', 0x8e, b'W'], 0, 0);
        let measure = |bytes: &[u8]| bytes.iter().map(|byte| match byte {
            b'i' => 5, 0x8e => 9, _ => 12,
        }).sum();
        for (x, offset) in [(-100, 0), (0, 0), (2, 0), (3, 1), (9, 1), (10, 2), (19, 2), (20, 3), (100, 3)] {
            assert_eq!(state.offset_at_x(x, measure), offset, "x={x}");
        }
        state.edit = crate::text_edit::TextEditBuffer::new(Vec::new(), 0, 0);
        assert_eq!(state.offset_at_x(100, measure), 0);
    }

    #[test]
    fn new_folder_caret_uses_guest_idle_ticks_and_resets_after_input() {
        let mut state = StandardFileNewFolderState::default();
        assert!(!state.caret_visible());
        state.begin_selection(3, false);
        state.track_selection(3, false);
        state.reset_caret(100);
        assert!(state.caret_visible());
        assert!(!state.idle(131, 32));
        assert!(state.idle(132, 32));
        assert!(!state.caret_visible());
        assert!(!state.idle(132, 32));
        assert!(state.idle(164, 32));
        state.reset_caret(u32::MAX - 15);
        assert!(!state.idle(15, 32));
        assert!(state.idle(16, 32));
        state.reset_caret(17);
        assert!(state.caret_visible());
        state.begin_selection(3, false);
        assert!(!state.idle(100, 32), "held tracking must not blink");
        state.track_selection(5, false);
        assert!(!state.idle(200, 32));
        assert!(!state.caret_visible());
        state.error = Some(-48);
        assert!(!state.idle(300, 32));
    }

    #[test]
    fn new_folder_caret_observes_changed_guest_interval() {
        let mut state = StandardFileNewFolderState::default();
        state.begin_selection(3, false);
        state.track_selection(3, false);
        state.reset_caret(100);
        assert!(!state.idle(163, 64));
        assert!(state.caret_visible());
        assert!(state.idle(164, 64));
        assert!(!state.caret_visible());
        assert!(!state.idle(168, 5));
        assert!(state.idle(169, 5));
        assert!(state.caret_visible());
        state.reset_caret(u32::MAX - 2);
        assert!(!state.idle(1, 5));
        assert!(state.idle(2, 5));
    }

    #[test]
    fn new_folder_scroll_retains_visible_caret_and_translates_hit_testing() {
        let mut state = StandardFileNewFolderState::default();
        state.edit = crate::text_edit::TextEditBuffer::new(vec![b'w'; 31], 31, 31);
        let measure = |bytes: &[u8]| bytes.len() as i16 * 9;
        assert!(state.reveal_offset(31, 180, measure));
        assert_eq!(state.scroll_x, 100);
        assert_eq!(state.offset_at_x(179, |bytes| i32::from(measure(bytes))), 31);
        assert!(!state.reveal_offset(30, 180, measure), "visible caret must not move the text");
        assert!(state.reveal_offset(0, 180, measure));
        assert_eq!(state.scroll_x, 0);
        assert!(state.reveal_offset(31, 180, measure));
        state.edit = crate::text_edit::TextEditBuffer::new(b"short".to_vec(), 5, 5);
        assert!(state.reveal_offset(5, 180, measure));
        assert_eq!(state.scroll_x, 0, "deletion must remove obsolete scroll");
    }

    #[test]
    fn new_folder_snapshot_positions_round_trip_mac_roman_offsets() {
        let mut state = StandardFileNewFolderState::default();
        state.edit = crate::text_edit::TextEditBuffer::new(vec![b'i', 0x8e, b'W'], 1, 2);
        let measure = |bytes: &[u8]| -> i16 {
            bytes.iter().map(|byte| match byte { b'i' => 5, 0x8e => 9, _ => 12 }).sum()
        };
        for inset in [1, 2] {
            let snapshot = state.snapshot((100, 100, 360, 460), inset, measure);
            assert_eq!(snapshot.name, "iéW");
            assert_eq!(snapshot.selection, (1, 2));
            assert_eq!(snapshot.insertion_positions.len(), 4);
            for (offset, x) in snapshot.insertion_positions.iter().enumerate() {
                assert_eq!(state.offset_at_x(
                    i32::from(*x - snapshot.layout.name.1 - inset),
                    |bytes| i32::from(measure(bytes)),
                ), offset);
            }
        }
    }

    #[test]
    fn new_folder_pointer_selection_retains_anchor_until_release() {
        let mut state = StandardFileNewFolderState::default();
        let mut scrap = Vec::new();
        state.begin_selection(usize::MAX, false);
        assert_eq!(state.edit.selection(), 15..15);
        assert!(state.is_selecting());
        state.track_selection(15, false);
        assert!(!state.is_selecting());
        state.key(b'x' as u32, 0, &mut scrap);
        assert_eq!(state.edit.text(), b"untitled folderx");
        state.begin_selection(16, false);
        assert!(state.track_selection(4, true));
        assert_eq!(state.edit.selection(), 4..16);
        assert!(state.track_selection(0, false));
        assert_eq!(state.edit.selection(), 0..16);
        assert!(!state.track_selection(8, false));
        state.key(b'a' as u32, 0, &mut scrap);
        assert_eq!(state.edit.text(), b"a");
    }

    #[test]
    fn new_folder_shift_click_extends_and_errors_block_selection() {
        let mut state = StandardFileNewFolderState::default();
        state.begin_selection(6, false);
        state.track_selection(6, false);
        state.begin_selection(2, true);
        assert_eq!(state.edit.selection(), 2..6);
        state.track_selection(1, false);
        assert_eq!(state.edit.selection(), 1..6);
        state.error = Some(-48);
        state.begin_selection(10, false);
        assert!(!state.is_selecting());
        assert_eq!(state.edit.selection(), 1..6);
    }

    #[test]
    fn new_folder_vertical_arrows_use_single_line_boundaries() {
        let mut state = StandardFileNewFolderState::default();
        let mut scrap = Vec::new();
        for modifiers in [0, 0x0200, 0x0800] {
            assert_eq!(state.key(0x1f, modifiers, &mut scrap), None);
            assert_eq!(state.edit.selection(), 15..15);
            assert_eq!(state.key(0x1e, modifiers, &mut scrap), None);
            assert_eq!(state.edit.selection(), 0..0);
        }
        state.key(b'X' as u32, 0, &mut scrap);
        assert_eq!(state.edit.text(), b"Xuntitled folder");
        state.key(0x1f, 0, &mut scrap);
        state.key(b'Y' as u32, 0, &mut scrap);
        assert_eq!(state.edit.text(), b"Xuntitled folderY");
        assert_eq!(state.edit.selection(), 17..17);
    }

    #[test]
    fn new_folder_error_is_a_single_action_modal_alert() {
        let mut state = StandardFileNewFolderState::default();
        state.error = Some(-48);
        let original = state.edit.clone();
        let mut scrap = b"clipboard".to_vec();
        let layout = super::StandardFileNewFolderLayout::new((90, 140, 350, 500));
        for (message, modifiers) in [(b'x' as u32, 0), (8, 0), (b'v' as u32, 0x100), (27, 0)] {
            assert_eq!(state.key(message, modifiers, &mut scrap), None);
            assert_eq!(state.edit, original);
            assert_eq!(scrap, b"clipboard");
        }
        assert_eq!(
            state.event(
                &layout,
                1,
                0,
                0,
                (layout.cancel.0 + 1, layout.cancel.1 + 1),
                &mut scrap
            ),
            None
        );
        assert_eq!(
            state.event(
                &layout,
                1,
                0,
                0,
                (layout.create.0 + 1, layout.create.1 + 1),
                &mut scrap
            ),
            Some(Action::Cancel)
        );
        assert_eq!(state.key(13, 0, &mut scrap), Some(Action::Cancel));
        assert_eq!(
            state.key((0x4c << 8) | 3, 0, &mut scrap),
            Some(Action::Cancel)
        );
    }

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

#[cfg(test)]
mod save_caret_tests {
    use super::StandardFileCaret;
    #[test]
    fn insertion_blink_uses_guest_interval_and_resets_after_selection() {
        let mut caret = StandardFileCaret::default();
        assert!(caret.idle(u32::MAX - 10, 30, true));
        assert!(caret.on);
        assert!(!caret.idle(18, 30, true));
        assert!(caret.idle(19, 30, true));
        assert!(!caret.on);
        assert!(caret.idle(49, 30, true));
        assert!(caret.on);
        assert!(caret.idle(50, 30, false));
        assert!(!caret.on);
        assert!(!caret.idle(80, 30, false));
        assert!(caret.idle(81, 30, true));
        assert!(caret.on);
        caret.reset(90);
        assert!(!caret.idle(119, 30, true));
        assert!(caret.idle(120, 30, true));
    }
}
