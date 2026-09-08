//! QuickTime movie media parsing: walk a `moov` atom tree and expand its
//! sample tables into a flat, timeline-ordered list of media samples that can
//! be fed to a codec (Cinepak, QuickTime Animation, …).
//!
//! The `moov` resource holds the structure (`stbl`: `stsd` sample descriptions,
//! `stts` durations, `stsc` sample-to-chunk map, `stsz` sizes, `stco` chunk
//! offsets, `stss` sync samples). The actual compressed bytes live in the data
//! fork (`mdat`); `stco` offsets are absolute into that fork.
//!
//! Reference: Inside Macintosh: QuickTime 1993, chapter on the movie resource
//! and the sample tables.

/// One media sample: a slice of the data fork plus its timeline placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MediaSample {
    /// Absolute byte offset into the data fork.
    pub offset: usize,
    pub size: usize,
    /// Start time in the track's media time scale.
    pub start_time: u32,
    /// Duration in the track's media time scale.
    pub duration: u32,
    /// True if this sample is a sync sample (keyframe). When a track has no
    /// `stss`, every sample is a sync sample.
    pub sync: bool,
}

/// A decoded-video track description plus its expanded sample list.
#[derive(Clone, Debug)]
pub(crate) struct VideoTrack {
    pub codec: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    /// Media time scale (units per second).
    pub time_scale: u32,
    pub samples: Vec<MediaSample>,
    /// Optional CLUT for indexed codecs (e.g. 8-bit QuickTime Animation).
    pub clut: Option<Vec<[u8; 3]>>,
}

impl VideoTrack {
    /// Index of the sample that should be displayed at `media_time` (in this
    /// track's media time scale). Returns the last sample whose `start_time`
    /// is `<= media_time`, or 0.
    pub(crate) fn sample_for_time(&self, media_time: u32) -> Option<usize> {
        if self.samples.is_empty() {
            return None;
        }
        let mut idx = 0usize;
        for (i, s) in self.samples.iter().enumerate() {
            if s.start_time <= media_time {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }
}

fn be16(d: &[u8], o: usize) -> u16 {
    if o + 2 <= d.len() {
        u16::from_be_bytes([d[o], d[o + 1]])
    } else {
        0
    }
}
fn be32(d: &[u8], o: usize) -> u32 {
    if o + 4 <= d.len() {
        u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
    } else {
        0
    }
}

/// Walk direct child atoms of `buf`, invoking `f(type, payload)`. Non-recursive.
fn for_each_atom(buf: &[u8], mut f: impl FnMut(&[u8; 4], &[u8])) {
    let mut off = 0usize;
    while off + 8 <= buf.len() {
        let size = be32(buf, off) as usize;
        let mut atom_type = [0u8; 4];
        atom_type.copy_from_slice(&buf[off + 4..off + 8]);
        let (header, real_size) = if size == 1 {
            // 64-bit size — unsupported/huge; treat as rest of buffer.
            (16usize, buf.len() - off)
        } else if size == 0 {
            (8usize, buf.len() - off)
        } else {
            (8usize, size)
        };
        if real_size < header || off + real_size > buf.len() {
            break;
        }
        f(&atom_type, &buf[off + header..off + real_size]);
        if size == 0 {
            break;
        }
        off += real_size;
    }
}

/// Find the payload of the first child atom of `buf` with the given type.
fn find_atom<'a>(buf: &'a [u8], want: &[u8; 4]) -> Option<&'a [u8]> {
    let mut off = 0usize;
    while off + 8 <= buf.len() {
        let size = be32(buf, off) as usize;
        let (header, real_size) = if size == 1 {
            (16usize, buf.len() - off)
        } else if size == 0 {
            (8usize, buf.len() - off)
        } else {
            (8usize, size)
        };
        if real_size < header || off + real_size > buf.len() {
            break;
        }
        if &buf[off + 4..off + 8] == want {
            return Some(&buf[off + header..off + real_size]);
        }
        if size == 0 {
            break;
        }
        off += real_size;
    }
    None
}

struct SampleTables {
    /// (first_chunk, samples_per_chunk, desc_index)
    stsc: Vec<(u32, u32, u32)>,
    /// per-sample sizes; if `sample_size` != 0 all samples share it.
    sizes: Vec<u32>,
    sample_size: u32,
    sample_count: u32,
    /// chunk offsets.
    chunks: Vec<u32>,
    /// (sample_count, sample_delta) run-length durations.
    stts: Vec<(u32, u32)>,
    /// sync sample indices (1-based); empty => all sync.
    stss: Vec<u32>,
}

