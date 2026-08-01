//! Software glyph rasteriser used by `DrawString` / `DrawText` / friends.
//!
//! Reads the active font/style from the current `GrafPort` (`txFont`,
//! `txSize`, `txFace`) and emits per-glyph coverage strips at the
//! current pen location. Bypasses the trap dispatcher entirely — this
//! is plain Rust glyph blitting, used by every QuickDraw text op
//! after argument decode.
//!
//! Glyph data lives in [`crate::quickdraw::fonts`] (original systemless
//! bitmap art, `const fn`-decoded at compile time). Italic faces are
//! synthesised by the runtime shear-blit at draw time.

use crate::quickdraw::fonts::{
    get_font_face_scaled, get_italic_glyph as get_italic_glyph_fn, get_macroman_glyph,
    override_format, FontMetrics, Glyph,
};

pub fn get_font_metrics(font_id: i16, size: i16) -> FontMetrics {
    // Resolve through the same selection path as DrawChar/DrawString. In
    // particular, a host outline at the requested size must not be drawn with
    // metrics from the bitmap face that would otherwise be scaled.
    get_font_face_scaled(font_id, size).0.metrics
}

pub fn get_glyph(font_id: i16, size: i16, ch: char) -> Option<(&'static Glyph, &'static [u8])> {
    // Callers that need the bitmap scale obtain it from
    // `get_font_face_scaled`; selecting the glyph through that same function
    // keeps measurement and drawing on one face.
    let face = get_font_face_scaled(font_id, size).0;
    let glyphs = face.glyphs;
    let data = face.data;

    // ASCII range: glyphs start at ' ' (32).
    if (' '..='~').contains(&ch) {
        let idx = (ch as usize) - 32;
        if idx < glyphs.len() {
            let glyph = &glyphs[idx];
            if glyph.width != 0 || glyph.height != 0 || glyph.advance != 0 {
                return Some((glyph, data));
            }
        }
        return None;
    }

    // Mac Roman extended characters (0x80-0xFF). The raw byte was cast
    // to char so char code == Mac Roman code for this range.
    let mac_code = ch as u32;
    if (0x80..=0xFF).contains(&mac_code) {
        return macroman_or_ascii_fallback(font_id, size, mac_code as u8);
    }

    // HLE strings are decoded to Unicode before rendering. Map representable
    // non-ASCII scalars back to their guest byte so resource, bitmap, and host
    // outline faces all select the same Mac Roman glyph. In particular,
    // U+2026 is the single character at Mac Roman $C9, not three periods.
    if let Some(mac_code) = crate::mac_roman::encode_char(ch).filter(|&code| code >= 0x80) {
        return macroman_or_ascii_fallback(font_id, size, mac_code);
    }

    // Unicode codepoints emitted directly by HLE code paths that don't
    // fit in the Mac Roman byte range. The Menu Manager emits U+2318
    // (COMMAND KEY) for command-key equivalents and U+2713 (CHECK MARK)
    // for checked items; route them through the classic System font
    // Mac Roman symbol slots. Inside Macintosh Volume I, I-247 and I-358.
    if ch == '\u{2318}' {
        if let Some(hit) =
            override_symbol_glyph(font_id, size, override_format::COMMAND_SYMBOL_GLYPH_INDEX)
        {
            return Some(hit);
        }
        return get_macroman_glyph(font_id, size, 0x11);
    }
    if ch == '\u{2713}' {
        if let Some(hit) =
            override_symbol_glyph(font_id, size, override_format::CHECKMARK_SYMBOL_GLYPH_INDEX)
        {
            return Some(hit);
        }
        return get_macroman_glyph(font_id, size, 0x12);
    }
    if ch == '\u{14}' || ch == '\u{F8FF}' {
        if let Some(hit) =
            override_symbol_glyph(font_id, size, override_format::APPLE_SYMBOL_GLYPH_INDEX)
        {
            return Some(hit);
        }
        return get_macroman_glyph(font_id, size, 0x14);
    }

    None
}

fn override_symbol_glyph(
    font_id: i16,
    size: i16,
    index: usize,
) -> Option<(&'static Glyph, &'static [u8])> {
    let face = get_font_face_scaled(font_id, size).0;
    let glyph = face.glyphs.get(index)?;
    if glyph.width == 0 && glyph.height == 0 && glyph.advance == 0 {
        return None;
    }
    Some((glyph, face.data))
}

fn macroman_or_ascii_fallback(
    font_id: i16,
    size: i16,
    mac_code: u8,
) -> Option<(&'static Glyph, &'static [u8])> {
    if let Some(hit) = get_macroman_glyph(font_id, size, mac_code) {
        return Some(hit);
    }
    // ASCII fallback for extended characters that have a close ASCII
    // equivalent. Better to render a slightly-wrong glyph than silently
    // drop the character.
    // Mac Roman encoding (Inside Macintosh Volume I, I-247):
    let ascii_fallback: char = match mac_code {
        0xD0 | 0xD1 => '-',  // en-dash (–), em-dash (—)
        0xD2 | 0xD3 => '"',  // left-double, right-double quote
        0xD4 | 0xD5 => '\'', // left-single, right-single quote
        0xA5 => '*',         // bullet •
        0xCA => ' ',         // non-breaking space
        0xE1 | 0xE5 => '.',  // leading/trailing space-like
        _ => return None,
    };
    get_glyph(font_id, size, ascii_fallback)
}

pub fn get_glyph_italic(
    font_id: i16,
    size: i16,
    ch: char,
) -> Option<(&'static Glyph, &'static [u8])> {
    get_italic_glyph_fn(font_id, size, ch)
}

pub fn get_underline_thickness(_font_id: i16, _size: i16) -> i16 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quickdraw::fonts::{get_font_face_scaled, FONT_TIMES};

    #[test]
    fn glyphs_and_metrics_use_the_same_selected_face() {
        let (face, _) = get_font_face_scaled(FONT_TIMES, 24);
        let (glyph, data) = get_glyph(FONT_TIMES, 24, 'W').expect("printable glyph missing");
        let expected = &face.glyphs[(b'W' - b' ') as usize];
        assert!(std::ptr::eq(glyph, expected));
        assert!(std::ptr::eq(data, face.data));

        let metrics = get_font_metrics(FONT_TIMES, 24);
        assert_eq!(metrics.ascent, face.metrics.ascent);
        assert_eq!(metrics.descent, face.metrics.descent);
        assert_eq!(metrics.wid_max, face.metrics.wid_max);
        assert_eq!(metrics.leading, face.metrics.leading);
    }

    #[test]
    fn host_times_renders_mac_roman_horizontal_ellipsis() {
        let Some((glyph, data)) = get_glyph(FONT_TIMES, 24, '\u{2026}') else {
            // A host without a Times-compatible outline keeps using the
            // portable bitmap fallback, which has no extended table yet.
            return;
        };
        assert!(glyph.width > 0);
        assert!(glyph.advance > 0);
        assert!(data.iter().any(|&pixel| pixel == 255));
    }
}
