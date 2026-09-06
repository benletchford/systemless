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

// Inside Macintosh: Text (1993), pp. 4-7–4-9 and 4-18–4-19:
// outline fonts generate a strike at the requested point size. At the logical
// 72-dpi screen, one point is one em pixel. Keep QuickDraw's binary masks and
// boolean transfer modes; hint before thresholding to retain small stems.
fn rasterize(font_id: i16, size: i16, bytes: &'static [u8]) -> Option<Faces> {
    use swash::scale::{Render, ScaleContext, Source};
    let font = swash::FontRef::from_index(bytes, 0)?;
    let metrics = font.metrics(&[]).scale(f32::from(size));
    let advances = font.glyph_metrics(&[]).scale(f32::from(size));
    let charmap = font.charmap();
    let mut context = ScaleContext::new();
    let mut scaler = context
        .builder(font)
        .size(f32::from(size))
        .hint(true)
        .build();
    // Darken regular and monospaced strokes enough to survive monochrome
    // quantization, while keeping already-bold faces' counters open.
    let ink_expansion = match font_id {
        FONT_CHICAGO | FONT_LONDON | FONT_CAIRO => 0.10,
        FONT_MONACO | FONT_COURIER => 0.35,
        _ => 0.25,
    };
    let mut data = Vec::new();
    let mut glyph = |ch: char| {
        let id = charmap.map(ch);
        let advance = advances.advance_width(id).round().clamp(0.0, 255.0) as u8;
        let mut result = Glyph {
            width: 0,
            height: 0,
            advance,
            origin_x: 0,
            origin_y: 0,
            data_offset: data.len(),
        };
        if let Some(image) = Render::new(&[Source::Outline])
            // A subpixel ink expansion keeps thin stems from falling below
            // the binary-mask threshold on the logical pixel grid. This is
            // rasterization only: the font data and advances are unchanged.
            .embolden(ink_expansion)
            .render(&mut scaler, id)
        {
            result.width = u8::try_from(image.placement.width).expect("bounded URW glyph width");
            result.height = u8::try_from(image.placement.height).expect("bounded URW glyph height");
            result.origin_x = i8::try_from(image.placement.left).expect("bounded URW left bearing");
            result.origin_y = i8::try_from(-image.placement.top).expect("bounded URW top bearing");
            data.extend(image.data.iter().map(|&alpha| {
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
    let descent = (metrics.descent.ceil() as i16).max(
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
