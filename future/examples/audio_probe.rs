//! Execute the public MPW sampled-sound fixture, capture actual mixed PCM and
//! optionally submit it to a real native output stream with bounded completion.
use systemless_future::{load::LoadedApplication, runtime::Stop};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) || args.len() == 4 && args[3] != "--device" {
        return Err("usage: audio_probe code|pef APPLICATION WAV_OUTPUT [--device]".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&args[1])?
        .take(systemless_future::load::IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > systemless_future::load::IMAGE_LIMIT {
        return Err("sample application exceeds input limit".into());
    }
    let mut app = match args[0].as_str() {
        "code" => LoadedApplication::code(&bytes)?,
        "pef" => LoadedApplication::pef(&bytes, None)?,
        _ => return Err("expected code or pef".into()),
    };
    if app.runtime.advance(50000)? != (Stop::Waiting { ticks: 1 }) {
        return Err("sample application did not reach its audio wait".into());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let output = if args.len() == 4 {
        Some(systemless_future::audio::Output::open()?)
    } else {
        None
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (rate, frames) = if let Some(output) = &output {
        (output.rate(), output.needed()?)
    } else {
        (8000, 4)
    };
    #[cfg(target_arch = "wasm32")]
    let (rate, frames) = (8000, 4);
    let samples = app.runtime.render_sound(frames, rate)?;
    let data: Vec<u8> = samples
        .iter()
        .flatten()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(&args[2], wav)?;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(output) = &output {
        output.push(&samples)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while output.consumed() < frames as u64 {
            output.queued()?;
            if std::time::Instant::now() >= deadline {
                return Err("native audio callback did not consume submitted frames".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        println!("native diagnostics={:?}", output.diagnostics()?);
        println!(
            "native frames consumed={}, rate={}, callbacks={}",
            output.consumed(),
            rate,
            output.callbacks()
        );
    }
    app.runtime.elapse_ticks(1)?;
    if app.runtime.advance(50000)? != Stop::Exit(0) {
        return Err("sample application did not observe completion and exit successfully".into());
    }
    println!("captured {frames} stereo frames at {rate} Hz; guest completed");
    Ok(())
}
