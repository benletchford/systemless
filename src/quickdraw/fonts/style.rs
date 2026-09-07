//! Shared QuickDraw style synthesis.

pub use super::families::*;
use super::{FontMetrics, Glyph};

/// QuickDraw synthesizes italic, bold and underline when no styled strike is
/// available (Inside Macintosh: Text, pp. 3-5–3-7). Use the active font's
/// metrics; no family or point-size-specific artwork corrections are needed.
pub fn get_italic_slant(
    _font_id: i16,
    _size: i16,
    metrics: &FontMetrics,
    baseline_y: i16,
    curr_y: i16,
) -> i16 {
    ((baseline_y.saturating_add(metrics.descent).saturating_sub(1)).saturating_sub(curr_y)).max(0)
        / 2
}

pub fn get_italic_slant_for_underline(
    font_id: i16,
    size: i16,
    metrics: &FontMetrics,
    baseline_y: i16,
    check_y: i16,
) -> i16 {
    get_italic_slant(font_id, size, metrics, baseline_y, check_y)
}

pub fn get_italic_underline_extend_left(
    _font_id: i16,
    size: i16,
    _is_bold: bool,
    _use_precaptured_italic: bool,
) -> i16 {
    (size / 12).max(1)
}
pub fn get_italic_underline_extend_right(_font_id: i16, _size: i16) -> i16 {
    0
}
pub fn get_underline_offset(_font_id: i16, _size: i16, _glyph: &Glyph, is_shadow: bool) -> i16 {
    -i16::from(is_shadow)
}
pub fn get_italic_end_extend(_font_id: i16, _size: i16, metrics: &FontMetrics) -> i16 {
    (metrics.descent / 2 + 1).max(1)
}
pub fn use_smart_underline_break(_font_id: i16, _size: i16) -> bool {
    false
}
pub fn use_baseline_analysis(_font_id: i16, _size: i16) -> bool {
    false
}
