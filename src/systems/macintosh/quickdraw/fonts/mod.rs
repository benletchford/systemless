//! Font routing, guest resources, and bundled TrueType outlines.
//! Guest FONT/NFNT/sfnt resources and explicit local overrides take precedence.

mod bundled;
mod classic_metrics;
mod compatibility;
pub mod families;
mod resources;
pub mod style;
use resources::*;
pub(crate) use resources::{
    parse_fond_associations, register_resource_font_strike,
    register_resource_font_strike_for_family, register_resource_outline_font, FondAssociation,
};
pub(crate) mod outline;
pub mod override_format;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use self::families::FONT_BUNDLED_ROMAN_BOLD;
pub use self::families::{
    FONT_APPLICATION, FONT_ATHENS, FONT_CAIRO, FONT_CHICAGO, FONT_COURIER, FONT_GENEVA,
    FONT_HELVETICA, FONT_LONDON, FONT_LOSANGELES, FONT_MOBILE, FONT_MONACO, FONT_NEWYORK,
    FONT_PALATINO, FONT_SANFRAN, FONT_SEATTLE, FONT_SYMBOL, FONT_TIMES, FONT_TORONTO, FONT_VENICE,
};

/// Rasterized glyph dimensions and offset into a shared coverage buffer.
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

/// One rasterized (font_id, size) face: metrics plus a slice of glyph
/// descriptors and a shared coverage-byte buffer that the descriptors'
/// `data_offset` fields index into.
pub struct FontFace {
    pub font_id: i16,
    pub size: i16,
    pub metrics: FontMetrics,
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
}

/// Glyph for a Mac Roman extended character (code 0x80..=0xFF).
pub struct MacRomanGlyph {
    pub mac_code: u8,
    pub glyph: Glyph,
}

/// Rasterized Mac Roman extension of a face.
pub struct MacRomanFace {
    pub font_id: i16,
    pub size: i16,
    pub glyphs: &'static [MacRomanGlyph],
    pub data: &'static [u8],
}

/// Coverage threshold used by QuickDraw binary transfer modes.
pub const MONO_COVERAGE_THRESHOLD: u8 = 128;

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
    (16, "Palatino"),
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
/// The directory is resolved on the first lookup and reused after that.
/// `get_font_face` sits on the per-glyph drawing path, where consulting
/// the environment means walking it and allocating: a CPU profile of EV
/// Override attributed several percent of the process to exactly that,
/// across the handful of lookups each character performs.
///
/// Embedders and test harnesses that set or clear
/// `SYSTEMLESS_ORIGINAL_FONTS_DIR` after other code has already queried
/// font metrics must call [`refresh_font_overrides`] to pick the change
/// up.
static OVERRIDES: LazyLock<Mutex<OverrideCache>> =
    LazyLock::new(|| Mutex::new(OverrideCache::default()));

/// The family to take glyphs and widths from for `style` text in `font_id`
/// at `size`, and the style bits QuickDraw must still synthesize on top.
///
/// Mac OS 9 draws bold New York and Times from 12 points up with the bold
/// face's own glyphs and widths, and synthesizes bold (one pixel wider, drawn
/// twice) below that: `CharWidth` measured on Mac OS 9.0 for the 94 printable
/// ASCII characters gives plain + 1 for every one at 9 and 10 points and a
/// different face's widths at 12, 13, 14, 15, 16, 18, 20 and 24. Sizes not
/// measured (11, below 9) stay synthesized. The bundled bold face replaces
/// only the bundled regular one: a guest font or an override for the family
/// at that size is drawn as before, with bold synthesized.
pub fn styled_face(font_id: i16, size: i16, style: u8) -> (i16, u8) {
    const BOLD: u8 = 0x01;
    const ITALIC: u8 = 0x02;
    let size = if size == 0 { 12 } else { size };
    if style & BOLD != 0
        && style & ITALIC == 0
        && matches!(font_id, FONT_NEWYORK | FONT_TIMES)
        && (12..=96).contains(&size)
        && bundled_face_in_use(font_id, size)
    {
        (FONT_BUNDLED_ROMAN_BOLD, style & !BOLD)
    } else {
        (font_id, style)
    }
}

/// Whether `font_id` at `size` is drawn with a bundled face: no
/// `SYSTEMLESS_ORIGINAL_FONTS_DIR` strike for it at that size, no guest
/// FONT/NFNT strike at that size and no guest outline font for the family.
fn bundled_face_in_use(font_id: i16, size: i16) -> bool {
    get_override_font_face(font_id, size).is_none()
        && !RESOURCE_FACES
            .lock()
            .expect("resource font cache poisoned")
            .contains_key(&(font_id, size))
        && !RESOURCE_OUTLINE_FONTS
            .lock()
            .expect("resource outline font cache poisoned")
            .contains_key(&font_id)
}

