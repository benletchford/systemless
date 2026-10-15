//! One Macintosh Time/Ticks clock; hosts choose the epoch, execution stays deterministic.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
pub const TIME: GuestPtr = GuestPtr(0x20c);
pub const TICKS: GuestPtr = GuestPtr(0x16a);
/// Reproducible loader epoch: 2000-01-01 00:00:00, seconds since 1904-01-01.
pub const DEFAULT_TIME: u32 = 3_029_529_600;
pub const UNIX_OFFSET: u64 = 2_082_844_800;
#[derive(Default)]
pub struct Clock {
    fractional_ticks: u32,
}
impl Clock {
    pub fn set(&mut self, memory: &mut Memory, seconds: u32) -> Result<()> {
        memory.write(TIME, &seconds.to_be_bytes())?;
        self.fractional_ticks = 0;
        Ok(())
    }
    pub fn advance(&mut self, memory: &mut Memory, ticks: u32) -> Result<()> {
        let now = memory.u32(TICKS)?;
        let seconds = memory.u32(TIME)?;
        let total = u64::from(self.fractional_ticks) + u64::from(ticks);
        memory.validate_write(TICKS, 4)?;
        memory.validate_write(TIME, 4)?;
        memory.write(TICKS, &now.wrapping_add(ticks).to_be_bytes())?;
        memory.write(
            TIME,
            &seconds.wrapping_add((total / 60) as u32).to_be_bytes(),
        )?;
        self.fractional_ticks = (total % 60) as u32;
        Ok(())
    }
}

pub fn read(memory: &mut Memory, destination: GuestPtr) -> Result<()> {
    let seconds = memory.u32(TIME)?;
    memory.write(destination, &seconds.to_be_bytes())
}
