//! Rasterization of the bundled Liberation TrueType faces.

use swash::scale::{Render, ScaleContext, Scaler, Source};
use swash::{Charmap, FontRef, GlyphMetrics};

use super::{FontFace, FontMetrics, Glyph, MacRomanFace, MacRomanGlyph};

pub(super) const LIBERATION_SANS: &[u8] = include_bytes!("liberation/LiberationSans-Regular.ttf");
pub(super) const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("liberation/LiberationSans-Bold.ttf");
pub(super) const LIBERATION_SERIF: &[u8] = include_bytes!("liberation/LiberationSerif-Regular.ttf");
pub(super) const LIBERATION_MONO: &[u8] = include_bytes!("liberation/LiberationMono-Regular.ttf");

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
        data.extend_from_slice(&image.data);
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
) -> (&'static FontFace, &'static MacRomanFace) {
    let font =
        FontRef::from_index(bytes, 0).expect("bundled Liberation TrueType font must be valid");
    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(&[]).scale(size as f32);
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(size as f32).hint(true).build();
    // Classic applications frequently draw into 1-bit QuickDraw ports. Stem
    // darkening prevents narrow hinted strokes from disappearing when their
    // antialiased coverage is collapsed to monochrome.
    let embolden = 0.65 / size as f32;
    let mut data = Vec::new();
    let glyphs: Vec<Glyph> = (' '..='~')
        .map(|ch| {
            rasterize(
                &mut scaler,
                &charmap,
                &glyph_metrics,
                ch,
                embolden,
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
                embolden,
                &mut data,
            ),
        })
        .collect();
    let line = font.metrics(&[]).scale(size as f32);
    let metrics = FontMetrics {
        ascent: line.ascent.ceil() as i16,
        descent: line.descent.abs().ceil() as i16,
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
