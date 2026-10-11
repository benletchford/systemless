//! Translation between painted host glyph boundaries and guest insertion points.

#[derive(Clone, Debug)]
pub(crate) struct TextPointerMap {
    pub identity: (u32, u64),
    pub text: String,
    pub scroll_x: f32,
    pub guest_bounds: (i16, i16, i16, i16),
    pub positions: Vec<(f32, i16)>,
}

impl TextPointerMap {
    pub fn horizontal(&self, host_x: f32) -> Option<i16> {
        for pair in self.positions.windows(2) {
            if host_x < (pair[0].0 + pair[1].0) / 2. {
                return Some(pair[0].1);
            }
        }
        self.positions.last().map(|position| position.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_trailing_dialog_space_does_not_become_a_caret() {
        let line = ClassicLine::plain(b"Pilot ", 0, 12);
        let layout = systemless::runner::DialogEditTextLayout {
            font: (0, 12), baseline: 12, line_height: 16,
            wrap: false, text_edit_geometry: true,
        };
        let geometry = dialog_field_geometry(&line, 5, &layout, (5, 6), true, true, 260, 20);
        assert_eq!(geometry.selection, None);
        assert_eq!(geometry.caret, None, "the guest selection remains non-empty after trimming");
        let collapsed = dialog_field_geometry(&line, 5, &layout, (6, 6), true, true, 260, 20);
        assert_eq!(collapsed.caret, Some((0, line.positions[5], 16, line.positions[5] + 1)));
        assert_eq!(dialog_field_geometry(&line, 5, &layout, (6, 6), false, true, 260, 20).caret, None);
    }

    #[test]
    fn shared_styled_glyph_plain_face_matches_gpui_binary_ink() {
        use std::collections::BTreeSet;
        for (font, size) in [(0, 12), (3, 9), (3, 12)] {
            for byte in [b'A', b'i', b'W', 0x8e, 0xa3] {
                let line = ClassicLine::plain(&[byte], font, size);
                let (advance, ink) = systemless::quickdraw::text::classic_styled_glyph(
                    font, size, byte, 0,
                );
                assert_eq!(line.positions, [0, advance]);
                let painted: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, width)| {
                    (x..x + width).map(move |px| (px as i16, y as i16))
                }).collect();
                assert_eq!(painted, ink.into_iter().collect());
            }
        }
    }

