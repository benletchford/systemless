//! Guest cursor as the host's hardware pointer.
//!
//! The composited cursor overlay rides the same present queue as the guest
//! frame, so it is shown exactly as late as the frame is: several vsyncs behind
//! the pointer on a composited desktop. The display controller's cursor plane
//! has none of that latency. On macOS the guest's current cursor image is
//! therefore installed as the window's `NSCursor`, sized so one guest pixel
//! matches the window's current guest-to-screen scale, and the frame is
//! presented without a cursor overlay. A hidden guest cursor hides the host
//! pointer while it is over the window.
//!
//! `SYSTEMLESS_SOFTWARE_CURSOR=1` restores the composited overlay.

use objc2::rc::Retained;
use objc2::ClassType;
use objc2_app_kit::{NSBitmapImageRep, NSCursor, NSDeviceRGBColorSpace, NSImage};
use objc2_foundation::{NSPoint, NSSize};
use systemless::display::CursorImage;
use winit::window::Window;

/// RGBA pixels for a guest cursor, straight alpha, plus the hotspot in pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorRgba {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub hot_h: usize,
    pub hot_v: usize,
}

pub struct HostCursor {
    enabled: bool,
    pointer_inside: bool,
    cursor: Option<Retained<NSCursor>>,
    /// (guest image, presentation scale bits, backing scale bits) the cursor
    /// was built for.
    key: Option<(Option<CursorImage>, u64, u64)>,
}

impl HostCursor {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYSTEMLESS_SOFTWARE_CURSOR").is_none(),
            pointer_inside: false,
            cursor: None,
            key: None,
        }
    }

    /// False when the composited overlay is in use instead.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_pointer_inside(&mut self, inside: bool) {
        self.pointer_inside = inside;
    }

    /// Rebuild the host cursor when the guest image or the presentation scale
    /// changed, mirror the guest's cursor visibility, and re-apply the cursor.
    ///
    /// `scale_phys` is physical pixels per guest pixel of the active
    /// presentation viewport — the same `ContentRect` + min-axis rule used by
    /// rendering and input (see [`presentation_scale`]).
    pub fn sync(&mut self, window: &Window, cursor: Option<&CursorImage>, scale_phys: f64) {
        if !self.enabled {
            return;
        }
        let backing = window.scale_factor().max(0.01);
        let key = (cursor.cloned(), scale_phys.to_bits(), backing.to_bits());
        if self.key.as_ref() != Some(&key) {
            match cursor {
                None => {
                    self.cursor = None;
                    window.set_cursor_visible(false);
                }
                Some(image) => {
                    let rgba = cursor_rgba(image);
                    let scale = scale_phys;
                    // Integer-upscaled bitmap so the cursor stays crisp; the
                    // image size in points carries the exact scale.
                    let n = (scale.round() as usize).max(1);
                    let pixels = upscale_rgba(&rgba.pixels, rgba.width, rgba.height, n);
                    let points_per_guest_px = scale / backing;
                    let size = (
                        rgba.width as f64 * points_per_guest_px,
                        rgba.height as f64 * points_per_guest_px,
                    );
                    let hotspot = (
                        rgba.hot_h as f64 * points_per_guest_px,
                        rgba.hot_v as f64 * points_per_guest_px,
                    );
                    self.cursor = Some(make_ns_cursor(
                        &pixels,
                        rgba.width * n,
                        rgba.height * n,
                        size,
                        hotspot,
                    ));
                    window.set_cursor_visible(true);
                }
            }
            self.key = Some(key);
        }
        self.reassert();
    }

    /// AppKit resets the cursor on `cursorUpdate:` (winit installs its own);
    /// re-apply ours whenever the pointer is over the window.
    pub fn reassert(&self) {
        if !self.enabled || !self.pointer_inside {
            return;
        }
        if let Some(ours) = self.cursor.as_ref() {
            let current = unsafe { NSCursor::currentCursor() };
            if !std::ptr::eq(&*current, &**ours) {
                unsafe { ours.set() };
            }
        }
    }
}

