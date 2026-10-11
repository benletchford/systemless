//! Native text-service adapter. Guest bytes and selection remain authoritative.
use super::{Command, Demo};
use gpui_kit::*;
use gpui_kit::component::ActiveTheme;
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

#[derive(Clone)]
struct MarkedRow { start: usize, positions: Vec<(usize, f32)>, origin: (f32, f32) }

#[derive(Clone)]
pub(super) struct PaintedComposition {
    owner: super::super::input::TextInputOwner,
    preedit: super::super::input::Preedit,
    record: systemless::runner::TextEditSnapshot,
    transform: ((f32, f32), f32),
    clip: (f32, f32, f32, f32),
    rows: Vec<MarkedRow>,
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

    /// Unicode staging belongs to the host text service. It uses host typography
    /// in a separate surface, never as a replacement for committed guest glyphs.
    pub(super) fn composition_surface(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let owner = self.composition.owner()?;
        let preedit = self.composition.preedit.clone()?;
        let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
        if record.text != owner.text || self.display_scale <= 0. { return None; }
        let starts = record.line_starts.as_ref()?;
        let index = starts.partition_point(|start| *start <= owner.selection.start).saturating_sub(1)
            .min(starts.len().checked_sub(2)?);
        let geometry = text_line_geometry(record, index)?;
        let dest = record.global_dest_rect?;
        let view = record.global_view_rect?;
        let x = i32::from(dest.1) - i32::from(record.dest_rect.1) + i32::from(geometry.left)
            + i32::from(text_range_width(record, starts[index]..owner.selection.start)?);
        let y = i32::from(dest.0) - i32::from(record.dest_rect.0) + i32::from(geometry.top);
        if x < i32::from(view.1) || x >= i32::from(view.3) || y < i32::from(view.0) || y >= i32::from(view.2) { return None; }
        let width = 240f32.min(self.width as f32 * self.display_scale).max(1.);
        let lines: Vec<String> = preedit.text.split('\n').map(str::to_owned).collect();
        let height = (lines.len().max(1) as f32 * 20. + 8.).min(self.height as f32 * self.display_scale);
        let left = (self.display_origin.0 + x as f32 * self.display_scale)
            .min(self.display_origin.0 + self.width as f32 * self.display_scale - width);
        let top = (self.display_origin.1 + (y + i32::from(geometry.height)) as f32 * self.display_scale)
            .min(self.display_origin.1 + self.height as f32 * self.display_scale - height);
        let cache = self.composition_geometry.clone();
        let cache_owner = owner.clone();
        let cache_record = record.clone();
        let transform = (self.display_origin, self.display_scale);
        let foreground = cx.theme().foreground;
        let selection = cx.theme().selection;
        Some(div().id("guest-composition-surface").absolute().left(px(left)).top(px(top))
            .w(px(width)).h(px(height)).bg(cx.theme().background).border_1().border_color(foreground)
            .overflow_hidden().child(canvas(move |bounds, _, _| bounds, move |_, bounds, window, cx| {
                let mut utf16_start = 0usize;
                let mut rows = Vec::new();
                for (index, text) in lines.iter().enumerate() {
                    let length = text.encode_utf16().count();
                    let byte_at = |unit: usize| {
                        let mut count = 0;
                        for (byte, ch) in text.char_indices() {
                            if count >= unit { return byte; }
                            count += ch.len_utf16();
                        }
                        text.len()
                    };
                    let selected_start = preedit.selection_utf16.start.saturating_sub(utf16_start).min(length);
                    let selected_end = preedit.selection_utf16.end.saturating_sub(utf16_start).min(length);
                    let line = window.text_system().shape_line(text.clone().into(), px(14.), &[TextRun {
                        len: text.len(), font: font(".SystemUIFont"), color: foreground,
                        background_color: None, underline: None, strikethrough: None,
                    }], None);
                    let caret = line.x_for_index(byte_at(selected_end));
                    let scroll = (f32::from(caret) - (width - 12.)).max(0.);
                    let origin = point(bounds.origin.x + px(4. - scroll), bounds.origin.y + px(4. + index as f32 * 20.));
                    if selected_start < selected_end {
                        let a = line.x_for_index(byte_at(selected_start));
                        let b = line.x_for_index(byte_at(selected_end));
                        window.paint_quad(fill(Bounds::new(point(origin.x + a, origin.y), size(b - a, px(20.))), selection));
                    }
                    let _ = line.paint(origin, px(20.), TextAlign::Left, None, window, cx);
                    window.paint_quad(fill(Bounds::new(point(origin.x, origin.y + px(19.)), size(line.width(), px(1.))), foreground));
                    if preedit.selection_utf16.end >= utf16_start && preedit.selection_utf16.end <= utf16_start + length {
                        window.paint_quad(fill(Bounds::new(point(origin.x + caret, origin.y), size(px(1.), px(20.))), foreground));
                    }
                    let mut positions = Vec::new();
                    let mut unit = 0;
                    for (byte, ch) in text.char_indices() {
                        positions.push((unit, f32::from(line.x_for_index(byte))));
                        unit += ch.len_utf16();
                    }
                    positions.push((length, f32::from(line.x_for_index(text.len()))));
                    rows.push(MarkedRow { start: utf16_start, positions,
                        origin: (f32::from(origin.x), f32::from(origin.y)) });
                    utf16_start += length + 1;
                }
                *cache.borrow_mut() = Some(PaintedComposition {
                    owner: cache_owner.clone(), preedit: preedit.clone(), record: cache_record.clone(), transform,
                    clip: (f32::from(bounds.origin.x), f32::from(bounds.origin.y),
                        f32::from(bounds.origin.x + bounds.size.width), f32::from(bounds.origin.y + bounds.size.height)), rows,
                });
            }).size_full()).into_any_element())
    }