    #[test]
    fn styled_line_preserves_guest_advances_and_full_line_underline() {
        use std::collections::BTreeSet;
        for face in 0..128 {
            let bytes = b"A i\x8e";
            let line = ClassicLine::styled(bytes, 3, 12, face);
            let mut expected = BTreeSet::new();
            let mut positions = vec![0];
            let mut pen = 0;
            for &byte in bytes {
                let (advance, pixels) = systemless::quickdraw::text::classic_styled_glyph(3, 12, byte, face);
                expected.extend(pixels.into_iter().map(|(x, y)| (pen + i32::from(x), i32::from(y))));
                pen += advance;
                positions.push(pen);
            }
            if face & 4 != 0 {
                expected.extend((0..pen).map(|x| (x, 1)));
            }
            let painted: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, width)| {
                (x..x + width).map(move |px| (px, y))
            }).collect();
            assert_eq!(line.positions, positions, "face {face}");
            assert_eq!(painted, expected, "face {face}");
        }
    }

    #[test]
    fn file_name_abbreviation_preserves_guest_character_boundary() {
        assert_eq!(file_row_name("é£πAB", Some(3)), "é£π...");
        assert_eq!(file_row_name("é£π", Some(3)), "é£π");
        assert_eq!(file_row_name("é£πAB", None), "é£πAB");
    }

    #[test]
    fn menu_command_display_preserves_every_mac_roman_glyph_and_advance() {
        for byte in 0x21..=255 {
            let decoded = systemless::systems::macintosh::mac_roman::decode_mac_roman(&[byte]);
            let raw = ClassicLine::plain(&[byte], 0, 12);
            let displayed = ClassicLine::unicode(&decoded, 0, 12);
            assert_eq!(displayed.positions, raw.positions, "command {byte:#x}");
            assert_eq!(displayed.ink, raw.ink, "command {byte:#x}");
        }
    }

    #[test]
    fn unicode_labels_resolve_guest_mac_roman_and_symbol_glyphs() {
        let bytes = b"A\x8e\xa3\xb9\xa9";
        let text = systemless::systems::macintosh::mac_roman::decode_mac_roman(bytes);
        for (font, size) in [(0, 12), (3, 9), (3, 12)] {
            let raw = ClassicLine::plain(bytes, font, size);
            let decoded = ClassicLine::unicode(&text, font, size);
            assert_eq!(raw.positions, decoded.positions);
            assert_eq!(raw.ink, decoded.ink);
        }
        let symbol = ClassicLine::unicode("▸", 0, 12);
        let expected = systemless::quickdraw::text::get_unicode_glyph(0, 12, '▸');
        let advance = expected.map_or(6, |(glyph, _)| i32::from(glyph.advance));
        assert_eq!(symbol.positions, [0, advance]);
        if expected.is_some() {
            assert!(!symbol.ink.is_empty());
        }
    }

    #[test]
    fn host_glyph_midpoints_map_to_guest_positions_at_every_scale() {
        for scale in [0.75, 1., 1.5, 2.] {
            let map = TextPointerMap {
                identity: (1, 2),
                text: "iéW".into(),
                scroll_x: 0.,
                guest_bounds: (10, 20, 30, 100),
                positions: [0., 3., 12., 25.]
                    .into_iter()
                    .zip([21, 26, 35, 47])
                    .map(|(x, guest)| (100. + x * scale, guest))
                    .collect(),
            };
            for (x, expected) in [
                (-20., 21),
                (1., 21),
                (2., 26),
                (7., 26),
                (8., 35),
                (18., 35),
                (19., 47),
                (50., 47),
            ] {
                assert_eq!(map.horizontal(100. + x * scale), Some(expected));
            }
        }
    }
}

