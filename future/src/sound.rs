//! Shared Sound Driver volume, channel ownership and sampled immediate playback.
//! Inside Macintosh II, pp. 232–236; MPW LowMem.h identifies SdVolume at $0260.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
const SD_VOLUME: GuestPtr = GuestPtr(0x0260);
pub fn volume(memory: &Memory) -> Result<u16> {
    Ok(u16::from(memory.read(SD_VOLUME, 1)?[0] & 7))
}
pub fn set_volume(memory: &mut Memory, level: i16) -> Result<()> {
    if !(0..=7).contains(&level) {
        return Err(Error::Cpu(format!(
            "unsupported Sound Driver volume {level}"
        )));
    }
    let old = memory.read(SD_VOLUME, 1)?[0];
    memory.write(SD_VOLUME, &[(old & !7) | level as u8])
}
pub fn get_volume(memory: &mut Memory, destination: GuestPtr) -> Result<()> {
    let level = volume(memory)?;
    memory.write(destination, &level.to_be_bytes())
}
/// Synchronous/immediate Control(-4, 2): inline volume word at csParam.
/// Other requests fail explicitly until their driver semantics are implemented.
pub fn control(memory: &mut Memory, pointer: GuestPtr) -> Result<i16> {
    let pb = memory.read(pointer, 30)?;
    let reference = i16::from_be_bytes(pb[24..26].try_into().unwrap());
    let code = i16::from_be_bytes(pb[26..28].try_into().unwrap());
    let level = i16::from_be_bytes(pb[28..30].try_into().unwrap());
    if reference != -4 || code != 2 {
        return Err(Error::Cpu(format!(
            "unsupported driver control: reference {reference}, code {code}"
        )));
    }
    let result = GuestPtr(
        pointer
            .0
            .checked_add(16)
            .ok_or_else(|| Error::Cpu("control parameter overflow".into()))?,
    );
    memory.validate_write(result, 2)?;
    set_volume(memory, level)?;
    memory.write(result, &0i16.to_be_bytes())?;
    Ok(0)
}

mod channels;
pub(crate) use channels::Play;
pub use channels::{Manager, NewChannel, Op, CHANNEL_SIZE};

/// Virtual Sound Manager ABI target, not the Systemless package version.
/// Features are queried independently; codecs/recording/double buffering are not advertised.
pub const VERSION: u32 = 0x03008000;
pub const ATTRIBUTES: u32 = (1 << 0) | (1 << 11) | (1 << 12);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub channel: Option<u32>,
    pub length: i16,
    pub destination: GuestPtr,
}

/// Complete request shape; unsupported disk/selection/completion paths fail explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartFilePlay {
    pub channel: u32,
    pub file: i16,
    pub resource: i16,
    pub buffer_size: i32,
    pub buffer: GuestPtr,
    pub selection: GuestPtr,
    pub completion: u32,
    pub asynchronous: bool,
}
