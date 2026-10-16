//! Basic QuickDraw BitMap layout and its packed-bit/GPU boundary.
//! Apple: Imaging With QuickDraw, Basic QuickDraw, BitMap record.
use crate::{
    graphics::{Draw, Graphics, Rect},
    memory::{GuestPtr, Memory},
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
    pub(crate) fn byte_len(&self) -> Result<usize> {
        Ok(self.row_bytes * self.extent()?.1)
    }

    pub(crate) fn surface(&self, memory: &Memory) -> Result<crate::surface::Surface> {
        crate::display::basic_surface(memory, *self)
    }
    pub(crate) fn unpack(&self, memory: &Memory) -> Result<Vec<u8>> {
        self.surface(memory)?.unpack(memory)
    }
    pub fn present(&self, gpu: &Graphics, memory: &Memory) -> Result<()> {
        self.surface(memory)?
            .present_draw(gpu, memory, Draw::Present)
    }
    pub fn present_draw(&self, gpu: &Graphics, memory: &Memory, draw: Draw) -> Result<()> {
        self.surface(memory)?.present_draw(gpu, memory, draw)
    }
    pub fn execute(&self, gpu: &Graphics, memory: &mut Memory, draw: Draw) -> Result<()> {
        self.execute_clipped(gpu, memory, draw, None)
    }
    pub fn execute_clipped(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
    ) -> Result<()> {
        let surface = self.surface(memory)?;
        let draw = basic_draw(&surface, draw)?;
        surface.execute_clipped(gpu, memory, draw, clips)
    }
    pub(crate) fn execute_source(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        source: Option<&[u8]>,
    ) -> Result<()> {
        let surface = self.surface(memory)?;
        let draw = basic_draw(&surface, draw)?;
        surface.execute_source(gpu, memory, draw, clips, source)
    }
}

fn basic_draw<'a>(surface: &crate::surface::Surface, mut draw: Draw<'a>) -> Result<Draw<'a>> {
    let color = |v: u32| match v {
        0 => surface.nearest([65535; 3]),
        1 => surface.nearest([0; 3]),
        _ => Err(Error::Graphics(
            "basic bitmap colors must be bit values".into(),
        )),
    };
    match &mut draw {
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
            *foreground = u32::from(color(*foreground)?);
            *background = u32::from(color(*background)?);
        }
        _ => {}
    }
    Ok(draw)
}