/// Shape, scroll and paint a Roman single-line guest field from one layout.
/// Scrolling changes presentation only; pointer positions still come from the guest.
pub(crate) fn single_line(
    folder: &systemless::runner::StandardFileNewFolderSnapshot,
    identity: (u32, u64),
    output: std::rc::Rc<std::cell::RefCell<Option<TextPointerMap>>>,
    scale: f32,
    foreground: gpui_kit::Hsla,
    selection_color: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let name = folder.name.clone();
    let bytes: Vec<u8> = name
        .chars()
        .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
        .collect::<Option<Vec<_>>>()
        .expect("Standard File names originate in Mac Roman guest buffers");
    let line = ClassicLine::plain(&bytes, 0, 12);
    let start = folder.selection.0.min(bytes.len());
    let end = folder.selection.1.min(bytes.len()).max(start);
    let visible = folder.visible_offset.min(bytes.len());
    let guest_positions = folder.insertion_positions.clone();
    let caret_visible = folder.caret_visible;
    let guest_bounds = folder.layout.name;
    canvas(
        move |bounds, _, _| {
            let advance = |index: usize| px(line.positions[index] as f32 * scale);
            let caret_width = px(scale);
            let available = (bounds.size.width - caret_width).max(px(0.));
            let target = advance(visible);
            let max_scroll = (advance(bytes.len()) - available).max(px(0.));
            let old = output
                .borrow()
                .as_ref()
                .filter(|map| map.identity == identity)
                .map_or(px(0.), |map| px(map.scroll_x));
            // Text 1993, TESelView: retain the existing origin while the target is
            // visible, scroll only far enough to reveal it, and clamp after deletion.
            let scroll = old
                .min(target)
                .max(target - available)
                .clamp(px(0.), max_scroll);
            let height = px(16. * scale);
            let origin = point(
                bounds.left() - scroll,
                bounds.top() + (bounds.size.height - height) / 2.,
            );
            let positions = (0..=bytes.len())
                .zip(&guest_positions)
                .map(|(index, guest)| (f32::from(origin.x + advance(index)), *guest))
                .collect();
            *output.borrow_mut() = Some(TextPointerMap {
                identity,
                text: name,
                guest_bounds,
                scroll_x: f32::from(scroll),
                positions,
            });
            (line, origin, height, caret_width)
        },
        move |_, (line, origin, height, caret_width), window, _| {
            let left = origin.x + px(line.positions[start] as f32 * scale);
            let right = origin.x + px(line.positions[end] as f32 * scale);
            if start != end {
                window.paint_quad(fill(
                    Bounds::new(point(left, origin.y), size(right - left, height)),
                    selection_color,
                ));
            }
            for &(x, y, width) in &line.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            origin.x + px(x as f32 * scale),
                            origin.y + px((y + 12) as f32 * scale),
                        ),
                        size(px(width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
            if start == end && caret_visible {
                window.paint_quad(fill(
                    Bounds::new(point(left, origin.y), size(caret_width, height)),
                    foreground,
                ));
            }
        },
    )
    .size_full()
}

/// Guest Font Manager glyphs, not host shaping. The same resolved strikes and
/// binary ink threshold serve QuickDraw on both CPU paths. Keep byte offsets:
/// decoded Unicode byte indices are not TextEdit insertion offsets.
#[derive(Clone, Debug)]
pub(crate) struct ClassicLine {
    pub positions: Vec<i32>,
    // Horizontal spans of binary ink, relative to the baseline.
    pub ink: Vec<(i32, i32, i32)>,
}

impl ClassicLine {
    pub fn plain(bytes: &[u8], font: i16, point_size: i16) -> Self {
        Self::from_glyphs(
            font,
            point_size,
            bytes.iter().map(|byte| {
                systemless::quickdraw::text::get_glyph(font, point_size, *byte as char)
            }),
        )
    }

    /// Exact guest strike and QuickDraw style synthesis. Underline spans the
    /// complete line, including spaces, just as the framebuffer painter does.
    pub fn styled(bytes: &[u8], font: i16, point_size: i16, face: u8) -> Self {
        let mut result = Self { positions: vec![0], ink: Vec::new() };
        let mut pen = 0;
        for &byte in bytes {
            let (advance, pixels) = systemless::quickdraw::text::classic_styled_glyph(
                font, point_size, byte, face,
            );
            for (x, y) in pixels {
                let (x, y) = (pen + i32::from(x), i32::from(y));
                if let Some(last) = result.ink.last_mut() {
                    if last.1 == y && last.0 + last.2 == x {
                        last.2 += 1;
                        continue;
                    }
                }
                result.ink.push((x, y, 1));
            }
            pen += advance;
            result.positions.push(pen);
        }
        if face & 4 != 0 && pen > 0 {
            for y in 1..=systemless::quickdraw::text::get_underline_thickness(font, point_size).max(1) {
                result.ink.push((0, i32::from(y), pen));
            }
        }
        result
    }

    /// HLE labels may include guest-drawn symbols outside Mac Roman, such as
    /// the 68k Standard File directory triangle. Resolve exactly as QuickDraw.
    pub fn unicode(text: &str, font: i16, point_size: i16) -> Self {
        Self::from_glyphs(
            font,
            point_size,
            text.chars()
                .map(|ch| systemless::quickdraw::text::get_unicode_glyph(font, point_size, ch)),
        )
    }

    fn from_glyphs(
        font: i16,
        point_size: i16,
        glyphs: impl Iterator<
            Item = Option<(&'static systemless::quickdraw::fonts::Glyph, &'static [u8])>,
        >,
    ) -> Self {
        use systemless::quickdraw::fonts;
        let (_, scale) = fonts::get_font_face_scaled(font, point_size);
        let scale = i32::from(scale);
        let mut result = Self {
            positions: vec![0],
            ink: Vec::new(),
        };
        let mut pen = 0;
        for resolved in glyphs {
            if let Some((glyph, data)) = resolved {
                let width = usize::from(glyph.width);
                for row in 0..usize::from(glyph.height) {
                    let mut column = 0;
                    while column < width {
                        let covered = |column| {
                            data[glyph.data_offset + row * width + column]
                                >= fonts::MONO_COVERAGE_THRESHOLD
                        };
                        if !covered(column) {
                            column += 1;
                            continue;
                        }
                        let start = column;
                        while column < width && covered(column) {
                            column += 1;
                        }
                        let x = pen + (i32::from(glyph.origin_x) + start as i32) * scale;
                        let y = (i32::from(glyph.origin_y) + row as i32) * scale;
                        for dy in 0..scale {
                            result
                                .ink
                                .push((x, y + dy, (column - start) as i32 * scale));
                        }
                    }
                }
                pen += i32::from(glyph.advance) * scale;
            } else {
                pen += 6 * scale;
            }
            result.positions.push(pen);
        }
        result
    }
}

/// Guest pen geometry differs between WDEF titles and TextEdit lines.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ClassicLineGeometry {
    Title,
    TextEdit,
}

