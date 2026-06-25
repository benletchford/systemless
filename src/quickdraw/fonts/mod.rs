//! Built-in DejaVu font baking + Mac font-family routing.
//!
//! Text renders by blitting pre-computed 8-bit coverage bitmaps out
//! of the static `baked` module — no runtime rasterization, no
//! hinting decisions, no threshold knobs, no CLUT-closest-match.
//! Every glyph pixel in `baked::*_DATA` is exactly 0 or 255, so the
//! downstream `ShapeOp::Glyph` partial-alpha branch never fires.
//!
//! The baker runs FreeType with TARGET_MONO + FORCE_AUTOHINT and emits
//! `baked.rs`. All three DejaVu TTFs (Sans, Sans Mono, Serif) ship
//! embedded bitmap strikes at 10-17 px, so text output at those sizes
//! is pixel-perfect (strikes used directly). At sizes without a strike
//! (9, 18, 24 px) FreeType falls back to autohinted vector rendering.
//!
//! Face routing:
//!
//! | Mac font family                                    | Backing TTF      |
//! |----------------------------------------------------|------------------|
//! | Monaco, Courier                                    | DejaVuSansMono   |
//! | New York, Times                                    | DejaVuSerif      |
//! | Chicago, Geneva, Application, Venice, London, Cairo | DejaVuSans       |
//!
//! LICENSE: DejaVu fonts ship under the DejaVu free-font license
//! (derivative of the Bitstream Vera license, essentially public-
//! domain-equivalent). See `assets/fonts/LICENSE.DejaVu.txt` — the
//! bitmap strikes in `baked.rs` are a derivative work.

mod baked;
pub mod heuristics;
pub mod override_format;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

pub use self::heuristics::{
    FONT_APPLICATION, FONT_ATHENS, FONT_CAIRO, FONT_CHICAGO, FONT_COURIER, FONT_GENEVA,
    FONT_HELVETICA, FONT_LONDON, FONT_LOSANGELES, FONT_MOBILE, FONT_MONACO, FONT_NEWYORK,
    FONT_SANFRAN, FONT_SEATTLE, FONT_SYMBOL, FONT_TIMES, FONT_TORONTO, FONT_VENICE,
};

/// Single-character bitmap descriptor: dimensions + offset into the
/// shared `data` byte buffer. Coverage bytes are 0 or 255 (mono) for
/// strike-baked sizes; intermediate values appear when FreeType has
/// to fall back to autohinted vector rendering at unbaked sizes.
pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub advance: u8,
    pub origin_x: i8,
    pub origin_y: i8,
    pub data_offset: usize,
}

/// Mac-style font metrics for a single (face, size) pair.
/// Returned by `GetFontInfo` ($A88B) via `get_font_metrics`.
#[derive(Copy, Clone)]
pub struct FontMetrics {
    pub ascent: i16,
    pub descent: i16,
    pub wid_max: i16,
    pub leading: i16,
}

/// One baked (font_id, size) face: metrics plus a slice of glyph
/// descriptors and a shared coverage-byte buffer that the descriptors'
/// `data_offset` fields index into.
pub struct FontFace {
    pub font_id: i16,
    pub size: i16,
    pub metrics: FontMetrics,
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
}

/// Mac Roman extended glyph (code 0x80..=0xFF). Preserved for callers
/// that expect the type; the Go-baked catalogue doesn't populate Mac
/// Roman extended tables yet so `get_macroman_glyph` returns None.
pub struct MacRomanGlyph {
    pub mac_code: u8,
    pub glyph: Glyph,
}

/// `FontFace` analogue covering Mac Roman extended characters
/// (codes 0x80..=0xFF). Currently has no baked entries — the Go
/// glyph baker doesn't populate these — but the type stays so the
/// `get_macroman_glyph` API surface is stable.
pub struct MacRomanFace {
    pub font_id: i16,
    pub size: i16,
    pub glyphs: &'static [MacRomanGlyph],
    pub data: &'static [u8],
}

