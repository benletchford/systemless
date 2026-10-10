//! Native text-service adapter. Guest bytes and selection remain authoritative.
use super::{Command, Demo};
use gpui_kit::*;
use std::ops::Range;

pub(super) fn text_range_width(record: &systemless::runner::TextEditSnapshot, range: Range<usize>) -> Option<i16> {
    if record.styled { return record.guest_styled_range_width(range); }
    let line = super::super::text::ClassicLine::plain(record.text.get(range)?, record.font, record.size);
    i16::try_from(*line.positions.last()?).ok()
}

pub(super) fn text_line_geometry(record: &systemless::runner::TextEditSnapshot, index: usize)
    -> Option<systemless::runner::TextEditLineGeometry> {
    if record.styled { return record.guest_styled_line_geometry(index).map(|(geometry, _)| geometry); }
    let starts = record.line_starts.as_ref()?;
    let start = *starts.get(index)?;
    let mut end = *starts.get(index + 1)?;
    while end > start && matches!(record.text.get(end - 1), Some(b' ' | b'\r' | b'\n')) { end -= 1; }
    record.line_geometry(index, text_range_width(record, start..end)?)
}

impl Demo {
    pub(super) fn synchronize_composition(&mut self, window: &Window, _: &mut Context<Self>) {
        let eligible = self.focus.is_focused(window) && self.host_active != Some(false)
            && self.open_menus.is_empty() && !self.guest_menu_tracking && self.guest_popup.is_none()
            && self.standard_file.is_none() && !self.dialogs.iter().any(|dialog| dialog.visible && dialog.active);
        let mut records = self.text_edits.iter().filter(|record| record.active && record.drawing_intact);
        let record = records.next();
        let owner = if eligible && records.next().is_none() {
            record.filter(|record| record.global_view_rect.is_some()).map(|record|
                super::super::input::TextInputOwner { identity: (record.guest_id, record.generation),
                    port: record.owner_port, text: record.text.clone(), selection: record.selection.0..record.selection.1 })
        } else { None };
        self.composition.synchronize(owner);
    }

    fn composition_text(&self) -> Option<String> {
        let owner = self.composition.owner()?;
        let mut text = systemless::systems::macintosh::mac_roman::decode_mac_roman(&owner.text);
        if let Some(preedit) = &self.composition.preedit {
            let start = text.char_indices().nth(owner.selection.start).map_or(text.len(), |(offset, _)| offset);
            let end = text.char_indices().nth(owner.selection.end).map_or(text.len(), |(offset, _)| offset);
            text.replace_range(start..end, &preedit.text);
        }
        Some(text)
    }

    fn composition_range_allowed(&self, range: Option<&Range<usize>>) -> bool {
        let Some(owner) = self.composition.owner() else { return false; };
        range.is_none_or(|range| range == &owner.selection || self.composition.preedit.as_ref().is_some_and(|preedit|
            range == &(owner.selection.start..owner.selection.start + preedit.text.encode_utf16().count())))
    }
}

