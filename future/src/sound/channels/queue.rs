//! Guest-visible inline command ring and CPU-bound callback scheduling.
use super::*;
use crate::Error;
fn bad(message: &str) -> Error {
    Error::Sound(message.into())
}
pub(super) fn matches(m: &Memory, channel: u32, c: &Channel) -> Result<bool> {
    for (i, bytes) in c.commands.iter().enumerate() {
        let slot = (usize::from(c.head) + i) % usize::from(c.length);
        if m.read(GuestPtr(channel + 36 + slot as u32 * 8), 8)? != bytes.bytes {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn check(m: &Memory, c: &Channel, bytes: [u8; 8], queued: bool) -> Result<i16> {
    let code = u16::from_be_bytes(bytes[..2].try_into().unwrap());
    let parameter = u16::from_be_bytes(bytes[2..4].try_into().unwrap());
    let value = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    match code {
        0 | 3 | 4 | 46 => {}
        13 if queued => {}
        43 if parameter <= 255 => {}
        43 => return Ok(-206),
        45 | 47 => {
            if value == 0 {
                return Ok(-206);
            }
            m.validate_write(GuestPtr(value), if code == 45 { 2 } else { 4 })?;
        }
        81 => {
            if c.init & !0xf7 != 0 {
                return Ok(-201);
            }
            if let Err(error) = super::playback::voice(m, value)? {
                return Ok(error);
            }
        }
        _ => {
            return Err(bad(&format!(
                "unsupported {} sound command {code}",
                if queued { "queued" } else { "immediate" }
            )))
        }
    }
    Ok(0)
}
pub(super) fn preflight(m: &Memory, channel: u32, c: &Channel) -> Result<()> {
    m.validate_write(GuestPtr(channel + 20), 16)?;
    for pending in &c.commands {
        let error = check(m, c, pending.bytes, true)?;
        if error != 0 {
            return Err(bad(&format!(
                "queued sound command became invalid ({error})"
            )));
        }
    }
    Ok(())
}
fn clear(m: &mut Memory, channel: u32, c: &mut Channel) -> Result<()> {
    m.write(GuestPtr(channel + 20), &[0; 8])?;
    c.playback.command = [0; 8];
    c.playback.voice = None;
    c.active = None;
    Ok(())
}
pub(super) fn execute(
    m: &mut Memory,
    channel: u32,
    c: &mut Channel,
    bytes: [u8; 8],
    queued: bool,
) -> Result<i16> {
    let error = check(m, c, bytes, queued)?;
    if error != 0 {
        return Ok(error);
    }
    let code = u16::from_be_bytes(bytes[..2].try_into().unwrap());
    let parameter = u16::from_be_bytes(bytes[2..4].try_into().unwrap());
    let value = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    match code {
        0 => {}
        3 => {
            // A callback already running owns cmdInProgress until its CPU returns.
            if c.callback.is_some() {
                return Ok(0);
            }
            m.validate_write(GuestPtr(channel + 20), 8)?;
            clear(m, channel, c)?;
        }
        4 => {
            m.validate_write(GuestPtr(channel + 32), 4)?;
            m.write(GuestPtr(channel + 32), &[0; 4])?;
            c.commands.clear();
            c.head = 0;
            c.tail = 0;
        }
        13 => {
            let routine = m.u32(GuestPtr(channel + 8))?;
            if routine != 0 {
                m.write(GuestPtr(channel + 20), &bytes)?;
                c.playback.command = bytes;
                c.callback = Some(false);
            }
        }
        43 => c.playback.amplitude = parameter,
        45 => m.write(GuestPtr(value), &c.playback.amplitude.to_be_bytes())?,
        46 => c.playback.volume = [value as u16, (value >> 16) as u16],
        47 => {
            let volume = u32::from(c.playback.volume[0]) | (u32::from(c.playback.volume[1]) << 16);
            m.write(GuestPtr(value), &volume.to_be_bytes())?;
        }
        81 => {
            if c.playback.voice.is_some() || c.callback.is_some() {
                return Ok(-209);
            }
            let next = super::playback::voice(m, value)?
                .map_err(|_| bad("validated sound header changed"))?;
            m.validate_write(GuestPtr(channel + 20), 8)?;
            m.write(GuestPtr(channel + 20), &bytes)?;
            c.playback.command = bytes;
            c.playback.voice = Some(next);
        }
        _ => unreachable!(),
    }
    Ok(0)
}
pub(super) fn pump(m: &mut Memory, channel: u32, c: &mut Channel) -> Result<()> {
    while c.playback.voice.is_none() && c.callback.is_none() {
        let Some(bytes) = c.commands.front().map(|p| p.bytes) else {
            break;
        };
        let error = check(m, c, bytes, true)?;
        if error != 0 {
            return Err(bad(&format!("invalid pending sound command ({error})")));
        }
        m.validate_write(GuestPtr(channel + 20), 16)?;
        let pending = c.commands.pop_front().unwrap();
        c.head = (c.head + 1) % c.length;
        m.write(GuestPtr(channel + 32), &c.head.to_be_bytes())?;
        let error = execute(m, channel, c, bytes, true)?;
        if error != 0 {
            return Err(bad(&format!("pending sound command failed ({error})")));
        }
        if c.playback.voice.is_some() || c.callback.is_some() {
            c.active = Some(pending);
        }
    }
    Ok(())
}
impl Manager {
    pub(crate) fn generation(&self, channel: u32) -> u64 {
        self.channels.get(&channel).map_or(0, |c| c.serial)
    }
    /// Copy a submitted command into the actual guest channel ring. None means
    /// queue full: Runtime decides whether to wait or return queueFull.
    pub fn enqueue(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        bytes: [u8; 8],
    ) -> Result<Option<i16>> {
        self.enqueue_for_play(m, p, channel, bytes, None)
    }
    pub(super) fn enqueue_for_play(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        bytes: [u8; 8],
        completion: Option<&resource::Completion>,
    ) -> Result<Option<i16>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if !self.valid(m, p, channel)? {
                return Ok(Some(BAD_CHANNEL));
            }
            let c = self.channels.get_mut(&channel).unwrap();
            let error = check(m, c, bytes, true)?;
            if error != 0 {
                return Ok(Some(error));
            }
            if c.commands.len() == usize::from(c.length) {
                return Ok(None);
            }
            preflight(m, channel, c)?;
            let slot = GuestPtr(channel + 36 + u32::from(c.tail) * 8);
            m.validate_write(slot, 8)?;
            m.write(slot, &bytes)?;
            c.tail = (c.tail + 1) % c.length;
            m.write(GuestPtr(channel + 34), &c.tail.to_be_bytes())?;
            c.commands
                .push_back(resource::Pending::new(bytes, completion));
            pump(m, channel, c)?;
            Ok(Some(0))
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    pub(crate) fn pending_callback(
        &self,
        m: &Memory,
        p: &mut Process,
    ) -> Result<Option<(u32, u32)>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            for &channel in &self.order {
                if self.channels[&channel].callback == Some(false) {
                    if !self.valid(m, p, channel)? {
                        return Err(bad("corrupt callback sound channel"));
                    }
                    return Ok(Some((channel, m.u32(GuestPtr(channel + 8))?)));
                }
            }
            Ok(None)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }

    pub(crate) fn begin_callback(&mut self, channel: u32) -> u64 {
        let c = self.channels.get_mut(&channel).unwrap();
        c.callback = Some(true);
        c.serial
    }
    pub(crate) fn finish_callback(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        serial: u64,
    ) -> Result<()> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            // Quiet disposal may release/reuse this address inside the callback.
            if self
                .channels
                .get(&channel)
                .is_none_or(|c| c.serial != serial)
            {
                return Ok(());
            }
            if !self.valid(m, p, channel)? {
                return Err(bad("corrupt returned sound callback channel"));
            }
            let c = self.channels.get_mut(&channel).unwrap();
            m.validate_write(GuestPtr(channel + 20), 16)?;
            preflight(m, channel, c)?;
            c.callback = None;
            clear(m, channel, c)?;
            pump(m, channel, c)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
