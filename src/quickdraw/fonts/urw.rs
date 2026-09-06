//! Experimental bundled outlines. See urw/README.md for provenance.
use super::*;

type Faces = (&'static FontFace, &'static MacRomanFace);
static FACES: LazyLock<Mutex<HashMap<(i16, i16), Faces>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn bytes(font_id: i16) -> Option<&'static [u8]> {
    Some(match font_id {
        FONT_CHICAGO => include_bytes!("urw/NimbusSans-Bold.ttf"),
        FONT_APPLICATION | FONT_GENEVA | FONT_HELVETICA => {
            include_bytes!("urw/NimbusSans-Regular.ttf")
        }
        FONT_MONACO | FONT_COURIER => include_bytes!("urw/NimbusMonoPS-Regular.ttf"),
        FONT_NEWYORK | FONT_TIMES => include_bytes!("urw/NimbusRoman-Regular.ttf"),
        FONT_PALATINO => include_bytes!("urw/P052-Roman.ttf"),
        FONT_VENICE => include_bytes!("urw/Z003-MediumItalic.ttf"),
        FONT_LONDON => include_bytes!("urw/C059-Bold.ttf"),
        FONT_CAIRO => include_bytes!("urw/URWGothic-Demi.ttf"),
        _ => return None,
    })
}

pub(super) fn face(font_id: i16, size: i16) -> Option<Faces> {
    // Bound permanent cached storage and keep bearings within Glyph's i8
    // representation. Larger requests retain the existing bitmap scaling.
    if !(1..=96).contains(&size) {
        return None;
    }
    let bytes = bytes(font_id)?;
    let mut cache = FACES.lock().expect("URW font cache poisoned");
    if let Some(faces) = cache.get(&(font_id, size)) {
        return Some(*faces);
    }
    let faces = rasterize(font_id, size, bytes)?;
    cache.insert((font_id, size), faces);
    Some(faces)
}

#[derive(Default)]
struct OutlinePath(Vec<zeno::Command>);

impl skrifa::outline::OutlinePen for OutlinePath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::MoveTo((x, y).into()));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::LineTo((x, y).into()));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0
            .push(zeno::Command::QuadTo((cx, cy).into(), (x, y).into()));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(zeno::Command::CurveTo(
            (cx0, cy0).into(),
            (cx1, cy1).into(),
            (x, y).into(),
        ));
    }
    fn close(&mut self) {
        self.0.push(zeno::Command::Close);
    }
}

// Inside Macintosh: Text (1993), pp. 4-7–4-9 and 4-18–4-19:
// outline fonts generate a strike at the requested point size. At the logical
// 72-dpi screen, one point is one em pixel. Keep QuickDraw's binary masks and
// boolean transfer modes; hint before thresholding to retain small stems.
fn rasterize(font_id: i16, size: i16, bytes: &'static [u8]) -> Option<Faces> {
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{DrawSettings, HintingInstance, Target},
        FontRef, MetadataProvider,
    };
    let font = FontRef::new(bytes).ok()?;
    let ppem = Size::new(f32::from(size));
    let location = LocationRef::default();
    let metrics = font.metrics(ppem, location);
    let advances = font.glyph_metrics(ppem, location);
    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    // The output is a one-bit QuickDraw mask. LCD hinting preserves fractional
    // stem positions and therefore loses strokes when thresholded. Mono fits
    // both axes to the pixel grid and returns matching adjusted advances.
    let hinter = HintingInstance::new(&outlines, ppem, location, Target::Mono).ok()?;
    let mut data = Vec::new();
    let mut glyph = |ch: char| {
        let id = charmap.map(ch).unwrap_or_default();
        let mut path = OutlinePath::default();
        let adjusted = outlines.get(id).and_then(|outline| {
            outline
                .draw(DrawSettings::hinted(&hinter, false), &mut path)
                .ok()
        });
        let advance = adjusted
            .and_then(|metrics| metrics.advance_width)
            .or_else(|| advances.advance_width(id))
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let mut result = Glyph {
            width: 0,
            height: 0,
            advance,
            origin_x: 0,
            origin_y: 0,
            data_offset: data.len(),
        };
        if !path.0.is_empty() {
            let mut coverage = Vec::new();
            let placement = zeno::Mask::new(path.0.as_slice())
                .origin(zeno::Origin::BottomLeft)
                .inspect(|format, width, height| {
                    coverage.resize(format.buffer_size(width, height), 0)
                })
                .render_into(&mut coverage, None);
            result.width = u8::try_from(placement.width).expect("bounded URW glyph width");
            result.height = u8::try_from(placement.height).expect("bounded URW glyph height");
            result.origin_x = i8::try_from(placement.left).expect("bounded URW left bearing");
            result.origin_y = i8::try_from(-placement.top).expect("bounded URW top bearing");
            data.extend(coverage.iter().map(|&alpha| {
                if alpha >= MONO_COVERAGE_THRESHOLD {
                    255
                } else {
                    0
                }
            }));
        }
        result
    };
    let ascii = (0x20u8..=0x7e)
        .map(|code| glyph(char::from(code)))
        .collect::<Vec<_>>();
    let extended = (0x80u8..=0xff)
        .map(|code| MacRomanGlyph {
            mac_code: code,
            glyph: glyph(
                crate::mac_roman::decode_mac_roman(&[code])
                    .chars()
                    .next()
                    .unwrap(),
            ),
        })
        .collect::<Vec<_>>();
    let all = || {
        ascii
            .iter()
            .chain(extended.iter().map(|entry| &entry.glyph))
    };
    let ascent = (metrics.ascent.ceil() as i16)
        .max(all().map(|g| -i16::from(g.origin_y)).max().unwrap_or(0));
    let descent = ((-metrics.descent).ceil() as i16).max(
        all()
            .map(|g| i16::from(g.origin_y) + i16::from(g.height))
            .max()
            .unwrap_or(0),
    );
    let wid_max = all().map(|g| i16::from(g.advance)).max().unwrap_or(0);
    let data = Box::leak(data.into_boxed_slice());
    let face = Box::leak(Box::new(FontFace {
        font_id,
        size,
        metrics: FontMetrics {
            ascent,
            descent,
            wid_max,
            leading: metrics.leading.round().max(0.0) as i16,
        },
        glyphs: Box::leak(ascii.into_boxed_slice()),
        data,
    }));
    let extended = Box::leak(Box::new(MacRomanFace {
        font_id,
        size,
        glyphs: Box::leak(extended.into_boxed_slice()),
        data,
    }));
    Some((face, extended))
}