impl ClassicLineGeometry {
    pub(crate) fn ink_x(self, x: i32) -> i32 {
        x + if matches!(self, Self::TextEdit) { 1 } else { 0 }
    }

    fn selection_x(self, x: i32, offset: usize) -> i32 {
        if matches!(self, Self::TextEdit) && offset == 0 { 0 } else { self.ink_x(x) }
    }

    pub(crate) fn caret_x(self, x: i32, offset: usize) -> i32 {
        self.ink_x(x) - i32::from(matches!(self, Self::TextEdit) && offset > 0)
    }
}

/// Paint a plain document TE line with guest baseline, advances, selection and
/// blink phase. Parent clipping and guest destRect supply scrolling/wrapping.
pub(crate) fn classic_line(
    line: ClassicLine,
    ascent: i16,
    line_height: i16,
    selection: (usize, usize),
    caret: Option<usize>,
    geometry: ClassicLineGeometry,
    scale: f32,
    foreground: gpui_kit::Hsla,
    selection_color: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let position =
                |offset: usize| line.positions[offset.min(line.positions.len() - 1)];
            let origin = bounds.origin;
            let height = px(f32::from(line_height) * scale);
            if selection.0 != selection.1 {
                let left = px(geometry.selection_x(position(selection.0), selection.0) as f32 * scale);
                let right = px(geometry.ink_x(position(selection.1)) as f32 * scale);
                window.paint_quad(fill(
                    Bounds::new(point(origin.x + left, origin.y), size(right - left, height)),
                    selection_color,
                ));
            }
            for &(x, y, width) in &line.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            origin.x + px(geometry.ink_x(x) as f32 * scale),
                            origin.y + px((y + i32::from(ascent)) as f32 * scale),
                        ),
                        size(px(width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
            if let Some(offset) = caret {
                window.paint_quad(fill(
                    Bounds::new(
                        point(origin.x + px(geometry.caret_x(position(offset), offset) as f32 * scale), origin.y),
                        size(px(scale), height),
                    ),
                    foreground,
                ));
            }
        },
    )
    .size_full()
}

/// Standard CDEF/dialog/Standard File buttons use the Roman system font.
/// Center in integer guest coordinates before applying presentation scale.
pub(crate) fn classic_button_label(
    label: &str,
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_control_label(label, true, scale, foreground)
}

pub(crate) fn classic_choice_label(
    label: &str,
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_control_label(label, false, scale, foreground)
}

fn classic_control_label(
    label: &str,
    centered: bool,
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_label_canvas(ClassicLine::unicode(label, 0, 12), centered, scale, foreground)
}

pub(crate) fn classic_menu_label(
    label: &str, face: u8, scale: f32, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let bytes: Vec<_> = label.chars().map(|ch|
        systemless::systems::macintosh::mac_roman::encode_mac_roman_char(ch).unwrap_or(b'?')).collect();
    let mut line = ClassicLine::styled(&bytes, 0, 12, face);
    // Outline/italic ink can extend outside its advance. Include that ink in
    // the label bounds instead of clipping it to the unstylized width.
    let left = line.ink.iter().map(|&(x, _, _)| x).min().unwrap_or(0).min(0);
    let right = line.ink.iter().map(|&(x, _, width)| x + width).max().unwrap_or(0)
        .max(line.positions.last().copied().unwrap_or(0));
    for ink in &mut line.ink { ink.0 -= left; }
    let width = (right - left).max(1);
    div().w(px(width as f32 * scale)).h(px(18. * scale)).flex_shrink_0()
        .overflow_hidden().child(classic_label_canvas(line, false, scale, foreground))
}