    fn painted_composition(&self) -> Option<PaintedComposition> {
        let painted = self.composition_geometry.borrow().clone()?;
        (self.composition.owner() == Some(&painted.owner)
            && self.composition.preedit.as_ref() == Some(&painted.preedit)
            && painted.transform == (self.display_origin, self.display_scale)
            && self.text_edits.iter().any(|record| record == &painted.record)).then_some(painted)
    }

    fn marked_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let painted = self.painted_composition()?;
        let start = range.start.checked_sub(painted.owner.selection.start)?;
        let end = range.end.checked_sub(painted.owner.selection.start)?;
        if start > end || end > painted.preedit.text.encode_utf16().count() { return None; }
        for row in &painted.rows {
            let Some(local) = start.checked_sub(row.start) else { continue; };
            if local > row.positions.last()?.0 { continue; }
            let x = row.positions.iter().find(|(unit, _)| *unit == local)?.1;
            let last = row.positions.last()?.0;
            let local_end = end.saturating_sub(row.start).min(last);
            let right = row.positions.iter().find(|(unit, _)| *unit == local_end)?.1;
            let left = (row.origin.0 + x.min(right)).max(painted.clip.0);
            let right = (row.origin.0 + x.max(right) + if range.is_empty() { 1. } else { 0. }).min(painted.clip.2);
            let top = row.origin.1.max(painted.clip.1);
            let bottom = (row.origin.1 + 20.).min(painted.clip.3);
            if left < right && top < bottom { return Some(Bounds::new(point(px(left), px(top)), size(px(right - left), px(bottom - top)))); }
        }
        None
    }

    fn marked_index_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        let painted = self.painted_composition()?;
        let x = f32::from(point.x); let y = f32::from(point.y);
        if !x.is_finite() || !y.is_finite() || x < painted.clip.0 || x >= painted.clip.2
            || y < painted.clip.1 || y >= painted.clip.3 { return None; }
        let row = painted.rows.iter().find(|row| y >= row.origin.1 && y < row.origin.1 + 20.)?;
        let (unit, _) = row.positions.iter().min_by(|a, b|
            (row.origin.0 + a.1 - x).abs().total_cmp(&(row.origin.0 + b.1 - x).abs()))?;
        Some(painted.owner.selection.start + row.start + unit)
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
        if self.composition.preedit.is_some() { return self.marked_bounds(range); }
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
        if self.composition.preedit.is_some() { return self.marked_index_for_point(point); }
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
