//! Explicit bounded headless sample pulls; this is not a realtime host clock.
use std::{error::Error, io::Write};
use systemless_future::load::LoadedApplication;
pub struct Checkpoint {
    pub instruction: u64,
    frames: usize,
    rate: u32,
    path: String,
}
impl Checkpoint {
    pub fn parse(spec: &str, budget: u64) -> Result<Self, Box<dyn Error>> {
        let parts: Vec<_> = spec.splitn(4, ':').collect();
        if parts.len() != 4 {
            return Err("audio checkpoint requires INSTRUCTION:FRAMES:RATE:NEW_WAV".into());
        }
        let instruction = parts[0].parse()?;
        let frames = parts[1].parse()?;
        let rate = parts[2].parse()?;
        if instruction >= budget
            || !(1..=65536).contains(&frames)
            || !(8000..=192000).contains(&rate)
            || parts[3].is_empty()
        {
            return Err(
                "audio checkpoint exceeds instruction/sample/rate bounds or has an empty path"
                    .into(),
            );
        }
        Ok(Self {
            instruction,
            frames,
            rate,
            path: parts[3].into(),
        })
    }
    pub fn render(&self, app: &mut LoadedApplication, poll: u64) -> Result<(), Box<dyn Error>> {
        // Refuse replacement before consuming guest samples. A failed render may
        // leave an empty diagnostic file; it never invents a completed WAV.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)?;
        let samples = app.runtime.render_sound(self.frames, self.rate)?;
        let bytes = (samples.len() * 4) as u32;
        let mut header = Vec::with_capacity(44);
        header.extend(b"RIFF");
        header.extend((36 + bytes).to_le_bytes());
        header.extend(b"WAVEfmt ");
        header.extend(16u32.to_le_bytes());
        header.extend(1u16.to_le_bytes());
        header.extend(2u16.to_le_bytes());
        header.extend(self.rate.to_le_bytes());
        header.extend((self.rate * 4).to_le_bytes());
        header.extend(4u16.to_le_bytes());
        header.extend(16u16.to_le_bytes());
        header.extend(b"data");
        header.extend(bytes.to_le_bytes());
        file.write_all(&header)?;
        let mut peak = 0u16;
        let mut nonzero = 0;
        let mut pcm = Vec::with_capacity(bytes as usize);
        for frame in samples {
            nonzero += usize::from(frame != [0; 2]);
            for sample in frame {
                peak = peak.max(sample.unsigned_abs());
                pcm.extend(sample.to_le_bytes());
            }
        }
        file.write_all(&pcm)?;
        file.sync_all()?;
        println!("audio checkpoint requested={} instruction={} poll={} frames={} rate={} nonzero_frames={} peak={} path={:?}", self.instruction, app.runtime.instructions, poll, self.frames, self.rate, nonzero, peak, self.path);
        Ok(())
    }
}
