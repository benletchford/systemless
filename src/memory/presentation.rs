//! Opt-in 8-bit presentation experiment. Guest memory and text metrics stay unchanged.
//! Ordinary framebuffer writes replace enlarged pixels in drawing order; supported
//! outline glyphs blend into a separate RGB plane. Copies intentionally fall back
//! to guest pixels. This is a capture prototype, not a retained display backend.
use super::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::{urw, Glyph};
use std::collections::{HashMap, HashSet};

pub(crate) struct OutlineGlyph {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub left: i32,
    pub top: i32,
}

pub(crate) struct Presentation {
    base: u32,
    row_bytes: u32,
    width: u32,
    height: u32,
    pub scale: u32,
    palette: [[u8; 3]; 256],
    pixels: Vec<u8>,
    ink: HashMap<usize, (u8, u32, [u8; 3])>,
    run_ink: HashSet<usize>,
    in_text_run: bool,
    pub erasing_text: bool,
    glyph: Option<(OutlineGlyph, i16, i16)>,
    pub glyph_count: usize,
}

impl Presentation {
    fn position(&self, address: u32) -> Option<(u32, u32)> {
        let offset = address.checked_sub(self.base)?;
        let (x, y) = (offset % self.row_bytes, offset / self.row_bytes);
        (x < self.width && y < self.height).then_some((x, y))
    }

    pub fn glyph_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (g, h, v) = self.glyph.as_ref()?;
        let scale = self.scale as i32;
        Some((
            i32::from(*v) + g.top.div_euclid(scale),
            i32::from(*h) + g.left.div_euclid(scale),
            i32::from(*v) + (g.top + g.height + scale - 1).div_euclid(scale),
            i32::from(*h) + (g.left + g.width + scale - 1).div_euclid(scale),
        ))
    }

    pub fn write(&mut self, address: u32, value: u8) {
        if self.glyph.is_some() {
            return;
        }
        let Some((x, y)) = self.position(address) else {
            return;
        };
        let color = self.palette[value as usize];
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let offset =
                    (((y * self.scale + sy) * self.width * self.scale + x * self.scale + sx) * 3)
                        as usize;
                // A following character's opaque background must not shave off
                // an outline overhang already painted by this same text run.
                if self.erasing_text && self.run_ink.contains(&offset) {
                    continue;
                }
                self.run_ink.remove(&offset);
                self.ink.remove(&offset);
                self.pixels[offset..offset + 3].copy_from_slice(&color);
            }
        }
    }

    /// Called for every visible glyph cell, including cells with zero 1x ink.
    /// QuickDraw has already applied both the visibility and clipping regions.
    pub fn glyph_pixel(&mut self, address: u32, x: i16, y: i16, foreground: u8) {
        let Some((px, py)) = self.position(address) else {
            return;
        };
        let Some((glyph, h, v)) = &self.glyph else {
            return;
        };
        let color = self.palette[foreground as usize];
        for sy in 0..self.scale {
            for sx in 0..self.scale {
                let gx =
                    (i32::from(x) - i32::from(*h)) * self.scale as i32 + sx as i32 - glyph.left;
                let gy = (i32::from(y) - i32::from(*v)) * self.scale as i32 + sy as i32 - glyph.top;
                if gx < 0 || gy < 0 || gx >= glyph.width || gy >= glyph.height {
                    continue;
                }
                let alpha = u32::from(glyph.pixels[(gy * glyph.width + gx) as usize]);
                if alpha == 0 {
                    continue;
                }
                let offset =
                    (((py * self.scale + sy) * self.width * self.scale + px * self.scale + sx) * 3)
                        as usize;
                if self.in_text_run {
                    self.run_ink.insert(offset);
                }
                if alpha == 255 {
                    self.pixels[offset..offset + 3].copy_from_slice(&color);
                    self.ink.remove(&offset);
                    continue;
                }
                // Inside Macintosh I, "Transfer Modes": srcOr forces source
                // ink on and leaves other bits alone; repeated ink is idempotent.
                // Retain coverage rather than
                // repeatedly blending the same ink into its own antialiased edge.
                let background = [
                    self.pixels[offset],
                    self.pixels[offset + 1],
                    self.pixels[offset + 2],
                ];
                let ink = self
                    .ink
                    .entry(offset)
                    .or_insert((foreground, 0, background));
                if ink.0 != foreground {
                    *ink = (foreground, 0, background);
                }
                ink.1 = ink.1.max(alpha);
                let alpha = ink.1;
                for c in 0..3 {
                    self.pixels[offset + c] =
                        ((u32::from(color[c]) * alpha + u32::from(ink.2[c]) * (255 - alpha) + 127)
                            / 255) as u8;
                }
            }
        }
    }
}

