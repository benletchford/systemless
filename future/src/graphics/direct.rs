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
    pub(crate) fn from_depth(depth: u16) -> Option<Self> {
        match depth {
            16 => Some(Self::Rgb555),
            32 => Some(Self::Xrgb8888),
            _ => None,
        }
    }
    pub(crate) fn pixel(self, rgb: [u16; 3]) -> u32 {
        let [r, g, b] = rgb.map(u32::from);
        match self {
            Self::Rgb555 => ((r >> 11) << 10) | ((g >> 11) << 5) | (b >> 11),
            Self::Xrgb8888 => ((r >> 8) << 16) | ((g >> 8) << 8) | (b >> 8),
        }
    }
    pub(crate) fn rgb(self, pixel: u32) -> [u16; 3] {
        match self {
            Self::Rgb555 => rgb555(pixel as u16),
            Self::Xrgb8888 => [16, 8, 0].map(|n| ((pixel >> n) & 255) as u16 * 257),
        }
    }
    pub(crate) fn mask(self) -> u32 {
        match self {
            Self::Rgb555 => 0x7fff,
            Self::Xrgb8888 => 0xffffff,
        }
    }
    pub(crate) fn unpack(self, memory: &Memory, bitmap: Bitmap) -> Result<Vec<u8>> {
        let (width, height) = bitmap.extent()?;
        let stride = self.bytes();
        if width * stride > bitmap.row_bytes || !bitmap.row_bytes.is_multiple_of(stride) {
            return Err(Error::Graphics("invalid direct pixel stride".into()));
        }
        let original = memory.read(bitmap.base, bitmap.byte_len()?)?;
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
        Ok(canonical)
    }
    pub(crate) fn pack(self, memory: &mut Memory, bitmap: Bitmap, canonical: &[u8]) -> Result<()> {
        let (width, height) = bitmap.extent()?;
        let stride = self.bytes();
        if width * stride > bitmap.row_bytes
            || !bitmap.row_bytes.is_multiple_of(stride)
            || canonical.len() != width * height * 4
        {
            return Err(Error::Graphics("invalid direct pixel storage".into()));
        }
        let length = bitmap.byte_len()?;
        memory.validate_write(bitmap.base, length)?;
        let mut result = memory.read(bitmap.base, length)?.to_vec();
        for y in 0..height {
            for x in 0..width {
                let at = y * bitmap.row_bytes + x * stride;
                let index = (y * width + x) * 4;
                let v = u32::from_be_bytes(canonical[index..index + 4].try_into().unwrap());
                if stride == 2 {
                    let value = u16::try_from(v)
                        .map_err(|_| Error::Graphics("invalid canonical RGB555 word".into()))?;
                    result[at..at + 2].copy_from_slice(&value.to_be_bytes());
                } else {
                    result[at..at + 4].copy_from_slice(&v.to_be_bytes());
                }
            }
        }
        memory.write(bitmap.base, &result)
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
        memory.validate_write(bitmap.base, bitmap.byte_len()?)?;
        let canonical = format.unpack(memory, bitmap)?;
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
        format.pack(memory, bitmap, output)
    }
}
