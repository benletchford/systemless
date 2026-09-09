//! Logical metrics from separately licensed classic-layout data components.

use super::{FONT_APPLICATION, FONT_GENEVA};

pub(super) const FIRST_ASCII_CODE: u8 = 0x20;
pub(super) const LAST_ASCII_CODE: u8 = 0x7E;

static GENEVA9_ADVANCES: &[u8; 95] = include_bytes!("geneva9-advances.bin");

pub(super) fn bundled_advances(font_id: i16, size: i16) -> Option<&'static [u8; 95]> {
    if size == 9 && matches!(font_id, FONT_APPLICATION | FONT_GENEVA) {
        Some(GENEVA9_ADVANCES)
    } else {
        None
    }
}
