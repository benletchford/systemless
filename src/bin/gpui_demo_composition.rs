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

pub(super) fn accessible_line_geometry(record: &systemless::runner::TextEditSnapshot,
    clip: super::super::frames::Rect, scale: f32) -> Vec<Option<super::super::a11y::TextRunGeometry>> {
    let Some(starts) = record.line_starts.as_ref() else { return Vec::new(); };
    let Some(dest) = record.global_dest_rect else { return Vec::new(); };
    if !scale.is_finite() || scale <= 0. { return Vec::new(); }
    let dx = i32::from(dest.1) - i32::from(record.dest_rect.1);
    let dy = i32::from(dest.0) - i32::from(record.dest_rect.0);
    starts.windows(2).enumerate().map(|(index, span)| {
        let geometry = text_line_geometry(record, index)?;
        let bytes = record.text.get(span[0]..span[1])?;
        let visible = bytes.iter().rposition(|byte| !matches!(byte, b' ' | b'\r' | b'\n')).map_or(0, |index| index + 1);
        let mut pens = vec![0i32];
        for (offset, _) in bytes.iter().enumerate() {
            let previous = *pens.last()?;
            let width = if record.clips_line_offsets_to_visible_text && offset >= visible { 0 } else {
                i32::from(text_range_width(record, span[0] + offset..span[0] + offset + 1)?)
            };
            pens.push(previous.checked_add(width)?);
        }
        let x = dx + i32::from(geometry.left); let y = dy + i32::from(geometry.top);
        let left = x.max(clip.left); let right = (x + pens.last()?.max(&1)).min(clip.right);
        let top = y.max(clip.top); let bottom = (y + i32::from(geometry.height)).min(clip.bottom);
        if left >= right || top >= bottom { return None; }
        Some(super::super::a11y::TextRunGeometry {
            bounds: ((left - clip.left) as f32 * scale, (top - clip.top) as f32 * scale,
                (right - clip.left) as f32 * scale, (bottom - clip.top) as f32 * scale),
            positions: pens.into_iter().map(|pen| (x + pen - left) as f32 * scale).collect(),
        })
    }).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkedLine { text: String, start: usize, end: usize }

fn marked_caret_end(lines: &[MarkedLine], index: usize) -> usize {
    // A CRLF has a valid UTF-16 insertion boundary between its two units.
    // It occupies the preceding line end without introducing a painted glyph.
    lines.get(index + 1).map_or(lines[index].end, |next| next.start - 1)
}

fn marked_lines(text: &str) -> Vec<MarkedLine> {
    let mut result = Vec::new();
    let mut chars = text.char_indices().peekable();
    let (mut byte_start, mut unit_start, mut units) = (0, 0, 0);
    while let Some((byte, ch)) = chars.next() {
        if matches!(ch, '\r' | '\n') {
            result.push(MarkedLine { text: text[byte_start..byte].to_owned(), start: unit_start, end: units });
            units += 1;
            byte_start = byte + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                let (byte, _) = chars.next().unwrap(); units += 1; byte_start = byte + 1;
            }
            unit_start = units;
        } else { units += ch.len_utf16(); }
    }
    result.push(MarkedLine { text: text[byte_start..].to_owned(), start: unit_start, end: units });
    result
}

#[derive(Clone)]
struct MarkedRow { start: usize, positions: Vec<(usize, f32)>, origin: (f32, f32) }

#[derive(Clone)]
enum PaintedSource {
    Document(systemless::runner::TextEditSnapshot),
    Dialog(systemless::runner::DialogSnapshot),
    DialogRecord(systemless::runner::DialogSnapshot, systemless::runner::TextEditSnapshot),
    StandardFile(systemless::runner::StandardFileSnapshot),
}

#[derive(Clone)]
pub(super) struct PaintedComposition {
    owner: super::super::input::TextInputOwner,
    preedit: super::super::input::Preedit,
    record: PaintedSource,
    transform: ((f32, f32), f32),
    stack_scroll: f32,
    clip: (f32, f32, f32, f32),
    rows: Vec<MarkedRow>,
}

