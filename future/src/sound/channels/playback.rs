//! Deterministic standard-header sampled playback. Guest buffers stay live;
//! only mixed stereo frames cross into a host audio transport.
use super::*;
use crate::Error;
#[derive(Default)]
pub(super) struct Playback {
    pub command: [u8; 8],
    voice: Option<Voice>,
    amplitude: u16,
    volume: [u16; 2],
}
struct Voice {
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
fn voice(m: &Memory, header: u32) -> Result<std::result::Result<Voice, i16>> {
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
        }
    }
    pub fn playing(&self) -> bool {
        self.channels.values().any(|c| c.playback.voice.is_some())
    }
    pub fn busy(&self, channel: u32) -> Option<bool> {
        self.channels
            .get(&channel)
            .map(|c| c.playback.voice.is_some())
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
            let code = u16::from_be_bytes(bytes[..2].try_into().unwrap());
            let parameter = u16::from_be_bytes(bytes[2..4].try_into().unwrap());
            let value = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
            let c = self.channels.get_mut(&channel).unwrap();
            match code {
                0 => {}
                3 => {
                    m.validate_write(GuestPtr(channel + 20), 8)?;
                    m.write(GuestPtr(channel + 20), &[0; 8])?;
                    c.playback.voice = None;
                    c.playback.command = [0; 8];
                }
                43 => {
                    if parameter > 255 {
                        return Ok(-206);
                    }
                    c.playback.amplitude = parameter;
                }
                45 => {
                    if value == 0 {
                        return Ok(-206);
                    }
                    m.write(GuestPtr(value), &c.playback.amplitude.to_be_bytes())?;
                }
                46 => {
                    c.playback.volume = [value as u16, (value >> 16) as u16];
                }
                47 => {
                    if value == 0 {
                        return Ok(-206);
                    }
                    let volume =
                        u32::from(c.playback.volume[0]) | (u32::from(c.playback.volume[1]) << 16);
                    m.write(GuestPtr(value), &volume.to_be_bytes())?;
                }
                81 => {
                    if c.playback.voice.is_some() {
                        return Ok(-209);
                    }
                    // Rate-conversion override flags and stereo source headers
                    // need separate semantics; mono pan and no-interp are live.
                    if c.init & !0x87 != 0 {
                        return Ok(-201);
                    }
                    let next = match voice(m, value)? {
                        Ok(v) => v,
                        Err(error) => return Ok(error),
                    };
                    m.validate_write(GuestPtr(channel + 20), 8)?;
                    m.write(GuestPtr(channel + 20), &bytes)?;
                    c.playback.command = bytes;
                    c.playback.voice = Some(next);
                }
                _ => return Err(bad(&format!("unsupported immediate sound command {code}"))),
            }
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    /// Headless hosts can discard output while advancing virtual time. Work
    /// is bounded by channel count even for a very large requested wait.
    pub fn elapse_silently(&mut self, m: &mut Memory, p: &mut Process, ticks: u32) -> Result<()> {
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
            let mut mixed = vec![[0i64; 2]; frames];
            let master = i64::from(super::super::volume(m)?);
            for (&channel, c) in &mut self.channels {
                let Some(v) = &mut c.playback.voice else {
                    continue;
                };
                let source = m.read(v.base, v.length)?;
                v.denominator(rate);
                let numerator = u64::from(v.rate) << 16;
                for frame in &mut mixed {
                    let index = (v.phase >> 32) as usize;
                    if index >= source.len() {
                        break;
                    }
                    let first = (i64::from(source[index]) - 128) * 256;
                    let sample = if c.init & 4 != 0 {
                        first
                    } else {
                        let second =
                            (i64::from(source[(index + 1).min(source.len() - 1)]) - 128) * 256;
                        let frac = (v.phase & 0xffff_ffff) as i64;
                        first + (((second - first) * frac) >> 32)
                    };
                    for (out, gain) in frame.iter_mut().zip(c.playback.volume) {
                        *out += sample * i64::from(c.playback.amplitude) * i64::from(gain)
                            / (255 * 256);
                    }
                    let elapsed = numerator + v.remainder;
                    v.phase += elapsed / u64::from(rate);
                    v.remainder = elapsed % u64::from(rate);
                }
                if v.phase >> 32 >= v.length as u64 {
                    m.write(GuestPtr(channel + 20), &[0; 8])?;
                    c.playback.voice = None;
                    c.playback.command = [0; 8];
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
