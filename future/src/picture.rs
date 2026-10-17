//! Bounded PICT bitmap commands. CPU work decodes storage; wgpu transfers pixels.
use crate::{
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::{read_rect, Ports},
    Error, Result,
};
mod direct;
const LIMIT: usize = 16 * 1024 * 1024;
fn bad(message: &str) -> Error {
    Error::Graphics(format!("PICT: {message}"))
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| bad("offset overflow"))?;
        let b = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| bad("truncated stream"))?;
        self.at = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn rect(&mut self) -> Result<Rect> {
        let b = self.take(8)?;
        let w = |i| i16::from_be_bytes(b[i..i + 2].try_into().unwrap()) as i32;
        Ok(Rect::new(w(2), w(0), w(6), w(4)))
    }
    fn region(&mut self) -> Result<Vec<Rect>> {
        let n = usize::from(self.word()?);
        if !(10..=32766).contains(&n) || !n.is_multiple_of(2) {
            return Err(bad("invalid clip size"));
        }
        let mut bytes = (n as u16).to_be_bytes().to_vec();
        bytes.extend(self.take(n - 2)?);
        crate::region_geometry::decode(&bytes)
    }
}
#[derive(Debug)]
pub struct BitmapCommand {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
    pub rgb: Option<Vec<[u16; 3]>>,
    pub palette: Option<Box<[[u16; 3]; 256]>>,
    pub source: Rect,
    pub destination: Rect,
    pub clips: Option<Vec<Rect>>,
    pub mode: u16,
    foreground: Option<u8>,
    background: Option<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Highlight {
    #[default]
    Default,
    Rgb([u16; 3]),
}
#[derive(Debug)]
pub struct Picture {
    pub highlight: Highlight,
    pub frame: Rect,
    pub commands: Vec<BitmapCommand>,
}
fn positive(r: Rect) -> bool {
    r.left < r.right && r.top < r.bottom
}
fn color(bytes: &[u8]) -> Result<u8> {
    match u32::from_be_bytes(bytes.try_into().unwrap()) {
        30 => Ok(0),
        33 => Ok(1),
        _ => Err(bad("non-monochrome color")),
    }
}
fn unpack(input: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut r = Reader {
        bytes: input,
        at: 0,
    };
    let mut out = Vec::with_capacity(expected);
    while r.at < input.len() {
        let control = r.byte()? as i8;
        match control {
            0..=127 => {
                let n = control as usize + 1;
                if out.len() + n > expected {
                    return Err(bad("PackBits row overflow"));
                }
                out.extend(r.take(n)?);
            }
            -127..=-1 => {
                let n = (1 - i16::from(control)) as usize;
                if out.len() + n > expected {
                    return Err(bad("PackBits row overflow"));
                }
                let value = r.byte()?;
                out.resize(out.len() + n, value);
            }
            -128 => {}
        }
    }
    if out.len() != expected {
        return Err(bad("PackBits row length"));
    }
    Ok(out)
}
pub fn decode(bytes: &[u8]) -> Result<Picture> {
    if bytes.len() > LIMIT {
        return Err(bad("stream limit"));
    }
    let mut r = Reader { bytes, at: 0 };
    r.word()?; // picSize may be zero or truncated for large pictures.
    let frame = r.rect()?;
    if !positive(frame) {
        return Err(bad("empty picture frame"));
    }
    let v2 = match r.byte()? {
        0x11 => {
            if r.byte()? != 1 {
                return Err(bad("picture version"));
            }
            false
        }
        0 => {
            if r.byte()? != 0x11 || r.word()? != 0x02ff {
                return Err(bad("picture version"));
            }
            true
        }
        _ => return Err(bad("missing version")),
    };
    let mut highlight = Highlight::Default;
    let mut commands = Vec::new();
    let mut clips = None;
    let mut foreground = None;
    let mut background = None;
    let mut expanded = 0usize;
    for _ in 0..4096 {
        if v2 && !r.at.is_multiple_of(2) {
            r.byte()?;
        }
        let opcode = if v2 { r.word()? } else { u16::from(r.byte()?) };
        match opcode {
            0 => {}
            0xff => {
                return Ok(Picture {
                    frame,
                    commands,
                    highlight,
                })
            }
            1 => clips = Some(r.region()?),
            0x0e => foreground = Some(color(r.take(4)?)?),
            0x0f => background = Some(color(r.take(4)?)?),
            0x1e if v2 => highlight = Highlight::Default,
            0x1d if v2 => {
                let b = r.take(6)?;
                highlight = Highlight::Rgb(std::array::from_fn(|i| {
                    u16::from_be_bytes([b[i * 2], b[i * 2 + 1]])
                }));
            }
            0xa0 => {
                r.take(2)?;
            }
            0xa1 => {
                r.take(2)?;
                let n = usize::from(r.word()?);
                r.take(n)?;
            }
            0x0c00 if v2 => {
                r.take(24)?;
            }
            0x9a | 0x9b if v2 => {
                let command = direct::decode(&mut r, opcode, &clips)?;
                let count = command.rgb.as_ref().unwrap().len() * 6
                    + command.clips.as_ref().map_or(0, |c| c.len() * 16);
                expanded = expanded
                    .checked_add(count)
                    .ok_or_else(|| bad("decoded size overflow"))?;
                if expanded > LIMIT {
                    return Err(bad("decoded command limit"));
                }
                commands.push(command);
            }
            0x90 | 0x91 | 0x98 | 0x99 => {
                let row_word = r.word()?;
                let indexed = row_word & 0x8000 != 0;
                let stride = usize::from(row_word & 0x3fff);
                if stride == 0 || row_word & 0x4000 != 0 {
                    return Err(bad("invalid bitmap stride/flags"));
                }
                if indexed && !v2 {
                    return Err(bad("indexed PixMap requires version 2"));
                }
                let bounds = r.rect()?;
                let mut palette = None;
                let mut defined = [false; 256];
                let mut pack_type = 0;
                let mut depth = 1;
                if indexed {
                    let version = r.word()?;
                    pack_type = r.word()?;
                    r.take(12)?; // packSize, horizontal and vertical resolution
                    let pixel_type = r.word()?;
                    depth = r.word()?;
                    let components = r.word()?;
                    let component_size = r.word()?;
                    let plane = u32::from_be_bytes(r.take(4)?.try_into().unwrap());
                    r.take(8)?; // serialized table pointer/reserved
                    if version != 0
                        || pack_type > 1
                        || pixel_type != 0
                        || ![1, 2, 4, 8].contains(&depth)
                        || components != 1
                        || component_size != depth
                        || plane != 0
                    {
                        return Err(bad("unsupported indexed PixMap layout"));
                    }
                    r.take(4)?;
                    let flags = r.word()?;
                    let count = usize::from(r.word()?) + 1;
                    if count > 256 {
                        return Err(bad("color table entry limit"));
                    }
                    let mut table = Box::new([[65535u16; 3]; 256]);
                    for ordinal in 0..count {
                        let value = usize::from(r.word()?);
                        let index = if flags & 0x8000 != 0 { ordinal } else { value };
                        if index >= 256 || defined[index] {
                            return Err(bad("invalid or duplicate color table index"));
                        }
                        defined[index] = true;
                        let b = r.take(6)?;
                        table[index] =
                            std::array::from_fn(|i| u16::from_be_bytes([b[i * 2], b[i * 2 + 1]]));
                    }
                    palette = Some(table);
                }
                let width = bounds.right - bounds.left;
                let height = bounds.bottom - bounds.top;
                if width <= 0
                    || height <= 0
                    || width > 4096
                    || height > 4096
                    || width as usize > stride * 8 / usize::from(depth)
                {
                    return Err(bad("bitmap extent"));
                }
                expanded = expanded
                    .checked_add(width as usize * height as usize + if indexed { 1536 } else { 0 })
                    .ok_or_else(|| bad("decoded size overflow"))?;
                if expanded > LIMIT || stride * height as usize > LIMIT {
                    return Err(bad("decoded bitmap limit"));
                }
                let mut source = r.rect()?;
                let destination = r.rect()?;
                let mode = r.word()?;
                if mode & !0x47 != 0 || !positive(source) || !positive(destination) {
                    return Err(bad("bitmap rectangles or transfer mode"));
                }
                source.left -= bounds.left;
                source.right -= bounds.left;
                source.top -= bounds.top;
                source.bottom -= bounds.top;
                if source.right - source.left > 4096 || source.bottom - source.top > 4096 {
                    return Err(bad("source rectangle limit"));
                }
                let mask = if opcode & 1 != 0 {
                    Some(r.region()?)
                } else {
                    None
                };
                let command_clip = match (&clips, mask) {
                    (Some(a), Some(b)) => {
                        if a.len().saturating_mul(b.len()) > 65536 {
                            return Err(bad("mask intersection limit"));
                        }
                        Some(
                            a.iter()
                                .flat_map(|a| {
                                    b.iter().map(|b| {
                                        Rect::new(
                                            a.left.max(b.left),
                                            a.top.max(b.top),
                                            a.right.min(b.right),
                                            a.bottom.min(b.bottom),
                                        )
                                    })
                                })
                                .filter(|r| positive(*r))
                                .collect(),
                        )
                    }
                    (Some(a), None) => Some(a.clone()),
                    (None, b) => b,
                };
                expanded = expanded
                    .checked_add(command_clip.as_ref().map_or(0, |r| r.len() * 16))
                    .ok_or_else(|| bad("clip storage overflow"))?;
                if expanded > LIMIT {
                    return Err(bad("decoded command limit"));
                }
                let mut indices = Vec::with_capacity(width as usize * height as usize);
                for _ in 0..height {
                    let row = if opcode >= 0x98 && stride >= 8 && pack_type != 1 {
                        let n = if stride > 250 {
                            usize::from(r.word()?)
                        } else {
                            usize::from(r.byte()?)
                        };
                        unpack(r.take(n)?, stride)?
                    } else {
                        r.take(stride)?.to_vec()
                    };
                    if indexed {
                        for x in 0..width as usize {
                            let bit = x * usize::from(depth);
                            let value = (row[bit / 8] >> (8 - usize::from(depth) - bit % 8))
                                & ((1u16 << depth) - 1) as u8;
                            if !defined[usize::from(value)] {
                                return Err(bad("pixel references undefined color table index"));
                            }
                            indices.push(value);
                        }
                    } else {
                        for x in 0..width as usize {
                            indices.push((row[x / 8] >> (7 - x % 8)) & 1);
                        }
                    }
                }
                commands.push(BitmapCommand {
                    width: width as u32,
                    height: height as u32,
                    indices,
                    rgb: None,
                    palette,
                    source,
                    destination,
                    clips: command_clip,
                    mode,
                    foreground,
                    background,
                });
            }
            _ => return Err(bad(&format!("unsupported opcode ${opcode:04X}"))),
        }
    }
    Err(bad("opcode limit or missing EndPic"))
}
fn mapped(r: Rect, frame: Rect, destination: Rect) -> Result<Rect> {
    let axis = |v: i32, start: i32, end: i32, to: i32, finish: i32| -> Result<i32> {
        let n = i64::from(to)
            + (i64::from(v) - i64::from(start)) * i64::from(finish - to) / i64::from(end - start);
        let n = i32::try_from(n).map_err(|_| bad("mapped coordinate overflow"))?;
        if !(-32768..=32767).contains(&n) {
            return Err(bad("mapped coordinate range"));
        }
        Ok(n)
    };
    Ok(Rect::new(
        axis(
            r.left,
            frame.left,
            frame.right,
            destination.left,
            destination.right,
        )?,
        axis(
            r.top,
            frame.top,
            frame.bottom,
            destination.top,
            destination.bottom,
        )?,
        axis(
            r.right,
            frame.left,
            frame.right,
            destination.left,
            destination.right,
        )?,
        axis(
            r.bottom,
            frame.top,
            frame.bottom,
            destination.top,
            destination.bottom,
        )?,
    ))
}
pub fn draw(
    memory: &mut Memory,
    process: &mut Process,
    ports: Ports,
    gpu: &Graphics,
    handle: u32,
    destination: GuestPtr,
) -> Result<()> {
    if handle == 0 {
        return Ok(());
    }
    let size = process.handle_size(memory, handle)? as usize;
    if !(12..=LIMIT).contains(&size) {
        return Err(bad("invalid picture handle size"));
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    let picture = decode(memory.read(data, size)?)?;
    let destination = read_rect(memory, destination)?;
    if !positive(destination) {
        return Ok(());
    }
    let (bitmap, port_clips) = ports.drawing_target(memory)?;
    // Picture playback has its own initial color state. Recorded color opcodes
    // override these defaults without reading or changing the caller's colors.
    let (foreground, background) = (bitmap.nearest([0; 3])?, bitmap.nearest([65535; 3])?);
    let mut prepared = Vec::new();
    for command in &picture.commands {
        let rect = mapped(command.destination, picture.frame, destination)?;
        let clips = if let Some(clips) = &command.clips {
            if clips.len().saturating_mul(port_clips.len()) > 65536 {
                return Err(bad("drawing clip limit"));
            }
            let mapped = clips
                .iter()
                .map(|r| mapped(*r, picture.frame, destination))
                .collect::<Result<Vec<_>>>()?;
            port_clips
                .iter()
                .flat_map(|a| {
                    mapped.iter().map(|b| {
                        Rect::new(
                            a.left.max(b.left),
                            a.top.max(b.top),
                            a.right.min(b.right),
                            a.bottom.min(b.bottom),
                        )
                    })
                })
                .filter(|r| positive(*r))
                .collect()
        } else {
            port_clips.clone()
        };
        prepared.push((rect, clips));
    }
    for (command, (rect, clips)) in picture.commands.iter().zip(prepared) {
        let draw = Draw::Blit {
            mask: None,
            rect,
            clip: bitmap.bounds,
            source: command.source,
            width: command.width,
            height: command.height,
            foreground: command.foreground.map_or(Ok(foreground), |v| {
                bitmap.nearest(if v == 0 { [65535; 3] } else { [0; 3] })
            })?,
            background: command.background.map_or(Ok(background), |v| {
                bitmap.nearest(if v == 0 { [65535; 3] } else { [0; 3] })
            })?,
            invert: command.mode & 4 != 0,
            mode: match command.mode & 3 {
                0 => Transfer::Copy,
                1 => Transfer::Or,
                2 => Transfer::Xor,
                _ => Transfer::Bic,
            },
        };
        if command.mode & 0x40 != 0 {
            let decoded;
            let rgb = if let Some(rgb) = &command.rgb {
                rgb.as_slice()
            } else {
                decoded = command
                    .indices
                    .iter()
                    .map(|&i| {
                        command
                            .palette
                            .as_ref()
                            .map_or(if i == 0 { [65535; 3] } else { [0; 3] }, |p| p[i as usize])
                    })
                    .collect::<Vec<_>>();
                &decoded
            };
            bitmap.execute_dithered_rgb(gpu, memory, draw, Some(&clips), rgb)?;
        } else if let Some(rgb) = &command.rgb {
            bitmap.execute_rgb(gpu, memory, draw, Some(&clips), rgb)?;
        } else {
            bitmap.execute_source_palette(
                gpu,
                memory,
                draw,
                Some(&clips),
                Some(&command.indices),
                command.palette.as_deref(),
            )?;
        }
    }
    Ok(())
}
