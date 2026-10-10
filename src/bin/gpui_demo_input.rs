//! Guest keyboard translation and focus lifecycle shared by GPUI views.

use gpui_kit::{Keystroke, Modifiers};
use std::collections::HashMap;
use systemless::systems::macintosh::session::MacintoshInput;

#[derive(Default)]
pub(crate) struct KeyboardState {
    host_modifiers: Modifiers,
    caps_lock_on: bool,
    held_keys: HashMap<u8, u8>,
}

impl KeyboardState {
    pub(crate) fn sync_caps_lock(&mut self, on: bool) -> Vec<MacintoshInput> {
        if self.caps_lock_on == on {
            return Vec::new();
        }
        self.caps_lock_on = on;
        // Guest Caps Lock latches on key-down and survives physical release.
        vec![
            MacintoshInput::KeyDown { mac_key: 0x39, character: 0 },
            MacintoshInput::KeyUp { mac_key: 0x39, character: 0 },
        ]
    }

    pub(crate) fn sync_host_modifiers(&mut self, modifiers: Modifiers) -> Vec<MacintoshInput> {
        let mut events = Vec::new();
        // Event Manager exposes Command, Shift, Option and Control in
        // EventRecord.modifiers (Macintosh Toolbox Essentials, chapter 2).
        for (mac_key, previous, down) in [
            (0x37, self.host_modifiers.platform, modifiers.platform),
            (0x38, self.host_modifiers.shift, modifiers.shift),
            (0x3a, self.host_modifiers.alt, modifiers.alt),
            (0x3b, self.host_modifiers.control, modifiers.control),
        ] {
            if previous != down {
                let input = if down {
                    MacintoshInput::KeyDown { mac_key, character: 0 }
                } else {
                    MacintoshInput::KeyUp { mac_key, character: 0 }
                };
                events.push(input);
            }
        }
        self.host_modifiers = modifiers;
        events
    }

    pub(crate) fn press_host_key(&mut self, mac_key: u8, character: u8) -> Option<MacintoshInput> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.held_keys.entry(mac_key) {
            entry.insert(character);
            return Some(MacintoshInput::KeyDown { mac_key, character });
        }
        None
    }

    pub(crate) fn release_host_key(&mut self, mac_key: u8) -> Option<MacintoshInput> {
        self.held_keys.remove(&mac_key).map(|character| MacintoshInput::KeyUp { mac_key, character })
    }

    // Focus loss releases physical keys but preserves the guest Caps Lock latch.
    pub(crate) fn release_all(&mut self) -> Vec<MacintoshInput> {
        let mut keys: Vec<u8> = self.held_keys.keys().copied().collect();
        keys.sort_unstable();
        let mut events: Vec<_> = keys.into_iter().filter_map(|key| self.release_host_key(key)).collect();
        events.extend(self.sync_host_modifiers(Modifiers::default()));
        events
    }
}

// GPUI reports the printed key separately from its typed character.
// Keep the Macintosh virtual code tied to the key and pass the typed
// character through the existing guest event queue. Inside Macintosh
// Volume V (1986), V-191, key-code assignments; Text (1993), pp. 2-32--2-37.
pub(crate) fn guest_virtual_key(key: &str) -> Option<u8> {
    Some(match key {
        "enter" => 0x24,
        "escape" => 0x35,
        "space" => 0x31,
        "tab" => 0x30,
        "backspace" => 0x33,
        "left" | "arrowleft" => 0x7b,
        "right" | "arrowright" => 0x7c,
        "down" | "arrowdown" => 0x7d,
        "up" | "arrowup" => 0x7e,
        "a" => 0x00,
        "s" => 0x01,
        "d" => 0x02,
        "f" => 0x03,
        "h" => 0x04,
        "g" => 0x05,
        "z" => 0x06,
        "x" => 0x07,
        "c" => 0x08,
        "v" => 0x09,
        "b" => 0x0b,
        "q" => 0x0c,
        "w" => 0x0d,
        "e" => 0x0e,
        "r" => 0x0f,
        "y" => 0x10,
        "t" => 0x11,
        "1" => 0x12,
        "2" => 0x13,
        "3" => 0x14,
        "4" => 0x15,
        "6" => 0x16,
        "5" => 0x17,
        "=" => 0x18,
        "9" => 0x19,
        "7" => 0x1a,
        "-" => 0x1b,
        "8" => 0x1c,
        "0" => 0x1d,
        "]" => 0x1e,
        "o" => 0x1f,
        "u" => 0x20,
        "[" => 0x21,
        "i" => 0x22,
        "p" => 0x23,
        "l" => 0x25,
        "j" => 0x26,
        "'" => 0x27,
        "k" => 0x28,
        ";" => 0x29,
        "\\" => 0x2a,
        "," => 0x2b,
        "/" => 0x2c,
        "n" => 0x2d,
        "m" => 0x2e,
        "." => 0x2f,
        "`" => 0x32,
        _ => return None,
    })
}

