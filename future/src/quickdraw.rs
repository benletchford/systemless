//! Basic QuickDraw globals and GrafPort services shared by both Macintosh ABIs.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    port_layout::{self, Pattern as PortPattern},
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
pub struct CopyMask {
    pub source: GuestPtr,
    pub mask: GuestPtr,
    pub destination: GuestPtr,
    pub source_rect: GuestPtr,
    pub mask_rect: GuestPtr,
    pub destination_rect: GuestPtr,
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
#[derive(Clone, Copy)]
enum Coverage<'a> {
    Shape(Option<&'a [Rect]>),
    Line(crate::graphics::PenLine),
    RecordingLine(crate::graphics::PenLine),
    Polygon {
        vertices: &'a [(i16, i16)],
        frame: Option<(i16, i16)>,
    },
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
    pub(crate) fn require_basic(&self, memory: &Memory) -> Result<()> {
        let port = self.current(memory)?;
        if memory.u16(offset(port, 6)?)? & 0xc000 != 0 {
            return Err(Error::Graphics(
                "drawing and pattern changes in color ports are not implemented".into(),
            ));
        }
        Ok(())
    }

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
    pub fn set_origin(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        h: i16,
        v: i16,
    ) -> Result<()> {
        let old_error = process.mem_error;
        let result = (|| {
            let port = self.current(memory)?;
            let bytes = memory.read(port, PORT_SIZE)?;
            let word = |at| i16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
            let dh = i32::from(h.wrapping_sub(word(18)));
            let dv = i32::from(v.wrapping_sub(word(16)));
            let visible = u32::from_be_bytes(bytes[24..28].try_into().unwrap());
            let clip = u32::from_be_bytes(bytes[28..32].try_into().unwrap());
            if visible == clip {
                return Err(Error::Graphics(
                    "SetOrigin cannot alias visible and clip regions".into(),
                ));
            }
            let bounds = port_layout::bounds_pointer(memory, port)?;
            let mut fields = memory.read(bounds, 8)?.to_vec();
            fields.extend_from_slice(&bytes[16..24]);
            for at in (0..16).step_by(2) {
                let delta = if (at / 2) % 2 == 0 { dv } else { dh };
                let value = i16::try_from(
                    i32::from(i16::from_be_bytes(fields[at..at + 2].try_into().unwrap())) + delta,
                )
                .map_err(|_| {
                    Error::Graphics("SetOrigin exceeds signed-word coordinate storage".into())
                })?;
                fields[at..at + 2].copy_from_slice(&value.to_be_bytes());
            }
            let region = crate::regions::read(memory, process, visible)?;
            let region = crate::region_geometry::translated(&region, dh, dv)?;
            let data = GuestPtr(memory.u32(GuestPtr(visible))?);
            let overlaps = |base: u32, len: usize| {
                u64::from(base) < u64::from(port.0) + PORT_SIZE as u64
                    && u64::from(port.0) < u64::from(base) + len as u64
            };
            if overlaps(data.0, region.len()) || overlaps(visible, 4) {
                return Err(Error::Graphics(
                    "SetOrigin region overlaps its port record".into(),
                ));
            }
            memory.validate_write(bounds, 8)?;
            memory.validate_write(offset(port, 16)?, 8)?;
            memory.validate_write(data, region.len())?;
            memory.write(data, &region)?;
            memory.write(bounds, &fields[..8])?;
            memory.write(offset(port, 16)?, &fields[8..])
        })();
        process.mem_error = old_error;
        result
    }
    pub fn move_port_to(&self, memory: &mut Memory, left: i16, top: i16) -> Result<()> {
        let port = self.current(memory)?;
        let target = port_layout::bounds_pointer(memory, port)?;
        let mut bytes = memory.read(target, 8)?.to_vec();
        bytes.extend_from_slice(memory.read(offset(port, 16)?, 8)?);
        let word = |i| i16::from_be_bytes(bytes[i..i + 2].try_into().unwrap());
        let new_top = word(8).wrapping_sub(top);
        let new_left = word(10).wrapping_sub(left);
        let bottom = new_top.wrapping_add(word(4).wrapping_sub(word(0)));
        let right = new_left.wrapping_add(word(6).wrapping_sub(word(2)));
        let mut bounds = [0; 8];
        for (i, v) in [new_top, new_left, bottom, right].into_iter().enumerate() {
            bounds[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        memory.write(target, &bounds)
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
        let field = offset(port, if background { 84 } else { 80 })?;
        if port_layout::color(memory, port)? {
            port_layout::bitmap(memory, port)?;
            let (rgb, index) = match color {
                33 => ([0u8; 6], 1u32),
                30 => ([255u8; 6], 0u32),
                _ => return Err(Error::Graphics("unsupported color constant".into())),
            };
            let rgb_field = offset(port, if background { 42 } else { 36 })?;
            memory.validate_write(field, 4)?;
            memory.validate_write(rgb_field, 6)?;
            memory.write(rgb_field, &rgb)?;
            memory.write(field, &index.to_be_bytes())
        } else {
            memory.write(field, &color.to_be_bytes())
        }
    }
    pub fn current(&self, memory: &Memory) -> Result<GuestPtr> {
        let port = GuestPtr(memory.u32(self.current_slot)?);
        if port.0 == 0 {
            return Err(Error::Graphics("no current QuickDraw port".into()));
        }
        memory.read(port, PORT_SIZE)?;
        Ok(port)
    }
    pub fn set(&self, memory: &mut Memory, port: GuestPtr) -> Result<()> {
        if port.0 != 0 {
            memory.read(port, PORT_SIZE)?;
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
        result: [u8; 4],
        to_global: bool,
    ) -> Result<[u8; 4]> {
        self.point_coordinates_for_port(memory, self.current(memory)?, result, to_global)
    }
    pub(crate) fn point_coordinates_for_port(
        &self,
        memory: &Memory,
        port: GuestPtr,
        mut result: [u8; 4],
        to_global: bool,
    ) -> Result<[u8; 4]> {
        memory.read(port, PORT_SIZE)?;
        let origin: [u8; 4] = memory
            .read(port_layout::bounds_pointer(memory, port)?, 4)?
            .try_into()
            .unwrap();
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
    pub fn pen_visibility(&self, memory: &mut Memory, delta: i16) -> Result<()> {
        let port = self.current(memory)?;
        let at = GuestPtr(port.0 + 66);
        let value = (memory.u16(at)? as i16).wrapping_add(delta);
        memory.write(at, &value.to_be_bytes())
    }
    pub(crate) fn draw_polygon(
        &self,
        memory: &mut Memory,
        gpu: Option<&Graphics>,
        vertices: &[(i16, i16)],
        verb: crate::polygons::Verb,
    ) -> Result<()> {
        use crate::polygons::Verb;
        let port = self.current(memory)?;
        let bytes = memory.read(port, PORT_SIZE)?.to_vec();
        let word = |at| i16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
        let frame = if verb == Verb::Frame {
            Some((word(54), word(52)))
        } else {
            None
        };
        if frame.is_some_and(|(w, h)| w < 0 || h < 0) {
            return Err(Error::Graphics("negative polygon pen size".into()));
        }
        if vertices.len() < if frame.is_some() { 2 } else { 3 }
            || frame.is_some_and(|(w, h)| w == 0 || h == 0 || word(66) < 0)
        {
            return Ok(());
        }
        let min_x = vertices.iter().map(|p| i32::from(p.0)).min().unwrap();
        let min_y = vertices.iter().map(|p| i32::from(p.1)).min().unwrap();
        let max_x = vertices.iter().map(|p| i32::from(p.0)).max().unwrap();
        let max_y = vertices.iter().map(|p| i32::from(p.1)).max().unwrap();
        let paint = match verb {
            Verb::Frame | Verb::Paint => None,
            Verb::Erase => Some(PatternPaint {
                pattern: port_layout::pattern(memory, port, PortPattern::Background)?,
                mode: 8,
                monochrome: false,
            }),
            Verb::Invert => Some(PatternPaint {
                pattern: [255; 8],
                mode: 10,
                monochrome: true,
            }),
            Verb::Fill(p) => {
                memory.validate_write(
                    port_layout::pattern_pointer(memory, port, PortPattern::Fill)?,
                    8,
                )?;
                Some(PatternPaint {
                    pattern: memory.read(p, 8)?.try_into().unwrap(),
                    mode: 8,
                    monochrome: false,
                })
            }
        };
        let (bitmap, _) = self.drawing_target(memory)?;
        let (w, h) = frame.unwrap_or((0, 0));
        let r = Rect::new(
            min_x.max(bitmap.bounds.left),
            min_y.max(bitmap.bounds.top),
            (max_x + i32::from(w)).min(bitmap.bounds.right),
            (max_y + i32::from(h)).min(bitmap.bounds.bottom),
        );
        if r.left < r.right && r.top < r.bottom {
            let gpu =
                gpu.ok_or_else(|| Error::Graphics("polygon drawing requires a GPU".into()))?;
            self.paint_geometry(gpu, memory, r, Coverage::Polygon { vertices, frame }, paint)?;
        }
        if matches!(verb, Verb::Fill(_)) {
            port_layout::write_pattern(memory, port, PortPattern::Fill, &paint.unwrap().pattern)?;
        }
        Ok(())
    }
    pub fn pen_normal(&self, memory: &mut Memory) -> Result<()> {
        let port = self.current(memory)?;
        let pattern = port_layout::pattern_pointer(memory, port, PortPattern::Pen)?;
        memory.validate_write(pattern, 8)?;
        memory.validate_write(offset(port, 52)?, 6)?;
        memory.write(pattern, &[255; 8])?;
        memory.write(offset(port, 52)?, &[0, 1, 0, 1, 0, 8])
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
        self.require_basic(memory)?;
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
        let source_data = crate::pixmap::source(memory, request.source)?;
        let source = source_data.bitmap;
        let destination = crate::pixmap::destination(memory, request.destination)?;
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
        let (mut clips, foreground, background) = self.copy_context(memory, destination)?;
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
        destination.execute_source_palette(
            gpu,
            memory,
            Draw::Blit {
                mask: None,
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
            Some(&source_data.indices),
            source_data.palette.as_deref(),
        )
    }
    pub fn copy_mask(&self, memory: &mut Memory, gpu: &Graphics, request: CopyMask) -> Result<()> {
        let source = Bitmap::read(memory, request.source)?;
        let mask = Bitmap::read(memory, request.mask)?;
        let destination = crate::pixmap::destination(memory, request.destination)?;
        let src = read_rect(memory, request.source_rect)?;
        let mr = read_rect(memory, request.mask_rect)?;
        let rect = read_rect(memory, request.destination_rect)?;
        if src.right - src.left != mr.right - mr.left || src.bottom - src.top != mr.bottom - mr.top
        {
            return Err(Error::Graphics(
                "CopyMask source and mask rectangle sizes differ".into(),
            ));
        }
        if src.left >= src.right
            || src.top >= src.bottom
            || rect.left >= rect.right
            || rect.top >= rect.bottom
        {
            return Ok(());
        }
        let (clips, foreground, background) = self.copy_context(memory, destination)?;
        let local = |r: Rect, b: Bitmap| {
            Rect::new(
                r.left - b.bounds.left,
                r.top - b.bounds.top,
                r.right - b.bounds.left,
                r.bottom - b.bounds.top,
            )
        };
        let indices = source.unpack(memory)?;
        let bits = mask.unpack(memory)?;
        let (width, height) = source.extent()?;
        let (mw, mh) = mask.extent()?;
        destination.execute_source(
            gpu,
            memory,
            Draw::Blit {
                mask: Some(crate::graphics::BlitMask {
                    rect: local(mr, mask),
                    width: mw as u32,
                    height: mh as u32,
                    bits: &bits,
                }),
                rect,
                clip: destination.bounds,
                source: local(src, source),
                width: width as u32,
                height: height as u32,
                foreground,
                background,
                invert: false,
                mode: Transfer::Copy,
            },
            Some(&clips),
            Some(&indices),
        )
    }
    fn copy_context(&self, memory: &Memory, destination: Bitmap) -> Result<(Vec<Rect>, u8, u8)> {
        let port = self.current(memory)?;
        let bytes = memory.read(port, PORT_SIZE)?;
        let word = |i| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|i| word(*i) != 0) {
            return Err(Error::Graphics(
                "bitmap copying recording or custom procedures are unsupported".into(),
            ));
        }
        let is_color = port_layout::color(memory, port)?;
        let color = |value| match value {
            30 => Ok(0),
            33 => Ok(1),
            0 | 1 if is_color => Ok(value as u8),
            _ => Err(Error::Graphics("unsupported monochrome port color".into())),
        };
        let foreground = color(word(80))?;
        let background = color(word(84))?;
        let current = port_layout::bitmap(memory, port)?;
        let clips = if current.base == destination.base {
            if current.row_bytes != destination.row_bytes {
                return Err(Error::Graphics(
                    "CopyBits destination aliases a different port stride".into(),
                ));
            }
            let (_, clips) = self.drawing_target_with_visibility(memory, true, false)?;
            let dx = destination.bounds.left - current.bounds.left;
            let dy = destination.bounds.top - current.bounds.top;
            clips
                .into_iter()
                .map(|r| Rect::new(r.left + dx, r.top + dy, r.right + dx, r.bottom + dy))
                .collect()
        } else {
            vec![destination.bounds]
        };
        Ok((clips, foreground, background))
    }
    pub fn line(
        &self,
        memory: &mut Memory,
        gpu: Option<&Graphics>,
        h: i16,
        v: i16,
        relative: bool,
    ) -> Result<()> {
        self.line_impl(memory, gpu, h, v, relative, false)
    }
    pub(crate) fn line_recorded(
        &self,
        memory: &mut Memory,
        gpu: Option<&Graphics>,
        h: i16,
        v: i16,
        relative: bool,
    ) -> Result<()> {
        self.line_impl(memory, gpu, h, v, relative, true)
    }
    fn line_impl(
        &self,
        memory: &mut Memory,
        gpu: Option<&Graphics>,
        h: i16,
        v: i16,
        relative: bool,
        recording: bool,
    ) -> Result<()> {
        let port = self.current(memory)?;
        let bytes = memory.read(port, PORT_SIZE)?;
        for i in [92, 96, 100, 104] {
            if u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) != 0
                && !(i == 100 && recording)
            {
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
            let (bitmap, _) = self.drawing_target_with_visibility(memory, false, recording)?;
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
                self.paint_geometry(
                    gpu,
                    memory,
                    clipped,
                    if recording {
                        Coverage::RecordingLine(geometry)
                    } else {
                        Coverage::Line(geometry)
                    },
                    None,
                )?;
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
                    pattern: port_layout::pattern(memory, port, PortPattern::Background)?,
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
                memory.validate_write(
                    port_layout::pattern_pointer(memory, port, PortPattern::Fill)?,
                    8,
                )?;
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
        self.paint_geometry(gpu, memory, r, Coverage::Shape(None), Some(paint))?;
        if fill {
            port_layout::write_pattern(memory, port, PortPattern::Fill, &paint.pattern)?;
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
        self.drawing_target_with_visibility(memory, false, false)
    }
    fn drawing_target_with_visibility(
        &self,
        memory: &Memory,
        force_visibility: bool,
        recording_polygon: bool,
    ) -> Result<(Bitmap, Vec<Rect>)> {
        let p = self.current(memory)?;
        let bytes = memory.read(p, PORT_SIZE)?;
        let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        if [92, 96, 100, 104]
            .iter()
            .any(|o| word(*o) != 0 && !(*o == 100 && recording_polygon))
        {
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
        let bitmap = port_layout::bitmap(memory, p)?;
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
        self.paint_geometry(gpu, memory, r, Coverage::Shape(shape), None)
    }
    fn paint_geometry(
        &self,
        gpu: &Graphics,
        memory: &mut Memory,
        r: Rect,
        coverage: Coverage<'_>,
        paint: Option<PatternPaint>,
    ) -> Result<()> {
        let p = self.current(memory)?;
        let bytes = memory.read(p, PORT_SIZE)?;
        let word = |o| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        if [92, 96, 100, 104].iter().any(|o| {
            word(*o) != 0 && !(*o == 100 && matches!(coverage, Coverage::RecordingLine(_)))
        }) {
            return Err(Error::Graphics(
                "QuickDraw recording or custom procedures are unsupported".into(),
            ));
        }
        // Empty rectangles cover no pixels, including while an application is
        // preparing an empty offscreen port whose bitmap has no storage yet.
        if r.left >= r.right || r.top >= r.bottom {
            return Ok(());
        }
        let mode = paint.map_or_else(
            || u16::from_be_bytes(bytes[56..58].try_into().unwrap()),
            |p| p.mode,
        );
        if !(8..=15).contains(&mode) {
            return Err(Error::Graphics("unsupported QuickDraw pen mode".into()));
        }
        let is_color = port_layout::color(memory, p)?;
        let color = |v| match v {
            33 => Ok(1),
            30 => Ok(0),
            0 | 1 if is_color => Ok(v as u8),
            _ => Err(Error::Graphics("unsupported monochrome port color".into())),
        };
        let (foreground, background) = if paint.is_some_and(|p| p.monochrome) {
            (1, 0)
        } else {
            (color(word(80))?, color(word(84))?)
        };
        let mut pattern: [u8; 8] = match paint {
            Some(p) => p.pattern,
            None => port_layout::pattern(memory, p, PortPattern::Pen)?,
        };
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
        let (bitmap, mut clips) = self.drawing_target_with_visibility(
            memory,
            false,
            matches!(coverage, Coverage::RecordingLine(_)),
        )?;
        let intersect = |a: Rect, b: Rect| {
            Rect::new(
                a.left.max(b.left),
                a.top.max(b.top),
                a.right.min(b.right),
                a.bottom.min(b.bottom),
            )
        };
        if let Coverage::Shape(Some(shape)) = coverage {
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
            if let Coverage::Polygon { vertices, frame } = coverage {
                Draw::Polygon {
                    rect: r,
                    clip: port_rect,
                    vertices,
                    origin_x: 0,
                    origin_y: 0,
                    frame,
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
            } else if let Coverage::Line(line) | Coverage::RecordingLine(line) = coverage {
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
