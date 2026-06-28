//! Host audio output backends.
//!
//! Provides a trait for audio output and implementations for native (cpal)
//! and null (headless/test) backends.

/// Trait for pushing mixed PCM samples to the host audio device.
pub trait AudioBackend {
    /// Queue unsigned 8-bit mono PCM samples (silence = 0x80) at 22050 Hz.
    /// An empty slice signals no audio this frame.
    fn queue_samples(&mut self, samples: &[u8]);

    /// Queue interleaved unsigned 8-bit stereo PCM samples
    /// (left, right, left, right; silence = 0x80) at 22050 Hz.
    ///
    /// Backends that only support the historical mono path can rely on this
    /// default downmix.
    fn queue_stereo_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() {
            return;
        }
        let mut mono = Vec::with_capacity(samples.len() / 2);
        for frame in samples.chunks_exact(2) {
            let left = frame[0] as i32 - 0x80;
            let right = frame[1] as i32 - 0x80;
            mono.push(((left + right) / 2 + 0x80).clamp(0, 255) as u8);
        }
        self.queue_samples(&mono);
    }

    /// Stop audio output and release resources.
    fn stop(&mut self);
}

/// No-op backend for tests, headless mode, and scripted harnesses.
pub struct NullAudioBackend;

impl AudioBackend for NullAudioBackend {
    fn queue_samples(&mut self, _samples: &[u8]) {}
    fn queue_stereo_samples(&mut self, _samples: &[u8]) {}
    fn stop(&mut self) {}
}

/// Diagnostic helper for tests that need to validate the stream shape after
/// the host-output rate conversion policy. Input and output are interleaved
/// unsigned 8-bit stereo frames.
#[doc(hidden)]
pub fn host_audio_probe_output_stereo_u8(samples: &[u8], device_sample_rate: u32) -> Vec<u8> {
    if samples.is_empty() || device_sample_rate == 0 {
        return Vec::new();
    }

    let frames = samples
        .chunks_exact(2)
        .map(|frame| [frame[0], frame[1]])
        .collect::<Vec<_>>();
    let step = crate::sound::OUTPUT_RATE as f32 / device_sample_rate as f32;
    let mut output = Vec::new();
    let mut source_phase = 0.0f32;
    let mut idx = 0usize;

    while let Some(&first) = frames.get(idx) {
        let frame = if step < 1.0 {
            first
        } else {
            let second = frames.get(idx + 1).copied().unwrap_or(first);
            [
                interpolate_u8_for_host_rate(first[0], second[0], source_phase),
                interpolate_u8_for_host_rate(first[1], second[1], source_phase),
            ]
        };
        output.extend_from_slice(&frame);

        source_phase += step;
        while source_phase >= 1.0 {
            idx += 1;
            source_phase -= 1.0;
            if idx >= frames.len() {
                break;
            }
        }
    }

    output
}

fn interpolate_u8_for_host_rate(first: u8, second: u8, phase: f32) -> u8 {
    let first = first as f32;
    let second = second as f32;
    (first + (second - first) * phase).round().clamp(0.0, 255.0) as u8
}

#[cfg(feature = "gui")]
const HOST_AUDIO_PREFILL_MSEC: usize = 90;

/// Maximum buffered audio in seconds. Larger = more latency but more
/// resilience to host scheduling jitter; smaller = lower latency but
/// more underruns under load.
#[cfg(feature = "gui")]
const HOST_AUDIO_MAX_BUFFER_SECS: f32 = 0.25;

#[cfg(feature = "gui")]
const HOST_AUDIO_ACTIVE_FRAME_ENERGY: f32 = 4.0;

#[cfg(feature = "gui")]
const HOST_AUDIO_TRANSIENT_PRESERVE_RATIO: f32 = 1.25;

#[cfg(feature = "gui")]
const HOST_AUDIO_TRANSIENT_PEAK_RATIO: f32 = 1.10;

#[cfg(feature = "gui")]
const HOST_AUDIO_TRANSIENT_PRESERVE_BUDGET_FRAMES: usize = 64;

