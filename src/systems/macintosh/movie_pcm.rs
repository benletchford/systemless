//! Uncompressed QuickTime sound tracks, independent of either CPU adapter.
//! <https://developer.apple.com/documentation/quicktime-file-format/sound_sample_descriptions>
use super::movie_timeline::TrackTimeline;

#[derive(Clone, Debug)]
struct TimeRun {
    start: u64,
    duration: u32,
    first: usize,
    count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PcmTrack {
    pub(crate) frames: Vec<[i16; 2]>,
    media_scale: u32,
    movie_scale: u32,
    timeline: Option<TrackTimeline>,
    times: Vec<TimeRun>,
    volume: i16,
}

impl PcmTrack {
    fn sample(&self, seconds: f64) -> Option<[i16; 2]> {
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let media = if let Some(timeline) = &self.timeline {
            let mut start = 0.0;
            let mut mapped = None;
            for edit in &timeline.edits {
                let end = start + edit.duration as f64 / self.movie_scale as f64;
                if seconds < end {
                    if edit.media_time >= 0 {
                        mapped = Some(
                            edit.media_time as f64
                                + (seconds - start) * self.media_scale as f64 * edit.rate as f64
                                    / 65536.0,
                        );
                    }
                    break;
                }
                start = end;
            }
            mapped?
        } else {
            seconds * self.media_scale as f64
        };
        if media < 0.0 {
            return None;
        }
        let run = self.times.get(
            self.times
                .partition_point(|run| run.start as f64 <= media)
                .checked_sub(1)?,
        )?;
        let index = ((media - run.start as f64) / run.duration as f64 + 1e-9).floor() as usize;
        if index >= run.count {
            return None;
        }
        self.frames.get(run.first.checked_add(index)?).copied()
    }

    pub(crate) fn mix(&self, seconds: f64, rate: f64, volume: i16, output: &mut [u8]) {
        for (index, frame) in output.chunks_exact_mut(2).enumerate() {
            let time = seconds + index as f64 * rate / crate::sound::OUTPUT_RATE as f64;
            let Some(sample) = self.sample(time) else {
                continue;
            };
            for channel in 0..2 {
                let value = i64::from(sample[channel])
                    * i64::from(volume.max(0))
                    * i64::from(self.volume.max(0))
                    / (256 * 256 * 256);
                frame[channel] = (i64::from(frame[channel]) + value).clamp(0, 255) as u8;
            }
        }
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn atoms(mut bytes: &[u8]) -> Option<Vec<([u8; 4], &[u8])>> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let size = u32_at(bytes, 0)?;
        let kind = bytes.get(4..8)?.try_into().ok()?;
        let (header, size) = match size {
            0 => (8, bytes.len()),
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?)).ok()?,
            ),
            n => (8, n as usize),
        };
        if size < header {
            return None;
        }
        result.push((kind, bytes.get(header..size)?));
        bytes = bytes.get(size..)?;
    }
    Some(result)
}
fn child<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    atoms(bytes)?
        .into_iter()
        .find_map(|(k, data)| (k == *kind).then_some(data))
}
fn table(bytes: &[u8], width: usize) -> Option<&[u8]> {
    let count = usize::try_from(u32_at(bytes, 4)?).ok()?;
    bytes.get(8..8usize.checked_add(count.checked_mul(width)?)?)
}

struct Description {
    codec: [u8; 4],
    channels: usize,
    bits: usize,
}
impl Description {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let codec = bytes.get(4..8)?.try_into().ok()?;
        let channels = usize::from(u16_at(bytes, 24)?);
        let bits = usize::from(u16_at(bytes, 26)?);
        if u16_at(bytes, 14)? != 1
            || u16_at(bytes, 16)? != 0
            || u16_at(bytes, 28)? != 0
            || u32_at(bytes, 32)? == 0
            || !(1..=2).contains(&channels)
            || ![8, 16].contains(&bits)
            || !(codec == *b"twos" || codec == *b"sowt" || (codec == *b"raw " && bits == 8))
        {
            return None;
        }
        Some(Self {
            codec,
            channels,
            bits,
        })
    }
    fn frame_bytes(&self) -> usize {
        self.channels * self.bits / 8
    }
    fn decode(&self, bytes: &[u8]) -> [i16; 2] {
        let value = |channel: usize| {
            if self.bits == 8 {
                if self.codec == *b"raw " {
                    (i16::from(bytes[channel]) - 128) << 8
                } else {
                    i16::from(bytes[channel] as i8) << 8
                }
            } else {
                let pair = [bytes[channel * 2], bytes[channel * 2 + 1]];
                if self.codec == *b"sowt" {
                    i16::from_le_bytes(pair)
                } else {
                    i16::from_be_bytes(pair)
                }
            }
        };
        [value(0), value(self.channels - 1)]
    }
}

