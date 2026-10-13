//! Guest cursor image/visibility for the browser's native pointer. Movement
//! belongs to the browser; only image and presentation-scale changes encode PNGs.

use base64::Engine;
use image::ImageEncoder;
use systemless::systems::macintosh::display::{self, CursorImage};

#[path = "host_cursor_layout.rs"]
mod layout;
use layout::CursorLayouts;

pub(crate) struct HostCursor {
    key: Option<(Option<CursorImage>, Option<NativeSize>, Option<NativeSize>)>,
    layouts: Option<CursorLayouts>,
    pub css: String,
    pub software: bool,
}

impl Default for HostCursor {
    fn default() -> Self {
        Self {
            key: None,
            layouts: normalize_hotspots_on_this_browser().then(CursorLayouts::default),
            css: "none".into(),
            software: true,
        }
    }
}

impl HostCursor {
    pub fn update(&mut self, image: Option<&CursorImage>, scale: f64) -> bool {
        // Coordinate-derived scale can vary below a pixel. Cache the actual
        // bitmap dimensions and hotspot, not the floating-point measurement.
        let size = image.and_then(|image| native_size(image, scale));
        let layout = size.and_then(|_| self.layouts.as_ref()?.plan(scale));
        if self
            .key
            .as_ref()
            .is_some_and(|(old, old_size, old_layout)| {
                old.as_ref() == image && *old_size == size && *old_layout == layout
            })
        {
            return false;
        }
        self.css = "none".into();
        self.software = false;
        if let Some(image) = image {
            if let Some(css) = native_css_with_layout(image, scale, self.layouts.as_mut()) {
                self.css = css;
            } else {
                self.software = true;
            }
        }
        // Encoding may have learned a new shape. Cache its resulting layout;
        // zoom can also change a known shape's extent while the current
        // image's own rounded dimensions/hotspot stay the same.
        let layout = size.and_then(|_| self.layouts.as_ref()?.plan(scale));
        self.key = Some((image.cloned(), size, layout));
        true
    }
}

// Restrict the presentation workaround to the browser/host combination
// investigated so far. Other hosts retain their existing native cursor path.
fn needs_hotspot_normalization(ua: &str, touch_points: u32) -> bool {
    ua.contains("Macintosh") && ua.contains("Chrome/") && touch_points <= 1
}

fn normalize_hotspots_on_this_browser() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        // BrowserMachine is also created in a worker, where window() is absent.
        let read = |object: &wasm_bindgen::JsValue, key: &str| {
            js_sys::Reflect::get(object, &wasm_bindgen::JsValue::from_str(key)).ok()
        };
        let global: wasm_bindgen::JsValue = js_sys::global().into();
        let Some(navigator) = read(&global, "navigator") else { return false };
        let ua = read(&navigator, "userAgent").and_then(|v| v.as_string()).unwrap_or_default();
        let touch = read(&navigator, "maxTouchPoints").and_then(|v| v.as_f64()).unwrap_or(0.0);
        needs_hotspot_normalization(&ua, touch as u32)
    }
    #[cfg(not(target_arch = "wasm32"))]
    { false }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeSize {
    width: usize,
    height: usize,
    hot_h: usize,
    hot_v: usize,
}

impl NativeSize {
    fn scaled(self, scale: f64) -> Option<Self> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let width = (self.width as f64 * scale).round().max(1.0) as usize;
        let height = (self.height as f64 * scale).round().max(1.0) as usize;
        // Keep software for images that already exceed the browser limit.
        // If only normalization exceeds it, retain the original native image.
        if width > 128 || height > 128 {
            return None;
        }
        Some(Self {
            width, height,
            hot_h: ((self.hot_h as f64 * scale).round() as usize).min(width - 1),
            hot_v: ((self.hot_v as f64 * scale).round() as usize).min(height - 1),
        })
    }
}

fn native_size(image: &CursorImage, scale: f64) -> Option<NativeSize> {
    let (w, h, hot_h, hot_v) = match image {
        CursorImage::Mono { hot_h, hot_v, .. } => (16, 16, *hot_h, *hot_v),
        CursorImage::Color {
            width,
            height,
            hot_h,
            hot_v,
            ..
        } => (usize::from(*width), usize::from(*height), *hot_h, *hot_v),
    };
    if !scale.is_finite()
        || scale <= 0.0
        || w == 0
        || h == 0
        || hot_h < 0
        || hot_v < 0
        || hot_h as usize >= w
        || hot_v as usize >= h
    {
        return None;
    }
    NativeSize { width: w, height: h, hot_h: hot_h as usize, hot_v: hot_v as usize }.scaled(scale)
}

