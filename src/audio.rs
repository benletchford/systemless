//! Host audio output backends.
//!
//! Provides a trait for audio output and implementations for native (cpal)
//! and null (headless/test) backends.

/// Trait for pushing mixed PCM samples to the host audio device.
pub trait AudioBackend {
    /// Queue unsigned 8-bit mono PCM samples (silence = 0x80) at 22050 Hz.
    /// An empty slice signals no audio this frame.
    fn queue_samples(&mut self, samples: &[u8]);

    /// Stop audio output and release resources.
    fn stop(&mut self);
}

/// No-op backend for tests, headless mode, and scripted harnesses.
pub struct NullAudioBackend;

impl AudioBackend for NullAudioBackend {
    fn queue_samples(&mut self, _samples: &[u8]) {}
    fn stop(&mut self) {}
}

#[cfg(feature = "gui")]
const HOST_AUDIO_PREFILL_MSEC: usize = 250;

/// Maximum buffered audio in seconds. Larger = more latency but more
/// resilience to host scheduling jitter; smaller = lower latency but
/// more underruns under load.
#[cfg(feature = "gui")]
const HOST_AUDIO_MAX_BUFFER_SECS: f32 = 0.5;

#[cfg(feature = "gui")]
fn host_audio_prefill_samples() -> usize {
    (crate::sound::OUTPUT_RATE as usize * HOST_AUDIO_PREFILL_MSEC) / 1000
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
    buffer: std::collections::VecDeque<u8>,
    source_phase: f32,
    /// Last sample emitted, held during underruns so the speaker stays at
    /// the current DC level instead of snapping to 0.0 (which would click).
    last_sample: f32,
    /// Number of consecutive callback samples emitted from an empty buffer
    /// since the last successful sample. Used to log underruns.
    underrun_samples: u32,
}

#[cfg(feature = "gui")]
impl SharedAudioState {
    fn next_sample(&mut self, device_sample_rate: u32) -> f32 {
        if self.buffer.is_empty() {
            // Hold the last sample value rather than snapping to 0.0 — a
            // hard jump to silence is audible as a click. The DC level we
            // hold is whatever voltage the speaker was last driven to.
            self.underrun_samples = self.underrun_samples.saturating_add(1);
            return self.last_sample;
        }

        if self.underrun_samples > 0 && trace_audio_enabled() {
            eprintln!(
                "[AUDIO] underrun ended after {} samples",
                self.underrun_samples
            );
            self.underrun_samples = 0;
        }

        let step = crate::sound::OUTPUT_RATE as f32 / device_sample_rate as f32;
        let first = Self::u8_to_f32(*self.buffer.front().unwrap());
        let second = self
            .buffer
            .get(1)
            .copied()
            .map(Self::u8_to_f32)
            .unwrap_or(first);
        let sample = first + (second - first) * self.source_phase;
        self.last_sample = sample;

        self.source_phase += step;
        while self.source_phase >= 1.0 {
            if self.buffer.pop_front().is_none() {
                break;
            }
            self.source_phase -= 1.0;
            if self.buffer.is_empty() {
                break;
            }
        }

        sample
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
        let sample = shared.next_sample(device_sample_rate);
        for channel in frame {
            *channel = sample;
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
        let sample = shared.next_sample(device_sample_rate);
        let converted = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        for channel in frame {
            *channel = converted;
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
        let sample = shared.next_sample(device_sample_rate);
        let converted = ((sample * 0.5 + 0.5) * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16;
        for channel in frame {
            *channel = converted;
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
                buffer.extend(std::iter::repeat_n(0x80, prefill_samples));
                buffer
            },
            source_phase: 0.0,
            // Start at silence (0x80 = 0.0 in f32 mapping).
            last_sample: 0.0,
            underrun_samples: 0,
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
        let mut shared = self.state.lock().unwrap();
        // Cap ring buffer to avoid unbounded growth / latency. We dimension
        // this against HOST_AUDIO_MAX_BUFFER_SECS so the latency budget is
        // explicit at the top of the file.
        let max_buffered = (crate::sound::OUTPUT_RATE as f32 * HOST_AUDIO_MAX_BUFFER_SECS) as usize;
        let total = shared.buffer.len() + samples.len();
        if total > max_buffered {
            let overflow = total - max_buffered;
            let drain_count = overflow.min(shared.buffer.len());
            shared.buffer.drain(..drain_count);
            if trace_audio_enabled() {
                eprintln!(
                    "[AUDIO] overflow: dropped {} samples (buffer was {}, adding {})",
                    drain_count,
                    shared.buffer.len() + drain_count,
                    samples.len()
                );
            }
        }
        shared.buffer.extend(samples.iter().copied());
    }

    fn stop(&mut self) {
        let mut shared = self.state.lock().unwrap();
        shared.buffer.clear();
        shared.source_phase = 0.0;
        shared.last_sample = 0.0;
        shared.underrun_samples = 0;
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

    #[test]
    fn host_audio_prefill_matches_250ms_target() {
        // 250ms × 22050 Hz = 5512 samples
        assert_eq!(host_audio_prefill_samples(), 5512);
    }
}
