//! Basic GrafPort text selection; font resolution and drawing are separate.
use crate::{
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

/// Resolve a plain old-style FONT strike by family*128+pointSize. Guest handle
/// bytes are reread on every call; resource mutation must not leave stale metrics.
pub fn font_info(
    memory: &mut Memory,
    process: &mut crate::process::Process,
    ports: Ports,
    destination: GuestPtr,
) -> Result<()> {
    memory.validate_write(destination, 8)?;
    let port = ports.current(memory)?;
    if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0 {
        return Err(Error::Graphics(
            "font metrics in color ports are unsupported".into(),
        ));
    }
    let mut family = memory.u16(GuestPtr(port.0 + 68))? as i16;
    let mut size = memory.u16(GuestPtr(port.0 + 74))? as i16;
    if memory.read(GuestPtr(port.0 + 70), 1)?[0] != 0 {
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
    let metrics = crate::font::Strike::parse(memory.read(data, length)?)?.metrics;
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
