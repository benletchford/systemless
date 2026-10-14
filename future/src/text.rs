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
