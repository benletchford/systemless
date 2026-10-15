//! Deterministic standard-header sampled playback. Guest buffers stay live;
//! only mixed stereo frames cross into a host audio transport.
use super::*;
use crate::Error;
#[derive(Default)]
pub(super) struct Playback {
    pub command: [u8; 8],
    pub(super) voice: Option<Voice>,
    pub(super) amplitude: u16,
    pub(super) volume: [u16; 2],
}
pub(super) struct Voice {
    base: GuestPtr,
    length: usize,
    rate: u32,
    phase: u64,
    remainder: u64,
    denominator: u32,
}
impl Voice {
    fn denominator(&mut self, denominator: u32) {
        if self.denominator != denominator {
            self.remainder = (u128::from(self.remainder) * u128::from(denominator)
                / u128::from(self.denominator)) as u64;
            self.denominator = denominator;
        }
    }
}
impl Playback {
    pub fn new(init: u32) -> Self {
        Self {
            amplitude: 255,
            volume: match init & 3 {
                2 => [256, 0],
                3 => [0, 256],
                _ => [256, 256],
            },
            ..Self::default()
        }
    }
}
fn bad(message: &str) -> Error {
    Error::Sound(message.into())
}
pub(super) fn voice(m: &Memory, header: u32) -> Result<std::result::Result<Voice, i16>> {
    if header == 0 {
        return Ok(Err(-206));
    }
    let data = m.read(GuestPtr(header), 22)?;
    let long = |at| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let length = long(4) as usize;
    let rate = long(8);
    let start = long(12);
    let end = long(16);
    if data[20] != 0
        || data[21] == 0
        || data[21] > 127
        || length == 0
        || length > crate::load::IMAGE_LIMIT
        || rate < 65536
        || u64::from(rate) > 64000u64 << 16
        || (start != 0 || end != 0) && (start >= end || end as usize > length)
    {
        return Ok(Err(-206));
    }
    let base = GuestPtr(if long(0) == 0 {
        header
            .checked_add(22)
            .ok_or_else(|| bad("sound sample address overflow"))?
    } else {
        long(0)
    });
    m.read(base, length)?;
    Ok(Ok(Voice {
        base,
        length,
        rate,
        phase: 0,
        remainder: 0,
        denominator: 1,
    }))
}
impl Manager {
    /// Stop host-owned playback when the process exits/faults. Final guest
    /// bytes remain an inspection snapshot, like the retained process heap.
    pub fn cancel(&mut self) {
        for c in self.channels.values_mut() {
            c.playback.voice = None;
            c.commands.clear();
            c.callback = None;
        }
    }
    pub fn playing(&self) -> bool {
        self.channels
            .values()
            .any(|c| c.playback.voice.is_some() || !c.commands.is_empty() || c.callback.is_some())
    }
    pub fn busy(&self, channel: u32) -> Option<bool> {
        self.channels
            .get(&channel)
            .map(|c| c.playback.voice.is_some() || !c.commands.is_empty() || c.callback.is_some())
    }
    /// SndDoImmediate bypasses the queue. Unsupported command families return
    /// an OSErr; they never claim queued playback or callback completion.
    pub fn immediate(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        command: GuestPtr,
    ) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if !self.valid(m, p, channel)? {
                return Ok(BAD_CHANNEL);
            }
            if command.0 == 0 {
                return Err(bad("nil immediate sound command"));
            }
            let bytes: [u8; 8] = m.read(command, 8)?.try_into().unwrap();
            let error = super::queue::execute(
                m,
                channel,
                self.channels.get_mut(&channel).unwrap(),
                bytes,
                false,
            )?;
            if error == 0 {
                super::queue::pump(m, channel, self.channels.get_mut(&channel).unwrap())?;
            }
            Ok(error)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    /// Headless hosts can discard output while advancing virtual time. Work
    /// is bounded by channel count even for a very large requested wait.
    pub fn elapse_silently(&mut self, m: &mut Memory, p: &mut Process, ticks: u32) -> Result<()> {
        if self.channels.values().any(|c| !c.commands.is_empty()) {
            return self.elapse_queued(m, p, ticks);
        }
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            for &channel in &self.order {
                if !self.valid(m, p, channel)? {
                    return Err(bad("corrupt sound channel during silent advancement"));
                }
                if let Some(v) = &self.channels[&channel].playback.voice {
                    m.read(v.base, v.length)?;
                    m.validate_write(GuestPtr(channel + 20), 8)?;
                }
            }
            for (&channel, c) in &mut self.channels {
                if let Some(v) = &mut c.playback.voice {
                    v.denominator(60);
                    let numerator =
                        (u128::from(v.rate) << 16) * u128::from(ticks) + u128::from(v.remainder);
                    let end = (v.length as u128) << 32;
                    v.phase = (u128::from(v.phase) + numerator / 60).min(end) as u64;
                    v.remainder = (numerator % 60) as u64;
                    if u128::from(v.phase) >= end {
                        m.write(GuestPtr(channel + 20), &[0; 8])?;
                        c.playback.voice = None;
                        c.playback.command = [0; 8];
                    }
                }
            }
            Ok(())
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    // Match a 48 kHz silent sink without allocating frames or looping over the
    // requested duration. Each queued command/voice is visited at most once.
    fn elapse_queued(&mut self, m: &mut Memory, p: &mut Process, ticks: u32) -> Result<()> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            for &channel in &self.order {
                if !self.valid(m, p, channel)? {
                    return Err(bad("corrupt queued sound channel"));
                }
                super::queue::preflight(m, channel, &self.channels[&channel])?;
                if let Some(v) = &self.channels[&channel].playback.voice {
                    m.read(v.base, v.length)?;
                }
            }
            for (&channel, c) in &mut self.channels {
                let mut frames = u64::from(ticks) * 800;
                loop {
                    super::queue::pump(m, channel, c)?;
                    let Some(v) = &mut c.playback.voice else {
                        break;
                    };
                    v.denominator(48000);
                    let end = (v.length as u128) << 32;
                    let numerator = u128::from(v.rate) << 16;
                    let remaining = ((end - u128::from(v.phase)) * 48000)
                        .saturating_sub(u128::from(v.remainder));
                    let consume = u128::from(frames).min(remaining.div_ceil(numerator)) as u64;
                    let elapsed = numerator * u128::from(consume) + u128::from(v.remainder);
                    v.phase = (u128::from(v.phase) + elapsed / 48000).min(end) as u64;
                    v.remainder = (elapsed % 48000) as u64;
                    frames -= consume;
                    if u128::from(v.phase) < end {
                        break;
                    }
                    m.write(GuestPtr(channel + 20), &[0; 8])?;
                    c.playback.voice = None;
                    c.playback.command = [0; 8];
                }
            }
            Ok(())
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    /// Pull bounded deterministic output at the host-selected sample rate.
    /// All channel/guest accesses are validated before any phase is advanced.
    pub fn render(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        frames: usize,
        rate: u32,
    ) -> Result<Vec<[i16; 2]>> {
        if frames > 65536 || !(8000..=192000).contains(&rate) {
            return Err(bad("unsupported sound output quantum"));
        }
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            for &channel in &self.order {
                if !self.valid(m, p, channel)? {
                    return Err(bad("corrupt sound channel during rendering"));
                }
                if let Some(v) = &self.channels[&channel].playback.voice {
                    m.read(v.base, v.length)?;
                    m.validate_write(GuestPtr(channel + 20), 8)?;
                }
            }
            for &channel in &self.order {
                super::queue::preflight(m, channel, &self.channels[&channel])?;
            }
            let mut mixed = vec![[0i64; 2]; frames];
            let master = i64::from(super::super::volume(m)?);
            for (&channel, c) in &mut self.channels {
                let mut offset = 0;
                loop {
                    super::queue::pump(m, channel, c)?;
                    let Some(v) = &mut c.playback.voice else {
                        break;
                    };
                    let source = m.read(v.base, v.length)?;
                    v.denominator(rate);
                    let numerator = u64::from(v.rate) << 16;
                    while offset < frames && v.phase >> 32 < v.length as u64 {
                        let index = (v.phase >> 32) as usize;
                        let first = (i64::from(source[index]) - 128) * 256;
                        let sample = if c.init & 4 != 0 {
                            first
                        } else {
                            let second =
                                (i64::from(source[(index + 1).min(source.len() - 1)]) - 128) * 256;
                            let frac = (v.phase & 0xffff_ffff) as i64;
                            first + (((second - first) * frac) >> 32)
                        };
                        for (out, gain) in mixed[offset].iter_mut().zip(c.playback.volume) {
                            *out += sample * i64::from(c.playback.amplitude) * i64::from(gain)
                                / (255 * 256);
                        }
                        let elapsed = numerator + v.remainder;
                        v.phase += elapsed / u64::from(rate);
                        v.remainder = elapsed % u64::from(rate);
                        offset += 1;
                    }
                    if v.phase >> 32 >= v.length as u64 {
                        m.write(GuestPtr(channel + 20), &[0; 8])?;
                        c.playback.voice = None;
                        c.playback.command = [0; 8];
                    } else {
                        break;
                    }
                    // Pump zero-time commands even at the quantum boundary.
                    if offset == frames {
                        super::queue::pump(m, channel, c)?;
                        break;
                    }
                }
            }
            Ok(mixed
                .into_iter()
                .map(|frame| frame.map(|sample| (sample * master / 7).clamp(-32768, 32767) as i16))
                .collect())
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
