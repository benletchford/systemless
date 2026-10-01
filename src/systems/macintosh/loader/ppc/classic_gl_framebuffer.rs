//! Pixel storage for classic OpenGL drawables. Coordinates at this boundary use
//! OpenGL's lower-left origin; QuickDraw's guest framebuffer uses the upper-left.

use super::{PpcFrontBuffer, PpcSectionMem};

const MAX_DRAWABLE_PIXELS: usize = 4096 * 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassicGlColorBuffer {
    Front,
    Back,
}

#[derive(Debug, Clone)]
pub struct ClassicGlFramebuffer {
    width: usize,
    height: usize,
    front: Vec<[u8; 4]>,
    back: Option<Vec<[u8; 4]>>,
    depth: Vec<f32>,
    stencil: Vec<u8>,
}

impl ClassicGlFramebuffer {
    pub fn new(width: u32, height: u32, double_buffered: bool) -> Option<Self> {
        let (width, height) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
        let len = width.checked_mul(height)?;
        if len == 0 || len > MAX_DRAWABLE_PIXELS {
            return None;
        }
        let color = vec![[0, 0, 0, 0]; len];
        Some(Self {
            width,
            height,
            front: color.clone(),
            back: double_buffered.then_some(color),
            depth: vec![1.0; len],
            stencil: vec![0; len],
        })
    }

    fn color(&self, buffer: ClassicGlColorBuffer) -> Option<&[[u8; 4]]> {
        match buffer {
            ClassicGlColorBuffer::Front => Some(&self.front),
            ClassicGlColorBuffer::Back => self.back.as_deref(),
        }
    }

    fn color_mut(&mut self, buffer: ClassicGlColorBuffer) -> Option<&mut [[u8; 4]]> {
        match buffer {
            ClassicGlColorBuffer::Front => Some(&mut self.front),
            ClassicGlColorBuffer::Back => self.back.as_deref_mut(),
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        (x < self.width && y < self.height).then(|| y * self.width + x)
    }

    pub fn clear_color(&mut self, buffer: ClassicGlColorBuffer, rgba: [u8; 4]) -> bool {
        let Some(pixels) = self.color_mut(buffer) else {
            return false;
        };
        pixels.fill(rgba);
        true
    }

    pub fn clear_depth(&mut self, value: f32) {
        self.depth.fill(value.clamp(0.0, 1.0));
    }

    pub fn clear_stencil(&mut self, value: u8) {
        self.stencil.fill(value);
    }

    pub fn pixel(&self, buffer: ClassicGlColorBuffer, x: u32, y: u32) -> Option<[u8; 4]> {
        self.color(buffer)?.get(self.index(x, y)?).copied()
    }

    pub fn set_pixel(
        &mut self,
        buffer: ClassicGlColorBuffer,
        x: u32,
        y: u32,
        rgba: [u8; 4],
    ) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        let Some(pixel) = self
            .color_mut(buffer)
            .and_then(|pixels| pixels.get_mut(index))
        else {
            return false;
        };
        *pixel = rgba;
        true
    }

    /// Copies the entire pixel rectangle in OpenGL's bottom-up row order.
    pub fn read_rgba(
        &self,
        buffer: ClassicGlColorBuffer,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let end_x = x.checked_add(width)?;
        let end_y = y.checked_add(height)?;
        if usize::try_from(end_x).ok()? > self.width || usize::try_from(end_y).ok()? > self.height {
            return None;
        }
        let len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
        let mut result = Vec::with_capacity(len);
        for row in y..end_y {
            for col in x..end_x {
                result.extend_from_slice(&self.pixel(buffer, col, row)?);
            }
        }
        Some(result)
    }

    /// Presents a completed color buffer to a 16-bit QuickDraw RGB555 surface.
    /// The guest rows are top-down and big-endian.
    pub fn present_rgb555(
        &self,
        buffer: ClassicGlColorBuffer,
        memory: &mut PpcSectionMem,
        surface: PpcFrontBuffer,
    ) -> bool {
        if surface.depth != 16
            || usize::try_from(surface.width).ok() != Some(self.width)
            || usize::try_from(surface.height).ok() != Some(self.height)
            || surface.row_bytes < surface.width.saturating_mul(2)
        {
            return false;
        }
        let Some(pixels) = self.color(buffer) else {
            return false;
        };
        let mut row = vec![0; self.width * 2];
        for guest_y in 0..self.height {
            let source_y = self.height - guest_y - 1;
            for x in 0..self.width {
                let [r, g, b, _] = pixels[source_y * self.width + x];
                let rgb555 =
                    (u16::from(r >> 3) << 10) | (u16::from(g >> 3) << 5) | u16::from(b >> 3);
                row[x * 2..x * 2 + 2].copy_from_slice(&rgb555.to_be_bytes());
            }
            let Some(address) = u32::try_from(guest_y)
                .ok()
                .and_then(|y| y.checked_mul(surface.row_bytes))
                .and_then(|offset| surface.base_addr.checked_add(offset))
            else {
                return false;
            };
            if memory.write_bytes(address, &row).is_none() {
                return false;
            }
        }
        true
    }

    pub fn swap(&mut self) -> bool {
        let Some(back) = self.back.as_mut() else {
            return false;
        };
        std::mem::swap(&mut self.front, back);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::PpcMemory;

    #[test]
    fn back_buffer_clear_readback_and_swap() {
        let mut frame = ClassicGlFramebuffer::new(2, 2, true).unwrap();
        assert!(frame.clear_color(ClassicGlColorBuffer::Back, [255, 32, 0, 255]));
        assert_eq!(
            frame.read_rgba(ClassicGlColorBuffer::Back, 0, 0, 2, 1),
            Some(vec![255, 32, 0, 255, 255, 32, 0, 255])
        );
        assert_eq!(frame.pixel(ClassicGlColorBuffer::Front, 0, 0), Some([0; 4]));
        assert!(frame.swap());
        assert_eq!(
            frame.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 32, 0, 255])
        );
    }

    #[test]
    fn presenting_flips_rows_and_writes_big_endian_rgb555() {
        let mut frame = ClassicGlFramebuffer::new(1, 2, false).unwrap();
        assert!(frame.set_pixel(ClassicGlColorBuffer::Front, 0, 0, [255, 0, 0, 255]));
        assert!(frame.set_pixel(ClassicGlColorBuffer::Front, 0, 1, [0, 255, 0, 255]));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 4]);
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 2,
            width: 1,
            height: 2,
            depth: 16,
        };
        assert!(frame.present_rgb555(ClassicGlColorBuffer::Front, &mut memory, surface));
        assert_eq!(memory.read_u16_be(0x1000), Some(0x03e0));
        assert_eq!(memory.read_u16_be(0x1002), Some(0x7c00));
    }

    #[test]
    fn readback_uses_lower_left_origin_and_checks_bounds() {
        let mut frame = ClassicGlFramebuffer::new(2, 2, false).unwrap();
        assert!(frame.set_pixel(ClassicGlColorBuffer::Front, 0, 0, [1, 2, 3, 4]));
        assert!(frame.set_pixel(ClassicGlColorBuffer::Front, 0, 1, [5, 6, 7, 8]));
        assert_eq!(
            frame.read_rgba(ClassicGlColorBuffer::Front, 0, 0, 1, 2),
            Some(vec![1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(
            frame.read_rgba(ClassicGlColorBuffer::Front, 1, 1, 2, 1),
            None
        );
        assert!(!frame.swap());
        assert!(ClassicGlFramebuffer::new(4097, 4097, true).is_none());
    }
}