impl Demo {
    pub(super) fn synchronize_composition(&mut self, window: &Window, _: &mut Context<Self>) {
        let eligible = self.focus.is_focused(window) && self.host_active != Some(false)
            && self.open_menus.is_empty() && !self.guest_menu_tracking && self.guest_popup.is_none();
        let mut records = self.text_edits.iter().filter(|record| record.active && record.drawing_intact);
        let record = records.next();
        let owner = if eligible && self.standard_file.is_some() {
            self.standard_file.as_ref().and_then(super::super::input::standard_file_text_owner).map(|owner|
                super::super::input::TextInputOwner { identity: owner.identity,
                    target: super::super::input::TextInputTarget::StandardFile { new_folder: owner.new_folder },
                    text: owner.text, selection: owner.selection })
        } else if eligible && self.dialogs.iter().any(|dialog| dialog.visible && dialog.active) {
            super::super::input::dialog_text_owner_with_records(&self.dialogs, &self.windows, &self.text_edits, &self.controls).map(|owner|
                super::super::input::TextInputOwner { identity: owner.identity,
                    target: super::super::input::TextInputTarget::Dialog { item: owner.item, content_revision: owner.content_revision },
                    text: owner.text, selection: owner.selection })
        } else if eligible && records.next().is_none() {
            record.filter(|record| record.global_view_rect.is_some()).map(|record|
                super::super::input::TextInputOwner { identity: (record.guest_id, record.generation),
                    target: super::super::input::TextInputTarget::Document { port: record.owner_port }, text: record.text.clone(), selection: record.selection.0..record.selection.1 })
        } else { None };
        self.composition.synchronize(owner);
    }

    fn file_geometry(&self) -> Option<((i16, i16, i16, i16), Vec<i32>, i16, i16)> {
        let owner = self.composition.owner()?;
        let panel = self.standard_file.as_ref()?;
        let actual = super::super::input::standard_file_text_owner(panel)?;
        let super::super::input::TextInputTarget::StandardFile { new_folder } = owner.target else { return None; };
        if actual.identity != owner.identity || actual.new_folder != new_folder || actual.text != owner.text { return None; }
        if new_folder {
            let folder = panel.new_folder.as_ref()?;
            return Some((folder.layout.name, folder.insertion_positions.iter().map(|x| i32::from(*x)).collect(),
                folder.layout.name.0.saturating_add(2), 16));
        }
        let bounds = panel.put_layout.as_ref()?.name;
        let layout = panel.name_text_layout.as_ref()?;
        let line = super::super::text::ClassicLine::plain(&owner.text, layout.font.0, layout.font.1);
        Some((bounds, line.positions.iter().map(|x| i32::from(bounds.1) + i32::from(layout.origin.0) + x).collect(),
            bounds.0.saturating_add(layout.selection_top), layout.selection_height))
    }

