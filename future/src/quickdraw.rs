//! Basic QuickDraw globals and GrafPort services shared by both Macintosh ABIs.
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
fn region(memory: &Memory, handle: GuestPtr) -> Result<Vec<Rect>> {
    let data = GuestPtr(memory.u32(handle)?);
    let size = memory.u16(data)? as usize;
    crate::region_geometry::decode(memory.read(data, size)?)
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
    /// QuickDraw's unsigned seed is part of QDGlobals, independent of the
    /// selected port and the unrelated low-memory random-seed location.
    pub fn random(&self, memory: &mut Memory) -> Result<i16> {
        let seed = GuestPtr(
            self.current_slot
                .0
                .checked_sub(126)
                .ok_or_else(|| Error::Graphics("invalid QuickDraw globals address".into()))?,
        );
        let next = ((u64::from(memory.u32(seed)?) * 16_807) % 2_147_483_647) as u32;
        memory.write(seed, &next.to_be_bytes())?;
        let value = next as i16;
        Ok(if value == i16::MIN { 0 } else { value })
    }
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
    /// Basic QuickDraw Point coordinates are signed words. Bitmap bounds
    /// express the global origin in the port's local coordinate system.
    pub fn convert_point(
        &self,
        memory: &mut Memory,
        point: GuestPtr,
        to_global: bool,
    ) -> Result<()> {
        let port = self.current(memory)?;
        if memory.u16(offset(port, 6)?)? & 0xc000 != 0 {
            return Err(Error::Graphics(
                "coordinate conversion for color ports is unsupported".into(),
            ));
        }
        let origin: [u8; 4] = memory.read(offset(port, 8)?, 4)?.try_into().unwrap();
        let mut result: [u8; 4] = memory.read(point, 4)?.try_into().unwrap();
        for at in [0, 2] {
            let value = i16::from_be_bytes(result[at..at + 2].try_into().unwrap());
            let delta = i16::from_be_bytes(origin[at..at + 2].try_into().unwrap());
            let value = if to_global {
                value.wrapping_sub(delta)
            } else {
                value.wrapping_add(delta)
            };
            result[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        memory.write(point, &result)
    }
    pub fn close(&self, memory: &mut Memory, process: &mut Process, port: GuestPtr) -> Result<()> {
        let bytes = memory.read(port, PORT_SIZE)?;
        if u16::from_be_bytes(bytes[6..8].try_into().unwrap()) & 0xc000 != 0 {
            return Err(Error::Graphics(
                "ClosePort does not support color ports".into(),
            ));
        }
        let visible = u32::from_be_bytes(bytes[24..28].try_into().unwrap());
        let clip = u32::from_be_bytes(bytes[28..32].try_into().unwrap());
        if visible == clip {
            return Err(Error::Graphics(
                "ClosePort requires distinct region handles".into(),
            ));
        }
        // Validate both before releasing either; malformed guest state must not
        // leave a half-closed port. The caller owns the port record and bitmap.
        for handle in [visible, clip] {
            if process.handle_size(memory, handle)? < 10 {
                return Err(Error::Graphics(
                    "ClosePort requires owned region handles".into(),
                ));
            }
            memory.validate_write(GuestPtr(handle), 4)?;
        }
        process.dispose_handle(memory, visible)?;
        process.dispose_handle(memory, clip)
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
        self.reset(memory, port, screen, bitmap, visible, clip)
    }
    pub fn init(&self, memory: &mut Memory, process: &mut Process, port: GuestPtr) -> Result<()> {
        memory.validate_write(port, PORT_SIZE)?;
        memory.validate_write(self.current_slot, 4)?;
        let bytes = memory.read(port, PORT_SIZE)?;
        if u16::from_be_bytes(bytes[6..8].try_into().unwrap()) & 0xc000 != 0 {
            return Err(Error::Graphics(
                "InitPort does not support color ports".into(),
            ));
        }
        let visible = u32::from_be_bytes(bytes[24..28].try_into().unwrap());
        let clip = u32::from_be_bytes(bytes[28..32].try_into().unwrap());
        if visible == clip {
            return Err(Error::Graphics("InitPort requires distinct regions".into()));
        }
        for handle in [visible, clip] {
            if process.handle_size(memory, handle)? != 10 {
                return Err(Error::Graphics(
                    "InitPort requires owned rectangular regions".into(),
                ));
            }
            let data = GuestPtr(memory.u32(GuestPtr(handle))?);
            memory.validate_write(data, 10)?;
        }
        let screen = Bitmap::read(memory, self.screen_bitmap)?;
        let bitmap = memory.read(self.screen_bitmap, 14)?.try_into().unwrap();
        self.reset(memory, port, screen, bitmap, visible, clip)
    }
    fn reset(
        &self,
        memory: &mut Memory,
        port: GuestPtr,
        screen: Bitmap,
        bitmap: [u8; 14],
        visible: u32,
        clip: u32,
    ) -> Result<()> {
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
            vec![bitmap.bounds]
        };
        if visible.len().saturating_mul(clip.len()) > 1_000_000 {
            return Err(Error::Graphics(
                "region intersection work limit exceeded".into(),
            ));
        }
        let mut clips = Vec::new();
        for a in &visible {
            for b in &clip {
                let r = intersect(port_rect, intersect(*a, *b));
                if r.left < r.right && r.top < r.bottom {
                    clips.push(r);
                }
            }
        }
        bitmap.execute_clipped(
            gpu,
            memory,
            Draw::Fill {
                rect: r,
                clip: port_rect,
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
            Some(&clips),
        )
    }
}

#[cfg(test)]
const GLOBALS: GuestPtr = GuestPtr(0x7400_0000);
pub const SCREEN: GuestPtr = GuestPtr(0x7410_0000);
/// Host display policy, independent of the current port or application globals.
pub fn screen_bitmap() -> Bitmap {
    Bitmap {
        base: SCREEN,
        row_bytes: 64,
        bounds: Rect::new(0, 0, 512, 342),
    }
}
/// Initial monochrome host policy; changing display modes is not implemented.
pub(crate) fn install(memory: &mut Memory) -> Result<()> {
    memory.map(SCREEN, vec![0; 64 * 342], crate::memory::Permissions::RW)
}
pub(crate) fn initialize(
    memory: &mut Memory,
    slot: GuestPtr,
    anchor: Option<GuestPtr>,
) -> Result<Ports> {
    let base = GuestPtr(
        slot.0
            .checked_sub(202)
            .ok_or_else(|| Error::Graphics("invalid QuickDraw globals address".into()))?,
    );
    memory.validate_write(base, 206)?;
    if let Some(anchor) = anchor {
        memory.validate_write(anchor, 4)?;
    }
    let mut bytes = [0; 206];
    bytes[76..80].copy_from_slice(&1u32.to_be_bytes());
    bytes[80..84].copy_from_slice(&SCREEN.0.to_be_bytes());
    bytes[84..86].copy_from_slice(&64u16.to_be_bytes());
    bytes[86..94].copy_from_slice(&rect_bytes(Rect::new(0, 0, 512, 342))?);
    let arrow: [u16; 32] = [
        0x8000, 0xc000, 0xe000, 0xf000, 0xf800, 0xfc00, 0xfe00, 0xff00, 0xff80, 0xfc00, 0xdc00,
        0x8e00, 0x0600, 0x0700, 0x0300, 0, 0xc000, 0xe000, 0xf000, 0xf800, 0xfc00, 0xfe00, 0xff00,
        0xff80, 0xffc0, 0xffc0, 0xfe00, 0xdf00, 0x8f00, 0x0780, 0x0780, 0x0380,
    ];
    for (i, v) in arrow.into_iter().enumerate() {
        bytes[94 + i * 2..96 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    bytes[158..162].copy_from_slice(&[0, 1, 0, 1]);
    bytes[162..170].copy_from_slice(&[0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd]);
    bytes[170..178].copy_from_slice(&[0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22]);
    bytes[178..186].copy_from_slice(&[0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55]);
    bytes[186..194].fill(255);
    memory.write(base, &bytes)?;
    if let Some(anchor) = anchor {
        memory.write(anchor, &slot.0.to_be_bytes())?;
    }
    Ok(Ports {
        current_slot: slot,
        screen_bitmap: offset(base, 80)?,
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOp {
    LocalToGlobal,
    GlobalToLocal,
    Open,
    Init,
    Close,
    Set,
    Get,
    SetBits,
    ClipRect,
    PaintRect,
    PenPat,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn init_graf_sets_guest_globals_and_68k_anchor_after_validation() {
        let mut memory = Memory::default();
        install(&mut memory).unwrap();
        memory
            .map(GLOBALS, vec![0; 206], crate::memory::Permissions::RW)
            .unwrap();
        memory
            .map(GuestPtr(0x1000), vec![0; 4], crate::memory::Permissions::RW)
            .unwrap();
        let slot = GuestPtr(GLOBALS.0 + 202);
        assert!(initialize(&mut memory, slot, Some(GuestPtr(0xffff_fffe))).is_err());
        assert_eq!(memory.read(GLOBALS, 206).unwrap(), &[0; 206]);
        let ports = initialize(&mut memory, slot, Some(GuestPtr(0x1000))).unwrap();
        assert_eq!(memory.u32(GuestPtr(0x1000)).unwrap(), slot.0);
        assert_eq!(memory.u32(slot).unwrap(), 0);
        assert_eq!(memory.u32(GuestPtr(GLOBALS.0 + 76)).unwrap(), 1);
        assert_eq!(
            memory.read(GuestPtr(GLOBALS.0 + 186), 8).unwrap(),
            &[255; 8]
        );
        assert_eq!(memory.read(GuestPtr(GLOBALS.0 + 194), 8).unwrap(), &[0; 8]);
        let bitmap = Bitmap::read(&memory, ports.screen_bitmap).unwrap();
        assert_eq!(bitmap.base, SCREEN);
        assert_eq!(bitmap.extent().unwrap(), (512, 342));
    }
}
