//! Operating System event queue. Inside Macintosh: Toolbox Essentials,
//! Event Manager, PostEvent/GetOSEvent/OSEventAvail/FlushEvents.
//! Guest links and records are authoritative; host state holds only input.
use crate::{
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
/// Apple TN DV520: high bit 1 means released; low bits are not the Boolean.
pub const MB_STATE: GuestPtr = GuestPtr(0x172);
pub fn button(memory: &Memory) -> Result<bool> {
    Ok(memory.read(MB_STATE, 1)?[0] & 0x80 == 0)
}
/// Tracking continues only during the same physical press, without a queued release.
/// Comparison contract: preserved Systemless Toolbox StillDown ($A973).
pub fn still_down(memory: &Memory) -> Result<bool> {
    Ok(button(memory)? && !available(memory, 0x0004)?)
}
pub const MOUSE: GuestPtr = GuestPtr(0x830);
pub const SYSTEM_MASK: GuestPtr = GuestPtr(0x144);
pub const QUEUE_HEADER: GuestPtr = GuestPtr(0x14a);
const BASE: u32 = 0x7300_0000;
const COUNT: usize = 20;
const STRIDE: u32 = 24;
#[derive(Clone, Copy)]
pub struct Input {
    pub vertical: i16,
    pub horizontal: i16,
    pub modifiers: u16,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            vertical: 0,
            horizontal: 0,
            modifiers: 0x80,
        }
    }
}
pub(crate) fn install(memory: &mut Memory) -> Result<()> {
    memory.map(
        GuestPtr(BASE),
        vec![0; COUNT * STRIDE as usize],
        Permissions::RW,
    )?;
    memory.write(MB_STATE, &[0x80])?;
    memory.write(SYSTEM_MASK, &0xffefu16.to_be_bytes())?;
    memory.write(QUEUE_HEADER, &[0; 10])
}
fn matches(what: u16, mask: u16) -> bool {
    what < 16 && mask & (1u16 << what) != 0
}
fn links(memory: &Memory) -> Result<Vec<u32>> {
    let mut items = Vec::new();
    let mut next = memory.u32(GuestPtr(QUEUE_HEADER.0 + 2))?;
    while next != 0 {
        if next < BASE
            || next >= BASE + COUNT as u32 * STRIDE
            || !(next - BASE).is_multiple_of(STRIDE)
            || items.contains(&next)
        {
            return Err(Error::Cpu("corrupt OS event queue links".into()));
        }
        items.push(next);
        next = memory.u32(GuestPtr(next))?;
    }
    if memory.u32(GuestPtr(QUEUE_HEADER.0 + 6))? != items.last().copied().unwrap_or(0) {
        return Err(Error::Cpu("corrupt OS event queue tail".into()));
    }
    Ok(items)
}
fn relink(memory: &mut Memory, items: &[u32]) -> Result<()> {
    // Validate every destination before publishing any queue changes.
    memory.validate_write(QUEUE_HEADER, 10)?;
    for &p in items {
        memory.validate_write(GuestPtr(p), 4)?;
    }
    for (i, &p) in items.iter().enumerate() {
        memory.write(
            GuestPtr(p),
            &items.get(i + 1).copied().unwrap_or(0).to_be_bytes(),
        )?;
    }
    memory.write(
        GuestPtr(QUEUE_HEADER.0 + 2),
        &items.first().copied().unwrap_or(0).to_be_bytes(),
    )?;
    memory.write(
        GuestPtr(QUEUE_HEADER.0 + 6),
        &items.last().copied().unwrap_or(0).to_be_bytes(),
    )
}
pub(crate) fn record(what: u16, message: u32, ticks: u32, input: Input) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&what.to_be_bytes());
    bytes[2..6].copy_from_slice(&message.to_be_bytes());
    bytes[6..10].copy_from_slice(&ticks.to_be_bytes());
    bytes[10..12].copy_from_slice(&input.vertical.to_be_bytes());
    bytes[12..14].copy_from_slice(&input.horizontal.to_be_bytes());
    bytes[14..].copy_from_slice(&input.modifiers.to_be_bytes());
    bytes
}
pub fn post(memory: &mut Memory, what: u16, message: u32, ticks: u32, input: Input) -> Result<i16> {
    if !matches!(what, 1..=5 | 7) || !matches(what, memory.u16(SYSTEM_MASK)?) {
        return Ok(1);
    }
    let mut items = links(memory)?;
    let slot = if items.len() == COUNT {
        items.remove(0)
    } else {
        (0..COUNT)
            .map(|i| BASE + i as u32 * STRIDE)
            .find(|p| !items.contains(p))
            .unwrap()
    };
    memory.validate_write(GuestPtr(BASE), COUNT * STRIDE as usize)?;
    memory.validate_write(QUEUE_HEADER, 10)?;
    let mut bytes = [0; 22];
    bytes[4..6].copy_from_slice(&4u16.to_be_bytes()); // evType
    bytes[6..].copy_from_slice(&record(what, message, ticks, input));
    memory.write(GuestPtr(slot), &bytes)?;
    items.push(slot);
    relink(memory, &items)?;
    Ok(0)
}
pub fn receive(
    memory: &mut Memory,
    mask: u16,
    destination: GuestPtr,
    remove: bool,
    ticks: u32,
    input: Input,
) -> Result<bool> {
    let mut items = links(memory)?;
    memory.validate_write(destination, 16)?;
    let mut found = None;
    for (i, &p) in items.iter().enumerate() {
        if matches(memory.u16(GuestPtr(p + 6))?, mask) {
            found = Some(i);
            break;
        }
    }
    let bytes = if let Some(i) = found {
        memory.read(GuestPtr(items[i] + 6), 16)?.try_into().unwrap()
    } else {
        record(0, 0, ticks, input)
    };
    if remove && found.is_some() {
        memory.validate_write(QUEUE_HEADER, 10)?;
        for &p in &items {
            memory.validate_write(GuestPtr(p), 4)?;
        }
    }
    memory.write(destination, &bytes)?;
    if remove {
        if let Some(i) = found {
            items.remove(i);
            relink(memory, &items)?;
        }
    }
    Ok(found.is_some())
}
pub fn flush(memory: &mut Memory, which: u16, stop: u16) -> Result<u16> {
    let items = links(memory)?;
    let mut keep = Vec::new();
    let mut stopped = 0;
    for p in items {
        let what = memory.u16(GuestPtr(p + 6))?;
        if stopped == 0 && matches(what, stop) {
            stopped = what;
        }
        if stopped != 0 || !matches(what, which) {
            keep.push(p);
        }
    }
    relink(memory, &keep)?;
    Ok(stopped)
}