impl MacMemoryBus {
    /// Start an experimental, fixed-palette 8-bit capture at 2x through 4x resolution.
    /// Enable after screen/palette setup. Does not enable high DPI in a frontend.
    pub fn enable_urw_presentation(
        &mut self,
        screen: (u32, u32, u16, u16, u16),
        palette: [[u8; 3]; 256],
        scale: u32,
    ) {
        let (base, row_bytes, width, height, depth) = screen;
        assert_eq!(
            depth, 8,
            "presentation experiment supports 8-bit screens only"
        );
        assert!((2..=4).contains(&scale));
        assert!(width > 0 && height > 0 && row_bytes >= u32::from(width));
        assert!(
            u64::from(base) + u64::from(row_bytes) * u64::from(height)
                <= u64::from(self.ram_size())
        );
        let mut presentation = Presentation {
            base,
            row_bytes,
            width: width.into(),
            height: height.into(),
            scale,
            palette,
            pixels: vec![0; width as usize * height as usize * scale as usize * scale as usize * 3],
            ink: HashMap::new(),
            run_ink: HashSet::new(),
            in_text_run: false,
            erasing_text: false,
            glyph: None,
            glyph_count: 0,
        };
        for y in 0..u32::from(height) {
            for x in 0..u32::from(width) {
                let address = base + y * row_bytes + x;
                presentation.write(address, self.read_byte(address));
            }
        }
        self.presentation = Some(presentation);
    }

    /// Return the real guest's presentation capture and number of outline draws.
    pub fn urw_presentation_rgb(&self) -> Option<(u32, u32, Vec<u8>, usize)> {
        let p = self.presentation.as_ref()?;
        Some((
            p.width * p.scale,
            p.height * p.scale,
            p.pixels.clone(),
            p.glyph_count,
        ))
    }

    pub(crate) fn begin_presentation_text_run(&mut self, opaque: bool) {
        if let Some(p) = &mut self.presentation {
            p.run_ink.clear();
            p.in_text_run = opaque;
        }
    }

    pub(crate) fn end_presentation_text_run(&mut self) {
        if let Some(p) = &mut self.presentation {
            p.run_ink.clear();
            p.in_text_run = false;
        }
    }

    pub(crate) fn begin_outline_glyph(
        &mut self,
        glyph: &Glyph,
        data: &[u8],
        x: i16,
        y: i16,
        bold: bool,
    ) {
        let Some(p) = &mut self.presentation else {
            return;
        };
        if let Some(mut outline) = urw::presentation_glyph(glyph, data, p.scale) {
            if bold && outline.width > 0 {
                let width = outline.width + p.scale as i32;
                let mut pixels = vec![0; (width * outline.height) as usize];
                for y in 0..outline.height {
                    for x in 0..outline.width {
                        let alpha = outline.pixels[(y * outline.width + x) as usize];
                        for shift in 0..=p.scale as i32 {
                            let dst = &mut pixels[(y * width + x + shift) as usize];
                            *dst = (*dst).max(alpha);
                        }
                    }
                }
                outline.width = width;
                outline.pixels = pixels;
            }
            p.glyph = Some((outline, x, y));
            p.glyph_count += 1;
        }
    }

