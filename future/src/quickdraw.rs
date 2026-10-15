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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyBits {
    pub source: GuestPtr,
    pub destination: GuestPtr,
    pub source_rect: GuestPtr,
    pub destination_rect: GuestPtr,
    pub mode: i16,
    pub mask: GuestPtr,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectangleOp {
    Frame,
    Erase,
    Invert,
    Fill(GuestPtr),
}
#[derive(Clone, Copy)]
struct PatternPaint {
    pattern: [u8; 8],
    mode: u16,
    monochrome: bool,
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
    pub fn move_port_to(&self, memory: &mut Memory, left: i16, top: i16) -> Result<()> {
        let port = self.current(memory)?;
        if memory.u16(offset(port, 6)?)? & 0xc000 != 0 {
            return Err(Error::Graphics("color GrafPorts are unsupported".into()));
        }
        let bytes = memory.read(offset(port, 8)?, 16)?;
        let word = |i| i16::from_be_bytes(bytes[i..i + 2].try_into().unwrap());
        let new_top = word(8).wrapping_sub(top);
        let new_left = word(10).wrapping_sub(left);
        let bottom = new_top.wrapping_add(word(4).wrapping_sub(word(0)));
        let right = new_left.wrapping_add(word(6).wrapping_sub(word(2)));
        let mut bounds = [0; 8];
        for (i, v) in [new_top, new_left, bottom, right].into_iter().enumerate() {
            bounds[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        memory.write(offset(port, 8)?, &bounds)
    }
    pub fn port_size(&self, memory: &mut Memory, width: i16, height: i16) -> Result<()> {
        let port = self.current(memory)?;
        let origin = memory.read(offset(port, 16)?, 4)?;
        let top = i16::from_be_bytes(origin[..2].try_into().unwrap());
        let left = i16::from_be_bytes(origin[2..].try_into().unwrap());
        let mut bytes = [0; 4];
        bytes[..2].copy_from_slice(&top.wrapping_add(height).to_be_bytes());
        bytes[2..].copy_from_slice(&left.wrapping_add(width).to_be_bytes());
        memory.write(offset(port, 20)?, &bytes)
    }
    pub fn set_color(&self, memory: &mut Memory, background: bool, color: u32) -> Result<()> {
        let port = self.current(memory)?;
        if memory.u16(offset(port, 6)?)? & 0xc000 != 0 {
            return Err(Error::Graphics("color GrafPorts are unsupported".into()));
        }
        memory.write(
            offset(port, if background { 84 } else { 80 })?,
            &color.to_be_bytes(),
        )
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
        let point_bytes = memory.read(point, 4)?.try_into().unwrap();
        let result = self.point_coordinates(memory, point_bytes, to_global)?;
        memory.write(point, &result)
    }
    pub fn point_coordinates(
        &self,
        memory: &Memory,
        mut result: [u8; 4],
        to_global: bool,
    ) -> Result<[u8; 4]> {
        let port = self.current(memory)?;
        if memory.u16(offset(port, 6)?)? & 0xc000 != 0 {
            return Err(Error::Graphics(
                "coordinate conversion for color ports is unsupported".into(),
            ));
        }
        let origin: [u8; 4] = memory.read(offset(port, 8)?, 4)?.try_into().unwrap();
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
        Ok(result)
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
    pub fn copy_bits(&self, memory: &mut Memory, gpu: &Graphics, request: CopyBits) -> Result<()> {
        if request.source.0 == 0 || request.destination.0 == 0 {
            return Ok(());
        }
        let source = Bitmap::read(memory, request.source)?;
        let destination = Bitmap::read(memory, request.destination)?;
        let mut source_rect = read_rect(memory, request.source_rect)?;
        let rect = read_rect(memory, request.destination_rect)?;
        if !(0..=7).contains(&request.mode) {
            return Err(Error::Graphics("unsupported CopyBits transfer mode".into()));
        }
        if source_rect.left >= source_rect.right
            || source_rect.top >= source_rect.bottom
            || rect.left >= rect.right
            || rect.top >= rect.bottom
        {
            return Ok(());
        }
        let port = self.current(memory)?;
        let bytes = memory.read(port, PORT_SIZE)?;
        let word = |i| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|i| word(*i) != 0) {
            return Err(Error::Graphics(
                "CopyBits recording or custom procedures are unsupported".into(),
            ));
        }
        let color = |value| match value {
            30 => Ok(0),
            33 => Ok(1),
            _ => Err(Error::Graphics("unsupported monochrome port color".into())),
        };
        let foreground = color(word(80))?;
        let background = color(word(84))?;
        let current = Bitmap::read(memory, offset(port, 2)?)?;
        let mut clips = if current.base == destination.base {
            if current.row_bytes != destination.row_bytes {
                return Err(Error::Graphics(
                    "CopyBits destination aliases a different port stride".into(),
                ));
            }
            let (_, clips) = self.drawing_target_with_visibility(memory, true)?;
            let dx = destination.bounds.left - current.bounds.left;
            let dy = destination.bounds.top - current.bounds.top;
            clips
                .into_iter()
                .map(|r| Rect::new(r.left + dx, r.top + dy, r.right + dx, r.bottom + dy))
                .collect()
        } else {
            vec![destination.bounds]
        };
        if request.mask.0 != 0 {
            let mask = region(memory, request.mask)?;
            if clips.len().saturating_mul(mask.len()) > 65536 {
                return Err(Error::Graphics("CopyBits mask work limit".into()));
            }
            clips = clips
                .iter()
                .flat_map(|a| {
                    mask.iter().map(|b| {
                        Rect::new(
                            a.left.max(b.left),
                            a.top.max(b.top),
                            a.right.min(b.right),
                            a.bottom.min(b.bottom),
                        )
                    })
                })
                .filter(|r| r.left < r.right && r.top < r.bottom)
                .collect();
        }
        source_rect.left -= source.bounds.left;
        source_rect.right -= source.bounds.left;
        source_rect.top -= source.bounds.top;
        source_rect.bottom -= source.bounds.top;
        let (width, height) = source.extent()?;
        // Decode before any destination write, including overlapping backing storage.
        let indices = source.unpack(memory)?;
        destination.execute_source(
            gpu,
            memory,
            Draw::Blit {
                rect,
                clip: destination.bounds,
                source: source_rect,
                width: width as u32,
                height: height as u32,
                foreground,
                background,
                invert: request.mode & 4 != 0,
                mode: match request.mode & 3 {
                    0 => Transfer::Copy,
                    1 => Transfer::Or,
                    2 => Transfer::Xor,
                    _ => Transfer::Bic,
                },
            },
            Some(&clips),
            Some(&indices),
        )
    }
    pub fn line(
        &self,
        memory: &mut Memory,
        gpu: Option<&Graphics>,
        h: i16,
        v: i16,
        relative: bool,
    ) -> Result<()> {
        let port = self.current(memory)?;
        let bytes = memory.read(port, PORT_SIZE)?;
        if u16::from_be_bytes(bytes[6..8].try_into().unwrap()) & 0xc000 != 0 {
            return Err(Error::Graphics("color line ports are unsupported".into()));
        }
        for i in [92, 96, 100, 104] {
            if u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) != 0 {
                return Err(Error::Graphics(
                    "line recording or custom procedures are unsupported".into(),
                ));
            }
        }
        let word = |i| i16::from_be_bytes(bytes[i..i + 2].try_into().unwrap());
        let from = (word(50), word(48));
        let to = if relative {
            (from.0.saturating_add(h), from.1.saturating_add(v))
        } else {
            (h, v)
        };
        let geometry = crate::lines::geometry(from, to, word(54), word(52))?;
        memory.validate_write(offset(port, 48)?, 4)?;
        if word(66) >= 0 && geometry.width != 0 && geometry.height != 0 {
            let (bitmap, _) = self.drawing_target(memory)?;
            let bounds = crate::lines::bounds(geometry);
            let clipped = Rect::new(
                bounds.left.max(bitmap.bounds.left),
                bounds.top.max(bitmap.bounds.top),
                bounds.right.min(bitmap.bounds.right),
                bounds.bottom.min(bitmap.bounds.bottom),
            );
            if clipped.left < clipped.right && clipped.top < clipped.bottom {
                let gpu =
                    gpu.ok_or_else(|| Error::Graphics("line drawing requires a GPU".into()))?;
                self.paint_coverage_with_line(gpu, memory, clipped, None, Some(geometry), None)?;
            }
        }
        let mut location = [0; 4];
        location[..2].copy_from_slice(&to.1.to_be_bytes());
        location[2..].copy_from_slice(&to.0.to_be_bytes());
        memory.write(offset(port, 48)?, &location)
    }
    pub fn draw_rectangle(
        &self,
        memory: &mut Memory,
        gpu: &Graphics,
        rectangle: GuestPtr,
        op: RectangleOp,
    ) -> Result<()> {
        let r = read_rect(memory, rectangle)?;
        if op == RectangleOp::Frame {
            return self.round_rect(gpu, memory, r, 0, 0, true);
        }
        let port = self.current(memory)?;
        let (paint, fill) = match op {
            RectangleOp::Erase => (
                PatternPaint {
                    pattern: memory.read(offset(port, 32)?, 8)?.try_into().unwrap(),
                    mode: 8,
                    monochrome: false,
                },
                false,
            ),
            RectangleOp::Invert => (
                PatternPaint {
                    pattern: [255; 8],
                    mode: 10,
                    monochrome: true,
                },
                false,
            ),
            RectangleOp::Fill(pattern) => {
                let pattern = memory.read(pattern, 8)?.try_into().unwrap();
                memory.validate_write(offset(port, 40)?, 8)?;
                (
                    PatternPaint {
                        pattern,
                        mode: 8,
                        monochrome: false,
                    },
                    true,
                )
            }
            RectangleOp::Frame => unreachable!(),
        };
        self.paint_coverage_with_line(gpu, memory, r, None, None, Some(paint))?;
        if fill {
            memory.write(offset(port, 40)?, &paint.pattern)?;
        }
        Ok(())
    }
    pub fn paint_rect(&self, gpu: &Graphics, memory: &mut Memory, r: Rect) -> Result<()> {
        self.paint_coverage(gpu, memory, r, None)
    }
    pub fn round_rect(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        r: Rect,
        width: i16,
        height: i16,
        frame: bool,
    ) -> Result<()> {
        let p = self.current(memory)?;
        let pen = if frame {
            Some((
                memory.u16(offset(p, 54)?)? as i16,
                memory.u16(offset(p, 52)?)? as i16,
            ))
        } else {
            None
        };
        let coverage = if frame && (memory.u16(offset(p, 66)?)? as i16) < 0 {
            vec![]
        } else {
            crate::rounded::coverage(r, width, height, pen)?
        };
        self.paint_coverage(gpu, memory, r, Some(&coverage))
    }
    pub(crate) fn drawing_target(&self, memory: &Memory) -> Result<(Bitmap, Vec<Rect>)> {
        self.drawing_target_with_visibility(memory, false)
    }
    fn drawing_target_with_visibility(
        &self,
        memory: &Memory,
        force_visibility: bool,
    ) -> Result<(Bitmap, Vec<Rect>)> {
        let p = self.current(memory)?;
        let bytes = memory.read(p, PORT_SIZE)?;
        let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|o| word(*o) != 0) {
            return Err(Error::Graphics(
                "QuickDraw recording or custom procedures are unsupported".into(),
            ));
        }
        let port_rect = read_rect(memory, offset(p, 16)?)?;
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
        let visible = if force_visibility || bitmap.base == screen.base {
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
        Ok((bitmap, clips))
    }
    pub(crate) fn paint_coverage(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        r: Rect,
        shape: Option<&[Rect]>,
    ) -> Result<()> {
        self.paint_coverage_with_line(gpu, memory, r, shape, None, None)
    }
    fn paint_coverage_with_line(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        r: Rect,
        shape: Option<&[Rect]>,
        line: Option<crate::graphics::PenLine>,
        paint: Option<PatternPaint>,
    ) -> Result<()> {
        let p = self.current(memory)?;
        let bytes = memory.read(p, PORT_SIZE)?;
        let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|o| word(*o) != 0) {
            return Err(Error::Graphics(
                "QuickDraw recording or custom procedures are unsupported".into(),
            ));
        }
        let mode = paint.map_or_else(
            || u16::from_be_bytes(bytes[56..58].try_into().unwrap()),
            |p| p.mode,
        );
        if !(8..=15).contains(&mode) {
            return Err(Error::Graphics("unsupported QuickDraw pen mode".into()));
        }
        let color = |v| match v {
            33 => Ok(1),
            30 => Ok(0),
            _ => Err(Error::Graphics("unsupported monochrome port color".into())),
        };
        let (foreground, background) = if paint.is_some_and(|p| p.monochrome) {
            (1, 0)
        } else {
            (color(word(80))?, color(word(84))?)
        };
        let mut pattern: [u8; 8] =
            paint.map_or_else(|| bytes[58..66].try_into().unwrap(), |p| p.pattern);
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
        let (bitmap, mut clips) = self.drawing_target(memory)?;
        let intersect = |a: Rect, b: Rect| {
            Rect::new(
                a.left.max(b.left),
                a.top.max(b.top),
                a.right.min(b.right),
                a.bottom.min(b.bottom),
            )
        };
        if let Some(shape) = shape {
            if clips.len().saturating_mul(shape.len()) > 1_000_000 {
                return Err(Error::Graphics(
                    "shape intersection work limit exceeded".into(),
                ));
            }
            clips = clips
                .iter()
                .flat_map(|clip| shape.iter().map(|part| intersect(*clip, *part)))
                .filter(|r| r.left < r.right && r.top < r.bottom)
                .collect();
        }
        bitmap.execute_clipped(
            gpu,
            memory,
            if let Some(line) = line {
                Draw::Line {
                    rect: r,
                    clip: port_rect,
                    line,
                    pattern,
                    foreground,
                    background,
                    mode: match mode & 3 {
                        0 => Transfer::Copy,
                        1 => Transfer::Or,
                        2 => Transfer::Xor,
                        _ => Transfer::Bic,
                    },
                }
            } else {
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
                }
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
