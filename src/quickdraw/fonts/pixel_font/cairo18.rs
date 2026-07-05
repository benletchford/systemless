// SPDX-License-Identifier: OFL-1.1
// Copyright (c) 2026 Ben Letchford. Bitmap glyph artwork under the SIL
// Open Font License 1.1 (see OFL.txt at the crate root). Reserved Font
// Name "Systemless".
//! Cairo 18 — the classic Mac "Cairo" picture/dingbat font (font ID 11).
//! On the systemless reference System the Cairo NFNT is not installed, so
//! QuickDraw substitutes the application font: font 11 at 18px renders as
//! Geneva 18. This is verified pixel-for-pixel against the `cairo_18`
//! reference fixture, which matches Geneva 18 exactly across every glyph
//! slot. This module therefore reuses
//! Kurrajong 18's glyph art and Geneva 18 metrics rather than shipping a
//! divergent dingbat palette the reference System never shows.

use super::{data_len, decode_data, decode_glyphs, DecodedFace};
use crate::quickdraw::fonts::{FontMetrics, Glyph};

pub use super::geneva18::GENEVA_18_SRC as CAIRO_18_SRC;

pub const CAIRO_18_METRICS: FontMetrics =
    FontMetrics { ascent: 18, descent: 4, wid_max: 18, leading: 1 };

const LEN: usize = data_len(CAIRO_18_SRC);
pub const CAIRO_18_GLYPHS: [Glyph; 95] = decode_glyphs(CAIRO_18_SRC);
pub const CAIRO_18_DATA: [u8; LEN] = decode_data(CAIRO_18_SRC);

#[allow(dead_code)]
pub const FACE: DecodedFace = DecodedFace {
    metrics: CAIRO_18_METRICS,
    glyphs: &CAIRO_18_GLYPHS,
    data: &CAIRO_18_DATA,
};
