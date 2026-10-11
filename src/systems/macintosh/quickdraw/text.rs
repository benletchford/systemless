//! Software glyph rasteriser used by `DrawString` / `DrawText` / friends.
//!
//! Reads the active font/style from the current `GrafPort` (`txFont`,
//! `txSize`, `txFace`) and emits per-glyph coverage strips at the
//! current pen location. Bypasses the trap dispatcher entirely — this
//! is plain Rust glyph blitting, used by every QuickDraw text op
//! after argument decode.
//!
//! Glyph data lives in [`crate::quickdraw::fonts`], rasterized from bundled
//! or guest-supplied fonts. Italic faces are
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
        (glyph_advance + self.advance_extra()).max(1)
    }

    pub(crate) fn advance_extra(self) -> i32 {
        i32::from(self.bold()) + i32::from(self.outline()) + 2 * i32::from(self.shadow())
            - i32::from(self.condensed())
            + i32::from(self.extended())
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

/// Antialiased coverage from the already resolved guest font outline.
/// Bitmap-only sources return None; this never substitutes a different face.
#[derive(Clone, Debug)]
pub struct SmoothGlyphSnapshot {
    pub pixels: std::sync::Arc<[u8]>,
    pub width: i32,
    pub height: i32,
    pub left: i32,
    pub top: i32,
    pub guest_advance: i32,
    pub raster_scale: u32,
}

pub fn smooth_unicode_glyph(font: i16, size: i16, ch: char, scale: u32) -> Option<SmoothGlyphSnapshot> {
    if !(1..=8).contains(&scale) { return None; }
    let (glyph, data) = get_unicode_glyph(font, size, ch)?;
    smooth_resolved_glyph(glyph, data, scale)
}

/// Smooth only the supplied resolved glyph, preserving font resource precedence.
pub fn smooth_resolved_glyph(glyph: &Glyph, data: &[u8], scale: u32) -> Option<SmoothGlyphSnapshot> {
    if !(1..=8).contains(&scale) { return None; }
    let mask = crate::quickdraw::fonts::outline::presentation_glyph(glyph, data, scale)?;
    Some(SmoothGlyphSnapshot {
        pixels: mask.pixels, width: mask.width, height: mask.height,
        left: mask.left, top: mask.top, guest_advance: i32::from(glyph.advance),
        raster_scale: scale,
    })
}

/// Look up decoded host text, as opposed to guest bytes cast directly to char.
/// Unicode Latin-1 overlaps the Mac Roman byte range with different meanings
/// (for example, U+00AE is registered, but Mac Roman byte AE is AE ligature).
pub fn get_unicode_glyph(
    font_id: i16,
    size: i16,
    ch: char,
) -> Option<(&'static Glyph, &'static [u8])> {
    if let Some(mac_code @ 0x80..=0xFF) = crate::mac_roman::encode_mac_roman_char(ch) {
        return macroman_or_ascii_fallback(font_id, size, mac_code);
    }
    get_glyph(font_id, size, ch)
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
        if let Some(hit) = crate::quickdraw::fonts::outline::unicode_glyph(font_id, size, ch) {
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
        if let Some(hit) = crate::quickdraw::fonts::outline::unicode_glyph(font_id, size, ch) {
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
        return crate::quickdraw::fonts::outline::unicode_glyph(font_id, size, '\u{2122}');
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

/// One architecture-neutral Classic Mac text line.
///
/// `start..visible_end` is the part drawn on screen; `next` is the guest-text
/// offset at which the following line begins. This keeps hard line endings and
/// wrap whitespace in the logical text while excluding them from rasterization.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WrappedTextLine {
    pub start: usize,
    pub visible_end: usize,
    pub next: usize,
}

/// Break Classic Mac text into display lines using caller-supplied glyph widths.
///
/// Both guest adapters use this primitive so TextEdit and dialog static text
/// agree on word wrapping and CR/LF/CRLF hard breaks. The width callback is
/// indexed so styled TextEdit can resolve the style run for each byte without
/// coupling this shared semantic layer to either guest's memory representation.
///
/// Inside Macintosh: Text (1993), pp. 2-88--2-89 and 5-24--5-27: TextEdit
/// prefers word-boundary breaks, uses glyph widths when laying out a line, and
/// treats trailing whitespace as non-visible.
#[doc(hidden)]
pub fn wrap_classic_text<F>(
    text: &[u8],
    max_width: i16,
    mut byte_advance: F,
) -> Vec<WrappedTextLine>
where
    F: FnMut(usize, u8) -> i16,
{
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let max_width = max_width.max(1);

    while line_start < text.len() {
        let mut index = line_start;
        let mut width = 0i16;
        let mut last_whitespace = None::<(usize, usize)>;
        let mut completed = false;

        while index < text.len() {
            if matches!(text[index], b'\r' | b'\n') {
                let next = if text[index] == b'\r' && text.get(index + 1) == Some(&b'\n') {
                    index + 2
                } else {
                    index + 1
                };
                lines.push(WrappedTextLine {
                    start: line_start,
                    visible_end: trim_classic_line_end(text, line_start, index),
                    next,
                });
                line_start = next;
                completed = true;
                break;
            }

            let advance = byte_advance(index, text[index]).max(0);
            if index > line_start && width.saturating_add(advance) > max_width {
                let (visible_end, next) = if text[index] <= b' ' {
                    let mut next = index + 1;
                    while next < text.len()
                        && text[next] <= b' '
                        && !matches!(text[next], b'\r' | b'\n')
                    {
                        next += 1;
                    }
                    (trim_classic_line_end(text, line_start, index), next)
                } else if let Some((visible_end, next)) = last_whitespace {
                    (visible_end, next)
                } else {
                    (index, index)
                };
                lines.push(WrappedTextLine {
                    start: line_start,
                    visible_end,
                    next,
                });
                line_start = next;
                completed = true;
                break;
            }

            width = width.saturating_add(advance);
            if text[index] <= b' ' {
                let whitespace_start = index;
                let mut next = index + 1;
                while next < text.len()
                    && text[next] <= b' '
                    && !matches!(text[next], b'\r' | b'\n')
                {
                    next += 1;
                }
                last_whitespace = Some((whitespace_start, next));
            }
            index += 1;
        }

        if !completed {
            lines.push(WrappedTextLine {
                start: line_start,
                visible_end: trim_classic_line_end(text, line_start, text.len()),
                next: text.len(),
            });
            break;
        }
    }

    lines
}

fn trim_classic_line_end(text: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && text[end - 1] <= b' ' {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::{get_glyph, wrap_classic_text, QuickDrawTextStyle, WrappedTextLine};

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

    #[test]
    fn unicode_latin1_chrome_uses_the_corresponding_mac_roman_slot() {
        for (ch, byte) in [('®', 0xA8), ('©', 0xA9), ('é', 0x8E), ('Æ', 0xAE)] {
            let (actual, data) = super::get_unicode_glyph(0, 12, ch).expect("Unicode glyph");
            let (expected, _) = get_glyph(0, 12, char::from(byte)).expect("guest glyph");
            assert!(
                std::ptr::eq(actual, expected),
                "wrong Mac Roman slot for {ch}"
            );
            let len = usize::from(actual.width) * usize::from(actual.height);
            assert!(data[actual.data_offset..actual.data_offset + len]
                .iter()
                .any(|p| *p != 0));
        }
    }

    #[test]
    fn classic_text_wrap_prefers_words_and_hides_wrap_whitespace() {
        let text = b"one two three";

        assert_eq!(
            wrap_classic_text(text, 6, |_, _| 1),
            vec![
                WrappedTextLine {
                    start: 0,
                    visible_end: 3,
                    next: 4,
                },
                WrappedTextLine {
                    start: 4,
                    visible_end: 7,
                    next: 8,
                },
                WrappedTextLine {
                    start: 8,
                    visible_end: 13,
                    next: 13,
                },
            ]
        );
    }

    #[test]
    fn classic_text_wrap_normalizes_cr_lf_and_crlf_hard_breaks() {
        let text = b"one\rtwo\nthree\r\nfour";

        assert_eq!(
            wrap_classic_text(text, 80, |_, _| 1),
            vec![
                WrappedTextLine {
                    start: 0,
                    visible_end: 3,
                    next: 4,
                },
                WrappedTextLine {
                    start: 4,
                    visible_end: 7,
                    next: 8,
                },
                WrappedTextLine {
                    start: 8,
                    visible_end: 13,
                    next: 15,
                },
                WrappedTextLine {
                    start: 15,
                    visible_end: 19,
                    next: 19,
                },
            ]
        );
    }

    #[test]
    fn classic_text_wrap_breaks_an_overlong_word_at_a_character() {
        assert_eq!(
            wrap_classic_text(b"toolbox", 3, |_, _| 1),
            vec![
                WrappedTextLine {
                    start: 0,
                    visible_end: 3,
                    next: 3,
                },
                WrappedTextLine {
                    start: 3,
                    visible_end: 6,
                    next: 6,
                },
                WrappedTextLine {
                    start: 6,
                    visible_end: 7,
                    next: 7,
                },
            ]
        );
    }
}


/// Shared binary glyph mask before outline/shadow expansion.
pub(crate) fn styled_glyph_base_pixels(
    x: i16,
    y: i16,
    glyph: &Glyph,
    data: &[u8],
    synthetic_italic: Option<(i16, i16)>,
    style: QuickDrawTextStyle,
) -> std::collections::HashSet<(i16, i16)> {
    let gx = x + glyph.origin_x as i16;
    let gy = y + glyph.origin_y as i16;
    let gw = glyph.width as usize;
    let gh = glyph.height as usize;
    let metrics = synthetic_italic
        .map(|(font_id, font_size)| (font_id, font_size, get_font_metrics(font_id, font_size)));
    let mut pixels = std::collections::HashSet::new();

    for row in 0..gh {
        for col in 0..gw {
            let byte_idx = glyph.data_offset + row * gw + col;
            if byte_idx >= data.len() || data[byte_idx] < 128 {
                continue;
            }

            let py = gy + row as i16;
            let slant = metrics
                .as_ref()
                .map(|(font_id, font_size, metrics)| {
                    crate::quickdraw::fonts::style::get_italic_slant(*font_id, *font_size, metrics, y, py)
                })
                .unwrap_or(0);
            let start = col as i16;
            let (dst_start, dst_end) = (start, start + 1);

            for dst_col in dst_start..dst_end {
                let px = gx + dst_col + slant;
                pixels.insert((px, py));
                if style.bold() {
                    pixels.insert((px + 1, py));
                }
            }
        }
    }

    pixels
}


/// Guest binary ink and advance for a Macintosh Roman byte and QuickDraw face.
/// Underlining belongs to the complete line and is intentionally separate.
/// Coordinates are relative to the glyph's pen baseline; callers must preserve
/// the guest's strike selection rather than applying host font shaping.
#[doc(hidden)]
pub fn classic_styled_glyph(
    font: i16,
    size: i16,
    byte: u8,
    face: u8,
) -> (i32, Vec<(i16, i16)>) {
    let style = QuickDrawTextStyle::from_bits(face);
    let hit = if style.italic() {
        get_glyph_italic(font, size, byte as char)
            .map(|(glyph, data)| (glyph, data, None))
            .or_else(|| get_glyph(font, size, byte as char)
                .map(|(glyph, data)| (glyph, data, Some((font, size)))))
    } else {
        get_glyph(font, size, byte as char).map(|(glyph, data)| (glyph, data, None))
    };
    let Some((glyph, data, italic)) = hit else {
        return (6, Vec::new());
    };
    let mut ink = styled_glyph_pixels(0, 0, glyph, data, italic, style).into_iter().collect::<Vec<_>>();
    ink.sort_unstable_by_key(|&(x, y)| (y, x));
    (style.glyph_advance(i32::from(glyph.advance)), ink)
}

/// Shared guest/GPUI glyph mask, including hollow outline and shadow synthesis.
pub(crate) fn styled_glyph_pixels(
    x: i16, y: i16, glyph: &Glyph, data: &[u8],
    synthetic_italic: Option<(i16, i16)>, style: QuickDrawTextStyle,
) -> std::collections::HashSet<(i16, i16)> {
    let base = styled_glyph_base_pixels(
        x, y.saturating_add(style.glyph_y_offset() as i16), glyph, data, synthetic_italic, style,
    );
    let ink = if let Some(smear) = style.smear_max() {
        let smear = smear as i16;
        let mut expanded = std::collections::HashSet::new();
        for &(x, y) in &base {
            for dy in -1..=smear {
                for dx in -1..=smear {
                    let pixel = (x + dx, y + dy);
                    if !base.contains(&pixel) {
                        expanded.insert(pixel);
                    }
                }
            }
        }
        expanded
    } else {
        base
    };
    ink
}

/// PPC source-strike underline before outline/shadow smear and exclusion.
/// This ribbon belongs to the entire run and is included in each glyph's
/// effect buffer by the native painter. Coordinates precede rational scaling.
#[doc(hidden)]
pub fn ppc_styled_run_halo_underline_ink(
    text_font: i16, text_size: i16, face: u8, line_advance: i32,
) -> Vec<(i32, i32)> {
    let style = QuickDrawTextStyle::from_bits(face);
    if !style.underline() || style.smear_max().is_none() || line_advance <= 0 {
        return Vec::new();
    }
    let metrics = get_font_metrics(text_font, text_size);
    let mut ink = Vec::new();
    if style.underline() && style.smear_max().is_some() && line_advance > 0 {
        let underline_offset: i32 = if style.shadow() { -1 } else { 0 };
        let synthetic_italic = style.italic()
            && get_glyph_italic(text_font, text_size, 'A').is_none();
        let underline_left = if synthetic_italic {
            crate::quickdraw::fonts::style::get_italic_underline_extend_left(
                text_font,
                text_size,
                style.bold(),
                false,
            )
        } else {
            0
        };
        let underline_right = if synthetic_italic {
            crate::quickdraw::fonts::style::get_italic_end_extend(text_font, text_size, &metrics)
        } else {
            0
        };
        let final_effect_advance = style.glyph_advance(0);
        for source_x in underline_offset.saturating_sub(i32::from(underline_left))
            ..line_advance
                .saturating_sub(final_effect_advance)
                .saturating_add(underline_offset)
                .saturating_add(i32::from(underline_right))
        {
            ink.push((source_x, 1));
        }
    }

    ink
}

/// PPC source-strike mask, including its run underline/outline interaction.
/// Pixel coordinates precede the CPU's rational scaling and port spacing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn visit_ppc_styled_glyph_source_ink(
    text_font: i16, text_size: i16, metrics: &FontMetrics,
    glyph: &Glyph, data: &[u8], synthetic_italic: bool,
    style: QuickDrawTextStyle, source_advance: i32, line_advance: i32,
    mut emit: impl FnMut(i32, i32),
) {
    let mut base_pixels = std::collections::HashSet::new();
    for row in 0..glyph.height as usize {
        for col in 0..glyph.width as usize {
            let index = glyph.data_offset + row * glyph.width as usize + col;
            if index >= data.len() || data[index] < 128 {
                continue;
            }
            let source_y = i32::from(glyph.origin_y) + row as i32;
            let slant = synthetic_italic.then(|| {
                crate::quickdraw::fonts::style::get_italic_slant(
                    text_font,
                    text_size,
                    &metrics,
                    0,
                    i16::try_from(source_y).unwrap_or(0),
                )
            });
            let source_x = source_advance
                + i32::from(glyph.origin_x)
                + col as i32
                + i32::from(slant.unwrap_or(0));
            base_pixels.insert((source_x, source_y));
            if style.bold() {
                base_pixels.insert((source_x + 1, source_y));
            }
        }
    }

    base_pixels.extend(ppc_styled_run_halo_underline_ink(
        text_font, text_size, style.0, line_advance));

    if let Some(smear_max) = style.smear_max() {
        let min_x = base_pixels
            .iter()
            .map(|(x, _)| *x)
            .min()
            .unwrap_or(source_advance)
            - 1;
        let max_x = base_pixels
            .iter()
            .map(|(x, _)| *x)
            .max()
            .unwrap_or(source_advance)
            + smear_max;
        let min_y = base_pixels.iter().map(|(_, y)| *y).min().unwrap_or(0) - 1;
        let max_y = base_pixels.iter().map(|(_, y)| *y).max().unwrap_or(0) + smear_max;
        for source_y in min_y..=max_y {
            for source_x in min_x..=max_x {
                if base_pixels.contains(&(source_x, source_y)) {
                    continue;
                }
                let smeared = (-1..=smear_max).any(|dy| {
                    (-1..=smear_max)
                        .any(|dx| base_pixels.contains(&(source_x - dx, source_y - dy)))
                });
                if smeared {
                    emit(source_x, source_y);
                }
            }
        }
    } else {
        for (source_x, source_y) in base_pixels.iter().copied() {
            emit(source_x, source_y);
        }
    }
}

/// Device-independent PPC font pixel footprint, preserving floor rounding on
/// negative bearings and at least one destination pixel when shrinking.
#[doc(hidden)]
pub fn ppc_font_source_pixel_bounds(
    x: i32, y: i32, numerator: i32, denominator: i32,
) -> Option<(i32, i32, i32, i32)> {
    if numerator <= 0 || denominator <= 0 { return None; }
    let floor = |value: i32| -> Option<i32> {
        // Retain native PPC's saturating product before floor division.
        let scaled = value.saturating_mul(numerator);
        Some(if scaled >= 0 { scaled / denominator } else {
            -(scaled.checked_neg()?.saturating_add(denominator - 1) / denominator)
        })
    };
    let (left, top) = (floor(x)?, floor(y)?);
    Some((left, top, floor(x.checked_add(1)?)?.max(left.checked_add(1)?),
        floor(y.checked_add(1)?)?.max(top.checked_add(1)?)))
}

/// PPC run ink for zero CharExtra, in guest pixel coordinates relative to the
/// run's pen baseline. Uses guest Font Manager ratios and the native PPC mask;
/// callers retain palette, text-mode, clipping and ownership policy.
#[doc(hidden)]
pub fn ppc_styled_run_ink(font: i16, size: i16, face_bits: u8, bytes: &[u8])
    -> (i16, Vec<(i32, i32)>)
{
    let style = QuickDrawTextStyle::from_bits(face_bits);
    let (face, numerator, denominator) = crate::quickdraw::fonts::get_font_face_scale_ratio(font, size);
    let metrics = get_font_metrics(font, face.size);
    let measured = bytes.iter().fold(0i32, |pen, byte| {
        pen.saturating_add(style.glyph_advance(get_glyph(font, face.size, *byte as char)
            .map_or(6, |(glyph, _)| i32::from(glyph.advance))))
    });
    let mut pixels = std::collections::BTreeSet::new();
    let mut emit = |x, y| {
        if let Some((left, top, right, bottom)) = ppc_font_source_pixel_bounds(x, y, numerator, denominator) {
            for y in top..bottom { for x in left..right { pixels.insert((x, y)); } }
        }
    };
    let mut pen = 0i32;
    for &byte in bytes {
        let (hit, synthetic) = if style.italic() {
            get_glyph_italic(font, face.size, byte as char).map(|hit| (Some(hit), false))
                .unwrap_or_else(|| (get_glyph(font, face.size, byte as char), true))
        } else { (get_glyph(font, face.size, byte as char), false) };
        if let Some((glyph, data)) = hit {
            visit_ppc_styled_glyph_source_ink(font, face.size, &metrics, glyph, data,
                synthetic, style, pen, measured, &mut emit);
            pen = pen.saturating_add(if style.is_plain() { i32::from(glyph.advance) }
                else { style.glyph_advance(i32::from(glyph.advance)) });
        } else { pen = pen.saturating_add(6); }
    }
    if style.underline() && style.smear_max().is_none() && pen > 0 {
        for y in 1..=i32::from(get_underline_thickness(font, face.size).max(1)) {
            for x in 0..pen { emit(x, y); }
        }
    }
    let advance = measured.saturating_mul(numerator).saturating_add(denominator / 2) / denominator;
    (advance.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        pixels.into_iter().collect())
}

/// Classic 68k glyph coverage used by draw_char, including per-character or
/// caller-supplied continuous underline breaks. Keep original guest coverage;
/// palette, transfer mode, clipping, background erasure and pen updates belong
/// to the caller. Coordinates use the guest's (vertical, horizontal) pen.
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn classic_glyph_coverage(
    glyph: &Glyph, data: &[u8], tx_font: i16, tx_size: i16,
    metrics: &FontMetrics, fs: i16, use_precaptured_italic: bool, face: u8,
    pen: (i16, i16),
    underline_info: Option<(i16, i16, &[std::collections::HashSet<i16>])>,
    y: i16, x: i16,
) -> u8 {
    use crate::quickdraw::fonts::style::{get_italic_slant,
        get_italic_underline_extend_left, get_italic_underline_extend_right, get_underline_offset};
    let (v, h) = pen;
    let is_bold = face & 1 != 0;
    let is_italic = face & 2 != 0 && !use_precaptured_italic;
    let is_underline = face & 4 != 0;
    let is_outline = face & 8 != 0;
    let is_shadow = face & 16 != 0;
    let bold_extra: i16 = if is_bold { 1 } else { 0 };
    let scaled_ascent = metrics.ascent * fs;
    let top = v - scaled_ascent;
    let left = h + glyph.origin_x as i16 * fs;
    let ul_thick = get_underline_thickness(tx_font, tx_size);
let check_pixel = |r: i16, c: i16| -> u8 {
    // Bounds check in SCALED coordinates first (avoids Rust's
    // truncate-toward-zero division giving wrong results for negatives)
    if c < 0 || c >= glyph.width as i16 * fs {
        return 0;
    }

    // Map screen-relative row to glyph data row
    // r is relative to font top (v - scaled_ascent)
    // content_row_scaled = r - scaled_ascent - origin_y * fs
    let content_row_scaled = r - scaled_ascent - glyph.origin_y as i16 * fs;
    if content_row_scaled < 0 || content_row_scaled >= glyph.height as i16 * fs {
        return 0;
    }

    // Now safe to divide (both values are non-negative)
    let gc = c / fs;
    let content_row = content_row_scaled / fs;

    // Row-major 8-bit coverage (one byte per pixel).
    let b_idx =
        glyph.data_offset + (content_row as usize) * glyph.width as usize + gc as usize;
    if b_idx >= data.len() {
        return 0;
    }
    data[b_idx]
};

    if !(is_bold || is_italic || is_underline || is_outline || is_shadow) {
        return check_pixel(y - top, x - left);
    }
    // All helper closures below return the per-pixel coverage
    // byte (0=off, 255=fully on, 1..254=partial). Bold smear
    // and underline clamp to full alpha where they set a pixel
    // (they're auxiliary strokes, not antialiased).

    // 1. Raw glyph with Italic slant
    let get_italic_pixel = |curr_y: i16, curr_x: i16| -> u8 {
        let slant = if is_italic {
            get_italic_slant(tx_font, tx_size, &metrics, v, curr_y) * fs
        } else {
            0
        };
        let c = curr_x - left - slant;
        let r = curr_y - top;
        check_pixel(r, c)
    };

    // 2. Add Bold smear: take the max of this column and the
    //    one to its left so antialiased stem edges stay
    //    crisp at their new rightward boundary.
    let get_bold_pixel = |curr_y: i16, curr_x: i16| -> u8 {
        let p = get_italic_pixel(curr_y, curr_x);
        if is_bold {
            p.max(get_italic_pixel(curr_y, curr_x - 1))
        } else {
            p
        }
    };

    // 3. Add Underline (uses global descender info if available).
    //    The underline ribbon is a solid, non-antialiased
    //    stroke: when the pixel lies inside the ribbon it
    //    gets clamped to full coverage (255) regardless of
    //    the antialiased glyph value beneath.
    let get_underlined_pixel = |curr_y: i16, curr_x: i16| -> u8 {
        let mut p = get_bold_pixel(curr_y, curr_x);

        if is_underline && curr_y > v && curr_y < v + 1 + ul_thick {
            let italic_extend = if is_italic {
                get_italic_underline_extend_left(
                    tx_font,
                    tx_size,
                    is_bold,
                    use_precaptured_italic,
                )
            } else {
                0
            };

            let underline_offset =
                get_underline_offset(tx_font, tx_size, glyph, is_shadow);

            let italic_extend_right = if is_italic {
                get_italic_underline_extend_right(tx_font, tx_size)
            } else {
                0
            };

            // Check if this x is within the underline range
            let (underline_start, underline_end, in_range) =
                if let Some((start, end, ref breaks)) = underline_info {
                    // Use global underline info from draw_string
                    let effective_start = start - italic_extend + underline_offset;
                    let effective_end =
                        end + underline_offset + italic_extend_right;
                    let in_range =
                        curr_x >= effective_start && curr_x < effective_end;

                    let row_idx = (curr_y - (v + 1)) as usize;
                    let has_break = if row_idx < breaks.len() {
                        breaks[row_idx].contains(&curr_x)
                    } else {
                        false
                    };

                    if in_range && !has_break {
                        p = 255;
                    }
                    (effective_start, effective_end, in_range)
                } else {
                    // Fallback: per-character underline
                    let start = h;
                    let end = h + glyph.advance as i16 + bold_extra;
                    let effective_start = start - italic_extend + underline_offset;
                    let effective_end =
                        end + underline_offset + italic_extend_right;
                    let in_range =
                        curr_x >= effective_start && curr_x < effective_end;
                    if in_range {
                        // Check this character's descenders only
                        // Note: Fallback doesn't support complex per-row/smart breaks yet,
                        // but Geneva 24 shouldn't be using fallback heavily in contiguous strings.
                        // If it does, we assume simplified break logic for now.
                        let has_descender = classic_underline_has_descender(
                            v, curr_x, metrics.descent, &get_bold_pixel);
                        if !has_descender {
                            p = 255;
                        }
                    }
                    (effective_start, end, in_range)
                };
            let _ = (underline_start, underline_end, in_range); // suppress warnings
        }
        p
    };

    let mut pixel = get_underlined_pixel(y, x);

    if is_outline || is_shadow {
        // Shadow offset for smear should match the advance_extra (always 2)
        let shadow_offset = 2;

        // For "Everything" style, tune horizontal smear to preserve gaps
        // while keeping edge shadows intact.
        let is_everything =
            is_italic && is_bold && is_outline && is_shadow && is_underline;
        let (underline_start, underline_end) = underline_info
            .as_ref()
            .map(|(start, end, _)| (*start, *end))
            .unwrap_or((i16::MIN, i16::MAX));
        let in_underline_range = x >= underline_start && x < underline_end;
        let is_break = underline_info
            .as_ref()
            .map(|(_, _, breaks)| {
                // For smear, assume checking breaks[0] or generic break?
                // Smear logic uses 'is_break' for line 3 (v+2).
                // v+2 corresponds to row_idx 1.
                if breaks.len() > 1 {
                    breaks[1].contains(&x)
                } else if !breaks.is_empty() {
                    breaks[0].contains(&x)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        let smear_max = if is_outline && is_shadow {
            3
        } else if is_shadow {
            shadow_offset
        } else {
            1
        };
        let dx_max = if is_everything {
            if y == v {
                if in_underline_range {
                    1
                } else {
                    smear_max
                }
            } else if y == v + 1 {
                if in_underline_range {
                    1
                } else {
                    0
                }
            } else if y == v + 2 {
                if is_break {
                    0
                } else {
                    smear_max
                }
            } else {
                smear_max
            }
        } else {
            smear_max
        };

        let mut is_smeared = false;
        if !(is_everything && y == v + 1 && !in_underline_range) {
            // QuickDraw shadow algorithm (DrawText.a lines 837-901):
            // 1. Smear buffer RIGHT (ROXR.L) and DOWN (OR with line above)
            // 2. Draw smeared buffer at (-1, -1) offset
            // 3. XOR with original at normal position
            // The (-1,-1) offset means outline appears on ALL sides.
            // The -1 in our loop accounts for this offset.
            // Outline/shadow uses a binary smear test (>=128
            // coverage counts as "set") — the halo stroke is
            // not antialiased.
            'smear: for dy in -1..=smear_max {
                for dx in -1..=dx_max {
                    if get_underlined_pixel(y - dy, x - dx) >= 128 {
                        is_smeared = true;
                        break 'smear;
                    }
                }
            }
        }

        // Outline/shadow halo: full-coverage where smeared
        // but the original glyph pixel is empty — producing
        // the hollow/shifted silhouette effect.
        pixel = if is_smeared && get_underlined_pixel(y, x) < 128 {
            255
        } else {
            0
        };
    }

    pixel
}

fn classic_underline_has_descender(
    baseline: i16, x: i16, descent: i16, coverage: impl Fn(i16, i16) -> u8,
) -> bool {
    (0..=descent).any(|dy| [-1, 0, 1].into_iter()
        .any(|dx| coverage(baseline + dy, x + dx) >= 128))
}

/// Native per-character underline strokes, independent of glyph coverage.
/// Presentation can smooth the glyph without changing descender gaps or making
/// overlapping underline pixels translucent. Outline/shadow callers apply the
/// effect after combining these strokes with the basic glyph. Only unscaled
/// strikes qualify; descender gaps use basic glyph ink before halo synthesis.
#[doc(hidden)]
pub fn classic_textedit_underline_ink(
    font: i16, size: i16, byte: u8, face: u8,
) -> Option<Vec<(i16, i16)>> {
    if face >= 128 { return None; }
    if face & 4 == 0 { return Some(Vec::new()); }
    let (_, scale) = crate::quickdraw::fonts::get_font_face_scaled(font, size);
    if scale != 1 { return None; }
    let (hit, precaptured) = if face & 2 != 0 {
        get_glyph_italic(font, size, byte as char).map(|hit| (Some(hit), true))
            .unwrap_or_else(|| (get_glyph(font, size, byte as char), false))
    } else { (get_glyph(font, size, byte as char), false) };
    let Some((glyph, data)) = hit else { return Some(Vec::new()); };
    let metrics = get_font_metrics(font, size);
    let synthetic = face & 2 != 0 && !precaptured;
    let extend = if synthetic {
        crate::quickdraw::fonts::style::get_italic_underline_extend_left(font, size, face & 1 != 0, precaptured)
    } else { 0 };
    let offset = crate::quickdraw::fonts::style::get_underline_offset(font, size, glyph, face & 16 != 0);
    let right_extend = if synthetic {
        crate::quickdraw::fonts::style::get_italic_underline_extend_right(font, size)
    } else { 0 };
    let coverage = |y, x| classic_glyph_coverage(glyph, data, font, size, &metrics,
        1, precaptured, face & 3, (0, 0), None, y, x);
    let mut ink = Vec::new();
    for x in -extend + offset..i16::from(glyph.advance) + i16::from(face & 1 != 0) + offset + right_extend {
        if !classic_underline_has_descender(0, x, metrics.descent, coverage) {
            for y in 1..=get_underline_thickness(font, size) { ink.push((x, y)); }
        }
    }
    Some(ink)
}

/// Classic 68k TextEdit's per-character binary ink at a zero pen baseline.
/// This retains its integer strike scaling and descender-aware underline.
/// Zero CharExtra/SpaceExtra and no DrawString continuous-underline state;
/// palette, transfer mode, erasure, clipping and ownership stay with the caller.
#[doc(hidden)]
pub fn classic_textedit_glyph_ink(
    font: i16, size: i16, byte: u8, face: u8,
) -> Option<(i32, Vec<(i16, i16)>)> {
    let (_, scale) = crate::quickdraw::fonts::get_font_face_scaled(font, size);
    let style = QuickDrawTextStyle::from_bits(face);
    let (hit, precaptured) = if style.italic() {
        get_glyph_italic(font, size, byte as char).map(|hit| (Some(hit), true))
            .unwrap_or_else(|| (get_glyph(font, size, byte as char), false))
    } else { (get_glyph(font, size, byte as char), false) };
    let Some((glyph, data)) = hit else {
        return Some(((6 + style.advance_extra()) * i32::from(scale), Vec::new()));
    };
    let metrics = get_font_metrics(font, size);
    let fs = i32::from(scale);
    let ascent = i32::from(metrics.ascent) * fs;
    let descent = i32::from(metrics.descent) * fs;
    let left = i32::from(glyph.origin_x) * fs;
    let visual_top = i32::from(glyph.origin_y) * fs;
    let bottom = visual_top + i32::from(glyph.height) * fs;
    let right = left + i32::from(glyph.width) * fs;
    let synthetic = style.italic() && !precaptured;
    let pad = if style.shadow() { 2 } else if style.outline() { 1 } else { 0 };
    let italic_extend = if synthetic && style.underline() {
        crate::quickdraw::fonts::style::get_italic_underline_extend_left(font, size, style.bold(), false)
    } else { 0 };
    let offset = if style.underline() {
        crate::quickdraw::fonts::style::get_underline_offset(font, size, glyph, style.shadow())
    } else { 0 };
    let draw_left = (-pad - i32::from(italic_extend) + i32::from(offset)).min(left - pad);
    let draw_right = (i32::from(glyph.advance) * fs + style.advance_extra() + pad + 2 + i32::from(offset))
        .max(right + i32::from(style.bold()) + if synthetic { (ascent + descent) / 2 } else { 0 } + pad + 2);
    let draw_top = (-ascent).min(visual_top) - pad;
    let draw_bottom = if style.underline() && style.shadow() {
        (bottom + pad + 1).max(descent + pad + 1).max(6)
    } else if style.underline() {
        (bottom + pad).max(1 + i32::from(get_underline_thickness(font, size)) + 1)
    } else if style.shadow() { (bottom + pad + 1).max(descent + pad + 1) }
    else { bottom + pad };
    let (left, top, right, bottom) = if !(style.bold() || synthetic || style.underline() || style.outline() || style.shadow()) {
        (left, visual_top, right, bottom)
    } else { (draw_left, draw_top, draw_right, draw_bottom) };
    let (left, top, right, bottom) = (i16::try_from(left).ok()?, i16::try_from(top).ok()?,
        i16::try_from(right).ok()?, i16::try_from(bottom).ok()?);
    // The coverage path itself uses the native i16 scaled metrics.
    i16::try_from(ascent).ok()?;
    i16::try_from(descent).ok()?;
    let mut pixels = Vec::new();
    for y in top..bottom { for x in left..right {
        if classic_glyph_coverage(glyph, data, font, size, &metrics, scale, precaptured,
            face, (0, 0), None, y, x) >= crate::quickdraw::fonts::MONO_COVERAGE_THRESHOLD {
            pixels.push((x, y));
        }
    } }
    Some((i32::from(glyph.advance) * fs + style.advance_extra(), pixels))
}

#[cfg(test)]
mod classic_textedit_ink_tests {
    use super::*;

    #[test]
    fn classic_halo_underline_recipe_preserves_descenders_and_everything_style() {
        use std::collections::BTreeSet;
        for face in (0u8..128).filter(|face| face & 4 != 0 && face & 24 != 0) {
            for byte in b"g W\x8e" {
                let (glyph, data) = get_glyph(3, 12, *byte as char).unwrap();
                let metrics = get_font_metrics(3, 12);
                let mut base = BTreeSet::new();
                for y in -20..10 { for x in -10..30 {
                    if classic_glyph_coverage(glyph, data, 3, 12, &metrics,
                        1, false, face & 3, (0, 0), None, y, x) >= 128 {
                        base.insert((x, y));
                    }
                } }
                base.extend(classic_textedit_underline_ink(3, 12, *byte, face).unwrap());
                let radius = if face & 24 == 24 { 3 } else if face & 16 != 0 { 2 } else { 1 };
                let mut halo = BTreeSet::new();
                for &(x, y) in &base {
                    for dy in -1..=radius { for dx in -1..=radius {
                        let target = (x + dx, y + dy);
                        // Classic's Everything style restricts horizontal smear
                        // at baseline and first underline row, before exclusion.
                        if face & 31 == 31 && matches!(target.1, 0 | 1) && dx > 1 { continue; }
                        if !base.contains(&target) { halo.insert(target); }
                    } }
                }
                let (_, native) = classic_textedit_glyph_ink(3, 12, *byte, face).unwrap();
                assert_eq!(halo, native.into_iter().collect(), "face={face}, byte={byte}");
            }
        }
        assert!(classic_textedit_underline_ink(3, 12, b'g', 128).is_none());
    }

    #[test]
    fn ppc_halo_underline_recipe_preserves_run_extent_and_per_glyph_exclusion() {
        use std::collections::BTreeSet;
        let bytes = b"g W\x8e";
        for face in (0u8..128).filter(|face| face & 4 != 0 && face & 24 != 0) {
            let (advance, native) = ppc_styled_run_ink(3, 12, face, bytes);
            let ribbon = ppc_styled_run_halo_underline_ink(3, 12, face, i32::from(advance));
            assert!(!ribbon.is_empty());
            let radius = if face & 24 == 24 { 3 } else if face & 16 != 0 { 2 } else { 1 };
            let mut actual = BTreeSet::new();
            let mut pen = 0;
            for byte in bytes {
                let (_, glyph) = ppc_styled_run_ink(3, 12, face & 3, &[*byte]);
                let mut base: BTreeSet<_> = glyph.into_iter().map(|(x, y)| (pen + x, y)).collect();
                base.extend(ribbon.iter().copied());
                for &(x, y) in &base {
                    for dy in -1..=radius { for dx in -1..=radius {
                        let target = (x + dx, y + dy);
                        if !base.contains(&target) { actual.insert(target); }
                    } }
                }
                pen += i32::from(ppc_styled_run_ink(3, 12, face, &[*byte]).0);
            }
            assert_eq!(pen, i32::from(advance));
            assert_eq!(actual, native.into_iter().collect(), "face={face}");
        }
        assert!(ppc_styled_run_halo_underline_ink(3, 12, 4, 10).is_empty());
        assert!(ppc_styled_run_halo_underline_ink(3, 12, 12, 0).is_empty());
    }

    #[test]
    fn shared_classic_coverage_honors_continuous_underline_breaks() {
        let (glyph, data) = get_glyph(3, 12, ' ').unwrap();
        let metrics = get_font_metrics(3, 12);
        let end = i16::from(glyph.advance);
        assert!(end > 1);
        let breaks = [std::collections::HashSet::from([1])];
        for x in 0..end {
            let coverage = classic_glyph_coverage(glyph, data, 3, 12, &metrics,
                1, false, 4, (0, 0), Some((0, end, &breaks)), 1, x);
            assert_eq!(coverage, if x == 1 { 0 } else { 255 });
        }
        assert_eq!(classic_glyph_coverage(glyph, data, 3, 12, &metrics,
            1, false, 4, (0, 0), Some((0, end, &breaks)), 1, end), 0);
    }
}