/// Physical pixels per guest pixel for a presentation viewport: the active
/// content rectangle scaled by the binding axis, exactly as rendering and
/// input compute it (`min(drawable_width / content.width, drawable_height /
/// content.height)`). Falls back to 1.0 when any dimension is zero.
pub fn presentation_scale(
    content_width: u32,
    content_height: u32,
    drawable_width: u32,
    drawable_height: u32,
) -> f64 {
    if content_width == 0 || content_height == 0 || drawable_width == 0 || drawable_height == 0 {
        return 1.0;
    }
    (f64::from(drawable_width) / f64::from(content_width))
        .min(f64::from(drawable_height) / f64::from(content_height))
}

/// Convert a guest cursor to straight-alpha RGBA.
///
/// Mono cursors follow the QuickDraw rule: a set mask bit paints the pixel
/// black (data 1) or white (data 0); with the mask clear, data 1 would invert
/// the pixels underneath, which a hardware cursor cannot do, so it is drawn
/// black; the rest is transparent. Colour cursors take their alpha from the
/// 1-bit mask.
pub fn cursor_rgba(image: &CursorImage) -> CursorRgba {
    fn bit(plane: &[u8; 32], row: usize, col: usize) -> bool {
        (plane[row * 2 + col / 8] >> (7 - (col % 8))) & 1 == 1
    }
    fn clamp_hot(hot: i16, max: usize) -> usize {
        usize::try_from(hot).unwrap_or(0).min(max.saturating_sub(1))
    }
    match image {
        CursorImage::Mono {
            data,
            mask,
            hot_v,
            hot_h,
        } => {
            let mut pixels = vec![0u8; 16 * 16 * 4];
            for row in 0..16 {
                for col in 0..16 {
                    let (d, m) = (bit(data, row, col), bit(mask, row, col));
                    let px = match (m, d) {
                        (true, true) => [0, 0, 0, 255],
                        (true, false) => [255, 255, 255, 255],
                        (false, true) => [0, 0, 0, 255],
                        (false, false) => [0, 0, 0, 0],
                    };
                    pixels[(row * 16 + col) * 4..][..4].copy_from_slice(&px);
                }
            }
            CursorRgba {
                pixels,
                width: 16,
                height: 16,
                hot_h: clamp_hot(*hot_h, 16),
                hot_v: clamp_hot(*hot_v, 16),
            }
        }
        CursorImage::Color {
            width,
            height,
            pixels_argb,
            mask,
            hot_v,
            hot_h,
            ..
        } => {
            let (w, h) = (usize::from(*width).max(1), usize::from(*height).max(1));
            let mut pixels = vec![0u8; w * h * 4];
            for row in 0..h {
                for col in 0..w {
                    let argb = pixels_argb.get(row * w + col).copied().unwrap_or(0);
                    let opaque = row < 16 && col < 16 && bit(mask, row, col);
                    let px = if opaque {
                        [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, 255]
                    } else {
                        [0, 0, 0, 0]
                    };
                    pixels[(row * w + col) * 4..][..4].copy_from_slice(&px);
                }
            }
            CursorRgba {
                pixels,
                width: w,
                height: h,
                hot_h: clamp_hot(*hot_h, w),
                hot_v: clamp_hot(*hot_v, h),
            }
        }
    }
}

/// Nearest-neighbour integer upscale of an RGBA bitmap.
pub fn upscale_rgba(rgba: &[u8], width: usize, height: usize, n: usize) -> Vec<u8> {
    if n <= 1 {
        return rgba.to_vec();
    }
    let (out_w, out_h) = (width * n, height * n);
    let mut out = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let src = ((y / n) * width + x / n) * 4;
            let dst = (y * out_w + x) * 4;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    out
}