impl EntityInputHandler for Demo {
    fn text_for_range(&mut self, range: Range<usize>, adjusted: &mut Option<Range<usize>>,
        _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let text = self.composition_text()?;
        let units: Vec<_> = text.encode_utf16().collect();
        if range.start > range.end || range.end > units.len() { return None; }
        let result = String::from_utf16(&units[range.clone()]).ok()?;
        *adjusted = Some(range);
        Some(result)
    }
    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let owner = self.composition.owner()?;
        let range = self.composition.preedit.as_ref().map_or_else(|| owner.selection.clone(), |preedit|
            owner.selection.start + preedit.selection_utf16.start..owner.selection.start + preedit.selection_utf16.end);
        Some(UTF16Selection { range, reversed: false })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let owner = self.composition.owner()?;
        let preedit = self.composition.preedit.as_ref()?;
        Some(owner.selection.start..owner.selection.start + preedit.text.encode_utf16().count())
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.composition.preedit.as_ref().map(|preedit| preedit.text.clone()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }
    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str,
        _: &mut Window, cx: &mut Context<Self>) {
        if !self.composition_range_allowed(range.as_ref()) { return; }
        if let Some((owner, bytes)) = self.composition.commit(text) {
            let _ = self.commands.send(Command::CommitText(owner, bytes));
            cx.notify();
        }
    }
    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str,
        selected: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        if !self.composition_range_allowed(range.as_ref()) { return; }
        let end = text.encode_utf16().count();
        if self.composition.mark(text, selected.unwrap_or(end..end)) { cx.notify(); }
    }
    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>,
        _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let owner = self.composition.owner()?;
        let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
        // Geometry is meaningful only for the currently painted guest text.
        // Pending commits and Unicode preedit need their own painted layout.
        if self.composition.preedit.is_some() || record.text != owner.text
            || range.start > range.end || range.end > owner.text.len()
            || !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let starts = record.line_starts.as_ref()?;
        let index = starts.partition_point(|start| *start <= range.start).saturating_sub(1).min(starts.len().checked_sub(2)?);
        let geometry = text_line_geometry(record, index)?;
        let start = starts[index];
        let end = range.end.min(starts[index + 1]);
        let x = i32::from(geometry.left) + i32::from(text_range_width(record, start..range.start)?);
        let right = if range.is_empty() { x + 1 } else {
            i32::from(geometry.left) + i32::from(text_range_width(record, start..end)?)
        };
        let dest = record.global_dest_rect?;
        let view = record.global_view_rect?;
        let dx = i32::from(dest.1) - i32::from(record.dest_rect.1);
        let dy = i32::from(dest.0) - i32::from(record.dest_rect.0);
        let left = (dx + x).max(i32::from(view.1));
        let right = (dx + right).min(i32::from(view.3));
        let top = (dy + i32::from(geometry.top)).max(i32::from(view.0));
        let bottom = (dy + i32::from(geometry.top) + i32::from(geometry.height)).min(i32::from(view.2));
        if left >= right || top >= bottom { return None; }
        Some(Bounds::new(point(px(self.display_origin.0 + left as f32 * self.display_scale),
            px(self.display_origin.1 + top as f32 * self.display_scale)),
            size(px((right - left) as f32 * self.display_scale), px((bottom - top) as f32 * self.display_scale))))
    }
    fn character_index_for_point(&mut self, point: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let owner = self.composition.owner()?;
        let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
        if self.composition.preedit.is_some() || record.text != owner.text
            || !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let x = (f32::from(point.x) - self.display_origin.0) / self.display_scale;
        let y = (f32::from(point.y) - self.display_origin.1) / self.display_scale;
        let view = record.global_view_rect?;
        if !x.is_finite() || !y.is_finite() || x < f32::from(view.1) || x >= f32::from(view.3)
            || y < f32::from(view.0) || y >= f32::from(view.2) { return None; }
        let dest = record.global_dest_rect?;
        let local_x = x - f32::from(dest.1) + f32::from(record.dest_rect.1);
        let local_y = y - f32::from(dest.0) + f32::from(record.dest_rect.0);
        let starts = record.line_starts.as_ref()?;
        for index in 0..record.line_count {
            let geometry = text_line_geometry(record, index)?;
            if local_y < f32::from(geometry.top) || local_y >= f32::from(geometry.top) + f32::from(geometry.height) { continue; }
            let start = *starts.get(index)?;
            let end = *starts.get(index + 1)?;
            let mut left = i32::from(geometry.left);
            for offset in start..end {
                if matches!(record.text.get(offset), Some(b'\r' | b'\n')) { return Some(offset); }
                let right = left + i32::from(text_range_width(record, offset..offset + 1)?);
                if local_x < (left + right) as f32 / 2. { return Some(offset); }
                left = right;
            }
            return Some(end);
        }
        None
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.composition_text().map(|text| text.encode_utf16().count())
    }
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && self.composition.owner().is_some()
    }
}
