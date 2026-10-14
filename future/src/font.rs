//! Checked monochrome FONT/NFNT strike layout, independent of font selection.
//! Apple Inside Macintosh: Text, FontRec and bitmapped font resource.
use crate::{Error, Result};
fn bad(message: &str) -> Error {
    Error::Load(message.into())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub ascent: i16,
    pub descent: i16,
    pub maximum_width: i16,
    pub leading: i16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub start: u16,
    pub width: u16,
    pub height: u16,
    pub bearing: i32,
    pub top: i32,
    pub advance: u8,
}
pub struct Strike<'a> {
    bytes: &'a [u8],
    first: u16,
    last: u16,
    locations: usize,
    widths: usize,
    row_bytes: usize,
    height: u16,
    kern: i16,
    pub metrics: Metrics,
}
impl<'a> Strike<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < 26 || bytes.len() > 16 * 1024 * 1024 {
            return Err(bad("invalid font resource size"));
        }
        let word = |offset| u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        let flags = word(0);
        if flags & 0x038f != 0 {
            return Err(bad("color or auxiliary-table bitmap font is unsupported"));
        }
        let first = word(2);
        let last = word(4);
        let height = word(14);
        let rows = word(24);
        if first > last || last > 255 || height == 0 || height > 4096 || rows == 0 || rows > 4096 {
            return Err(bad("invalid bitmap font dimensions"));
        }
        let row_bytes = usize::from(rows) * 2;
        let locations = 26 + row_bytes * usize::from(height);
        let count = usize::from(last - first) + 3;
        let location_end = locations + count * 2;
        let high = if word(10) as i16 > 0 {
            u32::from(word(10)) << 16
        } else {
            0
        };
        let widths = usize::try_from(16u64 + u64::from(high | u32::from(word(16))) * 2)
            .map_err(|_| bad("font table offset exceeds host address range"))?;
        if location_end > bytes.len()
            || widths < location_end
            || widths
                .checked_add(count * 2)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(bad("truncated font tables or invalid width-table offset"));
        }
        let mut previous = 0;
        for i in 0..count {
            let x = word(locations + i * 2);
            if x < previous || usize::from(x) > row_bytes * 8 {
                return Err(bad("invalid font glyph location"));
            }
            previous = x;
        }
        if word(widths + (count - 1) * 2) != 0xffff || word(widths + (count - 2) * 2) == 0xffff {
            return Err(bad("invalid missing glyph or width-table terminator"));
        }
        let metrics = Metrics {
            ascent: word(18) as i16,
            descent: word(20) as i16,
            maximum_width: word(6) as i16,
            leading: word(22) as i16,
        };
        if metrics.ascent < 0 || metrics.descent < 0 || metrics.maximum_width < 0 {
            return Err(bad("invalid font metrics"));
        }
        Ok(Self {
            bytes,
            first,
            last,
            locations,
            widths,
            row_bytes,
            height,
            kern: word(8) as i16,
            metrics,
        })
    }
    fn word(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.bytes[offset], self.bytes[offset + 1]])
    }
    pub fn glyph(&self, character: u8) -> Glyph {
        let missing = usize::from(self.last - self.first) + 1;
        let mut index = if u16::from(character) >= self.first && u16::from(character) <= self.last {
            usize::from(u16::from(character) - self.first)
        } else {
            missing
        };
        if self.word(self.widths + index * 2) == 0xffff {
            index = missing;
        }
        let widths = self.word(self.widths + index * 2);
        let start = self.word(self.locations + index * 2);
        let end = self.word(self.locations + (index + 1) * 2);
        Glyph {
            start,
            width: end - start,
            height: self.height,
            bearing: i32::from(self.kern) + i32::from((widths >> 8) as u8),
            top: -i32::from(self.metrics.ascent),
            advance: widths as u8,
        }
    }
    /// Decode only guest bitmap storage. These mask bits are intended as input
    /// to GPU glyph operations; this method does not draw destination pixels.
    pub fn mask(&self, character: u8) -> Result<Vec<u8>> {
        let glyph = self.glyph(character);
        let size = usize::from(glyph.width) * usize::from(glyph.height);
        if size > 16 * 1024 * 1024 {
            return Err(bad("font glyph mask limit exceeded"));
        }
        let mut mask = Vec::with_capacity(size);
        for row in 0..usize::from(glyph.height) {
            for x in usize::from(glyph.start)..usize::from(glyph.start + glyph.width) {
                mask.push((self.bytes[26 + row * self.row_bytes + x / 8] >> (7 - x % 8)) & 1);
            }
        }
        Ok(mask)
    }
    pub fn width(&self, text: &[u8]) -> Result<i32> {
        text.iter().try_fold(0i32, |sum, c| {
            sum.checked_add(i32::from(self.glyph(*c).advance))
                .ok_or_else(|| bad("font measurement overflow"))
        })
    }
}

/// Font Manager owns only guest handles and initialization state. Fallback
/// strikes remain editable guest bytes, not cached host metrics or glyphs.
#[derive(Default)]
pub struct Manager {
    pub initialized: bool,
    pub(crate) fallback: std::collections::BTreeMap<i16, u32>,
}
pub fn initialize(
    memory: &mut crate::memory::Memory,
    process: &mut crate::process::Process,
) -> Result<()> {
    if process.fonts.initialized {
        return Ok(());
    }
    let size = memory.u16(crate::memory::GuestPtr(0xba8))? as i16;
    let size = if size == 0 { 12 } else { size };
    let family = memory.u16(crate::memory::GuestPtr(0xba6))? as i16;
    let id = i16::try_from(i32::from(family) * 128 + i32::from(size))
        .map_err(|_| bad("system FONT resource ID overflow"))?;
    if process.resources.get(*b"FONT", id).is_some() {
        let handle = process.get_resource(memory, *b"FONT", id)?;
        let length = process.handle_size(memory, handle)? as usize;
        let data = crate::memory::GuestPtr(memory.u32(crate::memory::GuestPtr(handle))?);
        Strike::parse(memory.read(data, length)?)?;
    } else {
        if !matches!(family, 0 | 3 | 21) {
            return Err(bad("system font has no resource or fallback"));
        }
        fallback(memory, process, size)?;
    }
    if memory.u16(crate::memory::GuestPtr(0xba8))? == 0 {
        memory.write(crate::memory::GuestPtr(0xba8), &12u16.to_be_bytes())?;
    }
    if memory.u16(crate::memory::GuestPtr(0x984))? == 0 {
        memory.write(crate::memory::GuestPtr(0x984), &3u16.to_be_bytes())?;
    }
    process.fonts.initialized = true;
    Ok(())
}
pub(crate) fn fallback(
    memory: &mut crate::memory::Memory,
    process: &mut crate::process::Process,
    size: i16,
) -> Result<u32> {
    if let Some(handle) = process.fonts.fallback.get(&size) {
        return Ok(*handle);
    }
    if process.fonts.fallback.len() >= 32 {
        return Err(bad("fallback font strike count limit exceeded"));
    }
    let bytes = crate::font_fallback::strike(size)?;
    let handle = process.new_handle(memory, bytes.len() as u32, false)?;
    if handle == 0 {
        return Err(bad("fallback font heap allocation failed"));
    }
    let data = crate::memory::GuestPtr(memory.u32(crate::memory::GuestPtr(handle))?);
    memory.write(data, &bytes)?;
    process.fonts.fallback.insert(size, handle);
    Ok(handle)
}
