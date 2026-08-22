// SPDX-License-Identifier: OFL-1.1
// Copyright (c) 2026 Ben Letchford. Bitmap glyph artwork under the SIL
// Open Font License 1.1 (see OFL.txt at the crate root). Reserved Font
// Name "Systemless".
//! Systemless-original fallbacks for the special symbols used by the
//! standard Menu Manager definition procedure. Inside Macintosh Volume I
//! (1985), pp. I-358 and I-369 define the Command and checkmark character
//! codes that appear beside menu items.

use super::{data_len, decode_data, decode_glyphs, GlyphSrc};
use crate::g;
use crate::quickdraw::fonts::Glyph;

const COMMAND_KEY_SRC: &[GlyphSrc] = &[g!(9, (1, -8),
    ".##.##."
    "#..#..#"
    "#..#..#"
    ".#####."
    "#..#..#"
    "#..#..#"
    ".##.##."
)];
const COMMAND_KEY_LEN: usize = data_len(COMMAND_KEY_SRC);
static COMMAND_KEY_GLYPHS: [Glyph; 1] = decode_glyphs(COMMAND_KEY_SRC);
static COMMAND_KEY_DATA: [u8; COMMAND_KEY_LEN] = decode_data(COMMAND_KEY_SRC);

const CHECKMARK_SRC: &[GlyphSrc] = &[g!(9, (1, -7),
    "......#"
    ".....##"
    "#...##."
    "##.##.."
    ".###..."
    "..#...."
)];
const CHECKMARK_LEN: usize = data_len(CHECKMARK_SRC);
static CHECKMARK_GLYPHS: [Glyph; 1] = decode_glyphs(CHECKMARK_SRC);
static CHECKMARK_DATA: [u8; CHECKMARK_LEN] = decode_data(CHECKMARK_SRC);

const TRADEMARK_SRC: &[GlyphSrc] = &[g!(6, (0, -8),
    "###.#.#"
    ".#..###"
    ".#..#.#"
)];
const TRADEMARK_LEN: usize = data_len(TRADEMARK_SRC);
static TRADEMARK_GLYPHS: [Glyph; 1] = decode_glyphs(TRADEMARK_SRC);
static TRADEMARK_DATA: [u8; TRADEMARK_LEN] = decode_data(TRADEMARK_SRC);

pub(crate) fn get_glyph(ch: char) -> Option<(&'static Glyph, &'static [u8])> {
    match ch {
        '\u{2318}' => Some((&COMMAND_KEY_GLYPHS[0], &COMMAND_KEY_DATA)),
        '\u{2713}' => Some((&CHECKMARK_GLYPHS[0], &CHECKMARK_DATA)),
        '\u{2122}' => Some((&TRADEMARK_GLYPHS[0], &TRADEMARK_DATA)),
        _ => None,
    }
}
