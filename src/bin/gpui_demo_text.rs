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
        use systemless::quickdraw::{fonts, text::get_glyph};
        let (_, scale) = fonts::get_font_face_scaled(font, point_size);
        let scale = i32::from(scale);
        let mut result = Self {
            positions: vec![0],
            ink: Vec::new(),
        };
        let mut pen = 0;
        for byte in bytes {
            if let Some((glyph, data)) = get_glyph(font, point_size, *byte as char) {
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

/// Paint a plain document TE line with guest baseline, advances, selection and
/// blink phase. Parent clipping and guest destRect supply scrolling/wrapping.
pub(crate) fn classic_line(
    line: ClassicLine,
    ascent: i16,
    line_height: i16,
    selection: (usize, usize),
    caret: bool,
    scale: f32,
    foreground: gpui_kit::Hsla,
    selection_color: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let position =
                |offset: usize| line.positions[offset.min(line.positions.len() - 1)] as f32 * scale;
            let origin = bounds.origin;
            let height = px(f32::from(line_height) * scale);
            if selection.0 != selection.1 {
                let left = px(position(selection.0));
                let right = px(position(selection.1));
                window.paint_quad(fill(
                    Bounds::new(point(origin.x + left, origin.y), size(right - left, height)),
                    selection_color,
                ));
            }
            for &(x, y, width) in &line.ink {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            origin.x + px(x as f32 * scale),
                            origin.y + px((y + i32::from(ascent)) as f32 * scale),
                        ),
                        size(px(width as f32 * scale), px(scale)),
                    ),
                    foreground,
                ));
            }
            if caret {
                window.paint_quad(fill(
                    Bounds::new(
                        point(origin.x + px(position(selection.0)), origin.y),
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
    use gpui_kit::{prelude::*, *};
    let bytes = label
        .chars()
        .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
        .collect::<Option<Vec<_>>>()
        .expect("guest button labels originate in Mac Roman buffers");
    let line = ClassicLine::plain(&bytes, 0, 12);
    let metrics = systemless::quickdraw::text::get_font_metrics(0, 12);
    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let width = (f32::from(bounds.size.width) / scale).round() as i32;
            let height = (f32::from(bounds.size.height) / scale).round() as i32;
            let x = (width - line.positions.last().copied().unwrap_or(0)) / 2;
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