#[cfg(feature = "gui")]
fn host_audio_prefill_samples() -> usize {
    (crate::sound::OUTPUT_RATE as usize * HOST_AUDIO_PREFILL_MSEC) / 1000
}

#[cfg(feature = "gui")]
fn host_audio_max_buffered_samples() -> usize {
    (crate::sound::OUTPUT_RATE as f32 * HOST_AUDIO_MAX_BUFFER_SECS) as usize
}

#[cfg(feature = "gui")]
static TRACE_AUDIO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[cfg(feature = "gui")]
fn trace_audio_enabled() -> bool {
    *TRACE_AUDIO.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_AUDIO").is_some())
}

/// cpal-based audio backend for native GUI mode.
#[cfg(feature = "gui")]
pub struct CpalAudioBackend {
    /// Shared source buffer and resampler state between the emulator and cpal callback.
    state: std::sync::Arc<std::sync::Mutex<SharedAudioState>>,
    /// The cpal output stream — kept alive to maintain audio playback.
    _stream: cpal::Stream,
}

#[cfg(feature = "gui")]
struct SharedAudioState {
    buffer: std::collections::VecDeque<[u8; 2]>,
    source_phase: f32,
    /// Last non-underrun frame emitted. Kept for diagnostics and reset on
    /// underrun so sparse effects cannot smear into an artificial tail.
    last_frame: [f32; 2],
    /// Number of consecutive callback frames emitted from an empty buffer
    /// since the last successful sample. Used to log underruns.
    underrun_samples: u32,
    /// Incoming frames dropped to keep a queued transient at the head of a
    /// full buffer. Bounded so stale audio cannot starve newer samples when
    /// the GUI briefly outruns the host device.
    transient_preserved_frames: usize,
}

#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameEnergyStats {
    mean: f32,
    peak: f32,
}

#[cfg(feature = "gui")]
impl SharedAudioState {
    fn queue_frames(&mut self, frames: &[[u8; 2]], trace_added_frames: usize) {
        if frames.is_empty() {
            return;
        }

        // Cap ring buffer to avoid unbounded growth / latency. Preserve a
        // queued active transient over weaker incoming samples so short
        // effects are not aged out by later music/silence when the GUI briefly
        // produces audio faster than the host device consumes it.
        let max_buffered = host_audio_max_buffered_samples();
        let mut incoming_start = 0usize;
        while self.buffer.len() + frames.len().saturating_sub(incoming_start) > max_buffered {
            let overflow =
                self.buffer.len() + frames.len().saturating_sub(incoming_start) - max_buffered;
            let front_count = overflow.min(self.buffer.len());
            let incoming_count = overflow.min(frames.len().saturating_sub(incoming_start));
            let front_energy = Self::frame_energy_stats(self.buffer.iter().take(front_count));
            let incoming_energy = Self::frame_energy_stats(
                frames[incoming_start..incoming_start + incoming_count].iter(),
            );
            let transient_budget_available = self
                .transient_preserved_frames
                .saturating_add(incoming_count)
                <= HOST_AUDIO_TRANSIENT_PRESERVE_BUDGET_FRAMES;
            let preserve_front = front_count > 0
                && incoming_count > 0
                && transient_budget_available
                && ((front_energy.mean >= HOST_AUDIO_ACTIVE_FRAME_ENERGY
                    && front_energy.mean
                        > incoming_energy.mean * HOST_AUDIO_TRANSIENT_PRESERVE_RATIO)
                    || (front_energy.peak >= HOST_AUDIO_ACTIVE_FRAME_ENERGY
                        && front_energy.peak
                            > incoming_energy.peak * HOST_AUDIO_TRANSIENT_PEAK_RATIO));

            if preserve_front {
                self.transient_preserved_frames += incoming_count;
                incoming_start += incoming_count;
                if trace_audio_enabled() {
                    eprintln!(
                        "[AUDIO] overflow: dropped {} incoming frames (buffer={}, adding={}, front_mean={:.1}, incoming_mean={:.1}, front_peak={:.1}, incoming_peak={:.1})",
                        incoming_count,
                        self.buffer.len(),
                        trace_added_frames,
                        front_energy.mean,
                        incoming_energy.mean,
                        front_energy.peak,
                        incoming_energy.peak
                    );
                }
                continue;
            }

            let drain_count = overflow.min(self.buffer.len());
            if drain_count > 0 {
                self.buffer.drain(..drain_count);
            }
            self.transient_preserved_frames = 0;
            if drain_count < overflow {
                incoming_start += overflow - drain_count;
            }
            if trace_audio_enabled() {
                eprintln!(
                    "[AUDIO] overflow: dropped {} frames (buffer was {}, adding {})",
                    drain_count,
                    self.buffer.len() + drain_count,
                    trace_added_frames
                );
            }
        }
        self.buffer.extend(frames[incoming_start..].iter().copied());
    }

