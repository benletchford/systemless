//! Architecture-neutral TextEdit editing semantics.
//!
//! The 68K trap and PowerPC import layers translate guest ABI and memory into
//! this model, then serialize the result back into their respective `TERec`.

mod drawing;
pub(crate) use drawing::TextEditDrawing;

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

/// Changed byte span, anchored at the guest's original selection rather than
/// moving an edit past repeated characters with the same value.
pub(crate) fn edited_byte_span(old: &[u8], new: &[u8], selection_start: usize) -> Option<(usize, usize, usize)> {
    if old == new { return None; }
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let start = prefix.min(selection_start);
    let room = old.len().min(new.len()) - start;
    let suffix = old[start..].iter().rev().zip(new[start..].iter().rev())
        .take(room).take_while(|(a, b)| a == b).count();
    Some((start, old.len() - start - suffix, new.len() - start - suffix))
}

/// Architecture-neutral movement of style ownership with edited bytes. CPU
/// adapters resolve insertion/following styles and serialize their own ABI.
pub(crate) fn style_runs_after_edit<S: Copy + Eq>(
    runs: &[(usize, S)],
    (start, deleted, inserted): (usize, usize, usize),
    inserted_style: S,
    following_style: S,
    new_len: usize,
) -> Vec<(usize, S)> {
    let mut edited = Vec::with_capacity(runs.len() + 2);
    for &(offset, style) in runs {
        if offset < start { edited.push((offset, style)); }
        else if offset > start + deleted { edited.push((offset - deleted + inserted, style)); }
    }
    if inserted > 0 { edited.push((start, inserted_style)); }
    edited.push((start + inserted, following_style));
    edited.sort_by_key(|(offset, _)| *offset);
    let mut folded: Vec<(usize, S)> = Vec::with_capacity(edited.len());
    for (offset, style) in edited {
        if offset >= new_len && offset != 0 { continue; }
        match folded.last_mut() {
            Some((last_offset, last_style)) if *last_offset == offset => *last_style = style,
            Some((_, last_style)) if *last_style == style => {}
            _ => folded.push((offset, style)),
        }
    }
    if let Some(first) = folded.first_mut() { first.0 = 0; }
    folded
}

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

    pub(crate) fn move_caret_to(&mut self, offset: usize) {
        let caret = offset.min(self.text.len());
        self.selection = caret..caret;
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
            // Vertical navigation needs the guest's wrapped line metrics;
            // both Toolbox gateways resolve its target before applying keys.
            0x1e | 0x1f => {}
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
    #[test]
    fn paint_spacing_eligibility_uses_guest_cpu_units() {
        use super::{TextEditPaintSnapshot, TextEditCharExtraSnapshot};
        let mut paint = TextEditPaintSnapshot { depth: 8, mode: 1,
            char_extra: TextEditCharExtraSnapshot::ClassicFixed(0x8000),
            space_extra: 0x8000, style_ink: Vec::new(), solid_caret: None };
        assert!(paint.supports_zero_spacing_src_or());
        paint.space_extra = -0x8000;
        assert!(!paint.supports_zero_spacing_src_or());
        paint.char_extra = TextEditCharExtraSnapshot::PpcPacked(0);
        assert!(paint.supports_zero_spacing_src_or(), "PPC drawing does not apply SpaceExtra");
        paint.char_extra = TextEditCharExtraSnapshot::PpcPacked(1);
        assert!(!paint.supports_zero_spacing_src_or());
        paint.char_extra = TextEditCharExtraSnapshot::ClassicFixed(-0x8000);
        paint.space_extra = 0;
        assert!(!paint.supports_zero_spacing_src_or());
        paint.char_extra = TextEditCharExtraSnapshot::ClassicFixed(0);
        paint.mode = 2;
        assert!(!paint.supports_zero_spacing_src_or(), "XOR requires destination-dependent rendering");
    }

    #[test]
    fn styled_edits_keep_insertion_anchor_and_following_attributes() {
        let original = b"aaaaaa";
        let inserted = b"aaaaaaa";
        let change = super::edited_byte_span(original, inserted, 2).unwrap();
        assert_eq!(change, (2, 0, 1), "same-valued bytes must not move insertion to the end");
        let styles = [(0, "red"), (2, "blue"), (4, "green")];
        let shifted = super::style_runs_after_edit(&styles, change, "purple", "blue", inserted.len());
        assert_eq!(shifted, [(0, "red"), (2, "purple"), (3, "blue"), (5, "green")]);
        let removed = super::style_runs_after_edit(&shifted, (2, 1, 0), "purple", "blue", original.len());
        assert_eq!(removed, styles, "deleting the inserted byte restores each original character's style");
        assert_eq!(super::edited_byte_span(original, original, 2), None);
    }

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
    fn vertical_arrow_keys_do_not_insert_control_bytes() {
        for key in [0x1e, 0x1f] {
            let mut buffer = TextEditBuffer::new(b"abc\rdef".to_vec(), 2, 2);
            buffer.apply_key(key);
            assert_eq!(buffer.text(), b"abc\rdef", "navigation must preserve guest text");
        }
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
        assert_eq!(record.visible_style_runs(0).unwrap().iter()
            .map(|(range, run)| (range.clone(), run.start)).collect::<Vec<_>>(), vec![(0..1, 0)]);
        assert_eq!(record.visible_style_runs(1).unwrap().iter()
            .map(|(range, run)| (range.clone(), run.color)).collect::<Vec<_>>(), vec![(2..4, (0xffff, 0, 0))]);
        let mut split = record.clone();
        split.text = b"a\x8eb \r".to_vec();
        split.line_count = 1;
        split.line_starts = Some(vec![0, 5]);
        assert_eq!(split.visible_style_runs(0).unwrap().iter()
            .map(|(range, run)| (range.clone(), run.start)).collect::<Vec<_>>(),
            vec![(0..2, 0), (2..3, 2)], "retain byte boundaries while trimming styled whitespace");
        split.dest_rect = (10, 20, 100, 200);
        split.justification = -1;
        let (geometry, placed) = split.styled_line_geometry(0, |byte, style| {
            if style.start == 0 { if byte == 0x8e { 7 } else { 3 } } else { 11 }
        }).unwrap();
        assert_eq!(geometry.left, 179, "right alignment uses all mixed-run advances");
        assert_eq!(placed, vec![(0..2, vec![179, 182, 189]), (2..3, vec![189, 200])]);
        split.justification = 1;
        let (geometry, placed) = split.styled_line_geometry(0, |_, _| 5).unwrap();
        assert_eq!(geometry.left, 102);
        assert_eq!(placed, vec![(0..2, vec![102, 107, 112]), (2..3, vec![112, 117])]);
        let mut selected = split.clone();
        selected.active = true;
        selected.selection = (0, 5);
        selected.justification = 0;
        selected.view_rect = selected.dest_rect;
        for policy in [super::TextEditLineLayoutPolicy::CumulativeGuestMetrics,
            super::TextEditLineLayoutPolicy::PpcRunMetrics] {
            selected.line_layout_policy = policy;
            let end = if policy == super::TextEditLineLayoutPolicy::PpcRunMetrics { 3 } else { 5 };
            let geometry = selected.guest_styled_line_geometry(0).unwrap().0;
            let rect = selected.guest_styled_selection_rect(0).unwrap().unwrap();
            assert_eq!(rect, (10, 20, 10 + geometry.height,
                geometry.left + selected.guest_styled_range_width(0..end).unwrap()));
            selected.selection = (5, 0);
            assert_eq!(selected.guest_styled_selection_rect(0), Some(Some(rect)), "normalize reversed selection");
            selected.active = false;
            assert_eq!(selected.guest_styled_selection_rect(0), Some(None));
            selected.active = true;
            selected.selection = (0, 5);
            selected.view_rect = (12, 22, 15, 25);
            assert_eq!(selected.guest_styled_selection_rect(0), Some(Some((12, 22, 15, 25))), "clip decoration to the guest view");
            selected.view_rect = selected.dest_rect;
        }
        selected.line_layout_policy = super::TextEditLineLayoutPolicy::PpcRunMetrics;
        selected.dest_rect.0 = 32760;
        selected.view_rect = (0, 20, i16::MAX, 200);
        selected.selection = (0, 3);
        let geometry = selected.guest_styled_line_geometry(0).unwrap().0;
        let rect = selected.guest_styled_selection_rect(0).unwrap().unwrap();
        assert_eq!(rect.0, i16::MAX - geometry.ascent,
            "PPC selection derives its top from the saturated drawing baseline");
        assert_eq!(rect.2, i16::MAX);
        assert!(selected.guest_styled_range_width(0..6).is_none());
        assert!(selected.guest_styled_selection_rect(1).is_none());
        for policy in [super::TextEditLineLayoutPolicy::CumulativeGuestMetrics,
            super::TextEditLineLayoutPolicy::PpcRunMetrics] {
            split.line_layout_policy = policy;
            let (geometry, placed) = split.styled_line_geometry(0, |_, _| i16::MAX).unwrap();
            assert_eq!(geometry, split.line_geometry(0, i16::MAX).unwrap());
            assert_eq!(placed[0].1.last(), placed[1].1.first(), "style boundary has one insertion position");
        }
        split.dest_rect.1 = -20000;
        split.justification = 0;
        let (_, placed) = split.styled_line_geometry(0, |_, _| i16::MAX).unwrap();
        assert_eq!(placed[0].1, vec![-19999, 12768, i16::MAX],
            "clamp the guest pen after applying the scroll origin");
        for policy in [super::TextEditLineLayoutPolicy::CumulativeGuestMetrics,
            super::TextEditLineLayoutPolicy::PpcRunMetrics] {
            split.line_layout_policy = policy;
            let expected = split.styled_line_geometry(0, |byte, run| {
                super::styled_byte_advance(policy, run.font, run.size, run.face, byte)
            });
            assert_eq!(split.guest_styled_line_geometry(0), expected);
        }
        split.style_runs.as_mut().unwrap()[1].start = 0;
        assert!(split.visible_style_runs(0).is_none(), "reject overlapping style ownership");
        assert!(split.styled_line_geometry(0, |_, _| 1).is_none());
        let mut layout = record.clone();
        layout.dest_rect = (10, 20, 100, 200);
        layout.view_rect = layout.dest_rect;
        assert_eq!(layout.line_geometry(1, 80), Some(super::TextEditLineGeometry {
            top: 24, left: 21, height: 18, ascent: 13,
        }));
        layout.justification = 1;
        assert_eq!(layout.line_geometry(1, 80).unwrap().left, 70);
        layout.justification = -1;
        assert_eq!(layout.line_geometry(1, 80).unwrap().left, 120);
        layout.line_layout_policy = super::TextEditLineLayoutPolicy::PpcRunMetrics;
        // Deferred style changes retain stored origins but paint new run metrics.
        layout.line_metrics = Some(vec![(42, 40), (60, 59)]);
        assert_eq!(layout.line_geometry(1, 80), Some(super::TextEditLineGeometry {
            top: 52, left: 120, height: 18, ascent: 13,
        }));
        layout.line_metrics = Some(vec![(14, 11), (18, 13)]);
        layout.active = true;
        layout.selection = (0, 4);
        let first = layout.guest_styled_selection_rect(0).unwrap().unwrap();
        let second = layout.guest_styled_selection_rect(1).unwrap().unwrap();
        assert_eq!(first.2, second.0, "mixed-height PPC selection follows glyph origins");
        layout.selection = (1, 1);
        layout.clips_line_offsets_to_visible_text = true;
        assert_eq!(layout.caret_line(), Some((0, 1)), "styled native wrap caret trims CR");
        assert_eq!(layout.guest_styled_caret_rect(0, 4), Some(Some((10, 199, 24, 200))),
            "PPC caret trims CR and retains its native one-pixel width");
        assert_eq!(layout.guest_styled_caret_rect(1, 4), Some(None), "one line owns the caret");
        layout.selection = (2, 2);
        assert_eq!(layout.caret_line(), Some((1, 0)), "after CR belongs to the following row");
        assert_eq!(layout.guest_styled_caret_rect(0, 4), Some(None));
        let mut soft_wrap = layout.clone();
        soft_wrap.text[1] = b'a';
        assert_eq!(soft_wrap.caret_line(), Some((0, 2)), "soft wraps retain preceding-row affinity");
        layout.line_layout_policy = super::TextEditLineLayoutPolicy::CumulativeGuestMetrics;
        layout.line_metrics = Some(vec![(14, 11), (18, 13)]);
        layout.clips_line_offsets_to_visible_text = false;
        layout.view_rect.0 = 25;
        assert_eq!(layout.caret_line(), Some((1, 0)), "styled classic caret follows visible cumulative lines");
        let left = layout.guest_styled_line_geometry(1).unwrap().0.left;
        assert_eq!(layout.guest_styled_caret_rect(1, 3), Some(Some((25, left, 42, left + 3))),
            "classic caret keeps the supplied width and clips a partial line");
        layout.caret_visible = false;
        assert_eq!(layout.guest_styled_caret_rect(1, 3), Some(None));
        layout.caret_visible = true;
        layout.active = false;
        assert_eq!(layout.guest_styled_caret_rect(1, 3), Some(None));
        layout.active = true;
        layout.selection = (2, 3);
        assert_eq!(layout.guest_styled_caret_rect(1, 3), Some(None));
        layout.selection = (2, 2);
        assert_eq!(layout.line_geometry(2, 80), None);
        let mut scrolled = layout.clone();
        scrolled.styled = false;
        scrolled.line_count = 2001;
        scrolled.line_height = 20;
        scrolled.font_ascent = 11;
        scrolled.dest_rect.0 = -20_000;
        assert_eq!(scrolled.line_geometry(2000, 0).unwrap().top, 20_000,
            "classic cumulative line placement includes the negative scroll origin before clamping");
        scrolled.line_layout_policy = super::TextEditLineLayoutPolicy::PpcRunMetrics;
        assert_eq!(scrolled.line_geometry(2000, 0).unwrap().top, 12_767,
            "preserve native drawing's saturating indexed-height calculation");
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

/// Canonical TextEdit width policy used by layout, selection and TEClick.
/// 68k applies style extras after integer strike scaling; PPC scales the
/// styled advance with its Font Manager ratio. Keep these policies distinct.
pub(crate) fn styled_byte_advance(
    policy: TextEditLineLayoutPolicy, font: i16, size: i16, face: u8, byte: u8,
) -> i16 {
    use crate::quickdraw::{fonts, text};
    let style = text::QuickDrawTextStyle::from_bits(face);
    match policy {
        TextEditLineLayoutPolicy::CumulativeGuestMetrics => {
            let size = if size == 0 { 0 } else { size.max(1) };
            let (_, scale) = fonts::get_font_face_scaled(font, size);
            let advance = text::get_glyph(font, size, byte as char)
                .map_or(6, |(glyph, _)| i16::from(glyph.advance));
            advance * scale + style.advance_extra() as i16
        }
        TextEditLineLayoutPolicy::PpcRunMetrics => {
            let (strike, numerator, denominator) = fonts::get_font_face_scale_ratio(font, size);
            let advance = text::get_glyph(font, strike.size, byte as char)
                .map_or(6, |(glyph, _)| i32::from(glyph.advance));
            let scaled = style.glyph_advance(advance).saturating_mul(numerator)
                .saturating_add(denominator / 2) / denominator;
            scaled.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
        }
    }
}

/// Line placement used by the owning CPU's TextEdit drawing path.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextEditLineLayoutPolicy {
    /// 68k advances by each canonical LHElement height.
    CumulativeGuestMetrics,
    /// PPC recomputes mixed-run metrics and places line i at i * its height.
    PpcRunMetrics,
}

/// Canonical port-local line anchors, before the shared scene transform.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextEditLineGeometry {
    pub top: i16,
    pub left: i16,
    pub height: i16,
    pub ascent: i16,
}