pub(crate) fn guest_key(keystroke: &Keystroke) -> Option<(u8, u8)> {
    let key = keystroke.key.to_ascii_lowercase();
    let control = match key.as_str() {
        "enter" => Some((0x24, 13)),
        "escape" => Some((0x35, 27)),
        "space" => Some((0x31, 32)),
        "tab" => Some((0x30, 9)),
        "backspace" => Some((0x33, 8)),
        "left" | "arrowleft" => Some((0x7b, 28)),
        "right" | "arrowright" => Some((0x7c, 29)),
        "down" | "arrowdown" => Some((0x7d, 31)),
        "up" | "arrowup" => Some((0x7e, 30)),
        _ => None,
    };
    if control.is_some() {
        return control;
    }
    let virtual_key = guest_virtual_key(&key)?;
    let character = if let Some(text) = keystroke.key_char.as_deref() {
        let mut chars = text.chars();
        let character = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        systemless::systems::macintosh::mac_roman::encode_mac_roman_char(character)?
    } else {
        let character = key.as_bytes()[0];
        if keystroke.modifiers.shift && character.is_ascii_alphabetic() {
            character.to_ascii_uppercase()
        } else {
            character
        }
    };
    Some((virtual_key, character))
}


/// Host preedit is transient: the guest is changed only after a complete commit.
/// Mac Roman guest offsets are UTF-16 offsets because every decoded byte is BMP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextInputOwner {
    pub identity: (u32, u64),
    pub port: u32,
    pub text: Vec<u8>,
    pub selection: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Preedit {
    pub text: String,
    pub selection_utf16: std::ops::Range<usize>,
}

#[derive(Default)]
pub(crate) struct GuestComposition {
    owner: Option<TextInputOwner>,
    awaiting_guest_change: Option<TextInputOwner>,
    pub preedit: Option<Preedit>,
}

impl GuestComposition {
    pub fn owner(&self) -> Option<&TextInputOwner> { self.owner.as_ref() }

    /// Focus loss, modality changes, selection changes and disposal invalidate
    /// preedit. A recycled address cannot inherit another field's composition.
    pub fn synchronize(&mut self, owner: Option<TextInputOwner>) {
        let owner = owner.filter(|owner| owner.selection.start <= owner.selection.end
            && owner.selection.end <= owner.text.len());
        if owner.is_some() && owner == self.awaiting_guest_change {
            // Rendering may still hold the pre-commit snapshot. It cannot
            // authorize another replacement of that stale selection.
            return;
        }
        self.awaiting_guest_change = None;
        if self.owner != owner { self.preedit = None; }
        self.owner = owner;
    }

    pub fn mark(&mut self, text: &str, selected: std::ops::Range<usize>) -> bool {
        if self.owner.is_none() || selected.start > selected.end { return false; }
        // Host ranges must not split surrogate pairs in marked Unicode text.
        let boundaries: std::collections::BTreeSet<_> = std::iter::once(0)
            .chain(text.chars().scan(0, |offset, ch| {
                *offset += ch.len_utf16(); Some(*offset)
            })).collect();
        if !boundaries.contains(&selected.start) || !boundaries.contains(&selected.end) {
            return false;
        }
        self.preedit = Some(Preedit { text: text.into(), selection_utf16: selected });
        true
    }

    pub fn cancel(&mut self) { self.preedit = None; }

    /// Return a pinned request for the guest event path. Reject the entire
    /// commit on unrepresentable Unicode; never synthesize replacement glyphs.
    pub fn commit(&mut self, text: &str) -> Option<(TextInputOwner, Vec<u8>)> {
        let owner = self.owner.as_ref()?.clone();
        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = normalized.chars()
            .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
            .collect::<Option<Vec<_>>>()?;
        self.preedit = None;
        if bytes.is_empty() && owner.selection.is_empty() { return Some((owner, bytes)); }
        // A new guest snapshot must establish ownership before another commit;
        // otherwise asynchronous commits could reuse a stale selection.
        self.awaiting_guest_change = Some(owner.clone());
        self.owner = None;
        Some((owner, bytes))
    }
}

