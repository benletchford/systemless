//! Loaded sound-resource commands use the same inline FIFO and live samples.
use super::*;
use crate::Error;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
#[derive(Default, Clone)]
pub(super) struct Completion(Arc<AtomicU32>);
impl Completion {
    fn done(&self) -> bool {
        self.0.load(Ordering::Relaxed) == 0
    }
}
pub(super) struct FilePlay {
    completion: Completion,
    paused: bool,
}
impl FilePlay {
    fn active(&self) -> bool {
        !self.completion.done()
    }
    pub(super) fn paused(&self) -> bool {
        self.paused && self.active()
    }
}
pub(super) struct Pending {
    pub bytes: [u8; 8],
    completion: Option<Completion>,
}
impl Pending {
    pub fn new(bytes: [u8; 8], completion: Option<&Completion>) -> Self {
        if let Some(c) = completion {
            c.0.fetch_add(1, Ordering::Relaxed);
        }
        Self {
            bytes,
            completion: completion.cloned(),
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(c) = &self.completion {
            c.0.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
pub(crate) struct Play {
    pub channel: u32,
    serial: u64,
    internal: bool,
    synchronous: bool,
    commands: VecDeque<[u8; 8]>,
    completion: Completion,
}
struct Resource {
    configuration: Vec<(i16, u32)>,
    commands: VecDeque<[u8; 8]>,
}
fn bad(message: &str) -> Error {
    Error::Sound(message.into())
}
// Malformed resource-local offsets become badFormat. Absolute pointers continue
// through Memory's checked fault path instead of being mistaken for offsets.
fn parse(m: &Memory, base: u32, size: usize) -> Result<std::result::Result<Resource, i16>> {
    let data = m.read(GuestPtr(base), size)?;
    let word = |at: usize| {
        data.get(at..at + 2)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
    };
    let mut configuration = Vec::new();
    let at = match word(0) {
        Some(1) => {
            let Some(count) = word(2) else {
                return Ok(Err(-206));
            };
            let end = 4 + usize::from(count) * 6;
            if end + 2 > size {
                return Ok(Err(-206));
            }
            for entry in data[4..end].as_chunks::<6>().0 {
                configuration.push((
                    i16::from_be_bytes(entry[..2].try_into().unwrap()),
                    u32::from_be_bytes(entry[2..].try_into().unwrap()),
                ));
            }
            end
        }
        Some(2) => 4, // Reference count is application metadata, not a list length.
        _ => return Ok(Err(-206)),
    };
    let Some(count) = word(at) else {
        return Ok(Err(-206));
    };
    let command_end = at + 2 + usize::from(count) * 8;
    if command_end > size {
        return Ok(Err(-206));
    }
    let mut commands = VecDeque::new();
    for raw in data[at + 2..command_end].as_chunks::<8>().0 {
        let mut bytes: [u8; 8] = *raw;
        let code = u16::from_be_bytes(bytes[..2].try_into().unwrap());
        let value = u32::from_be_bytes(bytes[4..].try_into().unwrap());
        if code & 0x8000 != 0 {
            // Only data-bearing commands may use a resource-relative pointer.
            if code & 0x7fff != 81 {
                return Err(bad("unsupported resource-relative sound command"));
            }
            let offset = value as usize;
            if offset < command_end || offset.checked_add(22).is_none_or(|end| end > size) {
                return Ok(Err(-206));
            }
            let shape = match playback::layout(&data[offset..]) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            if shape.sample == 0
                && offset
                    .checked_add(shape.header)
                    .and_then(|v| v.checked_add(shape.bytes))
                    .is_none_or(|end| end > size)
            {
                return Ok(Err(-206));
            }
            let Some(pointer) = base.checked_add(value) else {
                return Ok(Err(-206));
            };
            bytes[..2].copy_from_slice(&(code & 0x7fff).to_be_bytes());
            bytes[4..].copy_from_slice(&pointer.to_be_bytes());
        }
        commands.push_back(bytes);
    }
    Ok(Ok(Resource {
        configuration,
        commands,
    }))
}
impl Manager {
    pub fn pause_file(&mut self, m: &Memory, p: &mut Process, channel: u32) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if !self.valid(m, p, channel)? {
                return Ok(BAD_CHANNEL);
            }
            let Some(file) = &mut self.channels.get_mut(&channel).unwrap().file_play else {
                return Ok(-211);
            };
            if !file.active() {
                return Ok(-211);
            }
            file.paused = !file.paused;
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    pub(crate) fn begin_file_play(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        r: crate::sound::StartFilePlay,
    ) -> Result<std::result::Result<Play, i16>> {
        if r.channel == 0 && r.asynchronous {
            return Ok(Err(-205));
        }
        if r.file != 0 || r.selection.0 != 0 || r.completion != 0 || r.buffer.0 != 0 {
            return Err(bad("SndStartFilePlay disk/selection/completion/caller-buffer paths are not implemented"));
        }
        if r.buffer_size < 0 {
            return Ok(Err(-210));
        }
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if r.channel != 0 {
                if !self.valid(m, p, r.channel)? {
                    return Ok(Err(BAD_CHANNEL));
                }
                if self.busy(r.channel) == Some(true) {
                    return Ok(Err(-209));
                }
            }
            let handle = p.get_resource(m, *b"snd ", r.resource)?;
            if handle == 0 {
                return Ok(Err(-192));
            }
            let result = self.begin_play(m, p, r.channel, handle, r.asynchronous)?;
            if let Ok(play) = &result {
                if r.asynchronous {
                    self.channels.get_mut(&play.channel).unwrap().file_play = Some(FilePlay {
                        completion: play.completion.clone(),
                        paused: false,
                    });
                }
            }
            Ok(result)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    pub(crate) fn begin_play(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        channel: u32,
        handle: u32,
        asynchronous: bool,
    ) -> Result<std::result::Result<Play, i16>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if handle == 0 {
                return Ok(Err(-204));
            }
            p.handle_state(m, handle)?;
            if p.mem_error != 0 {
                return Ok(Err(-204));
            }
            let size = p.handle_size(m, handle)?;
            if p.mem_error != 0 {
                return Ok(Err(-204));
            }
            let base = m.u32(GuestPtr(handle))?;
            if base == 0 {
                return Ok(Err(-204));
            }
            let resource = match parse(m, base, size as usize)? {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            if channel != 0 && !self.valid(m, p, channel)? {
                return Ok(Err(BAD_CHANNEL));
            }
            if let Some(c) = self.channels.get(&channel) {
                if c.callback == Some(true)
                    && (!asynchronous
                        || resource.commands.len() > usize::from(c.length) - c.commands.len())
                {
                    return Ok(Err(-209));
                }
            }
            // Check command families and all sample/output spans before allocating
            // an internal channel or publishing any part of the resource.
            let init = if channel != 0 {
                self.channels[&channel].init
            } else {
                match resource.configuration.as_slice() {
                    [] => 0,
                    [(5, init)] => *init,
                    _ => return Ok(Err(-201)),
                }
            };
            // Reuse exactly the normal channel configuration checks and executor.
            let probe = Channel {
                synth: 5,
                init,
                serial: 0,
                owned: false,
                record_size: 0,
                queue: 0,
                length: 128,
                playback: Playback::new(init),
                double: None,
                commands: Default::default(),
                active: None,
                head: 0,
                tail: 0,
                callback: None,
                file_play: None,
            };
            for &bytes in &resource.commands {
                let e = queue::check(m, &probe, bytes, true)?;
                if e != 0 {
                    return Ok(Err(e));
                }
            }
            let effective = if channel != 0 {
                channel
            } else {
                let slot = p.new_pointer(m, 4, true)?;
                if slot == 0 {
                    return Ok(Err(-108));
                }
                let created = self.create(
                    m,
                    p,
                    NewChannel {
                        destination: GuestPtr(slot),
                        synth: 5,
                        init,
                        callback: 0,
                    },
                );
                let pointer = if matches!(created, Ok(0)) {
                    m.u32(GuestPtr(slot))
                } else {
                    Ok(0)
                };
                p.dispose_pointer(slot);
                let e = created?;
                if e != 0 {
                    return Ok(Err(e));
                }
                pointer?
            };
            Ok(Ok(Play {
                channel: effective,
                serial: self.generation(effective),
                internal: channel == 0,
                synchronous: channel == 0 || !asynchronous,
                commands: resource.commands,
                completion: Completion::default(),
            }))
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    /// None means queue space/completion is pending. Completion counts belong
    /// only to this call, so later unrelated work does not prolong a sync play.
    pub(crate) fn poll_play(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        play: &mut Play,
    ) -> Result<Option<i16>> {
        if self.generation(play.channel) != play.serial {
            return Ok(Some(BAD_CHANNEL));
        }
        while let Some(bytes) = play.commands.front().copied() {
            match self.enqueue_for_play(m, p, play.channel, bytes, Some(&play.completion))? {
                None => return Ok(None),
                Some(0) => {
                    play.commands.pop_front();
                }
                Some(e) => return Ok(Some(e)),
            }
        }
        if play.synchronous && !play.completion.done() {
            return Ok(None);
        }
        if play.internal {
            let error = self.call(
                m,
                p,
                Op::Dispose {
                    channel: play.channel,
                    quiet_now: true,
                },
            )?;
            if error != 0 {
                return Ok(Some(error));
            }
            play.internal = false;
        }
        Ok(Some(0))
    }
    pub(crate) fn abandon_play(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        play: &mut Play,
    ) -> Result<()> {
        if play.internal && self.generation(play.channel) == play.serial {
            let error = self.call(
                m,
                p,
                Op::Dispose {
                    channel: play.channel,
                    quiet_now: true,
                },
            )?;
            if error != 0 {
                return Err(bad("failed to release an internal SndPlay channel"));
            }
            play.internal = false;
        }
        Ok(())
    }
}