    fn frame_energy(frame: &[u8; 2]) -> f32 {
        ((frame[0] as i16 - 0x80).abs() + (frame[1] as i16 - 0x80).abs()) as f32 * 0.5
    }

    fn frame_energy_stats<'a>(frames: impl Iterator<Item = &'a [u8; 2]>) -> FrameEnergyStats {
        let mut total = 0.0f32;
        let mut peak = 0.0f32;
        let mut count = 0usize;
        for frame in frames {
            let energy = Self::frame_energy(frame);
            total += energy;
            peak = peak.max(energy);
            count += 1;
        }
        let mean = if count == 0 {
            0.0
        } else {
            total / count as f32
        };
        FrameEnergyStats { mean, peak }
    }

    fn next_frame(&mut self, device_sample_rate: u32) -> [f32; 2] {
        if self.buffer.is_empty() {
            self.underrun_samples = self.underrun_samples.saturating_add(1);
            self.source_phase = 0.0;
            self.last_frame = [0.0, 0.0];
            self.transient_preserved_frames = 0;
            return [0.0, 0.0];
        }

        if self.underrun_samples > 0 && trace_audio_enabled() {
            eprintln!(
                "[AUDIO] underrun ended after {} samples",
                self.underrun_samples
            );
            self.underrun_samples = 0;
        }

        let first = *self.buffer.front().unwrap();
        let step = crate::sound::OUTPUT_RATE as f32 / device_sample_rate as f32;
        let frame = if step < 1.0 {
            // The emulator mixer has already converted guest sounds to the
            // 22 kHz Sound Manager stream. Preserve those samples during
            // host-device upsampling instead of smoothing classic 8-bit
            // effect edges a second time. Sound Manager exposes both linear
            // interpolation and drop-sample conversion as rate-conversion
            // modes. Sound 1994, 2-91 to 2-92.
            [Self::u8_to_f32(first[0]), Self::u8_to_f32(first[1])]
        } else {
            let second = self.buffer.get(1).copied().unwrap_or(first);
            [
                Self::interpolate_sample(first[0], second[0], self.source_phase),
                Self::interpolate_sample(first[1], second[1], self.source_phase),
            ]
        };
        self.last_frame = frame;

        self.source_phase += step;
        let mut consumed_frames = false;
        while self.source_phase >= 1.0 {
            if self.buffer.pop_front().is_none() {
                break;
            }
            consumed_frames = true;
            self.source_phase -= 1.0;
            if self.buffer.is_empty() {
                break;
            }
        }
        if consumed_frames {
            self.transient_preserved_frames = 0;
        }

        frame
    }

    fn interpolate_sample(first: u8, second: u8, phase: f32) -> f32 {
        let first = Self::u8_to_f32(first);
        let second = Self::u8_to_f32(second);
        first + (second - first) * phase
    }

    fn u8_to_f32(sample: u8) -> f32 {
        (sample as f32 - 128.0) / 128.0
    }
}