/// Symbols use the same Unicode-to-guest-glyph resolver as Menu Manager.
pub(crate) fn classic_menu_symbol(
    label: &str, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let line = ClassicLine::unicode(label, 0, 12);
    let width = line.positions.last().copied().unwrap_or(0).max(1);
    div().w(px(width as f32)).h(px(18.)).flex_shrink_0()
        .overflow_hidden().child(classic_label_canvas(line, false, 1., foreground))
}

pub(crate) fn classic_popup_control_label(
    label: &str, guest_font: systemless::menu_model::GuestMenuFont, title: bool, text_inset: i16,
    scale: f32, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let label = label.to_owned();
    let metrics = systemless::quickdraw::text::get_font_metrics(guest_font.family, guest_font.point_size());
    canvas(move |bounds, _, _| bounds, move |_, bounds, window, _| {
        let width = (f32::from(bounds.size.width) / scale).round() as i32;
        let height = (f32::from(bounds.size.height) / scale).round() as i32;
        let display = if title { label.clone() } else {
            let chars: Vec<_> = label.chars().collect();
            systemless::menu_model::popup_display_text(&chars, &['.', '.', '.'],
                (width - i32::from(text_inset)).clamp(0, i32::from(i16::MAX)) as i16, |chars| {
                    let text: String = chars.iter().collect();
                    ClassicLine::unicode(&text, guest_font.family, guest_font.point_size())
                        .positions.last().copied().unwrap_or(0).clamp(0, i32::from(i16::MAX)) as i16
                }).into_iter().collect()
        };
        let line = ClassicLine::unicode(&display, guest_font.family, guest_font.point_size());
        let x = if title { (width - 6 - line.positions.last().copied().unwrap_or(0)).max(0) } else { i32::from(text_inset) };
        let baseline = (height - i32::from(metrics.ascent) - i32::from(metrics.descent)) / 2 + i32::from(metrics.ascent) - i32::from(!title);
        for &(ink_x, ink_y, ink_width) in &line.ink {
            window.paint_quad(fill(Bounds::new(
                point(bounds.left() + px((x + ink_x) as f32 * scale),
                    bounds.top() + px((baseline + ink_y) as f32 * scale)),
                size(px(ink_width as f32 * scale), px(scale))), foreground));
        }
    }).size_full()
}

fn classic_label_canvas(
    line: ClassicLine, centered: bool, scale: f32, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_label_canvas_with_font(line, centered, scale, foreground, (0, 12), None)
}

fn classic_label_canvas_with_font(
    line: ClassicLine, centered: bool, scale: f32, foreground: gpui_kit::Hsla,
    guest_font: (i16, i16), right_inset: Option<i32>,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let metrics = systemless::quickdraw::text::get_font_metrics(guest_font.0, guest_font.1);
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let width = (f32::from(bounds.size.width) / scale).round() as i32;
            let height = (f32::from(bounds.size.height) / scale).round() as i32;
            let x = if let Some(inset) = right_inset {
                (width - inset - line.positions.last().copied().unwrap_or(0)).max(0)
            } else if centered {
                (width - line.positions.last().copied().unwrap_or(0)) / 2
            } else {
                0
            };
            let baseline = (height - i32::from(metrics.ascent) - i32::from(metrics.descent)) / 2
                + i32::from(metrics.ascent);
            for &(ink_x, ink_y, ink_width) in &line.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px((x + ink_x) as f32 * scale),
                            bounds.top() + px((baseline + ink_y) as f32 * scale),
                        ),
                        size(px(ink_width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
        },
    )
    .size_full()
}