fn parse_stsc(d: &[u8]) -> Vec<(u32, u32, u32)> {
    let n = be32(d, 4) as usize;
    (0..n)
        .map(|i| {
            let o = 8 + i * 12;
            (be32(d, o), be32(d, o + 4), be32(d, o + 8))
        })
        .collect()
}
fn parse_stsz(d: &[u8]) -> (u32, u32, Vec<u32>) {
    let sample_size = be32(d, 4);
    let count = be32(d, 8);
    let sizes = if sample_size == 0 {
        (0..count as usize).map(|i| be32(d, 12 + i * 4)).collect()
    } else {
        Vec::new()
    };
    (sample_size, count, sizes)
}
fn parse_stco(d: &[u8]) -> Vec<u32> {
    let n = be32(d, 4) as usize;
    (0..n).map(|i| be32(d, 8 + i * 4)).collect()
}
fn parse_stts(d: &[u8]) -> Vec<(u32, u32)> {
    let n = be32(d, 4) as usize;
    (0..n)
        .map(|i| (be32(d, 8 + i * 8), be32(d, 8 + i * 8 + 4)))
        .collect()
}
fn parse_stss(d: &[u8]) -> Vec<u32> {
    let n = be32(d, 4) as usize;
    (0..n).map(|i| be32(d, 8 + i * 4)).collect()
}

/// Expand the sample tables into a flat, time-ordered sample list.
fn expand_samples(t: &SampleTables) -> Vec<MediaSample> {
    let nchunks = t.chunks.len();
    if nchunks == 0 {
        return Vec::new();
    }
    // samples-per-chunk for every chunk (1-based chunk index).
    let mut spc = vec![0u32; nchunks + 1];
    for (k, &(first, cnt, _)) in t.stsc.iter().enumerate() {
        let last = if k + 1 < t.stsc.len() {
            t.stsc[k + 1].0.saturating_sub(1)
        } else {
            nchunks as u32
        };
        for c in first..=last {
            if (c as usize) <= nchunks {
                spc[c as usize] = cnt;
            }
        }
    }

    let size_of = |i: usize| -> u32 {
        if t.sample_size != 0 {
            t.sample_size
        } else {
            t.sizes.get(i).copied().unwrap_or(0)
        }
    };

    let total = if t.sample_size != 0 {
        t.sample_count as usize
    } else {
        t.sizes.len()
    };

    // Build a per-sample sync lookup.
    let sync_all = t.stss.is_empty();
    let sync_set: std::collections::HashSet<u32> = t.stss.iter().copied().collect();

    // Build per-sample duration from stts run-lengths.
    let mut durations = Vec::with_capacity(total);
    for &(cnt, delta) in &t.stts {
        for _ in 0..cnt {
            durations.push(delta);
        }
    }

    let mut samples = Vec::with_capacity(total);
    let mut sidx = 0usize;
    let mut time = 0u32;
    for c in 1..=nchunks {
        let mut off = t.chunks[c - 1] as usize;
        for _ in 0..spc[c] {
            if sidx >= total {
                break;
            }
            let sz = size_of(sidx) as usize;
            let dur = durations.get(sidx).copied().unwrap_or(0);
            samples.push(MediaSample {
                offset: off,
                size: sz,
                start_time: time,
                duration: dur,
                sync: sync_all || sync_set.contains(&(sidx as u32 + 1)),
            });
            off += sz;
            time = time.wrapping_add(dur);
            sidx += 1;
        }
    }
    samples
}

