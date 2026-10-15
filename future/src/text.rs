//! Basic GrafPort text selection; font resolution and drawing are separate.
use crate::{
    graphics::Rect,
    memory::{GuestPtr, Memory},
    quickdraw::Ports,
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Font,
    Face,
    Mode,
    Size,
}
pub fn set(memory: &mut Memory, ports: Ports, field: Field, value: u16) -> Result<()> {
    let port = ports.current(memory)?;
    if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0 {
        return Err(Error::Graphics(
            "text selection in color ports is unsupported".into(),
        ));
    }
    if memory.u32(GuestPtr(port.0 + 92))? != 0 {
        return Err(Error::Graphics(
            "text selection during picture recording is unsupported".into(),
        ));
    }
    let offset = match field {
        Field::Font => 68,
        Field::Face => 70,
        Field::Mode => 72,
        Field::Size => 74,
    };
    if field == Field::Face {
        memory.write(GuestPtr(port.0 + offset), &[value as u8])
    } else {
        memory.write(GuestPtr(port.0 + offset), &value.to_be_bytes())
    }
}

fn font_data(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    port: GuestPtr,
) -> Result<Vec<u8>> {
    if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0 {
        return Err(Error::Graphics(
            "font metrics in color ports are unsupported".into(),
        ));
    }
    font_bytes(
        memory,
        process,
        memory.u16(GuestPtr(port.0 + 68))? as i16,
        memory.u16(GuestPtr(port.0 + 74))? as i16,
        memory.read(GuestPtr(port.0 + 70), 1)?[0],
    )
}
pub(crate) fn font_bytes(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    mut family: i16,
    mut size: i16,
    face: u8,
) -> Result<Vec<u8>> {
    if face > 1 {
        return Err(Error::Graphics(
            "styled font metrics are not implemented".into(),
        ));
    }
    if family == 0 {
        family = memory.u16(GuestPtr(0xba6))? as i16;
    } else if family == 1 {
        family = memory.u16(GuestPtr(0x984))? as i16;
    }
    if size == 0 {
        size = memory.u16(GuestPtr(0xba8))? as i16;
    }
    if family < 0 || size <= 0 || size >= 128 {
        return Err(Error::Graphics("invalid font family or size".into()));
    }
    let id = i16::try_from(i32::from(family) * 128 + i32::from(size))
        .map_err(|_| Error::Graphics("FONT family/size resource ID overflow".into()))?;
    let handle = if process.resources.get(*b"FONT", id).is_some() {
        process.get_resource(memory, *b"FONT", id)?
    } else {
        if !process.fonts.initialized {
            return Err(Error::Graphics(
                "Font Manager has not been initialized".into(),
            ));
        }
        if !matches!(family, 0 | 3 | 21) {
            return Err(Error::Graphics(
                "selected font family has no resource or fallback".into(),
            ));
        }
        crate::font::fallback(memory, process, size)?
    };
    let length = process.handle_size(memory, handle)? as usize;
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    let bytes = memory.read(data, length)?;
    if face == 1 {
        crate::font::embolden(bytes)
    } else {
        Ok(bytes.to_vec())
    }
}
pub fn font_info(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    ports: Ports,
    destination: GuestPtr,
) -> Result<()> {
    memory.validate_write(destination, 8)?;
    let metrics = metrics(memory, process, ports)?;
    let bytes: Vec<_> = [
        metrics.ascent,
        metrics.descent,
        metrics.maximum_width,
        metrics.leading,
    ]
    .into_iter()
    .flat_map(i16::to_be_bytes)
    .collect();
    memory.write(destination, &bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Character(u8),
    Buffer {
        pointer: GuestPtr,
        first: i16,
        count: i16,
    },
    Pascal(GuestPtr),
}
fn input(memory: &Memory, input: Input) -> Result<Vec<u8>> {
    match input {
        Input::Character(c) => Ok(vec![c]),
        Input::Buffer {
            pointer,
            first,
            count,
        } => {
            if count <= 0 {
                return Ok(vec![]);
            }
            let address = pointer
                .0
                .checked_add_signed(i32::from(first))
                .ok_or_else(|| Error::Graphics("text buffer offset overflow".into()))?;
            Ok(memory.read(GuestPtr(address), count as usize)?.to_vec())
        }
        Input::Pascal(pointer) => {
            let n = usize::from(memory.read(pointer, 1)?[0]);
            let address = pointer
                .0
                .checked_add(1)
                .ok_or_else(|| Error::Graphics("text pointer overflow".into()))?;
            Ok(memory.read(GuestPtr(address), n)?.to_vec())
        }
    }
}
pub fn width(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    ports: Ports,
    text: Input,
) -> Result<i16> {
    let bytes = input(memory, text)?;
    if bytes.is_empty() {
        return Ok(0);
    }

    let port = ports.current(memory)?;
    if memory.u32(GuestPtr(port.0 + 76))? != 0 {
        return Err(Error::Graphics(
            "fractional space extra is unsupported".into(),
        ));
    }
    let data = font_data(memory, process, port)?;
    Ok(crate::font::Strike::parse(&data)?.width(&bytes)? as i16)
}
pub fn move_to(memory: &mut Memory, ports: Ports, h: i16, v: i16) -> Result<()> {
    let port = ports.current(memory)?;
    if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0 {
        return Err(Error::Graphics(
            "MoveTo in color ports is unsupported".into(),
        ));
    }
    for offset in [92, 96, 100] {
        if memory.u32(GuestPtr(port.0 + offset))? != 0 {
            return Err(Error::Graphics(
                "MoveTo while recording is unsupported".into(),
            ));
        }
    }
    let mut bytes = v.to_be_bytes().to_vec();
    bytes.extend(h.to_be_bytes());
    memory.write(GuestPtr(port.0 + 48), &bytes)
}
pub fn draw(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    ports: Ports,
    gpu: &crate::graphics::Graphics,
    text: Input,
) -> Result<()> {
    let bytes = input(memory, text)?;
    if bytes.is_empty() {
        return Ok(());
    }

    let port = ports.current(memory)?;
    let font_bytes = font_data(memory, process, port)?;
    let font = crate::font::Strike::parse(&font_bytes)?;
    draw_strike(memory, ports, gpu, &bytes, &font)
}
pub(crate) fn draw_strike(
    memory: &mut Memory,
    ports: Ports,
    gpu: &crate::graphics::Graphics,
    bytes: &[u8],
    font: &crate::font::Strike<'_>,
) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    let port = ports.current(memory)?;
    if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0 {
        return Err(Error::Graphics(
            "text drawing in color ports is unsupported".into(),
        ));
    }
    memory.validate_write(port, 108)?;
    let saved = memory.read(port, 108)?.to_vec();
    if [92usize, 96, 100, 104]
        .iter()
        .any(|offset| saved[*offset..*offset + 4] != [0; 4])
    {
        return Err(Error::Graphics(
            "text recording or custom procedures are unsupported".into(),
        ));
    }
    if memory.u32(GuestPtr(port.0 + 76))? != 0 {
        return Err(Error::Graphics(
            "fractional space extra is unsupported".into(),
        ));
    }
    let mode = memory.u16(GuestPtr(port.0 + 72))?;
    if mode > 7 {
        return Err(Error::Graphics("unsupported text transfer mode".into()));
    }
    let v = memory.u16(GuestPtr(port.0 + 48))? as i16;
    let mut h = memory.u16(GuestPtr(port.0 + 50))? as i16;
    let (plans, end) = plan_strike(font, bytes, h, v)?;
    h = end;
    let result = (|| {
        memory.write(GuestPtr(port.0 + 56), &(8 + mode).to_be_bytes())?;
        for (bounds, ink, paper) in plans {
            for (pattern, coverage) in [(0u8, paper), (255u8, ink)] {
                if coverage.is_empty() {
                    continue;
                }
                memory.write(GuestPtr(port.0 + 58), &[pattern; 8])?;
                ports.paint_coverage(gpu, memory, bounds, Some(&coverage))?;
            }
        }
        Ok(())
    })();
    memory.write(port, &saved)?;
    result?;
    memory.write(GuestPtr(port.0 + 50), &h.to_be_bytes())
}

