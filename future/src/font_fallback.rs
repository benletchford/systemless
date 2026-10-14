//! Generate a monochrome source strike from the licensed Nimbus Sans outline.
//! CPU rasterization creates source font data; guest drawing remains a GPU task.
use crate::{font::Strike, Error, Result};
use ab_glyph::{Font, FontRef, ScaleFont};
const MAC_ROMAN: [u32; 128] = [
    0xc4, 0xc5, 0xc7, 0xc9, 0xd1, 0xd6, 0xdc, 0xe1, 0xe0, 0xe2, 0xe4, 0xe3, 0xe5, 0xe7, 0xe9, 0xe8,
    0xea, 0xeb, 0xed, 0xec, 0xee, 0xef, 0xf1, 0xf3, 0xf2, 0xf4, 0xf6, 0xf5, 0xfa, 0xf9, 0xfb, 0xfc,
    0x2020, 0xb0, 0xa2, 0xa3, 0xa7, 0x2022, 0xb6, 0xdf, 0xae, 0xa9, 0x2122, 0xb4, 0xa8, 0x2260,
    0xc6, 0xd8, 0x221e, 0xb1, 0x2264, 0x2265, 0xa5, 0xb5, 0x2202, 0x2211, 0x220f, 0x3c0, 0x222b,
    0xaa, 0xba, 0x3a9, 0xe6, 0xf8, 0xbf, 0xa1, 0xac, 0x221a, 0x192, 0x2248, 0x2206, 0xab, 0xbb,
    0x2026, 0xa0, 0xc0, 0xc3, 0xd5, 0x152, 0x153, 0x2013, 0x2014, 0x201c, 0x201d, 0x2018, 0x2019,
    0xf7, 0x25ca, 0xff, 0x178, 0x2044, 0x20ac, 0x2039, 0x203a, 0xfb01, 0xfb02, 0x2021, 0xb7,
    0x201a, 0x201e, 0x2030, 0xc2, 0xca, 0xc1, 0xcb, 0xc8, 0xcd, 0xce, 0xcf, 0xcc, 0xd3, 0xd4,
    0xf8ff, 0xd2, 0xda, 0xdb, 0xd9, 0x131, 0x2c6, 0x2dc, 0xaf, 0x2d8, 0x2d9, 0x2da, 0xb8, 0x2dd,
    0x2db, 0x2c7,
];
pub(crate) fn strike(size: i16) -> Result<Vec<u8>> {
    if !(1..=96).contains(&size) {
        return Err(Error::Graphics("fallback font size outside 1..96".into()));
    }
    let font = FontRef::try_from_slice(include_bytes!("../assets/fonts/NimbusSans-Regular.ttf"))
        .map_err(|_| Error::Load("invalid bundled fallback font".into()))?;
    // Macintosh screen points at 72 dpi equal pixels per em. ab_glyph's
    // scale is ascent-minus-descent, so explicitly convert from em units.
    let scale = f32::from(size) * font.height_unscaled()
        / font
            .units_per_em()
            .ok_or_else(|| Error::Load("fallback font has no em size".into()))?;
    let scaled = font.as_scaled(scale);
    let mut ascent = scaled.ascent().ceil() as i16;
    let mut descent = (-scaled.descent()).ceil() as i16;
    let leading = scaled.line_gap().round() as i16;
    let mut glyphs = Vec::new();
    let mut kern = 0i16;
    let mut maximum = 0u8;
    for code in 32..=256u16 {
        let character = if code == 256 {
            '\u{fffd}'
        } else if code < 128 {
            char::from(code as u8)
        } else {
            char::from_u32(MAC_ROMAN[usize::from(code - 128)]).unwrap()
        };
        let id = if code == 256 {
            ab_glyph::GlyphId(0)
        } else {
            font.glyph_id(character)
        };
        let advance = scaled.h_advance(id).round().clamp(0., 255.) as u8;
        maximum = maximum.max(advance);
        let outlined = font.outline_glyph(id.with_scale(scale));
        let (left, top, width, height, mask) = if let Some(glyph) = outlined {
            let bounds = glyph.px_bounds();
            let width = (bounds.max.x - bounds.min.x) as usize;
            let height = (bounds.max.y - bounds.min.y) as usize;
            if width > 512 || height > 512 {
                return Err(Error::Load("fallback glyph extent exceeded".into()));
            }
            let mut mask = vec![0u8; width * height];
            glyph.draw(|x, y, coverage| {
                mask[y as usize * width + x as usize] = u8::from(coverage >= 0.5)
            });
            (
                bounds.min.x as i16,
                bounds.min.y as i16,
                width,
                height,
                mask,
            )
        } else {
            (0, 0, 0, 0, vec![])
        };
        kern = kern.min(left);
        ascent = ascent.max(-top);
        descent = descent.max(top + height as i16);
        let missing = id.0 == 0 && code != 256;
        glyphs.push(if missing {
            (0, 0, 0, 0, advance, vec![], true)
        } else {
            (left, top, width, height, advance, mask, false)
        });
    }
    let height = (ascent + descent) as usize;
    let width = glyphs.iter().map(|g| g.2).sum::<usize>();
    let row_words = width.div_ceil(16);
    let row_bytes = row_words * 2;
    let mut bitmap = vec![0u8; row_bytes * height];
    let mut locations = Vec::new();
    let mut offsets = Vec::new();
    let mut x = 0usize;
    for (left, top, w, h, advance, mask, missing) in glyphs {
        locations.push(
            u16::try_from(x).map_err(|_| Error::Load("fallback strike width overflow".into()))?,
        );
        let offset = u8::try_from(left - kern)
            .map_err(|_| Error::Load("fallback glyph bearing overflow".into()))?;
        offsets.push(if missing {
            0xffff
        } else {
            u16::from(offset) << 8 | u16::from(advance)
        });
        for y in 0..h {
            for dx in 0..w {
                if mask[y * w + dx] != 0 {
                    bitmap[(y + (top + ascent) as usize) * row_bytes + (x + dx) / 8] |=
                        0x80 >> ((x + dx) % 8);
                }
            }
        }
        x += w;
    }
    locations.push(x as u16);
    offsets.push(0xffff);
    let table_offset = 26 + bitmap.len() + locations.len() * 2;
    let offset_words = (table_offset - 16) / 2;
    let high = if offset_words > usize::from(u16::MAX) {
        (offset_words >> 16) as u16
    } else {
        (-descent) as u16
    };
    let mut words = vec![
        0x1000u16,
        32,
        255,
        u16::from(maximum),
        kern as u16,
        high,
        u16::from(maximum),
        height as u16,
        offset_words as u16,
        ascent as u16,
        descent as u16,
        leading as u16,
        row_words as u16,
    ];
    let mut bytes: Vec<u8> = words.drain(..).flat_map(u16::to_be_bytes).collect();
    bytes.extend(bitmap);
    bytes.extend(locations.into_iter().flat_map(u16::to_be_bytes));
    bytes.extend(offsets.into_iter().flat_map(u16::to_be_bytes));
    Strike::parse(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_strikes_decode_at_small_default_and_extended_offset_sizes() {
        for size in [1, 9, 12, 96] {
            let bytes = strike(size).unwrap();
            let font = Strike::parse(&bytes).unwrap();
            assert!(font.metrics.ascent > 0);
            assert!(font.metrics.maximum_width > 0);
            assert_eq!(font.glyph(b' ').width, 0);
            assert_eq!(
                font.mask(b'A').unwrap().len(),
                usize::from(font.glyph(b'A').width) * usize::from(font.glyph(b'A').height)
            );
            assert!(font.mask(b'A').unwrap().iter().any(|bit| *bit != 0) || size == 1);
            assert_eq!(font.glyph(0), font.glyph(0xf0));
            if size == 96 {
                assert!(i16::from_be_bytes([bytes[10], bytes[11]]) > 0);
            }
        }
        for size in [0, -1, 97] {
            assert!(strike(size).is_err());
        }
    }
}
