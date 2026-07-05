// SPDX-License-Identifier: OFL-1.1
// Copyright (c) 2026 Ben Letchford. Bitmap glyph artwork under the SIL
// Open Font License 1.1 (see OFL.txt at the crate root). Reserved Font
// Name "Systemless".
//! Venice 14 — the classic Mac "Venice" calligraphic script face (font
//! ID 5). On the systemless reference System the Venice NFNT is not
//! installed, so QuickDraw substitutes the application font: font 5 at
//! 14px renders as Geneva 14. This is verified pixel-for-pixel against
//! the `venice_14` reference fixture, whose glyph grid and cell metrics
//! match Geneva 14 exactly. This module therefore
//! reuses Kurrajong 14's glyph art and Geneva 14 metrics rather than
//! shipping a divergent, inauthentic face.

use super::{data_len, decode_data, decode_glyphs, DecodedFace};
use crate::quickdraw::fonts::{FontMetrics, Glyph};

pub use super::geneva14::GENEVA_14_SRC as VENICE_14_SRC;

pub const VENICE_14_METRICS: FontMetrics =
    FontMetrics { ascent: 14, descent: 4, wid_max: 15, leading: 1 };

const LEN: usize = data_len(VENICE_14_SRC);
pub const VENICE_14_GLYPHS: [Glyph; 95] = decode_glyphs(VENICE_14_SRC);
pub const VENICE_14_DATA: [u8; LEN] = decode_data(VENICE_14_SRC);

#[allow(dead_code)]
pub const FACE: DecodedFace = DecodedFace {
    metrics: VENICE_14_METRICS,
    glyphs: &VENICE_14_GLYPHS,
    data: &VENICE_14_DATA,
};
