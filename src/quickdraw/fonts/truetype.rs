//! Rasterization of the bundled URW Core 35 TrueType faces.

use swash::scale::{Render, ScaleContext, Scaler, Source};
use swash::{Charmap, FontRef, GlyphMetrics};

use super::{FontFace, FontMetrics, Glyph, MacRomanFace, MacRomanGlyph, MONO_COVERAGE_THRESHOLD};

pub(super) const NIMBUS_SANS: &[u8] = include_bytes!("urw-core35/NimbusSans-Regular.ttf");
pub(super) const NIMBUS_SANS_BOLD: &[u8] = include_bytes!("urw-core35/NimbusSans-Bold.ttf");
pub(super) const NIMBUS_ROMAN: &[u8] = include_bytes!("urw-core35/NimbusRoman-Regular.ttf");
pub(super) const NIMBUS_MONO: &[u8] = include_bytes!("urw-core35/NimbusMonoPS-Regular.ttf");
pub(super) const P052: &[u8] = include_bytes!("urw-core35/P052-Roman.ttf");
pub(super) const Z003: &[u8] = include_bytes!("urw-core35/Z003-MediumItalic.ttf");
pub(super) const C059_BOLD: &[u8] = include_bytes!("urw-core35/C059-Bold.ttf");
pub(super) const URW_GOTHIC_DEMI: &[u8] = include_bytes!("urw-core35/URWGothic-Demi.ttf");

fn rasterize(
    scaler: &mut Scaler<'_>,
    charmap: &Charmap<'_>,
    metrics: &GlyphMetrics<'_>,
    ch: char,
    embolden: f32,
    data: &mut Vec<u8>,
) -> Glyph {
    let glyph_id = charmap.map(ch);
    let image = Render::new(&[Source::Outline])
        .embolden(embolden)
        .render(scaler, glyph_id);
    let data_offset = data.len();
    if let Some(image) = &image {
        // The built-in compatibility catalogue deliberately follows the
        // classic bitmap-font contract: designer-selected pixels form each
        // strike (*Inside Macintosh: Text*, 1993, pp. 4-11–4-12). Keep hinted
        // coverage only long enough to make that binary pixel decision.
        data.extend(image.data.iter().map(|&coverage| {
            if coverage >= MONO_COVERAGE_THRESHOLD {
                u8::MAX
            } else {
                0
            }
        }));
    }
    let placement = image
        .as_ref()
        .map(|image| image.placement)
        .unwrap_or_default();
    Glyph {
        width: placement
            .width
            .try_into()
            .expect("TrueType glyph is too wide"),
        height: placement
            .height
            .try_into()
            .expect("TrueType glyph is too tall"),
        advance: metrics
            .advance_width(glyph_id)
            .round()
            .clamp(0.0, f32::from(u8::MAX)) as u8,
        origin_x: placement.left.clamp(i8::MIN as i32, i8::MAX as i32) as i8,
        origin_y: (-placement.top).clamp(i8::MIN as i32, i8::MAX as i32) as i8,
        data_offset,
    }
}

pub(super) fn bake_faces(
    font_id: i16,
    size: i16,
    bytes: &'static [u8],
    darkening: f32,
) -> (&'static FontFace, &'static MacRomanFace) {
    let font = FontRef::from_index(bytes, 0).expect("bundled URW Core 35 font must be valid");
    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(&[]).scale(size as f32);
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(size as f32).hint(true).build();
    // Swash returns scaled coordinates, so darkening is measured in pixels,
    // independent of point size. The catalogue uses less for bold faces and
    // more for thin monospaced strokes before the binary cutoff is applied.
    let mut data = Vec::new();
    let glyphs: Vec<Glyph> = (' '..='~')
        .map(|ch| {
            rasterize(
                &mut scaler,
                &charmap,
                &glyph_metrics,
                ch,
                darkening,
                &mut data,
            )
        })
        .collect();
    let macroman: Vec<MacRomanGlyph> = (0x80..=0xff)
        .map(|mac_code| MacRomanGlyph {
            mac_code,
            glyph: rasterize(
                &mut scaler,
                &charmap,
                &glyph_metrics,
                crate::mac_roman::decode_mac_roman_byte(mac_code),
                darkening,
                &mut data,
            ),
        })
        .collect();
    let line = font.metrics(&[]).scale(size as f32);
    let ink_ascent = glyphs
        .iter()
        .map(|glyph| -i16::from(glyph.origin_y))
        .max()
        .unwrap_or(0)
        .max(0);
    let ink_descent = glyphs
        .iter()
        .map(|glyph| i16::from(glyph.origin_y) + i16::from(glyph.height))
        .max()
        .unwrap_or(0)
        .max(0);
    let metrics = FontMetrics {
        // Faux darkening can extend a hinted outline beyond the font's line
        // box by one pixel. QuickDraw metrics must contain the cached strike
        // or callers will clip real ink at the top or bottom of a text run.
        ascent: (line.ascent.ceil() as i16).max(ink_ascent),
        descent: (line.descent.abs().ceil() as i16).max(ink_descent),
        wid_max: glyphs
            .iter()
            .map(|glyph| i16::from(glyph.advance))
            .max()
            .unwrap_or(0),
        leading: line.leading.round().max(0.0) as i16,
    };
    let data = Box::leak(data.into_boxed_slice());
    let face = Box::leak(Box::new(FontFace {
        font_id,
        size,
        metrics,
        glyphs: Box::leak(glyphs.into_boxed_slice()),
        data,
    }));
    let macroman = Box::leak(Box::new(MacRomanFace {
        font_id,
        size,
        glyphs: Box::leak(macroman.into_boxed_slice()),
        data,
    }));
    (face, macroman)
}