/// Resolved screen ink, distinct from the canonical RGB16 style intent.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditInkSnapshot {
    pub pixel: u16,
    pub rgb: [u8; 3],
    pub inverted_rgb: [u8; 3],
}

/// The CPU's actual CharExtra representation, not host letter spacing.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextEditCharExtraSnapshot {
    ClassicFixed(i32),
    PpcPacked(i16),
}

/// Read-only paint inputs for a recognized styled TextEdit screen port.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditPaintSnapshot {
    pub depth: u16,
    pub mode: i16,
    pub char_extra: TextEditCharExtraSnapshot,
    pub space_extra: i32,
    /// One ink per canonical style run, in the same order.
    pub style_ink: Vec<TextEditInkSnapshot>,
    /// Solid native caret fragment retained at drawing time, before restoring
    /// the caller's foreground. Missing evidence retains guest caret rendering.
    pub solid_caret: Option<((i16, i16, i16, i16), TextEditInkSnapshot)>,
}

impl TextEditPaintSnapshot {
    /// Eligibility of the currently verified zero-spacing binary run recipes.
    /// PPC draws ignore SpaceExtra; classic draw_char uses its integer part.
    pub fn supports_zero_spacing_src_or(&self) -> bool {
        self.mode == 1 && match self.char_extra {
            TextEditCharExtraSnapshot::ClassicFixed(value) => value >> 16 == 0 && self.space_extra >> 16 == 0,
            TextEditCharExtraSnapshot::PpcPacked(value) => value == 0,
        }
    }
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
    pub line_layout_policy: TextEditLineLayoutPolicy,
    pub paint: Option<TextEditPaintSnapshot>,
}

