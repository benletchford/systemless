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
    fn smooth_basic_menu_faces_keep_native_pens_and_continuous_underline() {
        for (font, size) in [(0, 12), (3, 12), (4, 10)] { for face in (0..=127).filter(|face| smooth_label_style_supported(*face)) {
            let bytes = b"g W\x8e";
            let line = super::ClassicLine::styled(bytes, font, size, face);
            let mut pen = 0;
            for (index, byte) in bytes.iter().enumerate() {
                assert_eq!(line.positions[index], pen);
                assert_eq!(line.smooth_sources[index].0, pen);
                pen += systemless::quickdraw::text::classic_styled_glyph(font, size, *byte, face).0;
            }
            assert_eq!(line.positions.last(), Some(&pen));
            if face & 4 != 0 {
                let thickness = systemless::quickdraw::text::get_underline_thickness(font, size).max(1);
                assert_eq!(line.smooth_strokes.len(), pen as usize * thickness as usize);
            } else { assert!(line.smooth_strokes.is_empty()); }
            for raster in 1..=8 {
                assert!(super::resolve_smooth_run(&line, raster).is_some());
            }
        } }
        for face in [128, 255] {
            assert!(super::resolve_smooth_run(&super::ClassicLine::styled(b"Menu", 0, 12, face), 2).is_none());
        }
    }

    #[test]
    fn smooth_label_halos_keep_native_shadow_baseline_and_separate_line_underline() {
        use std::collections::BTreeSet;
        for (font, size) in [(0, 12), (3, 12), (4, 10)] {
            for face in (0u8..128).filter(|face| face & 24 != 0) {
                let bytes = b"g W\x8e";
                let line = ClassicLine::styled(bytes, font, size, face);
                let mut pen = 0;
                let mut actual = BTreeSet::new();
                for byte in bytes {
                    let (advance, base) = systemless::quickdraw::text::classic_styled_glyph(font, size, *byte, face & 3);
                    let left = base.iter().map(|p| i32::from(p.0)).min().unwrap_or(0);
                    let top = base.iter().map(|p| i32::from(p.1)).min().unwrap_or(0);
                    let width = base.iter().map(|p| i32::from(p.0)).max().map_or(0, |x| x - left + 1);
                    let height = base.iter().map(|p| i32::from(p.1)).max().map_or(0, |y| y - top + 1);
                    let mut pixels = vec![0; (width * height) as usize];
                    for (x, y) in base { pixels[((i32::from(y) - top) * width + i32::from(x) - left) as usize] = 255; }
                    let mask = systemless::quickdraw::text::SmoothGlyphSnapshot {
                        pixels: pixels.into(), width, height, left, top: top + line.smooth_y_offset,
                        guest_advance: advance, raster_scale: 1,
                    };
                    let mask = smooth_halo_mask(mask, line.smooth_halo.unwrap()).unwrap();
                    let expected_advance = systemless::quickdraw::text::classic_styled_glyph(font, size, *byte, face).0;
                    assert_eq!((mask.guest_advance + smooth_spacing_adjustment(face)).max(1), expected_advance);
                    for y in 0..mask.height { for x in 0..mask.width {
                        if mask.pixels[(y * mask.width + x) as usize] != 0 {
                            actual.insert((pen + mask.left + x, mask.top + y));
                        }
                    } }
                    pen += expected_advance;
                }
                actual.extend(line.smooth_strokes.iter().copied());
                let native: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, count)|
                    (x..x + count).map(move |x| (x, y))).collect();
                assert_eq!(actual, native, "font={font} size={size} face={face}");
                assert_eq!(line.positions.last(), Some(&pen));
                for raster in 1..=8 {
                    let masks = resolve_smooth_run(&line, raster).unwrap();
                    for (index, byte) in bytes.iter().enumerate() {
                        assert_eq!(masks[index].1.guest_advance,
                            systemless::quickdraw::text::classic_styled_glyph(font, size, *byte, face).0);
                    }
                }
            }
        }
    }

    #[test]
    fn smooth_condensed_extended_textedit_preserves_native_paint_and_insertion_positions() {
        let bytes = b"g W\x8e";
        let positions = vec![0, 100, 200, 300, 400];
        for ppc in [false, true] {
            for face in (32..=127).filter(|face| smooth_basic_style_supported(*face)) {
                let (line, paint_advance) = if ppc {
                    ClassicLine::ppc_styled_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
                } else {
                    ClassicLine::classic_textedit_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
                }.unwrap();
                assert_eq!(line.positions, positions);
                let base = if ppc {
                    ClassicLine::ppc_styled_run(bytes, 3, 12, face & 31, positions.clone())
                } else {
                    ClassicLine::classic_textedit_run(bytes, 3, 12, face & 31, positions.clone())
                }.unwrap();
                let advances: Vec<_> = bytes.iter().map(|&byte| if ppc {
                    i32::from(systemless::quickdraw::text::ppc_styled_run_ink(3, 12, face, &[byte]).0)
                } else {
                    systemless::quickdraw::text::classic_textedit_glyph_ink(3, 12, byte, face).unwrap().0
                }).collect();
                assert_eq!(paint_advance, advances.iter().sum::<i32>(), "PPC={ppc} face={face}");
                for raster in 1..=8 {
                    let masks = resolve_smooth_run(&line, raster).unwrap();
                    let base_masks = resolve_smooth_run(&base, raster).unwrap();
                    let mut pen = 0;
                    for (index, &byte) in bytes.iter().enumerate() {
                        assert_eq!(line.smooth_sources[index].0, pen);
                        let mask = &masks[index].1;
                        let original = &base_masks[index].1;
                        assert_eq!(masks[index].0, pen);
                        assert_eq!(mask.guest_advance, advances[index]);
                        if ppc && face & 4 != 0 && face & 24 != 0 {
                            // PPC's halo includes the run-wide underline. Spacing
                            // changes that ribbon's extent, not the source outline.
                            assert!(std::ptr::eq(line.smooth_sources[index].1, base.smooth_sources[index].1));
                            assert_eq!(line.smooth_halo_underlines[index],
                                systemless::quickdraw::text::ppc_styled_run_halo_underline_ink(
                                    3, 12, face, paint_advance).into_iter().map(|(x, y)| (x - pen, y)).collect::<Vec<_>>());
                        } else {
                            assert_eq!((mask.left, mask.top, mask.width, mask.height),
                                (original.left, original.top, original.width, original.height));
                            assert_eq!(mask.pixels, original.pixels, "spacing must not stretch the source glyph");
                        }
                        if byte != b' ' {
                            assert!(mask.pixels.iter().any(|&alpha| alpha > 0 && alpha < 255));
                        }
                        pen += advances[index];
                    }
                }
            }
        }
    }

    #[test]
    fn smooth_halos_match_native_binary_silhouettes_and_keep_fractional_edges() {
        use std::collections::BTreeSet;
        for ppc in [false, true] { for face in [8, 9, 10, 11, 16, 17, 18, 19, 24, 25, 26, 27] {
            for byte in b"Wg" {
                let native = |face| -> (i32, BTreeSet<(i32, i32)>) {
                    if ppc {
                        let (advance, pixels) = systemless::quickdraw::text::ppc_styled_run_ink(3, 12, face, &[*byte]);
                        (i32::from(advance), pixels.into_iter().collect())
                    } else {
                        let (advance, pixels) = systemless::quickdraw::text::classic_textedit_glyph_ink(3, 12, *byte, face).unwrap();
                        (advance, pixels.into_iter().map(|(x, y)| (i32::from(x), i32::from(y))).collect())
                    }
                };
                let (advance, base) = native(face & 3);
                let left = base.iter().map(|p| p.0).min().unwrap();
                let top = base.iter().map(|p| p.1).min().unwrap();
                let width = base.iter().map(|p| p.0).max().unwrap() - left + 1;
                let height = base.iter().map(|p| p.1).max().unwrap() - top + 1;
                let mut pixels = vec![0; (width * height) as usize];
                for (x, y) in base { pixels[((y - top) * width + x - left) as usize] = 255; }
                let mask = smooth_halo_mask(systemless::quickdraw::text::SmoothGlyphSnapshot {
                    pixels: pixels.into(), width, height, left, top, guest_advance: advance, raster_scale: 1,
                }, smooth_style_halo(face).unwrap()).unwrap();
                let mask = &mask;
                let actual: BTreeSet<_> = (0..mask.height).flat_map(|y|
                    (0..mask.width).filter_map(move |x| (mask.pixels[(y * mask.width + x) as usize] != 0)
                        .then_some((mask.left + x, mask.top + y)))).collect();
                let (expected_advance, expected) = native(face);
                assert_eq!(actual, expected, "PPC={ppc}, face={face}, byte={byte}");
                assert_eq!(mask.guest_advance, expected_advance);
            }
            let positions = vec![0, 100];
            let (line, _) = if ppc {
                ClassicLine::ppc_styled_run_with_paint_advance(b"W", 3, 12, face, positions.clone())
            } else {
                ClassicLine::classic_textedit_run_with_paint_advance(b"W", 3, 12, face, positions.clone())
            }.unwrap();
            assert_eq!(line.positions, positions);
            for raster in 1..=4 {
                let masks = resolve_smooth_run(&line, raster).unwrap();
                assert_eq!(masks.len(), 1);
                assert!(masks[0].1.pixels.iter().any(|&a| a > 0 && a < 255));
            }
        } }
    }

    #[test]
    fn smooth_underlined_halos_keep_cpu_binary_recipes_and_resolve_original_outlines() {
        use std::collections::BTreeSet;
        let bytes = b"g W\x8e";
        let positions = vec![0, 100, 200, 300, 400];
        for ppc in [false, true] {
            for face in (0u8..128).filter(|face| face & 4 != 0 && face & 24 != 0) {
                let (line, _) = if ppc {
                    ClassicLine::ppc_styled_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
                } else {
                    ClassicLine::classic_textedit_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
                }.unwrap();
                assert_eq!(line.positions, positions);
                assert!(line.smooth_strokes.is_empty());
                assert_eq!(line.smooth_halo_underlines.len(), bytes.len());
                let mut actual = BTreeSet::new();
                for (index, byte) in bytes.iter().enumerate() {
                    let (advance, pixels): (i32, BTreeSet<(i32, i32)>) = if ppc {
                        let (advance, pixels) = systemless::quickdraw::text::ppc_styled_run_ink(3, 12, face & 3, &[*byte]);
                        (i32::from(advance), pixels.into_iter().collect())
                    } else {
                        let (advance, pixels) = systemless::quickdraw::text::classic_textedit_glyph_ink(3, 12, *byte, face & 3).unwrap();
                        (advance, pixels.into_iter().map(|(x, y)| (i32::from(x), i32::from(y))).collect())
                    };
                    let left = pixels.iter().map(|p| p.0).min().unwrap_or(0);
                    let top = pixels.iter().map(|p| p.1).min().unwrap_or(0);
                    let width = pixels.iter().map(|p| p.0).max().map_or(0, |x| x - left + 1);
                    let height = pixels.iter().map(|p| p.1).max().map_or(0, |y| y - top + 1);
                    let mut coverage = vec![0; (width * height) as usize];
                    for (x, y) in pixels { coverage[((y - top) * width + x - left) as usize] = 255; }
                    let mask = systemless::quickdraw::text::SmoothGlyphSnapshot {
                        pixels: coverage.into(), width, height, left, top, guest_advance: advance, raster_scale: 1,
                    };
                    let mask = smooth_union_strokes(mask, &line.smooth_halo_underlines[index]).unwrap();
                    let mask = smooth_halo_mask_with_policy(mask, line.smooth_halo.unwrap(), line.smooth_halo_classic_everything).unwrap();
                    let pen = line.smooth_sources[index].0;
                    for y in 0..mask.height { for x in 0..mask.width {
                        if mask.pixels[(y * mask.width + x) as usize] != 0 {
                            actual.insert((pen + mask.left + x, mask.top + y));
                        }
                    } }
                }
                let native: BTreeSet<_> = line.ink.iter().flat_map(|&(x, y, count)|
                    (x..x + count).map(move |x| (x, y))).collect();
                assert_eq!(actual, native, "PPC={ppc} face={face}");
                for raster in 1..=8 {
                    let masks = resolve_smooth_run(&line, raster).unwrap();
                    assert_eq!(masks.len(), bytes.len());
                    assert!(masks.iter().any(|(_, mask)| mask.pixels.iter().any(|&a| a > 0 && a < 255)));
                }
            }
        }
    }

    #[test]
    fn smooth_textedit_underlines_preserve_cpu_strokes_and_native_ink() {
        use std::collections::BTreeSet;
        let bytes = b"g W";
        let positions = vec![0, 100, 200, 300];
        for ppc in [false, true] { for face in 4..=7 {
            let make = |face| if ppc {
                ClassicLine::ppc_styled_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
            } else {
                ClassicLine::classic_textedit_run_with_paint_advance(bytes, 3, 12, face, positions.clone())
            }.unwrap();
            let (line, advance) = make(face);
            let (base, base_advance) = make(face & !4);
            assert_eq!(line.positions, positions);
            assert_eq!(advance, base_advance);
            let ink = |line: &ClassicLine| line.ink.iter().flat_map(|&(x, y, count)|
                (x..x + count).map(move |x| (x, y))).collect::<BTreeSet<_>>();
            let strokes: BTreeSet<_> = line.smooth_strokes.iter().copied().collect();
            assert!(!strokes.is_empty());
            assert_eq!(ink(&line), ink(&base).union(&strokes).copied().collect());
            if ppc {
                assert_eq!(strokes, (0..advance).map(|x| (x, 1)).collect());
            } else {
                assert!(strokes.len() < advance as usize, "classic descender gaps");
            }
            for raster in 1..=4 {
                let masks = resolve_smooth_run(&line, raster).unwrap();
                assert_eq!(masks.len(), bytes.len() + 1);
                let (pen, stroke) = masks.last().unwrap();
                assert_eq!(*pen, 0);
                let actual: BTreeSet<_> = (0..stroke.height).flat_map(|y|
                    (0..stroke.width).filter_map(move |x| {
                        let alpha = stroke.pixels[(y * stroke.width + x) as usize];
                        assert!(alpha == 0 || alpha == 255);
                        (alpha != 0).then_some((stroke.left + x, stroke.top + y))
                    })).collect();
                let mut expected = BTreeSet::new();
                for &(x, y) in &strokes { for dy in 0..raster as i32 { for dx in 0..raster as i32 {
                    expected.insert((x * raster as i32 + dx, y * raster as i32 + dy));
                } } }
                assert_eq!(actual, expected);
            }
        } }
    }

    #[test]
    fn smooth_textedit_italic_keeps_native_row_shear_and_guest_layout() {
        for ppc in [false, true] { for face in [2, 3] {
            let positions = vec![0, 100, 200];
            let (line, _) = if ppc {
                ClassicLine::ppc_styled_run_with_paint_advance(b"gW", 3, 12, face, positions.clone())
            } else {
                ClassicLine::classic_textedit_run_with_paint_advance(b"gW", 3, 12, face, positions.clone())
            }.unwrap();
            assert_eq!(line.positions, positions);
            let metrics = systemless::quickdraw::text::get_font_metrics(3, 12);
            for raster in 1..=4 {
                let resolved = resolve_smooth_run(&line, raster).unwrap();
                for ((pen, mask), &(source_pen, glyph, data)) in resolved.iter().zip(&line.smooth_sources) {
                    assert_eq!(*pen, source_pen);
                    let plain = systemless::quickdraw::text::smooth_resolved_glyph(glyph, data, raster).unwrap();
                    let mut expected = std::collections::BTreeMap::new();
                    for y in 0..plain.height { for x in 0..plain.width {
                        let alpha = plain.pixels[(y * plain.width + x) as usize];
                        if alpha == 0 { continue; }
                        let guest_y = (plain.top + y).div_euclid(raster as i32) as i16;
                        let shift = i32::from(systemless::quickdraw::fonts::style::get_italic_slant(
                            3, 12, &metrics, 0, guest_y)) * raster as i32;
                        for dx in [0, if face == 3 { raster as i32 } else { 0 }] {
                            let at = (plain.left + x + shift + dx, plain.top + y);
                            let entry = expected.entry(at).or_insert(0u8);
                            *entry = (*entry).max(alpha);
                        }
                    } }
                    let actual: std::collections::BTreeMap<_, _> = (0..mask.height).flat_map(|y|
                        (0..mask.width).filter_map(move |x| {
                            let alpha = mask.pixels[(y * mask.width + x) as usize];
                            (alpha != 0).then_some(((mask.left + x, mask.top + y), alpha))
                        })).collect();
                    if raster == 1 { assert_eq!(actual, expected); }
                    assert_eq!(mask.top, plain.top);
                    assert_eq!(mask.height, plain.height);
                    let native_left = expected.keys().map(|p| p.0).min().unwrap();
                    let native_right = expected.keys().map(|p| p.0).max().unwrap();
                    assert!(actual.keys().all(|p| p.0 >= mask.left && p.0 < mask.left + mask.width));
                    assert!(native_left >= mask.left && native_right < mask.left + mask.width);
                    if face == 2 {
                        for y in 0..plain.height {
                            let source_sum: u32 = plain.pixels[(y * plain.width) as usize..((y + 1) * plain.width) as usize]
                                .iter().map(|alpha| u32::from(*alpha)).sum();
                            let painted_sum: u32 = mask.pixels[(y * mask.width) as usize..((y + 1) * mask.width) as usize]
                                .iter().map(|alpha| u32::from(*alpha)).sum();
                            assert_eq!(painted_sum, source_sum, "shear preserves row coverage");
                        }
                    }
                    assert_eq!(mask.guest_advance, plain.guest_advance + i32::from(face == 3));
                }
            }
        } }
    }

    #[test]
    fn continuous_italic_edges_preserve_coverage_and_native_envelope() {
        for raster in 2..=8 {
            let width = 2 * raster;
            let height = 12 * raster;
            let top = -8 * raster;
            let source = systemless::quickdraw::text::SmoothGlyphSnapshot {
                pixels: vec![255u8; (width * height) as usize].into(),
                width, height, left: -raster, top, guest_advance: 7,
                raster_scale: raster as u32,
            };
            let metrics = systemless::quickdraw::text::get_font_metrics(3, 12);
            let shifts: Vec<_> = (0..height).map(|y| i32::from(
                systemless::quickdraw::fonts::style::get_italic_slant(3, 12, &metrics,
                    0, ((top + y).div_euclid(raster)) as i16)) * raster).collect();
            let min = *shifts.iter().min().unwrap();
            let max = *shifts.iter().max().unwrap();
            let painted = super::smooth_italic_mask(source, 3, 12).unwrap();
            assert_eq!(painted.left, -raster + min);
            assert_eq!(painted.width, width + max - min);
            assert_eq!((painted.top, painted.height, painted.guest_advance), (top, height, 7));
            assert!(painted.pixels.iter().any(|alpha| *alpha > 0 && *alpha < 255),
                "device-resolution shear must antialias the opaque bar's boundaries");
            let mut previous: Option<usize> = None;
            for row in painted.pixels.chunks(painted.width as usize) {
                assert_eq!(row.iter().map(|a| u32::from(*a)).sum::<u32>(), width as u32 * 255);
                let left = row.iter().position(|a| *a != 0).unwrap();
                if let Some(before) = previous { assert!(left.abs_diff(before) <= 1); }
                previous = Some(left);
            }
        }
    }

    #[test]
    fn smooth_textedit_bold_uses_native_smear_and_paint_advances_on_both_cpus() {
        for ppc in [false, true] {
            let positions = vec![0, 100, 200];
            let (line, advance) = if ppc {
                ClassicLine::ppc_styled_run_with_paint_advance(b"Wi", 3, 12, 1, positions.clone())
            } else {
                ClassicLine::classic_textedit_run_with_paint_advance(b"Wi", 3, 12, 1, positions.clone())
            }.unwrap();
            assert_eq!(line.positions, positions);
            let (w, _) = systemless::quickdraw::text::get_glyph(3, 12, 'W').unwrap();
            let (i, _) = systemless::quickdraw::text::get_glyph(3, 12, 'i').unwrap();
            assert_eq!(advance, i32::from(w.advance) + i32::from(i.advance) + 2);
            for raster in 1..=4 {
                let resolved = resolve_smooth_run(&line, raster).unwrap();
                assert_eq!(resolved[1].0, i32::from(w.advance) + 1);
                for ((_, bold), &(_, glyph, data)) in resolved.iter().zip(&line.smooth_sources) {
                    let plain = systemless::quickdraw::text::smooth_resolved_glyph(glyph, data, raster).unwrap();
                    assert_eq!((bold.left, bold.top, bold.height), (plain.left, plain.top, plain.height));
                    assert_eq!(bold.width, plain.width + raster as i32);
                    assert_eq!(bold.guest_advance, plain.guest_advance + 1);
                    assert!(bold.pixels.iter().any(|&alpha| alpha > 0 && alpha < 255));
                    for y in 0..bold.height { for x in 0..bold.width {
                        let sample = |x| if x < 0 || x >= plain.width { 0 }
                            else { plain.pixels[(y * plain.width + x) as usize] };
                        assert_eq!(bold.pixels[(y * bold.width + x) as usize],
                            sample(x).max(sample(x - raster as i32)));
                    } }
                }
            }
        }
    }

    #[test]
    fn smooth_field_preserves_physical_inversion_order_clipping_and_caret() {
        use super::{smooth_textedit_pixels, ClassicLine, StyledTextEditPaintOp as Op};
        use systemless::runner::TextEditInkSnapshot;
        // Deliberately non-complementary indexed colours.
        let background = TextEditInkSnapshot { pixel: 0, rgb: [220; 3], inverted_rgb: [17; 3] };
        let ink = TextEditInkSnapshot { pixel: 1, rgb: [50; 3], inverted_rgb: [170; 3] };
        let line = ClassicLine::plain(b"W", 3, 12);
        let view = (0, 0, 18, 18);
        let run = Op::Run { glyphs: line, left: 1, baseline: 13, ink };
        for raster in 1..=4 {
            let normal = smooth_textedit_pixels(&[run.clone()], raster, view, &background).unwrap();
            let (&at, _) = normal.iter().find(|(_, rgb)| **rgb == [50; 3]).unwrap();
            assert!(normal.values().any(|rgb| rgb[0] > 50 && rgb[0] < 220));
            let after = smooth_textedit_pixels(&[run.clone(), Op::Invert(view)], raster, view, &background).unwrap();
            let before = smooth_textedit_pixels(&[Op::Invert(view), run.clone()], raster, view, &background).unwrap();
            assert_eq!(after[&at], [170; 3]);
            assert_eq!(before[&at], [50; 3]);
            assert_eq!(after[&(0, 0)], [17; 3]);
            let twice = smooth_textedit_pixels(&[run.clone(), Op::Invert(view), Op::Invert(view)], raster, view, &background).unwrap();
            for (point, rgb) in &normal { assert_eq!(twice[point], *rgb); }
            let crop = (3, 2, 10, 6);
            let clipped = smooth_textedit_pixels(&[run.clone()], raster, crop, &background).unwrap();
            assert_eq!(clipped, normal.iter().filter(|(p, _)|
                p.0 >= 2 * raster as i32 && p.0 < 6 * raster as i32
                && p.1 >= 3 * raster as i32 && p.1 < 10 * raster as i32)
                .map(|(&p, &rgb)| (p, rgb)).collect());
            let caret = TextEditInkSnapshot { pixel: 2, rgb: [90; 3], inverted_rgb: [110; 3] };
            let final_ink = smooth_textedit_pixels(&[run.clone(), Op::Invert(view),
                Op::Caret((0, 0, 18, 2), caret)], raster, view, &background).unwrap();
            for y in 0..18 * raster as i32 { for x in 0..2 * raster as i32 {
                assert_eq!(final_ink[&(x, y)], [90; 3]);
            } }
        }
    }

    #[test]
    fn styled_outline_preflight_is_atomic_and_keeps_native_paint_pens() {
        use super::{ClassicLine, StyledTextEditPaintOp as Op, ResolvedTextEditPaintOp as Resolved};
        let ink = systemless::runner::TextEditInkSnapshot {
            pixel: 1, rgb: [20, 40, 60], inverted_rgb: [235, 215, 195],
        };
        let positions = vec![0, 100, 200];
        let (line, _) = ClassicLine::classic_textedit_run_with_paint_advance(
            b"Wi", 3, 12, 0, positions.clone()).unwrap();
        let pens: Vec<_> = line.smooth_sources.iter().map(|source| source.0).collect();
        assert_ne!(pens[1], positions[1]);
        let run = Op::Run { glyphs: line, left: 17, baseline: 23, ink: ink.clone() };
        let ops = vec![run.clone(), Op::Invert((0, 0, 30, 40)),
            Op::Caret((1, 2, 3, 4), ink.clone())];
        for raster in 1..=4 {
            let resolved = Op::resolve_all(&ops, raster).unwrap();
            match &resolved[0] {
                Resolved::Run { glyphs, left, baseline, ink: paint } => {
                    assert_eq!(glyphs.iter().map(|item| item.0).collect::<Vec<_>>(), pens);
                    assert_eq!((*left, *baseline, paint.rgb), (17, 23, ink.rgb));
                }
                _ => panic!("native run order"),
            }
            assert!(matches!(resolved[1], Resolved::Invert((0, 0, 30, 40))));
            assert!(matches!(&resolved[2], Resolved::Caret((1, 2, 3, 4), paint) if paint.rgb == ink.rgb));
        }
        let outline = Op::Run { glyphs: ClassicLine::styled(b"outline", 3, 12, 8),
            left: 0, baseline: 12, ink: ink.clone() };
        assert!(Op::resolve_all(&[run.clone(), outline], 2).is_some());
        let unsupported = Op::Run { glyphs: ClassicLine::styled(b"reserved face", 3, 12, 128),
            left: 0, baseline: 12, ink };
        assert!(Op::resolve_all(&[run, unsupported], 2).is_none());
        assert!(Op::resolve_all(&ops, 0).is_none());
        assert!(Op::resolve_all(&ops, 9).is_none());
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
    fn scaled_classic_outline_preserves_guest_metrics_and_resolves_source_at_display_density() {
        use std::collections::BTreeSet;
        let bytes = b"Ag";
        let positions = vec![0, 3, 8];
        let (strike, enlargement) = systemless::quickdraw::fonts::get_font_face_scaled(3, 192);
        assert_eq!((strike.size, enlargement), (96, 2));
        for face in [0, 1, 4, 32, 64] {
            let (line, advance) = ClassicLine::classic_textedit_run_with_paint_advance(bytes, 3, 192, face, positions.clone()).unwrap();
            assert_eq!(line.positions, positions, "guest insertion metrics remain authoritative");
            let mut expected = BTreeSet::new();
            let mut pen = 0;
            for &byte in bytes {
                let (width, ink) = systemless::quickdraw::text::classic_textedit_glyph_ink(3, 192, byte, face).unwrap();
                expected.extend(ink.into_iter().map(|(x,y)| (pen+i32::from(x), i32::from(y)))); pen += width;
            }
            assert_eq!(advance, pen);
            assert_eq!(line.ink.iter().flat_map(|&(x,y,width)| (x..x+width).map(move |x| (x,y))).collect::<BTreeSet<_>>(), expected);
            assert_eq!(line.smooth_strike_scale, 2);
            for raster in [1, 2, 4] {
                let masks = resolve_smooth_run(&line, raster).expect("scaled outline must no longer decline");
                assert!(masks.iter().take(bytes.len()).all(|(_, mask)| mask.raster_scale == raster));
                assert!(masks.iter().take(bytes.len()).any(|(_, mask)| mask.pixels.iter().any(|alpha| *alpha > 0 && *alpha < 255)));
                if face == 0 {
                    for ((pen, mask), &(source_pen, glyph, data)) in masks.iter().zip(&line.smooth_sources) {
                        let original = systemless::quickdraw::text::smooth_resolved_glyph(glyph, data, raster*2).unwrap();
                        assert_eq!(*pen, source_pen);
                        assert_eq!(mask.pixels, original.pixels, "resolve the original outline instead of enlarging its bitmap");
                        assert_eq!((mask.width,mask.height,mask.left,mask.top), (original.width,original.height,original.left,original.top));
                        assert_eq!(mask.guest_advance, original.guest_advance*2);
                    }
                }
            }
        }
        let plain = ClassicLine::plain(bytes, 3, 192);
        assert_eq!(plain.smooth_strike_scale, 2);
        assert!(resolve_smooth_run(&plain, 2).is_some());
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
                if !smooth_basic_style_supported(face) { assert!(line.smooth_sources.is_empty()); }
                if smooth_basic_style_supported(face) && size == 12 {
                    assert_eq!(line.smooth_sources.len(), bytes.len());
                    assert_eq!(line.smooth_sources[0].0, 0);
                    assert_ne!(line.smooth_sources[1].0, positions[1], "native paint pens must not be replaced by insertion metrics");
                }
            }
        }
        assert!(ClassicLine::classic_textedit_run(bytes, 3, 12, 0, vec![0, 1]).is_none());
    }

    #[test]
    fn rational_style_mask_area_preserves_negative_bearings_phase_and_coverage() {
        let make = || systemless::quickdraw::text::SmoothGlyphSnapshot {
            pixels: vec![255,128].into(), width: 2, height: 1, left: -1, top: 0,
            guest_advance: 2, raster_scale: 1,
        };
        let (pen, transformed) = transform_source_mask(make(),3,2,1,1).unwrap();
        assert_eq!((pen,transformed.left,transformed.top,transformed.width,transformed.height), (1,-1,0,3,2));
        // Exact rectangle-overlap areas, independently calculated for 3/2:
        // the first row blends the half-pixel boundary, the second is half-height.
        assert_eq!(&*transformed.pixels, &[255,192,128,128,96,64]);
        assert_eq!(transformed.guest_advance, 3);
        let (_, identity) = transform_source_mask(make(),1,1,7,1).unwrap();
        assert_eq!((identity.left,identity.top,identity.width,identity.height), (-1,0,2,1));
        assert_eq!(&*identity.pixels, &[255,128]);
        assert!(transform_source_mask(make(),0,2,1,1).is_none());
        assert!(transform_source_mask(make(),3,0,1,1).is_none());
    }

    #[test]
    fn fractional_ppc_styles_synthesize_before_scaling_and_keep_native_strokes() {
        let bytes = b"Ag";
        let positions = vec![0,3,8];
        for face in [1,2,3,4,5,6,7,8,16,31,32,64,127] {
            let (line, advance) = ClassicLine::ppc_styled_run_with_paint_advance(bytes,3,145,face,positions.clone()).unwrap();
            assert_eq!(line.positions, positions);
            let (native_advance,native_ink) = systemless::quickdraw::text::ppc_styled_run_ink(3,145,face,bytes);
            assert_eq!(advance,i32::from(native_advance));
            assert_eq!(line.ink.iter().flat_map(|&(x,y,width)| (x..x+width).map(move |x| (x,y))).collect::<std::collections::BTreeSet<_>>(), native_ink.into_iter().collect());
            for raster in [1,2] {
                let masks = resolve_smooth_run(&line,raster).unwrap();
                let mut source_pen = 0;
                for ((pen,mask), &byte) in masks.iter().zip(bytes) {
                    assert_eq!(*pen,source_pen*145/96);
                    assert_eq!(mask.raster_scale,raster);
                    assert_eq!(mask.guest_advance,i32::from(systemless::quickdraw::text::ppc_styled_run_ink(3,145,face,&[byte]).0));
                    source_pen += i32::from(systemless::quickdraw::text::ppc_styled_run_ink(3,96,face,&[byte]).0);
                }
                if face & 4 != 0 && face & 24 == 0 {
                    let (pen,stroke) = masks.last().unwrap(); assert_eq!(*pen,0);
                    let actual = (0..stroke.height).flat_map(|y| (0..stroke.width).filter_map(move |x|
                        (stroke.pixels[(y*stroke.width+x) as usize] != 0).then_some((stroke.left+x,stroke.top+y))))
                        .collect::<std::collections::BTreeSet<_>>();
                    let r = raster as i32;
                    let expected = line.smooth_strokes.iter().flat_map(|&(x,y)| (0..r).flat_map(move |dy| (0..r).map(move |dx| (x*r+dx,y*r+dy))))
                        .collect::<std::collections::BTreeSet<_>>();
                    assert_eq!(actual,expected);
                }
            }
        }
    }

    #[test]
    fn fractional_ppc_outline_preserves_guest_metrics_and_subpixel_pen_phase() {
        let bytes = b"AA i";
        let positions = vec![0, 3, 8, 13, 19];
        let (strike, numerator, denominator) = systemless::quickdraw::fonts::get_font_face_scale_ratio(3, 145);
        assert_eq!((strike.size, numerator, denominator), (96, 145, 96));
        let (line, advance) = ClassicLine::ppc_styled_run_with_paint_advance(bytes, 3, 145, 0, positions.clone()).unwrap();
        assert_eq!(line.positions, positions);
        let (expected_advance, expected_ink) = systemless::quickdraw::text::ppc_styled_run_ink(3, 145, 0, bytes);
        assert_eq!(advance, i32::from(expected_advance));
        assert_eq!(line.ink.iter().flat_map(|&(x,y,width)| (x..x+width).map(move |x| (x,y))).collect::<std::collections::BTreeSet<_>>(), expected_ink.into_iter().collect());
        assert_eq!(line.smooth_sources.len(), bytes.len());
        for raster in [1, 2, 4] {
            let masks = resolve_smooth_run(&line, raster).unwrap();
            for ((pen, mask), &(source_pen, glyph, data)) in masks.iter().zip(&line.smooth_sources) {
                assert_eq!(*pen, source_pen*numerator/denominator);
                assert_eq!(mask.raster_scale, raster);
                let (direct_pen, direct) = systemless::quickdraw::text::smooth_resolved_ratio_glyph(glyph, data,
                    numerator as u32, denominator as u32, source_pen, raster).unwrap();
                assert_eq!(*pen, direct_pen); assert_eq!(mask.pixels, direct.pixels); assert_eq!(mask.left, direct.left);
            }
            let (_, glyph, data) = line.smooth_sources[0];
            let (_, zero) = systemless::quickdraw::text::smooth_resolved_ratio_glyph(glyph,data,145,96,0,raster).unwrap();
            let (translated_pen, translated) = systemless::quickdraw::text::smooth_resolved_ratio_glyph(glyph,data,145,96,96,raster).unwrap();
            assert_eq!(translated_pen, 145); assert_eq!(translated.pixels, zero.pixels); assert_eq!(translated.left, zero.left);
            let (_, fractional) = systemless::quickdraw::text::smooth_resolved_ratio_glyph(glyph,data,145,96,1,raster).unwrap();
            assert_ne!((fractional.left,fractional.pixels), (zero.left,zero.pixels), "fractional pen phase must survive rasterization");
        }
        let styled = ClassicLine::ppc_styled_run(bytes,3,145,1,positions).unwrap();
        assert!(resolve_smooth_run(&styled,2).is_some());
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
                if !smooth_basic_style_supported(face) { assert!(line.smooth_sources.is_empty()); }
                if smooth_basic_style_supported(face) && size == 12 {
                    assert_eq!(line.smooth_sources.len(), bytes.len());
                    let mut native_pen = 0;
                    for (source, byte) in line.smooth_sources.iter().zip(bytes) {
                        let (glyph, _) = systemless::quickdraw::text::get_glyph(3, size, *byte as char).unwrap();
                        assert_eq!(source.0, native_pen);
                        assert!(std::ptr::eq(source.1, glyph));
                        native_pen += i32::from(systemless::quickdraw::text::ppc_styled_run_ink(
                            3, size, face, &[*byte]).0);
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
    smooth_strike_scale: u32,
    smooth_ratio: Option<(u32, u32)>,
    smooth_bold: bool,
    smooth_italic: Option<(i16, i16)>,
    smooth_strokes: Vec<(i32, i32)>,
    smooth_halo: Option<i32>,
    smooth_halo_underlines: Vec<Vec<(i32, i32)>>,
    smooth_halo_classic_everything: bool,
    smooth_y_offset: i32,
    smooth_spacing_adjustment: i32,
    smooth_clamp_advance: bool,
}

impl std::fmt::Debug for ClassicLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ClassicLine").field("positions", &self.positions)
            .field("ink", &self.ink).field("smooth_sources", &self.smooth_sources.len()).finish()
    }
}