/// Parse a `moov` resource and return the first video track, expanded against
/// the track's own chunk offsets (absolute into the data fork).
pub(crate) fn parse_video_track(moov: &[u8]) -> Option<VideoTrack> {
    // The moov resource payload begins after this atom's own 8-byte header
    // when it is a standalone `moov` atom; some resources store the payload
    // directly. Detect and unwrap a leading `moov` atom.
    let inner: &[u8] = {
        if moov.len() >= 8 && &moov[4..8] == b"moov" {
            find_atom(moov, b"moov").unwrap_or(moov)
        } else {
            moov
        }
    };

    let mut result: Option<VideoTrack> = None;
    for_each_atom(inner, |t, trak_payload| {
        if result.is_some() || t != b"trak" {
            return;
        }
        let Some(mdia) = find_atom(trak_payload, b"mdia") else {
            return;
        };
        // Media time scale from mdhd.
        let mut media_time_scale = 600u32;
        if let Some(mdhd) = find_atom(mdia, b"mdhd") {
            let version = mdhd.first().copied().unwrap_or(0);
            media_time_scale = if version == 0 {
                be32(mdhd, 12)
            } else {
                be32(mdhd, 20)
            };
        }
        // Handler subtype: only accept video tracks.
        if let Some(hdlr) = find_atom(mdia, b"hdlr") {
            // hdlr: version(1) flags(3) componentType(4) componentSubType(4) ...
            if hdlr.len() >= 12 && &hdlr[8..12] != b"vide" {
                return;
            }
        }
        let Some(minf) = find_atom(mdia, b"minf") else {
            return;
        };
        let Some(stbl) = find_atom(minf, b"stbl") else {
            return;
        };

        let Some(stsd) = find_atom(stbl, b"stsd") else {
            return;
        };
        // stsd: version(1) flags(3) count(4) then sample descriptions.
        // Each desc: size(4) dataFormat(4) resvd(6) dataRefIndex(2) then
        // the ImageDescription body.
        if stsd.len() < 8 + 16 {
            return;
        }
        let desc = &stsd[8..];
        let mut codec = [0u8; 4];
        codec.copy_from_slice(&desc[4..8]);
        // ImageDescription starts at desc[16]: version(2) revision(2)
        // vendor(4) temporalQ(4) spatialQ(4) width(2) height(2) hRes(4)
        // vRes(4) dataSize(4) frameCount(2) name(32) depth(2) clutID(2)
        let id = &desc[16..];
        let width = be16(id, 16);
        let height = be16(id, 18);
        let depth = be16(id, 66);
        let clut_id = be16(id, 68) as i16;

        let stsc = find_atom(stbl, b"stsc").map(parse_stsc).unwrap_or_default();
        let (sample_size, sample_count, sizes) = find_atom(stbl, b"stsz")
            .map(parse_stsz)
            .unwrap_or((0, 0, Vec::new()));
        let chunks = find_atom(stbl, b"stco").map(parse_stco).unwrap_or_default();
        let stts = find_atom(stbl, b"stts").map(parse_stts).unwrap_or_default();
        let stss = find_atom(stbl, b"stss").map(parse_stss).unwrap_or_default();

        let tables = SampleTables {
            stsc,
            sizes,
            sample_size,
            sample_count,
            chunks,
            stts,
            stss,
        };
        let samples = expand_samples(&tables);
        if samples.is_empty() {
            return;
        }

        // Embedded CLUT for indexed codecs: clut_id == 0 means the CLUT is
        // stored inline in the sample description after the ImageDescription.
        let clut = if clut_id == 0 {
            parse_inline_clut(id, depth)
        } else {
            None
        };

        result = Some(VideoTrack {
            codec,
            width,
            height,
            depth,
            time_scale: media_time_scale.max(1),
            samples,
            clut,
        });
    });
    result
}

/// Parse a QuickTime inline colour table that follows the ImageDescription.
/// Layout: seed(4) flags(2) size(2) then (size+1) entries of
/// index(2) r(2) g(2) b(2).
fn parse_inline_clut(id: &[u8], depth: u16) -> Option<Vec<[u8; 3]>> {
    // ImageDescription fixed part is 70 bytes; the CLUT follows.
    let base = 70usize;
    if depth > 8 || id.len() < base + 8 {
        return None;
    }
    let size = be16(id, base + 6) as usize; // count - 1
    let count = size + 1;
    let mut table = vec![[0u8; 3]; 256];
    let mut o = base + 8;
    for _ in 0..count {
        if o + 8 > id.len() {
            break;
        }
        let idx = be16(id, o) as usize;
        let r = (be16(id, o + 2) >> 8) as u8;
        let g = (be16(id, o + 4) >> 8) as u8;
        let b = (be16(id, o + 6) >> 8) as u8;
        if idx < 256 {
            table[idx] = [r, g, b];
        }
        o += 8;
    }
    Some(table)
}

/// A note in QuickTime music media, expressed in seconds and MIDI units.
#[derive(Clone, Debug)]
pub(crate) struct MusicNote {
    pub start: f64,
    pub duration: f64,
    pub pitch: f64,
    pub velocity: f64,
    pub part: u16,
}