    pub(crate) fn end_outline_glyph(&mut self) {
        if let Some(p) = &mut self.presentation {
            p.glyph = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> MacMemoryBus {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let palette = std::array::from_fn(|i| [i as u8; 3]);
        bus.fill_bytes(0x1000, 64, 255);
        bus.enable_urw_presentation((0x1000, 8, 8, 8, 8), palette, 2);
        bus
    }

    #[test]
    fn clipped_coverage_blends_over_background_and_later_writes_erase_it() {
        let mut bus = bus();
        let p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 8],
                width: 4,
                height: 2,
                left: 0,
                top: 0,
            },
            0,
            0,
        ));
        assert_eq!(p.glyph_bounds(), Some((0, 0, 1, 2)));
        // Only the first logical cell survives the caller's clipping region.
        p.glyph_pixel(0x1000, 0, 0, 0);
        p.glyph_pixel(0x1000, 0, 0, 0); // Repainting must not darken the edge.
        bus.write_byte(0x1000, 0); // Guest ink must not replace the blended plane.
        bus.end_outline_glyph();
        let (_, _, rgb, _) = bus.urw_presentation_rgb().unwrap();
        assert_eq!(&rgb[0..6], &[127; 6]);
        assert_eq!(&rgb[6..12], &[255; 6]);
        assert_eq!(bus.read_byte(0x1000), 0);
        bus.write_byte(0x1000, 0); // Even a same-value later write invalidates ink.
        assert_eq!(&bus.urw_presentation_rgb().unwrap().2[0..6], &[0; 6]);
    }

    #[test]
    fn text_run_overhang_survives_adjacent_erase_but_not_a_new_run() {
        let mut bus = bus();
        bus.begin_presentation_text_run(true);
        let p = bus.presentation.as_mut().unwrap();
        p.glyph = Some((
            OutlineGlyph {
                pixels: vec![128; 4],
                width: 2,
                height: 2,
                left: 2,
                top: 0,
            },
            0,
            0,
        ));
        p.glyph_pixel(0x1001, 1, 0, 0);
        bus.end_outline_glyph();
        bus.presentation.as_mut().unwrap().erasing_text = true;
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.urw_presentation_rgb().unwrap().2[6..12], &[127; 6]);
        assert_eq!(bus.read_byte(0x1001), 255);
        bus.end_presentation_text_run();
        bus.begin_presentation_text_run(true);
        bus.write_byte(0x1001, 255);
        assert_eq!(&bus.urw_presentation_rgb().unwrap().2[6..12], &[255; 6]);
    }

    #[test]
    fn four_times_capture_has_four_times_the_linear_resolution() {
        let mut bus = bus();
        bus.enable_urw_presentation((0x1000, 8, 8, 8, 8), [[255; 3]; 256], 4);
        let (w, h, pixels, _) = bus.urw_presentation_rgb().unwrap();
        assert_eq!((w, h, pixels.len()), (32, 32, 32 * 32 * 3));
    }

    #[test]
    fn bulk_writes_and_overlapping_copies_update_both_planes() {
        let mut bus = bus();
        bus.write_word(0x1000, 0x1020);
        bus.write_long(0x1002, 0x30405060);
        bus.write_bytes(0x1006, &[0x70, 0x80]);
        assert!(bus.copy_ram_bytes(0x1000, 0x1001, 7));
        let expected = [0x10, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
        assert_eq!(bus.read_bytes(0x1000, 8), expected);
        let rgb = bus.urw_presentation_rgb().unwrap().2;
        for (i, value) in expected.iter().enumerate() {
            assert_eq!(&rgb[i * 6..i * 6 + 6], &[*value; 6]);
        }
        bus.fill_bytes_strided(0x1000, 2, 4, 42);
        bus.fill_zeros(0x1008, 8);
        bus.fill_bytes(0x1010, 8, 99);
        let rgb = bus.urw_presentation_rgb().unwrap().2;
        assert_eq!(&rgb[..6], &[42; 6]);
        assert_eq!(&rgb[16 * 2 * 3..16 * 3 * 3], &[0; 48]);
        assert_eq!(&rgb[16 * 4 * 3..16 * 5 * 3], &[99; 48]);
        assert!(bus.fast_mem_window().is_none());
    }

    #[test]
    fn native_outline_capture_preserves_logical_font_metrics() {
        use crate::quickdraw::{fonts::FONT_GENEVA, text::get_glyph};
        let mut bus = bus();
        let (glyph, data) = get_glyph(FONT_GENEVA, 9, 'a').unwrap();
        let advance = glyph.advance;
        bus.begin_outline_glyph(glyph, data, 0, 7, false);
        let p = bus.presentation.as_ref().unwrap();
        let native = &p.glyph.as_ref().unwrap().0;
        assert!(native.pixels.iter().any(|&a| a > 0 && a < 255));
        assert!(native.height > i32::from(glyph.height));
        assert_eq!(get_glyph(FONT_GENEVA, 9, 'a').unwrap().0.advance, advance);
    }
}
