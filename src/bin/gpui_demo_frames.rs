//! Rectangular guest-frame overlays. Content pixels and input remain guest-owned.

use systemless::runner::{ControlSnapshot, DialogSnapshot, ListManagerSnapshot, TextEditSnapshot, WindowFrameSnapshot};
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
    matches!(control.proc_id, 0 | 1 | 2 | 16)
        || popup_control_label(control, menus).is_some()
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
        if !matches!(frame.definition_id, Some(0 | 4 | 8 | 12 | 16))
            || dialogs.iter().any(|dialog| dialog.guest_id == frame.guest_id)
        {
            covers.push(structure);
            continue;
        }
        for (index, record) in records.iter().enumerate() {
            if record.owner_port != frame.guest_id
                || record.styled
                || record.face != 0
                || record.justification != 0
                || record.line_height <= 0
                || record.display_lines().is_none()
            {
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
            let mut clips: Vec<_> = source
                .intersection(Rect::from(window.bounds))
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
            if let Some(visible) = window.visible_region.map(Rect::from) {
                clips = clips.into_iter().filter_map(|clip| clip.intersection(visible)).collect();
            }
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
    let thumb_start = track_start + if span > 0 { value * travel / span } else { 0 };
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
/// perpendicular 30-pixel slop region.
/// Macintosh Toolbox Essentials (1992), pp. 5-7--5-10, 5-58--5-61.
pub fn scrollbar_drag_outline(
    control: &ControlSnapshot,
    start: (i16, i16),
    current: (i16, i16),
) -> Option<i32> {
    let rect = Rect::from(control.bounds);
    let geometry = scrollbar_geometry(control);
    let (start_axis, current_axis, cross, cross_min, cross_max) = if geometry.vertical {
        (i32::from(start.0), i32::from(current.0), i32::from(current.1), rect.left, rect.right)
    } else {
        (i32::from(start.1), i32::from(current.1), i32::from(current.0), rect.top, rect.bottom)
    };
    if cross < cross_min - 30 || cross >= cross_max + 30 {
        return None;
    }
    let travel = geometry.track_extent - geometry.thumb_extent;
    Some((geometry.thumb_start + current_axis - start_axis).clamp(
        geometry.track_start,
        geometry.track_start + travel.max(0),
    ))
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
        if !matches!(frame.definition_id, Some(0 | 4 | 8 | 12 | 16)) {
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
            let mut clips: Vec<_> = source
                .intersection(content)
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
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
        if !matches!(frame.definition_id, Some(0 | 4 | 8 | 12 | 16)) {
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
            let mut clips: Vec<_> = source
                .intersection(Rect::from(window.bounds))
                .and_then(|rect| rect.intersection(viewport))
                .into_iter()
                .collect();
            if let Some(visible) = window.visible_region.map(Rect::from) {
                clips = clips
                    .into_iter()
                    .filter_map(|clip| clip.intersection(visible))
                    .collect();
            }
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

/// Restyle the content-edge areas reserved for standard document scrollbars
/// and DrawGrowIcon without painting over another window or a custom WDEF.
pub fn gutter_pieces(windows: &[WindowFrameSnapshot], viewport: Rect) -> Vec<GutterPiece> {
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
        if matches!(frame.definition_id, Some(0 | 4 | 8 | 12))
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
                        left: right,
                        bottom,
                        ..content
                    },
                ),
                (
                    GutterKind::Horizontal,
                    Rect {
                        top: bottom,
                        right,
                        ..content
                    },
                ),
                (
                    GutterKind::GrowBox,
                    Rect {
                        top: bottom,
                        left: right,
                        ..content
                    },
                ),
            ] {
                let mut clips: Vec<_> = source.intersection(viewport).into_iter().collect();
                for cover in &covers {
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(*cover))
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
            frame.definition_id,
            Some(0 | 1 | 2 | 3 | 4 | 5 | 8 | 12 | 16)
        );
        if supported && structure.intersection(content) == Some(content) {
            let has_title = matches!(frame.definition_id, Some(0 | 4 | 5 | 8 | 12 | 16));
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
    use super::{control_pieces, frame_pieces, gutter_pieces, list_pieces, popup_control_label, scrollbar_drag_outline, scrollbar_geometry, text_edit_pieces, GutterKind, Rect};
    use systemless::menu_model::{GuestMenu, GuestMenuItem, GuestMenuSnapshot};
    use systemless::runner::{ControlSnapshot, ListManagerSnapshot, TextEditSnapshot, WindowFrameSnapshot, WindowSnapshot};

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
            close_box: true,
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
        }
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
            justification: 0,
            line_count: 1,
            line_starts: Some(vec![0, 5]),
            line_height: 14,
            font: 0,
            face: 0,
            size: 12,
            styled: false,
        };
        let viewport = Rect::from((20, 0, 160, 180));
        let pieces = text_edit_pieces(&[record.clone()], &[], &[], &[front.clone(), back.clone()], viewport);
        assert!(!pieces.is_empty());
        assert!(pieces.iter().all(|piece| piece.clip.intersection(Rect::from((40, 60, 92, 142))).is_none()));
        let mut custom = back.clone();
        custom.definition_id = Some(128);
        assert!(text_edit_pieces(&[record.clone()], &[], &[], &[front, custom], viewport).is_empty());
        let mut styled = record;
        styled.styled = true;
        assert!(text_edit_pieces(&[styled], &[], &[], &[back], viewport).is_empty());
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
        assert_eq!(scrollbar_drag_outline(&bar, start, (368, 600)), Some(428));
        assert_eq!(scrollbar_drag_outline(&bar, start, (329, 500)), None);
        assert_eq!(bar.value, 0);
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
        let windows = [
            window((60, 60, 120, 120), true, 8),
            window((40, 40, 140, 140), true, 8),
            window((20, 20, 160, 160), true, 99),
        ];
        let pieces = gutter_pieces(&windows, Rect::from((0, 0, 180, 180)));
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
    fn hidden_and_custom_frames_keep_guest_presentation() {
        let windows = [
            window((30, 10, 90, 100), false, 0),
            window((60, 20, 120, 130), true, 128),
        ];
        assert!(frame_pieces(&windows, Rect::from((20, 0, 200, 200))).is_empty());
    }
}
