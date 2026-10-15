//! Shared packed guest-pixel storage and synchronized wgpu drawing boundary.
//! CPU work packs storage and validates metadata; rendering and transfers use wgpu.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, IndexedTarget, Rect},
    memory::{Memory, Permissions},
    Error, Result,
};
#[derive(Clone)]
pub(crate) struct Palette {
    pub rgb: Box<[[u16; 3]; 256]>,
    pub defined: [bool; 256],
}
#[derive(Clone)]
pub(crate) struct Surface {
    bitmap: Bitmap,
    pub depth: u16,
    pub colors: Option<Palette>,
}
impl std::ops::Deref for Surface {
    type Target = Bitmap;
    fn deref(&self) -> &Bitmap {
        &self.bitmap
    }
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
pub(crate) fn unpack_pixels(memory: &Memory, bitmap: Bitmap, depth: u16) -> Result<Vec<u8>> {
    let (width, height) = bitmap.extent()?;
    if ![1, 2, 4, 8].contains(&depth) || width * usize::from(depth) > bitmap.row_bytes * 8 {
        return Err(bad("invalid packed pixel storage"));
    }
    let data = memory.read(bitmap.base, bitmap.byte_len()?)?;
    let depth = usize::from(depth);
    let mask = ((1u16 << depth) - 1) as u8;
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let bit = x * depth;
            out.push((data[y * bitmap.row_bytes + bit / 8] >> (8 - depth - bit % 8)) & mask);
        }
    }
    Ok(out)
}
impl Surface {
    pub(crate) fn set_bounds(&mut self, bounds: Rect) {
        self.bitmap.bounds = bounds;
    }
    pub fn mono(bitmap: Bitmap) -> Self {
        Self {
            bitmap,
            depth: 1,
            colors: None,
        }
    }
    pub fn indexed(
        bitmap: Bitmap,
        depth: u16,
        rgb: Box<[[u16; 3]; 256]>,
        defined: [bool; 256],
    ) -> Result<Self> {
        if ![1, 2, 4, 8].contains(&depth)
            || bitmap.extent()?.0 * usize::from(depth) > bitmap.row_bytes * 8
        {
            return Err(bad("invalid indexed destination geometry"));
        }
        if depth == 1 {
            if !defined[0] || !defined[1] || rgb[0] != [65535; 3] || rgb[1] != [0; 3] {
                return Err(bad("one-bit destinations require a white/black palette"));
            }
            return Ok(Self::mono(bitmap));
        }
        if !defined[..1usize << depth].iter().any(|v| *v) {
            return Err(bad("indexed destination has no usable colors"));
        }
        Ok(Self {
            bitmap,
            depth,
            colors: Some(Palette { rgb, defined }),
        })
    }
    pub fn compatible(&self, other: &Self) -> bool {
        if self.depth != other.depth {
            return false;
        }
        match (&self.colors, &other.colors) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                let n = 1usize << self.depth;
                a.rgb[..n] == b.rgb[..n] && a.defined[..n] == b.defined[..n]
            }
            _ => false,
        }
    }
    pub fn index(&self, v: u32) -> Result<u8> {
        if v >= (1u32 << self.depth) || self.colors.as_ref().is_some_and(|c| !c.defined[v as usize])
        {
            Err(bad("undefined destination pixel index"))
        } else {
            Ok(v as u8)
        }
    }
    pub fn rgb(&self, v: u8) -> Result<[u16; 3]> {
        self.index(u32::from(v))?;
        Ok(self
            .colors
            .as_ref()
            .map_or(if v == 0 { [65535; 3] } else { [0; 3] }, |c| {
                c.rgb[v as usize]
            }))
    }
    pub fn nearest(&self, rgb: [u16; 3]) -> u8 {
        let Some(c) = &self.colors else {
            return u8::from(rgb.iter().map(|v| u32::from(*v)).sum::<u32>() < 98303);
        };
        crate::color::nearest_color(
            rgb,
            (0..1usize << self.depth)
                .filter(|i| c.defined[*i])
                .map(|i| (i as u8, c.rgb[i])),
        )
        .unwrap()
    }
    pub fn unpack(&self, memory: &Memory) -> Result<Vec<u8>> {
        let pixels = unpack_pixels(memory, self.bitmap, self.depth)?;
        for v in &pixels {
            self.index(u32::from(*v))?;
        }
        Ok(pixels)
    }
    fn pack(&self, memory: &mut Memory, indices: &[u8]) -> Result<()> {
        let (width, height) = self.extent()?;
        if indices.len() != width * height {
            return Err(bad("destination index extent mismatch"));
        }
        for v in indices {
            self.index(u32::from(*v))?;
        }
        let mut bytes = memory.read(self.base, self.byte_len()?)?.to_vec();
        let depth = usize::from(self.depth);
        let mask = ((1u16 << depth) - 1) as u8;
        for y in 0..height {
            for x in 0..width {
                let bit = x * depth;
                let shift = 8 - depth - bit % 8;
                let byte = &mut bytes[y * self.row_bytes + bit / 8];
                *byte = (*byte & !(mask << shift)) | (indices[y * width + x] << shift);
            }
        }
        memory.write(self.base, &bytes)
    }
    fn config(&self, direct_source: bool) -> Option<IndexedTarget<'_>> {
        self.colors.as_ref().map(|c| IndexedTarget {
            depth: self.depth,
            rgb: &c.rgb,
            defined: &c.defined,
            direct_source,
        })
    }
    /// Refresh a presentation texture without changing guest bytes. Unlike a
    /// drawing operation this also accepts read-only guest pixel mappings.
    pub fn present_draw(&self, gpu: &Graphics, memory: &Memory, draw: Draw) -> Result<()> {
        if !matches!(draw, Draw::Present | Draw::Cursor { .. }) {
            return Err(Error::Graphics(
                "presentation requires a presentation command".into(),
            ));
        }
        let (width, height) = self.extent()?;
        if (width, height) != (gpu.width as usize, gpu.height as usize) {
            return Err(Error::Graphics("presentation extent mismatch".into()));
        }
        let mut staging = Memory::default();
        staging.map(gpu.base, self.unpack(memory)?, Permissions::RW)?;
        gpu.execute_indexed(&mut staging, draw, None, None, None, self.config(false))
    }
    /// Commands use bitmap-local QuickDraw coordinates. Temporary indices only
    /// encode storage; all pixel operations still execute through wgpu.
    pub fn execute_clipped(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
    ) -> Result<()> {
        self.execute_source(gpu, memory, draw, clips, None)
    }
    pub(crate) fn execute_source(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        source: Option<&[u8]>,
    ) -> Result<()> {
        self.execute_source_palette(gpu, memory, draw, clips, source, None)
    }
    pub(crate) fn execute_source_palette(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        source: Option<&[u8]>,
        palette: Option<&[[u16; 3]; 256]>,
    ) -> Result<()> {
        if palette.is_none()
            && !matches!(draw, Draw::Copy { .. })
            && source.is_some_and(|s| s.iter().any(|v| *v > 1))
        {
            return Err(Error::Graphics("non-monochrome copy source".into()));
        }
        match draw {
            Draw::Fill {
                foreground,
                background,
                ..
            }
            | Draw::Line {
                foreground,
                background,
                ..
            }
            | Draw::Polygon {
                foreground,
                background,
                ..
            }
            | Draw::Blit {
                foreground,
                background,
                ..
            } => {
                self.index(u32::from(foreground))?;
                self.index(u32::from(background))?;
            }
            _ => {}
        }
        let (width, height) = self.extent()?;
        memory.validate_write(self.base, self.byte_len()?)?;
        if (width, height) != (gpu.width as usize, gpu.height as usize) {
            let target = gpu.target(width as u32, height as u32)?;
            return self.execute_source_palette(&target, memory, draw, clips, source, palette);
        }
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
            Draw::Cursor { .. } => {
                return Err(Error::Graphics(
                    "cursor commands require the presentation route".into(),
                ))
            }
            Draw::Present => Draw::Present,
            Draw::Polygon {
                rect: r,
                clip,
                vertices,
                origin_x,
                origin_y,
                frame,
                pattern,
                foreground,
                background,
                mode,
            } => {
                if self.depth == 1 && (foreground > 1 || background > 1) {
                    return Err(Error::Graphics(
                        "monochrome colors must be bit values".into(),
                    ));
                }
                let pattern = std::array::from_fn(|y| {
                    pattern[((self.bounds.top + y as i32) & 7) as usize]
                        .rotate_left((self.bounds.left & 7) as u32)
                });
                Draw::Polygon {
                    rect: rect(r)?,
                    clip: rect(clip)?,
                    vertices,
                    origin_x: coordinate(origin_x, self.bounds.left)?,
                    origin_y: coordinate(origin_y, self.bounds.top)?,
                    frame,
                    pattern,
                    foreground,
                    background,
                    mode,
                }
            }
            Draw::Line {
                rect: r,
                clip,
                mut line,
                pattern,
                foreground,
                background,
                mode,
            } => {
                line.origin_x = coordinate(line.origin_x, self.bounds.left)?;
                line.origin_y = coordinate(line.origin_y, self.bounds.top)?;
                Draw::Line {
                    rect: rect(r)?,
                    clip: rect(clip)?,
                    line,
                    pattern,
                    foreground,
                    background,
                    mode,
                }
            }
            Draw::Fill {
                rect: r,
                clip,
                pattern,
                foreground,
                background,
                mode,
            } => {
                if self.depth == 1 && (foreground > 1 || background > 1) {
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
            Draw::Blit {
                mask,
                rect: r,
                clip,
                source,
                width,
                height,
                foreground,
                background,
                invert,
                mode,
            } => Draw::Blit {
                mask,
                rect: rect(r)?,
                clip: rect(clip)?,
                source,
                width,
                height,
                foreground,
                background,
                invert,
                mode,
            },
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
        let translated = clips
            .map(|clips| clips.iter().copied().map(rect).collect::<Result<Vec<_>>>())
            .transpose()?;
        let direct_source = palette.zip(self.colors.as_ref()).is_some_and(|(a, b)| {
            a == b.rgb.as_ref()
                && source.is_some_and(|s| s.iter().all(|v| self.index(u32::from(*v)).is_ok()))
        });
        gpu.execute_indexed(
            &mut staging,
            draw,
            translated.as_deref(),
            source,
            palette,
            self.config(direct_source),
        )?;
        self.pack(memory, staging.read(gpu.base, width * height)?)
    }
}
