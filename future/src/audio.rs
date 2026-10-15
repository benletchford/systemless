//! Thin native audio transport. The callback owns only mixed frames, never
//! guest memory or CPU state; the Macintosh sound manager runs on its owner.
use crate::{Error, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
struct Shared {
    frames: VecDeque<[i16; 2]>,
    error: Option<String>,
    xruns: u64,
    realtime_denied: bool,
}
pub struct Output {
    _stream: cpal::Stream,
    shared: Arc<Mutex<Shared>>,
    callbacks: Arc<AtomicU64>,
    consumed: Arc<AtomicU64>,
    rate: u32,
    target: usize,
}
fn bad(message: impl ToString) -> Error {
    Error::Sound(message.to_string())
}
impl Output {
    pub fn open() -> Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| bad("no native audio output device"))?;
        let supported = device.default_output_config().map_err(bad)?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        if config.channels == 0 || !(8000..=192000).contains(&config.sample_rate) {
            return Err(bad("unsupported native audio configuration"));
        }
        let rate = config.sample_rate;
        let target = rate as usize / 20;
        let shared = Arc::new(Mutex::new(Shared {
            frames: VecDeque::with_capacity(target),
            error: None,
            xruns: 0,
            realtime_denied: false,
        }));
        let callbacks = Arc::new(AtomicU64::new(0));
        let consumed = Arc::new(AtomicU64::new(0));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(
                &device,
                config,
                shared.clone(),
                callbacks.clone(),
                consumed.clone(),
            )?,
            cpal::SampleFormat::I16 => build::<i16>(
                &device,
                config,
                shared.clone(),
                callbacks.clone(),
                consumed.clone(),
            )?,
            cpal::SampleFormat::U16 => build::<u16>(
                &device,
                config,
                shared.clone(),
                callbacks.clone(),
                consumed.clone(),
            )?,
            cpal::SampleFormat::F64 => build::<f64>(
                &device,
                config,
                shared.clone(),
                callbacks.clone(),
                consumed.clone(),
            )?,
            _ => return Err(bad("unsupported native audio sample format")),
        };
        stream.play().map_err(bad)?;
        Ok(Self {
            _stream: stream,
            shared,
            callbacks,
            consumed,
            rate,
            target,
        })
    }
    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Relaxed)
    }
    pub fn diagnostics(&self) -> Result<(u64, bool)> {
        let state = self.shared.lock().map_err(bad)?;
        Ok((state.xruns, state.realtime_denied))
    }
    pub fn queued(&self) -> Result<usize> {
        let state = self.shared.lock().map_err(bad)?;
        if let Some(error) = &state.error {
            return Err(bad(error));
        }
        Ok(state.frames.len())
    }
    pub fn needed(&self) -> Result<usize> {
        Ok(self.target.saturating_sub(self.queued()?))
    }
    pub fn push(&self, frames: &[[i16; 2]]) -> Result<()> {
        let mut state = self.shared.lock().map_err(bad)?;
        if let Some(error) = &state.error {
            return Err(bad(error));
        }
        if frames.len() > self.target.saturating_sub(state.frames.len()) {
            return Err(bad("native audio transport capacity exceeded"));
        }
        state.frames.extend(frames.iter().copied());
        Ok(())
    }
}
fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    shared: Arc<Mutex<Shared>>,
    callbacks: Arc<AtomicU64>,
    consumed: Arc<AtomicU64>,
) -> Result<cpal::Stream> {
    let channels = usize::from(config.channels);
    let errors = shared.clone();
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut state = shared.try_lock().ok();
                let mut count = 0;
                for frame in output.chunks_mut(channels) {
                    let pair = state.as_mut().and_then(|s| s.frames.pop_front());
                    if pair.is_some() {
                        count += 1;
                    }
                    let pair = pair.unwrap_or([0; 2]);
                    for (i, sample) in frame.iter_mut().enumerate() {
                        let value = if channels == 1 {
                            (i32::from(pair[0]) + i32::from(pair[1])) as f32 / 65536.0
                        } else if i < 2 {
                            f32::from(pair[i]) / 32768.0
                        } else {
                            0.0
                        };
                        *sample = T::from_sample(value);
                    }
                }
                consumed.fetch_add(count, Ordering::Relaxed);
                callbacks.fetch_add(1, Ordering::Relaxed);
            },
            move |error| {
                if let Ok(mut state) = errors.lock() {
                    match error.kind() {
                        cpal::ErrorKind::Xrun => state.xruns = state.xruns.saturating_add(1),
                        cpal::ErrorKind::RealtimeDenied => state.realtime_denied = true,
                        _ => state.error = Some(error.to_string()),
                    }
                }
            },
            Some(Duration::from_secs(2)),
        )
        .map_err(bad)
}