impl TextEditSnapshot {
    /// Intersect canonical style runs with one guest-wrapped visible line.
    /// Ranges remain absolute Macintosh Roman byte offsets. Trailing spaces
    /// and line breaks are omitted from ink, as in both guest draw paths;
    /// line metrics still include the complete untrimmed range.
    pub fn visible_style_runs(&self, index: usize) -> Option<Vec<(Range<usize>, &TextEditStyleRunSnapshot)>> {
        if !self.styled || index >= self.line_count { return None; }
        let starts = self.line_starts.as_ref()?;
        let start = *starts.get(index)?;
        let mut end = *starts.get(index + 1)?;
        self.text.get(start..end)?;
        while end > start && matches!(self.text[end - 1], b' ' | b'\r' | b'\n') {
            end -= 1;
        }
        let runs = self.style_runs.as_ref()?;
        if runs.first()?.start != 0
            || runs.iter().any(|run| run.start > self.text.len())
            || runs.windows(2).any(|pair| pair[0].start >= pair[1].start)
        { return None; }
        let mut visible = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let run_end = runs.get(index + 1).map_or(self.text.len(), |next| next.start);
            let left = start.max(run.start);
            let right = end.min(run_end);
            if left < right { visible.push((left..right, run)); }
        }
        Some(visible)
    }

    /// Visible styled insertion positions measured exactly as the guest's
    /// TextEdit layout and TEClick paths. Painting may additionally depend on
    /// port spacing and style synthesis; these positions do not grant ownership.
    pub fn guest_styled_line_geometry(
        &self, index: usize,
    ) -> Option<(TextEditLineGeometry, Vec<(Range<usize>, Vec<i16>)>)> {
        self.styled_line_geometry(index, |byte, run| {
            styled_byte_advance(self.line_layout_policy, run.font, run.size, run.face, byte)
        })
    }

    /// Place visible style runs using advances supplied by the owning guest
    /// drawing path. The callback receives Mac Roman bytes and canonical styles;
    /// it must retain guest strike/scaling/spacing policy, not host shaping.
    /// The returned positions include every insertion boundary, including at a
    /// style change. Each run shares the CPU-resolved line baseline.
    pub fn styled_line_geometry(
        &self, index: usize,
        mut advance: impl FnMut(u8, &TextEditStyleRunSnapshot) -> i16,
    ) -> Option<(TextEditLineGeometry, Vec<(Range<usize>, Vec<i16>)>)> {
        let runs = self.visible_style_runs(index)?;
        let mut pen = 0i16;
        let mut placed = Vec::with_capacity(runs.len());
        for (range, style) in runs {
            let mut positions = Vec::with_capacity(range.len() + 1);
            positions.push(pen);
            for &byte in self.text.get(range.clone())? {
                let width = advance(byte, style);
                pen = pen.saturating_add(width);
                positions.push(width);
            }
            placed.push((range, positions));
        }
        let geometry = self.line_geometry(index, pen)?;
        // Measure width separately from replaying the pen: clamping a relative
        // offset before adding a negative scroll origin loses visible positions.
        let mut pen = geometry.left;
        for (_, positions) in &mut placed {
            positions[0] = pen;
            for position in &mut positions[1..] {
                pen = pen.saturating_add(*position);
                *position = pen;
            }
        }
        Some((geometry, placed))
    }

    /// Measure a canonical styled byte range using the owning CPU's TEClick
    /// policy, including trailing spaces/returns when the caller requests them.
    pub fn guest_styled_range_width(&self, range: Range<usize>) -> Option<i16> {
        if !self.styled { return None; }
        let bytes = self.text.get(range.clone())?;
        let runs = self.style_runs.as_ref()?;
        if runs.first()?.start != 0 || runs.iter().any(|run| run.start > self.text.len())
            || runs.windows(2).any(|pair| pair[0].start >= pair[1].start) { return None; }
        let mut width = 0i16;
        for (offset, &byte) in bytes.iter().enumerate() {
            let run = runs.iter().rev().find(|run| run.start <= range.start + offset)?;
            width = width.saturating_add(styled_byte_advance(self.line_layout_policy,
                run.font, run.size, run.face, byte));
        }
        Some(width)
    }

    /// Active classic selection geometry, port-local and clipped to viewRect.
    /// Some(None) means no visible selection; None means unsupported/invalid
    /// geometry. Theme painting and inactive outline highlighting are separate.
    pub fn guest_styled_selection_rect(&self, index: usize) -> Option<Option<(i16, i16, i16, i16)>> {
        let (geometry, _) = self.guest_styled_line_geometry(index)?;
        if !self.active || self.selection.0 == self.selection.1 { return Some(None); }
        let starts = self.line_starts.as_ref()?;
        let line_start = *starts.get(index)?;
        let mut line_end = *starts.get(index + 1)?;
        if self.line_layout_policy == TextEditLineLayoutPolicy::PpcRunMetrics {
            while line_end > line_start && matches!(self.text[line_end - 1], b' ' | b'\r' | b'\n') { line_end -= 1; }
        }
        let start = self.selection.0.min(self.selection.1).max(line_start);
        let end = self.selection.0.max(self.selection.1).min(line_end);
        if start >= end { return Some(None); }
        let left_width = self.guest_styled_range_width(line_start..start)?;
        let right_width = self.guest_styled_range_width(line_start..end)?;
        let (mut left, right, top) = match self.line_layout_policy {
            TextEditLineLayoutPolicy::CumulativeGuestMetrics =>
                (geometry.left.checked_add(left_width)?, geometry.left.checked_add(right_width)?, geometry.top),
            TextEditLineLayoutPolicy::PpcRunMetrics => {
                let baseline = geometry.top.saturating_add(geometry.ascent);
                (geometry.left.saturating_add(left_width), geometry.left.saturating_add(right_width),
                    baseline.saturating_sub(geometry.ascent))
            }
        };
        if start == line_start && matches!(self.justification, 0 | -2) {
            left = match self.line_layout_policy {
                TextEditLineLayoutPolicy::CumulativeGuestMetrics => left.saturating_sub(1),
                TextEditLineLayoutPolicy::PpcRunMetrics => self.dest_rect.1,
            };
        }
        let rect = (top.max(self.view_rect.0), left.max(self.view_rect.1),
            top.saturating_add(geometry.height).min(self.view_rect.2), right.min(self.view_rect.3));
        Some((rect.0 < rect.2 && rect.1 < rect.3).then_some(rect))
    }

    /// Visible caret geometry for the guest-owned line and blink phase.
    /// Classic uses its theme caret width; PPC's native painter uses one pixel.
    /// Ink/pen patterns and any guest pixels outside viewRect remain separate.
    pub fn guest_styled_caret_rect(
        &self, index: usize, classic_width: i16,
    ) -> Option<Option<(i16, i16, i16, i16)>> {
        let (geometry, _) = self.guest_styled_line_geometry(index)?;
        let Some((owner, offset)) = self.caret_line() else { return Some(None); };
        if owner != index { return Some(None); }
        let start = *self.line_starts.as_ref()?.get(index)?;
        let width = self.guest_styled_range_width(start..start.checked_add(offset)?)?;
        let mut left = geometry.left.checked_add(width)?;
        if offset > 0 { left = left.saturating_sub(1); }
        let caret_width = match self.line_layout_policy {
            TextEditLineLayoutPolicy::CumulativeGuestMetrics => classic_width.max(1),
            TextEditLineLayoutPolicy::PpcRunMetrics => 1,
        };
        let rect = (geometry.top.max(self.view_rect.0), left.max(self.view_rect.1),
            geometry.top.saturating_add(geometry.height).min(self.view_rect.2),
            left.saturating_add(caret_width).min(self.view_rect.3));
        Some((rect.0 < rect.2 && rect.1 < rect.3).then_some(rect))
    }

    fn metrics_for_line(&self, index: usize) -> Option<(i16, i16)> {
        if index >= self.line_count { return None; }
        let (height, ascent) = if !self.styled {
            (self.line_height, self.font_ascent)
        } else if self.line_layout_policy == TextEditLineLayoutPolicy::CumulativeGuestMetrics {
            *self.line_metrics.as_ref()?.get(index)?
        } else {
            let starts = self.line_starts.as_ref()?;
            let (start, end) = (*starts.get(index)?, *starts.get(index + 1)?);
            self.text.get(start..end)?;
            let runs = self.style_runs.as_ref()?;
            if runs.first()?.start != 0 { return None; }
            let style = |offset| runs.iter().rev().find(|run| run.start <= offset);
            if start == end {
                let run = style(start)?;
                (run.line_height, run.ascent)
            } else {
                let mut height = 1;
                let mut ascent = 0;
                for (index, run) in runs.iter().enumerate() {
                    let run_end = runs.get(index + 1).map_or(self.text.len(), |next| next.start);
                    if run.start < end && run_end > start {
                        height = height.max(run.line_height);
                        ascent = ascent.max(run.ascent);
                    }
                }
                (height, ascent)
            }
        };
        (height > 0 && ascent >= 0).then_some((height, ascent))
    }

    /// Preserve CPU-specific line stacking and guest justification. Width is
    /// the caller's guest-font advance of the trimmed visible line, never a
    /// host-shaped width. Styled runs retain their own font and colour intent.
    pub fn line_geometry(&self, index: usize, visible_width: i16) -> Option<TextEditLineGeometry> {
        let (height, ascent) = self.metrics_for_line(index)?;
        let mut top = self.dest_rect.0;
        match self.line_layout_policy {
            _ if self.styled => {
                for previous in 0..index {
                    // PPC painting and point lookup stack the stored LHTable.
                    // TESetStyle(false) changes runs without recalculating it.
                    let height = if self.line_layout_policy == TextEditLineLayoutPolicy::PpcRunMetrics {
                        self.line_metrics.as_ref()?.get(previous)?.0
                    } else { self.metrics_for_line(previous)?.0 };
                    if height <= 0 { return None; }
                    top = top.saturating_add(height);
                }
            }
            TextEditLineLayoutPolicy::CumulativeGuestMetrics => {
                // Equivalent to repeated positive-height additions, including
                // a deeply scrolled negative origin; clamp after adding it.
                top = (i32::from(top) + i32::try_from(index).ok()?.saturating_mul(i32::from(height)))
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            }
            TextEditLineLayoutPolicy::PpcRunMetrics => {
                top = top.saturating_add(i16::try_from(index).ok()?.saturating_mul(height));
            }
        }
        Some(TextEditLineGeometry {
            top, left: aligned_line_left(self.dest_rect.1, self.dest_rect.3,
                visible_width, self.justification, 1), height, ascent,
        })
    }

    /// Select exactly one guest caret owner. Hard breaks belong to the next row.
    /// Both draw paths choose the first matching line at a soft-wrap boundary;
    /// the 68k path considers only lines intersecting viewRect and falls back
    /// to the last visible line. PPC trims spaces and line-break bytes before
    /// measuring, while 68k measures through the canonical line end.
    pub fn caret_line(&self) -> Option<(usize, usize)> {
        if !self.active || !self.caret_visible || self.selection.0 != self.selection.1 {
            return None;
        }
        let starts = self.line_starts.as_ref()?;
        let mut fallback = None;
        for (index, span) in starts.windows(2).enumerate() {
            let geometry = self.line_geometry(index, 0)?;
            let top = i32::from(geometry.top);
            if !self.clips_line_offsets_to_visible_text
                && (top + i32::from(geometry.height) <= i32::from(self.view_rect.0)
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
            let after_hard_break = self.selection.0 == end && index + 2 < starts.len()
                && end > start && matches!(self.text[end - 1], b'\r' | b'\n');
            if !after_hard_break && self.selection.0 >= start && (self.selection.0 <= end
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
                line_layout_policy: TextEditLineLayoutPolicy::CumulativeGuestMetrics,
                paint: None,
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
