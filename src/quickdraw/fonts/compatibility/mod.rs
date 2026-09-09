//! Logical metrics from separately licensed classic-layout data components.

use super::{FONT_APPLICATION, FONT_GENEVA, FONT_MONACO};

pub(super) const FIRST_ASCII_CODE: u8 = 0x20;
pub(super) const LAST_ASCII_CODE: u8 = 0x7E;

static GENEVA9_ADVANCES: &[u8; 95] = include_bytes!("geneva9-advances.bin");
// Inside Macintosh: Text (1993), p. 4-91 defines FOND.ffWidMax as the
// normalized maximum glyph width for a one-point font.
const MONACO_MAX_ADVANCE_UNITS: i32 = 1552;
const MONACO_UNITS_PER_EM: i32 = 2048;

pub(super) fn bundled_advances(font_id: i16, size: i16) -> Option<&'static [u8; 95]> {
    if size == 9 && matches!(font_id, FONT_APPLICATION | FONT_GENEVA) {
        Some(GENEVA9_ADVANCES)
    } else {
        None
    }
}

pub(super) fn bundled_wid_max(font_id: i16, size: i16) -> Option<i16> {
    // Monaco's scalable classic-family metadata records a 1552/2048-em
    // maximum advance. Match outline metric rounding at every requested size.
    (font_id == FONT_MONACO && size > 0).then(|| {
        ((MONACO_MAX_ADVANCE_UNITS * i32::from(size) + MONACO_UNITS_PER_EM / 2)
            / MONACO_UNITS_PER_EM) as i16
    })
}
