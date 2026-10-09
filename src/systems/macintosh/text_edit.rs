//! Architecture-neutral TextEdit editing semantics.
//!
//! The 68K trap and PowerPC import layers translate guest ABI and memory into
//! this model, then serialize the result back into their respective `TERec`.

mod drawing;
pub(crate) use drawing::TextEditDrawing;

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
    generations: HashMap<u32, u64>,
    next_generation: u64,
    click_tracking: Option<TextEditClickTracking>,
}

impl ProcessTextEditManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.records.is_empty() && self.handles.is_empty() && self.click_tracking.is_none()
    }

    pub(crate) fn register(&mut self, handle: u32) {
        if handle != 0 && self.handles.insert(handle) {
            self.next_generation = self.next_generation.saturating_add(1);
            self.generations.insert(handle, self.next_generation);
        }
    }

    #[cfg(test)]
    pub(crate) fn handles(&self) -> Vec<u32> {
        self.handles.iter().copied().collect()
    }

    pub(crate) fn identities(&self) -> Vec<(u32, u64)> {
        self.handles
            .iter()
            .map(|handle| (*handle, self.generations.get(handle).copied().unwrap_or(0)))
            .collect()
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
        self.generations.remove(handle);
        if self
            .click_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.handle == *handle)
        {
            self.click_tracking = None;
        }
    }

    pub(crate) fn clear_click_tracking(&mut self) {
        self.click_tracking = None;
    }

    pub(crate) fn take_click_tracking(&mut self) -> Option<TextEditClickTracking> {
        self.click_tracking.take()
    }

    pub(crate) fn retain_click_tracking(&mut self, tracking: TextEditClickTracking) {
        self.click_tracking = Some(tracking);
    }

    pub(crate) fn has_click_tracking(&self) -> bool {
        self.click_tracking.is_some()
    }

    pub(crate) fn has_classic_click_tracking(&self) -> bool {
        self.click_tracking
            .as_ref()
            .is_some_and(|tracking| !tracking.native)
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
    use super::{aligned_line_left, ProcessTextEditManagerState, TextEditBuffer};

    #[test]
    fn edit_handle_generation_changes_only_after_disposal() {
        let mut state = ProcessTextEditManagerState::default();
        state.register(0x1000);
        let first = state.identities()[0].1;
        state.register(0x1000);
        assert_eq!(state.identities(), vec![(0x1000, first)]);
        state.remove(&0x1000);
        state.register(0x1000);
        assert!(state.identities()[0].1 > first);
    }

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
    fn styled_snapshot_preserves_runs_colors_line_metrics_and_rejects_bad_indices() {
        fn word(memory: &mut [u8], address: usize, value: u16) {
            memory[address..address + 2].copy_from_slice(&value.to_be_bytes());
        }
        fn long(memory: &mut [u8], address: usize, value: u32) {
            memory[address..address + 4].copy_from_slice(&value.to_be_bytes());
        }
        let mut memory = vec![0; 0x800];
        for (address, value) in [(0x100, 0x200), (0x110, 0x300), (0x120, 0x400),
            (0x130, 0x500), (0x140, 0x600), (0x23e, 0x110), (0x24a, 0x120),
            (0x404, 0x130), (0x408, 0x140)] {
            long(&mut memory, address, value);
        }
        for (address, value) in [(0x218, 0xffff), (0x250, 0xffff), (0x23c, 4),
            (0x25e, 2), (0x260, 0), (0x262, 2), (0x264, 4), (0x220, 1), (0x222, 3),
            (0x400, 2), (0x402, 2), (0x414, 0), (0x416, 0), (0x418, 2), (0x41a, 1),
            (0x502, 14), (0x504, 11), (0x506, 3), (0x50a, 9),
            (0x514, 18), (0x516, 13), (0x518, 0), (0x51c, 12), (0x51e, 0xffff),
            (0x600, 14), (0x602, 11), (0x604, 18), (0x606, 13)] {
            word(&mut memory, address, value);
        }
        memory[0x51a] = 3;
        memory[0x300..0x304].copy_from_slice(b"a\rb\x8e");
        let snapshot = super::snapshot_guest_records(&[(0x100, 7)], &mut |address| memory.get(address as usize).copied());
        let record = &snapshot.records[0];
        assert!(record.styled);
        assert_eq!(record.selection, (1, 3));
        assert_eq!(record.line_starts, Some(vec![0, 2, 4]));
        assert_eq!(record.line_metrics, Some(vec![(14, 11), (18, 13)]));
        let runs = record.style_runs.as_ref().unwrap();
        assert_eq!((runs[0].start, runs[0].font, runs[0].size), (0, 3, 9));
        assert_eq!((runs[1].start, runs[1].face, runs[1].color), (2, 3, (0xffff, 0, 0)));
        word(&mut memory, 0x41a, 2);
        let invalid = super::snapshot_guest_records(&[(0x100, 7)], &mut |address| memory.get(address as usize).copied());
        assert_eq!(invalid.records[0].text, record.text);
        assert!(invalid.records[0].style_runs.is_none());
        assert!(invalid.records[0].line_metrics.is_none());
        long(&mut memory, 0x120, u32::MAX);
        assert!(super::snapshot_guest_records(&[(0x100, 7)], &mut |address| memory.get(address as usize).copied()).records[0].style_runs.is_none());
    }

    #[test]
    fn alignment_is_shared_for_every_guest_adapter() {
        assert_eq!(aligned_line_left(20, 200, 80, 0, 1), 21);
        assert_eq!(aligned_line_left(20, 200, 80, -2, 1), 21);
        assert_eq!(aligned_line_left(20, 200, 80, 1, 1), 70);
        assert_eq!(aligned_line_left(20, 200, 80, -1, 1), 120);
    }
}