impl ClassicLine {
    pub fn smooth_raster_support(&self) -> [bool; 8] {
        std::array::from_fn(|index| resolve_smooth_run(self, index as u32 + 1).is_some())
    }

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
        let mut result = Self { positions: vec![0], ink: Vec::new(), smooth_sources: Vec::new(), smooth_strike_scale: 1, smooth_ratio: None, smooth_bold: false, smooth_italic: None, smooth_strokes: Vec::new(), smooth_halo: None, smooth_halo_underlines: Vec::new(), smooth_halo_classic_everything: false, smooth_y_offset: if face & 16 != 0 { -1 } else { 0 }, smooth_spacing_adjustment: smooth_spacing_adjustment(face), smooth_clamp_advance: false };
        let mut pen = 0;
        for &byte in bytes {
            if smooth_label_style_supported(face) && (face & 2 == 0
                || systemless::quickdraw::text::get_glyph_italic(font, point_size, byte as char).is_none()) {
                if let Some((glyph, data)) = systemless::quickdraw::text::get_glyph(font, point_size, byte as char) {
                    result.smooth_sources.push((pen, glyph, data));
                }
            }
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
        result.smooth_halo = smooth_style_halo(face);
        result.smooth_clamp_advance = true;
        result.smooth_bold = face & 1 != 0;
        result.smooth_italic = (face & 2 != 0).then_some((font, point_size));
        if face & 4 != 0 && pen > 0 {
            for y in 1..=systemless::quickdraw::text::get_underline_thickness(font, point_size).max(1) {
                result.ink.push((0, i32::from(y), pen));
                if smooth_label_style_supported(face) { result.smooth_strokes.extend((0..pen).map(|x| (x, i32::from(y)))); }
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
        let mut smooth_strokes = Vec::new();
        let mut smooth_halo_underlines = Vec::new();
        let (_, strike_scale) = systemless::quickdraw::fonts::get_font_face_scaled(font, point_size);
        let mut pen: i32 = 0;
        for &byte in bytes {
            if smooth_basic_style_supported(face)
                && (face & 2 == 0 || systemless::quickdraw::text::get_glyph_italic(font, point_size, byte as char).is_none()) {
                if let Some((glyph, data)) = systemless::quickdraw::text::get_glyph(font, point_size, byte as char) {
                    smooth_sources.push((pen, glyph, data));
                    if face & 4 != 0 && face & 24 != 0 {
                        smooth_halo_underlines.push(systemless::quickdraw::text::classic_textedit_underline_ink(
                            font, point_size, byte, face)?.into_iter()
                            .map(|(x, y)| (i32::from(x), i32::from(y))).collect());
                    }
                }
            }
            if face < 128 && face & 24 == 0 {
                smooth_strokes.extend(systemless::quickdraw::text::classic_textedit_underline_ink(
                    font, point_size, byte, face & 7)?.into_iter().map(|(x, y)| (pen + i32::from(x), i32::from(y))));
            }
            let (advance, ink) = systemless::quickdraw::text::classic_textedit_glyph_ink(font, point_size, byte, face)?;
            pixels.extend(ink.into_iter().map(|(x, y)| (pen + i32::from(x), i32::from(y))));
            pen = pen.saturating_add(advance);
        }
        pixels.sort_unstable_by_key(|&(x, y)| (y, x));
        pixels.dedup();
        let mut result = Self { positions, ink: Vec::new(), smooth_sources, smooth_strike_scale: u32::try_from(strike_scale).ok()?, smooth_ratio: None, smooth_bold: face & 1 != 0,
            smooth_italic: (face & 2 != 0).then_some((font, point_size)), smooth_strokes,
            smooth_halo: smooth_style_halo(face), smooth_halo_underlines, smooth_halo_classic_everything: face & 31 == 31, smooth_y_offset: 0, smooth_spacing_adjustment: smooth_spacing_adjustment(face), smooth_clamp_advance: false };
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
        let mut smooth_strokes = Vec::new();
        let mut smooth_halo_underlines = Vec::new();

        let (strike, numerator, denominator) = systemless::quickdraw::fonts::get_font_face_scale_ratio(font, point_size);
        let source_advance = bytes.iter().map(|byte| systemless::quickdraw::text::get_glyph(font, strike.size, *byte as char)
            .map_or(6, |(glyph, _)| (i32::from(glyph.advance) + i32::from(face & 1 != 0)
                + i32::from(face & 8 != 0) + 2*i32::from(face & 16 != 0) + smooth_spacing_adjustment(face)).max(1))).sum();
        let halo_ribbon = systemless::quickdraw::text::ppc_styled_run_halo_underline_ink(
            font, strike.size, face, source_advance);
        if smooth_basic_style_supported(face) {
            let mut pen = 0;
            for &byte in bytes {
                if let Some((glyph, data)) = systemless::quickdraw::text::get_glyph(font, strike.size, byte as char) {
                    if face & 2 == 0 || systemless::quickdraw::text::get_glyph_italic(font, strike.size, byte as char).is_none() {
                        smooth_sources.push((pen, glyph, data));
                        if face & 4 != 0 && face & 24 != 0 {
                            smooth_halo_underlines.push(halo_ribbon.iter().map(|&(x, y)| (x - pen, y)).collect());
                        }
                    }
                    pen += (i32::from(glyph.advance) + i32::from(face & 1 != 0)
                        + i32::from(face & 8 != 0) + 2 * i32::from(face & 16 != 0)
                        + smooth_spacing_adjustment(face)).max(1);
                } else { pen += 6; }
            }
            if face & 4 != 0 && face & 24 == 0 {
                for y in 1..=i32::from(systemless::quickdraw::text::get_underline_thickness(font, strike.size).max(1)) {
                    smooth_strokes.extend((0..pen).map(|x| (x, y)));
                }
            }
        }
        if numerator != denominator {
            smooth_strokes = smooth_strokes.into_iter().flat_map(|(x,y)| {
                systemless::quickdraw::text::ppc_font_source_pixel_bounds(x,y,numerator,denominator)
                    .into_iter().flat_map(|(left,top,right,bottom)| (top..bottom).flat_map(move |y| (left..right).map(move |x| (x,y))))
            }).collect();
        }
        pixels.sort_unstable_by_key(|&(x, y)| (y, x));
        let mut result = Self { positions, ink: Vec::new(), smooth_sources, smooth_strike_scale: 1, smooth_ratio: (numerator != denominator).then_some((u32::try_from(numerator).ok()?, u32::try_from(denominator).ok()?)), smooth_bold: face & 1 != 0,
            smooth_italic: (face & 2 != 0).then_some((font, strike.size)), smooth_strokes,
            smooth_halo: smooth_style_halo(face), smooth_halo_underlines, smooth_halo_classic_everything: false, smooth_y_offset: 0, smooth_spacing_adjustment: smooth_spacing_adjustment(face), smooth_clamp_advance: true };
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
            smooth_strike_scale: u32::try_from(scale).unwrap_or(0),
            smooth_ratio: None,
            smooth_bold: false,
            smooth_italic: None,
            smooth_strokes: Vec::new(),
            smooth_halo: None,
            smooth_halo_underlines: Vec::new(),
            smooth_halo_classic_everything: false,
            smooth_y_offset: 0,
            smooth_spacing_adjustment: 0,
            smooth_clamp_advance: false,
        };
        let mut pen = 0;
        for resolved in glyphs {
            if let Some((glyph, data)) = resolved {
                result.smooth_sources.push((pen, glyph, data));
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
        Some((plan.clip, plan.background)), Some(plan.smooth), None)
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

#[derive(Clone, Debug)]
enum StyledTextEditPaintOp {
    Run { glyphs: ClassicLine, left: i16, baseline: i16,
        ink: systemless::runner::TextEditInkSnapshot },
    Invert((i16, i16, i16, i16)),
    Caret((i16, i16, i16, i16), systemless::runner::TextEditInkSnapshot),
}

#[derive(Clone, Debug)]
enum ResolvedTextEditPaintOp {
    Run { glyphs: Vec<(i32, systemless::quickdraw::text::SmoothGlyphSnapshot)>,
        left: i16, baseline: i16, ink: systemless::runner::TextEditInkSnapshot },
    Invert((i16, i16, i16, i16)),
    Caret((i16, i16, i16, i16), systemless::runner::TextEditInkSnapshot),
}

impl StyledTextEditPaintOp {
    /// All operations resolve before a field is painted. Preserve native paint
    /// pens independently of the guest insertion coordinates used by hit testing.
    fn resolve_all(ops: &[Self], raster: u32) -> Option<Vec<ResolvedTextEditPaintOp>> {
        if !(1..=8).contains(&raster) { return None; }
        ops.iter().map(|op| Some(match op {
            Self::Run { glyphs, left, baseline, ink } => ResolvedTextEditPaintOp::Run {
                glyphs: if glyphs.positions == [0] { Vec::new() } else { resolve_smooth_run(glyphs, raster)? },
                left: *left, baseline: *baseline, ink: ink.clone(),
            },
            Self::Invert(rect) => ResolvedTextEditPaintOp::Invert(*rect),
            Self::Caret(rect, ink) => ResolvedTextEditPaintOp::Caret(*rect, ink.clone()),
        })).collect()
    }
}

/// Compose coverage in guest-coordinate subpixels. Both physical ink values
/// are retained because indexed Macintosh inversion is not an RGB complement.
fn smooth_textedit_pixels(
    ops: &[StyledTextEditPaintOp], raster: u32, view: (i16, i16, i16, i16),
    background: &systemless::runner::TextEditInkSnapshot,
) -> Option<std::collections::BTreeMap<(i32, i32), [u8; 3]>> {
    let resolved = StyledTextEditPaintOp::resolve_all(ops, raster)?;
    let r = i32::try_from(raster).ok()?;
    let (top, left, bottom, right) = view;
    if top >= bottom || left >= right { return None; }
    let clip = (i32::from(top) * r, i32::from(left) * r,
        i32::from(bottom) * r, i32::from(right) * r);
    let base = (background.rgb, background.inverted_rgb);
    let mut pixels = std::collections::BTreeMap::<(i32, i32), ([u8; 3], [u8; 3])>::new();
    fn blend(old: [u8; 3], ink: [u8; 3], alpha: u8) -> [u8; 3] {
        std::array::from_fn(|i| ((u32::from(old[i]) * (255 - u32::from(alpha))
            + u32::from(ink[i]) * u32::from(alpha) + 127) / 255) as u8)
    }
    for op in resolved {
        match op {
            ResolvedTextEditPaintOp::Run { glyphs, left, baseline, ink } => {
                for (pen, glyph) in glyphs {
                    let origin_x = (i32::from(left).checked_add(pen)?).checked_mul(r)?.checked_add(glyph.left)?;
                    let origin_y = i32::from(baseline).checked_mul(r)?.checked_add(glyph.top)?;
                    for y in 0..glyph.height { for x in 0..glyph.width {
                        let at = (origin_x.checked_add(x)?, origin_y.checked_add(y)?);
                        if at.0 < clip.1 || at.0 >= clip.3 || at.1 < clip.0 || at.1 >= clip.2 { continue; }
                        let alpha = glyph.pixels[(y * glyph.width + x) as usize];
                        if alpha == 0 { continue; }
                        let pair = pixels.entry(at).or_insert(base);
                        pair.0 = blend(pair.0, ink.rgb, alpha);
                        pair.1 = blend(pair.1, ink.inverted_rgb, alpha);
                    } }
                }
            }
            op => {
                let (rect, caret) = match op {
                    ResolvedTextEditPaintOp::Invert(rect) => (rect, None),
                    ResolvedTextEditPaintOp::Caret(rect, ink) => (rect, Some(ink)),
                    ResolvedTextEditPaintOp::Run { .. } => unreachable!(),
                };
                let (top, left, bottom, right) = rect;
                for y in (i32::from(top) * r).max(clip.0)..(i32::from(bottom) * r).min(clip.2) {
                    for x in (i32::from(left) * r).max(clip.1)..(i32::from(right) * r).min(clip.3) {
                        let pair = pixels.entry((x, y)).or_insert(base);
                        if let Some(ink) = &caret { *pair = (ink.rgb, ink.inverted_rgb); }
                        else { std::mem::swap(&mut pair.0, &mut pair.1); }
                    }
                }
            }
        }
    }
    Some(pixels.into_iter().map(|(at, pair)| (at, pair.0)).collect())
}

/// A whole-field recipe, qualified against native pixels before ownership.
/// The caller supplies resolved background and caret paint; no host font or
/// theme colour is inferred. Application drawing or unsupported paint declines.
#[derive(Clone, Debug)]
pub(crate) struct StyledTextEditPaintPlan {
    pub pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    pub background: [u8; 3],
    pub view: (i16, i16, i16, i16),
    ops: Vec<StyledTextEditPaintOp>,
    background_ink: systemless::runner::TextEditInkSnapshot,
}

impl StyledTextEditPaintPlan {
    /// Capture diagnostics only: this preflight does not assert that the
    /// composed field painted or that its appearance has been reviewed.
    pub fn smooth_raster_support(&self) -> [bool; 8] {
        std::array::from_fn(|index| StyledTextEditPaintOp::resolve_all(&self.ops, index as u32 + 1).is_some())
    }

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
        let mut ops = Vec::new();
        let mut selections = Vec::new();
        for index in 0..record.line_count {
            let (geometry, _) = record.guest_styled_line_geometry(index)?;
            if record.line_layout_policy == TextEditLineLayoutPolicy::CumulativeGuestMetrics
                && (geometry.top.saturating_add(geometry.height) <= view.0 || geometry.top >= view.2) { continue; }
            let line = StyledTextEditLine::from_guest(record, index)?;
            for run in &line.runs {
                ops.push(StyledTextEditPaintOp::Run { glyphs: run.glyphs.clone(),
                    left: run.left, baseline: line.baseline, ink: run.ink.clone() });
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
                    ops.push(StyledTextEditPaintOp::Invert(rect));
                } else { selections.push(rect); }
            }
        }
        // Classic highlights after all visible line ink. Every inversion
        // swaps the actual current physical ink pair, including prior highlights.
        for rect in selections {
            invert(&mut pairs, rect, background);
            ops.push(StyledTextEditPaintOp::Invert(rect));
        }
        let mut pixels: std::collections::BTreeMap<_, _> = pairs.into_iter()
            .map(|(point, (rgb, _))| (point, rgb)).collect();
        if let Some((rect, ink)) = caret {
            if !record.active || !record.caret_visible || record.selection.0 != record.selection.1 { return None; }
            let (top, left, bottom, right) = rect;
            if top >= bottom || left >= right || top < view.0 || left < view.1
                || bottom > view.2 || right > view.3 { return None; }
            for y in top..bottom { for x in left..right { pixels.insert((x, y), ink.rgb); } }
            ops.push(StyledTextEditPaintOp::Caret(rect, ink));
        }
        for y in view.0..view.2 { for x in view.1..view.3 {
            let gx = i32::from(global.1) + i32::from(x) - i32::from(view.1);
            let gy = i32::from(global.0) + i32::from(y) - i32::from(view.0);
            let at = (gy as usize * width as usize + gx as usize) * 4;
            if native[at..at + 3] != pixels.get(&(x, y)).copied().unwrap_or(background.rgb) { return None; }
        } }
        Some(Self { pixels, background: background.rgb, view, ops, background_ink: background.clone() })
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
    classic_text_pixels_with_smooth(plan.pixels, scale, port_origin,
        Some((plan.view, plan.background)), None,
        Some((plan.ops, plan.view, plan.background_ink)))
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
    classic_text_pixels_with_smooth(pixels, scale, port_origin, background, None, None)
}

fn classic_text_pixels_with_smooth(
    pixels: std::collections::BTreeMap<(i16, i16), [u8; 3]>,
    scale: f32, port_origin: (f32, f32),
    background: Option<((i16, i16, i16, i16), [u8; 3])>,
    smooth: Option<(ClassicLine, i16, i16, [u8; 3])>,
    field: Option<(Vec<StyledTextEditPaintOp>, (i16, i16, i16, i16), systemless::runner::TextEditInkSnapshot)>,
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
        if let Some((ops, view, background)) = &field {
            let raster = (scale * device_scale).ceil().max(1.) as u32;
            if let Some(pixels) = smooth_textedit_pixels(ops, raster, *view, background) {
                let unit = scale / raster as f32;
                let mut coverage_paths = std::collections::BTreeMap::new();
                // Coalesce only equal resolved RGB coverage. Guest composition,
                // clipping, selection inversion and caret order are already final.
                for span in super::coverage::row_spans(pixels) {
                    let color = span.color;
                    let left = snap(port_origin.0 + span.left as f32 * unit);
                    let top = snap(port_origin.1 + span.y as f32 * unit);
                    let right = snap(port_origin.0 + span.right as f32 * unit);
                    let bottom = snap(port_origin.1 + (i64::from(span.y) + 1) as f32 * unit);
                    if right <= left || bottom <= top { continue; }
                    let path = coverage_paths.entry(color).or_insert_with(PathBuilder::fill);
                    path.move_to(point(left, top)); path.line_to(point(right, top));
                    path.line_to(point(right, bottom)); path.line_to(point(left, bottom)); path.close();
                }
                for ([r, g, b], path) in coverage_paths {
                    let ink: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
                    window.paint_path(path.build().expect("resolved styled field coverage"), ink);
                }
                return;
            }
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
    for stroke in &mut line.smooth_strokes { stroke.0 -= left; }
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
    if !line.smooth_halo_underlines.is_empty() && line.smooth_halo_underlines.len() != line.smooth_sources.len() {
        return None;
    }
    let mut glyphs = line.smooth_sources.iter().enumerate().map(|(index, &(source_pen, glyph, data))| {
        let styled_ratio = line.smooth_ratio.filter(|_| line.smooth_bold || line.smooth_italic.is_some()
            || line.smooth_halo.is_some() || !line.smooth_halo_underlines.is_empty() || line.smooth_spacing_adjustment != 0);
        let source_density = if let Some((numerator,denominator)) = styled_ratio {
            let required = numerator.checked_mul(raster)?.div_ceil(denominator);
            required.max(8).div_ceil(8).checked_mul(8)?
        } else { raster };
        let (pen, mut mask) = if styled_ratio.is_some() {
            (source_pen, systemless::quickdraw::text::smooth_resolved_glyph_at_density(glyph,data,source_density)?)
        } else if let Some((numerator, denominator)) = line.smooth_ratio {
            systemless::quickdraw::text::smooth_resolved_ratio_glyph(glyph, data, numerator, denominator, source_pen, raster)?
        } else {
            (source_pen, systemless::quickdraw::text::smooth_resolved_scaled_glyph(glyph, data, line.smooth_strike_scale, raster)?)
        };
        if let Some((font, size)) = line.smooth_italic { mask = smooth_italic_mask(mask, font, size)?; }
        if line.smooth_bold { mask = smooth_bold_mask(mask)?; }
        mask.top = mask.top.checked_add(line.smooth_y_offset.checked_mul(i32::try_from(source_density).ok()?)?)?;
        if let Some(strokes) = line.smooth_halo_underlines.get(index) { mask = smooth_union_strokes(mask, strokes)?; }
        if let Some(radius) = line.smooth_halo { mask = smooth_halo_mask_with_policy(mask, radius, line.smooth_halo_classic_everything)?; }
        mask.guest_advance = mask.guest_advance.checked_add(line.smooth_spacing_adjustment)?;
        if line.smooth_clamp_advance { mask.guest_advance = mask.guest_advance.max(1); }
        if let Some((numerator,denominator)) = styled_ratio {
            transform_source_mask(mask,numerator,denominator,source_pen,raster)
        } else { Some((pen,mask)) }
    }).collect::<Option<Vec<_>>>()?;
    if !line.smooth_strokes.is_empty() {
        let r = i32::try_from(raster).ok()?;
        let left = line.smooth_strokes.iter().map(|p| p.0).min()?.checked_mul(r)?;
        let top = line.smooth_strokes.iter().map(|p| p.1).min()?.checked_mul(r)?;
        let right = line.smooth_strokes.iter().map(|p| p.0).max()?.checked_add(1)?.checked_mul(r)?;
        let bottom = line.smooth_strokes.iter().map(|p| p.1).max()?.checked_add(1)?.checked_mul(r)?;
        let width = right.checked_sub(left)?;
        let height = bottom.checked_sub(top)?;
        let mut pixels = vec![0; usize::try_from(width.checked_mul(height)?).ok()?];
        for &(x, y) in &line.smooth_strokes {
            for dy in 0..r { for dx in 0..r {
                pixels[((y * r + dy - top) * width + x * r + dx - left) as usize] = 255;
            } }
        }
        glyphs.push((0, systemless::quickdraw::text::SmoothGlyphSnapshot {
            pixels: pixels.into(), width, height, left, top, guest_advance: 0, raster_scale: raster,
        }));
    }
    Some(glyphs)
}

/// Area coverage transforms the original outlined/style mask after synthesis.
/// Source pen phase is part of the transform; guest insertion widths never enter it.
fn transform_source_mask(mask: systemless::quickdraw::text::SmoothGlyphSnapshot,
    numerator: u32, denominator: u32, source_pen: i32, raster: u32)
    -> Option<(i32, systemless::quickdraw::text::SmoothGlyphSnapshot)> {
    if numerator == 0 || denominator == 0 || !(1..=8).contains(&raster) || mask.raster_scale == 0 { return None; }
    let density = i64::from(mask.raster_scale);
    let quantum = i64::from(denominator).checked_mul(density)?;
    let factor = i64::from(numerator).checked_mul(i64::from(raster))?;
    let pen = i64::from(source_pen).checked_mul(i64::from(numerator))?.div_euclid(i64::from(denominator));
    let origin_x = i64::from(mask.left).checked_add(i64::from(source_pen).checked_mul(density)?)?
        .checked_mul(factor)?.checked_sub(pen.checked_mul(i64::from(raster))?.checked_mul(quantum)?)?;
    let origin_y = i64::from(mask.top).checked_mul(factor)?;
    let end_x = origin_x.checked_add(i64::from(mask.width).checked_mul(factor)?)?;
    let end_y = origin_y.checked_add(i64::from(mask.height).checked_mul(factor)?)?;
    let left = origin_x.div_euclid(quantum); let top = origin_y.div_euclid(quantum);
    let right = end_x.checked_add(quantum-1)?.div_euclid(quantum);
    let bottom = end_y.checked_add(quantum-1)?.div_euclid(quantum);
    let width = right.checked_sub(left)?; let height = bottom.checked_sub(top)?;
    let count = usize::try_from(width.checked_mul(height)?).ok()?;
    if count > 16_777_216 || mask.width < 0 || mask.height < 0 { return None; }
    let mut coverage = vec![0u64; count];
    for y in 0..i64::from(mask.height) { for x in 0..i64::from(mask.width) {
        let alpha = u64::from(*mask.pixels.get(usize::try_from(y*i64::from(mask.width)+x).ok()?)?);
        if alpha == 0 { continue; }
        let x0 = origin_x + x*factor; let x1 = x0+factor;
        let y0 = origin_y + y*factor; let y1 = y0+factor;
        for dy in y0.div_euclid(quantum)..(y1+quantum-1).div_euclid(quantum) {
            let wy = u64::try_from(y1.min((dy+1)*quantum)-y0.max(dy*quantum)).ok()?;
            for dx in x0.div_euclid(quantum)..(x1+quantum-1).div_euclid(quantum) {
                let wx = u64::try_from(x1.min((dx+1)*quantum)-x0.max(dx*quantum)).ok()?;
                let index = usize::try_from((dy-top)*width+dx-left).ok()?;
                coverage[index] = coverage[index].checked_add(alpha.checked_mul(wx)?.checked_mul(wy)?)?;
            }
        }
    } }
    let area = u64::try_from(quantum.checked_mul(quantum)?).ok()?;
    let pixels: Vec<u8> = coverage.into_iter().map(|value| ((value+area/2)/area).min(255) as u8).collect();
    let advance = (i64::from(mask.guest_advance)*i64::from(numerator)+i64::from(denominator)/2)/i64::from(denominator);
    Some((i32::try_from(pen).ok()?, systemless::quickdraw::text::SmoothGlyphSnapshot { pixels: pixels.into(),
        width: i32::try_from(width).ok()?, height: i32::try_from(height).ok()?, left: i32::try_from(left).ok()?,
        top: i32::try_from(top).ok()?, guest_advance: i32::try_from(advance).ok()?, raster_scale: raster }))
}

fn smooth_label_style_supported(face: u8) -> bool {
    face < 128
}

fn smooth_spacing_adjustment(face: u8) -> i32 {
    -i32::from(face & 0x20 != 0) + i32::from(face & 0x40 != 0)
}

fn smooth_basic_style_supported(face: u8) -> bool {
    face < 128
}

fn smooth_style_halo(face: u8) -> Option<i32> {
    match (face & 8 != 0, face & 16 != 0) {
        (true, true) => Some(3), (false, true) => Some(2),
        (true, false) => Some(1), _ => None,
    }
}

/// Combine native underline strokes with the original-source glyph before
/// halo smear/exclusion. Keep the guest advance and baseline unchanged.
fn smooth_union_strokes(
    mut glyph: systemless::quickdraw::text::SmoothGlyphSnapshot, strokes: &[(i32, i32)],
) -> Option<systemless::quickdraw::text::SmoothGlyphSnapshot> {
    if strokes.is_empty() { return Some(glyph); }
    let r = i32::try_from(glyph.raster_scale).ok()?;
    if r <= 0 { return None; }
    let mut left = strokes.iter().map(|p| p.0).min()?.checked_mul(r)?;
    let mut top = strokes.iter().map(|p| p.1).min()?.checked_mul(r)?;
    let mut right = strokes.iter().map(|p| p.0).max()?.checked_add(1)?.checked_mul(r)?;
    let mut bottom = strokes.iter().map(|p| p.1).max()?.checked_add(1)?.checked_mul(r)?;
    if glyph.width > 0 && glyph.height > 0 {
        left = left.min(glyph.left); top = top.min(glyph.top);
        right = right.max(glyph.left.checked_add(glyph.width)?);
        bottom = bottom.max(glyph.top.checked_add(glyph.height)?);
    }
    let width = right.checked_sub(left)?;
    let height = bottom.checked_sub(top)?;
    let mut pixels = vec![0u8; usize::try_from(width.checked_mul(height)?).ok()?];
    for y in 0..glyph.height { for x in 0..glyph.width {
        let at = ((glyph.top + y - top) * width + glyph.left + x - left) as usize;
        pixels[at] = glyph.pixels[(y * glyph.width + x) as usize];
    } }
    for &(x, y) in strokes { for dy in 0..r { for dx in 0..r {
        let at = ((y.checked_mul(r)?.checked_add(dy)? - top) * width
            + x.checked_mul(r)?.checked_add(dx)? - left) as usize;
        pixels[at] = 255;
    } } }
    glyph.left = left; glyph.top = top; glyph.width = width; glyph.height = height;
    glyph.pixels = pixels.into();
    Some(glyph)
}

/// Outline/shadow smear from (-1,-1) through the native radius, then remove
/// original coverage to retain the hollow silhouette. Binary input reproduces
/// native set/exclude ink; fractional coverage keeps both boundaries smooth.
fn smooth_halo_mask(
    glyph: systemless::quickdraw::text::SmoothGlyphSnapshot, radius: i32,
) -> Option<systemless::quickdraw::text::SmoothGlyphSnapshot> {
    smooth_halo_mask_with_policy(glyph, radius, false)
}

fn smooth_halo_mask_with_policy(
    mut glyph: systemless::quickdraw::text::SmoothGlyphSnapshot, radius: i32,
    classic_everything: bool,
) -> Option<systemless::quickdraw::text::SmoothGlyphSnapshot> {
    if !(1..=3).contains(&radius) { return None; }
    let r = i32::try_from(glyph.raster_scale).ok()?;
    if r <= 0 { return None; }
    if glyph.width == 0 || glyph.height == 0 {
        glyph.guest_advance = glyph.guest_advance.checked_add(radius)?;
        return Some(glyph);
    }
    let width = glyph.width.checked_add((radius + 1).checked_mul(r)?)?;
    let height = glyph.height.checked_add((radius + 1).checked_mul(r)?)?;
    let mut pixels = vec![0u8; usize::try_from(width.checked_mul(height)?).ok()?];
    for y in 0..glyph.height { for x in 0..glyph.width {
        let alpha = glyph.pixels[(y * glyph.width + x) as usize];
        for dy in -1..=radius { for dx in -1..=radius {
            let output_y = glyph.top.checked_add(y)?.checked_add(dy.checked_mul(r)?)?;
            if classic_everything && matches!(output_y.div_euclid(r), 0 | 1) && dx > 1 { continue; }
            let at = ((y + (dy + 1) * r) * width + x + (dx + 1) * r) as usize;
            pixels[at] = pixels[at].max(alpha);
        } }
    } }
    for y in 0..glyph.height { for x in 0..glyph.width {
        let at = ((y + r) * width + x + r) as usize;
        pixels[at] = pixels[at].saturating_sub(glyph.pixels[(y * glyph.width + x) as usize]);
    } }
    glyph.left = glyph.left.checked_sub(r)?;
    glyph.top = glyph.top.checked_sub(r)?;
    glyph.width = width; glyph.height = height; glyph.pixels = pixels.into();
    glyph.guest_advance = glyph.guest_advance.checked_add(radius)?;
    Some(glyph)
}

/// Preserve native shear at raster1. At higher resolution, use its underlying
/// half-pixel slope with coverage interpolation inside the same native envelope.
/// The original outline, baseline/descent pivot and guest advance stay intact.
fn smooth_italic_mask(
    mut glyph: systemless::quickdraw::text::SmoothGlyphSnapshot, font: i16, size: i16,
) -> Option<systemless::quickdraw::text::SmoothGlyphSnapshot> {
    let raster = i32::try_from(glyph.raster_scale).ok()?;
    if raster <= 0 { return None; }
    let metrics = systemless::quickdraw::text::get_font_metrics(font, size);
    let shifts: Vec<i32> = (0..glyph.height).map(|row| {
        let guest_y = (glyph.top.checked_add(row)?).div_euclid(raster);
        let slant = systemless::quickdraw::fonts::style::get_italic_slant(
            font, size, &metrics, 0, i16::try_from(guest_y).ok()?);
        i32::from(slant).checked_mul(raster)
    }).collect::<Option<_>>()?;
    let min = shifts.iter().copied().min().unwrap_or(0);
    let max = shifts.iter().copied().max().unwrap_or(0);
    let width = glyph.width.checked_add(max.checked_sub(min)?)?;
    let mut pixels = vec![0u8; usize::try_from(width.checked_mul(glyph.height)?).ok()?];
    for (y, shift) in shifts.into_iter().enumerate() {
        let twice = if raster == 1 { shift.checked_mul(2)? } else {
            let pivot = i32::from(metrics.descent).checked_sub(1)?.checked_mul(raster)?;
            pivot.checked_sub(glyph.top.checked_add(i32::try_from(y).ok()?)?)?.max(0)
                .clamp(min.checked_mul(2)?, max.checked_mul(2)?)
        };
        let shift = twice.div_euclid(2);
        let half = twice.rem_euclid(2) != 0;
        for x in 0..glyph.width {
            let alpha = glyph.pixels[y * glyph.width as usize + x as usize];
            let at = y * width as usize + (x + shift - min) as usize;
            let first = if half { alpha / 2 } else { alpha };
            pixels[at] = pixels[at].saturating_add(first);
            if half { pixels[at + 1] = pixels[at + 1].saturating_add(alpha - first); }
        }
    }
    glyph.left = glyph.left.checked_add(min)?;
    glyph.width = width;
    glyph.pixels = pixels.into();
    Some(glyph)
}

/// QuickDraw bold uses max(original, one guest pixel to the right), rather
/// than substituting a host bold face with different outlines and metrics.
fn smooth_bold_mask(
    mut glyph: systemless::quickdraw::text::SmoothGlyphSnapshot,
) -> Option<systemless::quickdraw::text::SmoothGlyphSnapshot> {
    let shift = i32::try_from(glyph.raster_scale).ok()?;
    let width = glyph.width.checked_add(shift)?;
    let mut pixels = vec![0; usize::try_from(width.checked_mul(glyph.height)?).ok()?];
    for y in 0..glyph.height { for x in 0..glyph.width {
        let alpha = glyph.pixels[(y * glyph.width + x) as usize];
        for dx in [0, shift] {
            let at = (y * width + x + dx) as usize;
            pixels[at] = pixels[at].max(alpha);
        }
    } }
    glyph.pixels = pixels.into();
    glyph.width = width;
    glyph.guest_advance = glyph.guest_advance.checked_add(1)?;
    Some(glyph)
}

/// Outline coverage changes ink only; guest advances and baseline remain authoritative.
fn paint_smooth_label(
    line: &ClassicLine, left: gpui_kit::Pixels, baseline: gpui_kit::Pixels,
    scale: f32, foreground: gpui_kit::Hsla, window: &mut gpui_kit::Window,
) -> bool {
    use gpui_kit::*;
    if line.smooth_sources.is_empty() || line.smooth_sources.len() + 1 != line.positions.len() { return false; }
    // Paint pens can be translated for negative menu bearings independently
    // of insertion advances. Resolution validates the complete source run;
    // equating the two coordinate roles would silently reject styled labels.
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
