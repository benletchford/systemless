// SPDX-License-Identifier: OFL-1.1
// Copyright (c) 2026 Ben Letchford. Bitmap glyph artwork under the SIL
// Open Font License 1.1 (see OFL.txt at the crate root). Reserved Font
// Name "Systemless".
//! Application 12 — the classic Mac "application font" at 12px (font
//! ID 1), which historically resolves to Geneva. systemless renders it
//! with the Kurrajong 12 design: this module reuses Kurrajong 12's
//! original glyph art rather than duplicating the bitmaps, and only
//! carries its own metrics constant. Not derived from any third-party
//! font.

use super::{data_len, decode_data, decode_glyphs, DecodedFace};
use crate::quickdraw::fonts::{FontMetrics, Glyph};
pub use super::geneva12::GENEVA_12_SRC as APPLICATION_12_SRC;

pub const APPLICATION_12_METRICS: FontMetrics =
    FontMetrics { ascent: 12, descent: 3, wid_max: 12, leading: 0 };

const LEN: usize = data_len(APPLICATION_12_SRC);
pub const APPLICATION_12_GLYPHS: [Glyph; 95] = decode_glyphs(APPLICATION_12_SRC);
pub const APPLICATION_12_DATA: [u8; LEN] = decode_data(APPLICATION_12_SRC);

#[allow(dead_code)]
pub const FACE: DecodedFace = DecodedFace {
    metrics: APPLICATION_12_METRICS,
    glyphs: &APPLICATION_12_GLYPHS,
    data: &APPLICATION_12_DATA,
};