/// Resolve only glyphs actually supplied by this fallback, preserving resource fonts.
pub(crate) fn presentation_glyph(
    glyph: &Glyph,
    data: &[u8],
    scale: u32,
) -> Option<crate::memory::presentation::OutlineGlyph> {
    let cache = FACES.lock().ok()?;
    for (&(family, size), &(face, extended)) in cache.iter() {
        if face.data.as_ptr() != data.as_ptr() {
            continue;
        }
        let ch = face
            .glyphs
            .iter()
            .position(|g| std::ptr::eq(g, glyph))
            .map(|i| char::from(i as u8 + 32))
            .or_else(|| {
                extended
                    .glyphs
                    .iter()
                    .find(|g| std::ptr::eq(&g.glyph, glyph))
                    .map(|g| {
                        crate::mac_roman::decode_mac_roman(&[g.mac_code])
                            .chars()
                            .next()
                            .unwrap()
                    })
            })?;
        use skrifa::{
            instance::{LocationRef, Size},
            outline::{DrawSettings, HintingInstance, SmoothMode, Target},
            FontRef, MetadataProvider,
        };
        let font = FontRef::new(bytes(family)?).ok()?;
        let outlines = font.outline_glyphs();
        let hint = HintingInstance::new(
            &outlines,
            Size::new(size as f32 * scale as f32),
            LocationRef::default(),
            Target::from(SmoothMode::Normal),
        )
        .ok()?;
        let mut path = OutlinePath::default();
        outlines
            .get(font.charmap().map(ch)?)?
            .draw(DrawSettings::hinted(&hint, false), &mut path)
            .ok()?;
        let mut pixels = Vec::new();
        let placement = zeno::Mask::new(path.0.as_slice())
            .origin(zeno::Origin::BottomLeft)
            .inspect(|format, w, h| pixels.resize(format.buffer_size(w, h), 0))
            .render_into(&mut pixels, None);
        return Some(crate::memory::presentation::OutlineGlyph {
            pixels,
            width: placement.width as i32,
            height: placement.height as i32,
            left: placement.left,
            top: -placement.top,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_supply_exact_sizes_and_extended_mac_roman() {
        for family in [
            FONT_CHICAGO,
            FONT_GENEVA,
            FONT_MONACO,
            FONT_NEWYORK,
            FONT_PALATINO,
            FONT_VENICE,
            FONT_LONDON,
            FONT_CAIRO,
        ] {
            for size in [9, 12, 17, 24, 40, 96] {
                let (face, extended) = super::face(family, size).unwrap();
                assert_eq!(face.size, size);
                assert_eq!(face.glyphs.len(), 95);
                assert_eq!(extended.glyphs.len(), 128);
                assert!(face.data.iter().all(|&value| value == 0 || value == 255));
                for glyph in face
                    .glyphs
                    .iter()
                    .chain(extended.glyphs.iter().map(|entry| &entry.glyph))
                {
                    assert!(
                        glyph.data_offset + usize::from(glyph.width) * usize::from(glyph.height)
                            <= face.data.len()
                    );
                }
                assert!(std::ptr::eq(face, super::face(family, size).unwrap().0));
            }
        }
        let (face, numerator, denominator) = get_font_face_scale_ratio(FONT_GENEVA, 17);
        assert_eq!((face.size, numerator, denominator), (17, 17, 17));
        let (accent, data) = get_macroman_glyph(FONT_GENEVA, 17, 0x8e).unwrap(); // é
        assert!(data[accent.data_offset
            ..accent.data_offset + usize::from(accent.width) * usize::from(accent.height)]
            .iter()
            .any(|&v| v > 0));
    }

    #[test]
    fn monochrome_small_text_keeps_stems_counters_and_baselines() {
        for family in [FONT_GENEVA, FONT_MONACO] {
            for size in 9..=12 {
                let (face, _) = super::face(family, size).unwrap();
                let h = &face.glyphs[(b'H' - b' ') as usize];
                let w = usize::from(h.width);
                let pixels = &face.data[h.data_offset..h.data_offset + w * usize::from(h.height)];
                let rows = pixels
                    .chunks_exact(w)
                    .filter(|row| row.iter().any(|&v| v != 0))
                    .collect::<Vec<_>>();
                let stems = (0..w)
                    .filter(|&x| rows.iter().all(|row| row[x] == 255))
                    .count();
                assert!(
                    stems >= 2,
                    "{family}/{size}: H must have two unbroken stems"
                );
                assert!(h.origin_y < 0);
                assert!(
                    i16::from(h.origin_y) + i16::from(h.height) <= 1,
                    "H must sit above the baseline"
                );

                let o = &face.glyphs[(b'o' - b' ') as usize];
                let w = usize::from(o.width);
                let h = usize::from(o.height);
                let pixels = &face.data[o.data_offset..o.data_offset + w * h];
                let mut outside = vec![false; pixels.len()];
                let mut queue = (0..pixels.len())
                    .filter(|&i| i < w || i >= w * (h - 1) || i % w == 0 || i % w == w - 1)
                    .collect::<Vec<_>>();
                while let Some(i) = queue.pop() {
                    if outside[i] || pixels[i] != 0 {
                        continue;
                    }
                    outside[i] = true;
                    if i >= w {
                        queue.push(i - w);
                    }
                    if i + w < pixels.len() {
                        queue.push(i + w);
                    }
                    if i % w != 0 {
                        queue.push(i - 1);
                    }
                    if i % w + 1 < w {
                        queue.push(i + 1);
                    }
                }
                assert!(
                    pixels
                        .iter()
                        .zip(outside)
                        .any(|(&ink, outside)| ink == 0 && !outside),
                    "{family}/{size}: o must retain its enclosed counter"
                );
            }
        }
    }

    #[test]
    fn monospaced_advances_and_em_scale_are_preserved() {
        let (face, _) = super::face(FONT_COURIER, 20).unwrap();
        assert!(face.glyphs.iter().all(|glyph| glyph.advance == 12)); // 600/1000 em
        assert!(super::face(FONT_GENEVA, 97).is_none());
        assert!(super::face(30000, 12).is_none());
    }

    #[test]
    fn guest_outline_wins_for_an_extended_character_drawn_first() {
        assert!(register_resource_outline_font(
            30002,
            include_bytes!("urw/NimbusMonoPS-Regular.ttf")
        ));
        let (accent, _) = get_macroman_glyph(30002, 23, 0x8e).unwrap();
        let face = get_font_face(30002, 23).unwrap();
        assert_eq!(accent.advance, face.glyphs[0].advance);
        assert!(face
            .glyphs
            .iter()
            .all(|glyph| glyph.advance == accent.advance));
        // Registration is process-global, so use a unique compatibility family
        // for this test instead of changing a family shared by other tests.
    }

    #[test]
    fn guest_bitmap_strike_wins_even_after_urw_is_cached() {
        super::face(FONT_COURIER, 19).unwrap();
        // Keep this family/size unique so parallel tests do not share a strike.
        assert!(register_resource_font_strike_for_family(
            FONT_COURIER,
            19,
            &super::super::tests::minimal_nfnt()
        ));
        let face = get_font_face(FONT_COURIER, 19).unwrap();
        assert_eq!(face.metrics.ascent, 1);
        assert_eq!(face.glyphs[0].advance, 1);
    }
}
