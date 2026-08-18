//! Rasterization of the bundled URW Core 35 TrueType faces.

use fontdue::{Font, FontSettings};

use super::{FontFace, FontMetrics, Glyph, MacRomanFace, MacRomanGlyph};

pub(super) const NIMBUS_SANS: &[u8] = include_bytes!("urw/NimbusSans-Regular.ttf");
pub(super) const NIMBUS_SANS_BOLD: &[u8] = include_bytes!("urw/NimbusSans-Bold.ttf");
pub(super) const NIMBUS_ROMAN: &[u8] = include_bytes!("urw/NimbusRoman-Regular.ttf");
pub(super) const NIMBUS_MONO: &[u8] = include_bytes!("urw/NimbusMonoPS-Regular.ttf");

fn rasterize(font: &Font, ch: char, size: i16, data: &mut Vec<u8>) -> Glyph {
    let (metrics, bitmap) = font.rasterize(ch, size as f32);
    let data_offset = data.len();
    data.extend_from_slice(&bitmap);
    Glyph {
        width: metrics
            .width
            .try_into()
            .expect("TrueType glyph is too wide"),
        height: metrics
            .height
            .try_into()
            .expect("TrueType glyph is too tall"),
        advance: metrics.advance_width.round().clamp(0.0, f32::from(u8::MAX)) as u8,
        origin_x: metrics.xmin.clamp(i8::MIN as i32, i8::MAX as i32) as i8,
        // fontdue's ymin is measured upward from the baseline, while the
        // QuickDraw blitter stores the top edge in screen coordinates.
        origin_y: (-(metrics.ymin + metrics.height as i32)).clamp(i8::MIN as i32, i8::MAX as i32)
            as i8,
        data_offset,
    }
}

pub(super) fn bake_faces(
    font_id: i16,
    size: i16,
    bytes: &'static [u8],
) -> (&'static FontFace, &'static MacRomanFace) {
    let font = Font::from_bytes(bytes, FontSettings::default())
        .expect("bundled URW TrueType font must be valid");
    let mut data = Vec::new();
    let glyphs: Vec<Glyph> = (' '..='~')
        .map(|ch| rasterize(&font, ch, size, &mut data))
        .collect();
    let macroman: Vec<MacRomanGlyph> = (0x80..=0xff)
        .map(|mac_code| MacRomanGlyph {
            mac_code,
            glyph: rasterize(
                &font,
                crate::mac_roman::decode_mac_roman_byte(mac_code),
                size,
                &mut data,
            ),
        })
        .collect();
    let line = font
        .horizontal_line_metrics(size as f32)
        .expect("bundled horizontal font must have line metrics");
    let metrics = FontMetrics {
        ascent: line.ascent.ceil() as i16,
        descent: (-line.descent).ceil() as i16,
        wid_max: glyphs
            .iter()
            .map(|glyph| i16::from(glyph.advance))
            .max()
            .unwrap_or(0),
        leading: line.line_gap.round().max(0.0) as i16,
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