/// Build an `NSCursor` from `width`×`height` straight-alpha RGBA pixels shown at
/// `size` points with the hotspot at `hotspot` points.
fn make_ns_cursor(
    rgba: &[u8],
    width: usize,
    height: usize,
    size: (f64, f64),
    hotspot: (f64, f64),
) -> Retained<NSCursor> {
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            (width * 4) as isize,
            32,
        )
        .expect("NSBitmapImageRep for the guest cursor")
    };
    // Every pixel is either fully opaque or fully transparent, so the buffer is
    // valid premultiplied data as well.
    unsafe {
        std::slice::from_raw_parts_mut(bitmap.bitmapData(), rgba.len()).copy_from_slice(rgba)
    };
    let size = NSSize::new(size.0, size.1);
    unsafe { bitmap.setSize(size) };
    let image = unsafe { NSImage::initWithSize(NSImage::alloc(), size) };
    unsafe { image.addRepresentation(&bitmap) };
    NSCursor::initWithImage_hotSpot(
        NSCursor::alloc(),
        &image,
        NSPoint::new(hotspot.0, hotspot.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(rgba: &CursorRgba, row: usize, col: usize) -> [u8; 4] {
        let i = (row * rgba.width + col) * 4;
        rgba.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn presentation_scale_uses_the_binding_axis_for_height_constrained_windows() {
        // 800x600 content in a 1600x900 drawable: width would allow 2.0 but
        // the height axis binds at 1.5 — the cursor must not be oversized.
        assert_eq!(presentation_scale(800, 600, 1600, 900), 1.5);
    }

    #[test]
    fn presentation_scale_follows_a_cropped_presentation() {
        // A learned 640-wide gameplay crop inside an 800-wide guest fills an
        // 800px drawable at 1.25 — scaling by the full guest width (1.0)
        // would undersize the cursor.
        assert_eq!(presentation_scale(640, 480, 800, 600), 1.25);
        assert_eq!(
            presentation_scale(0, 480, 800, 600),
            1.0,
            "zero dims fall back"
        );
    }

    #[test]
    fn mono_cursor_follows_the_quickdraw_mask_rules() {
        let mut data = [0u8; 32];
        let mut mask = [0u8; 32];
        data[0] = 0b1000_0000; // (0,0) data 1
        mask[0] = 0b1100_0000; // (0,0) and (0,1) masked
        data[2] = 0b1000_0000; // (1,0) data 1, mask 0 -> "invert", drawn black
        let rgba = cursor_rgba(&CursorImage::mono(data, mask, 3, 2));
        assert_eq!(
            (rgba.width, rgba.height, rgba.hot_h, rgba.hot_v),
            (16, 16, 2, 3)
        );
        assert_eq!(px(&rgba, 0, 0), [0, 0, 0, 255]);
        assert_eq!(px(&rgba, 0, 1), [255, 255, 255, 255]);
        assert_eq!(px(&rgba, 1, 0), [0, 0, 0, 255]);
        assert_eq!(px(&rgba, 1, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn hotspot_is_clamped_into_the_image() {
        let rgba = cursor_rgba(&CursorImage::mono([0; 32], [0; 32], -4, 40));
        assert_eq!((rgba.hot_h, rgba.hot_v), (15, 0));
    }

    #[test]
    fn color_cursor_takes_alpha_from_the_mask() {
        let mut mask = [0u8; 32];
        mask[0] = 0b1000_0000;
        let image = CursorImage::Color {
            width: 2,
            height: 1,
            pixels_argb: vec![0x00FF8040, 0x00123456],
            mask,
            hot_v: 0,
            hot_h: 1,
            mono_data: [0; 32],
            mono_mask: [0; 32],
        };
        let rgba = cursor_rgba(&image);
        assert_eq!((rgba.width, rgba.height, rgba.hot_h), (2, 1, 1));
        assert_eq!(px(&rgba, 0, 0), [0xFF, 0x80, 0x40, 255]);
        assert_eq!(px(&rgba, 0, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn upscale_duplicates_pixels() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8]; // 2x1
        let up = upscale_rgba(&src, 2, 1, 2);
        assert_eq!(up.len(), 4 * 2 * 4);
        assert_eq!(&up[0..8], &[1, 2, 3, 4, 1, 2, 3, 4]);
        assert_eq!(&up[8..16], &[5, 6, 7, 8, 5, 6, 7, 8]);
        assert_eq!(&up[16..24], &[1, 2, 3, 4, 1, 2, 3, 4]);
        assert_eq!(upscale_rgba(&src, 2, 1, 1), src.to_vec());
    }
}
