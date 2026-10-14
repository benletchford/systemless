//! Basic GrafPort services over guest records. InitGraf/ABI binding is separate.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
pub const PORT_SIZE: usize = 108;
#[derive(Clone, Copy)]
pub struct Ports {
    pub current_slot: GuestPtr,
    pub screen_bitmap: GuestPtr,
}
fn offset(p: GuestPtr, n: u32) -> Result<GuestPtr> {
    p.0.checked_add(n)
        .map(GuestPtr)
        .ok_or_else(|| Error::Graphics("QuickDraw address overflow".into()))
}
pub fn read_rect(memory: &Memory, p: GuestPtr) -> Result<Rect> {
    let b = memory.read(p, 8)?;
    let v = |i| i16::from_be_bytes(b[i..i + 2].try_into().unwrap()) as i32;
    Ok(Rect::new(v(2), v(0), v(6), v(4)))
}
fn rect_bytes(r: Rect) -> Result<[u8; 8]> {
    let mut b = [0; 8];
    for (i, v) in [r.top, r.left, r.bottom, r.right].into_iter().enumerate() {
        let v =
            i16::try_from(v).map_err(|_| Error::Graphics("QuickDraw rectangle range".into()))?;
        b[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
    }
    Ok(b)
}
fn region(memory: &Memory, handle: GuestPtr) -> Result<Rect> {
    let data = GuestPtr(memory.u32(handle)?);
    if memory.u16(data)? != 10 {
        return Err(Error::Graphics(
            "nonrectangular QuickDraw region is unsupported".into(),
        ));
    }
    read_rect(memory, offset(data, 2)?)
}
fn write_region(memory: &mut Memory, handle: GuestPtr, r: Rect) -> Result<()> {
    let data = GuestPtr(memory.u32(handle)?);
    if memory.u16(data)? != 10 {
        return Err(Error::Graphics(
            "resizing QuickDraw regions is unsupported".into(),
        ));
    }
    let mut bytes = [0; 10];
    bytes[..2].copy_from_slice(&10u16.to_be_bytes());
    bytes[2..].copy_from_slice(&rect_bytes(r)?);
    memory.write(data, &bytes)
}
impl Ports {
    pub fn current(&self, memory: &Memory) -> Result<GuestPtr> {
        let port = GuestPtr(memory.u32(self.current_slot)?);
        memory.read(port, PORT_SIZE)?;
        if port.0 == 0 {
            return Err(Error::Graphics("no current QuickDraw port".into()));
        }
        Ok(port)
    }
    pub fn set(&self, memory: &mut Memory, port: GuestPtr) -> Result<()> {
        memory.read(port, PORT_SIZE)?;
        if port.0 == 0 {
            return Err(Error::Graphics("nil QuickDraw port".into()));
        }
        memory.write(self.current_slot, &port.0.to_be_bytes())
    }
    pub fn open(&self, memory: &mut Memory, process: &mut Process, port: GuestPtr) -> Result<()> {
        memory.validate_write(port, PORT_SIZE)?;
        memory.validate_write(self.current_slot, 4)?;
        let screen = Bitmap::read(memory, self.screen_bitmap)?;
        let bitmap: [u8; 14] = memory.read(self.screen_bitmap, 14)?.try_into().unwrap();
        let visible = process.new_handle(memory, 10, true)?;
        if visible == 0 {
            return Err(Error::Graphics("OpenPort region allocation failed".into()));
        }
        let clip = process.new_handle(memory, 10, true)?;
        if clip == 0 {
            process.dispose_handle(memory, visible)?;
            process.mem_error = -108;
            return Err(Error::Graphics("OpenPort region allocation failed".into()));
        }
        for (h, r) in [
            (visible, screen.bounds),
            (clip, Rect::new(-32768, -32768, 32767, 32767)),
        ] {
            let data = GuestPtr(memory.u32(GuestPtr(h))?);
            let mut b = [0; 10];
            b[..2].copy_from_slice(&10u16.to_be_bytes());
            b[2..].copy_from_slice(&rect_bytes(r)?);
            memory.write(data, &b)?;
        }
        let mut bytes = [0; PORT_SIZE];
        bytes[2..16].copy_from_slice(&bitmap);
        bytes[16..24].copy_from_slice(&rect_bytes(screen.bounds)?);
        bytes[24..28].copy_from_slice(&visible.to_be_bytes());
        bytes[28..32].copy_from_slice(&clip.to_be_bytes());
        bytes[40..48].fill(255);
        bytes[58..66].fill(255);
        for (o, v) in [(52, 1u16), (54, 1), (56, 8), (72, 1)] {
            bytes[o..o + 2].copy_from_slice(&v.to_be_bytes());
        }
        bytes[80..84].copy_from_slice(&33u32.to_be_bytes());
        bytes[84..88].copy_from_slice(&30u32.to_be_bytes());
        memory.write(port, &bytes)?;
        self.set(memory, port)
    }
    pub fn set_bits(&self, memory: &mut Memory, bitmap: GuestPtr) -> Result<()> {
        Bitmap::read(memory, bitmap)?;
        let bytes: [u8; 14] = memory.read(bitmap, 14)?.try_into().unwrap();
        memory.write(offset(self.current(memory)?, 2)?, &bytes)
    }
    pub fn clip_rect(&self, memory: &mut Memory, r: Rect) -> Result<()> {
        let p = self.current(memory)?;
        let handle = GuestPtr(memory.u32(offset(p, 28)?)?);
        write_region(memory, handle, r)
    }
    pub fn paint_rect(&self, gpu: &Graphics, memory: &mut Memory, r: Rect) -> Result<()> {
        let p = self.current(memory)?;
        let bytes = memory.read(p, PORT_SIZE)?;
        let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|o| word(*o) != 0) {
            return Err(Error::Graphics(
                "QuickDraw recording or custom procedures are unsupported".into(),
            ));
        }
        let mode = u16::from_be_bytes(bytes[56..58].try_into().unwrap());
        if !(8..=15).contains(&mode) {
            return Err(Error::Graphics("unsupported QuickDraw pen mode".into()));
        }
        let color = |v| match v {
            33 => Ok(1),
            30 => Ok(0),
            _ => Err(Error::Graphics("unsupported monochrome port color".into())),
        };
        let foreground = color(word(80))?;
        let background = color(word(84))?;
        let mut pattern: [u8; 8] = bytes[58..66].try_into().unwrap();
        if mode & 4 != 0 {
            for row in &mut pattern {
                *row = !*row;
            }
        }
        let port_rect = read_rect(memory, offset(p, 16)?)?;
        let pattern = std::array::from_fn(|y| {
            pattern[((y as i32 - port_rect.top) & 7) as usize]
                .rotate_right((port_rect.left & 7) as u32)
        });
        let visible_handle = GuestPtr(word(24));
        let clip = region(memory, GuestPtr(word(28)))?;
        let intersect = |a: Rect, b: Rect| {
            Rect::new(
                a.left.max(b.left),
                a.top.max(b.top),
                a.right.min(b.right),
                a.bottom.min(b.bottom),
            )
        };
        let bitmap = Bitmap::read(memory, offset(p, 2)?)?;
        let screen = Bitmap::read(memory, self.screen_bitmap)?;
        let visible = if bitmap.base == screen.base {
            region(memory, visible_handle)?
        } else {
            bitmap.bounds
        };
        bitmap.execute(
            gpu,
            memory,
            Draw::Fill {
                rect: r,
                clip: intersect(port_rect, intersect(visible, clip)),
                pattern,
                foreground,
                background,
                mode: match mode & 3 {
                    0 => Transfer::Copy,
                    1 => Transfer::Or,
                    2 => Transfer::Xor,
                    _ => Transfer::Bic,
                },
            },
        )
    }
}
