//! Complete owner-side indexed image plus a small, fully composed cursor patch.
//! The patch uses the scalar cursor renderer, including color-cursor inversion.
//! No renderer keeps a view into mutable guest RAM.

use systemless::display::{self, CursorImage, PackedScreenFrame};
use systemless::memory::MacMemoryBus;

type ScreenMode = (u32, u32, u16, u16, u16);

#[derive(Default)]
pub(crate) struct IndexedFrame {
    pub screen: PackedScreenFrame,
    pub palette: Vec<u8>,
    pub cursor: Option<CursorPatch>,
}

pub(crate) struct CursorPatch {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl IndexedFrame {
    pub fn capture(
        &mut self,
        bus: &MacMemoryBus,
        mode: ScreenMode,
        clut: &[[u16; 3]; 256],
        cursor: Option<&CursorImage>,
        mouse: (i16, i16),
    ) -> bool {
        if mode.4 != 8
            || mode.2 == 0
            || mode.3 == 0
            || mode.1 > 8192
            || mode.2 > 8192
            || mode.3 > 8192
            || u64::from(mode.1) * u64::from(mode.3) > 16 * 1024 * 1024
        {
            return false;
        }
        // Match the browser's existing render_rgba palette/gamma policy.
        self.screen
            .capture(bus, mode, clut, &display::default_display_gamma());
        self.prepare(cursor, mouse)
    }

    fn prepare(&mut self, cursor: Option<&CursorImage>, mouse: (i16, i16)) -> bool {
        let (_, stride, width, height, depth) = self.screen.screen_mode;
        if depth != 8
            || stride < u32::from(width)
            || self.screen.pixels.len() != stride as usize * height as usize
        {
            return false;
        }
        self.palette.clear();
        for argb in self.screen.palette {
            self.palette.extend_from_slice(&[
                (argb >> 16) as u8,
                (argb >> 8) as u8,
                argb as u8,
                (argb >> 24) as u8,
            ]);
        }
        let previous = self.cursor.take();
        let Some(cursor) = cursor else {
            return true;
        };
        let (cursor_w, cursor_h, hot_v, hot_h) = match cursor {
            CursorImage::Mono { hot_v, hot_h, .. } => (16, 16, *hot_v, *hot_h),
            CursorImage::Color {
                width,
                height,
                hot_v,
                hot_h,
                ..
            } => (i32::from(*width), i32::from(*height), *hot_v, *hot_h),
        };
        let origin_x = i32::from(mouse.1) - i32::from(hot_h);
        let origin_y = i32::from(mouse.0) - i32::from(hot_v);
        let x = origin_x.clamp(0, i32::from(width));
        let y = origin_y.clamp(0, i32::from(height));
        let right = (origin_x + cursor_w).clamp(0, i32::from(width));
        let bottom = (origin_y + cursor_h).clamp(0, i32::from(height));
        if right <= x || bottom <= y {
            return true;
        }
        let patch_w = (right - x) as u32;
        let patch_h = (bottom - y) as u32;
        // Large/unsupported cursor work stays on the full scalar image path.
        if patch_w * patch_h > 4096 {
            return false;
        }
        let Ok(local_v) = i16::try_from(i32::from(mouse.0) - y) else {
            return false;
        };
        let Ok(local_h) = i16::try_from(i32::from(mouse.1) - x) else {
            return false;
        };
        let mut pixels = previous.map(|patch| patch.pixels).unwrap_or_default();
        pixels.resize((patch_w * patch_h * 4) as usize, 0);
        for row in 0..patch_h as usize {
            for col in 0..patch_w as usize {
                let index = self.screen.pixels
                    [(y as usize + row) * stride as usize + x as usize + col]
                    as usize;
                let target = (row * patch_w as usize + col) * 4;
                pixels[target..target + 4].copy_from_slice(&self.palette[index * 4..index * 4 + 4]);
            }
        }
        display::render_cursor(&mut pixels, patch_w, patch_h, cursor, (local_v, local_h));
        self.cursor = Some(CursorPatch {
            x: x as u32,
            y: y as u32,
            width: patch_w,
            height: patch_h,
            pixels,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> IndexedFrame {
        let mut frame = IndexedFrame::default();
        frame.screen.screen_mode = (0, 40, 37, 29, 8);
        frame.screen.pixels = (0..40 * 29).map(|i| (i * 71) as u8).collect();
        for (index, color) in frame.screen.palette.iter_mut().enumerate() {
            *color = 0xff000000
                | (index as u32) << 16
                | ((index * 37) as u32 & 255) << 8
                | (255 - index) as u32;
        }
        frame
    }

    fn scalar(frame: &IndexedFrame, cursor: Option<&CursorImage>, mouse: (i16, i16)) -> Vec<u8> {
        let mut argb = Vec::new();
        frame.screen.render_argb(&mut argb);
        let mut rgba = Vec::new();
        for value in argb {
            rgba.extend_from_slice(&[
                (value >> 16) as u8,
                (value >> 8) as u8,
                value as u8,
                (value >> 24) as u8,
            ]);
        }
        if let Some(cursor) = cursor {
            display::render_cursor(&mut rgba, 37, 29, cursor, mouse);
        }
        rgba
    }

    #[test]
    fn cursor_patches_match_full_scalar_composition_at_every_edge() {
        let mono = CursorImage::mono([0xa5; 32], [0x5a; 32], 7, 3);
        let color = CursorImage::Color {
            width: 19,
            height: 18,
            pixels_argb: (0..19 * 18)
                .map(|i| if i % 3 == 0 { 0xff000000 } else { 0xff12ab56 })
                .collect(),
            mask: [0x5a; 32],
            hot_v: 8,
            hot_h: 6,
            mono_data: [0; 32],
            mono_mask: [0; 32],
        };
        for cursor in [&mono, &color] {
            for v in [-30, -1, 0, 1, 14, 28, 29, 40, i16::MIN, i16::MAX] {
                for h in [-30, -1, 0, 1, 18, 36, 37, 50, i16::MIN, i16::MAX] {
                    let mut frame = frame();
                    let expected = scalar(&frame, Some(cursor), (v, h));
                    assert!(frame.prepare(Some(cursor), (v, h)));
                    let mut actual = scalar(&frame, None, (0, 0));
                    if let Some(patch) = &frame.cursor {
                        for row in 0..patch.height as usize {
                            let target = ((patch.y as usize + row) * 37 + patch.x as usize) * 4;
                            let source = row * patch.width as usize * 4;
                            actual[target..target + patch.width as usize * 4].copy_from_slice(
                                &patch.pixels[source..source + patch.width as usize * 4],
                            );
                        }
                    }
                    assert_eq!(actual, expected, "cursor at {v},{h}");
                }
            }
        }
    }

    #[test]
    fn palette_and_cursor_removal_replace_the_complete_snapshot() {
        let mut frame = frame();
        let cursor = CursorImage::mono([0; 32], [255; 32], 0, 0);
        assert!(frame.prepare(Some(&cursor), (0, 0)));
        assert!(frame.cursor.is_some());
        frame.screen.palette[0] = 0xff123456;
        assert!(frame.prepare(None, (0, 0)));
        assert!(frame.cursor.is_none());
        assert_eq!(&frame.palette[..4], &[0x12, 0x34, 0x56, 0xff]);
        frame.screen.pixels.pop();
        assert!(!frame.prepare(None, (0, 0)));
    }
}
