//! Hand-authored bitmap fonts, decoded at compile time.
//!
//! A face is a slice of [`GlyphSrc`] — one entry per ASCII code
//! `0x20..=0x7E` — where each glyph is written as ASCII art: `'#'` is
//! an inked pixel, `'.'` is empty. This is the *source of truth* for
//! the glyph; it lives directly in this crate as readable Rust, with
//! no external font file and no offline baker.
//!
//! [`decode_glyphs`] / [`decode_data`] are `const fn`s, so a face
//! lowers to the exact same static `[Glyph; 95]` + coverage-byte
//! buffer the runtime already blits (`0` or `255` per pixel). There is
//! no runtime rasterization and no startup cost — the tables are baked
//! by the compiler, not by a build step.
//!
//! Per glyph we store `advance` and the `(origin_x, origin_y)` offset
//! of the bitmap's top-left from the pen position (origin_y is
//! negative above the baseline, matching [`Glyph`]); width and height
//! are derived from the art itself.

use super::{FontMetrics, Glyph};

pub mod application12;
pub mod cairo18;
pub mod chicago12;
pub mod chicago9;
pub mod geneva10;
pub mod geneva12;
pub mod geneva14;
pub mod geneva18;
pub mod geneva24;
pub mod geneva9;
pub mod london18;
pub mod monaco10;
pub mod monaco12;
pub mod monaco9;
pub mod newyork12;
pub mod newyork14;
pub mod newyork18;
pub mod venice14;

/// Readable source for one glyph. `rows` is the ASCII-art bitmap:
/// every row must be the same length (the glyph width); `'#'` inks a
/// pixel, any other byte (`'.'`) leaves it clear. An empty `rows`
/// slice is a zero-size glyph (e.g. a spacer) carrying only `advance`.
pub struct GlyphSrc {
    pub advance: u8,
    pub origin_x: i8,
    pub origin_y: i8,
    pub rows: &'static [&'static str],
}

/// `g!(advance, (origin_x, origin_y), "row" "row" …)` — one glyph.
#[macro_export]
macro_rules! g {
    ($adv:expr, ($ox:expr, $oy:expr), $($row:literal)*) => {
        $crate::quickdraw::fonts::pixel_font::GlyphSrc {
            advance: $adv,
            origin_x: $ox,
            origin_y: $oy,
            rows: &[$($row),*],
        }
    };
}

const fn glyph_width(g: &GlyphSrc) -> usize {
    if g.rows.is_empty() {
        0
    } else {
        g.rows[0].len()
    }
}

/// Total coverage bytes a face occupies: `sum(width * height)`.
pub const fn data_len(src: &[GlyphSrc]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < src.len() {
        total += glyph_width(&src[i]) * src[i].rows.len();
        i += 1;
    }
    total
}

/// Lower a face's ASCII art into the runtime `[Glyph; N]` descriptor
/// table. `data_offset` values index into the buffer produced by
/// [`decode_data`] over the same `src`.
pub const fn decode_glyphs<const N: usize>(src: &[GlyphSrc]) -> [Glyph; N] {
    let mut out = [Glyph {
        width: 0,
        height: 0,
        advance: 0,
        origin_x: 0,
        origin_y: 0,
        data_offset: 0,
    }; N];
    let mut off = 0;
    let mut i = 0;
    while i < N {
        let g = &src[i];
        let w = glyph_width(g);
        let h = g.rows.len();
        out[i] = Glyph {
            width: w as u8,
            height: h as u8,
            advance: g.advance,
            origin_x: g.origin_x,
            origin_y: g.origin_y,
            data_offset: off,
        };
        off += w * h;
        i += 1;
    }
    out
}

/// Lower a face's ASCII art into the flat coverage buffer (`0`/`255`,
/// row-major, `width * height` bytes per glyph in code-point order).
pub const fn decode_data<const LEN: usize>(src: &[GlyphSrc]) -> [u8; LEN] {
    let mut out = [0u8; LEN];
    let mut pos = 0;
    let mut i = 0;
    while i < src.len() {
        let g = &src[i];
        let w = glyph_width(g);
        let mut r = 0;
        while r < g.rows.len() {
            let bytes = g.rows[r].as_bytes();
            let mut c = 0;
            while c < w {
                out[pos] = if bytes[c] == b'#' { 255 } else { 0 };
                pos += 1;
                c += 1;
            }
            r += 1;
        }
        i += 1;
    }
    out
}

/// One fully-decoded face: `FontMetrics` plus the static glyph and
/// coverage tables, in the exact shape the runtime catalogue consumes.
pub struct DecodedFace {
    pub metrics: FontMetrics,
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
}