/// Readiness probing does not write an event record or consume queue entries.
pub fn available(memory: &Memory, mask: u16) -> Result<bool> {
    for p in links(memory)? {
        if matches(memory.u16(GuestPtr(p + 6))?, mask) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Snapshot and preflight a single queue removal. Unrelated events retain order.
pub(crate) struct Removal {
    pub record: [u8; 16],
    remaining: Vec<u32>,
}
impl Removal {
    pub fn commit(self, memory: &mut Memory) -> Result<()> {
        relink(memory, &self.remaining)
    }
}
pub(crate) fn prepare_remove(memory: &Memory, mask: u16) -> Result<Option<Removal>> {
    let mut items = links(memory)?;
    let mut found = None;
    for (i, &p) in items.iter().enumerate() {
        if matches(memory.u16(GuestPtr(p + 6))?, mask) {
            found = Some(i);
            break;
        }
    }
    let Some(i) = found else {
        return Ok(None);
    };
    let record = memory.read(GuestPtr(items[i] + 6), 16)?.try_into().unwrap();
    items.remove(i);
    memory.validate_write(QUEUE_HEADER, 10)?;
    for &p in &items {
        memory.validate_write(GuestPtr(p), 4)?;
    }
    Ok(Some(Removal {
        record,
        remaining: items,
    }))
}