fn parse_track(trak: &[u8], data: &[u8], movie_scale: u32) -> Option<PcmTrack> {
    let tkhd = child(trak, b"tkhd")?;
    if u32_at(tkhd, 0)? & 1 == 0 {
        return None;
    }
    let volume_offset = match tkhd.first()? {
        0 => 36,
        1 => 48,
        _ => return None,
    };
    let volume = u16_at(tkhd, volume_offset)? as i16;
    let mdia = child(trak, b"mdia")?;
    if child(mdia, b"hdlr")?.get(8..12)? != b"soun" {
        return None;
    }
    let mdhd = child(mdia, b"mdhd")?;
    let scale_offset = match mdhd.first()? {
        0 => 12,
        1 => 20,
        _ => return None,
    };
    let media_scale = u32_at(mdhd, scale_offset)?;
    if media_scale == 0 {
        return None;
    }
    let timeline = if let Some(edts) = child(trak, b"edts") {
        Some(TrackTimeline::parse(child(edts, b"elst")?)?)
    } else {
        None
    };
    let stbl = child(child(mdia, b"minf")?, b"stbl")?;
    let stsd = child(stbl, b"stsd")?;
    let mut description_bytes = stsd.get(8..)?;
    let mut descriptions = Vec::new();
    let description_count = u32_at(stsd, 4)? as usize;
    if description_count > description_bytes.len() / 36 {
        return None;
    }
    for _ in 0..description_count {
        let size = u32_at(description_bytes, 0)? as usize;
        if size < 36 {
            return None;
        }
        descriptions.push(Description::parse(description_bytes.get(..size)?));
        description_bytes = description_bytes.get(size..)?;
    }
    let sizes = child(stbl, b"stsz")?;
    let default_size = u32_at(sizes, 4)? as usize;
    let count = u32_at(sizes, 8)? as usize;
    if count == 0 {
        return None;
    }
    let variable_sizes = if default_size == 0 {
        Some(sizes.get(12..12usize.checked_add(count.checked_mul(4)?)?)?)
    } else {
        None
    };
    let stsc = table(child(stbl, b"stsc")?, 12)?;
    let mappings = stsc
        .chunks_exact(12)
        .map(|row| {
            Some((
                u32_at(row, 0)? as usize,
                u32_at(row, 4)? as usize,
                u32_at(row, 8)? as usize,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let offsets = if let Some(stco) = child(stbl, b"stco") {
        table(stco, 4)?
            .chunks_exact(4)
            .map(|row| Some(u32_at(row, 0)? as usize))
            .collect::<Option<Vec<_>>>()?
    } else {
        table(child(stbl, b"co64")?, 8)?
            .chunks_exact(8)
            .map(|row| usize::try_from(u64::from_be_bytes(row.try_into().ok()?)).ok())
            .collect::<Option<Vec<_>>>()?
    };
    if mappings.first()?.0 != 1
        || mappings.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        || mappings.iter().any(|&(first, n, desc)| {
            first > offsets.len() || n == 0 || n > count || desc == 0 || desc > descriptions.len()
        })
    {
        return None;
    }
    // Allocate only after validating each physical chunk, never from a
    // possibly corrupt logical sample count. Chunks may reuse source bytes.
    let mut frames = Vec::new();
    for (chunk, &offset) in offsets.iter().enumerate() {
        let mapping = mappings.iter().rfind(|mapping| mapping.0 <= chunk + 1)?;
        let desc = descriptions.get(mapping.2 - 1)?.as_ref()?;
        let n = mapping.1;
        if frames.len().checked_add(n)? > count {
            return None;
        }
        let bytes = data.get(offset..offset.checked_add(n.checked_mul(desc.frame_bytes())?)?)?;
        for raw in bytes.chunks_exact(desc.frame_bytes()) {
            let size = if let Some(sizes) = variable_sizes {
                u32_at(sizes, frames.len() * 4)? as usize
            } else {
                default_size
            };
            // Classic sound tables may count logical one-byte samples; the
            // SoundDescription supplies the physical interleaved frame width.
            if size != 1 && size != desc.frame_bytes() {
                return None;
            }
            frames.push(desc.decode(raw));
        }
    }
    if frames.len() != count {
        return None;
    }
    let mut times = Vec::new();
    let mut start = 0u64;
    let mut first = 0usize;
    for row in table(child(stbl, b"stts")?, 8)?.chunks_exact(8) {
        let n = u32_at(row, 0)? as usize;
        let duration = u32_at(row, 4)?;
        if n == 0 || duration == 0 {
            return None;
        }
        times.push(TimeRun {
            start,
            duration,
            first,
            count: n,
        });
        first = first.checked_add(n)?;
        start = start.checked_add((n as u64).checked_mul(u64::from(duration))?)?;
    }
    if first != count {
        return None;
    }
    Some(PcmTrack {
        frames,
        media_scale,
        movie_scale,
        timeline,
        times,
        volume,
    })
}

pub(crate) fn parse_tracks(moov: &[u8], data: &[u8], movie_scale: u32) -> Vec<PcmTrack> {
    if movie_scale == 0 {
        return Vec::new();
    }
    let inner = child(moov, b"moov").unwrap_or(moov);
    atoms(inner)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(kind, trak)| {
            (kind == *b"trak")
                .then(|| parse_track(trak, data, movie_scale))
                .flatten()
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
