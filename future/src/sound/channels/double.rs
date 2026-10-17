//! Live uncompressed double buffers: IM Sound, pp. 2-69, 2-111–113, 2-153.
use super::*;
use crate::Error;
use std::collections::VecDeque;
fn bad(s: &str) -> Error {
    Error::Sound(s.into())
}
pub(super) struct Double {
    pub generation: u64,
    buffers: [u32; 2],
    channels: usize,
    width: usize,
    rate: u32,
    pub routine: u32,
    next: usize,
    finished: bool,
    carry: (u64, u64, u32),
    pub pending: VecDeque<(u32, bool)>,
}
impl Double {
    fn buffer(&self, m: &Memory, index: usize) -> Result<Option<playback::Voice>> {
        let pointer = self.buffers[index];
        let b = m.read(GuestPtr(pointer), 16)?;
        m.validate_write(GuestPtr(pointer + 4), 4)?;
        let flags = u32::from_be_bytes(b[4..8].try_into().unwrap());
        if flags & 1 == 0 {
            return Ok(None);
        }
        let frames = i32::from_be_bytes(b[..4].try_into().unwrap());
        if frames <= 0 {
            return Err(bad("ready double buffer has no positive frame count"));
        }
        let base = pointer
            .checked_add(16)
            .ok_or_else(|| bad("double buffer overflow"))?;
        Ok(Some(playback::raw_voice(
            m,
            GuestPtr(base),
            frames as usize,
            self.channels,
            self.width,
            self.rate,
        )?))
    }
}
pub(super) fn preflight(m: &Memory, c: &Channel) -> Result<()> {
    if let Some(d) = &c.double {
        for i in 0..2 {
            d.buffer(m, i)?;
        }
    }
    Ok(())
}
pub(super) fn pump(m: &mut Memory, channel: u32, c: &mut Channel) -> Result<()> {
    let Some(d) = &mut c.double else {
        return Ok(());
    };
    if d.finished && d.pending.is_empty() {
        c.double = None;
        m.write(GuestPtr(channel + 20), &[0; 8])?;
        c.playback.command = [0; 8];
        return super::queue::pump(m, channel, c);
    }
    if d.finished
        || c.playback.voice.is_some()
        || d.pending.iter().any(|p| p.0 == d.buffers[d.next])
    {
        return Ok(());
    }
    if let Some(mut voice) = d.buffer(m, d.next)? {
        (voice.phase, voice.remainder, voice.denominator) = d.carry;
        c.playback.voice = Some(voice);
    }
    Ok(())
}
pub(super) fn following(m: &Memory, c: &Channel) -> Result<Option<playback::Voice>> {
    let Some(d) = &c.double else {
        return Ok(None);
    };
    let next = d.next ^ 1;
    if m.u32(GuestPtr(d.buffers[d.next] + 4))? & 4 != 0
        || d.pending.iter().any(|p| p.0 == d.buffers[next])
    {
        return Ok(None);
    }
    d.buffer(m, next)
}
pub(super) fn complete(m: &mut Memory, channel: u32, c: &mut Channel) -> Result<()> {
    if let Some(d) = &mut c.double {
        let pointer = d.buffers[d.next];
        let flags = m.u32(GuestPtr(pointer + 4))?;
        m.validate_write(GuestPtr(pointer + 4), 4)?;
        let voice = c
            .playback
            .voice
            .take()
            .ok_or_else(|| bad("missing double buffer voice"))?;
        d.carry = (
            voice.phase.saturating_sub((voice.length as u64) << 32),
            voice.remainder,
            voice.denominator,
        );
        m.write(GuestPtr(pointer + 4), &(flags & !1).to_be_bytes())?;
        d.finished = flags & 4 != 0;
        if d.routine != 0 {
            d.pending.push_back((pointer, false));
        }
        d.next ^= 1;
        return Ok(());
    }
    m.write(GuestPtr(channel + 20), &[0; 8])?;
    c.playback.voice = None;
    c.playback.command = [0; 8];
    c.active = None;
    Ok(())
}
impl Manager {
    pub fn play_double(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        header: GuestPtr,
    ) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if !self.valid(m, p, channel)? {
                return Ok(BAD_CHANNEL);
            }
            let c = &self.channels[&channel];
            if c.double.is_some()
                || c.playback.voice.is_some()
                || c.callback.is_some()
                || !c.commands.is_empty()
                || c.file_play.is_some()
            {
                return Ok(-209);
            }
            if header.0 == 0 {
                return Ok(-50);
            }
            let b = m.read(header, 24)?;
            let word = |i| u16::from_be_bytes(b[i..i + 2].try_into().unwrap());
            let long = |i| u32::from_be_bytes(b[i..i + 4].try_into().unwrap());
            if !matches!(word(0), 1 | 2)
                || !matches!(word(2), 8 | 16)
                || word(4) != 0
                || long(8) < 65536
                || u64::from(long(8)) > 64000u64 << 16
            {
                return Ok(-206);
            }
            let generation = self
                .next_serial
                .checked_add(1)
                .ok_or_else(|| bad("double buffer generation exhausted"))?;
            let d = Double {
                generation,
                buffers: [long(12), long(16)],
                channels: word(0) as usize,
                width: (word(2) / 8) as usize,
                rate: long(8),
                routine: long(20),
                next: 0,
                finished: false,
                carry: (0, 0, 1),
                pending: VecDeque::new(),
            };
            if d.buffers.contains(&0) || d.buffers[0] == d.buffers[1] {
                return Ok(-50);
            }
            for i in 0..2 {
                if i32::from_be_bytes(m.read(GuestPtr(d.buffers[i]), 4)?.try_into().unwrap()) <= 0 {
                    return Ok(-206);
                }
                let Some(_) = d.buffer(m, i)? else {
                    return Ok(-206);
                };
                let bytes = usize::try_from(m.u32(GuestPtr(d.buffers[i]))?)
                    .unwrap()
                    .checked_mul(d.channels * d.width)
                    .and_then(|v| v.checked_add(16))
                    .ok_or_else(|| bad("double buffer length overflow"))?;
                if overlaps(d.buffers[i], bytes as u32, channel, c.record_size)
                    || overlaps(d.buffers[i], bytes as u32, c.queue, u32::from(c.length) * 8)
                    || overlaps(d.buffers[i], bytes as u32, header.0, 24)
                {
                    return Ok(-50);
                }
                let other = d.buffers[i ^ 1];
                if overlaps(d.buffers[i], bytes as u32, other, 16) {
                    return Ok(-50);
                }
            }
            m.validate_write(GuestPtr(channel + 20), 8)?;
            self.next_serial = generation;
            let c = self.channels.get_mut(&channel).unwrap();
            let mut command = [0; 8];
            command[..2].copy_from_slice(&84u16.to_be_bytes());
            command[4..].copy_from_slice(&header.0.to_be_bytes());
            m.write(GuestPtr(channel + 20), &command)?;
            c.playback.command = command;
            c.double = Some(d);
            pump(m, channel, c)?;
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
