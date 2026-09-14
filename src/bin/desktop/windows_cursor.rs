//! Windows owns pointer movement; only guest image/visibility changes cross
//! the frame boundary. Winit installs a native HCURSOR (CreateIconIndirect),
//! so moving the pointer does not wait for a Systemless framebuffer present.
use super::{cursor_rgba, CursorRgba};
use systemless::display::CursorImage;

const BLACK_ARGB: u32 = 0xff00_0000;
use winit::{event_loop::ActiveEventLoop, window::CustomCursor, window::Window};

pub struct HostCursor {
    enabled: bool,
    software_overlay: bool,
    key: Option<(Option<CursorImage>, u64)>,
}

impl HostCursor {
    pub fn new() -> Self {
        let enabled = std::env::var_os("SYSTEMLESS_SOFTWARE_CURSOR").is_none();
        Self {
            enabled,
            software_overlay: !enabled,
            key: None,
        }
    }

    /// Whether the current image is installed as a native pointer.
    pub fn enabled(&self) -> bool {
        self.enabled && !self.software_overlay
    }

    pub fn sync(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: &Window,
        image: Option<&CursorImage>,
        scale: f64,
    ) {
        if !self.enabled {
            return;
        }
        if self
            .key
            .as_ref()
            .is_some_and(|(old, old_scale)| old.as_ref() == image && *old_scale == scale.to_bits())
        {
            return;
        }
        self.software_overlay = false;
        match image {
            None => window.set_cursor_visible(false),
            Some(image) => {
                match native_cursor_rgba(image, scale).and_then(|rgba| {
                    CustomCursor::from_rgba(
                        rgba.pixels,
                        rgba.width as u16,
                        rgba.height as u16,
                        rgba.hot_h as u16,
                        rgba.hot_v as u16,
                    )
                    .ok()
                }) {
                    Some(source) => {
                        window.set_cursor(event_loop.create_custom_cursor(source));
                        window.set_cursor_visible(true);
                    }
                    None => {
                        // An alpha cursor cannot express destination-dependent
                        // inversion. Keep the existing framebuffer overlay for it.
                        window.set_cursor_visible(false);
                        self.software_overlay = true;
                    }
                }
            }
        }
        self.key = Some((image.cloned(), scale.to_bits()));
    }
}

/// Match the software cursor's mask/hotspot semantics. Invalid or oversized
/// images keep the software path; do not allocate from unchecked guest sizes.
fn native_cursor_rgba(image: &CursorImage, scale: f64) -> Option<CursorRgba> {
    let (w, h, hot_h, hot_v) = match image {
        CursorImage::Mono {
            data,
            mask,
            hot_h,
            hot_v,
        } => {
            if data.iter().zip(mask).any(|(d, m)| d & !m != 0) {
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
            if w == 0 || h == 0 || w > 256 || h > 256 || pixels_argb.len() < w * h {
                return None;
            }
            for (i, &color) in pixels_argb.iter().take(w * h).enumerate() {
                let (row, col) = (i / w, i % w);
                let masked =
                    row < 16 && col < 16 && mask[row * 2 + col / 8] & (0x80 >> (col % 8)) != 0;
                if !masked && color == BLACK_ARGB {
                    return None;
                }
            }
            (w, h, *hot_h, *hot_v)
        }
    };
    if hot_h < 0
        || hot_v < 0
        || hot_h as usize >= w
        || hot_v as usize >= h
        || !scale.is_finite()
        || scale <= 0.0
    {
        return None;
    }
    let (out_w, out_h) = (
        (w as f64 * scale).round().max(1.0) as usize,
        (h as f64 * scale).round().max(1.0) as usize,
    );
    if out_w > 256 || out_h > 256 {
        return None;
    }
    let rgba = cursor_rgba(image);
    let mut pixels = vec![0; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let src = ((y * h / out_h) * w + x * w / out_w) * 4;
            let dst = (y * out_w + x) * 4;
            pixels[dst..dst + 4].copy_from_slice(&rgba.pixels[src..src + 4]);
        }
    }
    Some(CursorRgba {
        pixels,
        width: out_w,
        height: out_h,
        hot_h: ((hot_h as f64 * scale).round() as usize).min(out_w - 1),
        hot_v: ((hot_v as f64 * scale).round() as usize).min(out_h - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cursor_matches_software_mask_and_fractional_scale() {
        let mut mask = [0; 32];
        let mut data = [0; 32];
        mask[0] = 0xc0;
        data[0] = 0x80;
        let image = CursorImage::mono(data, mask, 4, 2);
        let rgba = native_cursor_rgba(&image, 1.5).unwrap();
        assert_eq!(
            (rgba.width, rgba.height, rgba.hot_h, rgba.hot_v),
            (24, 24, 3, 6)
        );
        assert_eq!(&rgba.pixels[..4], &[0, 0, 0, 255]);
        assert_eq!(&rgba.pixels[8..12], &[255, 255, 255, 255]);
        assert_eq!(&rgba.pixels[12..16], &[0, 0, 0, 0]);
    }
    #[test]
    fn native_cursor_keeps_destination_dependent_and_invalid_images_in_software() {
        let mono = CursorImage::mono([0x80; 32], [0; 32], 0, 0);
        assert!(native_cursor_rgba(&mono, 1.0).is_none());
        let color = CursorImage::Color {
            width: 1,
            height: 1,
            pixels_argb: vec![BLACK_ARGB],
            mask: [0; 32],
            hot_h: 0,
            hot_v: 0,
            mono_data: [0; 32],
            mono_mask: [0; 32],
        };
        assert!(native_cursor_rgba(&color, 1.0).is_none());
        let valid = CursorImage::mono([0; 32], [0xff; 32], 0, 0);
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 100.0] {
            assert!(native_cursor_rgba(&valid, scale).is_none());
        }
        assert!(native_cursor_rgba(&CursorImage::mono([0; 32], [0; 32], -1, 0), 1.0).is_none());
    }
    #[test]
    fn native_color_cursor_preserves_rgb_and_mask() {
        let mut mask = [0; 32];
        mask[0] = 0x80;
        let color = CursorImage::Color {
            width: 2,
            height: 1,
            pixels_argb: vec![0xff123456, 0xffffffff],
            mask,
            hot_h: 1,
            hot_v: 0,
            mono_data: [0; 32],
            mono_mask: [0; 32],
        };
        let rgba = native_cursor_rgba(&color, 2.0).unwrap();
        assert_eq!((rgba.width, rgba.height, rgba.hot_h), (4, 2, 2));
        assert_eq!(
            &rgba.pixels[..8],
            &[0x12, 0x34, 0x56, 255, 0x12, 0x34, 0x56, 255]
        );
        assert_eq!(&rgba.pixels[8..16], &[0; 8]);
    }
}
