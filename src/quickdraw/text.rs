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
    get_font_face_or_default, get_italic_glyph as get_italic_glyph_fn, get_macroman_glyph,
    override_format, FontMetrics, Glyph,
};

/// Architecture-neutral interpretation of QuickDraw's low-order `Style` byte.
///
/// QuickDraw and the Font Manager accept any combination of bold, italic,
/// underline, outline, shadow, condense, and extend. Intrinsic font faces take
/// priority; the remaining styles are synthesized while drawing. Inside
/// Macintosh: Text (1993), pp. 3-5--3-7 and 3-69--3-70.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct QuickDrawTextStyle(u8);

impl QuickDrawTextStyle {
    pub(crate) const BOLD_BIT: u8 = 0x01;
    pub(crate) const ITALIC_BIT: u8 = 0x02;
    pub(crate) const UNDERLINE_BIT: u8 = 0x04;
    pub(crate) const OUTLINE_BIT: u8 = 0x08;
    pub(crate) const SHADOW_BIT: u8 = 0x10;
    pub(crate) const CONDENSE_BIT: u8 = 0x20;
    pub(crate) const EXTEND_BIT: u8 = 0x40;
    const EFFECT_BITS: u8 = Self::BOLD_BIT
        | Self::ITALIC_BIT
        | Self::UNDERLINE_BIT
        | Self::OUTLINE_BIT
        | Self::SHADOW_BIT
        | Self::CONDENSE_BIT
        | Self::EXTEND_BIT;
    const PER_GLYPH_EFFECT_BITS: u8 = Self::EFFECT_BITS & !Self::UNDERLINE_BIT;

    pub(crate) const fn from_bits(bits: u8) -> Self {
        Self(bits & Self::EFFECT_BITS)
    }

    pub(crate) const fn plain() -> Self {
        Self(0)
    }

    pub(crate) const fn is_plain(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn has_per_glyph_effect(self) -> bool {
        self.0 & Self::PER_GLYPH_EFFECT_BITS != 0
    }

    pub(crate) const fn bold(self) -> bool {
        self.0 & Self::BOLD_BIT != 0
    }

    pub(crate) const fn italic(self) -> bool {
        self.0 & Self::ITALIC_BIT != 0
    }

    pub(crate) const fn underline(self) -> bool {
        self.0 & Self::UNDERLINE_BIT != 0
    }

    pub(crate) const fn outline(self) -> bool {
        self.0 & Self::OUTLINE_BIT != 0
    }

    pub(crate) const fn shadow(self) -> bool {
        self.0 & Self::SHADOW_BIT != 0
    }

    pub(crate) const fn condensed(self) -> bool {
        self.0 & Self::CONDENSE_BIT != 0
    }

    pub(crate) const fn extended(self) -> bool {
        self.0 & Self::EXTEND_BIT != 0
    }

    /// Advance one synthesized glyph using the frozen Roman system-font
    /// metrics shared by both guest adapters.
    pub(crate) fn glyph_advance(self, glyph_advance: i32) -> i32 {
        let mut advance = glyph_advance;
        if self.bold() {
            advance += 1;
        }
        if self.outline() {
            advance += 1;
        }
        if self.shadow() {
            advance += 2;
        }
        if self.condensed() && advance >= 6 {
            advance -= 1;
        }
        if self.extended() {
            advance += 1;
        }
        advance.max(1)
    }

    /// Vertical source-bitmap offset used before synthesizing a shadow.
    pub(crate) const fn glyph_y_offset(self) -> i32 {
        if self.shadow() {
            -1
        } else {
            0
        }
    }

    /// Radius of the mask smear used to synthesize hollow outline/shadow ink.
    pub(crate) const fn smear_max(self) -> Option<i32> {
        if self.shadow() && self.outline() {
            Some(3)
        } else if self.shadow() {
            Some(2)
        } else if self.outline() {
            Some(1)
        } else {
            None
        }
    }
}

pub fn get_font_metrics(font_id: i16, size: i16) -> FontMetrics {
    get_font_face_or_default(font_id, size).metrics
}

pub fn get_glyph(font_id: i16, size: i16, ch: char) -> Option<(&'static Glyph, &'static [u8])> {
    let face = get_font_face_or_default(font_id, size);
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
        if let Some(hit) = crate::quickdraw::fonts::pixel_font::menu_symbols::get_glyph(ch) {
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
        if let Some(hit) = crate::quickdraw::fonts::pixel_font::menu_symbols::get_glyph(ch) {
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

    // HLE chrome stores text as Unicode for layout and logging. Route every
    // representable extended character back through its Mac Roman glyph slot
    // so titles and menus use the same bitmap repertoire as guest DrawText.
    if let Some(mac_code @ 0x80..=0xFF) = crate::mac_roman::encode_mac_roman_char(ch) {
        return macroman_or_ascii_fallback(font_id, size, mac_code);
    }

    None
}

fn override_symbol_glyph(
    font_id: i16,
    size: i16,
    index: usize,
) -> Option<(&'static Glyph, &'static [u8])> {
    let face = get_font_face_or_default(font_id, size);
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
    if mac_code == 0xAA {
        return crate::quickdraw::fonts::pixel_font::menu_symbols::get_glyph('\u{2122}');
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
    use super::{get_glyph, QuickDrawTextStyle};

    #[test]
    fn quickdraw_style_plan_combines_all_low_order_face_bits() {
        let style = QuickDrawTextStyle::from_bits(0xff);

        assert!(style.bold());
        assert!(style.italic());
        assert!(style.underline());
        assert!(style.outline());
        assert!(style.shadow());
        assert!(style.condensed());
        assert!(style.extended());
        assert_eq!(style.glyph_y_offset(), -1);
        assert_eq!(style.smear_max(), Some(3));
        assert_eq!(style.glyph_advance(6), 10);
        assert!(QuickDrawTextStyle::from_bits(0x80).is_plain());
    }

    #[test]
    fn built_in_system_font_renders_menu_symbols() {
        for (symbol, name) in [('\u{2318}', "Command"), ('\u{2713}', "checkmark")] {
            let (glyph, data) = get_glyph(0, 12, symbol)
                .unwrap_or_else(|| panic!("{name} symbol should resolve to a bitmap glyph"));
            let glyph_len = usize::from(glyph.width) * usize::from(glyph.height);
            assert!(
                data[glyph.data_offset..glyph.data_offset + glyph_len]
                    .iter()
                    .any(|pixel| *pixel != 0),
                "{name} symbol should contain visible pixels"
            );
        }
    }

    #[test]
    fn unicode_hle_text_uses_mac_roman_extended_glyphs() {
        let (glyph, data) = get_glyph(0, 12, '™').expect("Mac Roman trademark glyph");
        let glyph_len = usize::from(glyph.width) * usize::from(glyph.height);
        assert!(data[glyph.data_offset..glyph.data_offset + glyph_len]
            .iter()
            .any(|pixel| *pixel != 0));
    }
}