/// `FontFace` analogue with pre-baked italic strikes. Synthesised
/// at draw time today via shear-blit (no italic strikes baked yet),
/// but the type is in place so a future bake step can plug in.
pub struct ItalicFace {
    pub font_id: i16,
    pub size: i16,
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
}

/// Threshold at which coverage is treated as "fully set" when
/// collapsing to a 1-bit destination. Baked glyph data is exclusively
/// 0 or 255 so this is effectively inert for the glyph path; kept for
/// shape-op callers that share the threshold constant.
pub const MONO_COVERAGE_THRESHOLD: u8 = 128;

// --- Static catalogue ----------------------------------------------------

struct PackedFace {
    font_id: i16,
    size: i16,
    metrics: FontMetrics,
    glyphs: &'static [Glyph],
    data: &'static [u8],
}

const PACKED_FACES: &[PackedFace] = &[
    PackedFace {
        font_id: FONT_CHICAGO,
        size: 9,
        metrics: baked::CHICAGO_9_METRICS,
        glyphs: &baked::CHICAGO_9_GLYPHS,
        data: baked::CHICAGO_9_DATA,
    },
    PackedFace {
        font_id: FONT_CHICAGO,
        size: 12,
        metrics: baked::CHICAGO_12_METRICS,
        glyphs: &baked::CHICAGO_12_GLYPHS,
        data: baked::CHICAGO_12_DATA,
    },
    PackedFace {
        font_id: FONT_APPLICATION,
        size: 12,
        metrics: baked::APPLICATION_12_METRICS,
        glyphs: &baked::APPLICATION_12_GLYPHS,
        data: baked::APPLICATION_12_DATA,
    },
    PackedFace {
        font_id: FONT_NEWYORK,
        size: 12,
        metrics: baked::NEWYORK_12_METRICS,
        glyphs: &baked::NEWYORK_12_GLYPHS,
        data: baked::NEWYORK_12_DATA,
    },
    PackedFace {
        font_id: FONT_NEWYORK,
        size: 14,
        metrics: baked::NEWYORK_14_METRICS,
        glyphs: &baked::NEWYORK_14_GLYPHS,
        data: baked::NEWYORK_14_DATA,
    },
    PackedFace {
        font_id: FONT_NEWYORK,
        size: 18,
        metrics: baked::NEWYORK_18_METRICS,
        glyphs: &baked::NEWYORK_18_GLYPHS,
        data: baked::NEWYORK_18_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 9,
        metrics: baked::GENEVA_9_METRICS,
        glyphs: &baked::GENEVA_9_GLYPHS,
        data: baked::GENEVA_9_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 10,
        metrics: baked::GENEVA_10_METRICS,
        glyphs: &baked::GENEVA_10_GLYPHS,
        data: baked::GENEVA_10_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 12,
        metrics: baked::GENEVA_12_METRICS,
        glyphs: &baked::GENEVA_12_GLYPHS,
        data: baked::GENEVA_12_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 14,
        metrics: baked::GENEVA_14_METRICS,
        glyphs: &baked::GENEVA_14_GLYPHS,
        data: baked::GENEVA_14_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 18,
        metrics: baked::GENEVA_18_METRICS,
        glyphs: &baked::GENEVA_18_GLYPHS,
        data: baked::GENEVA_18_DATA,
    },
    PackedFace {
        font_id: FONT_GENEVA,
        size: 24,
        metrics: baked::GENEVA_24_METRICS,
        glyphs: &baked::GENEVA_24_GLYPHS,
        data: baked::GENEVA_24_DATA,
    },
    PackedFace {
        font_id: FONT_MONACO,
        size: 9,
        metrics: baked::MONACO_9_METRICS,
        glyphs: &baked::MONACO_9_GLYPHS,
        data: baked::MONACO_9_DATA,
    },
    PackedFace {
        font_id: FONT_MONACO,
        size: 10,
        metrics: baked::MONACO_10_METRICS,
        glyphs: &baked::MONACO_10_GLYPHS,
        data: baked::MONACO_10_DATA,
    },
    PackedFace {
        font_id: FONT_MONACO,
        size: 12,
        metrics: baked::MONACO_12_METRICS,
        glyphs: &baked::MONACO_12_GLYPHS,
        data: baked::MONACO_12_DATA,
    },
    PackedFace {
        font_id: FONT_VENICE,
        size: 14,
        metrics: baked::VENICE_14_METRICS,
        glyphs: &baked::VENICE_14_GLYPHS,
        data: baked::VENICE_14_DATA,
    },
    PackedFace {
        font_id: FONT_LONDON,
        size: 18,
        metrics: baked::LONDON_18_METRICS,
        glyphs: &baked::LONDON_18_GLYPHS,
        data: baked::LONDON_18_DATA,
    },
    PackedFace {
        font_id: FONT_CAIRO,
        size: 18,
        metrics: baked::CAIRO_18_METRICS,
        glyphs: &baked::CAIRO_18_GLYPHS,
        data: baked::CAIRO_18_DATA,
    },
];