pub(crate) fn native_pointer_supported(
    ua: &str,
    touch_points: i32,
    fine: bool,
    pointer: Option<&str>,
) -> bool {
    // iPadOS permits its system pointer, but does not display PNG cursors.
    // Its desktop-site user agent identifies as Macintosh.
    let ipad = ua.contains("iPad") || (ua.contains("Macintosh") && touch_points > 1);
    fine && !ipad && pointer.is_none_or(|kind| kind == "mouse")
}

#[cfg(test)]
fn native_css(image: &CursorImage, scale: f64) -> Option<String> {
    native_css_with_layout(image, scale, None)
}

fn native_css_with_layout(image: &CursorImage, scale: f64, layouts: Option<&mut CursorLayouts>) -> Option<String> {
    let mut size = native_size(image, scale)?;
    let (w, h, hot_h, hot_v) = match image {
        CursorImage::Mono {
            data,
            mask,
            hot_h,
            hot_v,
        } => {
            // An image cursor cannot invert the destination beneath it.
            if data.iter().zip(mask).any(|(data, mask)| data & !mask != 0) {
                return None;
            }
            (16, 16, *hot_h, *hot_v)
        }
        CursorImage::Color {
            width,
            height,
            pixels_argb,
            mask,
            hot_h,
            hot_v,
            ..
        } => {
            let (w, h) = (usize::from(*width), usize::from(*height));
            if w == 0 || h == 0 || w > 128 || h > 128 || pixels_argb.len() < w * h {
                return None;
            }
            for (i, &color) in pixels_argb.iter().take(w * h).enumerate() {
                let (row, col) = (i / w, i % w);
                let masked =
                    row < 16 && col < 16 && mask[row * 2 + col / 8] & (0x80 >> (col % 8)) != 0;
                if !masked && color == 0xff00_0000 {
                    return None;
                }
            }
            (w, h, *hot_h, *hot_v)
        }
    };
    let (out_w, out_h) = (size.width, size.height);
    let mut source = vec![0; w * h * 4];
    display::render_cursor(&mut source, w as u32, h as u32, image, (hot_v, hot_h));
    let layout = layouts.and_then(|layouts| {
        let source_size = NativeSize { width: w, height: h, hot_h: hot_h as usize, hot_v: hot_v as usize };
        layouts.observe(source_size, &source).then(|| layouts.plan(scale)).flatten()
    });
    let mut scaled = vec![0; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let from = ((y * h / out_h) * w + x * w / out_w) * 4;
            let to = (y * out_w + x) * 4;
            scaled[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
    if let Some(layout) = layout {
        if let Some(packed) = layout::repack(&scaled, size, layout) {
            scaled = packed;
            size = layout;
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &scaled,
            size.width as u32,
            size.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    let (hot_h, hot_v) = (size.hot_h, size.hot_v);
    Some(format!(
        "url(\"data:image/png;base64,{encoded}\") {hot_h} {hot_v}, none"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_cursor(bounds: (usize, usize, usize, usize), hot: (i16, i16)) -> CursorImage {
        let mut mask = [0; 32];
        for y in bounds.1..bounds.3 {
            for x in bounds.0..bounds.2 {
                mask[y * 2 + x / 8] |= 0x80 >> (x % 8);
            }
        }
        CursorImage::mono([0; 32], mask, hot.1, hot.0)
    }

    fn decode_cursor(css: &str) -> (image::RgbaImage, (i32, i32)) {
        let encoded = css.split("base64,").nth(1).unwrap().split('"').next().unwrap();
        let png = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let mut hot = css.split(") ").nth(1).unwrap().trim_end_matches(", none").split_whitespace();
        (image::load_from_memory(&png).unwrap().to_rgba8(),
         (hot.next().unwrap().parse().unwrap(), hot.next().unwrap().parse().unwrap()))
    }

    fn visible_pixels(css: &str) -> Vec<(i32, i32, [u8; 4])> {
        let (png, hot) = decode_cursor(css);
        png.enumerate_pixels().filter(|(_, _, p)| p.0[3] != 0)
            .map(|(x, y, p)| (x as i32 - hot.0, y as i32 - hot.1, p.0)).collect()
    }

    fn normalized_state() -> HostCursor {
        HostCursor { layouts: Some(CursorLayouts::default()), ..HostCursor::default() }
    }

    #[test]
    fn normalization_is_scoped_to_mac_chromium() {
        assert!(needs_hotspot_normalization("Mozilla/5.0 (Macintosh; Intel Mac OS X) Chrome/155.0", 0));
        assert!(!needs_hotspot_normalization("Mozilla/5.0 (Macintosh) Chrome/155.0", 5));
        assert!(!needs_hotspot_normalization("Mozilla/5.0 (Macintosh) Safari/605.1", 0));
        assert!(!needs_hotspot_normalization("Mozilla/5.0 (Windows NT) Chrome/155.0", 0));
        assert!(!needs_hotspot_normalization("Mozilla/5.0 (X11; Linux) Chrome/155.0", 0));
    }

    #[test]
    fn known_shapes_keep_shared_origin_and_exact_visible_pixels_through_zoom() {
        let shapes = [solid_cursor((0, 0, 11, 16), (1, 1)),
            solid_cursor((2, 0, 13, 16), (8, 8)), solid_cursor((1, 1, 15, 15), (7, 7))];
        let mut state = normalized_state();
        for image in &shapes { state.update(Some(image), 1.0); }
        assert_eq!(decode_cursor(&state.css).1, (6, 8));
        for scale in [0.25, 0.75, 0.9975, 1.0, 1.2475, 1.355, 2.0, 4.0, 1.0] {
            let mut layout = None;
            for image in &shapes {
                state.update(Some(image), scale);
                assert!(!state.software);
                assert_eq!(visible_pixels(&state.css), visible_pixels(&native_css(image, scale).unwrap()));
                let (png, hot) = decode_cursor(&state.css);
                if let Some(expected) = layout { assert_eq!(expected, (png.dimensions(), hot)); }
                layout = Some((png.dimensions(), hot));
            }
        }
        // Hidden/unsupported images must not erase learned geometry or force
        // software movement for subsequent supported cursor images.
        state.update(None, 1.0);
        assert_eq!(state.css, "none");
        assert!(!state.software);
        state.update(Some(&CursorImage::mono([255; 32], [0; 32], 0, 0)), 1.0);
        assert!(state.software);
        state.update(Some(&shapes[0]), 1.0);
        assert_eq!(decode_cursor(&state.css).1, (6, 8));
        assert!(!state.software);
    }

    #[test]
    fn zoom_checks_learned_shape_even_if_current_image_size_is_unchanged() {
        let arrow = solid_cursor((0, 0, 11, 16), (1, 1));
        let cross = solid_cursor((1, 1, 15, 15), (7, 7));
        let mut state = normalized_state();
        state.update(Some(&cross), 1.0);
        state.update(Some(&arrow), 1.213);
        let before = decode_cursor(&state.css).1;
        assert_eq!(native_size(&arrow, 1.213), native_size(&arrow, 1.215));
        assert!(state.update(Some(&arrow), 1.215));
        assert_ne!(before, decode_cursor(&state.css).1);
        let pointer = state.css.as_ptr();
        assert!(!state.update(Some(&arrow), 1.215001));
        assert_eq!(pointer, state.css.as_ptr());
        // A newly learned shape doesn't trigger redundant encoding next frame.
        assert!(state.update(Some(&cross), 1.215001));
        assert!(!state.update(Some(&cross), 1.215001));
    }

    #[test]
    fn padding_limit_retains_original_native_cursor_and_recovers_at_normal_scale() {
        let arrow = solid_cursor((0, 0, 11, 16), (1, 1));
        let cross = solid_cursor((1, 1, 15, 15), (7, 7));
        let mut state = normalized_state();
        state.update(Some(&arrow), 1.0);
        state.update(Some(&cross), 1.0);
        state.update(Some(&arrow), 8.0);
        assert!(!state.software);
        assert_eq!(state.css, native_css(&arrow, 8.0).unwrap());
        assert!(!state.update(Some(&arrow), 8.0));
        state.update(Some(&arrow), 1.0);
        assert_eq!(decode_cursor(&state.css).1, (6, 6));
    }

    #[test]
    fn touch_and_ipad_keep_the_guest_cursor() {
        assert!(native_pointer_supported("Windows", 10, true, Some("mouse")));
        assert!(!native_pointer_supported(
            "Windows",
            10,
            true,
            Some("touch")
        ));
        assert!(!native_pointer_supported("Windows", 10, true, Some("pen")));
        assert!(!native_pointer_supported(
            "Macintosh",
            5,
            true,
            Some("mouse")
        ));
        assert!(!native_pointer_supported("iPad", 0, true, None));
        assert!(native_pointer_supported("Macintosh", 0, true, None));
        assert!(!native_pointer_supported("Android", 5, false, None));
    }

    #[test]
    fn image_preserves_mask_colours_scale_and_hotspot() {
        let mut data = [0; 32];
        let mut mask = [0; 32];
        data[0] = 0x80;
        mask[0] = 0xc0;
        let cursor = CursorImage::mono(data, mask, 3, 2);
        let css = native_css(&cursor, 1.5).unwrap();
        assert!(css.ends_with(" 3 5, none"));
        let encoded = css
            .split("base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (24, 24));
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(2, 0).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(3, 0).0, [0, 0, 0, 0]);
    }

    #[test]
    fn unsupported_cursors_keep_software_and_hidden_cursors_hide_both() {
        let mut state = HostCursor::default();
        let image = CursorImage::mono([0; 32], [255; 32], 0, 0);
        assert!(state.update(Some(&image), 1.0));
        assert!(!state.software);
        let pointer = state.css.as_ptr();
        assert!(!state.update(Some(&image), 1.0));
        assert_eq!(state.css.as_ptr(), pointer);
        for (index, scale) in [0.0, -1.0, f64::NAN, f64::INFINITY, 9.0]
            .into_iter()
            .enumerate()
        {
            assert_eq!(state.update(Some(&image), scale), index == 0);
            assert!(state.software);
            assert_eq!(state.css, "none");
        }
        let invert = CursorImage::mono([255; 32], [0; 32], 0, 0);
        assert!(state.update(Some(&invert), 1.0));
        assert!(state.software);
        assert!(state.update(None, 1.0));
        assert!(!state.software);
        assert_eq!(state.css, "none");
        assert!(native_css(&CursorImage::mono([0; 32], [255; 32], -1, 0), 1.0).is_none());
    }

    #[test]
    fn scale_measurement_noise_reuses_identical_pixels_and_hotspot() {
        let mut state = HostCursor::default();
        let image = CursorImage::mono([0; 32], [255; 32], 7, 7);
        assert!(state.update(Some(&image), 1.0));
        let pointer = state.css.as_ptr();
        for scale in [0.9999, 1.0001, 1.001, 1.01] {
            assert!(!state.update(Some(&image), scale));
            assert_eq!(state.css.as_ptr(), pointer);
        }
        assert!(state.update(Some(&image), 1.213));
        // Both images are 19 pixels wide, but the hotspot crosses a pixel.
        assert!(state.update(Some(&image), 1.215));
    }

    #[test]
    fn colour_cursor_inversion_and_malformed_images_fall_back() {
        let mut pixels = vec![0xffff_ffff; 16 * 16];
        let mut image = CursorImage::Color {
            width: 16,
            height: 16,
            pixels_argb: pixels.clone(),
            mask: [0; 32],
            hot_v: 0,
            hot_h: 0,
            mono_data: [0; 32],
            mono_mask: [0; 32],
        };
        assert!(native_css(&image, 1.0).is_some());
        pixels[0] = 0xff00_0000;
        if let CursorImage::Color { pixels_argb, .. } = &mut image {
            *pixels_argb = pixels;
        }
        assert!(native_css(&image, 1.0).is_none());
        if let CursorImage::Color { mask, .. } = &mut image {
            mask[0] = 0x80;
        }
        assert!(native_css(&image, 1.0).is_some());
        if let CursorImage::Color { pixels_argb, .. } = &mut image {
            pixels_argb.clear();
        }
        assert!(native_css(&image, 1.0).is_none());
    }
}
