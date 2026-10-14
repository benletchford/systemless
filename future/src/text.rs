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
    let family = memory.u16(GuestPtr(port.0 + 68))? as i16;
    let size = memory.u16(GuestPtr(port.0 + 74))? as i16;
    if family < 2 || size <= 0 || size >= 128 || memory.read(GuestPtr(port.0 + 70), 1)?[0] != 0 {
        return Err(Error::Graphics(
            "default, styled or scaled font metrics are not implemented".into(),
        ));
    }
    let id = i16::try_from(i32::from(family) * 128 + i32::from(size))
        .map_err(|_| Error::Graphics("FONT family/size resource ID overflow".into()))?;
    let handle = process.get_resource(memory, *b"FONT", id)?;
    if handle == 0 {
        return Err(Error::Graphics(
            "selected FONT strike is unavailable".into(),
        ));
    }
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