#[cfg(feature = "gui")]
fn fill_output_f32(
    data: &mut [f32],
    channels: usize,
    state: &std::sync::Arc<std::sync::Mutex<SharedAudioState>>,
    device_sample_rate: u32,
) {
    let mut shared = state.lock().unwrap();
    for frame in data.chunks_mut(channels) {
        let sample = shared.next_frame(device_sample_rate);
        if channels == 1 {
            frame[0] = (sample[0] + sample[1]) * 0.5;
        } else {
            for (idx, channel) in frame.iter_mut().enumerate() {
                *channel = sample[idx % 2];
            }
        }
    }
}

#[cfg(feature = "gui")]
fn fill_output_i16(
    data: &mut [i16],
    channels: usize,
    state: &std::sync::Arc<std::sync::Mutex<SharedAudioState>>,
    device_sample_rate: u32,
) {
    let mut shared = state.lock().unwrap();
    for frame in data.chunks_mut(channels) {
        let sample = shared.next_frame(device_sample_rate);
        if channels == 1 {
            frame[0] = (((sample[0] + sample[1]) * 0.5) * i16::MAX as f32)
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        } else {
            for (idx, channel) in frame.iter_mut().enumerate() {
                let source = sample[idx % 2];
                *channel =
                    (source * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            }
        }
    }
}

#[cfg(feature = "gui")]
fn fill_output_u16(
    data: &mut [u16],
    channels: usize,
    state: &std::sync::Arc<std::sync::Mutex<SharedAudioState>>,
    device_sample_rate: u32,
) {
    let mut shared = state.lock().unwrap();
    for frame in data.chunks_mut(channels) {
        let sample = shared.next_frame(device_sample_rate);
        if channels == 1 {
            frame[0] = ((((sample[0] + sample[1]) * 0.5) * 0.5 + 0.5) * u16::MAX as f32)
                .clamp(0.0, u16::MAX as f32) as u16;
        } else {
            for (idx, channel) in frame.iter_mut().enumerate() {
                let source = sample[idx % 2];
                *channel =
                    ((source * 0.5 + 0.5) * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16;
            }
        }
    }
}

#[cfg(feature = "gui")]
impl CpalAudioBackend {
    /// Create a new cpal audio backend using the device's preferred output format.
    pub fn new() -> Option<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host.default_output_device()?;

        let supported_config = device.default_output_config().ok()?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let channels = config.channels as usize;
        let device_sample_rate = config.sample_rate.0;
        let prefill_samples = host_audio_prefill_samples();

        let state = std::sync::Arc::new(std::sync::Mutex::new(SharedAudioState {
            // Keep a small lead over the device callback so minor host jitter
            // does not translate into audible underruns.
            buffer: {
                let mut buffer =
                    std::collections::VecDeque::with_capacity(crate::sound::OUTPUT_RATE as usize);
                buffer.extend(std::iter::repeat_n([0x80, 0x80], prefill_samples));
                buffer
            },
            source_phase: 0.0,
            // Start at silence (0x80 = 0.0 in f32 mapping).
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        }));

        let err_fn = |err| {
            eprintln!("[AUDIO] cpal stream error: {}", err);
        };
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let state_clone = state.clone();
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            fill_output_f32(data, channels, &state_clone, device_sample_rate);
                        },
                        err_fn,
                        None,
                    )
                    .ok()?
            }
            cpal::SampleFormat::I16 => {
                let state_clone = state.clone();
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            fill_output_i16(data, channels, &state_clone, device_sample_rate);
                        },
                        err_fn,
                        None,
                    )
                    .ok()?
            }
            cpal::SampleFormat::U16 => {
                let state_clone = state.clone();
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            fill_output_u16(data, channels, &state_clone, device_sample_rate);
                        },
                        err_fn,
                        None,
                    )
                    .ok()?
            }
            _ => return None,
        };

        stream.play().ok()?;

        if trace_audio_enabled() {
            eprintln!(
                "[AUDIO] cpal backend started: {} Hz {}ch {:?}, prefill={} samples",
                device_sample_rate, channels, sample_format, prefill_samples
            );
        }

        Some(Self {
            state,
            _stream: stream,
        })
    }
}

