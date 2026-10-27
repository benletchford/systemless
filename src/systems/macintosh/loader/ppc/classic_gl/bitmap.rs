//! Bitmap unpacking and raster state (OpenGL 1.2.1, sections 2.12 and 3.7).

use super::args::PpcCallArgs;
use super::consts::{INVALID_VALUE, OUT_OF_MEMORY};
use super::state::TEXTURE_UNITS;
use super::texture::PixelStore;
use ppc::PpcMemory;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub origin: [f32; 2],
    pub advance: [f32; 2],
    /// One byte per bit, bottom row first. Owned when a display list is compiled.
    pub pixels: Rc<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct RasterPosition {
    pub window: [f32; 4],
    pub valid: bool,
    pub color: [f32; 4],
    pub tex: [[f32; 4]; TEXTURE_UNITS],
    pub distance: f32,
}

impl Default for RasterPosition {
    fn default() -> Self {
        Self {
            window: [0.0, 0.0, 0.0, 1.0],
            valid: true,
            color: [1.0; 4],
            tex: [[0.0, 0.0, 0.0, 1.0]; TEXTURE_UNITS],
            distance: 0.0,
        }
    }
}

pub(super) fn decode(args: &mut PpcCallArgs, store: &PixelStore) -> Result<Bitmap, u32> {
    let width = args.i32();
    let height = args.i32();
    let origin = [args.f32(), args.f32()];
    let advance = [args.f32(), args.f32()];
    let pointer = args.u32();
    if width < 0 || height < 0 {
        return Err(INVALID_VALUE);
    }
    let (width, height) = (width as usize, height as usize);
    let length = width
        .checked_mul(height)
        .filter(|&n| n <= 1 << 24)
        .ok_or(OUT_OF_MEMORY)?;
    let row_bits = if store.row_length > 0 {
        store.row_length as usize
    } else {
        width
    };
    let alignment = store.alignment as usize;
    let stride = row_bits.div_ceil(8).div_ceil(alignment) * alignment;
    let mut pixels = vec![0; length];
    for row in 0..height {
        let base = (row + store.skip_rows as usize)
            .checked_mul(stride)
            .ok_or(INVALID_VALUE)?;
        for col in 0..width {
            let bit = col + store.skip_pixels as usize;
            let offset = base
                .checked_add(bit / 8)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(INVALID_VALUE)?;
            let address = pointer.checked_add(offset).ok_or(INVALID_VALUE)?;
            let byte = args.memory().read_u8(address).ok_or(INVALID_VALUE)?;
            let shift = if store.lsb_first {
                bit % 8
            } else {
                7 - bit % 8
            };
            pixels[row * width + col] = (byte >> shift) & 1;
        }
    }
    Ok(Bitmap {
        width,
        height,
        origin,
        advance,
        pixels: Rc::new(pixels),
    })
}

impl super::GlContext {
    /// AGL creates sequential bitmap lists, independent of pixel-unpack state.
    /// Apple's AGL Reference, aglUseFont (font family/size/style are QuickDraw values).
    pub fn use_font(
        &mut self,
        font: i16,
        style: u8,
        size: i32,
        first: i32,
        count: i32,
        base: u32,
    ) -> Result<(), u32> {
        use super::super::classic_gl_agl::{AGL_BAD_ALLOC, AGL_BAD_STATE, AGL_BAD_VALUE};
        if self.begin.is_some() || self.compiling.is_some() {
            return Err(AGL_BAD_STATE);
        }
        if size < 0
            || first < 0
            || count < 0
            || base == 0
            || first.checked_add(count).is_none()
            || base.checked_add(count as u32).is_none()
        {
            return Err(AGL_BAD_VALUE);
        }
        // Bound guest-driven host allocations without publishing partial lists.
        if count > 65536 || size > 256 {
            return Err(AGL_BAD_ALLOC);
        }
        let mut lists = Vec::with_capacity(count as usize);
        for index in 0..count {
            let bitmap = font_bitmap(font, style, size as i16, first + index);
            lists.push((
                base + index as u32,
                Rc::new(vec![super::GlCall::Bitmap(bitmap)]),
            ));
        }
        self.lists.extend(lists);
        Ok(())
    }
}

