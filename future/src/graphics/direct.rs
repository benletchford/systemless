//! Classic big-endian RGB555 and xRGB8888 storage around the shared GPU commands.
use super::*;
use crate::{bitmap::Bitmap, memory::Permissions};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectFormat {
    Rgb555,
    Xrgb8888,
}
impl DirectFormat {
    pub fn depth(self) -> u16 {
        match self {
            Self::Rgb555 => 16,
            Self::Xrgb8888 => 32,
        }
    }
    fn bytes(self) -> usize {
        usize::from(self.depth() / 8)
    }
}
/// Expand stored components by repeating their five bits, shared with PICT input.
pub(crate) fn rgb555(pixel: u16) -> [u16; 3] {
    [10, 5, 0].map(|shift| {
        let v = (pixel >> shift) & 31;
        (v << 11) | (v << 6) | (v << 1) | (v >> 4)
    })
}

impl Graphics {
    /// Draw coordinates are bitmap-local, as for the underlying Graphics API.
    /// CPU work only encodes storage. Pixel operations use the existing shader.
    pub fn execute_direct(
        &self,
        memory: &mut Memory,
        bitmap: Bitmap,
        format: DirectFormat,
        draw: Draw,
        clips: Option<&[Rect]>,
        rgb: Option<&[[u16; 3]]>,
    ) -> Result<()> {
        let (width, height) = bitmap.extent()?;
        let stride = format.bytes();
        if width * stride > bitmap.row_bytes || !bitmap.row_bytes.is_multiple_of(stride) {
            return Err(Error::Graphics("invalid direct pixel stride".into()));
        }
        let length = bitmap.byte_len()?;
        memory.validate_write(bitmap.base, length)?;
        let original = memory.read(bitmap.base, length)?.to_vec();
        let mut canonical = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let at = y * bitmap.row_bytes + x * stride;
                let v = if stride == 2 {
                    u32::from(u16::from_be_bytes(original[at..at + 2].try_into().unwrap()))
                } else {
                    u32::from_be_bytes(original[at..at + 4].try_into().unwrap())
                };
                canonical.extend(v.to_be_bytes());
            }
        }
        if (width, height) != (self.width as usize, self.height as usize) {
            return self
                .target(width as u32, height as u32)?
                .execute_direct(memory, bitmap, format, draw, clips, rgb);
        }
        let target = self;
        let mut staging = Memory::default();
        staging.map(target.base, canonical, Permissions::RW)?;
        let colors = [[0; 3]; 256];
        let eligible = [true; 256];
        let config = Some(IndexedTarget {
            depth: format.depth(),
            rgb: &colors,
            matchable: &eligible,
            direct_source: false,
            neutral_copy: false,
        });
        target.execute_source_input(
            &mut staging,
            draw,
            clips,
            rgb.map(CopySource::Rgb),
            None,
            config,
        )?;
        let output = staging.read(target.base, width * height * 4)?;
        let mut result = original;
        for y in 0..height {
            for x in 0..width {
                let at = y * bitmap.row_bytes + x * stride;
                let index = (y * width + x) * 4;
                let v = u32::from_be_bytes(output[index..index + 4].try_into().unwrap());
                if stride == 2 {
                    result[at..at + 2].copy_from_slice(&(v as u16).to_be_bytes());
                } else {
                    result[at..at + 4].copy_from_slice(&v.to_be_bytes());
                }
            }
        }
        memory.write(bitmap.base, &result)
    }
}