#[cfg(feature = "gui")]
impl AudioBackend for CpalAudioBackend {
    fn queue_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() {
            return;
        }
        let frames = samples
            .iter()
            .copied()
            .map(|sample| [sample, sample])
            .collect::<Vec<_>>();
        self.queue_frames(&frames, samples.len());
    }

    fn queue_stereo_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() {
            return;
        }
        let frames = samples
            .chunks_exact(2)
            .map(|frame| [frame[0], frame[1]])
            .collect::<Vec<_>>();
        self.queue_frames(&frames, samples.len() / 2);
    }

    fn stop(&mut self) {
        let mut shared = self.state.lock().unwrap();
        shared.buffer.clear();
        shared.source_phase = 0.0;
        shared.last_frame = [0.0, 0.0];
        shared.underrun_samples = 0;
        shared.transient_preserved_frames = 0;
    }
}

#[cfg(feature = "gui")]
impl CpalAudioBackend {
    fn queue_frames(&mut self, frames: &[[u8; 2]], trace_added_frames: usize) {
        let mut shared = self.state.lock().unwrap();
        shared.queue_frames(frames, trace_added_frames);
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

    #[test]
    fn host_audio_prefill_matches_90ms_target() {
        // 90ms × 22050 Hz = 1984 samples
        assert_eq!(host_audio_prefill_samples(), 1984);
    }

    #[test]
    fn host_audio_max_buffer_matches_latency_cap() {
        assert_eq!(host_audio_max_buffered_samples(), 5512);
        assert!(host_audio_max_buffered_samples() > host_audio_prefill_samples());
    }

    #[test]
    fn host_audio_queue_cap_preserves_new_frames() {
        let max_buffered = host_audio_max_buffered_samples();
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::from(vec![[0x80, 0x80]; max_buffered - 2]),
            source_phase: 0.0,
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };
        let new_frames = [[0x90, 0x91], [0xA0, 0xA1], [0xB0, 0xB1], [0xC0, 0xC1]];

        state.queue_frames(&new_frames, new_frames.len());

        assert_eq!(state.buffer.len(), max_buffered);
        assert_eq!(
            state
                .buffer
                .iter()
                .rev()
                .take(new_frames.len())
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>(),
            new_frames
        );
    }

    #[test]
    fn host_audio_queue_cap_preserves_strong_queued_transient() {
        let max_buffered = host_audio_max_buffered_samples();
        let transient = [[0xF0, 0xF0]; 4];
        let mut queued = transient.to_vec();
        queued.extend(std::iter::repeat_n(
            [0x80, 0x80],
            max_buffered - queued.len(),
        ));
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::from(queued),
            source_phase: 0.0,
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };
        let weaker_tail = [[0x84, 0x84]; 4];

        state.queue_frames(&weaker_tail, weaker_tail.len());