/// The line metrics `GetFontInfo`, `FontMetrics` and `StdTxMeas` report for
/// `font_id` at `size` in `style`, when they differ from the drawn face's: a
/// built-in family drawn with a bundled face reports Mac OS 9.0's measured
/// metrics (see `classic_metrics`). `None` means the face's own metrics apply,
/// as they do whenever a guest font or a `SYSTEMLESS_ORIGINAL_FONTS_DIR`
/// strike is drawn, including a guest strike of another size that
/// `get_font_face_or_default` falls back to.
pub fn classic_font_info(font_id: i16, size: i16, style: u8) -> Option<FontMetrics> {
    let metrics = classic_metrics::classic_line_metrics(font_id, size, style)?;
    let size = if size == 0 { 12 } else { size.max(1) };
    let drawn_from_guest_strike =
        || get_font_face(font_id, size).is_none() && closest_font_face(font_id, size).is_some();
    (bundled_face_in_use(font_id, size) && !drawn_from_guest_strike()).then_some(metrics)
}

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
    if let Some(face) = rasterize_resource_outline_face(font_id, size) {
        return Some(face);
    }
    outline::face(font_id, size).map(|(face, _)| face)
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
    outline::face(font_id, size).map(|(face, _)| face)
}

/// Re-read `SYSTEMLESS_ORIGINAL_FONTS_DIR` on the next font lookup.
///
/// Call this after setting or clearing the variable at runtime. Lookups
/// otherwise reuse the directory resolved by the first one, because
/// consulting the environment per glyph is measurably expensive.
pub fn refresh_font_overrides() {
    OVERRIDE_RESOLVED.store(false, Ordering::Release);
}

/// Whether the override directory has been resolved, and whether it
/// yielded any faces. Both are read per glyph, so they are plain atomics;
/// when no overrides exist -- the usual case, since the variable is an
/// opt-in hook -- lookups take neither the environment nor the mutex.
static OVERRIDE_RESOLVED: AtomicBool = AtomicBool::new(false);
static OVERRIDE_ANY: AtomicBool = AtomicBool::new(false);

fn get_override_font_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
    if OVERRIDE_RESOLVED.load(Ordering::Acquire) {
        if !OVERRIDE_ANY.load(Ordering::Acquire) {
            return None;
        }
        let cache = OVERRIDES.lock().expect("font override cache poisoned");
        return cache.faces.get(&(font_id, size)).copied();
    }
    let env_dir = std::env::var_os("SYSTEMLESS_ORIGINAL_FONTS_DIR");
    let mut cache = OVERRIDES.lock().expect("font override cache poisoned");
    if cache.env_dir != env_dir {
        cache.faces = env_dir
            .as_ref()
            .map(|dir| override_format::load_directory(Path::new(dir)))
            .unwrap_or_default();
        cache.env_dir = env_dir;
    }
    OVERRIDE_ANY.store(!cache.faces.is_empty(), Ordering::Release);
    OVERRIDE_RESOLVED.store(true, Ordering::Release);
    cache.faces.get(&(font_id, size)).copied()
}

fn closest_font_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
    RESOURCE_FACES
        .lock()
        .expect("resource font cache poisoned")
        .values()
        .copied()
        .filter(|face| face.font_id == font_id)
        .min_by_key(|face| {
            (
                (i32::from(face.size) - i32::from(size)).unsigned_abs(),
                std::cmp::Reverse(face.size),
            )
        })
}

pub fn get_font_face_or_default(font_id: i16, size: i16) -> &'static FontFace {
    let size = if size == 0 { 12 } else { size.max(1) };
    if let Some(face) = get_font_face(font_id, size) {
        return face;
    }
    if let Some(face) = closest_font_face(font_id, size) {
        return face;
    }
    // Rasterize bounded strikes; extreme sizes use the same outline face with
    // the scale ratio below rather than falling back to handwritten bitmaps.
    outline::face(font_id, size.min(96))
        .or_else(|| outline::face(FONT_APPLICATION, size.min(96)))
        .expect("bundled font must be valid")
        .0
}

pub fn get_font_face_scaled(font_id: i16, size: i16) -> (&'static FontFace, i16) {
    let face = get_font_face_or_default(font_id, size);
    (face, (size.max(1) / face.size.max(1)).max(1))
}