/// Paint a standard popup row at guest MDEF anchors and the owner-port font.
/// The caller owns row clipping and tracking; no host shaping or ellipsis occurs.
pub(crate) fn classic_popup_row(
    popup: &systemless::menu_model::GuestPopupSnapshot,
    item: &systemless::menu_model::GuestMenuItem,
    height: i16, scale: f32, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let font = popup.font;
    let (mark_x, text_x, command_x, baseline) = popup.text_anchors(0, height);
    let bytes: Vec<_> = item.text.chars().map(|ch|
        systemless::systems::macintosh::mac_roman::encode_mac_roman_char(ch).unwrap_or(b'?')).collect();
    let mut lines = vec![(text_x, ClassicLine::styled(&bytes, font.family, font.point_size(), item.style))];
    if item.mark != 0 && item.submenu_id.is_none() {
        lines.push((mark_x, ClassicLine::plain(&[item.mark], font.family, font.point_size())));
    }
    if let Some(key) = item.key_equivalent {
        lines.push((command_x, ClassicLine::unicode(&format!("⌘{key}"), font.family, font.point_size())));
    }
    let hierarchy = item.submenu_id.is_some().then(|| {
        let x = popup.bounds.3.saturating_sub(popup.bounds.1).saturating_sub(12);
        (x, systemless::menu_model::standard_hierarchy_indicator_pixels())
    });
    canvas(move |bounds, _, _| bounds, move |_, bounds, window, _| {
        if let Some((left, pixels)) = &hierarchy {
            for &(x, y) in pixels {
                window.paint_quad(fill(Bounds::new(
                    point(bounds.left() + px((i32::from(*left) + i32::from(x)) as f32 * scale),
                        bounds.top() + px((i32::from(height / 2) + i32::from(y)) as f32 * scale)),
                    size(px(scale), px(scale))), foreground));
            }
        }
        for (x, line) in &lines {
            for &(ink_x, ink_y, width) in &line.ink {
                window.paint_quad(fill(Bounds::new(
                    point(bounds.left() + px((i32::from(*x) + ink_x) as f32 * scale),
                        bounds.top() + px((i32::from(baseline) + ink_y) as f32 * scale)),
                    size(px(width as f32 * scale), px(scale))), foreground));
            }
        }
    }).size_full()
}

/// Standard File prompt layout uses the same word/hard-break algorithm as
/// guest dialog drawing. Parent bounds own the clip, never host text shaping.
pub(crate) fn classic_file_prompt(
    text: &str,
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_wrapped_text(text, (0, 12), 0, 0, (0, 12, 16), false, scale, foreground)
}

pub(crate) fn classic_directory_label(
    text: &str,
    font: (i16, i16, u8),
    layout: (i16, i16, i16),
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_wrapped_text(
        text,
        (font.0, font.1),
        font.2,
        0,
        layout,
        layout.0 != 0,
        scale,
        foreground,
    )
}

#[derive(Debug, PartialEq)]
pub(crate) struct DialogFieldGeometry {
    pub selection: Option<(i32, i32, i32, i32)>,
    pub caret: Option<(i32, i32, i32, i32)>,
}

pub(crate) fn dialog_field_geometry(
    line: &ClassicLine, visible_length: usize,
    layout: &systemless::runner::DialogEditTextLayout,
    selection: (usize, usize), focused: bool, caret_visible: bool,
    width: i32, height: i32,
) -> DialogFieldGeometry {
    let start = selection.0.min(visible_length);
    let end = selection.1.min(visible_length).max(start);
    let x = |offset: usize| 1 + line.positions[offset];
    let highlight = if focused && start < end {
        let left = if start == 0 { 0 } else { x(start) };
        let right = if !layout.text_edit_geometry && end == visible_length { width } else { x(end) };
        Some((0, left, i32::from(layout.line_height).min(height), right))
    } else { None };
    // Trimming may collapse the visible range without collapsing the guest
    // selection. A selected trailing space must never acquire a caret.
    let caret = if focused && selection.0 == selection.1 && caret_visible {
        let caret_x = x(start) - i32::from(layout.text_edit_geometry && start != 0);
        let (top, bottom) = if layout.text_edit_geometry {
            (0, i32::from(layout.line_height).min(height))
        } else { (2, height - 1) };
        let limit = width - i32::from(!layout.text_edit_geometry);
        (caret_x >= 0 && caret_x < limit && top < bottom)
            .then_some((top, caret_x, bottom, caret_x + 1))
    } else { None };
    DialogFieldGeometry { selection: highlight, caret }
}