pub static FONT_TABLE: LazyLock<&'static [FontFace]> = LazyLock::new(|| {
    let faces: Vec<FontFace> = PACKED_FACES
        .iter()
        .map(|pf| FontFace {
            font_id: pf.font_id,
            size: pf.size,
            metrics: pf.metrics,
            glyphs: pf.glyphs,
            data: pf.data,
        })
        .collect();
    Box::leak(faces.into_boxed_slice())
});

static MACROMAN_TABLE: LazyLock<&'static [MacRomanFace]> =
    LazyLock::new(|| Box::leak(Vec::<MacRomanFace>::new().into_boxed_slice()));

static ITALIC_TABLE: LazyLock<&'static [ItalicFace]> =
    LazyLock::new(|| Box::leak(Vec::<ItalicFace>::new().into_boxed_slice()));

// --- Font ID ↔ name lookup -----------------------------------------------

pub static FONT_NAMES: &[(i16, &str)] = &[
    (0, "Chicago"),
    (1, "Application"),
    (2, "New York"),
    (3, "Geneva"),
    (4, "Monaco"),
    (5, "Venice"),
    (6, "London"),
    (7, "Athens"),
    (8, "San Francisco"),
    (9, "Toronto"),
    (11, "Cairo"),
    (12, "Los Angeles"),
    (20, "Times"),
    (21, "Helvetica"),
    (22, "Courier"),
    (23, "Symbol"),
    (24, "Mobile"),
];

pub fn font_name_for_id(font_id: i16) -> Option<&'static str> {
    FONT_NAMES
        .iter()
        .find(|(id, _)| *id == font_id)
        .map(|(_, name)| *name)
}

pub fn font_id_for_name(name: &str) -> Option<i16> {
    let needle = name.trim();
    FONT_NAMES
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(needle))
        .map(|(id, _)| *id)
}

#[derive(Default)]
struct OverrideCache {
    env_dir: Option<OsString>,
    faces: HashMap<(i16, i16), &'static FontFace>,
}

/// Optional runtime override map populated from `SYSTEMLESS_ORIGINAL_FONTS_DIR`.
/// Entries here win over the baked DejaVu catalogue — the opt-in hook for
/// substituting authentic Mac bitmap glyphs at runtime without committing
/// Apple-copyrighted data into this repo.
///
/// The cache follows the current env path instead of freezing the first
/// lookup. Test harnesses and embedders can validate or set the local font
/// cache shortly before launching a fixture, after unrelated code has already
/// queried fallback metrics.
static OVERRIDES: LazyLock<Mutex<OverrideCache>> =
    LazyLock::new(|| Mutex::new(OverrideCache::default()));

pub fn get_font_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
    let size = if size == 0 { 12 } else { size };
    if let Some(face) = get_override_font_face(font_id, size) {
        return Some(face);
    }
    get_baked_font_face(font_id, size)
}

#[cfg(test)]
fn get_font_face_with_overrides(
    overrides: &HashMap<(i16, i16), &'static FontFace>,
    font_id: i16,
    size: i16,
) -> Option<&'static FontFace> {
    let size = if size == 0 { 12 } else { size };
    if let Some(face) = overrides.get(&(font_id, size)) {
        return Some(*face);
    }
    get_baked_font_face(font_id, size)
}

