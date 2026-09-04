//! Architecture-neutral TextEdit editing semantics.
//!
//! The 68K trap and PowerPC import layers translate guest ABI and memory into
//! this model, then serialize the result back into their respective `TERec`.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

/// Private feature flags associated with one guest `TERec`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessTextEditState {
    pub(crate) feature_bits: u16,
}

/// Canonical host-only TextEdit metadata for one Macintosh process.
///
/// Edit records and the private TextEdit scrap remain canonical guest memory.
/// This manager retains constructor identities and feature bits outside the
/// `TERec`. Inside Macintosh: Text (1993), pp. 2-90--2-92.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessTextEditManagerState {
    records: HashMap<u32, ProcessTextEditState>,
    handles: BTreeSet<u32>,
    pub(crate) click_tracking: Option<TextEditClickTracking>,
}

impl ProcessTextEditManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.records.is_empty() && self.handles.is_empty() && self.click_tracking.is_none()
    }

    pub(crate) fn register(&mut self, handle: u32) {
        if handle != 0 {
            self.handles.insert(handle);
        }
    }

    pub(crate) fn handles(&self) -> Vec<u32> {
        self.handles.iter().copied().collect()
    }

    pub(crate) fn feature_bit(&self, handle: u32, feature: u16) -> bool {
        let mask = 1u16.checked_shl(feature as u32).unwrap_or(0);
        self.records
            .get(&handle)
            .is_some_and(|state| state.feature_bits & mask != 0)
    }

    pub(crate) fn set_feature_bit(&mut self, handle: u32, feature: u16, enabled: bool) {
        let mask = 1u16.checked_shl(feature as u32).unwrap_or(0);
        if mask == 0 {
            return;
        }
        if enabled {
            self.records.entry(handle).or_default().feature_bits |= mask;
            return;
        }

        if let Some(state) = self.records.get_mut(&handle) {
            state.feature_bits &= !mask;
            if state.feature_bits == 0 {
                self.records.remove(&handle);
            }
        }
    }

    pub(crate) fn remove(&mut self, handle: &u32) {
        self.records.remove(handle);
        self.handles.remove(handle);
        if self
            .click_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.handle == *handle)
        {
            self.click_tracking = None;
        }
    }
}

/// Retained mouse ownership while TEClick tracks a selection.
/// Inside Macintosh: Text (1993), p. 2-85.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TextEditClickTracking {
    pub(crate) handle: u32,
    pub(crate) anchor: usize,
    pub(crate) native: bool,
    pub(crate) last_point: (i16, i16),
}

/// Mutable text and normalized selection state for one TextEdit operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TextEditBuffer {
    text: Vec<u8>,
    selection: Range<usize>,
}

impl TextEditBuffer {
    /// Load guest text and clamp its possibly reversed selection.
    pub(crate) fn new(text: Vec<u8>, selection_start: usize, selection_end: usize) -> Self {
        let start = selection_start.min(text.len());
        let end = selection_end.min(text.len());
        Self {
            text,
            selection: start.min(end)..start.max(end),
        }
    }

    pub(crate) fn text(&self) -> &[u8] {
        &self.text
    }

    pub(crate) fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    pub(crate) fn selected_text(&self) -> &[u8] {
        &self.text[self.selection.clone()]
    }

    /// Replace the selected range and collapse the selection after the insert.
    ///
    /// Inside Macintosh: Text (1993), pp. 2-81 and 2-92--2-93.
    pub(crate) fn replace_selection(&mut self, inserted: &[u8]) {
        let insertion_start = self.selection.start;
        self.text
            .splice(self.selection.clone(), inserted.iter().copied());
        let insertion_end = insertion_start
            .saturating_add(inserted.len())
            .min(self.text.len());
        self.selection = insertion_end..insertion_end;
    }

    /// Delete a nonempty selection, leaving an insertion point at its start.
    ///
    /// Inside Macintosh: Text (1993), pp. 2-91--2-92.
    pub(crate) fn delete_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.replace_selection(&[]);
        true
    }

    /// Apply the byte accepted by `TEKey`.
    ///
    /// Backspace deletes the selection or the preceding character. Left and
    /// right arrows move or collapse the caret rather than becoming text.
    /// Inside Macintosh: Text (1993), pp. 2-36--2-37 and 2-81--2-82.
    pub(crate) fn apply_key(&mut self, key: u8) {
        match key {
            0x08 | 0x7f => {
                if self.selection.is_empty() && self.selection.start > 0 {
                    self.selection.start -= 1;
                }
                self.replace_selection(&[]);
            }
            0x1c => {
                let caret = if self.selection.is_empty() {
                    self.selection.start.saturating_sub(1)
                } else {
                    self.selection.start
                };
                self.selection = caret..caret;
            }
            0x1d => {
                let caret = if self.selection.is_empty() {
                    self.selection.end.saturating_add(1).min(self.text.len())
                } else {
                    self.selection.end
                };
                self.selection = caret..caret;
            }
            _ => self.replace_selection(&[key]),
        }
    }
}