/// Decode the note events in a music track. Event bit layouts are specified
/// by Apple's QuickTimeMusic.h (Universal Interfaces), Music Media Events.
/// Samples use the media time scale; rest events advance the event cursor,
/// while note durations do not delay the following event (polyphony).
pub(crate) fn parse_music_track(moov: &[u8], data: &[u8]) -> Option<Vec<MusicNote>> {
    let inner = find_atom(moov, b"moov").unwrap_or(moov);
    let mut result = None;
    for_each_atom(inner, |kind, trak| {
        if kind != b"trak" || result.is_some() {
            return;
        }
        let Some(mdia) = find_atom(trak, b"mdia") else {
            return;
        };
        let Some(hdlr) = find_atom(mdia, b"hdlr") else {
            return;
        };
        if hdlr.get(8..12) != Some(b"musi") {
            return;
        }
        let Some(mdhd) = find_atom(mdia, b"mdhd") else {
            return;
        };
        let scale = be32(mdhd, if mdhd.first() == Some(&1) { 20 } else { 12 });
        if scale == 0 {
            return;
        }
        let Some(stbl) = find_atom(mdia, b"minf").and_then(|m| find_atom(m, b"stbl")) else {
            return;
        };
        let Some(stsd) = find_atom(stbl, b"stsd") else {
            return;
        };
        if stsd.get(12..16) != Some(b"musi") {
            return;
        }
        let Some(sz) = find_atom(stbl, b"stsz") else {
            return;
        };
        let count = be32(sz, 8) as usize;
        // Bound every table by its actual bytes before using the common
        // expansion helpers. Malformed media must not allocate by file counts.
        if count > data.len() / 4 || (be32(sz, 4) == 0 && count > sz.len().saturating_sub(12) / 4) {
            return;
        }
        for (name, width) in [(b"stsc", 12), (b"stco", 4), (b"stts", 8)] {
            let Some(table) = find_atom(stbl, name) else {
                return;
            };
            if be32(table, 4) as usize > table.len().saturating_sub(8) / width {
                return;
            }
        }
        let (sample_size, sample_count, sizes) = parse_stsz(sz);
        let tables = SampleTables {
            stsc: parse_stsc(find_atom(stbl, b"stsc").unwrap()),
            chunks: parse_stco(find_atom(stbl, b"stco").unwrap()),
            stts: parse_stts(find_atom(stbl, b"stts").unwrap()),
            stss: Vec::new(),
            sample_size,
            sample_count,
            sizes,
        };
        if tables.stts.iter().map(|&(n, _)| u64::from(n)).sum::<u64>() != count as u64 {
            return;
        }
        if tables.stsc.iter().any(|&(first, n, _)| {
            first == 0 || first as usize > tables.chunks.len() || n as usize > count
        }) {
            return;
        }
        let mut notes = Vec::new();
        let mut sustain_changes = Vec::new();
        for sample in expand_samples(&tables) {
            let Some(end) = sample.offset.checked_add(sample.size) else {
                return;
            };
            let Some(bytes) = data.get(sample.offset..end) else {
                return;
            };
            let mut time = sample.start_time as f64 / scale as f64;
            let mut off = 0;
            while off + 4 <= bytes.len() {
                let word = be32(bytes, off);
                let length = match word >> 30 {
                    0 | 1 => 1,
                    2 => 2,
                    _ => (word & 0xffff) as usize,
                };
                if length == 0 || length > (bytes.len() - off) / 4 {
                    return;
                }
                let part = ((word >> 24) & 31) as u16;
                match word >> 29 {
                    0 => time += (word & 0xffffff) as f64 / scale as f64,
                    1 => notes.push(MusicNote {
                        start: time,
                        duration: (word & 2047) as f64 / scale as f64,
                        pitch: ((word >> 18) & 63) as f64 + 32.0,
                        velocity: ((word >> 11) & 127) as f64 / 127.0,
                        part,
                    }),
                    2 if (word >> 16) & 255 == 64 => {
                        let down = word & 0xffff >= 64 * 256;
                        sustain_changes.push((part, time, down));
                    }
                    _ if word >> 28 == 9 => {
                        let next = be32(bytes, off + 4);
                        notes.push(MusicNote {
                            start: time,
                            duration: (next & 0x3fffff) as f64 / scale as f64,
                            pitch: (word & 0xffff) as f64,
                            velocity: ((next >> 22) & 127) as f64 / 127.0,
                            part: ((word >> 16) & 4095) as u16,
                        });
                    }
                    _ => {}
                }
                off += length * 4;
            }
        }
        // The sustain pedal holds released notes until the next pedal-up.
        for note in &mut notes {
            let end = note.start + note.duration;
            let down = sustain_changes
                .iter()
                .rev()
                .find(|&&(p, t, _)| p == note.part && t <= end)
                .is_some_and(|&(_, _, down)| down);
            if down {
                if let Some(&(_, up, _)) = sustain_changes
                    .iter()
                    .find(|&&(p, t, down)| p == note.part && t > end && !down)
                {
                    note.duration = up - note.start;
                }
            }
        }
        notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        result = Some(notes);
    });
    result
}

