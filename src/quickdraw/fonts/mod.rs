//! Original systemless bitmap fonts + Mac font-family routing.
//!
//! Text renders by blitting pre-computed 8-bit coverage bitmaps out of
//! the [`pixel_font`] modules — no runtime rasterization, no hinting
//! decisions, no threshold knobs, no CLUT-closest-match. Every glyph
//! pixel is exactly 0 or 255, so the downstream `ShapeOp::Glyph`
//! partial-alpha branch never fires.
//!
//! Each face is authored as hand-editable ASCII art in a `pixel_font`
//! module and lowered to static glyph tables by `const fn` at compile
//! time — no offline baker, no external font asset. The systemless
//! faces are named after Australian native plants and stand in for the
//! classic Mac families, which survive only as internal compatibility
//! identifiers:
//!
//! | Mac font family (compat ID)                        | systemless face |
//! |----------------------------------------------------|-----------------|
//! | Chicago                                            | Jarrah          |
//! | Geneva / Application / Helvetica                   | Kurrajong       |
//! | Monaco / Courier                                   | Mallee        |
//! | New York / Times                                   | Ironbark        |
//! | Venice                                             | Wattle           |
//! | London                                             | Karri           |
//! | Cairo                                              | Grevillea          |
//!
//! All glyphs are original artwork authored for systemless; no
//! third-party font data is used. See the "Font Data" section of the
//! crate README for the full mapping and trademark / non-affiliation
//! notice.

pub mod heuristics;
pub mod override_format;
pub mod pixel_font;

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
/// shared `data` byte buffer. Coverage bytes are always exactly 0 or
/// 255 — the ASCII-art source is `const fn`-decoded to a binary mask,
/// so the `ShapeOp::Glyph` partial-alpha branch never fires.
#[derive(Clone, Copy)]
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
/// that expect the type; the built-in faces don't populate Mac Roman
/// extended tables yet so `get_macroman_glyph` returns None.
pub struct MacRomanGlyph {
    pub mac_code: u8,
    pub glyph: Glyph,
}

/// `FontFace` analogue covering Mac Roman extended characters
/// (codes 0x80..=0xFF). Currently has no entries — the built-in faces
/// don't populate these yet — but the type stays so the
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

const HELVETICA_12_FALLBACK_METRICS: FontMetrics = FontMetrics {
    ascent: 10,
    descent: 3,
    wid_max: 12,
    leading: 1,
};

// Every face is sourced from hand-editable ASCII art in `pixel_font`,
// decoded to these tables at compile time — no offline baker, no
// external font asset. `pf!` maps a Mac-compat (font_id, size) onto its
// `pixel_font` module.
macro_rules! pf {
    ($fid:expr, $size:expr, $module:ident) => {
        PackedFace {
            font_id: $fid,
            size: $size,
            metrics: pixel_font::$module::FACE.metrics,
            glyphs: pixel_font::$module::FACE.glyphs,
            data: pixel_font::$module::FACE.data,
        }
    };
}