/// One canonical style run, indexed by Macintosh Roman byte offset.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditStyleRunSnapshot {
    pub start: usize,
    pub font: i16,
    pub face: u8,
    pub size: i16,
    pub color: (u16, u16, u16),
    pub line_height: i16,
    pub ascent: i16,
}

/// Immutable guest TextEdit contents for fixture and diagnostic assertions.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditSnapshot {
    /// True only while the last guest drawing has a verified visible region.
    pub drawing_intact: bool,
    /// Screen rectangles covered by the last verified guest drawing.
    pub painted_regions: Vec<(i16, i16, i16, i16)>,
    pub guest_id: u32,
    pub generation: u64,
    pub owner_port: u32,
    pub global_dest_rect: Option<(i16, i16, i16, i16)>,
    pub global_view_rect: Option<(i16, i16, i16, i16)>,
    pub dest_rect: (i16, i16, i16, i16),
    pub view_rect: (i16, i16, i16, i16),
    pub text: Vec<u8>,
    pub selection: (usize, usize),
    pub active: bool,
    /// Guest-controlled blink phase; Systemless TEIdle uses zero for visible.
    pub caret_visible: bool,
    /// PPC clamps caret/selection measurement to the trimmed visible line end.
    pub clips_line_offsets_to_visible_text: bool,
    pub justification: i16,
    pub line_count: usize,
    /// Guest byte offsets for the start of each line and the final end offset.
    pub line_starts: Option<Vec<usize>>,
    pub line_height: i16,
    /// TERec fontAscent, measured from the line top to the guest baseline.
    pub font_ascent: i16,
    pub font: i16,
    pub face: u8,
    pub size: i16,
    pub styled: bool,
    /// None if the canonical style tables are absent or inconsistent.
    pub style_runs: Option<Vec<TextEditStyleRunSnapshot>>,
    /// Guest LHElement (height, ascent) for each displayed line.
    pub line_metrics: Option<Vec<(i16, i16)>>,
}