fn get_override_font_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
    let env_dir = std::env::var_os("SYSTEMLESS_ORIGINAL_FONTS_DIR");
    let mut cache = OVERRIDES.lock().expect("font override cache poisoned");
    if cache.env_dir != env_dir {
        cache.faces = env_dir
            .as_ref()
            .map(|dir| override_format::load_directory(Path::new(dir)))
            .unwrap_or_default();
        cache.env_dir = env_dir;
    }
    cache.faces.get(&(font_id, size)).copied()
}

fn get_baked_font_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
    FONT_TABLE
        .iter()
        .find(|face| face.font_id == font_id && face.size == size)
}

fn fallback_font_id(font_id: i16) -> Option<i16> {
    match font_id {
        1 => Some(FONT_GENEVA),
        FONT_TIMES => Some(FONT_NEWYORK),
        FONT_HELVETICA => Some(FONT_GENEVA),
        FONT_COURIER => Some(FONT_MONACO),
        _ => None,
    }
}

pub fn get_font_face_or_default(font_id: i16, size: i16) -> &'static FontFace {
    if let Some(face) = get_font_face(font_id, size) {
        return face;
    }
    if let Some(fb) = fallback_font_id(font_id) {
        if let Some(face) = get_font_face(fb, size) {
            return face;
        }
        for scale in [2i16, 3] {
            let base_size = size / scale;
            if base_size * scale == size {
                if let Some(face) = get_font_face(fb, base_size) {
                    return face;
                }
            }
        }
        if let Some(face) = FONT_TABLE.iter().find(|f| f.font_id == fb) {
            return face;
        }
    }
    for scale in [2i16, 3] {
        let base_size = size / scale;
        if base_size * scale == size {
            if let Some(face) = get_font_face(font_id, base_size) {
                return face;
            }
        }
    }
    if let Some(face) = FONT_TABLE.iter().find(|f| f.font_id == font_id) {
        return face;
    }
    if let Some(default_face) = get_font_face(FONT_CHICAGO, 12) {
        return default_face;
    }
    &FONT_TABLE[0]
}

pub fn get_font_face_scaled(font_id: i16, size: i16) -> (&'static FontFace, i16) {
    if let Some(face) = get_font_face(font_id, size) {
        return (face, 1);
    }
    if let Some(fb) = fallback_font_id(font_id) {
        if let Some(face) = get_font_face(fb, size) {
            return (face, 1);
        }
        for scale in [2i16, 3] {
            let base_size = size / scale;
            if base_size * scale == size {
                if let Some(face) = get_font_face(fb, base_size) {
                    return (face, scale);
                }
            }
        }
        if let Some(face) = FONT_TABLE.iter().find(|f| f.font_id == fb) {
            return (face, 1);
        }
    }
    for scale in [2i16, 3] {
        let base_size = size / scale;
        if base_size * scale == size {
            if let Some(face) = get_font_face(font_id, base_size) {
                return (face, scale);
            }
        }
    }
    if let Some(face) = FONT_TABLE.iter().find(|f| f.font_id == font_id) {
        return (face, 1);
    }
    (get_font_face_or_default(font_id, size), 1)
}

pub fn get_macroman_glyph(
    font_id: i16,
    size: i16,
    mac_code: u8,
) -> Option<(&'static Glyph, &'static [u8])> {
    let size = if size == 0 { 12 } else { size };
    let face = MACROMAN_TABLE
        .iter()
        .find(|f| f.font_id == font_id && f.size == size)?;
    face.glyphs
        .iter()
        .find(|e| e.mac_code == mac_code)
        .map(|e| (&e.glyph, face.data))
}