pub fn metrics(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    ports: Ports,
) -> Result<crate::font::Metrics> {
    let port = ports.current(memory)?;
    let data = font_data(memory, process, port)?;
    Ok(crate::font::Strike::parse(&data)?.metrics)
}

pub(crate) type GlyphCoverage = (Rect, Vec<Rect>, Vec<Rect>);
/// Shared source glyph geometry; destination transfers stay in the GPU services.
pub(crate) fn plan_strike(
    font: &crate::font::Strike<'_>,
    bytes: &[u8],
    mut h: i16,
    v: i16,
) -> Result<(Vec<GlyphCoverage>, i16)> {
    let mut plans = Vec::new();
    let mut work = 0usize;
    for &c in bytes {
        let g = font.glyph(c);
        let mask = font.mask(c)?;
        let gx = i32::from(h) + g.bearing;
        let left = i32::from(h).min(gx);
        let right = (i32::from(h) + i32::from(g.advance)).max(gx + i32::from(g.width));
        let top = i32::from(v) + g.top;
        let bottom = top + i32::from(g.height);
        if [left, right, top, bottom]
            .iter()
            .any(|n| i16::try_from(*n).is_err())
        {
            return Err(Error::Graphics("text drawing coordinate overflow".into()));
        }
        work = work.saturating_add((right - left) as usize * usize::from(g.height));
        if work > 1_000_000 {
            return Err(Error::Graphics("text glyph work limit exceeded".into()));
        }
        let mut ink = Vec::new();
        let mut paper = Vec::new();
        for y in 0..usize::from(g.height) {
            let mut x = left;
            while x < right {
                let bit = |x: i32| {
                    x >= gx
                        && x < gx + i32::from(g.width)
                        && mask[y * usize::from(g.width) + (x - gx) as usize] != 0
                };
                let value = bit(x);
                let start = x;
                x += 1;
                while x < right && bit(x) == value {
                    x += 1;
                }
                let rows = if value { &mut ink } else { &mut paper };
                rows.push(Rect::new(start, top + y as i32, x, top + y as i32 + 1));
            }
        }
        plans.push((Rect::new(left, top, right, bottom), ink, paper));
        h = h.wrapping_add(i16::from(g.advance));
    }
    Ok((plans, h))
}
