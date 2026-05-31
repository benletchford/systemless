//! Font-specific rendering heuristics
//!
//! This module contains font-specific adjustments and heuristics for text rendering.
//! These are empirically derived values that match classic QuickDraw behavior for
//! specific font/size combinations.

use super::{FontMetrics, Glyph};

/// Font ID constants for readability
/// See Inside Macintosh: Text, Appendix B for the complete list
pub const FONT_CHICAGO: i16 = 0;
pub const FONT_APPLICATION: i16 = 1; // Alias for system font
pub const FONT_NEWYORK: i16 = 2;
pub const FONT_GENEVA: i16 = 3;
pub const FONT_MONACO: i16 = 4;
pub const FONT_VENICE: i16 = 5;
pub const FONT_LONDON: i16 = 6;
pub const FONT_ATHENS: i16 = 7;
pub const FONT_SANFRAN: i16 = 8;
pub const FONT_TORONTO: i16 = 9;
pub const FONT_SEATTLE: i16 = 10;
pub const FONT_CAIRO: i16 = 11;
pub const FONT_LOSANGELES: i16 = 12;
pub const FONT_TIMES: i16 = 20;
pub const FONT_HELVETICA: i16 = 21;
pub const FONT_COURIER: i16 = 22;
pub const FONT_SYMBOL: i16 = 23;
pub const FONT_MOBILE: i16 = 24;

/// Calculate the italic slant offset for a given row position.
///
/// The slant determines how many pixels to shift right for italic rendering.
/// Different fonts use different slant algorithms based on their design.
pub fn get_italic_slant(
    font_id: i16,
    size: i16,
    metrics: &FontMetrics,
    baseline_y: i16,
    curr_y: i16,
) -> i16 {
    // Geneva 9 uses a special reduced-height slant calculation
    if font_id == FONT_GENEVA && size == 9 {
        let dy = curr_y - baseline_y;
        return match dy {
            -10..=-5 => 1, // Body slant of 1 pixel
            _ => 0,
        };
    }

    // Chicago 9: empirically derived slant values
    if font_id == FONT_CHICAGO && size == 9 {
        let dy = curr_y - baseline_y;
        return match dy {
            ..=-6 => 2,
            -5..=-3 => 1,
            _ => 0,
        };
    }

    // Monaco 10: empirically derived slant values
    if font_id == FONT_MONACO && size == 10 {
        let dy = curr_y - baseline_y;
        return match dy {
            ..=-8 => 3,
            -7..=-6 => 2,
            -5..=-3 => 1,
            _ => 0,
        };
    }

    // Generic QuickDraw slant calculation
    let font_height = metrics.ascent + metrics.descent;
    let font_top = baseline_y - metrics.ascent;
    let row = (curr_y - font_top).max(0);

    if font_height % 2 == 0 {
        // Even height: shift = 0 for row 0, then (row/2)+1
        let shift = if row == 0 { 0 } else { (row / 2) + 1 };
        (font_height / 2 - shift).max(0)
    } else {
        // Odd height: use distance from bottom
        let dy = ((baseline_y + metrics.descent - 1) - curr_y).max(0);
        dy / 2
    }
}

/// Calculate the italic slant for underline descender checking.
/// This is slightly different from the main italic slant in some cases.
pub fn get_italic_slant_for_underline(
    font_id: i16,
    size: i16,
    metrics: &FontMetrics,
    baseline_y: i16,
    check_y: i16,
) -> i16 {
    // Geneva 9: use reference algorithm with reduced fRectHeight
    if font_id == FONT_GENEVA && size == 9 {
        let font_height = 7i16;
        let font_top = baseline_y - metrics.ascent;
        let row = (check_y - font_top).max(0);
        let shift_count = (row + 1) / 2;
        return (font_height / 2 - shift_count).max(0);
    }

    // Default: use distance from bottom
    let font_bottom = baseline_y + metrics.descent;
    let dy = ((font_bottom - 1) - check_y).max(0);
    dy / 2
}

/// Get the left extension for italic underlines.
/// This determines how far left the underline should extend beyond the character.
pub fn get_italic_underline_extend_left(
    font_id: i16,
    size: i16,
    is_bold: bool,
    use_precaptured_italic: bool,
) -> i16 {
    if font_id == FONT_GENEVA && size == 24 {
        // Executor formula: descent/2 + 1 = 4, but 3 works better for Bold+Italic
        return 3;
    }

    if font_id == FONT_GENEVA && size == 9 && use_precaptured_italic {
        // Pre-captured italic glyphs have slightly different positioning
        return 2;
    }

    let base = if size >= 18 { 2 } else { 1 };

    // Geneva 14 Bold needs extra pixel
    if is_bold && size == 14 && font_id == FONT_GENEVA {
        return base + 1;
    }

    // Venice 14 Bold+Italic needs extra pixel
    if is_bold && size == 14 && font_id == FONT_VENICE {
        return base + 1;
    }

    base
}

/// Get the right extension for italic underlines.
pub fn get_italic_underline_extend_right(font_id: i16, size: i16) -> i16 {
    if font_id == FONT_GENEVA && size == 24 {
        // Executor formula: ascent/2 - 1 = 10, but testing shows 4 is correct
        return 4;
    }
    0
}

/// Get the underline position offset.
/// Some fonts need the underline shifted for proper alignment.
pub fn get_underline_offset(font_id: i16, size: i16, glyph: &Glyph, is_shadow: bool) -> i16 {
    let mut off = 0;

    // Geneva 24 has kernMax -1, needs shift for chars with origin_x <= 0
    if font_id == FONT_GENEVA && size == 24
        && glyph.origin_x <= 0 {
            off -= 1;
        }

    // Shadow always shifts left by 1
    if is_shadow {
        off -= 1;
    }

    off
}

/// Calculate extra extension at end of italic text for underline.
pub fn get_italic_end_extend(font_id: i16, size: i16, metrics: &FontMetrics) -> i16 {
    // For small fonts (size <= 10), use Executor's formula: ascent / 2 - 1
    // For larger fonts, use descent-based formula with floor
    let base = if size <= 10 {
        (metrics.ascent / 2 - 1).max(0)
    } else {
        (metrics.descent / 2 + 2).max(4) + 1 // At least 5 pixels
    };

    match size {
        14 => base + 1,
        18 => base + 3, // NY18 Italic needs extra for spill-over
        9 if font_id == FONT_MONACO => base + 1, // Monaco 9 italic underline
        _ => base,
    }
}

/// Whether to use "smart break" logic for underline descender detection.
/// Smart break logic measures horizontal run length to decide break width.
pub fn use_smart_underline_break(font_id: i16, size: i16) -> bool {
    size == 18 || (font_id == FONT_GENEVA && size == 24)
}

/// Whether this font/size combination requires NY18-style baseline analysis.
/// This is used for detecting descenders that shouldn't break the underline.
pub fn use_baseline_analysis(font_id: i16, size: i16) -> bool {
    font_id == FONT_NEWYORK && size == 18
}
