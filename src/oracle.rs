//! Cross-runtime parity oracle.
//!
//! When two emulator backends (Systemless and a reference BasiliskII run)
//! claim to execute the same fixture, this module captures a structured
//! event stream from each side so an external comparator can detect the
//! first divergence. Events flow through [`OracleRecorder`], which
//! persists them to JSONL files alongside per-frame indexed-8 pixel
//! dumps for visual diffs.
//!
//! Recording is opt-in (`enable_oracle_recording`) and incurs no cost
//! when disabled. The artifact layout under the chosen output directory:
//!
//! ```text
//!   events.jsonl       — one OracleEvent per line
//!   snapshots.jsonl    — one OracleSnapshot per line (frame metadata)
//!   pixels/<tick>.bin  — raw indexed-8 framebuffer dumps
//! ```
//!
//! [`OracleSource`] tags each artifact so the comparator can tell
//! Systemless-emitted entries from BasiliskII-emitted ones at merge time.

use crate::memory::{MacMemoryBus, MemoryBus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub const ORACLE_EVENTS_FILE: &str = "events.jsonl";
pub const ORACLE_SNAPSHOTS_FILE: &str = "snapshots.jsonl";
pub const ORACLE_PIXELS_DIR: &str = "pixels";
pub const ORACLE_PIXEL_FORMAT_INDEXED_8: &str = "indexed8";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OracleSource {
    Systemless,
    BasiliskIi,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OracleEvent {
    pub source: OracleSource,
    pub tick: u32,
    pub instructions: u64,
    pub pc: u32,
    pub trap_count: u64,
    pub game_trap_count: u64,
    pub screen_event_count: u64,
    pub event: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub source: OracleSource,
    pub capture_id: u64,
    pub screen_event_count: u64,
    pub tick: u32,
    pub instructions: u64,
    pub width: u32,
    pub height: u32,
    pub row_bytes: u32,
    pub pixel_format: String,
    pub palette: Vec<[u16; 3]>,
    pub pixels_file: String,
}

pub struct OracleRecorder {
    source: OracleSource,
    output_dir: PathBuf,
    pixels_dir: PathBuf,
    event_writer: BufWriter<File>,
    snapshot_writer: BufWriter<File>,
    next_capture_id: u64,
}

impl OracleRecorder {
    pub fn create(output_dir: impl AsRef<Path>, source: OracleSource) -> Result<Self, String> {
        let output_dir = output_dir.as_ref().to_path_buf();
        let pixels_dir = output_dir.join(ORACLE_PIXELS_DIR);
        fs::create_dir_all(&pixels_dir)
            .map_err(|err| format!("create oracle pixels dir {}: {}", pixels_dir.display(), err))?;
        let events_path = output_dir.join(ORACLE_EVENTS_FILE);
        let snapshots_path = output_dir.join(ORACLE_SNAPSHOTS_FILE);
        let event_writer =
            BufWriter::new(File::create(&events_path).map_err(|err| {
                format!("create oracle events {}: {}", events_path.display(), err)
            })?);
        let snapshot_writer = BufWriter::new(File::create(&snapshots_path).map_err(|err| {
            format!(
                "create oracle snapshots {}: {}",
                snapshots_path.display(),
                err
            )
        })?);
        Ok(Self {
            source,
            output_dir,
            pixels_dir,
            event_writer,
            snapshot_writer,
            next_capture_id: 0,
        })
    }

    pub fn source(&self) -> OracleSource {
        self.source
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn record_event(&mut self, event: &OracleEvent) -> Result<(), String> {
        serde_json::to_writer(&mut self.event_writer, event)
            .map_err(|err| format!("serialize oracle event: {}", err))?;
        self.event_writer
            .write_all(b"\n")
            .map_err(|err| format!("write oracle event newline: {}", err))?;
        self.event_writer
            .flush()
            .map_err(|err| format!("flush oracle event writer: {}", err))?;
        Ok(())
    }

    pub fn record_snapshot(
        &mut self,
        bus: &MacMemoryBus,
        screen_mode: (u32, u32, u16, u16, u16),
        palette: &[[u16; 3]; 256],
        screen_event_count: u64,
        tick: u32,
        instructions: u64,
    ) -> Result<SnapshotEntry, String> {
        let (screen_base, row_bytes, width, height, depth) = screen_mode;
        if screen_base == 0 {
            return Err("oracle snapshot requires initialized screen base".to_string());
        }
        if width != 800 || height != 600 || depth != 8 {
            return Err(format!(
                "oracle snapshot requires 800x600x8 mode, got {}x{}x{}",
                width, height, depth
            ));
        }
        let capture_id = self.next_capture_id;
        self.next_capture_id = self.next_capture_id.wrapping_add(1);
        let file_name = format!("{:06}.bin", capture_id);
        let relative_pixels_path = Path::new(ORACLE_PIXELS_DIR).join(&file_name);
        let full_pixels_path = self.pixels_dir.join(&file_name);
        let mut pixels = vec![0u8; row_bytes as usize * height as usize];
        for y in 0..height as u32 {
            let row_base = screen_base + y * row_bytes;
            let offset = y as usize * row_bytes as usize;
            for x in 0..row_bytes {
                pixels[offset + x as usize] = bus.read_byte(row_base + x);
            }
        }
        fs::write(&full_pixels_path, pixels).map_err(|err| {
            format!(
                "write oracle pixels {}: {}",
                full_pixels_path.display(),
                err
            )
        })?;
        let entry = SnapshotEntry {
            source: self.source,
            capture_id,
            screen_event_count,
            tick,
            instructions,
            width: width as u32,
            height: height as u32,
            row_bytes,
            pixel_format: ORACLE_PIXEL_FORMAT_INDEXED_8.to_string(),
            palette: palette.to_vec(),
            pixels_file: relative_pixels_path.to_string_lossy().to_string(),
        };
        serde_json::to_writer(&mut self.snapshot_writer, &entry)
            .map_err(|err| format!("serialize oracle snapshot entry: {}", err))?;
        self.snapshot_writer
            .write_all(b"\n")
            .map_err(|err| format!("write oracle snapshot newline: {}", err))?;
        self.snapshot_writer
            .flush()
            .map_err(|err| format!("flush oracle snapshot writer: {}", err))?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::dispatch::TrapDispatcher;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn recorder_captures_current_palette_and_pixels() {
        let temp = tempdir().unwrap();
        let mut dispatcher = TrapDispatcher::new();
        dispatcher
            .enable_oracle_recording(temp.path(), OracleSource::Systemless)
            .unwrap();
        dispatcher.set_screen_mode_for_test(0x2000, 800, 800, 600, 8);
        dispatcher.tick_count = 77;
        dispatcher.instruction_count = 1234;
        dispatcher.trap_count = 9;
        dispatcher.game_trap_count = 5;
        dispatcher.device_clut[7] = [0xFFFF, 0x0000, 0x0000];

        let mut bus = MacMemoryBus::new(0x2000 + 800 * 600 + 1024);
        bus.write_byte(0x2000, 7);

        dispatcher
            .record_oracle_event(
                &bus,
                0x0012_3456,
                "set_entries",
                BTreeMap::from([
                    ("start".to_string(), "0".to_string()),
                    ("count".to_string(), "255".to_string()),
                ]),
                true,
            )
            .unwrap();

        let events: Vec<OracleEvent> = read_jsonl(temp.path().join(ORACLE_EVENTS_FILE));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].screen_event_count, 1);
        assert_eq!(events[0].pc, 0x0012_3456);

        let snapshots: Vec<SnapshotEntry> = read_jsonl(temp.path().join(ORACLE_SNAPSHOTS_FILE));
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].screen_event_count, 1);
        assert_eq!(snapshots[0].palette[7], [0xFFFF, 0x0000, 0x0000]);
        let pixels = std::fs::read(temp.path().join(&snapshots[0].pixels_file)).unwrap();
        assert_eq!(pixels[0], 7);
    }

    #[test]
    fn non_screen_event_keeps_screen_event_counter_stable() {
        let temp = tempdir().unwrap();
        let mut dispatcher = TrapDispatcher::new();
        dispatcher
            .enable_oracle_recording(temp.path(), OracleSource::Systemless)
            .unwrap();
        dispatcher.set_screen_mode_for_test(0x2000, 800, 800, 600, 8);

        let bus = MacMemoryBus::new(0x2000 + 800 * 600 + 1024);
        dispatcher
            .record_oracle_event(
                &bus,
                0x1000,
                "delay",
                BTreeMap::from([("ticks".to_string(), "3".to_string())]),
                false,
            )
            .unwrap();

        let events: Vec<OracleEvent> = read_jsonl(temp.path().join(ORACLE_EVENTS_FILE));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].screen_event_count, 0);
        assert!(!temp
            .path()
            .join(ORACLE_PIXELS_DIR)
            .join("000000.bin")
            .exists());
    }

    fn read_jsonl<T>(path: PathBuf) -> Vec<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        reader
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect()
    }
}