/// Deterministic polyphonic piano synthesis. The oscillators and decaying
/// partials are generated here; no proprietary QuickTime instrument samples
/// are copied. This is an approximation of the instrument timbre.
pub(crate) fn mix_music_notes(
    notes: &[MusicNote],
    start: f64,
    rate: f64,
    volume: f64,
    output: &mut [u8],
) {
    let step = rate / crate::sound::OUTPUT_RATE as f64;
    let finish = start + output.len() as f64 / 2.0 * step;
    for note in notes {
        if note.start > finish {
            break;
        }
        if note.start + note.duration + 0.18 < start || note.velocity == 0.0 {
            continue;
        }
        let frequency = 440.0 * 2.0f64.powf((note.pitch - 69.0) / 12.0);
        for (i, frame) in output.chunks_exact_mut(2).enumerate() {
            let age = start + i as f64 * step - note.start;
            if age < 0.0 || age > note.duration + 0.18 {
                continue;
            }
            let release = if age > note.duration {
                (1.0 - (age - note.duration) / 0.18).max(0.0)
            } else {
                1.0
            };
            let envelope = (age / 0.004).min(1.0) * (-age / 2.8).exp() * release;
            let phase = std::f64::consts::TAU * frequency * age;
            let wave = phase.sin()
                + 0.35 * (2.0 * phase).sin() * (-age * 1.5).exp()
                + 0.15 * (3.0 * phase).sin() * (-age * 3.0).exp();
            let sample = (wave * envelope * note.velocity * volume * 22.0) as i32;
            for channel in frame {
                *channel = (*channel as i32 + sample).clamp(0, 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(t: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
        v.extend_from_slice(t);
        v.extend_from_slice(body);
        v
    }

    fn music_movie(events: &[u32]) -> (Vec<u8>, Vec<u8>) {
        let data: Vec<u8> = events.iter().flat_map(|w| w.to_be_bytes()).collect();
        let mut mdhd = vec![0; 24];
        mdhd[12..16].copy_from_slice(&600u32.to_be_bytes());
        let mut handler = vec![0; 24];
        handler[8..12].copy_from_slice(b"musi");
        let mut description = vec![0; 28];
        description[4..8].copy_from_slice(&1u32.to_be_bytes());
        description[8..12].copy_from_slice(&20u32.to_be_bytes());
        description[12..16].copy_from_slice(b"musi");
        let words = |values: &[u32]| {
            values
                .iter()
                .flat_map(|w| w.to_be_bytes())
                .collect::<Vec<_>>()
        };
        let stbl = [
            atom(b"stsd", &description),
            atom(b"stsc", &words(&[0, 1, 1, 1, 1])),
            atom(b"stco", &words(&[0, 1, 0])),
            atom(b"stsz", &words(&[0, 0, 1, data.len() as u32])),
            atom(b"stts", &words(&[0, 1, 1, 600])),
        ]
        .concat();
        let mdia = [
            atom(b"mdhd", &mdhd),
            atom(b"hdlr", &handler),
            atom(b"minf", &atom(b"stbl", &stbl)),
        ]
        .concat();
        (atom(b"moov", &atom(b"trak", &atom(b"mdia", &mdia))), data)
    }

    #[test]
    fn music_events_preserve_polyphony_pitch_duration_and_sustain() {
        let note = 0x20000000 | ((60 - 32) << 18) | (100 << 11) | 60;
        let (moov, data) = music_movie(&[
            600,
            0x40407f00,
            note,
            0x90000045,
            (100 << 22) | 120,
            180,
            0x40400000,
        ]);
        let notes = parse_music_track(&moov, &data).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].start, 1.0);
        assert_eq!(notes[1].start, 1.0);
        assert_eq!(notes[0].pitch, 60.0);
        assert_eq!(notes[1].pitch, 69.0);
        assert!((notes[0].duration - 0.3).abs() < 1e-9);
        assert!((notes[1].duration - 0.3).abs() < 1e-9);
    }

    #[test]
    fn music_rejects_truncated_events_and_out_of_file_sample_offsets() {
        let (moov, data) = music_movie(&[0x90000045]);
        assert!(parse_music_track(&moov, &data).is_none());
        let (mut moov, data) = music_movie(&[600, 0x20001020]);
        let pos = moov.windows(4).position(|s| s == b"stco").unwrap();
        moov[pos + 12..pos + 16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_music_track(&moov, &data).is_none());
    }

    #[test]
    fn music_synthesis_obeys_rest_release_and_volume() {
        let notes = vec![MusicNote {
            start: 1.0,
            duration: 0.25,
            pitch: 69.0,
            velocity: 0.8,
            part: 0,
        }];
        let mut silence = vec![128; 2205 * 2];
        mix_music_notes(&notes, 0.0, 1.0, 1.0, &mut silence);
        assert!(silence.iter().all(|&v| v == 128));
        let mut audible = silence.clone();
        mix_music_notes(&notes, 1.0, 1.0, 1.0, &mut audible);
        assert!(audible.iter().filter(|&&v| v != 128).count() > 3000);
        let mut muted = silence.clone();
        mix_music_notes(&notes, 1.0, 1.0, 0.0, &mut muted);
        assert_eq!(muted, silence);
        mix_music_notes(&notes, 1.5, 1.0, 1.0, &mut silence);
        assert_eq!(muted, silence);
        let mut whole = vec![128; 4410];
        mix_music_notes(&notes, 1.0, 1.0, 1.0, &mut whole);
        let mut split = vec![128; 4410];
        mix_music_notes(&notes, 1.0, 1.0, 1.0, &mut split[..2200]);
        mix_music_notes(&notes, 1.0 + 1100.0 / 22050.0, 1.0, 1.0, &mut split[2200..]);
        assert_eq!(whole, split);
    }

    #[test]
    fn expand_maps_stsc_stsz_stco_to_offsets() {
        // 3 chunks. stsc: chunk1 has 1 sample, chunks 2-3 have 2 samples each
        // => 5 samples. sizes 10,20,30,40,50. chunk offsets 1000,2000,3000.
        let tables = SampleTables {
            stsc: vec![(1, 1, 1), (2, 2, 1)],
            sizes: vec![10, 20, 30, 40, 50],
            sample_size: 0,
            sample_count: 5,
            chunks: vec![1000, 2000, 3000],
            stts: vec![(5, 100)],
            stss: vec![],
        };
        let s = expand_samples(&tables);
        assert_eq!(s.len(), 5);
        // chunk1: sample0 @1000 size10
        assert_eq!((s[0].offset, s[0].size), (1000, 10));
        // chunk2: sample1 @2000 size20, sample2 @2020 size30
        assert_eq!((s[1].offset, s[1].size), (2000, 20));
        assert_eq!((s[2].offset, s[2].size), (2020, 30));
        // chunk3: sample3 @3000 size40, sample4 @3040 size50
        assert_eq!((s[3].offset, s[3].size), (3000, 40));
        assert_eq!((s[4].offset, s[4].size), (3040, 50));
        // timeline
        assert_eq!(s[0].start_time, 0);
        assert_eq!(s[1].start_time, 100);
        assert_eq!(s[4].start_time, 400);
        // no stss => all sync
        assert!(s.iter().all(|x| x.sync));
    }

    #[test]
    fn stss_marks_only_listed_samples_sync() {
        let tables = SampleTables {
            stsc: vec![(1, 4, 1)],
            sizes: vec![1, 1, 1, 1],
            sample_size: 0,
            sample_count: 4,
            chunks: vec![0],
            stts: vec![(4, 10)],
            stss: vec![1, 3],
        };
        let s = expand_samples(&tables);
        assert!(s[0].sync);
        assert!(!s[1].sync);
        assert!(s[2].sync);
        assert!(!s[3].sync);
    }

    #[test]
    fn parse_video_track_reads_cvid_stsd_and_samples() {
        // Build a minimal moov with one video trak carrying a cvid stsd and
        // one sample. ImageDescription: width=212 height=168 depth=24.
        let mut idesc = vec![0u8; 70];
        idesc[16] = 0;
        idesc[17] = 212u8; // width low byte (212)
        idesc[18] = 0;
        idesc[19] = 168u8; // height
        idesc[66] = 0;
        idesc[67] = 24; // depth
        idesc[68] = 0xFF;
        idesc[69] = 0xFF; // clutID = -1
        let mut desc = Vec::new();
        desc.extend_from_slice(&((16 + idesc.len()) as u32).to_be_bytes());
        desc.extend_from_slice(b"cvid");
        desc.extend_from_slice(&[0u8; 6]); // resvd
        desc.extend_from_slice(&1u16.to_be_bytes()); // dataRefIndex
        desc.extend_from_slice(&idesc);
        let mut stsd_body = vec![0u8, 0, 0, 0]; // version/flags
        stsd_body.extend_from_slice(&1u32.to_be_bytes()); // count
        stsd_body.extend_from_slice(&desc);
        let stsd = atom(b"stsd", &stsd_body);

        let mut stsz_body = vec![0, 0, 0, 0]; // version/flags
        stsz_body.extend_from_slice(&0u32.to_be_bytes()); // sample_size=0
        stsz_body.extend_from_slice(&1u32.to_be_bytes()); // count
        stsz_body.extend_from_slice(&9224u32.to_be_bytes());
        let stsz = atom(b"stsz", &stsz_body);

        let mut stsc_body = vec![0, 0, 0, 0];
        stsc_body.extend_from_slice(&1u32.to_be_bytes()); // 1 entry
        stsc_body.extend_from_slice(&1u32.to_be_bytes()); // first_chunk
        stsc_body.extend_from_slice(&1u32.to_be_bytes()); // samples_per_chunk
        stsc_body.extend_from_slice(&1u32.to_be_bytes()); // desc_index
        let stsc = atom(b"stsc", &stsc_body);

        let mut stco_body = vec![0, 0, 0, 0];
        stco_body.extend_from_slice(&1u32.to_be_bytes());
        stco_body.extend_from_slice(&20037u32.to_be_bytes());
        let stco = atom(b"stco", &stco_body);

        let mut stts_body = vec![0, 0, 0, 0];
        stts_body.extend_from_slice(&1u32.to_be_bytes());
        stts_body.extend_from_slice(&1u32.to_be_bytes()); // count
        stts_body.extend_from_slice(&11u32.to_be_bytes()); // delta
        let stts = atom(b"stts", &stts_body);

        let mut stbl_body = Vec::new();
        stbl_body.extend_from_slice(&stsd);
        stbl_body.extend_from_slice(&stts);
        stbl_body.extend_from_slice(&stsc);
        stbl_body.extend_from_slice(&stsz);
        stbl_body.extend_from_slice(&stco);
        let stbl = atom(b"stbl", &stbl_body);

        let mut hdlr_body = vec![0u8; 8];
        hdlr_body.extend_from_slice(b"vide");
        let hdlr = atom(b"hdlr", &hdlr_body);

        let mut mdhd_body = vec![0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        mdhd_body.extend_from_slice(&600u32.to_be_bytes()); // time scale @12
        mdhd_body.extend_from_slice(&8420u32.to_be_bytes());
        let mdhd = atom(b"mdhd", &mdhd_body);

        let minf = atom(b"minf", &stbl);

        let mut mdia_body = Vec::new();
        mdia_body.extend_from_slice(&mdhd);
        mdia_body.extend_from_slice(&hdlr);
        mdia_body.extend_from_slice(&minf);
        let mdia = atom(b"mdia", &mdia_body);
        let trak = atom(b"trak", &mdia);
        let moov = atom(b"moov", &trak);

        let vt = parse_video_track(&moov).expect("video track");
        assert_eq!(&vt.codec, b"cvid");
        assert_eq!(vt.width, 212);
        assert_eq!(vt.height, 168);
        assert_eq!(vt.depth, 24);
        assert_eq!(vt.time_scale, 600);
        assert_eq!(vt.samples.len(), 1);
        assert_eq!(vt.samples[0].offset, 20037);
        assert_eq!(vt.samples[0].size, 9224);
    }
}