impl TextEditSnapshot {
    /// Select exactly one guest caret owner, retaining inclusive byte ends.
    /// Both draw paths choose the first matching line at a wrap boundary;
    /// the 68k path considers only lines intersecting viewRect and falls back
    /// to the last visible line. PPC trims spaces and line-break bytes before
    /// measuring, while 68k measures through the canonical line end.
    pub fn caret_line(&self) -> Option<(usize, usize)> {
        if self.styled || !self.active || !self.caret_visible || self.selection.0 != self.selection.1 {
            return None;
        }
        let starts = self.line_starts.as_ref()?;
        let mut fallback = None;
        for (index, span) in starts.windows(2).enumerate() {
            let top = i32::from(self.dest_rect.0) + index as i32 * i32::from(self.line_height);
            if !self.clips_line_offsets_to_visible_text
                && (top + i32::from(self.line_height) <= i32::from(self.view_rect.0)
                    || top >= i32::from(self.view_rect.2)) {
                continue;
            }
            let (start, end) = (span[0], span[1]);
            self.text.get(start..end)?;
            let mut measured_end = end;
            if self.clips_line_offsets_to_visible_text {
                while measured_end > start && matches!(self.text[measured_end - 1], b' ' | b'\r' | b'\n') {
                    measured_end -= 1;
                }
            }
            let offset = self.selection.0.min(measured_end).max(start) - start;
            fallback = Some((index, offset));
            if self.selection.0 >= start && (self.selection.0 <= end
                || self.clips_line_offsets_to_visible_text && index + 2 == starts.len()) {
                return fallback;
            }
        }
        if self.clips_line_offsets_to_visible_text { None } else { fallback }
    }

