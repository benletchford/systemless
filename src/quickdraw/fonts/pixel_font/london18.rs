// SPDX-License-Identifier: OFL-1.1
// Copyright (c) 2026 Ben Letchford. Bitmap glyph artwork under the SIL
// Open Font License 1.1 (see OFL.txt at the crate root). Reserved Font
// Name "Systemless".
//! London 18 — the classic Mac "London" Old English display face (font
//! ID 6). On the systemless reference System the London NFNT is not
//! installed, so QuickDraw substitutes the application font: font 6 at
//! 18px renders as Geneva 18. This is verified pixel-for-pixel against
//! the `london_18` reference fixture, whose glyph grid and cell metrics
//! match Geneva 18 exactly. This module therefore
//! reuses Kurrajong 18's glyph art and Geneva 18 metrics rather than
//! shipping a divergent, inauthentic face.

use super::{data_len, decode_data, decode_glyphs, DecodedFace};
use crate::quickdraw::fonts::{FontMetrics, Glyph};

pub use super::geneva18::GENEVA_18_SRC as LONDON_18_SRC;

pub const LONDON_18_METRICS: FontMetrics = FontMetrics {
    ascent: 18,
    descent: 4,
    wid_max: 18,
    leading: 1,
};

const LEN: usize = data_len(LONDON_18_SRC);
pub const LONDON_18_GLYPHS: [Glyph; 95] = decode_glyphs(LONDON_18_SRC);
pub const LONDON_18_DATA: [u8; LEN] = decode_data(LONDON_18_SRC);

#[allow(dead_code)]
pub const FACE: DecodedFace = DecodedFace {
    metrics: LONDON_18_METRICS,
    glyphs: &LONDON_18_GLYPHS,
    data: &LONDON_18_DATA,
};