        assert_eq!(state.buffer.len(), max_buffered);
        assert_eq!(
            state
                .buffer
                .iter()
                .take(transient.len())
                .copied()
                .collect::<Vec<_>>(),
            transient,
            "a queued click/effect transient should not be discarded by weaker later samples"
        );
        assert!(
            !state.buffer.iter().any(|frame| weaker_tail.contains(frame)),
            "weaker overflow tail should be dropped before a stronger queued transient"
        );
    }

    #[test]
    fn host_audio_queue_cap_preserves_short_peak_transient() {
        let max_buffered = host_audio_max_buffered_samples();
        let mut queued = vec![[0x80, 0x80]; max_buffered];
        queued[0] = [0xF0, 0xF0];
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::from(queued),
            source_phase: 0.0,
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };
        let weaker_tail = [[0x86, 0x86]; 32];

        state.queue_frames(&weaker_tail, weaker_tail.len());

        assert_eq!(state.buffer.len(), max_buffered);
        assert_eq!(
            state.buffer.front().copied(),
            Some([0xF0, 0xF0]),
            "single-frame click peaks should survive host queue overflow even when their overflow-window mean energy is low"
        );
        assert!(
            !state.buffer.iter().any(|frame| weaker_tail.contains(frame)),
            "weaker incoming frames should be dropped before a queued click peak"
        );
    }

    #[test]
    fn host_audio_queue_cap_bounds_transient_preservation_when_host_lags() {
        let max_buffered = host_audio_max_buffered_samples();
        let mut queued = vec![[0x80, 0x80]; max_buffered];
        queued[0] = [0xF0, 0xF0];
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::from(queued),
            source_phase: 0.0,
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };
        let weaker_tail = [[0x86, 0x86]; 32];

        state.queue_frames(&weaker_tail, weaker_tail.len());
        state.queue_frames(&weaker_tail, weaker_tail.len());
        assert_eq!(
            state.buffer.front().copied(),
            Some([0xF0, 0xF0]),
            "short overflow bursts should still protect the leading click"
        );

        state.queue_frames(&weaker_tail, weaker_tail.len());

        assert_eq!(state.buffer.len(), max_buffered);
        assert_ne!(
            state.buffer.front().copied(),
            Some([0xF0, 0xF0]),
            "transient preservation must be bounded so stale queue head audio cannot starve newer samples"
        );
        assert!(
            state
                .buffer
                .iter()
                .rev()
                .take(weaker_tail.len())
                .any(|frame| weaker_tail.contains(frame)),
            "once the transient budget is spent, current incoming audio should be admitted"
        );
    }

    #[test]
    fn host_audio_upsampling_preserves_queued_sample_edges() {
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::from(vec![
                [0x90, 0x91],
                [0x90, 0x91],
                [0xA0, 0xA1],
                [0xA0, 0xA1],
            ]),
            source_phase: 0.0,
            last_frame: [0.0, 0.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };

        let output = (0..8)
            .map(|_| state.next_frame(crate::sound::OUTPUT_RATE * 2))
            .collect::<Vec<_>>();

        let held_first = [
            SharedAudioState::u8_to_f32(0x90),
            SharedAudioState::u8_to_f32(0x91),
        ];
        let held_second = [
            SharedAudioState::u8_to_f32(0xA0),
            SharedAudioState::u8_to_f32(0xA1),
        ];
        assert_eq!(
            &output[..4],
            &[held_first, held_first, held_first, held_first],
            "host upsampling must not insert a linear midpoint before a low-rate effect edge"
        );
        assert_eq!(
            &output[4..],
            &[held_second, held_second, held_second, held_second],
            "host upsampling must hold the next queued sample after the edge"
        );
    }

    #[test]
    fn host_audio_underrun_outputs_silence_without_smearing_last_frame() {
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::new(),
            source_phase: 0.75,
            last_frame: [1.0, -0.5],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };

        let first = state.next_frame(44_100);
        assert_eq!(first, [0.0, 0.0]);
        assert_eq!(state.last_frame, [0.0, 0.0]);
        assert_eq!(state.source_phase, 0.0);
        assert!(state.underrun_samples > 0);
    }

    #[test]
    fn host_audio_resume_after_underrun_starts_at_next_queued_frame() {
        let mut state = SharedAudioState {
            buffer: std::collections::VecDeque::new(),
            source_phase: 0.75,
            last_frame: [1.0, 1.0],
            underrun_samples: 0,
            transient_preserved_frames: 0,
        };

        assert_eq!(state.next_frame(44_100), [0.0, 0.0]);
        state.queue_frames(&[[0x90, 0x91], [0xA0, 0xA1]], 2);

        let first = state.next_frame(crate::sound::OUTPUT_RATE * 2);
        assert_eq!(
            first,
            [
                SharedAudioState::u8_to_f32(0x90),
                SharedAudioState::u8_to_f32(0x91)
            ],
            "after a starvation gap, host playback should restart on the first queued effect sample"
        );
    }
}