#[cfg(test)]
mod composition_tests {
    use super::{GuestComposition, TextInputOwner};

    fn owner() -> TextInputOwner {
        TextInputOwner { identity: (42, 3), port: 100,
            text: vec![b'a', 0x8e, b'b'], selection: 1..2 }
    }

    #[test]
    fn composition_keeps_unicode_preedit_and_commits_mac_roman_atomically() {
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        assert!(state.mark("日😀", 1..3));
        assert!(!state.mark("日😀", 1..2));
        assert!(state.commit("é😀").is_none());
        assert_eq!(state.preedit.as_ref().unwrap().text, "日😀");
        let (pinned, bytes) = state.commit("é\r\nx\ny").unwrap();
        assert_eq!(pinned, owner());
        assert_eq!(bytes, [0x8e, b'\r', b'x', b'\r', b'y']);
        assert!(state.preedit.is_none());
        assert!(state.commit("z").is_none());
        state.synchronize(Some(owner()));
        assert!(state.commit("z").is_none(), "old snapshot cannot reopen stale selection");
        let mut updated = owner();
        updated.text = bytes;
        updated.selection = 5..5;
        state.synchronize(Some(updated));
        assert!(state.commit("z").is_some());
    }

    #[test]
    fn composition_cancels_on_guest_mutation_focus_loss_and_identity_reuse() {
        for mutation in 0..5 {
            let mut state = GuestComposition::default();
            let initial = owner();
            state.synchronize(Some(initial.clone()));
            assert!(state.mark("é", 1..1));
            state.synchronize(Some(initial.clone()));
            assert!(state.preedit.is_some());
            let mut changed = initial;
            match mutation {
                0 => changed.identity.1 += 1,
                1 => changed.port += 1,
                2 => changed.text.push(b'x'),
                3 => changed.selection = 0..0,
                _ => changed.selection = 0..99,
            }
            state.synchronize(Some(changed));
            assert!(state.preedit.is_none());
            if mutation == 4 { assert!(state.commit("z").is_none()); }
            state.synchronize(None);
            assert!(!state.mark("x", 0..1));
            assert!(state.commit("z").is_none());
        }
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        assert!(state.mark("x", 0..1));
        state.cancel();
        assert!(state.preedit.is_none());
        assert!(state.commit("z").is_some());
    }
}

/// Resolve only the exact live guest-owned document field. Dialogs and file
/// panels have separate modal input ownership and must use their own paths.
pub(crate) fn guest_commit_inputs(
    session: &mut systemless::systems::macintosh::session::MacintoshSession,
    owner: &TextInputOwner,
    bytes: &[u8],
) -> Option<Vec<MacintoshInput>> {
    if session.runner().is_ui_tracking_active()
        || session.runner().standard_file_snapshot().is_some()
        || session.runner_mut().dialog_snapshot().iter().any(|dialog| dialog.visible && dialog.active) {
        return None;
    }
    let keys = session.runner().event_manager_snapshot().key_map;
    // Never release a real held A key or interpret committed characters as
    // Command/Control/Option shortcuts. Physical modifiers stay guest-owned.
    if [0u8, 0x37, 0x3a, 0x3b].iter().any(|key|
        keys[usize::from(*key / 8)] & (1 << (*key % 8)) != 0) { return None; }
    let records = session.runner_mut().text_edit_snapshot().records;
    let record = records.iter().find(|record| record.active && record.drawing_intact
        && (record.guest_id, record.generation) == owner.identity
        && record.owner_port == owner.port && record.text == owner.text
        && record.selection == (owner.selection.start, owner.selection.end))?;
    if records.iter().filter(|record| record.active && record.drawing_intact).count() != 1
        || record.global_view_rect.is_none() { return None; }
    // Control characters are commands, not committed text. Newlines have
    // already been normalized; Return remains an ordinary guest TEKey event.
    if bytes.iter().any(|byte| *byte < 32 && !matches!(*byte, b'\r' | b'\t')) {
        return None;
    }
    let delete = [8u8];
    let bytes = if bytes.is_empty() && !owner.selection.is_empty() { &delete[..] } else { bytes };
    Some(bytes.iter().flat_map(|&character| [
        MacintoshInput::KeyDown { mac_key: 0x00, character },
        MacintoshInput::KeyUp { mac_key: 0x00, character },
    ]).collect())
}
