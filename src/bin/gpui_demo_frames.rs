//! Rectangular guest-frame overlays. Content pixels and input remain guest-owned.

use systemless::runner::{ControlSnapshot, DialogItemKind, DialogSnapshot, ListManagerSnapshot, TextEditSnapshot, WindowFrameSnapshot};
use systemless::menu_model::GuestMenuSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
}

impl From<(i16, i16, i16, i16)> for Rect {
    fn from(r: (i16, i16, i16, i16)) -> Self {
        Self {
            top: r.0.into(),
            left: r.1.into(),
            bottom: r.2.into(),
            right: r.3.into(),
        }
    }
}

/// Prepare only compositor-owned pixels; coordinates outside these clipped
/// rectangles retain their original bytes, including custom guest content.
pub fn fill_texture_clips(pixels: &mut [u8], width: u32, height: u32, clips: &[Rect], color: [u8; 4]) {
    let viewport = Rect { top: 0, left: 0, bottom: height as i32, right: width as i32 };
    assert_eq!(pixels.len(), width as usize * height as usize * 4);
    for clip in clips {
        let Some(rect) = clip.intersection(viewport) else { continue; };
        for y in rect.top..rect.bottom {
            let start = (y as usize * width as usize + rect.left as usize) * 4;
            let end = (y as usize * width as usize + rect.right as usize) * 4;
            for pixel in pixels[start..end].chunks_exact_mut(4) { pixel.copy_from_slice(&color); }
        }
    }
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    fn valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }
    pub fn intersection(self, other: Self) -> Option<Self> {
        let r = Self {
            top: self.top.max(other.top),
            left: self.left.max(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.min(other.right),
        };
        r.valid().then_some(r)
    }
    fn subtract(self, cover: Self) -> Vec<Self> {
        let Some(cut) = self.intersection(cover) else {
            return vec![self];
        };
        [
            Self {
                bottom: cut.top,
                ..self
            },
            Self {
                top: cut.bottom,
                ..self
            },
            Self {
                top: cut.top,
                bottom: cut.bottom,
                right: cut.left,
                ..self
            },
            Self {
                top: cut.top,
                bottom: cut.bottom,
                left: cut.right,
                ..self
            },
        ]
        .into_iter()
        .filter(|r| r.valid())
        .collect()
    }
}

pub struct FramePiece {
    pub window: usize,
    /// Full strip geometry, retained so clipping never recentres a title.
    pub source: Rect,
    pub clip: Rect,
    pub title: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GutterKind {
    Vertical,
    Horizontal,
    GrowBox,
}

pub struct GutterPiece {
    pub window: usize,
    pub source: Rect,
    pub clip: Rect,
    pub kind: GutterKind,
}

pub struct ControlPiece {
    pub control: usize,
    pub source: Rect,
    pub clip: Rect,
}

/// The popup's selected item remains the ControlRecord value; the label is
/// read from the live associated MENU, including disabled selected items.
/// MTE (1992), pp. 5-25--5-27 and 5-77.
pub fn popup_control_label<'a>(
    control: &ControlSnapshot,
    menus: &'a GuestMenuSnapshot,
) -> Option<&'a str> {
    if !(1008..=1023).contains(&control.proc_id) {
        return None;
    }
    let menu_id = control.popup_menu_id?;
    let menu = menus.menus.iter().find(|menu| menu.id == menu_id)?;
    let item = menu.items.iter().find(|item| item.number == control.value)?;
    (!item.separator).then_some(item.text.as_str())
}

fn standard_control(control: &ControlSnapshot, menus: &GuestMenuSnapshot) -> bool {
    // Appearance overrides need guest font/style shaping before Kit can own
    // these pixels. Keep their guest CDEF painting, including overlap masks.
    control.font_style.is_none() && (matches!(control.proc_id, 0 | 1 | 2 | 16)
        || popup_control_label(control, menus).is_some())
}

pub struct ListPiece {
    pub list: usize,
    pub source: Rect,
    pub clip: Rect,
}

pub struct TextEditPiece {
    pub record: usize,
    pub source: Rect,
    pub clip: Rect,
}

pub struct DialogItemPiece {
    pub dialog: usize,
    pub item: usize,
    pub source: Rect,
    pub clip: Rect,
}

fn clip_to_guest_visible_content(
    clips: Vec<Rect>,
    frame: &WindowFrameSnapshot,
) -> Vec<Rect> {
    let Some(visible) = frame.visible_content_rects.as_ref() else {
        return Vec::new();
    };
    clips
        .into_iter()
        .flat_map(|clip| {
            visible
                .iter()
                .filter_map(move |rect| clip.intersection(Rect::from(*rect)))
        })
        .collect()
}

