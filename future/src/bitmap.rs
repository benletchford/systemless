//! Basic QuickDraw BitMap layout and its packed-bit/GPU boundary.
//! Apple: Imaging With QuickDraw, Basic QuickDraw, BitMap record.
use crate::{
    graphics::{Draw, Graphics, Rect},
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
#[derive(Debug, Clone, Copy)]
pub struct Bitmap {
    pub base: GuestPtr,
    pub row_bytes: usize,
    pub bounds: Rect,
}
impl Bitmap {
    pub fn read(memory: &Memory, address: GuestPtr) -> Result<Self> {
        let bytes = memory.read(address, 14)?;
        let short = |i| i16::from_be_bytes(bytes[i..i + 2].try_into().unwrap()) as i32;
        let row_bytes = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
        let bitmap = Self {
            base: GuestPtr(u32::from_be_bytes(bytes[..4].try_into().unwrap())),
            row_bytes,
            bounds: Rect::new(short(8), short(6), short(12), short(10)),
        };
        bitmap.extent()?;
        memory.read(bitmap.base, bitmap.byte_len()?)?;
        Ok(bitmap)
    }
    pub fn extent(&self) -> Result<(usize, usize)> {
        let width = self
            .bounds
            .right
            .checked_sub(self.bounds.left)
            .filter(|v| *v > 0)
            .ok_or_else(|| Error::Graphics("empty/inverted bitmap width".into()))?
            as usize;
        let height = self
            .bounds
            .bottom
            .checked_sub(self.bounds.top)
            .filter(|v| *v > 0)
            .ok_or_else(|| Error::Graphics("empty/inverted bitmap height".into()))?
            as usize;
        if self.row_bytes == 0
            || self.row_bytes >= 0x4000
            || width > self.row_bytes * 8
            || width > 4096
            || height > 4096
        {
            return Err(Error::Graphics(
                "unsupported bitmap stride, flags or extent".into(),
            ));
        }
        Ok((width, height))
    }
    fn byte_len(&self) -> Result<usize> {
        Ok(self.row_bytes * self.extent()?.1)
    }
    fn unpack(&self, memory: &Memory) -> Result<Vec<u8>> {
        let (width, height) = self.extent()?;
        let bytes = memory.read(self.base, self.byte_len()?)?;
        let mut indices = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                indices.push((bytes[y * self.row_bytes + x / 8] >> (7 - x % 8)) & 1);
            }
        }
        Ok(indices)
    }
    fn pack(&self, memory: &mut Memory, indices: &[u8]) -> Result<()> {
        let (width, height) = self.extent()?;
        if indices.len() != width * height || indices.iter().any(|v| *v > 1) {
            return Err(Error::Graphics("non-monochrome bitmap result".into()));
        }
        // Retain bits outside logical bounds, including partial final bytes.
        let mut bytes = memory.read(self.base, self.byte_len()?)?.to_vec();
        for y in 0..height {
            for x in 0..width {
                let mask = 1u8 << (7 - x % 8);
                let byte = &mut bytes[y * self.row_bytes + x / 8];
                *byte = (*byte & !mask) | (indices[y * width + x] << (7 - x % 8));
            }
        }
        memory.write(self.base, &bytes)
    }
    /// Commands use bitmap-local QuickDraw coordinates. Temporary indices only
    /// encode storage; all pixel operations still execute through wgpu.
    pub fn execute(&self, gpu: &Graphics, memory: &mut Memory, draw: Draw) -> Result<()> {
        let (width, height) = self.extent()?;
        if (width, height) != (gpu.width as usize, gpu.height as usize) {
            return Err(Error::Graphics("bitmap/GPU extent mismatch".into()));
        }
        memory.validate_write(self.base, self.byte_len()?)?;
        let coordinate = |value: i32, origin: i32| {
            value
                .checked_sub(origin)
                .ok_or_else(|| Error::Graphics("bitmap coordinate overflow".into()))
        };
        let rect = |r: Rect| -> Result<Rect> {
            Ok(Rect::new(
                coordinate(r.left, self.bounds.left)?,
                coordinate(r.top, self.bounds.top)?,
                coordinate(r.right, self.bounds.left)?,
                coordinate(r.bottom, self.bounds.top)?,
            ))
        };
        let draw = match draw {
            Draw::Present => Draw::Present,
            Draw::Fill {
                rect: r,
                clip,
                pattern,
                foreground,
                background,
                mode,
            } => {
                if foreground > 1 || background > 1 {
                    return Err(Error::Graphics(
                        "monochrome colors must be bit values".into(),
                    ));
                }
                let pattern = std::array::from_fn(|y| {
                    pattern[((self.bounds.top + y as i32) & 7) as usize]
                        .rotate_left((self.bounds.left & 7) as u32)
                });
                Draw::Fill {
                    rect: rect(r)?,
                    clip: rect(clip)?,
                    pattern,
                    foreground,
                    background,
                    mode,
                }
            }
            Draw::Copy {
                rect: r,
                clip,
                source_x,
                source_y,
                mode,
            } => Draw::Copy {
                rect: rect(r)?,
                clip: rect(clip)?,
                source_x: coordinate(source_x, self.bounds.left)?,
                source_y: coordinate(source_y, self.bounds.top)?,
                mode,
            },
        };
        let mut staging = Memory::default();
        staging.map(gpu.base, self.unpack(memory)?, Permissions::RW)?;
        gpu.execute(&mut staging, draw)?;
        self.pack(memory, staging.read(gpu.base, width * height)?)
    }
}