    /// Decode guest-defined lines without changing their byte offsets or wrapping.
    /// Inside Macintosh: Text (1993), pp. 2-66--2-68.
    pub fn display_lines(&self) -> Option<Vec<String>> {
        let starts = self.line_starts.as_ref()?;
        starts
            .windows(2)
            .map(|span| {
                let bytes = self.text.get(span[0]..span[1])?;
                let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
                Some(crate::mac_roman::decode_mac_roman(bytes))
            })
            .collect()
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditManagerSnapshot {
    pub records: Vec<TextEditSnapshot>,
    pub private_scrap_length: usize,
    pub private_scrap: Vec<u8>,
}

pub(crate) fn snapshot_guest_records(
    handles: &[(u32, u64)],
    read: &mut dyn FnMut(u32) -> Option<u8>,
) -> TextEditManagerSnapshot {
    fn word(read: &mut dyn FnMut(u32) -> Option<u8>, addr: u32) -> Option<u16> {
        Some(u16::from_be_bytes([read(addr)?, read(addr.checked_add(1)?)?]))
    }
    fn long(read: &mut dyn FnMut(u32) -> Option<u8>, addr: u32) -> Option<u32> {
        Some(u32::from_be_bytes([
            read(addr)?,
            read(addr.checked_add(1)?)?,
            read(addr.checked_add(2)?)?,
            read(addr.checked_add(3)?)?,
        ]))
    }
    fn styled_tables(
        read: &mut dyn FnMut(u32) -> Option<u8>, ptr: u32, length: usize, lines: usize,
    ) -> Option<(Vec<TextEditStyleRunSnapshot>, Vec<(i16, i16)>)> {
        let style_handle = long(read, ptr + 0x4a).filter(|value| *value != 0)?;
        let style = long(read, style_handle).filter(|value| *value != 0 && value.checked_add(0x14 + 4096 * 4).is_some())?;
        let runs = usize::from(word(read, style)?);
        let styles = usize::from(word(read, style + 2)?);
        if runs == 0 || runs > 4096 || styles == 0 || styles > 4096 || lines > 4096 {
            return None;
        }
        let table_handle = long(read, style + 4).filter(|value| *value != 0)?;
        let table = long(read, table_handle).filter(|value| *value != 0 && value.checked_add(4096 * 18).is_some())?;
        let lh_handle = long(read, style + 8).filter(|value| *value != 0)?;
        let lh = long(read, lh_handle).filter(|value| *value != 0 && value.checked_add(4096 * 4).is_some())?;
        let mut result = Vec::with_capacity(runs);
        for index in 0..runs {
            let run = style + 0x14 + index as u32 * 4;
            let start = usize::from(word(read, run)?);
            let style_index = usize::from(word(read, run + 2)?);
            if start > length || style_index >= styles
                || (index == 0 && start != 0)
                || result.last().is_some_and(|last: &TextEditStyleRunSnapshot| last.start >= start)
            {
                return None;
            }
            let element = table + style_index as u32 * 18;
            result.push(TextEditStyleRunSnapshot {
                start,
                font: word(read, element + 6)? as i16,
                face: read(element + 8)?,
                size: word(read, element + 10)? as i16,
                color: (word(read, element + 12)?, word(read, element + 14)?, word(read, element + 16)?),
                line_height: word(read, element + 2)? as i16,
                ascent: word(read, element + 4)? as i16,
            });
        }
        let metrics = (0..lines).map(|index| {
            let element = lh + index as u32 * 4;
            Some((word(read, element)? as i16, word(read, element + 2)? as i16))
        }).collect::<Option<Vec<_>>>()?;
        Some((result, metrics))
    }

    // TERec fields and private scrap are canonical guest memory on both
    // architectures. Inside Macintosh: Text (1993), pp. 2-64--2-69, 2-98.
    let records = handles
        .iter()
        .filter_map(|(handle, generation)| {
            let ptr = long(read, *handle).filter(|ptr| *ptr != 0)?;
            let length = usize::from(word(read, ptr + 0x3c)?);
            let text_handle = long(read, ptr + 0x3e)?;
            let text_ptr = long(read, text_handle)?;
            let text = (0..length)
                .map(|i| read(text_ptr + i as u32))
                .collect::<Option<Vec<_>>>()?;
            let line_count = usize::from(word(read, ptr + 0x5e)?);
            // lineStarts[0..nLines] are guest byte offsets. Reject an
            // incomplete or inconsistent table before any host presentation
            // uses it for wrapping or selection. Text (1993), pp. 2-66--2-68.
            let line_starts = (line_count <= length.saturating_add(1) && line_count <= 4096)
                .then(|| {
                    (0..=line_count)
                        .map(|index| word(read, ptr + 0x60 + index as u32 * 2).map(usize::from))
                        .collect::<Option<Vec<_>>>()
                })
                .flatten()
                .filter(|starts| {
                    starts.first() == Some(&0)
                        && starts.last() == Some(&length)
                        && starts.windows(2).all(|pair| pair[0] <= pair[1])
                });
            let line_height = word(read, ptr + 0x18)? as i16;
            let size = word(read, ptr + 0x50)? as i16;
            let styled = size == -1 || line_height == -1;
            let tables = styled.then(|| styled_tables(read, ptr, length, line_count)).flatten();
            let (style_runs, line_metrics) = tables.map_or((None, None), |(runs, metrics)| (Some(runs), Some(metrics)));
            Some(TextEditSnapshot {
                drawing_intact: false,
                painted_regions: Vec::new(),
                guest_id: *handle,
                generation: *generation,
                owner_port: long(read, ptr + 0x52)?,
                global_dest_rect: None,
                global_view_rect: None,
                dest_rect: (
                    word(read, ptr)? as i16,
                    word(read, ptr + 2)? as i16,
                    word(read, ptr + 4)? as i16,
                    word(read, ptr + 6)? as i16,
                ),
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
                // TERec internal caretState: Text (1993), pp. 2-64--2-69, 2-84.
                caret_visible: word(read, ptr + 0x38)? == 0,
                clips_line_offsets_to_visible_text: false,
                justification: word(read, ptr + 0x3a)? as i16,
                line_count,
                line_starts,
                line_height,
                font_ascent: word(read, ptr + 0x1a)? as i16,
                font: word(read, ptr + 0x4a)? as i16,
                face: read(ptr + 0x4c)?,
                size,
                styled,
                style_runs,
                line_metrics,
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
