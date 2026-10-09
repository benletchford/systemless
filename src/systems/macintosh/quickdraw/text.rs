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
            base_pixels.insert((source_x, 1));
        }
    }

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
