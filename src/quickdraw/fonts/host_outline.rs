//! Host-provided outline-font fallback.
//!
//! A Classic application names a font family and point size; it does not
//! embed the resulting screen bitmap.  When the guest has no matching font
//! resource, a translation layer may use a host-installed equivalent before
//! falling back to Systemless's OFL bitmap catalogue.  The provider lowers an
//! outline once into the same [`FontFace`] representation as an `NFNT`, so
//! QuickDraw's drawing path remains platform-independent and does not call a
//! host font API per glyph.
//!
//! The current rasterizer intentionally thresholds coverage to monochrome.
//! Executing TrueType hinting instructions and offering the configurable
//! smoothing introduced in Mac OS 8.5 are separate future compatibility
//! features; neither is approximated implicitly here.

use super::FontFace;

#[cfg(any(target_os = "linux", target_arch = "wasm32", test))]
const TIMES_FAMILY_PREFERENCE: &[&str] = &[
    "Times",
    "Times New Roman",
    // Common metric-compatible installations on Linux.  Do not silently
    // choose an arbitrary desktop serif: that could change guest layout.
    "Liberation Serif",
    "Nimbus Roman",
];

/// Apply one family preference policy independently of the platform's font
/// discovery mechanism. Keeping this decision outside `fontdb` and browser
/// Canvas makes its ordering deterministic and directly unit-testable.
#[cfg(any(target_os = "linux", test))]
fn resolve_preferred_times<T>(mut resolve: impl FnMut(&str) -> Option<T>) -> Option<T> {
    TIMES_FAMILY_PREFERENCE
        .iter()
        .find_map(|family| resolve(family))
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::FontFace;
    use crate::quickdraw::fonts::{FontMetrics, Glyph, FONT_TIMES};
    use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};

    #[cfg(any(target_os = "linux", test))]
    fn parse_fontconfig_match(family: &str, output: &[u8]) -> Option<(PathBuf, u32)> {
        let result = std::str::from_utf8(output).ok()?;
        let mut lines = result.lines();
        let matched_families = lines.next()?;
        // Fontconfig always supplies *some* fallback. Accept it only when it
        // really is the family currently being tried, preserving our explicit
        // preference order rather than silently taking an arbitrary serif.
        if !matched_families
            .split(',')
            .any(|matched| matched.trim().eq_ignore_ascii_case(family))
        {
            return None;
        }
        let path = PathBuf::from(lines.next()?);
        let collection_index = lines.next()?.parse().ok()?;
        Some((path, collection_index))
    }

    #[cfg(target_os = "macos")]
    fn locate_times() -> Option<(PathBuf, u32)> {
        // Times is installed in macOS's read-only system-font directory.  A
        // targeted lookup avoids scanning the very large downloadable-font
        // catalogue merely to locate this standard family.
        [
            "/System/Library/Fonts/Times.ttc",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "/Library/Fonts/Microsoft/Times New Roman.ttf",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .map(|path| (path, 0))
    }

    #[cfg(target_os = "linux")]
    fn locate_times() -> Option<(PathBuf, u32)> {
        fn fontconfig_match(family: &str) -> Option<(PathBuf, u32)> {
            // `fc-match` consults Fontconfig's existing cache. Unlike
            // `fontdb::load_system_fonts`, it does not make every Systemless
            // process recursively enumerate and parse all installed fonts.
            let output = std::process::Command::new("fc-match")
                .args([
                    "--format=%{family}\n%{file}\n%{index}\n",
                    &format!("{family}:style=Regular"),
                ])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let (path, collection_index) = parse_fontconfig_match(family, &output.stdout)?;
            path.is_file().then_some((path, collection_index))
        }

        super::resolve_preferred_times(fontconfig_match).or_else(|| {
            // Minimal systems sometimes ship the face but no `fc-match`.
            [
                "/usr/share/fonts/truetype/msttcorefonts/Times_New_Roman.ttf",
                "/usr/share/fonts/truetype/msttcorefonts/times.ttf",
                "/usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
                "/usr/share/fonts/opentype/urw-base35/NimbusRoman-Regular.otf",
            ]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .map(|path| (path, 0))
        })
    }

    #[cfg(target_os = "windows")]
    fn locate_times() -> Option<(PathBuf, u32)> {
        let mut directories = Vec::with_capacity(2);
        if let Some(windows) = std::env::var_os("WINDIR") {
            directories.push(PathBuf::from(windows).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            directories.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        for name in [
            // Standard Windows Times New Roman file name.
            "times.ttf",
            "Times New Roman.ttf",
            "LiberationSerif-Regular.ttf",
            "NimbusRoman-Regular.otf",
        ] {
            if let Some(path) = directories
                .iter()
                .map(|directory| directory.join(name))
                .find(|path| path.is_file())
            {
                return Some((path, 0));
            }
        }
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    fn locate_times() -> Option<(PathBuf, u32)> {
        None
    }

    /// Locate the regular host face once. On platforms whose discovery API
    /// builds a system-wide database, retain only the chosen path and
    /// collection index rather than every installed font's metadata.
    static TIMES_SOURCE: LazyLock<Option<(PathBuf, u32)>> = LazyLock::new(locate_times);

    type CachedGlyph = Option<(&'static Glyph, &'static [u8])>;

    static FACES: LazyLock<Mutex<HashMap<(i16, i16), Option<&'static FontFace>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    static MAC_ROMAN_GLYPHS: LazyLock<Mutex<HashMap<(i16, i16, u8), CachedGlyph>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    fn round_metric(value: f32) -> i16 {
        value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
    }

    fn classic_point_to_px_scale(font: &impl Font, point_size: f32) -> Option<PxScale> {
        // `Font::pt_to_px_scale` deliberately assumes the modern 96-dpi CSS
        // reference pixel. Classic QuickDraw's screen fonts use the original
        // Macintosh 72-dpi mapping, where N points correspond to N pixels per
        // em. ab_glyph's PxScale is the full font height rather than the em,
        // so preserve its height/units-per-em conversion while omitting the
        // modern 96/72 enlargement.
        let units_per_em = font.units_per_em()?;
        Some(PxScale::from(
            point_size * font.height_unscaled() / units_per_em,
        ))
    }

    fn source_for_font(font_id: i16) -> Option<&'static (PathBuf, u32)> {
        match font_id {
            FONT_TIMES => TIMES_SOURCE.as_ref(),
            _ => None,
        }
    }

    fn load_font(font_id: i16) -> Option<FontVec> {
        let (path, collection_index) = source_for_font(font_id)?;
        FontVec::try_from_vec_and_index(std::fs::read(path).ok()?, *collection_index).ok()
    }

    fn rasterize_character(
        font: &FontVec,
        scale: PxScale,
        character: char,
        data: &mut Vec<u8>,
    ) -> Option<Glyph> {
        let scaled = font.as_scaled(scale);
        let id = scaled.glyph_id(character);
        if id.0 == 0 {
            return None;
        }
        let advance = round_metric(scaled.h_advance(id)).max(0);
        let data_offset = data.len();
        let Some(outlined) =
            scaled.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0)))
        else {
            return Some(Glyph {
                width: 0,
                height: 0,
                advance: u8::try_from(advance).ok()?,
                origin_x: 0,
                origin_y: 0,
                data_offset,
            });
        };

        let bounds = outlined.px_bounds();
        let width = bounds.width() as usize;
        let height = bounds.height() as usize;
        let mut coverage = vec![0u8; width.saturating_mul(height)];
        outlined.draw(|x, y, amount| {
            let index = y as usize * width + x as usize;
            if let Some(pixel) = coverage.get_mut(index) {
                // Pre-8.5 QuickDraw screen strikes are monochrome. Preserve
                // the outline shape without silently opting the guest into a
                // later system's configurable font-smoothing policy.
                *pixel = if amount >= 0.5 { 255 } else { 0 };
            }
        });
        data.extend_from_slice(&coverage);
        Some(Glyph {
            width: u8::try_from(width).ok()?,
            height: u8::try_from(height).ok()?,
            advance: u8::try_from(advance).ok()?,
            origin_x: i8::try_from(bounds.min.x as i16).ok()?,
            origin_y: i8::try_from(bounds.min.y as i16).ok()?,
            data_offset,
        })
    }

    fn rasterize_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
        let font = load_font(font_id)?;
        let size = size.max(1);
        let scale = classic_point_to_px_scale(&font, size as f32)?;
        let scaled = font.as_scaled(scale);
        let metrics = FontMetrics {
            ascent: scaled.ascent().ceil() as i16,
            descent: (-scaled.descent()).ceil() as i16,
            wid_max: 0,
            leading: scaled.line_gap().round().max(0.0) as i16,
        };

        let mut glyphs = Vec::with_capacity(95);
        let mut data = Vec::new();
        let mut widest_advance = 0i16;
        for code in b' '..=b'~' {
            let glyph = rasterize_character(&font, scale, code as char, &mut data)?;
            widest_advance = widest_advance.max(i16::from(glyph.advance));
            glyphs.push(glyph);
        }
        let data = Box::leak(data.into_boxed_slice());
        Some(Box::leak(Box::new(FontFace {
            font_id,
            size,
            metrics: FontMetrics {
                wid_max: widest_advance,
                ..metrics
            },
            glyphs: Box::leak(glyphs.into_boxed_slice()),
            data,
        })))
    }

    fn rasterize_mac_roman_glyph(
        font_id: i16,
        size: i16,
        mac_code: u8,
    ) -> Option<(&'static Glyph, &'static [u8])> {
        let font = load_font(font_id)?;
        let size = size.max(1);
        let scale = classic_point_to_px_scale(&font, size as f32)?;
        let character = crate::mac_roman::decode_byte(mac_code);
        let mut data = Vec::new();
        let mut glyph = rasterize_character(&font, scale, character, &mut data)?;
        glyph.data_offset = 0;
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        Some((Box::leak(Box::new(glyph)), data))
    }

    pub(crate) fn get(font_id: i16, size: i16) -> Option<&'static FontFace> {
        source_for_font(font_id)?;
        let size = if size == 0 { 12 } else { size };
        let key = (font_id, size);
        let mut faces = FACES.lock().expect("host outline font cache poisoned");
        if let Some(face) = faces.get(&key) {
            return *face;
        }
        let face = rasterize_face(font_id, size);
        faces.insert(key, face);
        face
    }

    pub(crate) fn get_macroman(
        font_id: i16,
        size: i16,
        mac_code: u8,
    ) -> Option<(&'static Glyph, &'static [u8])> {
        source_for_font(font_id)?;
        let size = if size == 0 { 12 } else { size };
        let key = (font_id, size, mac_code);
        let mut glyphs = MAC_ROMAN_GLYPHS
            .lock()
            .expect("host outline Mac Roman glyph cache poisoned");
        if let Some(glyph) = glyphs.get(&key) {
            return *glyph;
        }
        let glyph = rasterize_mac_roman_glyph(font_id, size, mac_code);
        glyphs.insert(key, glyph);
        glyph
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn installed_times_rasterizes_as_a_cached_24_point_face() {
            let Some(first) = get(FONT_TIMES, 24) else {
                // The portable fallback is the supported behavior on a macOS
                // installation that genuinely lacks the system family.
                return;
            };
            let second = get(FONT_TIMES, 24).expect("cached face disappeared");
            assert!(std::ptr::eq(first, second));
            assert_eq!(first.font_id, FONT_TIMES);
            assert_eq!(first.size, 24);
            assert_eq!(first.glyphs.len(), 95);
            assert!(first.metrics.ascent > 0);
            assert!(first.data.iter().all(|pixel| matches!(pixel, 0 | 255)));
        }

        #[test]
        fn installed_times_rasterizes_the_mac_roman_ellipsis() {
            let Some((first, first_data)) = get_macroman(FONT_TIMES, 24, 0xC9) else {
                return;
            };
            let (second, second_data) =
                get_macroman(FONT_TIMES, 24, 0xC9).expect("cached ellipsis disappeared");
            assert!(std::ptr::eq(first, second));
            assert!(std::ptr::eq(first_data, second_data));
            assert!(first.width > 0);
            assert!(first.advance > 0);
            assert!(first_data.iter().any(|&pixel| pixel == 255));
        }

        #[test]
        fn classic_point_scale_does_not_apply_the_modern_96_dpi_enlargement() {
            let Some((path, collection_index)) = TIMES_SOURCE.as_ref() else {
                return;
            };
            let font = FontVec::try_from_vec_and_index(
                std::fs::read(path).expect("installed Times unreadable"),
                *collection_index,
            )
            .expect("installed Times unparsable");
            let classic = classic_point_to_px_scale(&font, 24.0).expect("invalid font metrics");
            let modern = font.pt_to_px_scale(24.0).expect("invalid font metrics");
            assert!((classic.x * 4.0 - modern.x * 3.0).abs() < 0.01);
            assert!((classic.y * 4.0 - modern.y * 3.0).abs() < 0.01);
        }

        #[test]
        fn fontconfig_result_must_name_the_requested_family() {
            assert_eq!(
                parse_fontconfig_match(
                    "Times New Roman",
                    b"Times New Roman,Times New Roman\n/fonts/times.ttf\n2\n",
                ),
                Some((PathBuf::from("/fonts/times.ttf"), 2)),
            );
            assert_eq!(
                parse_fontconfig_match("Times New Roman", b"DejaVu Serif\n/fonts/dejavu.ttf\n0\n",),
                None,
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) use platform::get;
#[cfg(not(target_arch = "wasm32"))]
pub(super) use platform::get_macroman;

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{FontFace, TIMES_FAMILY_PREFERENCE};
    use crate::quickdraw::fonts::{FontMetrics, Glyph, FONT_TIMES};
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    use wasm_bindgen::JsCast;
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

    type CachedGlyph = Option<(&'static Glyph, &'static [u8])>;

    static FACES: LazyLock<Mutex<HashMap<(i16, i16), Option<&'static FontFace>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    static MAC_ROMAN_GLYPHS: LazyLock<Mutex<HashMap<(i16, i16, u8), CachedGlyph>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    fn context(width: u32, height: u32) -> Option<(HtmlCanvasElement, CanvasRenderingContext2d)> {
        let document = web_sys::window()?.document()?;
        let canvas = document
            .create_element("canvas")
            .ok()?
            .dyn_into::<HtmlCanvasElement>()
            .ok()?;
        canvas.set_width(width);
        canvas.set_height(height);
        let context = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;
        Some((canvas, context))
    }

    fn css_font(font_id: i16, size: i16) -> Option<String> {
        if font_id != FONT_TIMES {
            return None;
        }
        let families = TIMES_FAMILY_PREFERENCE
            .iter()
            .map(|family| format!("\"{family}\""))
            .collect::<Vec<_>>()
            .join(", ");
        // A browser cannot enumerate host font files. The explicit list keeps
        // the same preference policy as desktop, while `serif` lets the user
        // agent supply its Times-compatible default on restricted browsers.
        Some(format!("{}px {families}, serif", size.max(1)))
    }

    fn rasterize_character(
        measuring: &CanvasRenderingContext2d,
        css_font: &str,
        character: char,
        data: &mut Vec<u8>,
    ) -> Option<Glyph> {
        let text = character.to_string();
        let metrics = measuring.measure_text(&text).ok()?;
        let advance = metrics.width().round().clamp(0.0, u8::MAX as f64) as u8;
        // Canvas reports distances *from* the alignment point. Convert those
        // to an integer bitmap box relative to the guest pen, preserving
        // negative left bearings and right overhangs.
        let origin_x = (-metrics.actual_bounding_box_left()).floor() as i16;
        let right = metrics.actual_bounding_box_right().ceil() as i16;
        let origin_y = (-metrics.actual_bounding_box_ascent()).floor() as i16;
        let bottom = metrics.actual_bounding_box_descent().ceil() as i16;
        let width = u32::try_from((right - origin_x).max(0)).ok()?;
        let height = u32::try_from((bottom - origin_y).max(0)).ok()?;
        let data_offset = data.len();
        if width == 0 || height == 0 {
            return Some(Glyph {
                width: 0,
                height: 0,
                advance,
                origin_x: 0,
                origin_y: 0,
                data_offset,
            });
        }

        let padding = 2u32;
        let (_, drawing) = context(width + padding * 2, height + padding * 2)?;
        drawing.set_font(css_font);
        drawing.set_text_baseline("alphabetic");
        drawing.set_fill_style_str("white");
        drawing
            .fill_text(
                &text,
                f64::from(padding) - f64::from(origin_x),
                f64::from(padding) - f64::from(origin_y),
            )
            .ok()?;
        let pixels = drawing
            .get_image_data(
                f64::from(padding),
                f64::from(padding),
                f64::from(width),
                f64::from(height),
            )
            .ok()?
            .data();
        data.extend(
            pixels
                .chunks_exact(4)
                .map(|rgba| if rgba[3] >= 128 { 255 } else { 0 }),
        );
        Some(Glyph {
            width: u8::try_from(width).ok()?,
            height: u8::try_from(height).ok()?,
            advance,
            origin_x: i8::try_from(origin_x).ok()?,
            origin_y: i8::try_from(origin_y).ok()?,
            data_offset,
        })
    }

    fn rasterize_face(font_id: i16, size: i16) -> Option<&'static FontFace> {
        let size = size.max(1);
        let rough_extent = u32::try_from(i32::from(size) * 4 + 32).ok()?;
        let (_, measuring) = context(rough_extent, rough_extent)?;
        let css_font = css_font(font_id, size)?;
        measuring.set_font(&css_font);
        measuring.set_text_baseline("alphabetic");

        let mut glyphs = Vec::with_capacity(95);
        let mut data = Vec::new();
        let mut widest_advance = 0i16;
        let mut face_ascent = 0i16;
        let mut face_descent = 0i16;
        for code in b' '..=b'~' {
            let glyph = rasterize_character(&measuring, &css_font, code as char, &mut data)?;
            widest_advance = widest_advance.max(i16::from(glyph.advance));
            face_ascent = face_ascent.max(-i16::from(glyph.origin_y));
            face_descent = face_descent.max(i16::from(glyph.origin_y) + i16::from(glyph.height));
            glyphs.push(glyph);
        }

        Some(Box::leak(Box::new(FontFace {
            font_id,
            size,
            metrics: FontMetrics {
                ascent: face_ascent,
                descent: face_descent,
                wid_max: widest_advance,
                leading: 0,
            },
            glyphs: Box::leak(glyphs.into_boxed_slice()),
            data: Box::leak(data.into_boxed_slice()),
        })))
    }

    fn rasterize_mac_roman_glyph(
        font_id: i16,
        size: i16,
        mac_code: u8,
    ) -> Option<(&'static Glyph, &'static [u8])> {
        let size = size.max(1);
        let rough_extent = u32::try_from(i32::from(size) * 4 + 32).ok()?;
        let (_, measuring) = context(rough_extent, rough_extent)?;
        let css_font = css_font(font_id, size)?;
        measuring.set_font(&css_font);
        measuring.set_text_baseline("alphabetic");
        let mut data = Vec::new();
        let character = crate::mac_roman::decode_byte(mac_code);
        let mut glyph = rasterize_character(&measuring, &css_font, character, &mut data)?;
        glyph.data_offset = 0;
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        Some((Box::leak(Box::new(glyph)), data))
    }

    pub(crate) fn get(font_id: i16, size: i16) -> Option<&'static FontFace> {
        css_font(font_id, size)?;
        let size = if size == 0 { 12 } else { size };
        let key = (font_id, size);
        let mut faces = FACES.lock().expect("browser outline font cache poisoned");
        if let Some(face) = faces.get(&key) {
            return *face;
        }
        let face = rasterize_face(font_id, size);
        faces.insert(key, face);
        face
    }

    pub(crate) fn get_macroman(
        font_id: i16,
        size: i16,
        mac_code: u8,
    ) -> Option<(&'static Glyph, &'static [u8])> {
        css_font(font_id, size)?;
        let size = if size == 0 { 12 } else { size };
        let key = (font_id, size, mac_code);
        let mut glyphs = MAC_ROMAN_GLYPHS
            .lock()
            .expect("browser outline Mac Roman glyph cache poisoned");
        if let Some(glyph) = glyphs.get(&key) {
            return *glyph;
        }
        let glyph = rasterize_mac_roman_glyph(font_id, size, mac_code);
        glyphs.insert(key, glyph);
        glyph
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) use browser::get;
#[cfg(target_arch = "wasm32")]
pub(super) use browser::get_macroman;

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn times_family_preference_is_deterministic() {
        let available = ["Nimbus Roman", "Times New Roman"];
        let selected = resolve_preferred_times(|family| {
            available.iter().position(|available| *available == family)
        });
        assert_eq!(
            selected.map(|index| available[index]),
            Some("Times New Roman")
        );
    }

    #[test]
    fn times_family_preference_has_no_arbitrary_desktop_serif() {
        let selected = resolve_preferred_times(|family| (family == "DejaVu Serif").then_some(()));
        assert_eq!(selected, None);
    }
}
