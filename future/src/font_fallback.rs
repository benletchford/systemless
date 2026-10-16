//! Generate monochrome source strikes from bundled or guest TrueType outlines.
//! CPU rasterization creates source font data; guest drawing remains a GPU task.
use crate::{font::Strike, Error, Result};
use ab_glyph::{Font, FontRef, ScaleFont};
pub(crate) fn strike(size: i16) -> Result<Vec<u8>> {
    if !(1..=96).contains(&size) {
        return Err(Error::Graphics("fallback font size outside 1..96".into()));
    }
    let font = FontRef::try_from_slice(include_bytes!("../assets/fonts/NimbusSans-Regular.ttf"))
        .map_err(|_| Error::Load("invalid bundled fallback font".into()))?;
    outline(font, size, None)
}
/// Prepare source glyphs only: destination drawing remains in shared wgpu services.
pub(crate) fn outline(font: FontRef<'_>, size: i16, widths: Option<&[u8; 257]>) -> Result<Vec<u8>> {
    if !(1..=96).contains(&size) {
        return Err(Error::Graphics("outline font size outside 1..96".into()));
    }
    // Macintosh screen points at 72 dpi equal pixels per em. ab_glyph's
    // scale is ascent-minus-descent, so explicitly convert from em units.
    let scale = f32::from(size) * font.height_unscaled()
        / font
            .units_per_em()
            .ok_or_else(|| Error::Load("fallback font has no em size".into()))?;
    if !scale.is_finite() || scale <= 0. {
        return Err(Error::Load("invalid outline scale".into()));
    }
    let scaled = font.as_scaled(scale);
    if ![scaled.ascent(), -scaled.descent()]
        .into_iter()
        .all(|value| value.is_finite() && (0. ..=512.).contains(&value))
        || !scaled.line_gap().is_finite()
        || scaled.line_gap().abs() > 512.
    {
        return Err(Error::Load(
            "outline font metrics exceed source limits".into(),
        ));
    }
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
            crate::mac_roman::decode(code as u8)
        };
        let id = if code == 256 {
            ab_glyph::GlyphId(0)
        } else {
            font.glyph_id(character)
        };
        let advance = if let Some(widths) = widths {
            widths[usize::from(code)]
        } else {
            scaled.h_advance(id).round().clamp(0., 255.) as u8
        };
        maximum = maximum.max(advance);
        let outlined = font.outline_glyph(id.with_scale(scale));
        let (left, top, width, height, mask) = if let Some(glyph) = outlined {
            let bounds = glyph.px_bounds();
            if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
                .into_iter()
                .all(|value| value.is_finite() && value.abs() <= 512.)
                || bounds.max.x < bounds.min.x
                || bounds.max.y < bounds.min.y
            {
                return Err(Error::Load(
                    "outline glyph bounds exceed source limits".into(),
                ));
            }
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
    if width > usize::from(u16::MAX) || width.div_ceil(16) * 2 * height > 16 * 1024 * 1024 {
        return Err(Error::Load("outline strike storage limit exceeded".into()));
    }
    let row_words = width.div_ceil(16).max(1);
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
