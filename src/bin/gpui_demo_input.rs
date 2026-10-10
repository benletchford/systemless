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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextInputTarget {
    Document { port: u32 },
    StandardFile { new_folder: bool },
    Dialog { item: i16, content_revision: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextInputOwner {
    pub identity: (u32, u64),
    pub target: TextInputTarget,
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
    pending_commits: Vec<(TextInputOwner, Vec<u8>, Option<TextInputOwner>, Option<usize>)>,
    pub preedit: Option<Preedit>,
}

impl GuestComposition {
    pub fn owner(&self) -> Option<&TextInputOwner> { self.owner.as_ref() }

    /// Focus loss, modality changes, selection changes and disposal invalidate
    /// preedit. A recycled address cannot inherit another field's composition.
    pub fn synchronize(&mut self, owner: Option<TextInputOwner>) {
        let owner = owner.filter(|owner| owner.selection.start <= owner.selection.end
            && owner.selection.end <= owner.text.len());
        if let Some(actual) = &owner {
            if !self.pending_commits.is_empty() {
                if self.owner.as_ref() == Some(actual) {
                    self.pending_commits.clear();
                    return;
                }
                // A frame can show the old snapshot or a partially consumed
                // commit. Retain the host's future caret instead of restarting
                // the next commit at that stale guest selection.
                if self.pending_commits.iter().any(|(base, bytes, prior, caret)| {
                    if prior.as_ref() == Some(actual) { return true; }
                    if actual == base { return true; }
                    if actual.identity != base.identity || actual.target != base.target { return false; }
                    let retained = base.text.len() - base.selection.len();
                    let Some(inserted) = actual.text.len().checked_sub(retained) else { return false; };
                    inserted <= bytes.len()
                        && (actual.selection == (base.selection.start + inserted..base.selection.start + inserted)
                            || inserted == bytes.len() && caret.is_some_and(|caret|
                                actual.selection.is_empty() && actual.selection.start >= caret
                                    && actual.selection.start <= base.selection.start + inserted))
                        && actual.text[..base.selection.start] == base.text[..base.selection.start]
                        && actual.text[base.selection.start..base.selection.start + inserted] == bytes[..inserted]
                        && actual.text[base.selection.start + inserted..] == base.text[base.selection.end..]
                }) { return; }
            }
        }
        self.pending_commits.clear();
        if self.owner != owner { self.preedit = None; }
        self.owner = owner;
    }

    /// A rejected request invalidates dependent predicted ranges. Ignore delayed
    /// rejections from an older field or an already acknowledged request.
    pub fn reject(&mut self, rejected: &TextInputOwner) {
        if self.pending_commits.iter().any(|(base, _, prior, _)| base == rejected || prior.as_ref() == Some(rejected)) {
            self.pending_commits.clear();
            self.owner = None;
            self.preedit = None;
        }
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

    /// Map an untouched guest glyph position to the virtual UTF-16 document.
    /// Positions inside the replaced guest selection have no surrounding ink.
    pub fn surrounding_virtual_index(&self, index: usize) -> Option<usize> {
        let owner = self.owner.as_ref()?;
        let preedit = self.preedit.as_ref()?;
        if index > owner.text.len() { return None; }
        if index < owner.selection.start { return Some(index); }
        if index >= owner.selection.end {
            return owner.selection.start.checked_add(preedit.text.encode_utf16().count())?
                .checked_add(index - owner.selection.end);
        }
        None
    }

    /// Map untouched surrounding text back to the still-painted guest record.
    /// Stage-intersecting ranges must use separate Unicode geometry.
    pub fn surrounding_guest_range(&self, range: std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
        let owner = self.owner.as_ref()?;
        let preedit = self.preedit.as_ref()?;
        if range.start > range.end { return None; }
        let stage_end = owner.selection.start.checked_add(preedit.text.encode_utf16().count())?;
        let mapped = if range.end <= owner.selection.start && range.start < owner.selection.start {
            range
        } else if range.start >= stage_end && range.end > stage_end {
            let start = owner.selection.end.checked_add(range.start - stage_end)?;
            let end = owner.selection.end.checked_add(range.end - stage_end)?;
            start..end
        } else { return None; };
        (mapped.end <= owner.text.len()).then_some(mapped)
    }

    /// Split candidate geometry requests at guest/stage ownership boundaries.
    /// Return source order so the platform receives the first painted rectangle.
    pub fn geometry_ranges(&self, range: std::ops::Range<usize>) -> Option<Vec<std::ops::Range<usize>>> {
        let owner = self.owner.as_ref()?;
        let preedit = self.preedit.as_ref()?;
        let units: Vec<_> = preedit.text.encode_utf16().collect();
        let start = owner.selection.start;
        let end = start.checked_add(units.len())?;
        let length = end.checked_add(owner.text.len().checked_sub(owner.selection.end)?)?;
        if range.start > range.end || range.end > length { return None; }
        for offset in [range.start, range.end] {
            if offset >= start && offset <= end {
                String::from_utf16(&units[..offset - start]).ok()?;
            }
        }
        if range.is_empty() { return Some(vec![range]); }
        let mut boundaries = vec![range.start];
        for boundary in [start, end] {
            if boundary > range.start && boundary < range.end && boundaries.last() != Some(&boundary) {
                boundaries.push(boundary);
            }
        }
        boundaries.push(range.end);
        Some(boundaries.windows(2).map(|pair| pair[0]..pair[1]).collect())
    }

    /// Host replacement ranges use document UTF-16 offsets. A range inside
    /// staged Unicode can be edited without changing the pinned guest range.
    /// Return the insertion offset within the resulting stage for its selection.
    pub fn replacement_text(&self, range: Option<&std::ops::Range<usize>>, text: &str)
        -> Option<(String, usize)> {
        let owner = self.owner.as_ref()?;
        let Some(range) = range else { return Some((text.into(), 0)); };
        let Some(preedit) = &self.preedit else {
            return (range == &owner.selection).then(|| (text.into(), 0));
        };
        let start = range.start.checked_sub(owner.selection.start)?;
        let end = range.end.checked_sub(owner.selection.start)?;
        if start > end { return None; }
        let units: Vec<_> = preedit.text.encode_utf16().collect();
        // Decoding the slices rejects ranges that split a surrogate pair.
        let prefix = String::from_utf16(units.get(..start)?).ok()?;
        let suffix = String::from_utf16(units.get(end..)?).ok()?;
        Some((prefix + text + &suffix, start))
    }

    /// Commit a range crossing an active stage boundary without rewriting
    /// untouched guest text between disjoint edits. Retain staged fragments.
    pub fn commit_overlapping_range(&mut self, range: std::ops::Range<usize>, text: &str)
        -> Option<(TextInputOwner, TextInputOwner, Vec<u8>, usize)> {
        self.geometry_ranges(range.clone())?;
        let owner = self.owner.as_ref()?;
        let preedit = self.preedit.as_ref()?;
        let units: Vec<_> = preedit.text.encode_utf16().collect();
        let start = owner.selection.start;
        let end = start.checked_add(units.len())?;
        if if range.is_empty() { range.start < start || range.start > end }
            else { range.end <= start || range.start >= end } { return None; }
        let prefix = String::from_utf16(&units[..range.start.saturating_sub(start)]).ok()?;
        let suffix = String::from_utf16(&units[range.end.saturating_sub(start).min(units.len())..]).ok()?;
        let guest = range.start.min(start)..owner.selection.end.checked_add(range.end.saturating_sub(end))?;
        let insertion_length = (prefix.clone() + text).replace("\r\n", "\r").replace('\n', "\r").chars().count();
        let caret = guest.start.checked_add(insertion_length)?;
        let payload = prefix + text + &suffix;
        let saved = self.preedit.take();
        let result = self.commit_range(guest, &payload);
        let Some((expected, request, bytes)) = result else { self.preedit = saved; return None; };
        self.owner.as_mut()?.selection = caret..caret;
        self.pending_commits.last_mut()?.3 = Some(caret);
        Some((expected, request, bytes, caret))
    }

    /// Pin both the original selection and an explicit document replacement.
    /// Old frames remain valid while the guest consumes the selection request.
    pub fn commit_range(&mut self, range: std::ops::Range<usize>, text: &str)
        -> Option<(TextInputOwner, TextInputOwner, Vec<u8>)> {
        let expected = self.owner.as_ref()?.clone();
        if self.preedit.is_some() || !matches!(expected.target, TextInputTarget::Document { .. })
            || range.start > range.end || range.end > expected.text.len() || range.end > i16::MAX as usize {
            return None;
        }
        let mut selected = expected.clone(); selected.selection = range;
        self.owner = Some(selected.clone());
        let pending = self.pending_commits.len();
        let Some((request, bytes)) = self.commit(text) else {
            self.owner = Some(expected); return None;
        };
        if self.pending_commits.len() == pending {
            self.pending_commits.push((request.clone(), bytes.clone(), Some(expected.clone()), None));
        } else {
            self.pending_commits.last_mut()?.2 = Some(expected.clone());
        }
        Some((expected, request, bytes))
    }

    /// Return a pinned request for the guest event path. Reject the entire
    /// commit on unrepresentable Unicode; never synthesize replacement glyphs.
    pub fn commit(&mut self, text: &str) -> Option<(TextInputOwner, Vec<u8>)> {
        let owner = self.owner.as_ref()?.clone();
        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = normalized.chars()
            .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
            .collect::<Option<Vec<_>>>()?;
        if bytes.iter().any(|byte| *byte < 32 && !matches!(*byte, b'\r' | b'\t')) { return None; }
        self.preedit = None;
        if bytes.is_empty() && owner.selection.is_empty() { return Some((owner, bytes)); }
        let mut predicted = owner.clone();
        predicted.text.splice(owner.selection.clone(), bytes.iter().copied());
        let caret = owner.selection.start + bytes.len();
        predicted.selection = caret..caret;
        self.pending_commits.push((owner.clone(), bytes.clone(), None, None));
        self.owner = Some(predicted);
        Some((owner, bytes))
    }
}

#[cfg(test)]
mod composition_tests {
    use super::{GuestComposition, TextInputOwner, TextInputTarget};

    fn owner() -> TextInputOwner {
        TextInputOwner { identity: (42, 3), target: TextInputTarget::Document { port: 100 },
            text: vec![b'a', 0x8e, b'b'], selection: 1..2 }
    }

    #[test]
    fn surrounding_ranges_keep_guest_offsets_without_crossing_unicode_stage() {
        let mut state = GuestComposition::default();
        let mut current = owner();
        current.text = b"abcdef".to_vec(); current.selection = 2..4;
        state.synchronize(Some(current));
        assert!(state.mark("日😀", 1..3));
        assert_eq!(state.surrounding_guest_range(0..2), Some(0..2));
        assert_eq!(state.surrounding_guest_range(5..7), Some(4..6));
        assert_eq!(state.surrounding_guest_range(6..6), Some(5..5));
        assert_eq!(state.surrounding_virtual_index(1), Some(1));
        assert_eq!(state.surrounding_virtual_index(4), Some(5));
        assert_eq!(state.surrounding_virtual_index(6), Some(7));
        assert_eq!(state.surrounding_virtual_index(3), None);
        assert_eq!(state.surrounding_virtual_index(7), None);
        for range in [2..2, 5..5, 1..3, 4..6, 7..8, 6..5] {
            assert!(state.surrounding_guest_range(range).is_none());
        }
    }
    #[test]
    fn geometry_queries_split_at_stage_boundaries_and_reject_surrogate_splits() {
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        assert!(state.mark("日😀", 1..3));
        assert_eq!(state.geometry_ranges(0..5), Some(vec![0..1, 1..4, 4..5]));
        assert_eq!(state.geometry_ranges(1..5), Some(vec![1..4, 4..5]));
        assert_eq!(state.geometry_ranges(4..4), Some(vec![4..4]));
        for range in [2..3, 3..5, 0..6, 5..4] {
            assert!(state.geometry_ranges(range).is_none());
        }
    }

    #[test]
    fn crossing_replacement_retains_stage_fragments_and_rejects_invalid_unicode_atomically() {
        for (range, guest, payload, result, caret) in [
            (0..2, 0..2, vec![b'Q', b'Z'], vec![b'Q', b'Z', b'b'], 1),
            (2..4, 1..3, vec![0x8e, b'Q'], vec![b'a', 0x8e, b'Q'], 3),
            (0..4, 0..3, vec![b'Q'], vec![b'Q'], 1),
            (1..2, 1..2, vec![b'Q', b'Z'], vec![b'a', b'Q', b'Z', b'b'], 2),
            (2..2, 1..2, vec![0x8e, b'Q', b'Z'], vec![b'a', 0x8e, b'Q', b'Z', b'b'], 3),
        ] {
            let mut state = GuestComposition::default(); state.synchronize(Some(owner()));
            assert!(state.mark("éZ", 1..2));
            let (expected, request, bytes, insertion) = state.commit_overlapping_range(range, "Q").unwrap();
            assert_eq!(insertion, caret);
            assert_eq!(expected, owner()); assert_eq!(request.selection, guest); assert_eq!(bytes, payload);
            assert_eq!(state.owner().unwrap().text, result); assert!(state.preedit.is_none());
            assert_eq!(state.owner().unwrap().selection, caret..caret, "caret follows inserted text before retained stage suffix");
            let future = state.owner().unwrap().clone(); state.synchronize(Some(expected));
            assert_eq!(state.owner(), Some(&future));
        }
        let mut state = GuestComposition::default(); state.synchronize(Some(owner()));
        assert!(state.mark("日😀", 1..3));
        let before = state.preedit.clone();
        for range in [0..2, 0..3, 0..9, 4..5] {
            assert!(state.commit_overlapping_range(range, "Q").is_none());
            assert_eq!(state.owner(), Some(&owner())); assert_eq!(state.preedit, before);
        }
    }

    #[test]
    fn explicit_replacement_pins_prior_selection_until_guest_acknowledges() {
        let mut state = GuestComposition::default(); state.synchronize(Some(owner()));
        assert!(state.commit_range(0..1, "😀").is_none());
        assert_eq!(state.owner(), Some(&owner()));
        let (expected, request, bytes) = state.commit_range(0..1, "Z").unwrap();
        assert_eq!(expected, owner()); assert_eq!(request.selection, 0..1); assert_eq!(bytes, b"Z");
        let future = state.owner().unwrap().clone(); assert_eq!(future.text, [b'Z', 0x8e, b'b']);
        state.synchronize(Some(expected.clone())); assert_eq!(state.owner(), Some(&future));
        state.synchronize(Some(request.clone())); assert_eq!(state.owner(), Some(&future));
        state.reject(&expected); assert!(state.owner().is_none());
        state.synchronize(Some(owner()));
        state.commit_range(0..1, "Z").unwrap();
        let future = state.owner().unwrap().clone(); state.synchronize(Some(future.clone()));
        state.reject(&owner()); assert_eq!(state.owner(), Some(&future));
    }

    #[test]
    fn composition_keeps_unicode_preedit_and_commits_mac_roman_atomically() {
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        assert!(state.mark("日😀", 1..3));
        assert!(!state.mark("日😀", 1..2));
        assert!(state.commit("é😀").is_none());
        assert!(state.commit("é\u{1b}").is_none());
        assert_eq!(state.preedit.as_ref().unwrap().text, "日😀");
        let (pinned, bytes) = state.commit("é\r\nx\ny").unwrap();
        assert_eq!(pinned, owner());
        assert_eq!(bytes, [0x8e, b'\r', b'x', b'\r', b'y']);
        assert!(state.preedit.is_none());
        let predicted = state.owner().unwrap().clone();
        assert_eq!(predicted.text, [b'a', 0x8e, b'\r', b'x', b'\r', b'y', b'b']);
        assert_eq!(predicted.selection, 6..6);
        state.synchronize(Some(owner()));
        assert_eq!(state.owner(), Some(&predicted), "old snapshot cannot reopen stale selection");
        let (second_owner, second_bytes) = state.commit("z").unwrap();
        assert_eq!(second_owner, predicted);
        assert_eq!(second_bytes, b"z");
        let final_owner = state.owner().unwrap().clone();
        state.synchronize(Some(predicted));
        assert_eq!(state.owner(), Some(&final_owner), "partial acknowledgement preserves queued caret");
        state.synchronize(Some(final_owner.clone()));
        assert_eq!(state.owner(), Some(&final_owner));
        assert!(state.pending_commits.is_empty());
    }

    #[test]
    fn composition_rejection_recovers_guest_selection_without_inheriting_stale_feedback() {
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        let (first, _) = state.commit("x").unwrap();
        let (second, _) = state.commit("y").unwrap();
        assert!(state.mark("日", 1..1));
        state.reject(&second);
        assert!(state.owner().is_none());
        assert!(state.preedit.is_none());
        state.synchronize(Some(owner()));
        assert_eq!(state.owner(), Some(&owner()));
        let mut reused = owner();
        reused.identity.1 += 1;
        state.synchronize(Some(reused.clone()));
        state.commit("z").unwrap();
        let predicted = state.owner().cloned();
        state.reject(&first);
        assert_eq!(state.owner().cloned(), predicted);
    }

    #[test]
    fn composition_replaces_marked_subranges_without_changing_guest_ownership() {
        let mut state = GuestComposition::default();
        state.synchronize(Some(owner()));
        assert!(state.mark("a😀Z", 1..3));
        let unchanged = state.preedit.clone();
        for invalid in [0..2, 2..3, 3..3, 4..6, 4..2] {
            assert!(state.replacement_text(Some(&invalid), "é").is_none());
            assert_eq!(state.preedit, unchanged);
        }
        let (text, offset) = state.replacement_text(Some(&(2..4)), "é").unwrap();
        assert_eq!(text, "aéZ");
        assert_eq!(offset, 1);
        assert!(state.mark(&text, offset..offset + 1));
        assert_eq!(state.owner(), Some(&owner()), "staging must not alter pinned guest state");
        let (text, offset) = state.replacement_text(Some(&(2..2)), "x").unwrap();
        assert_eq!((text.as_str(), offset), ("axéZ", 1));
        let (text, _) = state.replacement_text(Some(&(2..3)), "").unwrap();
        assert_eq!(text, "aZ");
        let (pinned, bytes) = state.commit(&text).unwrap();
        assert_eq!(pinned, owner());
        assert_eq!(bytes, b"aZ");
        assert_eq!(state.owner().unwrap().text, b"aaZb");
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
                1 => changed.target = TextInputTarget::Document { port: 101 },
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
    if let TextInputTarget::StandardFile { new_folder } = owner.target {
        return guest_standard_file_commit_inputs(session, &StandardFileTextOwner {
            identity: owner.identity, new_folder, text: owner.text.clone(), selection: owner.selection.clone(),
        }, bytes);
    }
    if let TextInputTarget::Dialog { item, content_revision } = owner.target {
        return guest_dialog_commit_inputs(session, &DialogTextOwner {
            identity: owner.identity, item, content_revision,
            text: owner.text.clone(), selection: owner.selection.clone(),
        }, bytes);
    }
    let TextInputTarget::Document { port } = owner.target else { return None; };
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
        && record.owner_port == port && record.text == owner.text
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

/// Modal text commits pin the dialog lifetime and active item independently of
/// document TERec ownership. Characters still travel through DialogSelect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DialogTextOwner {
    pub identity: (u32, u64),
    pub content_revision: u64,
    pub item: i16,
    pub text: Vec<u8>,
    pub selection: std::ops::Range<usize>,
}

pub(crate) fn dialog_text_owner(
    dialogs: &[systemless::runner::DialogSnapshot],
    windows: &[systemless::runner::WindowFrameSnapshot],
) -> Option<DialogTextOwner> {
    let mut active = dialogs.iter().filter(|dialog| dialog.visible && dialog.active);
    let dialog = active.next()?;
    if active.next().is_some() || !windows.iter().any(|window|
        (window.guest_id, window.generation) == (dialog.guest_id, dialog.generation)
            && window.presentation_definition_id() == Some(1)) { return None; }
    let item = dialog.items.iter().find(|item| Some(item.number) == dialog.edit_field
        && item.kind == systemless::runner::DialogItemKind::EditText && item.enabled && item.visible
        && item.edit_text_layout.is_some())?;
    let text: Vec<u8> = item.text.chars().map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
        .collect::<Option<_>>()?;
    let (start, end) = item.selection?;
    let selection = usize::try_from(start).ok()?..usize::try_from(end).ok()?;
    if selection.start > selection.end || selection.end > text.len() { return None; }
    Some(DialogTextOwner { content_revision: dialog.content_revision, identity: (dialog.guest_id, dialog.generation), item: item.number, text, selection })
}

/// Admit an internal dialog TERec only through the same ownership and paint
/// guards used by the shared compositor. The dialog remains the event target.
pub(crate) fn dialog_text_owner_with_records(
    dialogs: &[systemless::runner::DialogSnapshot],
    windows: &[systemless::runner::WindowFrameSnapshot],
    records: &[systemless::runner::TextEditSnapshot],
    controls: &[systemless::runner::ControlSnapshot],
) -> Option<DialogTextOwner> {
    if let Some(owner) = dialog_text_owner(dialogs, windows) { return Some(owner); }
    let viewport = super::frames::Rect { top: i16::MIN as i32, left: i16::MIN as i32,
        bottom: i16::MAX as i32, right: i16::MAX as i32 };
    let pieces = super::frames::text_edit_pieces(records, dialogs, controls, windows, viewport);
    let mut candidates = pieces.iter().filter_map(|piece| {
        let record = &records[piece.record];
        let dialog = dialogs.iter().find(|dialog| dialog.guest_id == record.owner_port)?;
        let item = dialog.items.iter().find(|item| Some(item.number) == dialog.edit_field)?;
        Some(DialogTextOwner { identity: (dialog.guest_id, dialog.generation),
            content_revision: dialog.content_revision, item: item.number,
            text: record.text.clone(), selection: record.selection.0..record.selection.1 })
    });
    let owner = candidates.next()?;
    // Clipping can produce several pieces for one record, but never several owners.
    candidates.all(|other| other == owner).then_some(owner)
}

pub(crate) fn guest_dialog_commit_inputs(
    session: &mut systemless::systems::macintosh::session::MacintoshSession,
    owner: &DialogTextOwner,
    bytes: &[u8],
) -> Option<Vec<MacintoshInput>> {
    if session.runner().is_non_dialog_ui_tracking_active() || session.runner().standard_file_snapshot().is_some()
        || bytes.iter().any(|byte| *byte < 32) { return None; }
    let keys = session.runner().event_manager_snapshot().key_map;
    if [0u8, 0x37, 0x3a, 0x3b].iter().any(|key|
        keys[usize::from(*key / 8)] & (1 << (*key % 8)) != 0) { return None; }
    let dialogs = session.runner_mut().dialog_snapshot();
    let windows = session.runner_mut().window_frame_snapshot();
    let records = session.runner_mut().text_edit_snapshot().records;
    let controls = session.runner_mut().control_snapshot();
    if dialog_text_owner_with_records(&dialogs, &windows, &records, &controls).as_ref() != Some(owner) { return None; }
    // Return/Tab/Escape are modal actions, never characters in a text commit.
    let delete = [8u8];
    let bytes = if bytes.is_empty() && !owner.selection.is_empty() { &delete[..] } else { bytes };
    Some(bytes.iter().flat_map(|&character| [
        MacintoshInput::KeyDown { mac_key: 0, character },
        MacintoshInput::KeyUp { mac_key: 0, character },
    ]).collect())
}

/// Standard File owns its filename independently of application dialog TERecs.
/// The panel generation changes when a subsidiary modal panel takes ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StandardFileTextOwner {
    pub identity: (u32, u64),
    pub new_folder: bool,
    pub text: Vec<u8>,
    pub selection: std::ops::Range<usize>,
}

pub(crate) fn standard_file_text_owner(panel: &systemless::runner::StandardFileSnapshot) -> Option<StandardFileTextOwner> {
    if !panel.standard_entry_point || panel.confirming_replace { return None; }
    let (name, selection, new_folder) = if let Some(folder) = &panel.new_folder {
        if folder.error.is_some() { return None; }
        (&folder.name, folder.selection, true)
    } else {
        if panel.kind != systemless::runner::StandardFileKind::Put || panel.name_has_focus != Some(true)
            || panel.name_text_layout.is_none() { return None; }
        (panel.name.as_ref()?, panel.name_selection?, false)
    };
    let text: Vec<u8> = name.chars().map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
        .collect::<Option<_>>()?;
    if selection.0 > selection.1 || selection.1 > text.len() { return None; }
    Some(StandardFileTextOwner { identity: (panel.guest_id, panel.generation), new_folder,
        text, selection: selection.0..selection.1 })
}

pub(crate) fn guest_standard_file_commit_inputs(
    session: &mut systemless::systems::macintosh::session::MacintoshSession,
    owner: &StandardFileTextOwner,
    bytes: &[u8],
) -> Option<Vec<MacintoshInput>> {
    // Control characters invoke panel actions; DEL is not printable Mac Roman.
    if bytes.iter().any(|byte| *byte < 32 || *byte == 127) { return None; }
    let keys = session.runner().event_manager_snapshot().key_map;
    if [0u8, 0x37, 0x3a, 0x3b].iter().any(|key|
        keys[usize::from(*key / 8)] & (1 << (*key % 8)) != 0) { return None; }
    let panel = session.runner().standard_file_snapshot()?;
    if standard_file_text_owner(&panel).as_ref() != Some(owner) { return None; }
    // Preserve each guest editor limit and reject filtered New Folder bytes atomically.
    let limit = if owner.new_folder { 31 } else { 63 };
    if owner.text.len() - owner.selection.len() + bytes.len() > limit
        || bytes.contains(&b':') || (!owner.new_folder && bytes.contains(&b'/')) { return None; }
    let delete = [8u8];
    let bytes = if bytes.is_empty() && !owner.selection.is_empty() { &delete[..] } else { bytes };
    Some(bytes.iter().flat_map(|&character| [
        MacintoshInput::KeyDown { mac_key: 0, character },
        MacintoshInput::KeyUp { mac_key: 0, character },
    ]).collect())
}
