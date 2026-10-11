//! Guest cursor pixels drawn above GPUI system overlays.
use gpui_kit::{canvas, fill, point, px, rgb, size, AnyElement, Bounds, IntoElement, Styled};
use systemless::systems::macintosh::display::CursorImage;
use systemless::runner::CursorSnapshot;

/// Keep visual pointer coordinates separate from glyph-aligned guest input.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointerMapping {
    pub visual: (i16, i16),
    pub forwarded: (i16, i16),
}
impl PointerMapping {
    pub fn paint_position(self, guest: (i16, i16)) -> (i16, i16) {
        if guest == self.forwarded { self.visual } else { guest }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnsupportedCursor { Inversion, InvalidImage }

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CursorPaint {
    pub hotspot: (i16, i16),
    pub pixels: Vec<(u16, u16, u32)>,
}

impl CursorPaint {
    pub fn resolve(snapshot: &CursorSnapshot) -> Result<Self, UnsupportedCursor> {
        if !snapshot.visible { return Ok(Self { hotspot: (0, 0), pixels: Vec::new() }); }
        let image = snapshot.image.as_ref().ok_or(UnsupportedCursor::InvalidImage)?;
        let (width, height, hotspot) = match image {
            CursorImage::Mono { hot_v, hot_h, .. } => (16, 16, (*hot_v, *hot_h)),
            CursorImage::Color { width, height, hot_v, hot_h, pixels_argb, .. } => {
                if *width == 0 || *height == 0 || *width > 16 || *height > 16 || pixels_argb.len() != usize::from(*width) * usize::from(*height) {
                    return Err(UnsupportedCursor::InvalidImage);
                }
                (*width, *height, (*hot_v, *hot_h))
            }
        };
        let mut pixels = Vec::new();
        let bit = |bits: &[u8; 32], y: u16, x: u16| bits[usize::from(y) * 2 + usize::from(x / 8)] & (0x80 >> (x % 8)) != 0;
        for y in 0..height { for x in 0..width {
            let color = match image {
                CursorImage::Mono { data, mask, .. } => {
                    let ink = bit(data, y, x);
                    if !bit(mask, y, x) {
                        if ink { return Err(UnsupportedCursor::Inversion); }
                        continue;
                    }
                    if ink { 0 } else { 0xffffff }
                }
                CursorImage::Color { pixels_argb, mask, .. } => {
                    let color = pixels_argb[usize::from(y) * usize::from(width) + usize::from(x)] & 0xffffff;
                    if !bit(mask, y, x) {
                        if color == 0 { return Err(UnsupportedCursor::Inversion); }
                        if color != 0xffffff { return Err(UnsupportedCursor::InvalidImage); }
                        continue;
                    }
                    color
                }
            };
            pixels.push((x, y, color));
        }}
        Ok(Self { hotspot, pixels })
    }

    pub fn element(self, position: (i16, i16), origin: (f32, f32), scale: f32,
        extent: (u32, u32)) -> Option<AnyElement> {
        if !scale.is_finite() || scale <= 0. || !origin.0.is_finite() || !origin.1.is_finite() { return None; }
        Some(canvas(|_, _, _| {}, move |_, _, window, _| {
            let clip = Bounds::new(point(px(origin.0), px(origin.1)),
                size(px(extent.0 as f32 * scale), px(extent.1 as f32 * scale)));
            window.with_content_mask(Some(gpui_kit::ContentMask { bounds: clip }), |window| {
                for &(x, y, color) in &self.pixels {
                    let x = i32::from(position.1) - i32::from(self.hotspot.1) + i32::from(x);
                    let y = i32::from(position.0) - i32::from(self.hotspot.0) + i32::from(y);
                    window.paint_quad(fill(Bounds::new(
                        point(px(origin.0 + x as f32 * scale), px(origin.1 + y as f32 * scale)),
                        size(px(scale), px(scale))), rgb(color)));
                }
            });
        }).absolute().size_full().into_any_element())
    }
}

#[derive(Default)]
pub(crate) struct HostCursorVisibility { hidden: bool }
impl HostCursorVisibility {
    pub fn set_hidden(&mut self, hidden: bool) {
        if self.hidden == hidden { return; }
        // Called by Demo rendering/activation on GPUI's macOS main thread.
        unsafe { if hidden { objc2_app_kit::NSCursor::hide(); } else { objc2_app_kit::NSCursor::unhide(); } }
        self.hidden = hidden;
    }
}
impl Drop for HostCursorVisibility {
    fn drop(&mut self) { self.set_hidden(false); }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(image: CursorImage) -> CursorSnapshot {
        CursorSnapshot { image: Some(image), visible: true, level: 0, position: (10, 20) }
    }
    #[test]
    fn opaque_cursor_keeps_white_black_colour_and_hotspot() {
        let mut data = [0; 32]; data[0] = 0x80;
        let mut mask = [0; 32]; mask[0] = 0xc0;
        let plan = CursorPaint::resolve(&snapshot(CursorImage::mono(data, mask, 3, 4))).unwrap();
        assert_eq!(plan.hotspot, (3, 4));
        assert_eq!(plan.pixels, vec![(0, 0, 0), (1, 0, 0xffffff)]);
        let color = CursorImage::Color { width: 2, height: 1,
            pixels_argb: vec![0xff12abcd, 0xffffffff], mask: { let mut mask = [0; 32]; mask[0] = 0x80; mask },
            hot_v: 1, hot_h: 2, mono_data: [0; 32], mono_mask: [0; 32] };
        let plan = CursorPaint::resolve(&snapshot(color)).unwrap();
        assert_eq!(plan.hotspot, (1, 2)); assert_eq!(plan.pixels, vec![(0, 0, 0x12abcd)]);
        assert!(plan.clone().element((0, 0), (0., 0.), f32::NAN, (800, 600)).is_none());
        assert!(plan.element((0, 0), (0., 0.), 0., (800, 600)).is_none());
    }
    #[test]
    fn inversion_declines_the_whole_cursor_and_hidden_state_paints_nothing() {
        let mut data = [0; 32]; data[0] = 0xc0;
        let mut mask = [0; 32]; mask[0] = 0x80;
        let mut state = snapshot(CursorImage::mono(data, mask, 0, 0));
        assert_eq!(CursorPaint::resolve(&state), Err(UnsupportedCursor::Inversion));
        state.image = Some(CursorImage::Color { width: 0, height: 0, pixels_argb: vec![],
            mask: [0; 32], hot_v: 0, hot_h: 0, mono_data: [0; 32], mono_mask: [0; 32] });
        assert_eq!(CursorPaint::resolve(&state), Err(UnsupportedCursor::InvalidImage));
        state.visible = false; state.level = -1;
        assert!(CursorPaint::resolve(&state).unwrap().pixels.is_empty());
        state.visible = true;
        state.image = Some(CursorImage::Color { width: 1, height: 1, pixels_argb: vec![0xff000000],
            mask: [0; 32], hot_v: 0, hot_h: 0, mono_data: [0; 32], mono_mask: [0; 32] });
        assert_eq!(CursorPaint::resolve(&state), Err(UnsupportedCursor::Inversion));
    }
}