    fn file_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        if !self.display_scale.is_finite() || self.display_scale <= 0. || range.start > range.end { return None; }
        let (bounds, positions, top, height) = self.file_geometry()?;
        let left = (*positions.get(range.start)?).max(i32::from(bounds.1));
        let right = (*positions.get(range.end)? + i32::from(range.is_empty())).min(i32::from(bounds.3));
        let bottom = top.saturating_add(height).min(bounds.2);
        let top = top.max(bounds.0);
        if left >= right || top >= bottom { return None; }
        Some(Bounds::new(point(px(self.display_origin.0 + left as f32 * self.display_scale),
            px(self.display_origin.1 + f32::from(top) * self.display_scale)),
            size(px((right-left) as f32 * self.display_scale), px(f32::from(bottom-top) * self.display_scale))))
    }

    fn file_index_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        if !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let (bounds, positions, _, _) = self.file_geometry()?;
        let x = (f32::from(point.x)-self.display_origin.0)/self.display_scale;
        let y = (f32::from(point.y)-self.display_origin.1)/self.display_scale;
        if !x.is_finite() || !y.is_finite() || x < f32::from(bounds.1) || x >= f32::from(bounds.3)
            || y < f32::from(bounds.0) || y >= f32::from(bounds.2) { return None; }
        positions.iter().enumerate().filter(|(_, position)| **position >= i32::from(bounds.1) && **position < i32::from(bounds.3))
            .min_by(|a,b| (*a.1 as f32-x).abs().total_cmp(&(*b.1 as f32-x).abs())).map(|(index,_)| index)
    }

    fn dialog_field(&self, owner: &super::super::input::TextInputOwner)
        -> Option<(&systemless::runner::DialogSnapshot, &systemless::runner::DialogItemSnapshot)> {
        let super::super::input::TextInputTarget::Dialog { item, content_revision } = owner.target else { return None; };
        let dialog = self.dialogs.iter().find(|dialog| (dialog.guest_id, dialog.generation) == owner.identity
            && dialog.content_revision == content_revision && dialog.visible && dialog.active && dialog.edit_field == Some(item))?;
        let field = dialog.items.iter().find(|field| field.number == item && field.enabled && field.visible)?;
        if field.text != systemless::systems::macintosh::mac_roman::decode_mac_roman(&owner.text) { return None; }
        Some((dialog, field))
    }

    pub(super) fn accessible_record_owner(&self, record: &systemless::runner::TextEditSnapshot)
        -> Option<super::super::input::TextInputOwner> {
        let owner = self.composition.owner()?;
        if owner.text != record.text || owner.selection != (record.selection.0..record.selection.1) { return None; }
        let matches = match owner.target {
            super::super::input::TextInputTarget::Document { port } =>
                owner.identity == (record.guest_id, record.generation) && port == record.owner_port,
            super::super::input::TextInputTarget::Dialog { .. } => self.dialog_record(owner).is_some_and(|actual|
                (actual.guest_id, actual.generation) == (record.guest_id, record.generation)),
            _ => false,
        };
        matches.then(|| owner.clone())
    }

    fn dialog_record(&self, owner: &super::super::input::TextInputOwner)
        -> Option<&systemless::runner::TextEditSnapshot> {
        let (dialog, field) = self.dialog_field(owner)?;
        if field.edit_text_layout.is_some() { return None; }
        let viewport = super::super::frames::Rect { top: 0, left: 0,
            bottom: self.height as i32, right: self.width as i32 };
        let pieces = super::super::frames::text_edit_pieces(&self.text_edits, &self.dialogs,
            &self.controls, &self.windows, viewport);
        pieces.iter().map(|piece| &self.text_edits[piece.record]).find(|record|
            record.owner_port == dialog.guest_id && record.text == owner.text
                && record.global_view_rect == Some(field.bounds))
    }

    fn record_bounds(&self, record: &systemless::runner::TextEditSnapshot, range: Range<usize>) -> Option<Bounds<Pixels>> {
        if range.start > range.end || range.end > record.text.len()
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

    fn record_index_for_point(&self, record: &systemless::runner::TextEditSnapshot, point: Point<Pixels>) -> Option<usize> {
        if !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
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

    fn dialog_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let owner = self.composition.owner()?;
        if let Some(record) = self.dialog_record(owner) { return self.record_bounds(record, range); }
        let (_, field) = self.dialog_field(owner)?;
        if range.start > range.end || range.end > owner.text.len() || !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let layout = field.edit_text_layout.as_ref()?;
        if layout.wrap { return None; }
        let line = super::super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
        let left = i32::from(field.bounds.1) + 1 + *line.positions.get(range.start)?
            - i32::from(range.is_empty() && layout.text_edit_geometry && range.start > 0);
        let right = if range.is_empty() { left + 1 } else { i32::from(field.bounds.1) + 1 + *line.positions.get(range.end)? };
        let left = left.max(i32::from(field.bounds.1)); let right = right.min(i32::from(field.bounds.3));
        let top = field.bounds.0;
        let bottom = i32::from(top).saturating_add(i32::from(layout.line_height)).min(i32::from(field.bounds.2));
        if left >= right || i32::from(top) >= bottom { return None; }
        Some(Bounds::new(point(px(self.display_origin.0 + left as f32 * self.display_scale),
            px(self.display_origin.1 + f32::from(top) * self.display_scale)),
            size(px((right - left) as f32 * self.display_scale), px((bottom - i32::from(top)) as f32 * self.display_scale))))
    }

    fn dialog_index_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        let owner = self.composition.owner()?;
        if let Some(record) = self.dialog_record(owner) { return self.record_index_for_point(record, point); }
        let (_, field) = self.dialog_field(owner)?;
        if !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let x = (f32::from(point.x) - self.display_origin.0) / self.display_scale;
        let y = (f32::from(point.y) - self.display_origin.1) / self.display_scale;
        if !x.is_finite() || !y.is_finite() || x < f32::from(field.bounds.1) || x >= f32::from(field.bounds.3)
            || y < f32::from(field.bounds.0) || y >= f32::from(field.bounds.2) { return None; }
        let layout = field.edit_text_layout.as_ref()?;
        if layout.wrap { return None; }
        let line = super::super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
        line.positions.iter().enumerate().min_by(|a, b|
            (f32::from(field.bounds.1) + 1. + *a.1 as f32 - x).abs().total_cmp(
                &(f32::from(field.bounds.1) + 1. + *b.1 as f32 - x).abs())).map(|(offset, _)| offset)
    }

    /// Unicode staging belongs to the host text service. It uses host typography
    /// in a separate surface, never as a replacement for committed guest glyphs.
    pub(super) fn composition_surface(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let owner = self.composition.owner()?;
        let preedit = self.composition.preedit.clone()?;
        let spans = self.composition.retained_spans();
        let menu_height = if self.menu_presented { f32::from(self.menu_height.max(1)) * self.display_scale } else { 0. };
        let scene_top = self.display_origin.1 + menu_height;
        let scene_height = (self.height as f32 * self.display_scale - menu_height).max(0.);
        let height = |stage: &super::super::input::Preedit|
            (marked_lines(&stage.text).len().max(1) as f32 * 20. + 8.).min(168.).min(scene_height);
        let total = height(&preedit) + spans.iter().map(|(_, stage)| height(stage) + 4.).sum::<f32>();
        if !total.is_finite() || scene_height <= 0. { return None; }
        let maximum_scroll = (total - scene_height).max(0.);
        let signature = format!("{:?}:{:?}", self.composition.virtual_document()?.text()?, preedit.selection_utf16);
        let mut scrolling = self.composition_stack_scroll.borrow_mut();
        if scrolling.0 != signature { *scrolling = (signature, 0.); }
        scrolling.1 = scrolling.1.clamp(0., maximum_scroll);
        let scroll = scrolling.1;
        drop(scrolling);
        let (_, y, line_height, _, _) = self.composition_anchor(owner)?;
        let top = (self.display_origin.1 + (y + i32::from(line_height)) as f32 * self.display_scale)
            .min(scene_top + (scene_height - total).max(0.)).max(scene_top) - scroll;
        let active = self.composition_span_surface(owner, preedit.clone(),
            self.composition_geometry.clone(), true, 0, Some(top), cx)?;
        let mut caches = self.retained_composition_geometry.borrow_mut();
        caches.resize_with(spans.len(), Default::default);
        let mut surfaces = Vec::new();
        let mut offset = height(&preedit) + 4.;
        for (index, (owner, stage)) in spans.iter().enumerate() {
            surfaces.push(self.composition_span_surface(owner, stage.clone(), caches[index].clone(),
                false, index + 1, Some(top + offset), cx)?);
            offset += height(stage) + 4.;
        }
        surfaces.push(active);
        Some(div().absolute().size_full().children(surfaces)
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                let point = (f32::from(event.position.x), f32::from(event.position.y));
                // Consume successive wheel events over the last displayed
                // panels even while text-service geometry awaits repaint.
                let covers = |painted: &PaintedComposition| {
                    let (left, top, right, bottom) = painted.clip;
                    painted.transform == (this.display_origin, this.display_scale)
                        && point.0 >= left && point.0 < right && point.1 >= top && point.1 < bottom
                };
                let active_hit = this.composition_geometry.borrow().as_ref().is_some_and(|painted|
                    this.composition.owner() == Some(&painted.owner)
                        && this.composition.preedit.as_ref() == Some(&painted.preedit) && covers(painted));
                let retained_hit = this.retained_composition_geometry.borrow().iter().enumerate().any(|(index, cache)|
                    cache.borrow().as_ref().is_some_and(|painted|
                        this.composition.retained_spans().get(index).is_some_and(|(owner, preedit)|
                            owner == &painted.owner && preedit == &painted.preedit) && covers(painted)));
                let over_stage = active_hit || retained_hit;
                if !over_stage { return; }
                cx.stop_propagation();
                let delta = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y * 20.,
                    ScrollDelta::Pixels(delta) => f32::from(delta.y),
                };
                if delta.is_finite() {
                    let mut scrolling = this.composition_stack_scroll.borrow_mut();
                    scrolling.1 = (scrolling.1 - delta).clamp(0., maximum_scroll);
                    drop(scrolling);
                    cx.notify();
                }
            })).into_any_element())
    }

    fn composition_anchor(&self, owner: &super::super::input::TextInputOwner)
        -> Option<(i32, i32, i16, (i16, i16, i16, i16), PaintedSource)> {
        let (x, y, line_height, view, cache_record) = if matches!(owner.target, super::super::input::TextInputTarget::StandardFile { .. }) {
            let (bounds, positions, top, height) = self.file_geometry()?;
            (*positions.get(owner.selection.start)?, i32::from(top), height, bounds,
                PaintedSource::StandardFile(self.standard_file.as_ref()?.clone()))
        } else if matches!(owner.target, super::super::input::TextInputTarget::Dialog { .. }) {
            let (dialog, field) = self.dialog_field(owner)?;
            if let Some(record) = self.dialog_record(owner) {
                let starts = record.line_starts.as_ref()?;
                let index = starts.partition_point(|start| *start <= owner.selection.start).saturating_sub(1)
                    .min(starts.len().checked_sub(2)?);
                let geometry = text_line_geometry(record, index)?;
                let dest = record.global_dest_rect?;
                let view = record.global_view_rect?;
                let x = i32::from(dest.1) - i32::from(record.dest_rect.1) + i32::from(geometry.left)
                    + i32::from(text_range_width(record, starts[index]..owner.selection.start)?);
                let y = i32::from(dest.0) - i32::from(record.dest_rect.0) + i32::from(geometry.top);
                (x, y, geometry.height, view, PaintedSource::DialogRecord(dialog.clone(), record.clone()))
            } else {
            let layout = field.edit_text_layout.as_ref()?;
            if layout.wrap { return None; }
            let line = super::super::text::ClassicLine::unicode(&field.text, layout.font.0, layout.font.1);
            let pen = *line.positions.get(owner.selection.start)?;
            (i32::from(field.bounds.1) + 1 + pen, i32::from(field.bounds.0), layout.line_height,
                field.bounds, PaintedSource::Dialog(dialog.clone()))
            }
        } else {
        let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
        if record.text != owner.text { return None; }
        let starts = record.line_starts.as_ref()?;
        let index = starts.partition_point(|start| *start <= owner.selection.start).saturating_sub(1)
            .min(starts.len().checked_sub(2)?);
        let geometry = text_line_geometry(record, index)?;
        let dest = record.global_dest_rect?;
        let view = record.global_view_rect?;
        let x = i32::from(dest.1) - i32::from(record.dest_rect.1) + i32::from(geometry.left)
            + i32::from(text_range_width(record, starts[index]..owner.selection.start)?);
        let y = i32::from(dest.0) - i32::from(record.dest_rect.0) + i32::from(geometry.top);
        (x, y, geometry.height, view, PaintedSource::Document(record.clone()))
        };
        Some((x, y, line_height, view, cache_record))
    }

    fn composition_span_surface(&self, owner: &super::super::input::TextInputOwner,
        preedit: super::super::input::Preedit,
        cache: std::rc::Rc<std::cell::RefCell<Option<PaintedComposition>>>,
        active: bool, slot: usize, top_override: Option<f32>, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        let (x, y, line_height, view, cache_record) = self.composition_anchor(owner)?;
        if x < i32::from(view.1) || x >= i32::from(view.3) || y < i32::from(view.0) || y >= i32::from(view.2) { return None; }
        let width = 240f32.min(self.width as f32 * self.display_scale).max(1.);
        let lines = marked_lines(&preedit.text);
        let menu_height = if self.menu_presented { f32::from(self.menu_height.max(1)) * self.display_scale } else { 0. };
        let height = (lines.len().max(1) as f32 * 20. + 8.).min(168.)
            .min((self.height as f32 * self.display_scale - menu_height).max(0.));
        let left = (self.display_origin.0 + x as f32 * self.display_scale)
            .min(self.display_origin.0 + self.width as f32 * self.display_scale - width);
        let top = top_override.unwrap_or_else(||
            (self.display_origin.1 + (y + i32::from(line_height)) as f32 * self.display_scale)
                .min(self.display_origin.1 + self.height as f32 * self.display_scale - height));
        let cache_owner = owner.clone();
        let scene_clip = (self.display_origin.0, self.display_origin.1 + menu_height,
            self.display_origin.0 + self.width as f32 * self.display_scale,
            self.display_origin.1 + self.height as f32 * self.display_scale);
        let transform = (self.display_origin, self.display_scale);
        let stack_scroll = self.composition_stack_scroll.borrow().1;
        let foreground = cx.theme().foreground;
        let selection = cx.theme().selection;
        Some(div().id("guest-composition-surface").absolute().left(px(left)).top(px(top))
            .w(px(width)).h(px(height)).bg(cx.theme().background).border_1().border_color(foreground)
            .overflow_hidden().child(canvas(move |bounds, _, _| bounds, move |_, bounds, window, cx| {
                let caret_line = lines.iter().enumerate().position(|(index, line)| preedit.selection_utf16.end >= line.start
                    && preedit.selection_utf16.end <= marked_caret_end(&lines, index)).unwrap_or(lines.len() - 1);
                let vertical_scroll = (caret_line as f32 * 20. - (height - 28.).max(0.)).max(0.);
                let mut rows = Vec::new();
                for (index, marked) in lines.iter().enumerate() {
                    let row_top = 4. + index as f32 * 20. - vertical_scroll;
                    if row_top + 20. <= 0. || row_top >= height { continue; }
                    let text = &marked.text;
                    let utf16_start = marked.start;
                    let length = marked.end - marked.start;
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
                    // Only the caret row needs horizontal reveal. Earlier rows
                    // clamp selected_end to their length, which is not a caret.
                    let scroll = if index == caret_line {
                        (f32::from(caret) - (width - 12.)).max(0.)
                    } else { 0. };
                    let origin = point(bounds.origin.x + px(4. - scroll), bounds.origin.y + px(row_top));
                    if active && selected_start < selected_end {
                        let a = line.x_for_index(byte_at(selected_start));
                        let b = line.x_for_index(byte_at(selected_end));
                        window.paint_quad(fill(Bounds::new(point(origin.x + a, origin.y), size(b - a, px(20.))), selection));
                    }
                    let _ = line.paint(origin, px(20.), TextAlign::Left, None, window, cx);
                    window.paint_quad(fill(Bounds::new(point(origin.x, origin.y + px(19.)), size(line.width(), px(1.))), foreground));
                    if active && preedit.selection_utf16.end >= utf16_start && preedit.selection_utf16.end <= marked_caret_end(&lines, index) {
                        window.paint_quad(fill(Bounds::new(point(origin.x + caret, origin.y), size(px(1.), px(20.))), foreground));
                    }
                    let mut positions = Vec::new();
                    let mut unit = 0;
                    for (byte, ch) in text.char_indices() {
                        positions.push((unit, f32::from(line.x_for_index(byte))));
                        unit += ch.len_utf16();
                    }
                    positions.push((length, f32::from(line.x_for_index(text.len()))));
                    let caret_end = marked_caret_end(&lines, index) - utf16_start;
                    if caret_end > length {
                        positions.push((caret_end, f32::from(line.x_for_index(text.len()))));
                    }
                    rows.push(MarkedRow { start: utf16_start, positions,
                        origin: (f32::from(origin.x), f32::from(origin.y)) });
                }
                *cache.borrow_mut() = Some(PaintedComposition {
                    owner: cache_owner.clone(), preedit: preedit.clone(), record: cache_record.clone(), transform, stack_scroll,
                    clip: (f32::from(bounds.origin.x).max(scene_clip.0), f32::from(bounds.origin.y).max(scene_clip.1),
                        f32::from(bounds.origin.x + bounds.size.width).min(scene_clip.2),
                        f32::from(bounds.origin.y + bounds.size.height).min(scene_clip.3)), rows,
                });
            }).size_full()).into_any_element())
    }

    fn painted_composition(&self) -> Option<PaintedComposition> {
        let painted = self.composition_geometry.borrow().clone()?;
        (self.composition.owner() == Some(&painted.owner)
            && self.composition.preedit.as_ref() == Some(&painted.preedit)
            && painted.transform == (self.display_origin, self.display_scale)
            && painted.stack_scroll == self.composition_stack_scroll.borrow().1
            && match &painted.record {
                PaintedSource::Document(expected) => self.text_edits.iter().any(|record| record == expected),
                PaintedSource::Dialog(expected) => self.dialogs.iter().any(|dialog| dialog == expected),
                PaintedSource::DialogRecord(expected, record) => self.dialogs.iter().any(|dialog| dialog == expected)
                    && self.text_edits.iter().any(|actual| actual == record),
                PaintedSource::StandardFile(expected) => self.standard_file.as_ref() == Some(expected),
            }).then_some(painted)
    }

    fn painted_compositions(&self) -> Option<Vec<(usize, PaintedComposition)>> {
        let document = self.composition.virtual_document()?;
        let active = self.painted_composition()?;
        let mut result = vec![(document.marked_range(&active.owner.selection)?.start, active)];
        let caches = self.retained_composition_geometry.borrow();
        let spans = self.composition.retained_spans();
        if caches.len() != spans.len() { return None; }
        for (index, (owner, preedit)) in spans.iter().enumerate().rev() {
            let painted = caches[index].borrow().clone()?;
            if &painted.owner != owner || &painted.preedit != preedit
                || painted.transform != (self.display_origin, self.display_scale)
                || painted.stack_scroll != self.composition_stack_scroll.borrow().1 { return None; }
            let current = match &painted.record {
                PaintedSource::Document(expected) => self.text_edits.iter().any(|record| record == expected),
                PaintedSource::Dialog(expected) => self.dialogs.iter().any(|dialog| dialog == expected),
                PaintedSource::DialogRecord(expected, record) => self.dialogs.iter().any(|dialog| dialog == expected)
                    && self.text_edits.iter().any(|actual| actual == record),
                PaintedSource::StandardFile(expected) => self.standard_file.as_ref() == Some(expected),
            };
            if !current { return None; }
            result.push((document.marked_range(&owner.selection)?.start, painted));
        }
        Some(result)
    }

    fn marked_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        self.painted_compositions()?.into_iter().find_map(|(start, painted)|
            Self::marked_span_bounds(&painted, start, range.clone()))
    }

    fn marked_span_bounds(painted: &PaintedComposition, virtual_start: usize,
        range: Range<usize>) -> Option<Bounds<Pixels>> {
        let start = range.start.checked_sub(virtual_start)?;
        let end = range.end.checked_sub(virtual_start)?;
        if start > end || end > painted.preedit.text.encode_utf16().count() { return None; }
        let boundaries: std::collections::BTreeSet<_> = std::iter::once(0).chain(painted.preedit.text.chars().scan(0, |units, ch| {
            *units += ch.len_utf16(); Some(*units)
        })).collect();
        if !boundaries.contains(&start) || !boundaries.contains(&end) { return None; }
        for row in &painted.rows {
            // A range may begin in a row that has scrolled out of the stage.
            // Intersect each actually painted row and return its first visible
            // portion, while an offscreen insertion point still has no bounds.
            let last = row.positions.last()?.0;
            let row_end = row.start.checked_add(last)?;
            let visible_start = start.max(row.start);
            let visible_end = end.min(row_end);
            if visible_start > visible_end || (!range.is_empty() && visible_start == visible_end) { continue; }
            let local = visible_start - row.start;
            let x = row.positions.iter().find(|(unit, _)| *unit == local)?.1;
            let local_end = visible_end - row.start;
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
        let x = f32::from(point.x); let y = f32::from(point.y);
        if !x.is_finite() || !y.is_finite() { return None; }
        for (start, painted) in self.painted_compositions()? {
            if x < painted.clip.0 || x >= painted.clip.2 || y < painted.clip.1 || y >= painted.clip.3 { continue; }
            let Some(row) = painted.rows.iter().find(|row| y >= row.origin.1 && y < row.origin.1 + 20.) else { continue; };
            let (unit, _) = row.positions.iter().min_by(|a, b|
                (row.origin.0 + a.1 - x).abs().total_cmp(&(row.origin.0 + b.1 - x).abs()))?;
            return Some(start + row.start + unit);
        }
        None
    }

    fn composition_text(&self) -> Option<String> {
        self.composition.virtual_document()?.text()
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
        let range = if let Some(preedit) = self.composition.preedit.as_ref() {
            let start = self.composition.active_virtual_range()?.start;
            start + preedit.selection_utf16.start..start + preedit.selection_utf16.end
        } else { owner.selection.clone() };
        Some(UTF16Selection { range, reversed: false })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.composition.active_virtual_range()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.composition.preedit.as_ref().map(|preedit| preedit.text.clone()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }
    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str,
        _: &mut Window, cx: &mut Context<Self>) {
        if !self.composition.retained_spans().is_empty() {
            // The marked transaction remains pinned until every payload passes
            // conversion. Each request uses the existing guest replacement path.
            if let Some(requests) = self.composition.commit_marked_replacement(range.as_ref(), text) {
                for request in requests {
                    let _ = self.commands.send(Command::ReplaceText(request.expected, request.range,
                        request.bytes, request.caret));
                }
                cx.notify();
            }
            return;
        }
        if self.composition.preedit.is_some() && self.composition.owner().is_some() {
            if let Some(range) = range.clone() {
                let marked_base = self.composition.marked_base().cloned();
                if let Some((first, (expected, request, bytes))) = self.composition.commit_disjoint_range(range.clone(), text) {
                    let command = if let Some(base) = marked_base {
                        Command::ReplaceText(base, first.0.selection, first.1, None)
                    } else { Command::CommitText(first.0, first.1) };
                    let _ = self.commands.send(command);
                    let _ = self.commands.send(Command::ReplaceText(expected, request.selection, bytes, None));
                    cx.notify();
                    return;
                }
                if let Some((expected, request, bytes, caret)) = self.composition.commit_overlapping_range(range, text) {
                    if request.selection == expected.selection {
                        let command = if caret == request.selection.start + bytes.len() {
                            Command::CommitText(expected, bytes)
                        } else { Command::CommitTextCaret(expected, bytes, caret) };
                        let _ = self.commands.send(command);
                    } else {
                        let _ = self.commands.send(Command::ReplaceText(expected, request.selection, bytes, Some(caret)));
                    }
                    cx.notify();
                }
                return;
            }
        }
        if self.composition.preedit.is_none() {
            if let Some(range) = range.as_ref().filter(|range|
                self.composition.owner().is_some_and(|owner| **range != owner.selection)) {
                if let Some((expected, request, bytes)) = self.composition.commit_range(range.clone(), text) {
                    let _ = self.commands.send(Command::ReplaceText(expected, request.selection, bytes, None));
                    cx.notify();
                }
                return;
            }
        }
        let marked_base = self.composition.marked_base().cloned();
        if let Some((owner, bytes, caret)) = self.composition.commit_replacement(range.as_ref(), text) {
            let command = if let Some(base) = marked_base {
                Command::ReplaceText(base, owner.selection, bytes, Some(caret))
            } else if caret == owner.selection.start + bytes.len() {
                Command::CommitText(owner, bytes)
            } else { Command::CommitTextCaret(owner, bytes, caret) };
            let _ = self.commands.send(command);
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str,
        selected: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        let end = text.encode_utf16().count();
        let selected = selected.unwrap_or(end..end);
        // Selection is relative to the inserted text, not the retained prefix.
        if selected.start > selected.end || selected.end > end { return; }
        if self.composition.mark_range(range.as_ref(), text, selected) { cx.notify(); }
    }
    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>,
        window: &mut Window, cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let range = if self.composition.preedit.is_some() {
            self.painted_compositions()?;
            let pieces = self.composition.geometry_ranges(range.clone())?;
            if pieces.len() > 1 {
                for piece in pieces {
                    if let Some(bounds) = self.bounds_for_range(piece, Bounds::default(), window, cx) {
                        return Some(bounds);
                    }
                }
                return None;
            }
            if let Some(guest) = self.composition.surrounding_guest_range(range.clone()) { guest }
            else { return self.marked_bounds(range); }
        } else { range };
        if matches!(self.composition.owner()?.target, super::super::input::TextInputTarget::StandardFile { .. }) { return self.file_bounds(range); }
        if matches!(self.composition.owner()?.target, super::super::input::TextInputTarget::Dialog { .. }) { return self.dialog_bounds(range); }
        let owner = self.composition.owner()?;
        let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
        // Geometry is meaningful only for the currently painted guest text.
        // Pending commits and Unicode preedit need their own painted layout.
        if record.text != owner.text
            || range.start > range.end || range.end > owner.text.len()
            || !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
        self.record_bounds(record, range)
    }
    fn character_index_for_point(&mut self, point: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let staged = self.composition.preedit.is_some();
        if staged {
            if let Some(index) = self.marked_index_for_point(point) { return Some(index); }
            // The stage masks the underlying guest pixels even at its border.
            let painted = self.painted_compositions()?;
            let (x, y) = (f32::from(point.x), f32::from(point.y));
            if painted.iter().any(|(_, span)| x >= span.clip.0 && x < span.clip.2
                && y >= span.clip.1 && y < span.clip.3) { return None; }
        }
        let owner = self.composition.owner()?;
        let index = match owner.target {
            super::super::input::TextInputTarget::StandardFile { .. } => self.file_index_for_point(point)?,
            super::super::input::TextInputTarget::Dialog { .. } => self.dialog_index_for_point(point)?,
            _ => {
                let record = self.text_edits.iter().find(|record| (record.guest_id, record.generation) == owner.identity)?;
                if record.text != owner.text || !self.display_scale.is_finite() || self.display_scale <= 0. { return None; }
                self.record_index_for_point(record, point)?
            }
        };
        if staged { self.composition.surrounding_virtual_index(index) } else { Some(index) }
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.composition_text().map(|text| text.encode_utf16().count())
    }
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && self.composition.owner().is_some()
    }
}

#[cfg(test)]
mod marked_line_tests {
    #[test]
    fn crlf_internal_caret_boundary_stays_at_preceding_line_end() {
        let lines = super::marked_lines("日😀\r\nx\ry\nz\n");
        let ends: Vec<_> = (0..lines.len()).map(|index| super::marked_caret_end(&lines, index)).collect();
        assert_eq!(ends, vec![4, 6, 8, 10, 11]);
        for caret in 0..=11 {
            let owners: Vec<_> = lines.iter().enumerate().filter(|(index, line)|
                caret >= line.start && caret <= ends[*index]).map(|(index, _)| index).collect();
            assert_eq!(owners.len(), 1, "each insertion offset has exactly one visual row: {caret}");
        }
        assert_eq!(ends[0], 4, "CRLF interior must not scroll to the last row");
    }
    #[test]
    fn preserves_original_utf16_offsets_across_all_line_endings() {
        let lines = super::marked_lines("日😀\r\nx\ry\nz\n");
        assert_eq!(lines.iter().map(|line| (line.text.as_str(), line.start, line.end)).collect::<Vec<_>>(),
            vec![("日😀", 0, 3), ("x", 5, 6), ("y", 7, 8), ("z", 9, 10), ("", 11, 11)]);
    }
}
