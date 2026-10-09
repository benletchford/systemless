//! Guest cursor image/visibility for the browser's native pointer. Movement
//! belongs to the browser; only image and presentation-scale changes encode PNGs.

use base64::Engine;
use image::ImageEncoder;
use systemless::systems::macintosh::display::{self, CursorImage};

pub(crate) struct HostCursor {
    key: Option<(Option<CursorImage>, u64)>,
    pub css: String,
    pub software: bool,
}

impl Default for HostCursor {
    fn default() -> Self {
        Self {
            key: None,
            css: "none".into(),
            software: true,
        }
    }
}

impl HostCursor {
    pub fn update(&mut self, image: Option<&CursorImage>, scale: f64) -> bool {
        if self
            .key
            .as_ref()
            .is_some_and(|(old, old_scale)| old.as_ref() == image && *old_scale == scale.to_bits())
        {
            return false;
        }
        self.key = Some((image.cloned(), scale.to_bits()));
        self.css = "none".into();
        self.software = false;
        if let Some(image) = image {
            if let Some(css) = native_css(image, scale) {
                self.css = css;
            } else {
                self.software = true;
            }
        }
        true
    }
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

fn native_css(image: &CursorImage, scale: f64) -> Option<String> {
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
    if !scale.is_finite()
        || scale <= 0.0
        || hot_h < 0
        || hot_v < 0
        || hot_h as usize >= w
        || hot_v as usize >= h
    {
        return None;
    }
    let out_w = (w as f64 * scale).round().max(1.0) as usize;
    let out_h = (h as f64 * scale).round().max(1.0) as usize;
    // Chromium/Firefox reject oversized image cursors. Keep the software
    // overlay instead of hiding it and relying on an ignored CSS image.
    if out_w > 128 || out_h > 128 {
        return None;
    }
    let mut source = vec![0; w * h * 4];
    display::render_cursor(&mut source, w as u32, h as u32, image, (hot_v, hot_h));
    let mut scaled = vec![0; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let from = ((y * h / out_h) * w + x * w / out_w) * 4;
            let to = (y * out_w + x) * 4;
            scaled[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &scaled,
            out_w as u32,
            out_h as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);
    let hot_h = ((hot_h as f64 * scale).round() as usize).min(out_w - 1);
    let hot_v = ((hot_v as f64 * scale).round() as usize).min(out_h - 1);
    Some(format!(
        "url(\"data:image/png;base64,{encoded}\") {hot_h} {hot_v}, none"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 9.0] {
            assert!(state.update(Some(&image), scale));
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
