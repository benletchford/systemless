//! Shared GrafPort/CGrafPort field access. Common geometry/text/pen fields keep
//! their classic offsets; bitmaps and pattern storage differ between layouts.
use crate::{
    bitmap::Bitmap,
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Clone, Copy)]
pub(crate) enum Pattern {
    Background,
    Pen,
    Fill,
}
fn bad() -> Error {
    Error::Graphics("invalid or unsupported color port layout".into())
}
pub(crate) fn color(memory: &Memory, port: GuestPtr) -> Result<bool> {
    memory.read(port, 108)?;
    match memory.u16(GuestPtr(port.0 + 6))? & 0xc000 {
        0 => Ok(false),
        0xc000 => Ok(true),
        _ => Err(bad()),
    }
}
pub(crate) fn pixmap(memory: &Memory, port: GuestPtr) -> Result<GuestPtr> {
    let handle = memory.u32(GuestPtr(port.0 + 2))?;
    if handle == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(handle))?;
    if data == 0 {
        return Err(bad());
    }
    memory.read(GuestPtr(data), 50)?;
    Ok(GuestPtr(data))
}
pub(crate) fn bounds_pointer(memory: &Memory, port: GuestPtr) -> Result<GuestPtr> {
    Ok(if color(memory, port)? {
        let map = pixmap(memory, port)?;
        GuestPtr(map.0.checked_add(6).ok_or_else(bad)?)
    } else {
        GuestPtr(port.0 + 8)
    })
}
pub(crate) fn bitmap(memory: &Memory, port: GuestPtr) -> Result<crate::surface::Surface> {
    if color(memory, port)? {
        crate::pixmap::destination(memory, pixmap(memory, port)?)
    } else {
        Bitmap::read(memory, GuestPtr(port.0 + 2))?.surface(memory)
    }
}
pub(crate) fn pattern_pointer(
    memory: &Memory,
    port: GuestPtr,
    pattern: Pattern,
) -> Result<GuestPtr> {
    if !color(memory, port)? {
        return Ok(GuestPtr(
            port.0
                + match pattern {
                    Pattern::Background => 32,
                    Pattern::Pen => 58,
                    Pattern::Fill => 40,
                },
        ));
    }
    let at = match pattern {
        Pattern::Background => 32,
        Pattern::Pen => 58,
        Pattern::Fill => 62,
    };
    let handle = memory.u32(GuestPtr(port.0 + at))?;
    if handle == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(handle))?;
    if data == 0 {
        return Err(bad());
    }
    let bytes = memory.read(GuestPtr(data), 28)?;
    if bytes[..20].iter().any(|byte| *byte != 0) {
        return Err(Error::Graphics(
            "expanded/color pixel patterns are not implemented".into(),
        ));
    }
    Ok(GuestPtr(data.checked_add(20).ok_or_else(bad)?))
}
pub(crate) fn pattern(memory: &Memory, port: GuestPtr, kind: Pattern) -> Result<[u8; 8]> {
    Ok(memory
        .read(pattern_pointer(memory, port, kind)?, 8)?
        .try_into()
        .unwrap())
}
pub(crate) fn write_pattern(
    memory: &mut Memory,
    port: GuestPtr,
    kind: Pattern,
    pattern: &[u8],
) -> Result<()> {
    if pattern.len() != 8 {
        return Err(bad());
    }
    let pointer = pattern_pointer(memory, port, kind)?;
    memory.write(pointer, pattern)
}

pub(crate) fn colors(
    memory: &Memory,
    port: GuestPtr,
    target: &crate::surface::Surface,
) -> Result<(u8, u8)> {
    let resolve = |at| {
        let value = memory.u32(GuestPtr(port.0 + at))?;
        if color(memory, port)? {
            target.index(value)
        } else {
            match value {
                33 => target.nearest([0; 3]),
                30 => target.nearest([65535; 3]),
                _ => Err(Error::Graphics("unsupported basic port color".into())),
            }
        }
    };
    Ok((resolve(80)?, resolve(84)?))
}

pub(crate) fn selected_surface(memory: &Memory) -> Result<crate::surface::Surface> {
    let h = memory.u32(crate::devices::CURRENT)?;
    if h == 0 {
        return Err(bad());
    }
    let data = memory.u32(GuestPtr(h))?;
    if data == 0 {
        return Err(bad());
    }
    let record = memory.read(GuestPtr(data), 62)?;
    if record[12..20].iter().any(|v| *v != 0) {
        return Err(Error::Graphics(
            "custom color search/complement procedures are unsupported".into(),
        ));
    }
    let map_h = memory.u32(GuestPtr(data.checked_add(22).ok_or_else(bad)?))?;
    if map_h == 0 {
        return Err(bad());
    }
    let map = memory.u32(GuestPtr(map_h))?;
    if map == 0 {
        return Err(bad());
    }
    crate::pixmap::destination(memory, GuestPtr(map))
}