pub fn get_font_face_scale_ratio(font_id: i16, size: i16) -> (&'static FontFace, i32, i32) {
    let requested_size = if size == 0 { 12 } else { size }.max(1);
    let (face, _) = get_font_face_scaled(font_id, requested_size);
    (face, i32::from(requested_size), i32::from(face.size.max(1)))
}

pub fn get_macroman_glyph(
    font_id: i16,
    size: i16,
    mac_code: u8,
) -> Option<(&'static Glyph, &'static [u8])> {
    let resolved = get_font_face_or_default(font_id, size);
    let (font_id, size) = (resolved.font_id, resolved.size);
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
    if get_override_font_face(font_id, size).is_none()
        && !RESOURCE_FACES
            .lock()
            .expect("resource font cache poisoned")
            .contains_key(&(font_id, size))
    {
        if let Some((_, face)) = outline::face(font_id, size) {
            return face
                .glyphs
                .iter()
                .find(|entry| entry.mac_code == mac_code)
                .map(|entry| (&entry.glyph, face.data));
        }
    }
    None
}

/// No separate bitmap italic strikes are bundled. QuickDraw callers synthesize
/// the requested style from the selected font.
pub fn get_italic_glyph(
    _font_id: i16,
    _size: i16,
    _ch: char,
) -> Option<(&'static Glyph, &'static [u8])> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn bold_times_takes_its_bold_face_from_twelve_points_unless_a_guest_strike_stands_in() {
        use super::families::FONT_BUNDLED_ROMAN_BOLD as BOLD_FACE;
        assert_eq!(styled_face(FONT_TIMES, 12, 1), (BOLD_FACE, 0));
        assert_eq!(
            styled_face(FONT_TIMES, 0, 1),
            (BOLD_FACE, 0),
            "size 0 is 12"
        );
        assert_eq!(
            styled_face(FONT_NEWYORK, 20, 0x05),
            (BOLD_FACE, 0x04),
            "underline stays"
        );
        // Measured on Mac OS 9.0: bold is synthesized at 9 and 10 points;
        // 11 and smaller sizes are not measured and stay synthesized.
        for size in [8, 9, 10, 11] {
            assert_eq!(styled_face(FONT_TIMES, size, 1), (FONT_TIMES, 1), "{size}");
        }
        assert_eq!(
            styled_face(FONT_TIMES, 20, 0x03),
            (FONT_TIMES, 0x03),
            "bold italic"
        );
        assert_eq!(styled_face(FONT_TIMES, 20, 0), (FONT_TIMES, 0));
        assert_eq!(styled_face(FONT_HELVETICA, 20, 1), (FONT_HELVETICA, 1));
        // A guest strike for the family at that size is drawn as before.
        assert_eq!(styled_face(FONT_TIMES, 37, 1), (BOLD_FACE, 0));
        let mut strike = vec![0u8; 44];
        for (at, value) in [
            (2, 65u16),
            (4, 65),
            (6, 4),
            (12, 8),
            (14, 4),
            (16, 12),
            (18, 4),
            (24, 1),
            (36, 4),
            (38, 8),
            (40, 4),
            (42, 4),
        ] {
            strike[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        assert!(register_resource_font_strike_for_family(FONT_TIMES, 37, &strike));
        assert_eq!(styled_face(FONT_TIMES, 37, 1), (FONT_TIMES, 1));
    }

    #[test]
    fn built_in_families_report_mac_os_9_line_metrics_unless_a_guest_or_override_strike_stands_in()
    {
        use super::families::*;
        // GetFontInfo on Mac OS 9.0, bitmaps preferred: family, size, bold,
        // then ascent / descent / widMax / leading. Each family has rule
        // sizes and, where it has them, exception sizes.
        for (font, size, bold, [ascent, descent, wid_max, leading]) in [
            (FONT_CHICAGO, 9, 0, [9, 2, 12, 1]),
            (FONT_CHICAGO, 20, 1, [20, 5, 26, 2]),
            (FONT_CHICAGO, 47, 0, [47, 12, 61, 4]),
            (FONT_NEWYORK, 9, 0, [10, 2, 12, 0]),
            (FONT_NEWYORK, 10, 1, [10, 2, 14, 0]),
            (FONT_NEWYORK, 15, 1, [15, 4, 21, 1]),
            (FONT_GENEVA, 10, 1, [10, 2, 12, 1]),
            (FONT_GENEVA, 12, 0, [12, 3, 16, 1]),
            (FONT_GENEVA, 47, 0, [47, 12, 62, 4]),
            (FONT_MONACO, 9, 0, [9, 2, 6, 0]),
            (FONT_MONACO, 12, 0, [12, 3, 15, 1]),
            (FONT_MONACO, 20, 1, [20, 5, 26, 2]),
            (FONT_PALATINO, 10, 1, [9, 3, 13, 0]),
            (FONT_PALATINO, 15, 1, [12, 4, 15, 0]),
            (FONT_PALATINO, 47, 0, [39, 13, 54, 0]),
            (FONT_TIMES, 9, 0, [8, 2, 9, 1]),
            (FONT_TIMES, 10, 1, [9, 2, 11, 1]),
            (FONT_TIMES, 12, 0, [9, 3, 12, 0]),
            (FONT_TIMES, 15, 1, [11, 4, 15, 0]),
            (FONT_TIMES, 20, 1, [15, 5, 20, 0]),
            (FONT_TIMES, 47, 0, [35, 12, 47, 0]),
            (FONT_HELVETICA, 9, 0, [8, 2, 9, 1]),
            (FONT_HELVETICA, 15, 1, [12, 3, 15, 0]),
            (FONT_HELVETICA, 47, 0, [36, 11, 48, 0]),
            (FONT_COURIER, 10, 1, [9, 2, 10, 1]),
            (FONT_COURIER, 12, 0, [9, 3, 10, 0]),
            (FONT_COURIER, 20, 1, [15, 5, 17, 0]),
            (FONT_SYMBOL, 9, 0, [10, 3, 9, 0]),
            (FONT_SYMBOL, 12, 0, [8, 4, 13, 0]),
            (FONT_SYMBOL, 20, 1, [14, 6, 21, 0]),
        ] {
            let metrics = classic_font_info(font, size, bold).expect("built-in family");
            assert_eq!(
                [
                    metrics.ascent,
                    metrics.descent,
                    metrics.wid_max,
                    metrics.leading
                ],
                [ascent, descent, wid_max, leading],
                "family {font} size {size} bold {bold}"
            );
        }
        // Size 0 is 12; the application font is Geneva; other families have
        // no classic metrics.
        assert_eq!(classic_font_info(FONT_TIMES, 0, 0).unwrap().ascent, 9);
        assert_eq!(
            classic_font_info(FONT_APPLICATION, 12, 0).unwrap().wid_max,
            16
        );
        assert!(classic_font_info(FONT_VENICE, 12, 0).is_none());
        assert!(classic_font_info(1234, 12, 0).is_none());

        // A guest strike is drawn with its own metrics, both at its size and,
        // for Symbol, which has no bundled face, at the sizes it stands in for.
        assert!(classic_font_info(FONT_SYMBOL, 14, 0).is_some());
        let mut strike = vec![0u8; 44];
        for (at, value) in [
            (2, 65u16),
            (4, 65),
            (6, 4),
            (12, 8),
            (14, 4),
            (16, 12),
            (18, 4),
            (24, 1),
            (36, 4),
            (38, 8),
            (40, 4),
            (42, 4),
        ] {
            strike[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        assert!(classic_font_info(FONT_TIMES, 41, 0).is_some());
        assert!(register_resource_font_strike_for_family(
            FONT_TIMES, 41, &strike
        ));
        assert!(classic_font_info(FONT_TIMES, 41, 0).is_none());
        assert!(register_resource_font_strike_for_family(
            FONT_SYMBOL,
            12,
            &strike
        ));
        assert!(classic_font_info(FONT_SYMBOL, 12, 0).is_none());
        assert!(classic_font_info(FONT_SYMBOL, 14, 0).is_none());

        // So does a SYSTEMLESS_ORIGINAL_FONTS_DIR strike.
        assert!(classic_font_info(FONT_COURIER, 43, 0).is_some());
        let blob = distinctive_override_blob(FONT_COURIER, 43);
        let face: &'static FontFace = Box::leak(Box::new(FontFace {
            font_id: blob.font_id,
            size: blob.size,
            metrics: blob.metrics,
            glyphs: Box::leak(blob.glyphs.into_boxed_slice()),
            data: Box::leak(blob.data.into_boxed_slice()),
        }));
        struct RemoveOverride;
        impl Drop for RemoveOverride {
            fn drop(&mut self) {
                let mut cache = OVERRIDES.lock().unwrap();
                cache.faces.remove(&(FONT_COURIER, 43));
                OVERRIDE_ANY.store(!cache.faces.is_empty(), Ordering::Release);
            }
        }
        let _ = get_override_font_face(FONT_COURIER, 43);
        let _remove = RemoveOverride;
        OVERRIDES
            .lock()
            .unwrap()
            .faces
            .insert((FONT_COURIER, 43), face);
        OVERRIDE_ANY.store(true, Ordering::Release);
        assert!(classic_font_info(FONT_COURIER, 43, 0).is_none());
    }

    use super::*;
    use std::fs;

    pub(super) fn minimal_nfnt() -> Vec<u8> {
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

    #[test]
    fn resource_font_can_draw_del_character() {
        let mut font = minimal_nfnt();
        font[2..4].copy_from_slice(&127u16.to_be_bytes());
        font[4..6].copy_from_slice(&127u16.to_be_bytes());
        assert!(register_resource_font_strike_for_family(30002, 17, &font));
        let (glyph, data) = crate::quickdraw::text::get_glyph(30002, 17, '\u{7f}').unwrap();
        assert_eq!(glyph.width, 1);
        assert_eq!(data[glyph.data_offset], 255);
    }

    #[test]
    fn resource_font_without_space_advances_without_missing_symbol_ink() {
        // Civilization's nation fonts encode no space ('!' only here), so
        // char 32 maps to the missing symbol. QuickDraw's DrawText loop
        // handles 32 before the missing-glyph lookup and draws nothing.
        let mut font = minimal_nfnt();
        font[2..4].copy_from_slice(&33u16.to_be_bytes());
        font[4..6].copy_from_slice(&33u16.to_be_bytes());
        assert!(register_resource_font_strike_for_family(30003, 17, &font));
        let (space, _) = crate::quickdraw::text::get_glyph(30003, 17, ' ').unwrap();
        assert_eq!((space.width, space.advance), (0, 1));
        let (bang, data) = crate::quickdraw::text::get_glyph(30003, 17, '!').unwrap();
        assert_eq!(bang.width, 1);
        assert_eq!(data[bang.data_offset], 255);
    }

    fn distinctive_override_blob(font_id: i16, size: i16) -> override_format::Blob {
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
            font_id,
            size,
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
    fn bundled_families_rasterize_requested_sizes() {
        for id in [
            FONT_CHICAGO,
            FONT_GENEVA,
            FONT_MONACO,
            FONT_PALATINO,
            FONT_HELVETICA,
            FONT_COURIER,
        ] {
            for size in [9, 12, 17, 40, 96] {
                let (face, numerator, denominator) = get_font_face_scale_ratio(id, size);
                assert_eq!((face.font_id, face.size), (id, size));
                assert_eq!(numerator, denominator);
                assert_eq!(face.glyphs.len(), 95);
            }
        }
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
    fn override_directory_entries_win_over_bundled_faces() {
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
        override_format::write_blob(&mut buf, &distinctive_override_blob(FONT_CHICAGO, 12))
            .unwrap();
        fs::write(&blob_path, &buf).unwrap();

        let overrides = override_format::load_directory(&dir);
        let face = get_font_face_with_overrides(&overrides, FONT_CHICAGO, 12)
            .expect("chicago 12 should resolve");
        assert_eq!(face.metrics.ascent, 99, "override should win over bundled");
        assert_eq!(face.metrics.descent, 11);
        assert_eq!(face.glyphs.len(), override_format::GLYPH_COUNT as usize);
        assert!(
            face.glyphs.iter().all(|g| g.advance == 13),
            "all override glyphs carry the fingerprint advance"
        );

        let geneva = get_font_face_with_overrides(&overrides, FONT_GENEVA, 12)
            .expect("bundled geneva 12 still there");
        assert_ne!(
            geneva.metrics.ascent, 99,
            "non-overridden face must keep built-in systemless metrics"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn explicit_geneva9_override_keeps_its_own_metrics() {
        let mut overrides = HashMap::new();
        let blob = distinctive_override_blob(FONT_GENEVA, 9);
        let face = Box::leak(Box::new(FontFace {
            font_id: blob.font_id,
            size: blob.size,
            metrics: blob.metrics,
            glyphs: Box::leak(blob.glyphs.into_boxed_slice()),
            data: Box::leak(blob.data.into_boxed_slice()),
        }));
        overrides.insert((FONT_GENEVA, 9), &*face);

        let resolved = get_font_face_with_overrides(&overrides, FONT_GENEVA, 9)
            .expect("explicit Geneva 9 override");
        assert!(resolved.glyphs.iter().all(|glyph| glyph.advance == 13));
        assert_eq!(resolved.metrics.wid_max, 13);
        assert!(std::ptr::eq(resolved, face));
    }
}

/// Original bundled outline bytes for Resource-Manager-visible font handles.
pub(crate) fn bundled_font_bytes(font_id: i16) -> Option<&'static [u8]> {
    bundled::bytes(font_id)
}
