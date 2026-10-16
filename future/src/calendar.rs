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

/// Gregorian DateTimeRec fields for the complete unsigned Macintosh seconds range.
/// IM: OS Utilities, SecondsToDate and DateTimeRec (Sunday = 1).
pub fn date(seconds: u32) -> [u16; 7] {
    let total_days = seconds / 86400;
    let mut days = total_days;
    let mut year = 1904u16;
    let leap = |y: u16| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= lengths[month] {
        days -= lengths[month];
        month += 1;
    }
    [
        year,
        month as u16 + 1,
        days as u16 + 1,
        (seconds / 3600 % 24) as u16,
        (seconds / 60 % 60) as u16,
        (seconds % 60) as u16,
        ((total_days + 5) % 7 + 1) as u16,
    ]
}
/// Publish exactly fourteen big-endian bytes without consulting or changing the clock.
pub fn seconds_to_date(memory: &mut Memory, seconds: u32, destination: GuestPtr) -> Result<()> {
    if destination.0 == 0 {
        return Err(crate::Error::Cpu("nil DateTimeRec destination".into()));
    }
    memory.validate_write(destination, 14)?;
    let mut bytes = [0; 14];
    for (field, value) in bytes.chunks_exact_mut(2).zip(date(seconds)) {
        field.copy_from_slice(&value.to_be_bytes());
    }
    memory.write(destination, &bytes)
}
