//! Host-selected main display policy. Guest pixels remain canonical storage.
use crate::{
    bitmap::Bitmap,
    graphics::Rect,
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    width: u16,
    height: u16,
    depth: u16,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            width: 512,
            height: 342,
            depth: 1,
        }
    }
}
impl Display {
    /// An explicit host RGB332 policy, not a ROM/default Macintosh CLUT claim.
    pub fn indexed8(width: u16, height: u16) -> Result<Self> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(Error::Graphics("invalid display dimensions".into()));
        }
        Ok(Self {
            width,
            height,
            depth: 8,
        })
    }
    pub fn width(self) -> u32 {
        u32::from(self.width)
    }
    pub fn height(self) -> u32 {
        u32::from(self.height)
    }
    pub fn depth(self) -> u16 {
        self.depth
    }
    pub(crate) fn bitmap(self) -> Bitmap {
        Bitmap {
            base: crate::quickdraw::SCREEN,
            row_bytes: (usize::from(self.width) * usize::from(self.depth)).div_ceil(16) * 2,
            bounds: Rect::new(0, 0, i32::from(self.width), i32::from(self.height)),
        }
    }
    pub(crate) fn colors(self) -> Vec<[u16; 3]> {
        if self.depth == 1 {
            return vec![[65535; 3], [0; 3]];
        }
        (0..256u32)
            .map(|i| {
                [
                    ((7 - (i >> 5)) * 65535 / 7) as u16,
                    ((7 - ((i >> 2) & 7)) * 65535 / 7) as u16,
                    ((3 - (i & 3)) * 65535 / 3) as u16,
                ]
            })
            .collect()
    }
}
/// Basic screen BitMaps describe the same physical storage as the main PixMap.
/// Their local bounds may be translated; offscreen device selection is irrelevant.
pub(crate) fn basic_surface(m: &Memory, b: Bitmap) -> Result<crate::surface::Surface> {
    if b.base.0 < crate::quickdraw::SCREEN.0 || b.base.0 >= crate::quickdraw::SCREEN.0 + 4096 * 4096
    {
        return Ok(crate::surface::Surface::mono(b));
    }
    let main = m.u32(crate::devices::MAIN).unwrap_or(0);
    if main == 0 {
        return Ok(crate::surface::Surface::mono(b));
    }
    let gd = m.u32(GuestPtr(main))?;
    let ph = m
        .u32(GuestPtr(gd.checked_add(22).ok_or_else(|| {
            Error::Graphics("invalid main display".into())
        })?))?;
    let pm = m.u32(GuestPtr(ph))?;
    let mut s = crate::pixmap::destination(m, GuestPtr(pm))?;
    if s.base != b.base {
        if s.depth > 1
            && b.base.0 >= s.base.0
            && u64::from(b.base.0) < u64::from(s.base.0) + s.byte_len()? as u64
        {
            return Err(Error::Graphics(
                "partial-base basic color screen views are unsupported".into(),
            ));
        }
        return Ok(crate::surface::Surface::mono(b));
    }
    if s.row_bytes != b.row_bytes || s.extent()? != b.extent()? {
        return Err(Error::Graphics(
            "basic screen view does not match main display storage".into(),
        ));
    }
    s.set_bounds(b.bounds);
    Ok(s)
}

impl std::str::FromStr for Display {
    type Err = crate::Error;
    fn from_str(value: &str) -> Result<Self> {
        let parts: Vec<_> = value.split('x').collect();
        if parts.len() != 3 || parts[2] != "8" {
            return Err(Error::Graphics("display must be WIDTHxHEIGHTx8".into()));
        }
        let number = |v: &str| {
            v.parse::<u16>()
                .map_err(|_| Error::Graphics("invalid display dimension".into()))
        };
        Self::indexed8(number(parts[0])?, number(parts[1])?)
    }
}