/// Horizontal origin for a TextEdit line, including its caller-selected inset.
///
/// Inside Macintosh: Text (1993), pp. 2-87--2-88.
pub(crate) fn aligned_line_left(
    left: i16,
    right: i16,
    width: i16,
    alignment: i16,
    left_inset: i16,
) -> i16 {
    match alignment {
        1 => left.saturating_add(right.saturating_sub(left).saturating_sub(width) / 2),
        -1 => right.saturating_sub(width),
        _ => left.saturating_add(left_inset),
    }
}

#[cfg(test)]
mod tests {
    use super::{aligned_line_left, TextEditBuffer};

    #[test]
    fn selection_is_clamped_and_normalized_once() {
        let buffer = TextEditBuffer::new(b"toolbox".to_vec(), 20, 3);

        assert_eq!(buffer.selection(), 3..7);
        assert_eq!(buffer.selected_text(), b"lbox");
    }

    #[test]
    fn replace_and_delete_share_selection_semantics() {
        let mut buffer = TextEditBuffer::new(b"one three".to_vec(), 4, 4);
        buffer.replace_selection(b"two ");
        assert_eq!(buffer.text(), b"one two three");
        assert_eq!(buffer.selection(), 8..8);

        let mut buffer = TextEditBuffer::new(buffer.text().to_vec(), 8, 4);
        assert!(buffer.delete_selection());
        assert_eq!(buffer.text(), b"one three");
        assert_eq!(buffer.selection(), 4..4);
        assert!(!buffer.delete_selection());
    }

    #[test]
    fn key_editing_handles_backspace_and_caret_arrows() {
        let mut buffer = TextEditBuffer::new(b"abc".to_vec(), 2, 2);
        buffer.apply_key(0x08);
        assert_eq!(buffer.text(), b"ac");
        assert_eq!(buffer.selection(), 1..1);

        buffer.apply_key(0x1c);
        assert_eq!(buffer.selection(), 0..0);
        buffer.apply_key(0x1d);
        assert_eq!(buffer.selection(), 1..1);
        assert_eq!(buffer.text(), b"ac");
    }

    #[test]
    fn alignment_is_shared_for_every_guest_adapter() {
        assert_eq!(aligned_line_left(20, 200, 80, 0, 1), 21);
        assert_eq!(aligned_line_left(20, 200, 80, -2, 1), 21);
        assert_eq!(aligned_line_left(20, 200, 80, 1, 1), 70);
        assert_eq!(aligned_line_left(20, 200, 80, -1, 1), 120);
    }
}

/// Immutable guest TextEdit contents for fixture and diagnostic assertions.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditSnapshot {
    pub view_rect: (i16, i16, i16, i16),
    pub text: Vec<u8>,
    pub selection: (usize, usize),
    pub active: bool,
    pub justification: i16,
    pub line_count: usize,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditManagerSnapshot {
    pub records: Vec<TextEditSnapshot>,
    pub private_scrap_length: usize,
    pub private_scrap: Vec<u8>,
}

pub(crate) fn snapshot_guest_records(
    handles: &[u32],
    read: &mut dyn FnMut(u32) -> Option<u8>,
) -> TextEditManagerSnapshot {
    fn word(read: &mut dyn FnMut(u32) -> Option<u8>, addr: u32) -> Option<u16> {
        Some(u16::from_be_bytes([read(addr)?, read(addr + 1)?]))
    }
    fn long(read: &mut dyn FnMut(u32) -> Option<u8>, addr: u32) -> Option<u32> {
        Some(u32::from_be_bytes([
            read(addr)?,
            read(addr + 1)?,
            read(addr + 2)?,
            read(addr + 3)?,
        ]))
    }
    // TERec fields and private scrap are canonical guest memory on both
    // architectures. Inside Macintosh: Text (1993), pp. 2-64--2-69, 2-98.
    let records = handles
        .iter()
        .filter_map(|handle| {
            let ptr = long(read, *handle).filter(|ptr| *ptr != 0)?;
            let length = usize::from(word(read, ptr + 0x3c)?);
            let text_handle = long(read, ptr + 0x3e)?;
            let text_ptr = long(read, text_handle)?;
            let text = (0..length)
                .map(|i| read(text_ptr + i as u32))
                .collect::<Option<Vec<_>>>()?;
            Some(TextEditSnapshot {
                view_rect: (
                    word(read, ptr + 8)? as i16,
                    word(read, ptr + 10)? as i16,
                    word(read, ptr + 12)? as i16,
                    word(read, ptr + 14)? as i16,
                ),
                text,
                selection: (
                    usize::from(word(read, ptr + 0x20)?),
                    usize::from(word(read, ptr + 0x22)?),
                ),
                active: word(read, ptr + 0x24)? != 0,
                justification: word(read, ptr + 0x3a)? as i16,
                line_count: usize::from(word(read, ptr + 0x5e)?),
            })
        })
        .collect();
    let private_scrap_length = usize::from(word(read, 0x0ab0).unwrap_or(0));
    let handle = long(read, 0x0ab4).unwrap_or(0);
    let ptr = if handle != 0 {
        long(read, handle).unwrap_or(0)
    } else {
        0
    };
    let private_scrap = if ptr != 0 {
        (0..private_scrap_length)
            .filter_map(|i| read(ptr + i as u32))
            .collect()
    } else {
        Vec::new()
    };
    TextEditManagerSnapshot {
        records,
        private_scrap_length,
        private_scrap,
    }
}