const PACKED_FACES: &[PackedFace] = &[
    pf!(FONT_CHICAGO, 9, chicago9),
    pf!(FONT_CHICAGO, 12, chicago12),
    pf!(FONT_APPLICATION, 12, application12),
    pf!(FONT_NEWYORK, 12, newyork12),
    pf!(FONT_NEWYORK, 14, newyork14),
    pf!(FONT_NEWYORK, 18, newyork18),
    pf!(FONT_GENEVA, 9, geneva9),
    pf!(FONT_GENEVA, 10, geneva10),
    PackedFace {
        // Text 1993, pp. 1-61 and 3-65: font family 21 is Helvetica,
        // and QuickDraw measurements come from the current font/size.
        // Systemless cannot ship Apple's Helvetica NFNTs, so the
        // fallback uses the narrower Geneva 10 strike for Helvetica 12
        // instead of same-size Geneva. That keeps classic app-owned
        // Helvetica dialog text from overflowing fixed rects.
        font_id: FONT_HELVETICA,
        size: 12,
        metrics: HELVETICA_12_FALLBACK_METRICS,
        glyphs: pixel_font::geneva10::FACE.glyphs,
        data: pixel_font::geneva10::FACE.data,
    },
    pf!(FONT_GENEVA, 12, geneva12),
    pf!(FONT_GENEVA, 14, geneva14),
    pf!(FONT_GENEVA, 18, geneva18),
    pf!(FONT_GENEVA, 24, geneva24),
    pf!(FONT_MONACO, 9, monaco9),
    pf!(FONT_MONACO, 10, monaco10),
    pf!(FONT_MONACO, 12, monaco12),
    pf!(FONT_VENICE, 14, venice14),
    pf!(FONT_LONDON, 18, london18),
    pf!(FONT_CAIRO, 18, cairo18),
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

/// Bitmap strikes supplied by the currently loaded Classic Mac application.
/// The decoded storage is leaked deliberately: font resources are tiny and
/// glyph references are handed through the renderer as `'static` slices.
/// A Systemless process hosts one guest application, while replacement lets a
/// later resource fork with the same family/size take precedence.
static RESOURCE_FACES: LazyLock<Mutex<HashMap<(i16, i16), &'static FontFace>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RESOURCE_MACROMAN_FACES: LazyLock<Mutex<HashMap<(i16, i16), &'static MacRomanFace>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn resource_word(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

/// One entry from a font family (`FOND`) resource's association table.
///
/// `font_resource_id` identifies an `NFNT`, `FONT`, or `sfnt` resource;
/// unlike old-style `FONT` IDs, an `NFNT` ID does not encode its family or
/// point size. *Inside Macintosh: Text* (1993), pp. 4-47–4-48 and 4-95–4-96.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FondAssociation {
    pub family_id: i16,
    pub size: i16,
    pub style: u16,
    pub font_resource_id: i16,
}

/// Decode the mandatory association table that immediately follows a
/// `FOND` resource's 52-byte `FamRec` header.
pub(crate) fn parse_fond_associations(
    fond_resource_id: i16,
    bytes: &[u8],
) -> Option<Vec<FondAssociation>> {
    const FAM_REC_LEN: usize = 52;
    const ASSOCIATION_LEN: usize = 6;
    let count_minus_one = resource_word(bytes, FAM_REC_LEN)? as i16;
    if count_minus_one < -1 {
        return None;
    }
    let count = usize::try_from(i32::from(count_minus_one) + 1).ok()?;
    let table_len = count.checked_mul(ASSOCIATION_LEN)?;
    let table_end = FAM_REC_LEN.checked_add(2)?.checked_add(table_len)?;
    if table_end > bytes.len() {
        return None;
    }

    let mut associations = Vec::with_capacity(count);
    for index in 0..count {
        let offset = FAM_REC_LEN + 2 + index * ASSOCIATION_LEN;
        associations.push(FondAssociation {
            // The family number used by Font Manager clients is the FOND
            // resource ID. FamRec.ffFamID redundantly records that value.
            family_id: fond_resource_id,
            size: resource_word(bytes, offset)? as i16,
            style: resource_word(bytes, offset + 2)?,
            font_resource_id: resource_word(bytes, offset + 4)? as i16,
        });
    }
    Some(associations)
}

/// Decode a classic `FONT` bitmap strike whose resource ID uses the original
/// `family * 128 + pointSize` encoding. `NFNT` resources normally use the
/// arbitrary IDs recorded by a `FOND` association table and should call
/// [`register_resource_font_strike_for_family`] instead.
pub(crate) fn register_resource_font_strike(resource_id: i16, bytes: &[u8]) -> bool {
    if resource_id <= 0 {
        return false;
    }
    let family_id = resource_id / 128;
    let size = resource_id % 128;
    if size <= 0 {
        return false;
    }
    register_resource_font_strike_for_family(family_id, size, bytes)
}

/// Decode and register a bitmap strike under the family and point size from
/// its `FOND` association entry. The resource bytes stay in the user's
/// application; Systemless expands their 1-bit glyph bitmap into the same
/// runtime coverage representation used by its built-in faces.
pub(crate) fn register_resource_font_strike_for_family(
    family_id: i16,
    size: i16,
    bytes: &[u8],
) -> bool {
    const HEADER_LEN: usize = 26;
    if family_id < 0 || size <= 0 || bytes.len() < HEADER_LEN {
        return false;
    }

    let first_char = resource_word(bytes, 2)
        .map(usize::from)
        .unwrap_or(usize::MAX);
    let last_char = resource_word(bytes, 4).map(usize::from).unwrap_or(0);
    let wid_max = resource_word(bytes, 6).map(|v| v as i16).unwrap_or(0);
    let kern_max = resource_word(bytes, 8).map(|v| v as i16).unwrap_or(0);
    let f_rect_height = resource_word(bytes, 14).map(usize::from).unwrap_or(0);
    let ow_t_loc = resource_word(bytes, 16).map(usize::from).unwrap_or(0);
    let ascent = resource_word(bytes, 18).map(|v| v as i16).unwrap_or(0);
    let descent = resource_word(bytes, 20).map(|v| v as i16).unwrap_or(0);
    let leading = resource_word(bytes, 22).map(|v| v as i16).unwrap_or(0);
    let row_words = resource_word(bytes, 24).map(usize::from).unwrap_or(0);
    if first_char > last_char
        || last_char > 255
        || f_rect_height == 0
        || f_rect_height > u8::MAX as usize
        || row_words == 0
    {
        return false;
    }

    let bitmap_len = match row_words
        .checked_mul(2)
        .and_then(|row_bytes| row_bytes.checked_mul(f_rect_height))
    {
        Some(len) => len,
        None => return false,
    };
    let location_offset = HEADER_LEN + bitmap_len;
    // One glyph per encoded character plus the missing-character glyph; the
    // location table has one additional terminal entry.
    let glyph_count = last_char - first_char + 2;
    let location_count = glyph_count + 1;
    let location_end = match location_offset.checked_add(location_count * 2) {
        Some(end) => end,
        None => return false,
    };
    // FontRec.owTLoc is measured in words from the owTLoc field itself
    // (byte offset 16), not from the start of the resource.
    let ow_offset = match 16usize.checked_add(ow_t_loc * 2) {
        Some(offset) => offset,
        None => return false,
    };
    if HEADER_LEN + bitmap_len > bytes.len()
        || location_end > bytes.len()
        || ow_offset < location_end
        || ow_offset.saturating_add(glyph_count * 2) > bytes.len()
    {
        return false;
    }
    let bitmap_width = row_words * 16;
    let missing_index = last_char - first_char + 1;
    let mut coverage = Vec::new();

    let mut decode_index = |mut index: usize| -> Option<Glyph> {
        if index >= glyph_count {
            index = missing_index;
        }
        let mut ow = resource_word(bytes, ow_offset + index * 2)?;
        if ow == 0xFFFF && index != missing_index {
            index = missing_index;
            ow = resource_word(bytes, ow_offset + index * 2)?;
        }
        if ow == 0xFFFF {
            return None;
        }
        let start = resource_word(bytes, location_offset + index * 2)? as usize;
        let end = resource_word(bytes, location_offset + (index + 1) * 2)? as usize;
        if end < start || end > bitmap_width || end - start > u8::MAX as usize {
            return None;
        }
        let width = end - start;
        let data_offset = coverage.len();
        coverage.reserve(width.saturating_mul(f_rect_height));
        let row_bytes = row_words * 2;
        for row in 0..f_rect_height {
            let row_start = HEADER_LEN + row * row_bytes;
            for column in start..end {
                let byte = *bytes.get(row_start + column / 8)?;
                let mask = 0x80 >> (column & 7);
                coverage.push(if byte & mask != 0 { 255 } else { 0 });
            }
        }
        // The offset byte is unsigned; adding the (normally negative)
        // kernMax converts it to the signed displacement from the pen.
        let origin_x = kern_max.saturating_add(i16::from((ow >> 8) as u8));
        Some(Glyph {
            width: width as u8,
            height: f_rect_height as u8,
            advance: (ow & 0x00FF) as u8,
            origin_x: origin_x.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            origin_y: (-ascent).clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            data_offset,
        })
    };

    let mut ascii = Vec::with_capacity(95);
    for code in 0x20usize..=0x7E {
        let index = code
            .checked_sub(first_char)
            .filter(|_| code <= last_char)
            .unwrap_or(missing_index);
        ascii.push(decode_index(index).unwrap_or(Glyph {
            width: 0,
            height: 0,
            advance: wid_max.clamp(0, u8::MAX as i16) as u8,
            origin_x: 0,
            origin_y: 0,
            data_offset: 0,
        }));
    }
    let mut macroman = Vec::new();
    for code in 0x80usize..=0xFF {
        if code < first_char || code > last_char {
            continue;
        }
        if let Some(glyph) = decode_index(code - first_char) {
            macroman.push(MacRomanGlyph {
                mac_code: code as u8,
                glyph,
            });
        }
    }

    let coverage: &'static [u8] = Box::leak(coverage.into_boxed_slice());
    let ascii: &'static [Glyph] = Box::leak(ascii.into_boxed_slice());
    let face = Box::leak(Box::new(FontFace {
        font_id: family_id,
        size,
        metrics: FontMetrics {
            ascent,
            descent,
            wid_max,
            leading,
        },
        glyphs: ascii,
        data: coverage,
    }));
    RESOURCE_FACES
        .lock()
        .expect("resource font cache poisoned")
        .insert((family_id, size), face);
    if !macroman.is_empty() {
        let glyphs: &'static [MacRomanGlyph] = Box::leak(macroman.into_boxed_slice());
        let face = Box::leak(Box::new(MacRomanFace {
            font_id: family_id,
            size,
            glyphs,
            data: coverage,
        }));
        RESOURCE_MACROMAN_FACES
            .lock()
            .expect("resource Mac Roman font cache poisoned")
            .insert((family_id, size), face);
    }
    true
}

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
/// Entries here win over the built-in systemless catalogue — the opt-in hook for
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
    if let Some(face) = RESOURCE_FACES
        .lock()
        .expect("resource font cache poisoned")
        .get(&(font_id, size))
        .copied()
    {
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
    get_font_face_scaled_impl(font_id, size)
}

fn get_font_face_scaled_impl(font_id: i16, size: i16) -> (&'static FontFace, i16) {
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
    if let Some(face) = RESOURCE_MACROMAN_FACES
        .lock()
        .expect("resource Mac Roman font cache poisoned")
        .get(&(font_id, size))
        .copied()
    {
        if let Some(hit) = face.glyphs.iter().find(|entry| entry.mac_code == mac_code) {
            return Some((&hit.glyph, face.data));
        }
    }
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

    fn minimal_nfnt() -> Vec<u8> {
        // One encoded character plus the missing-character glyph. The bitmap
        // is one row by one word; location and offset/width tables follow it.
        let mut bytes = vec![0u8; 38];
        bytes[2..4].copy_from_slice(&32u16.to_be_bytes()); // firstChar
        bytes[4..6].copy_from_slice(&32u16.to_be_bytes()); // lastChar
        bytes[6..8].copy_from_slice(&1u16.to_be_bytes()); // widMax
        bytes[14..16].copy_from_slice(&1u16.to_be_bytes()); // fRectHeight
        bytes[16..18].copy_from_slice(&9u16.to_be_bytes()); // owTLoc: 16 + 9*2 = 34
        bytes[18..20].copy_from_slice(&1u16.to_be_bytes()); // ascent
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes()); // rowWords
        bytes[26] = 0xC0; // one pixel for each of the two glyphs
        bytes[28..30].copy_from_slice(&0u16.to_be_bytes());
        bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
        bytes[32..34].copy_from_slice(&2u16.to_be_bytes());
        bytes[34..36].copy_from_slice(&1u16.to_be_bytes()); // offset 0, advance 1
        bytes[36..38].copy_from_slice(&1u16.to_be_bytes());
        bytes
    }

    #[test]
    fn fond_associations_map_arbitrary_bitmap_resource_ids() {
        let mut fond = vec![0u8; 66];
        fond[2..4].copy_from_slice(&1234u16.to_be_bytes()); // redundant ffFamID
        fond[52..54].copy_from_slice(&1u16.to_be_bytes()); // two entries minus one
        fond[54..56].copy_from_slice(&12u16.to_be_bytes());
        fond[56..58].copy_from_slice(&0u16.to_be_bytes());
        fond[58..60].copy_from_slice(&42u16.to_be_bytes());
        fond[60..62].copy_from_slice(&18u16.to_be_bytes());
        fond[62..64].copy_from_slice(&1u16.to_be_bytes());
        fond[64..66].copy_from_slice(&(-32000i16 as u16).to_be_bytes());

        assert_eq!(
            parse_fond_associations(16000, &fond),
            Some(vec![
                FondAssociation {
                    family_id: 16000,
                    size: 12,
                    style: 0,
                    font_resource_id: 42,
                },
                FondAssociation {
                    family_id: 16000,
                    size: 18,
                    style: 1,
                    font_resource_id: -32000,
                },
            ])
        );
    }

    #[test]
    fn resource_strike_can_register_under_fond_family_and_size() {
        let family_id = 30001;
        assert!(register_resource_font_strike_for_family(
            family_id,
            17,
            &minimal_nfnt(),
        ));
        let face = get_font_face(family_id, 17).expect("FOND-associated face should resolve");
        assert_eq!(face.font_id, family_id);
        assert_eq!(face.size, 17);
        assert_eq!(face.metrics.ascent, 1);
    }

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
    fn baked_helvetica_12_fallback_is_narrower_than_geneva_12() {
        let overrides = HashMap::new();
        let helvetica = get_font_face_with_overrides(&overrides, FONT_HELVETICA, 12)
            .expect("baked Helvetica 12 fallback should resolve directly");
        let geneva = get_font_face_with_overrides(&overrides, FONT_GENEVA, 12)
            .expect("baked Geneva 12 should resolve");
        assert_eq!(helvetica.font_id, FONT_HELVETICA);
        assert_eq!(helvetica.size, 12);
        assert_eq!(
            helvetica.metrics.ascent + helvetica.metrics.descent + helvetica.metrics.leading,
            14
        );

        fn width(face: &FontFace, text: &str) -> u16 {
            text.bytes()
                .map(|byte| face.glyphs[(byte - b' ') as usize].advance as u16)
                .sum()
        }

        let notice_line = "Please note that EV Override is not a free product.";
        assert!(width(helvetica, notice_line) < width(geneva, notice_line));
        assert!(
            width(helvetica, "Register...") <= 56,
            "fallback Helvetica 12 should fit classic 90px dialog buttons"
        );
    }

    #[test]
    fn space_has_advance_and_no_ink() {
        // ASCII 0x20 space: must carry a positive advance (otherwise
        // strings collapse) and must render no ink. Some faces encode
        // space as a minimal empty bitmap rather than a strictly
        // zero-sized one, so assert by scanning the data slice rather
        // than the width/height fields.
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
    fn alphanumerics_rest_on_the_baseline() {
        // Regression guard for the hand-authored faces: a glyph's bottom edge
        // is `origin_y + height` (0 = the baseline; positive descends below
        // it). Letters and digits must never *float above* the baseline — that
        // is the "bouncing text" bug you get when redrawn art is shorter than
        // the original but keeps the original (cap-height) origin_y. Digits and
        // capitals rest exactly on the baseline; lowercase may descend, but no
        // further than the face's descent.
        for pf in PACKED_FACES {
            for byte in (b'0'..=b'9').chain(b'A'..=b'Z').chain(b'a'..=b'z') {
                let g = &pf.glyphs[(byte - b' ') as usize];
                if g.height == 0 {
                    continue;
                }
                let bottom = g.origin_y as i32 + g.height as i32;
                assert!(
                    bottom >= 0,
                    "({}, {}) glyph {:?} floats {}px above the baseline",
                    pf.font_id,
                    pf.size,
                    byte as char,
                    -bottom
                );
                assert!(
                    bottom <= pf.metrics.descent as i32,
                    "({}, {}) glyph {:?} sinks {}px below the {}px descent",
                    pf.font_id,
                    pf.size,
                    byte as char,
                    bottom,
                    pf.metrics.descent
                );
                // Digits, capitals and J/Q tails are only held to the
                // >=0 / <=descent bounds above: the authentic originals give
                // some digits a 1px rounded overshoot below the baseline (e.g.
                // New York's '3'), so an exact rest-on-baseline rule would
                // reject faithful metrics.
            }
        }
    }

    // --- generic font-family invariants ----------------------------------
    // These run over every PACKED_FACE and derive their expectations from the
    // face's own glyphs (no per-face magic numbers), so any hand-authored face
    // is held to the same alignment/height contract.

    /// Lowercase letters that occupy exactly the x-height band (no ascender,
    /// no descender).
    const PLAIN_X_HEIGHT: &[u8] = b"acemnorsuvwxz";
    /// Lowercase letters whose stems rise to the ascender line (`f` and `t`
    /// are intentionally excluded — their reach differs by design).
    const ASCENDER_LETTERS: &[u8] = b"bdhkl";
    /// Lowercase letters with a descender below the baseline.
    const DESCENDER_LETTERS: &[u8] = b"gpqy";

    fn packed_glyph(pf: &PackedFace, byte: u8) -> &'static Glyph {
        &pf.glyphs[(byte - b' ') as usize]
    }

    /// Glyphs in `letters` must share a top line within `tol` px. A small
    /// tolerance is required because the authentic strikes give round-topped
    /// letters (b/d/h, the digit 6) a 1–2px optical overshoot above the flat
    /// tops; the guard still catches a glyph drawn a whole band off (the
    /// "bouncing text" / cap-height mistake).
    fn assert_shared_top(group: &str, letters: &[u8], tol: i8) {
        for pf in PACKED_FACES {
            let tops: Vec<(u8, i8)> = letters
                .iter()
                .map(|&b| (b, packed_glyph(pf, b)))
                .filter(|(_, g)| g.height != 0)
                .map(|(b, g)| (b, g.origin_y))
                .collect();
            if let (Some(lo), Some(hi)) = (
                tops.iter().map(|(_, t)| *t).min(),
                tops.iter().map(|(_, t)| *t).max(),
            ) {
                assert!(
                    hi - lo <= tol,
                    "({}, {}) {group} letters span {}px of top-line variation (>{}px): {:?}",
                    pf.font_id,
                    pf.size,
                    hi - lo,
                    tol,
                    tops
                );
            }
        }
    }

    /// Glyphs in `letters` must share a bottom line within `tol` px. The
    /// tolerance covers authentic descender depth differences (Geneva's g/y
    /// reach 1–2px below p/q) while still catching a floating glyph.
    fn assert_shared_bottom(group: &str, letters: &[u8], tol: i32) {
        for pf in PACKED_FACES {
            let bottoms: Vec<(u8, i32)> = letters
                .iter()
                .map(|&b| (b, packed_glyph(pf, b)))
                .filter(|(_, g)| g.height != 0)
                .map(|(b, g)| (b, g.origin_y as i32 + g.height as i32))
                .collect();
            if let (Some(lo), Some(hi)) = (
                bottoms.iter().map(|(_, b)| *b).min(),
                bottoms.iter().map(|(_, b)| *b).max(),
            ) {
                assert!(
                    hi - lo <= tol,
                    "({}, {}) {group} letters span {}px of bottom-line variation (>{}px): {:?}",
                    pf.font_id,
                    pf.size,
                    hi - lo,
                    tol,
                    bottoms
                );
            }
        }
    }

    #[test]
    fn ascender_letters_share_one_top_line() {
        assert_shared_top("ascender", ASCENDER_LETTERS, 1);
    }

    #[test]
    fn descender_bowls_sit_on_the_x_height_line() {
        // The bowl of g/p/q/y occupies the x-height band; its top (origin_y)
        // must match the plain x-height letters. If a descender is drawn from
        // the cap line instead, its bowl towers over its neighbours and reads
        // like a capital (e.g. a monospace 'p' that looks like 'P').
        for pf in PACKED_FACES {
            let x_top = packed_glyph(pf, b'o').origin_y;
            for &byte in DESCENDER_LETTERS {
                let g = packed_glyph(pf, byte);
                if g.height == 0 {
                    continue;
                }
                // The bowl top must sit on the x-height line, give or take a
                // 2px optical overshoot above it (New York's 'g' bowl rides
                // 1px high). It must never drop below the x-height line, nor
                // rise toward the cap line — that is the "'p' looks like 'P'"
                // bug this guard exists to catch.
                let above = x_top as i32 - g.origin_y as i32;
                assert!(
                    (0..=2).contains(&above),
                    "({}, {}) descender {:?} bowl starts at row {} but x-height is at {}",
                    pf.font_id,
                    pf.size,
                    byte as char,
                    g.origin_y,
                    x_top
                );
            }
        }
    }

    #[test]
    fn descender_letters_share_one_bottom_line() {
        assert_shared_bottom("descender", DESCENDER_LETTERS, 2);
    }

    /// Original per-glyph box metrics (advance, origin_x, origin_y, width,
    /// height) for the classic families, keyed by `@ font_id size`. Test-only
    /// compatibility data — see the file header.
    const ORIGINAL_CELLS: &str = include_str!("original_cells.txt");

    type Cell = (i32, i32, i32, i32, i32);

    fn parse_original_cells() -> std::collections::HashMap<(i16, i16), Vec<Cell>> {
        let mut map = std::collections::HashMap::new();
        let mut cur: Option<(i16, i16)> = None;
        for line in ORIGINAL_CELLS.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("@ ") {
                let mut it = rest.split_whitespace();
                let f = it.next().unwrap().parse().unwrap();
                let s = it.next().unwrap().parse().unwrap();
                cur = Some((f, s));
                map.insert((f, s), Vec::new());
            } else {
                let v: Vec<i32> = line
                    .split_whitespace()
                    .map(|x| x.parse().unwrap())
                    .collect();
                map.get_mut(&cur.unwrap())
                    .unwrap()
                    .push((v[0], v[1], v[2], v[3], v[4]));
            }
        }
        map
    }

    #[test]
    fn advances_match_original_cells() {
        // Every face that has a true original strike must reproduce that
        // strike's per-glyph ADVANCE and left bearing (origin_x). Those two
        // numbers are what govern text layout, so matching them makes classic
        // app text lay out at the exact original width and never overflow the
        // fixed rects it was designed for (e.g. Escape Velocity's status
        // panel). Our bitmaps deliberately keep their own clean, hand-drawn
        // width/height/origin_y rather than resizing to the originals' pixel
        // boxes (which warps the letterforms), so those fields are reported
        // informationally only.
        let refs = parse_original_cells();
        for pf in PACKED_FACES {
            let Some(exp) = refs.get(&(pf.font_id, pf.size)) else {
                continue;
            };
            let mut layout_off = Vec::new();
            let mut box_off = 0;
            for (i, g) in pf.glyphs.iter().enumerate() {
                let (adv, ox, oy, w, h) = exp[i];
                if g.advance as i32 != adv || (i != 0 && g.origin_x as i32 != ox) {
                    layout_off.push((b' ' + i as u8) as char);
                }
                if i != 0
                    && (g.origin_y as i32 != oy || g.width as i32 != w || g.height as i32 != h)
                {
                    box_off += 1;
                }
            }
            eprintln!(
                "({:2},{:2}): advances {:2}/95 exact, {:2}/95 own bitmap box",
                pf.font_id,
                pf.size,
                95 - layout_off.len(),
                box_off
            );
            assert!(
                layout_off.is_empty(),
                "({}, {}) advance/bearing off original — text will misalign: {}",
                pf.font_id,
                pf.size,
                layout_off.iter().collect::<String>()
            );
        }
    }

    #[test]
    fn descender_letters_actually_descend() {
        // A descender must drop below the baseline (bottom > 0) but no further
        // than the face's declared descent.
        for pf in PACKED_FACES {
            for &byte in DESCENDER_LETTERS {
                let g = packed_glyph(pf, byte);
                if g.height == 0 {
                    continue;
                }
                let bottom = g.origin_y as i32 + g.height as i32;
                assert!(
                    bottom > 0 && bottom <= pf.metrics.descent as i32,
                    "({}, {}) descender {:?} bottom {} outside 1..={}",
                    pf.font_id,
                    pf.size,
                    byte as char,
                    bottom,
                    pf.metrics.descent
                );
            }
        }
    }

    #[test]
    fn digits_are_uniform_height() {
        // All ten digits are drawn to one common top and bottom (they never
        // ascend or descend), so a run of numbers never bounces.
        // New York gives '6' a 1px taller hook and rounds '3'/'5'/'8' with a
        // 1–2px overshoot below the baseline, so allow that optical slack.
        assert_shared_top("digit", b"0123456789", 1);
        assert_shared_bottom("digit", b"0123456789", 2);
    }

    #[test]
    fn x_height_letters_share_one_top_line() {
        // Regression guard: the plain x-height lowercase letters (no ascender,
        // no descender) must all start at the same row (`origin_y`). If one is
        // drawn a pixel taller than its siblings it pokes above the x-height
        // line and the word visibly "bounces" — e.g. an `a` sitting higher than
        // the surrounding `n o c e ...`.
        assert_shared_top("x-height", PLAIN_X_HEIGHT, 0);
    }

    #[test]
    fn baked_data_is_binary() {
        // The `const fn` decoder emits a binary mask — every pixel must
        // be exactly 0 or 255. Enforces the invariant the `ShapeOp::Glyph`
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
            "non-overridden face must keep built-in systemless metrics"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
