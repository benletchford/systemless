//! Shared Sound Manager channel/queue lifetime; no synthesized playback yet.
use crate::{
    memory::{GuestPtr, Memory},
    process::Process,
    Result,
};
use std::collections::BTreeMap;
pub const CHANNEL_SIZE: u32 = 1060;
const HEADER: u32 = 36;
const BAD_CHANNEL: i16 = -205;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewChannel {
    pub destination: GuestPtr,
    pub synth: i16,
    pub init: u32,
    pub callback: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    New(NewChannel),
    Dispose { channel: u32, quiet_now: bool },
}
struct Channel {
    synth: i16,
    init: u32,
    owned: bool,
    queue: u32,
    length: u16,
}
#[derive(Default)]
pub struct Manager {
    channels: BTreeMap<u32, Channel>,
    order: Vec<u32>,
}
fn overlaps(a: u32, an: u32, b: u32, bn: u32) -> bool {
    u64::from(a) < u64::from(b) + u64::from(bn) && u64::from(b) < u64::from(a) + u64::from(an)
}
impl Manager {
    /// Host diagnostics read the actual shared channel configuration.
    pub fn configuration(&self, channel: u32) -> Option<(i16, u32)> {
        self.channels.get(&channel).map(|c| (c.synth, c.init))
    }
    pub fn call(&mut self, m: &mut Memory, p: &mut Process, op: Op) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = match op {
            Op::New(request) => self.create(m, p, request),
            Op::Dispose { channel, quiet_now } => self.dispose(m, p, channel, quiet_now),
        };
        (p.mem_error, p.res_error) = errors;
        result
    }
    fn create(&mut self, m: &mut Memory, p: &mut Process, r: NewChannel) -> Result<i16> {
        m.validate_write(r.destination, 4)?;
        let existing = m.u32(r.destination)?;
        // This slice supports uncompressed sampled-sound channel configuration.
        // Unsupported synth/configurations report errors, never fake playback.
        if !matches!(r.synth, 0 | 1 | 3 | 5) {
            return Ok(-204);
        }
        if matches!(r.synth, 1 | 3)
            || r.init & !0xff != 0
            || r.init & 0xc0 == 0x40
            || r.init & 3 == 1
        {
            return Ok(-201);
        }
        if self.channels.len() >= 64 {
            return Ok(-201);
        }
        let (length, user_info) = if existing == 0 {
            (128, 0)
        } else {
            m.validate_write(GuestPtr(existing), CHANNEL_SIZE as usize)?;
            let header = m.read(GuestPtr(existing), HEADER as usize)?;
            let length = i16::from_be_bytes(header[30..32].try_into().unwrap());
            if length <= 0
                || overlaps(existing, CHANNEL_SIZE, r.destination.0, 4)
                || self.channels.iter().any(|(&base, c)| {
                    overlaps(existing, CHANNEL_SIZE, base, CHANNEL_SIZE)
                        || overlaps(existing, CHANNEL_SIZE, c.queue, u32::from(c.length) * 8)
                })
            {
                return Ok(BAD_CHANNEL);
            }
            (
                length as u16,
                u32::from_be_bytes(header[12..16].try_into().unwrap()),
            )
        };
        let queue_size = u32::from(length) * 8;
        let queue = p.new_pointer(m, queue_size, true)?;
        if queue == 0 {
            return Ok(-108);
        }
        if overlaps(queue, queue_size, r.destination.0, 4)
            || existing != 0 && overlaps(queue, queue_size, existing, CHANNEL_SIZE)
        {
            p.dispose_pointer(queue);
            return Ok(BAD_CHANNEL);
        }
        let channel = if existing == 0 {
            match p.new_pointer(m, CHANNEL_SIZE, true) {
                Ok(0) => {
                    p.dispose_pointer(queue);
                    return Ok(-108);
                }
                Ok(value) => value,
                Err(error) => {
                    p.dispose_pointer(queue);
                    return Err(error);
                }
            }
        } else {
            existing
        };
        if overlaps(channel, CHANNEL_SIZE, r.destination.0, 4) {
            if existing == 0 {
                p.dispose_pointer(channel);
            }
            p.dispose_pointer(queue);
            return Ok(BAD_CHANNEL);
        }
        let mut header = [0; 36];
        header[..4].copy_from_slice(&self.order.first().copied().unwrap_or(0).to_be_bytes());
        header[4..8].copy_from_slice(&queue.to_be_bytes());
        header[8..12].copy_from_slice(&r.callback.to_be_bytes());
        header[12..16].copy_from_slice(&user_info.to_be_bytes());
        header[30..32].copy_from_slice(&length.to_be_bytes());
        m.write(GuestPtr(channel), &header)?;
        m.write(r.destination, &channel.to_be_bytes())?;
        self.channels.insert(
            channel,
            Channel {
                synth: r.synth,
                init: r.init,
                owned: existing == 0,
                queue,
                length,
            },
        );
        self.order.insert(0, channel);
        Ok(0)
    }
    fn valid(&self, m: &Memory, p: &mut Process, channel: u32) -> Result<bool> {
        let Some(c) = self.channels.get(&channel) else {
            return Ok(false);
        };
        let Some(index) = self.order.iter().position(|v| *v == channel) else {
            return Ok(false);
        };
        if p.pointer_size(c.queue) != u32::from(c.length) * 8
            || c.owned && p.pointer_size(channel) != CHANNEL_SIZE
        {
            return Ok(false);
        }
        let header = m.read(GuestPtr(channel), HEADER as usize)?;
        let next = self.order.get(index + 1).copied().unwrap_or(0);
        Ok(header[..4] == next.to_be_bytes()
            && header[4..8] == c.queue.to_be_bytes()
            && header[16..30].iter().all(|b| *b == 0)
            && header[30..32] == c.length.to_be_bytes()
            && header[32..36].iter().all(|b| *b == 0))
    }
    fn dispose(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        _quiet_now: bool,
    ) -> Result<i16> {
        if !self.valid(m, p, channel)? {
            return Ok(BAD_CHANNEL);
        }
        let index = self.order.iter().position(|v| *v == channel).unwrap();
        let previous = index.checked_sub(1).map(|i| self.order[i]);
        if let Some(previous) = previous {
            if !self.valid(m, p, previous)? {
                return Ok(BAD_CHANNEL);
            }
            m.validate_write(GuestPtr(previous), 4)?;
        }
        m.validate_write(GuestPtr(channel), HEADER as usize)?;
        let c = &self.channels[&channel];
        let mut header = m.read(GuestPtr(channel), HEADER as usize)?.to_vec();
        // Preserve caller userInfo/callback and the caller-owned record allocation.
        header[..8].fill(0);
        header[16..36].fill(0);
        if let Some(previous) = previous {
            let next = self.order.get(index + 1).copied().unwrap_or(0);
            m.write(GuestPtr(previous), &next.to_be_bytes())?;
        }
        m.write(GuestPtr(channel), &header)?;
        p.dispose_pointer(c.queue);
        if c.owned {
            p.dispose_pointer(channel);
        }
        self.channels.remove(&channel);
        self.order.remove(index);
        // Both quiet modes finish immediately on an idle channel. Queue execution
        // and active playback are unsupported; corrupt/busy records were rejected.
        Ok(0)
    }
}
