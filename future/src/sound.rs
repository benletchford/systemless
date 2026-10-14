//! Legacy Sound Driver volume state. Playback is not implemented.
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