pub(crate) fn classic_dialog_edit_text(
    text: &str, layout: &systemless::runner::DialogEditTextLayout,
    selection: (usize, usize), focused: bool, caret_visible: bool,
    scale: f32, foreground: gpui_kit::Hsla, selection_color: gpui_kit::Hsla,
) -> gpui_kit::AnyElement {
    use gpui_kit::{prelude::*, *};
    if layout.wrap {
        return classic_wrapped_text(text, layout.font, 0, 0, (1, layout.baseline, layout.line_height),
            false, scale, foreground).into_any_element();
    }
    let line = ClassicLine::unicode(text, layout.font.0, layout.font.1);
    let length = if layout.text_edit_geometry {
        text.trim_end_matches([' ', '\r', '\n']).chars().count()
    } else { line.positions.len() - 1 };
    let layout = layout.clone();
    canvas(move |bounds, _, _| bounds, move |_, bounds, window, _| {
        let geometry = dialog_field_geometry(&line, length, &layout, selection, focused, caret_visible,
            (f32::from(bounds.size.width) / scale).round() as i32,
            (f32::from(bounds.size.height) / scale).round() as i32);
        let rect = |(top, left, bottom, right): (i32, i32, i32, i32)| Bounds::new(
            point(bounds.left() + px(left as f32 * scale), bounds.top() + px(top as f32 * scale)),
            size(px((right - left).max(0) as f32 * scale), px((bottom - top).max(0) as f32 * scale)));
        if let Some(selection) = geometry.selection {
            window.paint_quad(fill(rect(selection), selection_color));
        }
        for &(x, y, width) in &line.ink {
            window.paint_quad(fill(Bounds::new(
                point(bounds.left() + px((x + 1) as f32 * scale),
                    bounds.top() + px((y + i32::from(layout.baseline)) as f32 * scale)),
                size(px(width as f32 * scale), px(scale))), foreground));
        }
        if let Some(caret) = geometry.caret {
            window.paint_quad(fill(rect(caret), foreground));
        }
    }).size_full().into_any_element()
}

pub(crate) fn classic_dialog_static_text(
    text: &str, layout: &systemless::runner::DialogStaticTextLayout,
    scale: f32, foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    classic_wrapped_text(text, layout.font, layout.face, layout.wrap_advance_extra, (layout.origin.0, layout.origin.1, layout.line_height),
        layout.inclusive_bottom, scale, foreground)
}

fn classic_wrapped_text(
    text: &str,
    guest_font: (i16, i16),
    face: u8,
    wrap_advance_extra: i16,
    layout: (i16, i16, i16),
    inclusive_bottom: bool,
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let bytes = text
        .chars()
        .map(|ch| {
            systemless::systems::macintosh::mac_roman::encode_mac_roman_char(ch).unwrap_or(b'?')
        })
        .collect::<Vec<_>>();
    let advances = ClassicLine::plain(&bytes, guest_font.0, guest_font.1).positions;
    canvas(
        move |bounds, _, _| {
            let width = (f32::from(bounds.size.width) / scale - f32::from(layout.0))
                .round()
                .clamp(1., i16::MAX as f32) as i16;
            let lines =
                systemless::quickdraw::text::wrap_classic_text(&bytes, width, |index, _| {
                    (advances[index + 1] - advances[index]) as i16 + wrap_advance_extra
                })
                .into_iter()
                .map(|line| {
                    ClassicLine::styled(
                        &bytes[line.start..line.visible_end],
                        guest_font.0,
                        guest_font.1,
                        face,
                    )
                })
                .collect::<Vec<_>>();
            (bounds, lines)
        },
        move |_, (bounds, lines), window, _| {
            for (index, line) in lines.iter().enumerate() {
                let baseline = i32::from(layout.1) + index as i32 * i32::from(layout.2);
                if baseline as f32 * scale > f32::from(bounds.size.height)
                    || (!inclusive_bottom
                        && baseline as f32 * scale == f32::from(bounds.size.height))
                {
                    break;
                }
                for &(x, y, width) in &line.ink {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                bounds.left() + px((x + i32::from(layout.0)) as f32 * scale),
                                bounds.top() + px((baseline + y) as f32 * scale),
                            ),
                            size(px(width as f32 * scale), px(scale)),
                        ),
                        foreground,
                    ));
                }
            }
        },
    )
    .size_full()
}