fn font_bitmap(font: i16, style: u8, size: i16, code: i32) -> Bitmap {
    use crate::quickdraw::fonts::{get_font_face_scale_ratio, style::get_italic_slant};
    use crate::quickdraw::text::{get_glyph, get_glyph_italic, QuickDrawTextStyle};
    use std::collections::HashSet;
    let empty = || Bitmap {
        width: 0,
        height: 0,
        origin: [0.0; 2],
        advance: [0.0; 2],
        pixels: Rc::new(Vec::new()),
    };
    if !(0..=255).contains(&code) {
        return empty();
    }
    let style = QuickDrawTextStyle::from_bits(style);
    let (face, numerator, denominator) = get_font_face_scale_ratio(font, size);
    let ch = char::from(code as u8);
    let italic = style
        .italic()
        .then(|| get_glyph_italic(font, face.size, ch))
        .flatten();
    let synthetic_italic = style.italic() && italic.is_none();
    let Some((glyph, data)) = italic.or_else(|| get_glyph(font, face.size, ch)) else {
        return empty();
    };
    let mut ink = HashSet::new();
    for row in 0..i32::from(glyph.height) {
        let y = i32::from(glyph.origin_y) + row;
        let slant = if synthetic_italic {
            i32::from(get_italic_slant(
                font,
                face.size,
                &face.metrics,
                0,
                y as i16,
            ))
        } else {
            0
        };
        for col in 0..i32::from(glyph.width) {
            let index = glyph.data_offset + row as usize * usize::from(glyph.width) + col as usize;
            if data.get(index).copied().unwrap_or(0) < 128 {
                continue;
            }
            let x = i32::from(glyph.origin_x) + col + slant;
            ink.insert((x, y));
            if style.bold() {
                ink.insert((x + 1, y));
            }
        }
    }
    let advance = style.glyph_advance(i32::from(glyph.advance));
    if style.underline() {
        for x in 0..advance {
            ink.insert((x, 1));
        }
    }
    if let Some(radius) = style.smear_max() {
        let mut outline = HashSet::new();
        for &(x, y) in &ink {
            for dy in -1..=radius {
                for dx in -1..=radius {
                    let point = (x + dx, y + dy);
                    if !ink.contains(&point) {
                        outline.insert(point);
                    }
                }
            }
        }
        ink = outline;
    }
    let scale = |v: i32| (v * numerator).div_euclid(denominator);
    let mut scaled = HashSet::new();
    for (x, y) in ink {
        for yy in scale(y)..scale(y + 1).max(scale(y) + 1) {
            for xx in scale(x)..scale(x + 1).max(scale(x) + 1) {
                scaled.insert((xx, -yy - 1));
            }
        }
    }
    let min_x = scaled.iter().map(|p| p.0).min().unwrap_or(0);
    let min_y = scaled.iter().map(|p| p.1).min().unwrap_or(0);
    let width = scaled
        .iter()
        .map(|p| p.0)
        .max()
        .map_or(0, |x| (x - min_x + 1) as usize);
    let height = scaled
        .iter()
        .map(|p| p.1)
        .max()
        .map_or(0, |y| (y - min_y + 1) as usize);
    let mut pixels = vec![0; width * height];
    for (x, y) in scaled {
        pixels[(y - min_y) as usize * width + (x - min_x) as usize] = 1;
    }
    Bitmap {
        width,
        height,
        origin: [-min_x as f32, -min_y as f32],
        advance: [
            ((advance * numerator + denominator / 2) / denominator) as f32,
            0.0,
        ],
        pixels: Rc::new(pixels),
    }
}