pub fn get_italic_glyph(
    font_id: i16,
    size: i16,
    ch: char,
) -> Option<(&'static Glyph, &'static [u8])> {
    let size = if size == 0 { 12 } else { size };
    let face = ITALIC_TABLE
        .iter()
        .find(|f| f.font_id == font_id && f.size == size)?;
    if !(' '..='~').contains(&ch) {
        return None;
    }
    let idx = (ch as usize) - 32;
    if idx >= face.glyphs.len() {
        return None;
    }
    let glyph = &face.glyphs[idx];
    if glyph.width == 0 && glyph.height == 0 && glyph.advance == 0 {
        return None;
    }
    Some((glyph, face.data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn distinctive_override_blob() -> override_format::Blob {
        let glyphs: Vec<Glyph> = (0..override_format::GLYPH_COUNT)
            .map(|_| Glyph {
                width: 0,
                height: 0,
                advance: 13,
                origin_x: 0,
                origin_y: 0,
                data_offset: 0,
            })
            .collect();
        override_format::Blob {
            font_id: FONT_CHICAGO,
            size: 12,
            style: override_format::STYLE_PLAIN,
            metrics: FontMetrics {
                ascent: 99,
                descent: 11,
                wid_max: 13,
                leading: 7,
            },
            glyphs,
            data: vec![],
        }
    }

    #[test]
    fn every_packed_face_is_accessible() {
        for pf in PACKED_FACES {
            let face = get_font_face(pf.font_id, pf.size)
                .unwrap_or_else(|| panic!("missing ({}, {})", pf.font_id, pf.size));
            assert_eq!(face.glyphs.len(), 95);
        }
    }

    #[test]
    fn default_face_is_chicago_12() {
        let face = get_font_face_or_default(FONT_CHICAGO, 12);
        assert_eq!(face.font_id, FONT_CHICAGO);
        assert_eq!(face.size, 12);
    }

    #[test]
    fn fallback_courier_to_monaco() {
        let face = get_font_face_or_default(FONT_COURIER, 12);
        assert_eq!(face.font_id, FONT_MONACO);
    }

    #[test]
    fn space_has_advance_and_no_ink() {
        // ASCII 0x20 space: must carry a positive advance (otherwise
        // strings collapse) and must render no ink. FreeType's mono
        // path sometimes produces a minimal 1x1 empty bitmap rather
        // than a strictly zero-sized one, so assert by scanning the
        // data slice rather than the width/height fields.
        let face = get_font_face(FONT_GENEVA, 12).unwrap();
        let space = &face.glyphs[0];
        assert!(space.advance > 0, "space must advance");
        let len = (space.width as usize) * (space.height as usize);
        let data_slice = &face.data[space.data_offset..space.data_offset + len];
        assert!(
            data_slice.iter().all(|&b| b == 0),
            "space must render no ink"
        );
    }

    #[test]
    fn baked_data_is_binary() {
        // The baker emits FreeType mono output — every pixel must be
        // exactly 0 or 255. Enforces the invariant the `ShapeOp::Glyph`
        // partial-alpha branch depends on to stay dormant.
        for pf in PACKED_FACES {
            for &b in pf.data {
                assert!(b == 0 || b == 255, "baked pixel 0x{b:02X} not binary");
            }
        }
    }

    #[test]
    fn override_directory_entries_win_over_baked_faces() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-font-override-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let blob_path = dir.join("chicago_12_plain.bin");
        let mut buf = Vec::new();
        override_format::write_blob(&mut buf, &distinctive_override_blob()).unwrap();
        fs::write(&blob_path, &buf).unwrap();

        let overrides = override_format::load_directory(&dir);
        let face = get_font_face_with_overrides(&overrides, FONT_CHICAGO, 12)
            .expect("chicago 12 should resolve");
        assert_eq!(face.metrics.ascent, 99, "override should win over baked");
        assert_eq!(face.metrics.descent, 11);
        assert_eq!(face.glyphs.len(), override_format::GLYPH_COUNT as usize);
        assert!(
            face.glyphs.iter().all(|g| g.advance == 13),
            "all override glyphs carry the fingerprint advance"
        );

        let geneva = get_font_face_with_overrides(&overrides, FONT_GENEVA, 12)
            .expect("baked geneva 12 still there");
        assert_ne!(
            geneva.metrics.ascent, 99,
            "non-overridden face must keep baked DejaVu metrics"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