/// Clip standard DITL presentation to its owning dialog and all windows in
/// front of it. Standard modeless dialogs use noGrowDocProc (4), while modal
/// dialogs commonly use dBoxProc (1); custom WDEFs keep their guest pixels.
/// Macintosh Toolbox Essentials (1992), pp. 4-10, 4-19, 4-40, 6-13--6-15.
pub fn dialog_item_pieces(
    dialogs: &[DialogSnapshot],
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Vec<DialogItemPiece> {
    let mut pieces = Vec::new();
    let mut covers = Vec::new();
    for frame in windows {
        if !frame.window.visible {
            continue;
        }
        let Some(structure) = frame.window.structure_bounds.map(Rect::from) else {
            continue;
        };
        if matches!(
            frame.presentation_definition_id(),
            Some(0 | 1 | 2 | 3 | 4 | 5 | 8 | 12 | 16)
        ) {
            if let Some((dialog_index, dialog)) = dialogs.iter().enumerate().find(|(_, dialog)| {
                dialog.visible
                    && dialog.guest_id == frame.guest_id
                    && dialog.generation == frame.generation
                    && !dialog.items.is_empty()
                    && dialog.items.iter().all(|item| match item.kind {
                        DialogItemKind::Button
                        | DialogItemKind::StaticText
                        | DialogItemKind::EditText => true,
                        DialogItemKind::Checkbox | DialogItemKind::RadioButton => {
                            item.value.is_some()
                        }
                        _ => false,
                    })
            }) {
                for (item_index, item) in dialog.items.iter().enumerate() {
                    if (item.kind == DialogItemKind::StaticText && item.static_text_layout.is_none())
                        || !item.visible
                        || (item.kind == DialogItemKind::EditText
                            && (item.edit_text_layout.is_none() || item.text.contains('\r') || item.bounds.2 - item.bounds.0 > 24))
                    {
                        continue;
                    }
                    let item_rect = Rect::from(item.bounds);
                    let source = if (item.kind == DialogItemKind::Button
                        && dialog.default_item == Some(item.number))
                        || item.kind == DialogItemKind::EditText
                    {
                        Rect {
                            top: item_rect.top - 4,
                            left: item_rect.left - 4,
                            bottom: item_rect.bottom + 4,
                            right: item_rect.right + 4,
                        }
                    } else {
                        item_rect
                    };
                    let clips: Vec<_> = source
                        .intersection(Rect::from(dialog.bounds))
                        .and_then(|rect| rect.intersection(viewport))
                        .into_iter()
                        .collect();
                    let mut clips = clip_to_guest_visible_content(clips, frame);
                    for cover in &covers {
                        clips = clips.into_iter().flat_map(|clip| clip.subtract(*cover)).collect();
                    }
                    pieces.extend(clips.into_iter().map(|clip| DialogItemPiece {
                        dialog: dialog_index,
                        item: item_index,
                        source,
                        clip,
                    }));
                }
            }
        }
        covers.push(structure);
    }
    pieces
}

/// Present only ordinary unstyled TextEdit records in standard document
/// windows. The TERec owns line layout; other windows and controls retain
/// their guest pixels. Inside Macintosh: Text (1993), pp. 2-64--2-69.
pub fn text_edit_pieces(
    records: &[TextEditSnapshot],
    dialogs: &[DialogSnapshot],
    controls: &[ControlSnapshot],
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Vec<TextEditPiece> {
    text_edit_pieces_for_kind(records, dialogs, controls, windows, viewport, false)
}

/// Visibility candidates only. A styled recipe must additionally pass native
/// whole-field paint qualification before any candidate becomes GPUI-owned.
pub fn styled_text_edit_candidates(
    records: &[TextEditSnapshot], dialogs: &[DialogSnapshot], controls: &[ControlSnapshot],
    windows: &[WindowFrameSnapshot], viewport: Rect,
) -> Vec<TextEditPiece> {
    text_edit_pieces_for_kind(records, dialogs, controls, windows, viewport, true)
}

fn text_edit_pieces_for_kind(
    records: &[TextEditSnapshot], dialogs: &[DialogSnapshot], controls: &[ControlSnapshot],
    windows: &[WindowFrameSnapshot], viewport: Rect, styled: bool,
) -> Vec<TextEditPiece> {
    let mut pieces = Vec::new();
    let mut covers = Vec::new();
    for frame in windows {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        if !matches!(frame.presentation_definition_id(), Some(0 | 4 | 8 | 12 | 16))
            || dialogs.iter().any(|dialog| dialog.guest_id == frame.guest_id)
        {
            covers.push(structure);
            continue;
        }
        for (index, record) in records.iter().enumerate() {
            if !record.drawing_intact || record.owner_port != frame.guest_id
                || record.styled != styled
                || (!styled && (record.face != 0 || record.justification != 0 || record.line_height <= 0
                    // Plain scaled substitutes retain their existing qualification guard.
                    || systemless::quickdraw::fonts::get_font_face_or_default(record.font, record.size).size
                        != if record.size == 0 { 12 } else { record.size }
                    || record.display_lines().is_none())) {
                continue;
            }
            let Some(source) = record.global_view_rect.map(Rect::from) else {
                continue;
            };
            if controls.iter().any(|control| {
                control.owner_id == frame.guest_id
                    && control.visible
                    && Rect::from(control.bounds).intersection(source).is_some()
            }) {
                continue;
            }
            let clips: Vec<_> = source
                .intersection(Rect::from(window.bounds))
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
            let clips = clips.into_iter().flat_map(|clip| record.painted_regions.iter()
                .filter_map(move |painted| clip.intersection(Rect::from(*painted)))).collect();
            let mut clips = clip_to_guest_visible_content(clips, frame);
            for cover in &covers {
                clips = clips.into_iter().flat_map(|clip| clip.subtract(*cover)).collect();
            }
            pieces.extend(clips.into_iter().map(|clip| TextEditPiece { record: index, source, clip }));
        }
        covers.push(structure);
    }
    pieces
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrollbarGeometry {
    pub vertical: bool,
    pub arrow_extent: i32,
    pub track_start: i32,
    pub track_extent: i32,
    pub thumb_start: i32,
    pub thumb_extent: i32,
}

/// Match the standard CDEF's 16-pixel arrow and thumb hit regions while
/// locating the thumb from the live ControlRecord value/range.
/// Macintosh Toolbox Essentials (1992), pp. 5-7--5-10, 5-58--5-61.
pub fn scrollbar_geometry(control: &ControlSnapshot) -> ScrollbarGeometry {
    let rect = Rect::from(control.bounds);
    let vertical = rect.height() > rect.width();
    let extent = if vertical { rect.height() } else { rect.width() }.max(0);
    let arrow_extent = 16.min(extent / 2);
    let track_start = arrow_extent;
    let track_extent = (extent - 2 * arrow_extent).max(0);
    let thumb_extent = 16.min(track_extent);
    let span = i32::from(control.maximum) - i32::from(control.minimum);
    let value = (i32::from(control.value) - i32::from(control.minimum)).clamp(0, span.max(0));
    let travel = track_extent - thumb_extent;
    let thumb_start = track_start + if span > 0 {
        (i64::from(value) * i64::from(travel) / i64::from(span)) as i32
    } else { 0 };
    ScrollbarGeometry {
        vertical,
        arrow_extent,
        track_start,
        track_extent,
        thumb_start,
        thumb_extent,
    }
}

/// Position the moving thumb outline while TrackControl holds the value.
/// The standard CDEF cancels the outline when the pointer leaves the
/// 30-pixel slop region on either axis.
/// Macintosh Toolbox Essentials (1992), pp. 5-7--5-10, 5-58--5-61.
pub fn scrollbar_drag_outline(
    control: &ControlSnapshot,
    start: (i16, i16),
    current: (i16, i16),
) -> Option<i32> {
    let geometry = scrollbar_geometry(control);
    systemless::runner::scrollbar_drag_position(
        control.bounds,
        geometry.vertical,
        (control.value, control.minimum, control.maximum),
        start,
        current,
    )
}

/// Only standard CDEF-owned rectangles are eligible for replacement. Clip
/// them to their owning content and remove the structures of front windows.
/// The control registry follows creation order, so reverse it to match
/// DrawControls: first-created overlapping controls paint frontmost. Retain
/// guest pixels where a custom CDEF intersects a standard control, because
/// its draw region may extend beyond its recorded rectangle.
/// Macintosh Toolbox Essentials (1992), pp. 5-60--5-64, 5-87--5-88.
pub fn control_pieces(
    controls: &[ControlSnapshot],
    menus: &GuestMenuSnapshot,
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Vec<ControlPiece> {
    let mut result = Vec::new();
    let mut covers = Vec::new();
    for frame in windows {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        if !matches!(frame.presentation_definition_id(), Some(0 | 4 | 8 | 12 | 16)) {
            covers.push(structure);
            continue;
        }
        let content = Rect::from(window.bounds);
        for (index, control) in controls.iter().enumerate().rev() {
            if control.owner_id != frame.guest_id
                || !control.owner_visible
                || !control.visible
                || !standard_control(control, menus)
            {
                continue;
            }
            let source = Rect::from(control.bounds);
            if controls.iter().any(|other| {
                other.owner_id == frame.guest_id
                    && other.owner_visible
                    && other.visible
                    && !standard_control(other, menus)
                    && Rect::from(other.bounds).intersection(source).is_some()
            }) {
                continue;
            }
            let clips: Vec<_> = source
                .intersection(content)
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
            let mut clips = clip_to_guest_visible_content(clips, frame);
            for cover in &covers {
                clips = clips
                    .into_iter()
                    .flat_map(|clip| clip.subtract(*cover))
                    .collect();
            }
            result.extend(clips.into_iter().map(|clip| ControlPiece {
                control: index,
                source,
                clip,
            }));
        }
        covers.push(structure);
    }
    result
}

/// Resolve an action only within the current active standard dialog.
/// DialogSelect owns control tracking and item delivery (Macintosh Toolbox
/// Essentials, pp. 6-139--6-141); host actions never update item values.
pub fn dialog_activation_point(
    id: u32,
    generation: u64,
    number: i16,
    identity: (u32, u64),
    dialogs: &[DialogSnapshot],
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Option<(i16, i16)> {
    let dialog = dialogs.iter().find(|d| d.guest_id == id && d.generation == generation)?;
    if !dialog.active || !dialog.visible || !windows.iter().any(|frame| {
        frame.guest_id == id && frame.generation == generation && frame.window.active && frame.window.visible
    }) {
        return None;
    }
    let item = dialog.items.iter().find(|item| item.number == number)?;
    if item.control_identity != Some(identity) || !item.enabled || !item.visible || item.pressed
        || !matches!(item.kind, DialogItemKind::Button | DialogItemKind::Checkbox | DialogItemKind::RadioButton)
    {
        return None;
    }
    dialog_item_pieces(dialogs, windows, viewport).into_iter()
        .filter(|piece| dialogs[piece.dialog].guest_id == id
            && dialogs[piece.dialog].items[piece.item].number == number)
        .find_map(|piece| {
            let mut regions: Vec<_> = piece.clip.intersection(Rect::from(item.bounds)).into_iter().collect();
            for other in dialog.items.iter().filter(|other| other.number != number && other.visible) {
                regions = regions.into_iter().flat_map(|region| region.subtract(Rect::from(other.bounds))).collect();
            }
            let region = regions.into_iter().max_by_key(|r| i64::from(r.width()) * i64::from(r.height()))?;
            Some((i16::try_from((region.top + region.bottom) / 2).ok()?,
                i16::try_from((region.left + region.right) / 2).ok()?))
        })
}

/// Resolve semantic activation to an exposed guest hit point, never a cached
/// host coordinate. The caller must also reject active tracking/modal input.
/// FindControl and TrackControl retain guest ownership of the action and value
/// (Macintosh Toolbox Essentials, pp. 5-55--5-59).
pub fn control_activation_point(
    id: u32,
    generation: u64,
    controls: &[ControlSnapshot],
    menus: &GuestMenuSnapshot,
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Option<(i16, i16)> {
    let control = controls.iter().find(|control| {
        control.guest_id == id && control.generation == generation
    })?;
    if !control.enabled || control.hilite != 0 || !matches!(control.proc_id, 0 | 1 | 2) {
        return None;
    }
    let owner = windows.iter().find(|frame| frame.guest_id == control.owner_id)?;
    if !owner.window.active {
        return None;
    }
    control_pieces(controls, menus, windows, viewport)
        .into_iter()
        .filter(|piece| controls[piece.control].guest_id == id)
        .find_map(|piece| {
            let mut regions = vec![piece.clip];
            // Overlapping controls can make FindControl select another handle.
            // Use only unambiguous pixels, even for two standard controls.
            for other in controls.iter().filter(|other| {
                other.guest_id != id && other.owner_id == control.owner_id
                    && other.owner_visible && other.visible
            }) {
                regions = regions.into_iter()
                    .flat_map(|region| region.subtract(Rect::from(other.bounds)))
                    .collect();
            }
            let region = regions.into_iter().max_by_key(|region| {
                i64::from(region.right - region.left) * i64::from(region.bottom - region.top)
            })?;
            Some((
                i16::try_from((region.top + region.bottom) / 2).ok()?,
                i16::try_from((region.left + region.right) / 2).ok()?,
            ))
        })
}

/// Replace only a visible standard text LDEF in a known rectangular window.
/// A custom definition or overlapping custom control retains guest pixels.
/// More Macintosh Toolbox (1993), pp. 4-3--4-7, 4-70--4-76.
pub fn list_pieces(
    lists: &[ListManagerSnapshot],
    controls: &[ControlSnapshot],
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
) -> Vec<ListPiece> {
    let mut pieces = Vec::new();
    let mut covers = Vec::new();
    for frame in windows {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        if !matches!(frame.presentation_definition_id(), Some(0 | 4 | 8 | 12 | 16)) {
            covers.push(structure);
            continue;
        }
        for (index, list) in lists.iter().enumerate() {
            if list.owner_port != frame.guest_id
                || list.definition_id != 0
                || !list.draw_enabled
                || list.text_cells.is_none()
            {
                continue;
            }
            let Some(source) = list.global_view_rect.map(Rect::from) else {
                continue;
            };
            if controls.iter().any(|control| {
                control.owner_id == frame.guest_id
                    && control.visible
                    && !matches!(control.proc_id, 0 | 1 | 2 | 16)
                    && Rect::from(control.bounds).intersection(source).is_some()
            }) {
                continue;
            }
            let clips: Vec<_> = source
                .intersection(Rect::from(window.bounds))
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
            let mut clips = clip_to_guest_visible_content(clips, frame);
            for cover in &covers {
                clips = clips
                    .into_iter()
                    .flat_map(|clip| clip.subtract(*cover))
                    .collect();
            }
            pieces.extend(clips.into_iter().map(|clip| ListPiece {
                list: index,
                source,
                clip,
            }));
        }
        covers.push(structure);
    }
    pieces
}

/// Restyle only the grow-icon background and separator pixels after the guest
/// draws them; the WDEF alone does not reserve content pixels for scrollbars.
/// Visible guest controls take precedence over this decorative presentation.
/// Macintosh Toolbox Essentials (1992), pp. 4-4--4-5, 4-12, 4-111--4-112,
/// 5-60--5-64.
pub fn gutter_pieces(
    windows: &[WindowFrameSnapshot],
    controls: &[ControlSnapshot],
    viewport: Rect,
) -> Vec<GutterPiece> {
    let mut result = Vec::new();
    let mut covers = Vec::new();
    for (index, frame) in windows.iter().enumerate() {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        let content = Rect::from(window.bounds);
        if frame.grow_icon_drawn
            && matches!(frame.presentation_definition_id(), Some(0 | 8))
            && content.width() > 30
            && content.height() > 30
            && structure.intersection(content) == Some(content)
        {
            let right = content.right - 15;
            let bottom = content.bottom - 15;
            for (kind, source) in [
                (
                    GutterKind::Vertical,
                    Rect {
                        top: content.top,
                        left: right,
                        bottom: content.bottom,
                        right: right + 1,
                    },
                ),
                (
                    GutterKind::Horizontal,
                    Rect {
                        top: bottom,
                        left: content.left - 1,
                        bottom: bottom + 1,
                        right: content.right + 2,
                    },
                ),
                (
                    GutterKind::GrowBox,
                    Rect {
                        top: bottom,
                        left: right,
                        bottom: content.bottom,
                        right: content.right,
                    },
                ),
            ] {
                let clips: Vec<_> = source.intersection(viewport).into_iter().collect();
                let mut clips = clip_to_guest_visible_content(clips, frame);
                for cover in &covers {
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(*cover))
                        .collect();
                }
                for control in controls.iter().filter(|control| {
                    control.owner_id == frame.guest_id
                        && control.owner_visible
                        && control.visible
                }) {
                    let bounds = Rect::from(control.bounds);
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(bounds))
                        .collect();
                }
                result.extend(clips.into_iter().map(|clip| GutterPiece {
                    window: index,
                    source,
                    clip,
                    kind,
                }));
            }
        }
        covers.push(structure);
    }
    result
}

/// Preserve the guest's frame/content boundary and front-to-back ordering.
/// Standard window variants: Inside Macintosh I, I-275; structure/content
/// regions and FindWindow: I-276--I-288. Custom WDEFs retain their pixels.
pub fn frame_pieces(windows: &[WindowFrameSnapshot], viewport: Rect) -> Vec<FramePiece> {
    let mut result = Vec::new();
    let mut covers = Vec::new();
    for (index, frame) in windows.iter().enumerate() {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        let content = Rect::from(window.bounds);
        let supported = matches!(
            frame.presentation_definition_id(),
            Some(0 | 1 | 2 | 3 | 4 | 5 | 8 | 12 | 16)
        );
        if supported && structure.intersection(content) == Some(content) {
            let has_title = matches!(
                frame.presentation_definition_id(),
                Some(0 | 4 | 5 | 8 | 12 | 16)
            );
            for source in structure.subtract(content) {
                let title = has_title && source.bottom == content.top;
                let mut clips: Vec<_> = source.intersection(viewport).into_iter().collect();
                for cover in &covers {
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(*cover))
                        .collect();
                }
                result.extend(clips.into_iter().map(|clip| FramePiece {
                    window: index,
                    source,
                    clip,
                    title,
                }));
            }
        }
        // Bounding-box occlusion is conservative for custom, nonrectangular WDEFs:
        // leave the original guest pixels there rather than paint over them.
        covers.push(structure);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{control_pieces, dialog_item_pieces, frame_pieces, gutter_pieces, list_pieces, popup_control_label, scrollbar_drag_outline, scrollbar_geometry, text_edit_pieces, GutterKind, Rect};
    use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};
    use systemless::runner::{ControlSnapshot, DialogItemKind, DialogItemSnapshot, DialogSnapshot, ListManagerSnapshot, TextEditSnapshot, WindowFrameSnapshot, WindowSnapshot};

    fn window(
        bounds: (i16, i16, i16, i16),
        visible: bool,
        definition_id: i16,
    ) -> WindowFrameSnapshot {
        WindowFrameSnapshot {
            guest_id: 1,
            generation: 1,
            window: WindowSnapshot {
                title: "Test".into(),
                bounds,
                structure_bounds: Some((bounds.0 - 19, bounds.1 - 1, bounds.2 + 2, bounds.3 + 2)),
                visible_region: None,
                update_region: None,
                visible,
                active: true,
            },
            definition_id: Some(definition_id),
            rectangular_regions: true,
            visible_content_rects: Some(vec![bounds]),
            close_box: true,
            grow_icon_drawn: false,
        }
    }

    fn control(owner_id: u32, proc_id: i16, bounds: (i16, i16, i16, i16)) -> ControlSnapshot {
        ControlSnapshot {
            guest_id: 10,
            generation: 1,
            owner_id,
            proc_id,
            local_bounds: bounds,
            bounds,
            owner_visible: true,
            visible: true,
            enabled: true,
            hilite: 0,
            value: 0,
            minimum: 0,
            maximum: 10,
            title: String::new(),
            popup_menu_id: None,
            popup_title_width: None,
            popup_text_inset: 15,
            popup_ink: None,
            popup_indicator: None,
            popup_box_bounds: None,
            popup_font: None,
            font_style: None,
        }
    }

    #[test]
    fn texture_preparation_preserves_unowned_pixels_and_clip_holes() {
        let original: Vec<u8> = (0..8 * 6 * 4).map(|index| index as u8).collect();
        let mut pixels = original.clone();
        let clips = [
            Rect { top: -2, left: -3, bottom: 2, right: 3 },
            Rect { top: 3, left: 4, bottom: 9, right: 10 },
        ];
        let color = [12, 34, 56, 255];
        super::fill_texture_clips(&mut pixels, 8, 6, &clips, color);
        for y in 0..6 {
            for x in 0..8 {
                let offset = (y * 8 + x) * 4;
                let owned = (y < 2 && x < 3) || (y >= 3 && x >= 4);
                assert_eq!(&pixels[offset..offset + 4], if owned {
                    &color[..]
                } else {
                    &original[offset..offset + 4]
                });
            }
        }
        super::fill_texture_clips(&mut pixels, 8, 6, &[], [0; 4]);
        assert_eq!(&pixels[(2 * 8 + 3) * 4..(2 * 8 + 4) * 4],
            &original[(2 * 8 + 3) * 4..(2 * 8 + 4) * 4]);
    }

    #[test]
    fn dialog_activation_rejects_stale_disabled_obscured_and_inactive_items() {
        let mut windows = vec![window((50, 50, 180, 220), true, 1)];
        let mut dialogs = vec![DialogSnapshot {
            guest_id: 1, generation: 1, bounds: (50, 50, 180, 220),
            visible: true, active: true, default_item: Some(1),
            cancel_item: None, edit_field: None,
            items: vec![DialogItemSnapshot {
                static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                control_identity: Some((10, 1)),
                pressed: false, number: 1, kind: DialogItemKind::Button,
                bounds: (90, 90, 110, 180), text: "OK".into(),
                enabled: true, visible: true, value: None,
                selection: None, caret_visible: None,
            }],
        }];
        let resolve = |dialogs: &[DialogSnapshot], windows: &[WindowFrameSnapshot]| {
            super::dialog_activation_point(1, 1, 1, (10, 1), dialogs, windows, Rect::from((0, 0, 300, 300)))
        };
        assert_eq!(resolve(&dialogs, &windows), Some((100, 135)));
        dialogs[0].items[0].control_identity = Some((11, 1));
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].items[0].control_identity = Some((10, 2));
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].items[0].control_identity = Some((10, 1));
        dialogs[0].generation = 2;
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].generation = 1;
        dialogs[0].items[0].enabled = false;
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].items[0].enabled = true;
        dialogs[0].items[0].pressed = true;
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].items[0].pressed = false;
        dialogs[0].active = false;
        assert_eq!(resolve(&dialogs, &windows), None);
        dialogs[0].active = true;
        windows[0].window.active = false;
        assert_eq!(resolve(&dialogs, &windows), None);
        windows[0].window.active = true;
        let mut cover = window((80, 80, 130, 200), true, 1);
        cover.guest_id = 2;
        windows.insert(0, cover);
        assert_eq!(resolve(&dialogs, &windows), None);
        windows.remove(0);
        // A clip consisting solely of the default-button halo is not clickable.
        windows[0].visible_content_rects = Some(vec![(86, 86, 90, 184)]);
        assert_eq!(resolve(&dialogs, &windows), None);
        windows[0].visible_content_rects = Some(vec![(90, 90, 110, 120)]);
        assert_eq!(resolve(&dialogs, &windows), Some((100, 105)));
        let mut overlap = dialogs[0].items[0].clone();
        overlap.number = 2;
        dialogs[0].items.push(overlap);
        assert_eq!(resolve(&dialogs, &windows), None);
    }

    #[test]
    fn inactive_standard_dialog_items_clip_below_front_window() {
        let mut front = window((80, 80, 130, 150), true, 0);
        front.guest_id = 2;
        let mut back = window((50, 50, 180, 220), true, 4);
        back.window.active = false;
        let windows = [front, back];
        let mut dialog = DialogSnapshot {
            guest_id: 1,
            generation: 1,
            bounds: (50, 50, 180, 220),
            visible: true,
            active: false,
            default_item: None,
            cancel_item: None,
            edit_field: None,
            items: vec![DialogItemSnapshot {
                static_text_layout: Some(systemless::runner::DialogStaticTextLayout { wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false }),
                edit_text_layout: Some(systemless::runner::DialogEditTextLayout { font: (0, 0), baseline: 12, line_height: 16, wrap: false, text_edit_geometry: false }),
                control_identity: None,
                pressed: false,
                number: 1,
                kind: DialogItemKind::StaticText,
                bounds: (90, 90, 110, 180),
                text: "Behind".into(),
                enabled: false,
                visible: true,
                value: None,
                selection: None,
                caret_visible: Some(true),
            }],
        };
        let viewport = Rect::from((0, 0, 300, 300));
        let pieces = dialog_item_pieces(&[dialog.clone()], &windows, viewport);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].source, Rect::from((90, 90, 110, 180)));
        assert_eq!(pieces[0].clip, Rect::from((90, 152, 110, 180)));

        let text_layout = dialog.items[0].static_text_layout.take();
        assert!(dialog_item_pieces(&[dialog.clone()], &windows, viewport).is_empty(),
            "unknown or unsupported statText layout must preserve the guest pixels");
        dialog.items[0].static_text_layout = text_layout;
        dialog.items[0].kind = DialogItemKind::EditText;
        assert!(!dialog_item_pieces(&[dialog.clone()], &windows, viewport).is_empty());
        let edit_layout = dialog.items[0].edit_text_layout.take();
        assert!(dialog_item_pieces(&[dialog.clone()], &windows, viewport).is_empty(),
            "unsupported editText geometry must retain guest rendering");
        dialog.items[0].edit_text_layout = edit_layout;
        dialog.items[0].kind = DialogItemKind::UserItem;
        assert!(dialog_item_pieces(&[dialog.clone()], &windows, viewport).is_empty());
        dialog.items[0].kind = DialogItemKind::StaticText;
        let mut custom_windows = windows.to_vec();
        custom_windows[1].definition_id = Some(32);
        assert!(dialog_item_pieces(&[dialog.clone()], &custom_windows, viewport).is_empty());
        dialog.generation += 1;
        assert!(dialog_item_pieces(&[dialog], &windows, viewport).is_empty());
    }

    #[test]
    fn popup_overlay_requires_its_live_selected_menu_item() {
        let mut popup = control(1, 1008, (80, 100, 120, 180));
        popup.popup_menu_id = Some(143);
        popup.value = 2;
        let mut menus = GuestMenuSnapshot::default();
        assert_eq!(popup_control_label(&popup, &menus), None);
        menus.menus.push(GuestMenu {
            guest_id: 143,
            generation: 1,
            id: 143,
            title: "Loadout".into(),
            enabled: true,
            standard_definition: true,
            hierarchical: true,
            visible_in_menu_bar: false,
            items: vec![GuestMenuItem {
                mark: 0,
                style: 0,
                number: 2,
                text: "Scout Kit".into(),
                enabled: true,
                checked: false,
                key_equivalent: None,
                submenu_id: None,
                separator: false,
            }],
        });
        assert_eq!(popup_control_label(&popup, &menus), Some("Scout Kit"));
        menus.menus[0].items[0].separator = true;
        assert_eq!(popup_control_label(&popup, &menus), None);
    }

    #[test]
    fn semantic_control_activation_revalidates_identity_visibility_and_ownership() {
        let mut windows = vec![window((20, 0, 160, 180), true, 0)];
        let mut controls = vec![control(1, 1, (40, 20, 60, 120))];
        let menus = GuestMenuSnapshot::default();
        let viewport = Rect::from((0, 0, 160, 180));
        let resolve = |controls: &[ControlSnapshot], windows: &[WindowFrameSnapshot]| {
            super::control_activation_point(10, 1, controls, &menus, windows, viewport)
        };
        assert_eq!(resolve(&controls, &windows), Some((50, 70)));
        controls[0].generation = 2;
        assert_eq!(resolve(&controls, &windows), None);
        controls[0].generation = 1;
        controls[0].enabled = false;
        assert_eq!(resolve(&controls, &windows), None);
        controls[0].enabled = true;
        windows[0].window.active = false;
        assert_eq!(resolve(&controls, &windows), None);
        windows[0].window.active = true;
        windows[0].visible_content_rects = Some(vec![(40, 20, 60, 40)]);
        assert_eq!(resolve(&controls, &windows), Some((50, 30)));
        let mut overlap = control(1, 0, (40, 20, 60, 40));
        overlap.guest_id = 11;
        controls.push(overlap);
        assert_eq!(resolve(&controls, &windows), None);
        controls[1].visible = false;
        assert_eq!(resolve(&controls, &windows), Some((50, 30)));
        controls[0].proc_id = 99;
        assert_eq!(resolve(&controls, &windows), None);
    }

    #[test]
    fn standard_controls_clip_to_content_and_front_windows() {
        let front = window((40, 60, 90, 140), true, 0);
        let mut back = window((65, 10, 150, 170), true, 0);
        back.guest_id = 2;
        let controls = [
            control(2, 16, (70, 20, 86, 150)),
            control(2, 99, (95, 20, 111, 150)),
            control(2, 1, (112, 20, 130, 155)),
        ];
        let pieces = control_pieces(&controls, &GuestMenuSnapshot::default(), &[front, back], Rect::from((20, 0, 160, 180)));
        assert!(pieces.iter().any(|piece| piece.control == 0));
        assert!(pieces.iter().any(|piece| piece.control == 2));
        assert!(pieces.iter().all(|piece| piece.control != 1));
        for piece in pieces {
            assert!(piece.clip.intersection(Rect::from((40, 60, 92, 142))).is_none());
            assert_eq!(piece.clip.intersection(Rect::from((65, 10, 150, 170))), Some(piece.clip));
        }
    }

    #[test]
    fn standard_list_clips_beneath_front_window_and_custom_definition_falls_back() {
        let front = window((40, 60, 90, 140), true, 0);
        let mut back = window((65, 10, 150, 170), true, 0);
        back.guest_id = 2;
        let mut list = ListManagerSnapshot {
            guest_id: 10,
            generation: 1,
            definition_id: 0,
            owner_port: 2,
            global_view_rect: Some((70, 20, 130, 160)),
            view_rect: (5, 10, 65, 150),
            data_bounds: (0, 0, 2, 1),
            cell_size: (20, 140),
            visible: (0, 0, 2, 1),
            draw_enabled: true,
            active: true,
            cells: Default::default(),
            text_cells: Some(Default::default()),
            selected: Default::default(),
            vertical_scrollbar: None,
            horizontal_scrollbar: None,
        };
        let viewport = Rect::from((20, 0, 160, 180));
        let pieces = list_pieces(&[list.clone()], &[], &[front.clone(), back.clone()], viewport);
        assert!(!pieces.is_empty());
        assert!(pieces.iter().all(|piece| {
            piece.clip.intersection(Rect::from((40, 60, 92, 142))).is_none()
        }));
        list.definition_id = 128;
        assert!(list_pieces(&[list.clone()], &[], &[front, back.clone()], viewport).is_empty());
        list.definition_id = 0;
        list.draw_enabled = false;
        assert!(list_pieces(&[list], &[], &[back], viewport).is_empty());
    }

    #[test]
    fn text_edit_clips_to_owner_and_front_window_with_custom_fallback() {
        let front = window((40, 60, 90, 140), true, 0);
        let mut back = window((65, 10, 150, 170), true, 0);
        back.guest_id = 2;
        let record = TextEditSnapshot {
            drawing_intact: true,
            painted_regions: vec![(70, 20, 130, 160)],
            guest_id: 10,
            generation: 1,
            owner_port: 2,
            global_dest_rect: Some((70, 20, 130, 160)),
            global_view_rect: Some((70, 20, 130, 160)),
            dest_rect: (5, 10, 65, 150),
            view_rect: (5, 10, 65, 150),
            text: b"hello".to_vec(),
            selection: (0, 0),
            active: true,
            caret_visible: true,
            clips_line_offsets_to_visible_text: false,
            justification: 0,
            line_count: 1,
            line_starts: Some(vec![0, 5]),
            line_height: 14,
            font_ascent: 11,
            font: 0,
            face: 0,
            size: 12,
            styled: false,
            style_runs: None,
            line_metrics: None,
            line_layout_policy: systemless::runner::TextEditLineLayoutPolicy::CumulativeGuestMetrics,
            paint: None,
        };
        let mut boundary = record.clone();
        boundary.text = b"ab \rcd".to_vec();
        boundary.line_starts = Some(vec![0, 4, 6]);
        boundary.line_count = 2;
        boundary.line_height = 10;
        boundary.dest_rect = (0, 0, 20, 100);
        boundary.view_rect = (0, 0, 20, 100);
        boundary.selection = (4, 4);
        assert_eq!(boundary.caret_line(), Some((0, 4)), "68k measures canonical CR/space bytes on the first matching line");
        boundary.clips_line_offsets_to_visible_text = true;
        assert_eq!(boundary.caret_line(), Some((0, 2)), "PPC measures the trimmed first matching line");
        boundary.selection = (5, 5);
        assert_eq!(boundary.caret_line(), Some((1, 1)));
        boundary.clips_line_offsets_to_visible_text = false;
        boundary.view_rect.0 = 10;
        boundary.selection = (4, 4);
        assert_eq!(boundary.caret_line(), Some((1, 0)), "68k resolves the first visible line after scrolling");
        boundary.text = b"ab\r\rc".to_vec();
        boundary.line_starts = Some(vec![0, 3, 4, 5]);
        boundary.line_count = 3;
        boundary.view_rect = (0, 0, 30, 100);
        assert_eq!(boundary.caret_line(), Some((1, 1)), "empty CR line retains its byte span");
        boundary.clips_line_offsets_to_visible_text = true;
        assert_eq!(boundary.caret_line(), Some((1, 0)));
        boundary.active = false;
        assert_eq!(boundary.caret_line(), None);
        let viewport = Rect::from((20, 0, 160, 180));
        let pieces = text_edit_pieces(&[record.clone()], &[], &[], &[front.clone(), back.clone()], viewport);
        assert!(!pieces.is_empty());
        assert!(pieces.iter().all(|piece| piece.clip.intersection(Rect::from((40, 60, 92, 142))).is_none()));
        let mut custom = back.clone();
        custom.definition_id = Some(128);
        assert!(text_edit_pieces(&[record.clone()], &[], &[], &[front.clone(), custom], viewport).is_empty());
        let mut partial = record.clone();
        partial.painted_regions = vec![(100, 30, 110, 70)];
        let partial_pieces = text_edit_pieces(&[partial], &[], &[], &[back.clone()], viewport);
        assert_eq!(partial_pieces.len(), 1);
        assert_eq!(partial_pieces[0].clip, Rect::from((100, 30, 110, 70)));
        let mut scaled = record.clone();
        scaled.size = 120;
        assert!(text_edit_pieces(&[scaled], &[], &[], &[back.clone()], viewport).is_empty());
        let mut styled = record;
        styled.styled = true;
        assert!(text_edit_pieces(&[styled.clone()], &[], &[], &[back.clone()], viewport).is_empty());
        let candidates = super::styled_text_edit_candidates(&[styled.clone()], &[], &[], &[back.clone()], viewport);
        assert!(!candidates.is_empty());
        let covered = super::styled_text_edit_candidates(&[styled.clone()], &[], &[], &[front, back.clone()], viewport);
        assert!(covered.iter().all(|piece| piece.clip.intersection(Rect::from((40, 60, 92, 142))).is_none()));
        styled.painted_regions = vec![(100, 30, 110, 70)];
        let candidates = super::styled_text_edit_candidates(&[styled.clone()], &[], &[], &[back.clone()], viewport);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].clip, Rect::from((100, 30, 110, 70)));
        styled.drawing_intact = false;
        assert!(super::styled_text_edit_candidates(&[styled.clone()], &[], &[], &[back.clone()], viewport).is_empty());
        styled.drawing_intact = true;
        let mut custom = back;
        custom.definition_id = Some(128);
        assert!(super::styled_text_edit_candidates(&[styled], &[], &[], &[custom], viewport).is_empty());
    }

    #[test]
    fn overlapping_controls_follow_guest_draw_order_and_custom_fallback() {
        let window = window((40, 40, 180, 240), true, 0);
        let controls = [
            control(1, 0, (60, 60, 100, 140)),
            control(1, 1, (80, 100, 120, 180)),
        ];
        let viewport = Rect::from((20, 0, 200, 260));
        let pieces = control_pieces(&controls, &GuestMenuSnapshot::default(), &[window.clone()], viewport);
        assert_eq!(pieces.iter().map(|piece| piece.control).collect::<Vec<_>>(), [1, 0]);

        let mut overridden = controls.to_vec();
        overridden[1].font_style = Some(systemless::runner::ControlFontStyle {
            flags: 7, font: 3, size: 10, style: 1, mode: 1, justification: -1,
            foreground: [0; 3], background: [65535; 3],
        });
        let pieces = control_pieces(&overridden, &GuestMenuSnapshot::default(), &[window.clone()], viewport);
        // Override retains its guest pixels and masks the intersecting standard
        // control rather than being erased underneath a host-shaped label.
        assert!(pieces.is_empty(), "overlapping guest-owned font override retains both controls");

        let mut controls_with_custom = controls.to_vec();
        controls_with_custom.push(control(1, 99, (105, 150, 130, 190)));
        let pieces = control_pieces(&controls_with_custom, &GuestMenuSnapshot::default(), &[window], viewport);
        assert_eq!(pieces.iter().map(|piece| piece.control).collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn scrollbar_thumb_follows_guest_value_and_range() {
        let mut bar = control(1, 16, (360, 80, 376, 540));
        let start = scrollbar_geometry(&bar);
        assert!(!start.vertical);
        assert_eq!(start.arrow_extent, 16);
        assert_eq!(start.thumb_start, 16);
        assert_eq!(start.thumb_extent, 16);
        bar.value = 5;
        let middle = scrollbar_geometry(&bar);
        assert_eq!(middle.thumb_start, 222);
        bar.value = 10;
        let end = scrollbar_geometry(&bar);
        assert_eq!(end.thumb_start, 428);
    }

    #[test]
    fn scrollbar_drag_outline_clamps_and_cancels_without_changing_value() {
        let bar = control(1, 16, (360, 80, 376, 540));
        let start = (368, 104);
        assert_eq!(scrollbar_drag_outline(&bar, start, start), Some(16));
        assert_eq!(scrollbar_drag_outline(&bar, start, (368, 500)), Some(412));
        assert_eq!(scrollbar_drag_outline(&bar, start, (368, 569)), Some(428));
        assert_eq!(scrollbar_drag_outline(&bar, start, (368, 570)), None);
        assert_eq!(scrollbar_drag_outline(&bar, start, (368, 600)), None);
        assert_eq!(scrollbar_drag_outline(&bar, start, (329, 500)), None);
        let vertical = control(2, 16, (128, 514, 242, 530));
        assert_eq!(scrollbar_drag_outline(&vertical, (150, 522), (271, 522)), Some(82));
        assert_eq!(scrollbar_drag_outline(&vertical, (150, 522), (272, 522)), None);
        assert_eq!(scrollbar_drag_outline(&vertical, (150, 522), (218, 560)), None);
        let mut wide = control(3, 16, (i16::MIN, 0, i16::MAX, 16));
        wide.minimum = i16::MIN;
        wide.maximum = i16::MAX;
        wide.value = i16::MAX;
        assert_eq!(scrollbar_geometry(&wide).thumb_start, 65503);
        assert_eq!(bar.value, 0);
    }

    #[test]
    fn nonrectangular_window_regions_retain_guest_pixels() {
        let mut frame = window((40, 40, 180, 240), true, 0);
        frame.grow_icon_drawn = true;
        let viewport = Rect::from((20, 0, 200, 260));
        let controls = [control(1, 0, (60, 60, 100, 140))];
        assert!(!frame_pieces(&[frame.clone()], viewport).is_empty());
        assert!(!gutter_pieces(&[frame.clone()], &controls, viewport).is_empty());
        assert!(!control_pieces(
            &controls,
            &GuestMenuSnapshot::default(),
            &[frame.clone()],
            viewport
        )
        .is_empty());

        frame.rectangular_regions = false;
        assert!(frame_pieces(&[frame.clone()], viewport).is_empty());
        assert!(gutter_pieces(&[frame.clone()], &controls, viewport).is_empty());
        assert!(control_pieces(
            &controls,
            &GuestMenuSnapshot::default(),
            &[frame],
            viewport
        )
        .is_empty());
    }

    #[test]
    fn visible_region_hole_never_receives_control_or_grow_overlay() {
        let mut frame = window((40, 40, 180, 240), true, 0);
        frame.grow_icon_drawn = true;
        frame.visible_content_rects = Some(vec![(40, 40, 180, 100), (40, 140, 180, 240)]);
        let viewport = Rect::from((20, 0, 200, 260));
        let hole = Rect::from((40, 100, 180, 140));
        let controls = [control(1, 0, (60, 80, 100, 160))];
        let themed_controls = control_pieces(
            &controls,
            &GuestMenuSnapshot::default(),
            &[frame.clone()],
            viewport,
        );
        assert_eq!(themed_controls.len(), 2);
        assert!(themed_controls
            .iter()
            .all(|piece| piece.clip.intersection(hole).is_none()));
        assert!(gutter_pieces(&[frame.clone()], &controls, viewport)
            .iter()
            .all(|piece| piece.clip.intersection(hole).is_none()));

        frame.visible_content_rects = None;
        assert!(frame_pieces(&[frame.clone()], viewport).is_empty());
        assert!(control_pieces(
            &controls,
            &GuestMenuSnapshot::default(),
            &[frame],
            viewport,
        )
        .is_empty());
    }

    #[test]
    fn clipped_chrome_never_paints_guest_content_or_front_windows() {
        let windows = [
            window((40, 60, 90, 140), true, 0),
            window((65, 10, 150, 170), true, 0),
        ];
        let viewport = Rect::from((20, 0, 160, 180));
        let pieces = frame_pieces(&windows, viewport);
        assert!(pieces.iter().any(|p| p.window == 1 && p.title));
        for piece in &pieces {
            assert_eq!(piece.clip.intersection(viewport), Some(piece.clip));
            assert!(piece
                .clip
                .intersection(windows[piece.window].window.bounds.into())
                .is_none());
            for front in &windows[..piece.window] {
                assert!(piece
                    .clip
                    .intersection(front.window.structure_bounds.unwrap().into())
                    .is_none());
            }
        }
        // Every visible standard frame pixel is painted exactly once.
        for y in 20..160 {
            for x in 0..180 {
                let point = Rect {
                    top: y,
                    left: x,
                    bottom: y + 1,
                    right: x + 1,
                };
                let expected = windows
                    .iter()
                    .find(|w| {
                        Rect::from(w.window.structure_bounds.unwrap())
                            .intersection(point)
                            .is_some()
                    })
                    .is_some_and(|w| Rect::from(w.window.bounds).intersection(point).is_none());
                assert_eq!(
                    pieces
                        .iter()
                        .filter(|p| p.clip.intersection(point).is_some())
                        .count(),
                    usize::from(expected)
                );
            }
        }
    }

    #[test]
    fn gutters_stay_on_standard_document_edges_and_below_front_windows() {
        let mut windows = [
            window((60, 60, 120, 120), true, 8),
            window((40, 40, 140, 140), true, 8),
            window((20, 20, 160, 160), true, 99),
        ];
        assert!(gutter_pieces(&windows, &[], Rect::from((0, 0, 180, 180))).is_empty());
        windows[0].grow_icon_drawn = true;
        windows[1].grow_icon_drawn = true;
        let pieces = gutter_pieces(&windows, &[], Rect::from((0, 0, 180, 180)));
        assert!(pieces.iter().all(|piece| match piece.kind {
            GutterKind::Vertical => piece.source.width() == 1,
            GutterKind::Horizontal => piece.source.height() == 1,
            GutterKind::GrowBox => piece.source.width() == 15 && piece.source.height() == 15,
        }));
        assert!(pieces.iter().any(|piece| {
            piece.window == 0
                && piece.kind == GutterKind::Vertical
                && piece.source == Rect::from((60, 105, 120, 106))
        }));
        assert!(pieces.iter().any(|piece| {
            piece.window == 0
                && piece.kind == GutterKind::Horizontal
                && piece.source == Rect::from((105, 59, 106, 122))
        }));
        assert!(pieces
            .iter()
            .any(|piece| { piece.window == 0 && piece.kind == GutterKind::GrowBox }));
        assert!(pieces.iter().all(|piece| piece.window != 2));
        let front = Rect::from(windows[0].window.structure_bounds.unwrap());
        assert!(pieces
            .iter()
            .filter(|piece| piece.window == 1)
            .all(|piece| piece.clip.intersection(front).is_none()));
    }

    #[test]
    fn grow_overlay_leaves_guest_controls_visible_at_content_edges() {
        let mut frame = window((60, 60, 120, 120), true, 8);
        frame.grow_icon_drawn = true;
        let custom_corner = control(1, 99, (108, 108, 119, 119));
        let standard_bar = control(1, 16, (60, 105, 105, 120));
        let controls = [custom_corner, standard_bar];
        let pieces = gutter_pieces(
            &[frame],
            &controls,
            Rect::from((0, 0, 180, 180)),
        );
        for piece in &pieces {
            for control in &controls {
                assert!(piece.clip.intersection(Rect::from(control.bounds)).is_none());
            }
        }
        assert!(pieces.iter().any(|piece| piece.kind == GutterKind::GrowBox));
    }

    #[test]
    fn hidden_and_custom_frames_keep_guest_presentation() {
        let windows = [
            window((30, 10, 90, 100), false, 0),
            window((60, 20, 120, 130), true, 128),
        ];
        assert!(frame_pieces(&windows, Rect::from((20, 0, 200, 200))).is_empty());
    }
}
