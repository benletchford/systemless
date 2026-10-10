//! Native text-service adapter. Guest bytes and selection remain authoritative.
use super::{Command, Demo};
use gpui_kit::*;
use std::ops::Range;

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
        let offset = range.start.min(owner.text.len());
        let starts = record.line_starts.as_ref()?;
        let index = starts.partition_point(|start| *start <= offset).saturating_sub(1).min(starts.len().checked_sub(2)?);
        let (geometry, runs) = record.guest_styled_line_geometry(index)?;
        let x = runs.iter().find_map(|(bytes, positions)|
            (bytes.start <= offset && offset <= bytes.end).then(|| positions.get(offset - bytes.start).copied()).flatten())?;
        let dest = record.global_dest_rect?;
        let view = record.global_view_rect?;
        let left = (i32::from(dest.1) - i32::from(record.dest_rect.1) + i32::from(x)).clamp(i32::from(view.1), i32::from(view.3));
        let top = (i32::from(dest.0) - i32::from(record.dest_rect.0) + i32::from(geometry.top)).clamp(i32::from(view.0), i32::from(view.2));
        Some(Bounds::new(point(px(self.display_origin.0 + left as f32 * self.display_scale),
            px(self.display_origin.1 + top as f32 * self.display_scale)),
            size(px(self.display_scale), px(f32::from(geometry.height.max(1)) * self.display_scale))))
    }
    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.composition_text().map(|text| text.encode_utf16().count())
    }
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && self.composition.owner().is_some()
    }
}