/// Paint one guest-owned Standard File list row; the row clip owns overflow.
pub(crate) fn classic_file_row(
    text: &str,
    origin: (i16, i16),
    scale: f32,
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let line = ClassicLine::unicode(text, 0, 12);
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            for &(x, y, width) in &line.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px((x + i32::from(origin.0)) as f32 * scale),
                            bounds.top() + px((y + i32::from(origin.1)) as f32 * scale),
                        ),
                        size(px(width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
        },
    )
    .size_full()
}

/// Preserve the guest painter's display abbreviation without changing the
/// canonical filename used by guest selection and file operations.
pub(crate) fn file_row_name(name: &str, limit: Option<usize>) -> String {
    match limit {
        Some(limit) if name.chars().count() > limit => {
            format!("{}...", name.chars().take(limit).collect::<String>())
        }
        _ => name.to_owned(),
    }
}

/// Guest glyph positions and selection geometry for the Save filename field.
pub(crate) fn classic_save_name(
    name: &str,
    selection: (usize, usize),
    focused: bool,
    caret_visible: bool,
    layout: &systemless::runner::StandardFileNameTextLayout,
    scale: f32,
    foreground: gpui_kit::Hsla,
    selection_color: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let bytes: Vec<_> = name
        .chars()
        .map(|ch| {
            systemless::systems::macintosh::mac_roman::encode_mac_roman_char(ch).unwrap_or(b'?')
        })
        .collect();
    let layout = layout.clone();
    let line = ClassicLine::plain(&bytes, layout.font.0, layout.font.1);
    let start = selection.0.min(bytes.len());
    let end = selection.1.max(start).min(bytes.len());
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let width = f32::from(bounds.size.width) / scale;
            let position = |index: usize| f32::from(layout.origin.0) + line.positions[index] as f32;
            if focused && start < end {
                let left = if layout.selection_to_edge && start == 0 {
                    0.
                } else {
                    position(start)
                };
                let right = if layout.selection_to_edge && end == bytes.len() {
                    width
                } else {
                    position(end)
                };
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(left * scale),
                            bounds.top() + px(f32::from(layout.selection_top) * scale),
                        ),
                        size(
                            px((right - left).max(0.) * scale),
                            px(f32::from(layout.selection_height) * scale),
                        ),
                    ),
                    selection_color,
                ));
            }
            let visible_end = if layout.wraps {
                systemless::quickdraw::text::wrap_classic_text(
                    &bytes,
                    width.max(1.) as i16,
                    |index, _| (line.positions[index + 1] - line.positions[index]) as i16,
                )
                .first()
                .map_or(0, |line| line.visible_end)
            } else {
                bytes.len()
            };
            let painted = ClassicLine::plain(&bytes[..visible_end], layout.font.0, layout.font.1);
            for &(x, y, width) in &painted.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px((x + i32::from(layout.origin.0)) as f32 * scale),
                            bounds.top() + px((y + i32::from(layout.origin.1)) as f32 * scale),
                        ),
                        size(px(width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
            if focused && caret_visible && start == end && position(start) >= 0. && position(start) < width - 1. {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(position(start) * scale),
                            bounds.top() + px(2. * scale),
                        ),
                        size(px(scale), (bounds.size.height - px(3. * scale)).max(px(0.))),
                    ),
                    foreground,
                ));
            }
        },
    )
    .size_full()
}

/// Use the guest Menu Manager raster, never a host-font chevron.
pub(crate) fn classic_menu_hierarchy_indicator(
    foreground: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let pixels = systemless::menu_model::standard_hierarchy_indicator_pixels();
    canvas(move |bounds, _, _| bounds, move |_, bounds, window, _| {
        let middle = bounds.top() + bounds.size.height / 2.;
        for &(x, y) in &pixels {
            window.paint_quad(fill(Bounds::new(
                point(bounds.left() + px(f32::from(x)), middle + px(f32::from(y))),
                size(px(1.), px(1.))), foreground));
        }
    }).w(px(6.)).h(px(18.)).flex_shrink_0()
}
