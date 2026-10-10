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
    #[test]
    fn smooth_label_sources_preserve_guest_advances_and_have_fractional_coverage() {
        for (font, size) in [(0, 12), (1, 9), (3, 12)] {
            // Guest bytes include accented letters and symbols whose Unicode
            // code points differ from their Macintosh Roman byte values.
            let bytes = b"Systemless \x8e\xae\xbe";
            let line = super::ClassicLine::plain(bytes, font, size);
            assert_eq!(line.smooth_sources.len() + 1, line.positions.len());
            for raster in [1, 2, 3, 4] {
                let mut fractional = false;
                for (index, &(pen, glyph, data)) in line.smooth_sources.iter().enumerate() {
                    let mask = systemless::quickdraw::text::smooth_resolved_glyph(glyph, data, raster)
                        .expect("resolved bundled outline");
                    assert_eq!(pen, line.positions[index]);
                    assert_eq!(mask.guest_advance, line.positions[index + 1] - pen);
                    assert_eq!(mask.raster_scale, raster);
                    assert_eq!(mask.pixels.len(), (mask.width * mask.height) as usize);
                    fractional |= mask.pixels.iter().any(|&coverage| coverage > 0 && coverage < 255);
                }
                assert!(fractional, "outline edges must be antialiased: {font}/{size}/{raster}");
            }
            let decoded = systemless::systems::macintosh::mac_roman::decode_mac_roman(bytes);
            let unicode = super::ClassicLine::unicode(&decoded, font, size);
            assert_eq!(unicode.positions, line.positions);
            assert_eq!(unicode.ink, line.ink);
            assert_eq!(super::ClassicLine::styled(bytes, font, size, 0).positions, line.positions);
        }
        assert!(systemless::quickdraw::text::smooth_unicode_glyph(0, 12, 'A', 0).is_none());
        assert!(systemless::quickdraw::text::smooth_unicode_glyph(0, 12, 'A', 9).is_none());
    }

    #[test]
    fn list_cell_qualification_requires_complete_native_evidence() {
        use super::{ClassicListCellLayout, ClassicListCellPaintPlan};
        let layout = ClassicListCellLayout { font: 3, size: 12, left: 3, baseline: 10,
            clip: (0, 0, 12, 20), stop_before: None, char_extra: 0 };
        let native = vec![255; 24 * 40 * 4];
        let qualify = |intact, global, pixels: &[u8]| ClassicListCellPaintPlan::qualify(
            layout, b"", intact, [0; 3], [255; 3], global, pixels, 40, 24);
        assert!(qualify(true, (2, 3, 14, 23), &native).is_some());
        assert!(qualify(false, (2, 3, 14, 23), &native).is_none());
        assert!(qualify(true, (-1, 3, 11, 23), &native).is_none());
        assert!(qualify(true, (2, 3, 15, 23), &native).is_none());
        assert!(qualify(true, (2, 3, 14, 23), &native[..native.len() - 1]).is_none());
        let mut modified = native.clone();
        modified[(2 * 40 + 3) * 4] = 0;
        assert!(qualify(true, (2, 3, 14, 23), &modified).is_none());
        modified = native.clone();
        modified[0] = 0;
        assert!(qualify(true, (2, 3, 14, 23), &modified).is_some());
    }

    #[test]
    fn list_bitmap_recipe_preserves_guest_crop_and_stopping_boundary() {
        use super::ClassicListCellLayout;
        let full = ClassicListCellLayout { font: 3, size: 12, left: 3, baseline: 18,
            clip: (0, 0, 40, 100), stop_before: None, char_extra: 0 };
        let bytes = b"A i\x8e";
        let ink = full.pixels(bytes).unwrap();
        assert!(!ink.is_empty());
        let clipped = ClassicListCellLayout { clip: (10, 5, 19, 15), ..full };
        assert_eq!(clipped.pixels(bytes).unwrap(), ink.iter().copied()
            .filter(|&(x, y)| x >= 5 && x < 15 && y >= 10 && y < 19).collect());
        let stopped = ClassicListCellLayout { stop_before: Some(4), ..full };
        assert_eq!(stopped.pixels(bytes), full.pixels(b"A"));
        assert!(ClassicListCellLayout { char_extra: 1, ..full }.pixels(bytes).is_none());
        assert!(ClassicListCellLayout { clip: (0, 0, 0, 100), ..full }.pixels(bytes).is_none());
    }

    use super::*;

    #[test]
    fn solid_styled_caret_overwrites_ink_without_inverting_or_selecting() {
        use systemless::runner::{TextEditInkSnapshot, TextEditLineGeometry};
        let glyphs = ClassicLine::plain(b"W", 3, 12);
        let ink = TextEditInkSnapshot { pixel: 1, rgb: [20, 40, 60], inverted_rgb: [235, 215, 195] };
        let mut line = StyledTextEditLine {
            geometry: TextEditLineGeometry { top: 0, left: 0, height: 16, ascent: 12 },
            baseline: 12, selection: None,
            runs: vec![StyledTextEditRun { bytes: 0..1, left: 0,
                measured_positions: vec![0, 10], glyphs, ink: ink.clone() }],
        };
        let original = line.pixels().unwrap();
        let &(x, y) = original.keys().next().unwrap();
        let caret = TextEditInkSnapshot { pixel: 7, rgb: [100, 80, 160], inverted_rgb: [155, 175, 95] };
        let painted = line.pixels_with_solid_caret(Some((y, x, y + 2, x + 1)), &caret).unwrap();
        assert_eq!(painted[&(x, y)], caret.rgb);
        assert_eq!(painted[&(x, y + 1)], caret.rgb);
        for (&point, &rgb) in &original {
            if point != (x, y) && point != (x, y + 1) { assert_eq!(painted[&point], rgb); }
        }
        assert_eq!(line.pixels_with_solid_caret(None, &caret), Some(original));
        assert!(line.pixels_with_solid_caret(Some((y, x, y, x + 1)), &caret).is_none());
        line.selection = Some((0, 0, 16, 10));
        assert!(line.pixels_with_solid_caret(Some((y, x, y + 2, x + 1)), &caret).is_none());
    }

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
    fn classic_textedit_run_keeps_per_character_underline_and_guest_positions() {
        use std::collections::BTreeSet;
        let bytes = b"g \x8e";
        let positions = vec![0, 3, 8, 13];
        for size in [9, 10, 12, 14, 24] {
            for face in 0..128 {
                let (line, paint_advance) = ClassicLine::classic_textedit_run_with_paint_advance(bytes, 3, size, face, positions.clone()).unwrap();
                assert_eq!(line.positions, positions);
                let painted: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, width)| {
                    (x..x + width).map(move |px| (px, y))
                }).collect();
                let mut expected = BTreeSet::new();
                let mut pen = 0;
                for &byte in bytes {
                    let (advance, pixels) = systemless::quickdraw::text::classic_textedit_glyph_ink(3, size, byte, face).unwrap();
                    expected.extend(pixels.into_iter().map(|(x, y)| (pen + i32::from(x), i32::from(y))));
                    pen += advance;
                }
                assert_eq!(painted, expected);
                assert_eq!(paint_advance, pen);
                if face != 0 { assert!(line.smooth_sources.is_empty()); }
                if face == 0 && size == 12 {
                    assert_eq!(line.smooth_sources.len(), bytes.len());
                    assert_eq!(line.smooth_sources[0].0, 0);
                    assert_ne!(line.smooth_sources[1].0, positions[1], "native paint pens must not be replaced by insertion metrics");
                }
            }
        }
        assert!(ClassicLine::classic_textedit_run(bytes, 3, 12, 0, vec![0, 1]).is_none());
    }

    #[test]
    fn ppc_run_preserves_guest_insertion_positions_and_binary_ink() {
        use std::collections::BTreeSet;
        let bytes = b"A i\x8e";
        // Deliberately differ from glyph-mask extents: caret ownership comes
        // from guest TextEdit measurement, not from painting or host shaping.
        let positions = vec![0, 3, 5, 9, 17];
        for size in [9, 10, 12, 14, 24] {
            for face in 0..128 {
                let (line, paint_advance) = ClassicLine::ppc_styled_run_with_paint_advance(bytes, 3, size, face, positions.clone()).unwrap();
                assert_eq!(line.positions, positions);
                let painted: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, width)| {
                    (x..x + width).map(move |px| (px, y))
                }).collect();
                let (expected_advance, expected) = systemless::quickdraw::text::ppc_styled_run_ink(3, size, face, bytes);
                assert_eq!(paint_advance, i32::from(expected_advance));
                assert_eq!(painted, expected.into_iter().collect());
                if face != 0 { assert!(line.smooth_sources.is_empty()); }
                if face == 0 && size == 12 {
                    assert_eq!(line.smooth_sources.len(), bytes.len());
                    let mut native_pen = 0;
                    for (source, byte) in line.smooth_sources.iter().zip(bytes) {
                        let (glyph, _) = systemless::quickdraw::text::get_glyph(3, size, *byte as char).unwrap();
                        assert_eq!(source.0, native_pen);
                        assert!(std::ptr::eq(source.1, glyph));
                        native_pen += i32::from(glyph.advance);
                    }
                }
            }
        }
        assert!(ClassicLine::ppc_styled_run(bytes, 3, 12, 0, vec![0, 1]).is_none());
        assert!(ClassicLine::ppc_styled_run(bytes, 3, 12, 0, vec![1, 2, 3, 4, 5]).is_none());
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
            if !paint_smooth_label(&line, origin.x, origin.y + px(12.0 * scale),
                scale, foreground, window) {
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
#[derive(Clone)]
pub(crate) struct ClassicLine {
    pub positions: Vec<i32>,
    // Horizontal spans of binary ink, relative to the baseline.
    pub ink: Vec<(i32, i32, i32)>,
    smooth_sources: Vec<(i32, &'static systemless::quickdraw::fonts::Glyph, &'static [u8])>,
}

impl std::fmt::Debug for ClassicLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ClassicLine").field("positions", &self.positions)
            .field("ink", &self.ink).field("smooth_sources", &self.smooth_sources.len()).finish()
    }
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
        let mut result = Self { positions: vec![0], ink: Vec::new(), smooth_sources: Vec::new() };
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
        if face == 0 {
            result.smooth_sources = Self::plain(bytes, font, point_size).smooth_sources;
        }
        if face & 4 != 0 && pen > 0 {
            for y in 1..=systemless::quickdraw::text::get_underline_thickness(font, point_size).max(1) {
                result.ink.push((0, i32::from(y), pen));
            }
        }
        result
    }

    /// Classic TextEdit draws each character separately, including its
    /// descender-aware underline. Keep this distinct from DrawString's line
    /// underline and PPC's ratio/run policy. Zero CharExtra/SpaceExtra, srcOr.
    pub fn classic_textedit_run(
        bytes: &[u8], font: i16, point_size: i16, face: u8, positions: Vec<i32>,
    ) -> Option<Self> {
        Self::classic_textedit_run_with_paint_advance(bytes, font, point_size, face, positions)
            .map(|(line, _)| line)
    }

    /// Return the native paint advance separately from measured insertion
    /// positions so the next style run starts where guest drawing leaves it.
    pub fn classic_textedit_run_with_paint_advance(
        bytes: &[u8], font: i16, point_size: i16, face: u8, positions: Vec<i32>,
    ) -> Option<(Self, i32)> {
        if positions.len() != bytes.len() + 1 || positions.first() != Some(&0) {
            return None;
        }
        let mut pixels = Vec::new();
        let mut smooth_sources = Vec::new();
        let (_, strike_scale) = systemless::quickdraw::fonts::get_font_face_scaled(font, point_size);
        let mut pen: i32 = 0;
        for &byte in bytes {
            if face == 0 && strike_scale == 1 {
                if let Some((glyph, data)) = systemless::quickdraw::text::get_glyph(font, point_size, byte as char) {
                    smooth_sources.push((pen, glyph, data));
                }
            }
            let (advance, ink) = systemless::quickdraw::text::classic_textedit_glyph_ink(font, point_size, byte, face)?;
            pixels.extend(ink.into_iter().map(|(x, y)| (pen + i32::from(x), i32::from(y))));
            pen = pen.saturating_add(advance);
        }
        pixels.sort_unstable_by_key(|&(x, y)| (y, x));
        pixels.dedup();
        let mut result = Self { positions, ink: Vec::new(), smooth_sources };
        for (x, y) in pixels {
            if let Some(last) = result.ink.last_mut() {
                if last.1 == y && last.0 + last.2 == x {
                    last.2 += 1;
                    continue;
                }
            }
            result.ink.push((x, y, 1));
        }
        Some((result, pen))
    }

    /// PPC styled run ink with insertion positions supplied by guest TextEdit
    /// measurement. Glyph paint ratios and insertion widths are distinct guest
    /// policies; preserve both rather than measuring these masks on the host.
    /// This recipe requires zero CharExtra; port policy stays with the caller.
    pub fn ppc_styled_run(
        bytes: &[u8], font: i16, point_size: i16, face: u8, positions: Vec<i32>,
    ) -> Option<Self> {
        Self::ppc_styled_run_with_paint_advance(bytes, font, point_size, face, positions)
            .map(|(line, _)| line)
    }

    /// Return the native paint advance separately from measured insertion
    /// positions so the next style run starts where guest drawing leaves it.
    pub fn ppc_styled_run_with_paint_advance(
        bytes: &[u8], font: i16, point_size: i16, face: u8, positions: Vec<i32>,
    ) -> Option<(Self, i32)> {
        if positions.len() != bytes.len() + 1 || positions.first() != Some(&0) {
            return None;
        }
        let (paint_advance, mut pixels) = systemless::quickdraw::text::ppc_styled_run_ink(font, point_size, face, bytes);
        let mut smooth_sources = Vec::new();
        let (strike, numerator, denominator) = systemless::quickdraw::fonts::get_font_face_scale_ratio(font, point_size);
        if face == 0 && numerator == denominator {
            let mut pen = 0;
            for &byte in bytes {
                if let Some((glyph, data)) = systemless::quickdraw::text::get_glyph(font, strike.size, byte as char) {
                    smooth_sources.push((pen, glyph, data));
                    pen += i32::from(glyph.advance);
                } else { pen += 6; }
            }
        }
        pixels.sort_unstable_by_key(|&(x, y)| (y, x));
        let mut result = Self { positions, ink: Vec::new(), smooth_sources };
        for (x, y) in pixels {
            if let Some(last) = result.ink.last_mut() {
                if last.1 == y && last.0 + last.2 == x {
                    last.2 += 1;
                    continue;
                }
            }
            result.ink.push((x, y, 1));
        }
        Some((result, i32::from(paint_advance)))
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
            smooth_sources: Vec::new(),
        };
        let mut pen = 0;
        for resolved in glyphs {
            if let Some((glyph, data)) = resolved {
                if scale == 1 { result.smooth_sources.push((pen, glyph, data)); }
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
            #[cfg(feature = "gpui-demo-test")]
            if matches!(geometry, ClassicLineGeometry::Title)
                && std::env::var_os("SYSTEMLESS_GPUI_TEXT_TRACE").is_some()
            {
                eprintln!("GPUI title paint: bounds={bounds:?}, mask={:?}, foreground={foreground:?}, scale={scale}, ascent={ascent}, height={line_height}, ink_runs={}", window.content_mask(), line.ink.len());
            }
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
            // Keep guest bitmap titles in one GPUI path. Small quad batches
            // can disappear in fractional-scale composed frames; device-edge
            // snapping below preserves their original bitmap rasterization.
            let smooth_ink = paint_smooth_label(&line, origin.x + px(geometry.ink_x(0) as f32 * scale),
                origin.y + px(f32::from(ascent) * scale), scale, foreground, window);
            if !smooth_ink && matches!(geometry, ClassicLineGeometry::Title) {
                let mut path = PathBuilder::fill();
                let device_scale = window.scale_factor();
                // Match GPUI quad snapping: nearest device pixel, half ties
                // toward zero. Keep classic bitmap edges off subpixel paths.
                let snap = |value: Pixels| {
                    let device = f32::from(value) * device_scale;
                    px((device.abs() - 0.5).ceil().copysign(device) / device_scale)
                };
                for &(x, y, width) in &line.ink {
                    let raw_left = origin.x + px(geometry.ink_x(x) as f32 * scale);
                    let raw_top = origin.y + px((y + i32::from(ascent)) as f32 * scale);
                    let left = snap(raw_left);
                    let top = snap(raw_top);
                    let right = snap(raw_left + px(width as f32 * scale));
                    let bottom = snap(raw_top + px(scale));
                    if right <= left || bottom <= top { continue; }
                    path.move_to(point(left, top));
                    path.line_to(point(right, top));
                    path.line_to(point(right, bottom));
                    path.line_to(point(left, bottom));
                    path.close();
                }
                window.paint_path(path.build().expect("guest title ink rectangles"), foreground);
            } else if !smooth_ink {
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

/// A styled line keeps measured insertion coordinates distinct from the
/// actual pen used by successive native draw calls. All coordinates are
/// port-local, so scrolling and justification stay in the guest domain.
#[derive(Clone, Debug)]
pub(crate) struct StyledTextEditLine {
    pub geometry: systemless::runner::TextEditLineGeometry,
    pub baseline: i16,
    pub selection: Option<(i16, i16, i16, i16)>,
    pub runs: Vec<StyledTextEditRun>,
}

#[derive(Clone, Debug)]
pub(crate) struct StyledTextEditRun {
    pub bytes: std::ops::Range<usize>,
    pub left: i16,
    pub measured_positions: Vec<i16>,
    pub glyphs: ClassicLine,
    pub ink: systemless::runner::TextEditInkSnapshot,
}

/// Native standard-LDEF layout inputs. CPU painters supply their own inset,
/// baseline and stopping boundary; host typography never supplies metrics.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassicListCellLayout {
    pub font: i16,
    pub size: i16,
    pub left: i16,
    pub baseline: i16,
    pub clip: (i16, i16, i16, i16),
    pub stop_before: Option<i16>,
    pub char_extra: i16,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassicListCellPaintPlan {
    pub pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    pub background: [u8; 3],
    pub clip: (i16, i16, i16, i16),
    smooth: (ClassicLine, i16, i16, [u8; 3]),
}

impl ClassicListCellPaintPlan {
    /// Uniform physical ink/background candidates come from the intact native
    /// standard draw. The complete cell must match the guest bitmap recipe.
    pub fn from_guest(
        paint: &systemless::runner::StandardListCellPaintSnapshot,
        global_clip: (i16, i16, i16, i16), native: &[u8], width: u32, height: u32,
    ) -> Option<Self> {
        use systemless::runner::TextEditCharExtraSnapshot;
        if paint.painted_regions.is_empty() || !matches!(paint.char_extra,
            TextEditCharExtraSnapshot::ClassicFixed(0) | TextEditCharExtraSnapshot::PpcPacked(0))
            || (matches!(paint.char_extra, TextEditCharExtraSnapshot::ClassicFixed(_)) && paint.space_extra >> 16 != 0) {
            return None;
        }
        let layout = ClassicListCellLayout { font: paint.font, size: paint.size,
            left: paint.left, baseline: paint.baseline, clip: paint.clip,
            stop_before: paint.stop_before, char_extra: 0 };
        let pixels = layout.pixels(&paint.bytes)?;
        let (top, left, bottom, right) = global_clip;
        if top < 0 || left < 0 || top >= bottom || left >= right
            || u32::try_from(bottom).ok()? > height || u32::try_from(right).ok()? > width
            || i32::from(bottom) - i32::from(top) != i32::from(paint.clip.2) - i32::from(paint.clip.0)
            || i32::from(right) - i32::from(left) != i32::from(paint.clip.3) - i32::from(paint.clip.1)
            || native.len() != (width as usize).checked_mul(height as usize)?.checked_mul(4)? { return None; }
        let sample = |x: i16, y: i16| -> Option<[u8; 3]> {
            let gx = i32::from(left) + i32::from(x) - i32::from(paint.clip.1);
            let gy = i32::from(top) + i32::from(y) - i32::from(paint.clip.0);
            if gx < 0 || gy < 0 || gx >= width as i32 || gy >= height as i32 { return None; }
            let at = (gy as usize * width as usize + gx as usize) * 4;
            native.get(at..at + 3)?.try_into().ok()
        };
        let stride = usize::try_from(i32::from(right) - i32::from(left)).ok()?;
        let rows = usize::try_from(i32::from(bottom) - i32::from(top)).ok()?;
        let mut ownership = vec![false; stride.checked_mul(rows)?];
        for &(t, l, b, r) in &paint.painted_regions {
            let t = t.max(top); let l = l.max(left); let b = b.min(bottom); let r = r.min(right);
            if t >= b || l >= r { continue; }
            for y in t..b {
                let at = (i32::from(y) - i32::from(top)) as usize * stride;
                ownership[at + (i32::from(l) - i32::from(left)) as usize..
                    at + (i32::from(r) - i32::from(left)) as usize].fill(true);
            }
        }
        let owned = |x: i16, y: i16| ownership[(i32::from(y) - i32::from(paint.clip.0)) as usize * stride
            + (i32::from(x) - i32::from(paint.clip.1)) as usize];
        let empty = (paint.clip.0..paint.clip.2).flat_map(|y|
            (paint.clip.1..paint.clip.3).map(move |x| (x, y))).find(|&(x, y)| owned(x, y) && !pixels.contains(&(x, y)))?;
        let background = sample(empty.0, empty.1)?;
        let foreground = match pixels.iter().find(|&&(x, y)| owned(x, y)) {
            Some(&(x, y)) => sample(x, y)?, None => background,
        };
        for y in paint.clip.0..paint.clip.2 { for x in paint.clip.1..paint.clip.3 {
            if owned(x, y) && sample(x, y)? != if pixels.contains(&(x, y)) { foreground } else { background } {
                return None;
            }
        } }
        Some(Self { pixels: pixels.into_iter().map(|point| (point, foreground)).collect(),
            background, clip: paint.clip,
            smooth: (layout.label(&paint.bytes), layout.left, layout.baseline, foreground) })
    }

    /// The caller must establish standard painter ownership and intact drawing;
    /// equality alone cannot establish that an application-drawn cell is ours.
    pub fn qualify(
        layout: ClassicListCellLayout, bytes: &[u8], drawing_intact: bool,
        foreground: [u8; 3], background: [u8; 3],
        global_clip: (i16, i16, i16, i16), native: &[u8], width: u32, height: u32,
    ) -> Option<Self> {
        if !drawing_intact { return None; }
        let (top, left, bottom, right) = global_clip;
        if top < 0 || left < 0 || top >= bottom || left >= right
            || i32::from(bottom) > i32::try_from(height).ok()?
            || i32::from(right) > i32::try_from(width).ok()?
            || i32::from(bottom) - i32::from(top) != i32::from(layout.clip.2) - i32::from(layout.clip.0)
            || i32::from(right) - i32::from(left) != i32::from(layout.clip.3) - i32::from(layout.clip.1)
            || native.len() != (width as usize).checked_mul(height as usize)?.checked_mul(4)? { return None; }
        let pixels: std::collections::BTreeMap<_, _> = layout.pixels(bytes)?.into_iter()
            .map(|point| (point, foreground)).collect();
        for y in layout.clip.0..layout.clip.2 { for x in layout.clip.1..layout.clip.3 {
            let gx = i32::from(left) + i32::from(x) - i32::from(layout.clip.1);
            let gy = i32::from(top) + i32::from(y) - i32::from(layout.clip.0);
            let at = (gy as usize * width as usize + gx as usize) * 4;
            if native[at..at + 3] != pixels.get(&(x, y)).copied().unwrap_or(background) { return None; }
        } }
        Some(Self { pixels, background, clip: layout.clip,
            smooth: (layout.label(bytes), layout.left, layout.baseline, foreground) })
    }
}

/// Use the shared device-snapped canvas for a fully qualified standard cell.
pub(crate) fn classic_list_cell(
    plan: ClassicListCellPaintPlan, scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_text_pixels_with_smooth(plan.pixels, scale, port_origin,
        Some((plan.clip, plan.background)), Some(plan.smooth))
}

impl ClassicListCellLayout {
    /// Preserve the native stop-before pen policy, including the final glyph
    /// that starts inside the boundary and is then clipped by the owned cell.
    fn label(self, bytes: &[u8]) -> ClassicLine {
        let line = ClassicLine::plain(bytes, self.font, self.size);
        let count = line.positions.iter().take(bytes.len()).take_while(|&&pen|
            self.stop_before.is_none_or(|stop| i32::from(self.left) + pen < i32::from(stop))).count();
        ClassicLine::plain(&bytes[..count], self.font, self.size)
    }
    /// Bytes are the owning CPU painter's actual input, after its decoding
    /// policy. Do not substitute the snapshot's decoded display label.
    /// Bitmap ink only. Physical paint and intact cell ownership must be
    /// independently qualified before this recipe replaces guest drawing.
    pub fn pixels(self, bytes: &[u8]) -> Option<std::collections::BTreeSet<(i16, i16)>> {
        let (top, left, bottom, right) = self.clip;
        if top >= bottom || left >= right || self.char_extra != 0 { return None; }
        let mut pixels = std::collections::BTreeSet::new();
        let mut pen = i32::from(self.left);
        for byte in bytes {
            if self.stop_before.is_some_and(|stop| pen >= i32::from(stop)) { break; }
            let line = ClassicLine::plain(&[*byte], self.font, self.size);
            for (x, y, width) in line.ink {
                let y = i32::from(self.baseline).checked_add(y)?;
                if y < i32::from(top) || y >= i32::from(bottom) { continue; }
                for dx in 0..width {
                    let x = pen.checked_add(x)?.checked_add(dx)?;
                    if x >= i32::from(left) && x < i32::from(right) {
                        pixels.insert((i16::try_from(x).ok()?, i16::try_from(y).ok()?));
                    }
                }
            }
            pen = pen.checked_add(*line.positions.last()?)?;
        }
        Some(pixels)
    }
}

/// A whole-field recipe, qualified against native pixels before ownership.
/// The caller supplies resolved background and caret paint; no host font or
/// theme colour is inferred. Application drawing or unsupported paint declines.
#[derive(Clone, Debug)]
pub(crate) struct StyledTextEditPaintPlan {
    pub pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    pub background: [u8; 3],
    pub view: (i16, i16, i16, i16),
}

impl StyledTextEditPaintPlan {
    pub fn qualify(
        record: &systemless::runner::TextEditSnapshot,
        background: &systemless::runner::TextEditInkSnapshot,
        caret: Option<((i16, i16, i16, i16), systemless::runner::TextEditInkSnapshot)>,
        native: &[u8], width: u32, height: u32,
    ) -> Option<Self> {
        use systemless::runner::TextEditLineLayoutPolicy;
        if !record.drawing_intact { return None; }
        let view = record.view_rect;
        let global = record.global_view_rect?;
        if global.0 < 0 || global.1 < 0 || global.0 >= global.2 || global.1 >= global.3
            || i32::from(global.2) > i32::try_from(height).ok()?
            || i32::from(global.3) > i32::try_from(width).ok()?
            || native.len() != (width as usize).checked_mul(height as usize)?.checked_mul(4)?
            || i32::from(global.2) - i32::from(global.0) != i32::from(view.2) - i32::from(view.0)
            || i32::from(global.3) - i32::from(global.1) != i32::from(view.3) - i32::from(view.1) { return None; }
        let inside = |&(x, y): &(i16, i16)| x >= view.1 && x < view.3 && y >= view.0 && y < view.2;
        type InkPairs = std::collections::BTreeMap<(i16, i16), ([u8; 3], [u8; 3])>;
        fn invert(pixels: &mut InkPairs, rect: (i16, i16, i16, i16), background: &systemless::runner::TextEditInkSnapshot) {
            let (top, left, bottom, right) = rect;
            for y in top..bottom { for x in left..right {
                let pair = pixels.entry((x, y)).or_insert((background.rgb, background.inverted_rgb));
                std::mem::swap(&mut pair.0, &mut pair.1);
            } }
        }
        let mut pairs = InkPairs::new();
        let mut selections = Vec::new();
        for index in 0..record.line_count {
            let (geometry, _) = record.guest_styled_line_geometry(index)?;
            if record.line_layout_policy == TextEditLineLayoutPolicy::CumulativeGuestMetrics
                && (geometry.top.saturating_add(geometry.height) <= view.0 || geometry.top >= view.2) { continue; }
            let line = StyledTextEditLine::from_guest(record, index)?;
            for run in &line.runs {
                for &(x, y, count) in &run.glyphs.ink {
                    let y = i16::try_from(i32::from(line.baseline).checked_add(y)?).ok()?;
                    for dx in 0..count {
                        let x = i16::try_from(i32::from(run.left).checked_add(x)?.checked_add(dx)?).ok()?;
                        if inside(&(x, y)) { pairs.insert((x, y), (run.ink.rgb, run.ink.inverted_rgb)); }
                    }
                }
            }
            if let Some(rect) = line.selection {
                // PPC highlights immediately after each line. Later run ink
                // can overwrite earlier selected pixels in overlapping boxes.
                if record.line_layout_policy == TextEditLineLayoutPolicy::PpcRunMetrics {
                    invert(&mut pairs, rect, background);
                } else { selections.push(rect); }
            }
        }
        // Classic highlights after all visible line ink. Every inversion
        // swaps the actual current physical ink pair, including prior highlights.
        for rect in selections { invert(&mut pairs, rect, background); }
        let mut pixels: std::collections::BTreeMap<_, _> = pairs.into_iter()
            .map(|(point, (rgb, _))| (point, rgb)).collect();
        if let Some((rect, ink)) = caret {
            if !record.active || !record.caret_visible || record.selection.0 != record.selection.1 { return None; }
            let (top, left, bottom, right) = rect;
            if top >= bottom || left >= right || top < view.0 || left < view.1
                || bottom > view.2 || right > view.3 { return None; }
            for y in top..bottom { for x in left..right { pixels.insert((x, y), ink.rgb); } }
        }
        for y in view.0..view.2 { for x in view.1..view.3 {
            let gx = i32::from(global.1) + i32::from(x) - i32::from(view.1);
            let gy = i32::from(global.0) + i32::from(y) - i32::from(view.0);
            let at = (gy as usize * width as usize + gx as usize) * 4;
            if native[at..at + 3] != pixels.get(&(x, y)).copied().unwrap_or(background.rgb) { return None; }
        } }
        Some(Self { pixels, background: background.rgb, view })
    }
}

impl StyledTextEditLine {
    pub fn from_guest(record: &systemless::runner::TextEditSnapshot, index: usize) -> Option<Self> {
        use systemless::runner::TextEditLineLayoutPolicy;
        if !record.styled { return None; }
        let paint = record.paint.as_ref()?;
        if !paint.supports_zero_spacing_src_or() { return None; }
        let styles = record.style_runs.as_ref()?;
        if styles.len() != paint.style_ink.len() { return None; }
        let (geometry, measured) = record.guest_styled_line_geometry(index)?;
        let mut left = geometry.left;
        let mut runs = Vec::with_capacity(measured.len());
        for (bytes, measured_positions) in measured {
            let style_index = styles.iter().rposition(|style| style.start <= bytes.start)?;
            let style = &styles[style_index];
            let origin = i32::from(*measured_positions.first()?);
            let positions = measured_positions.iter().map(|&x| i32::from(x) - origin).collect();
            let text = record.text.get(bytes.clone())?;
            let (glyphs, advance) = match record.line_layout_policy {
                TextEditLineLayoutPolicy::CumulativeGuestMetrics =>
                    ClassicLine::classic_textedit_run_with_paint_advance(text, style.font, style.size, style.face, positions)?,
                TextEditLineLayoutPolicy::PpcRunMetrics =>
                    ClassicLine::ppc_styled_run_with_paint_advance(text, style.font, style.size, style.face, positions)?,
            };
            let next = left.checked_add(i16::try_from(advance).ok()?)?;
            runs.push(StyledTextEditRun { bytes, left, measured_positions, glyphs,
                ink: paint.style_ink.get(style_index)?.clone() });
            left = next;
        }
        let baseline = match record.line_layout_policy {
            TextEditLineLayoutPolicy::CumulativeGuestMetrics => geometry.top.saturating_add(geometry.ascent),
            TextEditLineLayoutPolicy::PpcRunMetrics => record.dest_rect.0.saturating_add(geometry.ascent)
                .saturating_add(i16::try_from(index).ok()?.saturating_mul(geometry.height)),
        };
        Some(Self { geometry, baseline, selection: record.guest_styled_selection_rect(index)?, runs })
    }

    /// Apply classic pixel inversion after ordered run ink. Background ink
    /// comes from the caller's qualified erase policy, not a host accent colour.
    pub fn pixels_with_selection(
        &self, background: &systemless::runner::TextEditInkSnapshot,
    ) -> Option<std::collections::BTreeMap<(i16, i16), [u8; 3]>> {
        let mut pixels = self.pixels()?;
        let Some((top, left, bottom, right)) = self.selection else { return Some(pixels); };
        let baseline = self.baseline;
        let mut inverse_ink = std::collections::BTreeMap::new();
        for run in &self.runs {
            for &(x, y, width) in &run.glyphs.ink {
                let y = i16::try_from(i32::from(baseline).checked_add(y)?).ok()?;
                for dx in 0..width {
                    let x = i16::try_from(i32::from(run.left).checked_add(x)?.checked_add(dx)?).ok()?;
                    inverse_ink.insert((x, y), run.ink.inverted_rgb);
                }
            }
        }
        for y in top..bottom { for x in left..right {
            pixels.insert((x, y), inverse_ink.get(&(x, y)).copied().unwrap_or(background.inverted_rgb));
        } }
        Some(pixels)
    }

    /// Paint a qualified solid caret after run ink. The caller must resolve
    /// the native pen/theme policy; a style colour alone does not establish
    /// classic PaintRect's pattern or transfer mode. Geometry is already
    /// clipped to the guest-owned view by guest_styled_caret_rect.
    pub fn pixels_with_solid_caret(
        &self, rect: Option<(i16, i16, i16, i16)>,
        ink: &systemless::runner::TextEditInkSnapshot,
    ) -> Option<std::collections::BTreeMap<(i16, i16), [u8; 3]>> {
        // Selection and insertion caret are mutually exclusive in TextEdit.
        if self.selection.is_some() { return None; }
        let mut pixels = self.pixels()?;
        if let Some((top, left, bottom, right)) = rect {
            if top >= bottom || left >= right { return None; }
            for y in top..bottom { for x in left..right {
                pixels.insert((x, y), ink.rgb);
            } }
        }
        Some(pixels)
    }

    /// srcOr's set mask pixels replace foreground in both native CPU paths.
    /// Resolve overlapping runs in draw order, retaining the last run's ink.
    /// Background erasure and selection are separate presentation operations.
    pub fn pixels(&self) -> Option<std::collections::BTreeMap<(i16, i16), [u8; 3]>> {
        let mut pixels = std::collections::BTreeMap::new();
        let baseline = self.baseline;
        for run in &self.runs {
            for &(x, y, width) in &run.glyphs.ink {
                let top = i16::try_from(i32::from(baseline).checked_add(y)?).ok()?;
                for dx in 0..width {
                    let left = i16::try_from(i32::from(run.left).checked_add(x)?.checked_add(dx)?).ok()?;
                    pixels.insert((left, top), run.ink.rgb);
                }
            }
        }
        Some(pixels)
    }
}

/// Paint styled native bitmap ink in the shared GPUI compositor. The caller
/// owns background/selection, clipping and the port-to-scene transform.
pub(crate) fn classic_styled_text_edit_ink(
    line: StyledTextEditLine, scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_styled_text_pixels(line.pixels()?, scale, port_origin)
}

pub(crate) fn classic_styled_text_edit_selection(
    line: StyledTextEditLine, background: &systemless::runner::TextEditInkSnapshot,
    scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_styled_text_pixels(line.pixels_with_selection(background)?, scale, port_origin)
}

/// Solid guest caret paint only: patterned pens and themed caps require
/// their own qualified raster rather than silent substitution here.
pub(crate) fn classic_styled_text_edit_solid_caret(
    line: StyledTextEditLine, rect: Option<(i16, i16, i16, i16)>,
    ink: &systemless::runner::TextEditInkSnapshot,
    scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_styled_text_pixels(line.pixels_with_solid_caret(rect, ink)?, scale, port_origin)
}

/// Paint an already-qualified whole field in the shared live/headless canvas.
/// Background and ink use the same device snapping, including fractional scales.
pub(crate) fn classic_styled_text_edit_field(
    plan: StyledTextEditPaintPlan, scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_styled_text_pixels_with_background(plan.pixels, scale, port_origin,
        Some((plan.view, plan.background)))
}

fn classic_styled_text_pixels(
    pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    scale: f32, port_origin: (f32, f32),
) -> Option<impl gpui_kit::IntoElement> {
    classic_styled_text_pixels_with_background(pixels, scale, port_origin, None)
}

fn classic_styled_text_pixels_with_background(
    pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    scale: f32, port_origin: (f32, f32),
    background: Option<((i16, i16, i16, i16), [u8; 3])>,
) -> Option<impl gpui_kit::IntoElement> {
    classic_text_pixels_with_smooth(pixels, scale, port_origin, background, None)
}

fn classic_text_pixels_with_smooth(
    pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    scale: f32, port_origin: (f32, f32),
    background: Option<((i16, i16, i16, i16), [u8; 3])>,
    smooth: Option<(ClassicLine, i16, i16, [u8; 3])>,
) -> Option<impl gpui_kit::IntoElement> {
    if !scale.is_finite() || scale <= 0. || !port_origin.0.is_finite() || !port_origin.1.is_finite() {
        return None;
    }
    use gpui_kit::{prelude::*, *};
    let mut paths: std::collections::BTreeMap<[u8; 3], Vec<(i16, i16)>> = std::collections::BTreeMap::new();
    for (point, rgb) in pixels { paths.entry(rgb).or_default().push(point); }
    Some(canvas(|bounds, _, _| bounds, move |_, _, window, _| {
        let device_scale = window.scale_factor();
        let snap = |value: f32| {
            let device = value * device_scale;
            px((device.abs() - 0.5).ceil().copysign(device) / device_scale)
        };
        if let Some(((top, left, bottom, right), [r, g, b])) = background {
            let left = snap(port_origin.0 + f32::from(left) * scale);
            let top = snap(port_origin.1 + f32::from(top) * scale);
            let right = snap(port_origin.0 + f32::from(right) * scale);
            let bottom = snap(port_origin.1 + f32::from(bottom) * scale);
            if right > left && bottom > top {
                let ink: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
                window.paint_quad(fill(Bounds::new(point(left, top), size(right - left, bottom - top)), ink));
            }
        }
        if let Some((line, left, baseline, [r, g, b])) = &smooth {
            let ink: Hsla = rgb((u32::from(*r) << 16) | (u32::from(*g) << 8) | u32::from(*b)).into();
            if paint_smooth_label(line, px(port_origin.0 + f32::from(*left) * scale),
                px(port_origin.1 + f32::from(*baseline) * scale), scale, ink, window) { return; }
        }
        for (color, pixels) in &paths {
            let mut path = PathBuilder::fill();
            for &(x, y) in pixels {
                let x = port_origin.0 + f32::from(x) * scale;
                let y = port_origin.1 + f32::from(y) * scale;
                let left = snap(x); let top = snap(y);
                let right = snap(x + scale); let bottom = snap(y + scale);
                if right <= left || bottom <= top { continue; }
                path.move_to(point(left, top));
                path.line_to(point(right, top));
                path.line_to(point(right, bottom));
                path.line_to(point(left, bottom));
                path.close();
            }
            let [r, g, b] = *color;
            let ink: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
            window.paint_path(path.build().expect("guest styled TextEdit ink rectangles"), ink);
        }
    }).size_full())
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
    for source in &mut line.smooth_sources { source.0 -= left; }
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

/// Paint CPU-resolved indicator runs using the same scene transform as text.
pub(crate) fn classic_popup_indicator(
    indicator: systemless::runner::ControlPopupIndicator,
    scale: f32, scene_origin: (f32, f32),
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    canvas(|bounds, _, _| bounds, move |_, _, window, _| {
        let [r, g, b] = indicator.rgb;
        let ink: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
        for span in &indicator.spans {
            window.paint_quad(fill(Bounds::new(
                point(px(scene_origin.0 + f32::from(span.left) * scale),
                    px(scene_origin.1 + f32::from(span.top) * scale)),
                size(px(f32::from(span.width) * scale), px(scale))), ink));
        }
    }).size_full()
}

pub(crate) fn classic_popup_control_label(
    label: &str, guest_font: systemless::menu_model::GuestMenuFont, title: bool, text_inset: i16,
    scale: f32, ink: systemless::runner::ControlTextInk,
    guest_bounds: (i32, i32, i32, i32), scene_origin: (f32, f32),
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let label = label.to_owned();
    let metrics = systemless::quickdraw::text::get_font_metrics(guest_font.family, guest_font.point_size());
    canvas(move |bounds, _, _| bounds, move |_, _, window, _| {
        // Guest CDEF geometry is authoritative. Fractional host layout can
        // round a canvas's origin/height independently and move bitmap rows.
        let (top, left, bottom, right) = guest_bounds;
        let width = right - left;
        let height = bottom - top;
        let origin = point(px(scene_origin.0 + left as f32 * scale),
            px(scene_origin.1 + top as f32 * scale));
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
        if let systemless::runner::ControlTextInk::Solid([r, g, b]) = ink {
            let foreground: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
            if paint_smooth_label(&line, origin.x + px(x as f32 * scale),
                origin.y + px(baseline as f32 * scale), scale, foreground, window) { return; }
        }
        for &(ink_x, ink_y, ink_width) in &line.ink {
            let y = baseline + ink_y;
            // Solid ink retains one quad per bitmap run; only checker ink
            // needs individual guest pixels to preserve its global phase.
            if let systemless::runner::ControlTextInk::Solid([r, g, b]) = ink {
                let foreground: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
                window.paint_quad(fill(Bounds::new(
                    point(origin.x + px((x + ink_x) as f32 * scale), origin.y + px(y as f32 * scale)),
                    size(px(ink_width as f32 * scale), px(scale))), foreground));
                continue;
            }
            for dx in 0..ink_width {
                let x = x + ink_x + dx;
                let [r, g, b] = ink.pixel(left + x, top + y);
                let foreground: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
                window.paint_quad(fill(Bounds::new(
                    point(origin.x + px(x as f32 * scale), origin.y + px(y as f32 * scale)),
                    size(px(scale), px(scale))), foreground));
            }
        }
    }).size_full()
}

/// Resolve a whole run before painting so an unsupported glyph cannot leave
/// partially replaced text. Native paint pens may differ from insertion positions.
fn resolve_smooth_run(
    line: &ClassicLine, raster: u32,
) -> Option<Vec<(i32, systemless::quickdraw::text::SmoothGlyphSnapshot)>> {
    if line.smooth_sources.is_empty() || line.smooth_sources.len() + 1 != line.positions.len() {
        return None;
    }
    line.smooth_sources.iter().map(|&(pen, glyph, data)| {
        systemless::quickdraw::text::smooth_resolved_glyph(glyph, data, raster).map(|mask| (pen, mask))
    }).collect()
}

/// Outline coverage changes ink only; guest advances and baseline remain authoritative.
fn paint_smooth_label(
    line: &ClassicLine, left: gpui_kit::Pixels, baseline: gpui_kit::Pixels,
    scale: f32, foreground: gpui_kit::Hsla, window: &mut gpui_kit::Window,
) -> bool {
    use gpui_kit::*;
    if line.smooth_sources.is_empty() || line.smooth_sources.len() + 1 != line.positions.len() { return false; }
    if line.smooth_sources.iter().enumerate().any(|(index, source)| source.0 != line.positions[index]) { return false; }
    let raster = (scale * window.scale_factor()).ceil().max(1.) as u32;
    let Some(glyphs) = resolve_smooth_run(line, raster) else { return false; };
    let unit = scale / raster as f32;
    let mut paths = std::collections::BTreeMap::new();
    for (pen, glyph) in glyphs {
        for y in 0..glyph.height {
            let mut x = 0;
            while x < glyph.width {
                let alpha = glyph.pixels[(y * glyph.width + x) as usize];
                let start = x;
                x += 1;
                while x < glyph.width && glyph.pixels[(y * glyph.width + x) as usize] == alpha { x += 1; }
                if alpha == 0 { continue; }
                let x0 = left + px(pen as f32 * scale + (glyph.left + start) as f32 * unit);
                let y0 = baseline + px((glyph.top + y) as f32 * unit);
                let x1 = x0 + px((x - start) as f32 * unit);
                let y1 = y0 + px(unit);
                let path = paths.entry(alpha).or_insert_with(PathBuilder::fill);
                path.move_to(point(x0, y0));
                path.line_to(point(x1, y0));
                path.line_to(point(x1, y1));
                path.line_to(point(x0, y1));
                path.close();
            }
        }
    }
    for (alpha, path) in paths {
        let mut ink = foreground;
        ink.a *= f32::from(alpha) / 255.;
        window.paint_path(path.build().expect("resolved outline coverage spans"), ink);
    }
    true
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
            if paint_smooth_label(&line, bounds.left() + px(x as f32 * scale),
                bounds.top() + px(baseline as f32 * scale), scale, foreground, window) { return; }
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
            if paint_smooth_label(line, bounds.left() + px(f32::from(*x) * scale),
                bounds.top() + px(f32::from(baseline) * scale), scale, foreground, window) {
                continue;
            }
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
        if !paint_smooth_label(&line, bounds.left() + px(scale),
            bounds.top() + px(f32::from(layout.baseline) * scale),
            scale, foreground, window) {
            for &(x, y, width) in &line.ink {
                window.paint_quad(fill(Bounds::new(
                    point(bounds.left() + px((x + 1) as f32 * scale),
                        bounds.top() + px((y + i32::from(layout.baseline)) as f32 * scale)),
                    size(px(width as f32 * scale), px(scale))), foreground));
            }
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
                if paint_smooth_label(line, bounds.left() + px(f32::from(layout.0) * scale),
                    bounds.top() + px(baseline as f32 * scale), scale, foreground, window) { continue; }
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
            if paint_smooth_label(&line, bounds.left() + px(f32::from(origin.0) * scale),
                bounds.top() + px(f32::from(origin.1) * scale), scale, foreground, window) { return; }
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
            if !paint_smooth_label(&painted, bounds.left() + px(f32::from(layout.origin.0) * scale),
                bounds.top() + px(f32::from(layout.origin.1) * scale), scale, foreground, window) {
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
