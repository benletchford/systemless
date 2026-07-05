//! Runtime trace hook for cross-runtime parity comparison.
//!
//! When two emulator backends claim to execute the same fixture, the
//! runtime emits a structured event stream so an external comparator can
//! detect the first divergence. This module defines only the *hook*: the
//! event/snapshot data types and the [`TraceSink`] trait the dispatcher
//! emits to. The concrete sink — which persists events, snapshots, and
//! framebuffer dumps — lives outside this crate and is installed via
//! `Runner::set_trace_sink`.
//!
//! Recording is opt-in (no sink installed = no cost) and the runtime is
//! agnostic to how a sink stores what it receives.

use crate::memory::MacMemoryBus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which backend produced a trace artifact, so a comparator can tell the
/// two event streams apart at merge time.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceSource {
    Systemless,
    BasiliskIi,
}

/// One structured runtime event, tagged with the counters needed to align
/// it against the other backend's stream.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TraceEvent {
    pub source: TraceSource,
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

/// Metadata for one captured framebuffer snapshot; the raw pixels are
/// stored separately by the sink and referenced by `pixels_file`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub source: TraceSource,
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

/// Destination for runtime trace output. The dispatcher emits events and
/// screen snapshots to whichever sink the host installs; the sink decides
/// how (and whether) to persist them. Errors are surfaced as `String` and
/// wrapped by the runtime as [`crate::error::Error::Trace`].
pub trait TraceSink {
    /// The backend this sink is recording for.
    fn source(&self) -> TraceSource;

    /// Persist one structured event.
    fn record_event(&mut self, event: &TraceEvent) -> Result<(), String>;

    /// Capture the current framebuffer as a snapshot. `screen_mode` is
    /// `(base, row_bytes, width, height, depth)`; `palette` is the active
    /// 256-entry CLUT.
    fn record_snapshot(
        &mut self,
        bus: &MacMemoryBus,
        screen_mode: (u32, u32, u16, u16, u16),
        palette: &[[u16; 3]; 256],
        screen_event_count: u64,
        tick: u32,
        instructions: u64,
    ) -> Result<SnapshotEntry, String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;
    use crate::trap::dispatch::TrapDispatcher;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Captured {
        events: Vec<TraceEvent>,
        snapshots: Vec<SnapshotEntry>,
    }

    /// In-memory sink so the dispatcher's emission logic (counter bumps,
    /// snapshot triggering, palette hand-off) can be asserted without the
    /// on-disk recorder, which lives in the host crate.
    struct MemSink(Rc<RefCell<Captured>>);

    impl TraceSink for MemSink {
        fn source(&self) -> TraceSource {
            TraceSource::Systemless
        }
        fn record_event(&mut self, event: &TraceEvent) -> Result<(), String> {
            self.0.borrow_mut().events.push(event.clone());
            Ok(())
        }
        fn record_snapshot(
            &mut self,
            _bus: &MacMemoryBus,
            screen_mode: (u32, u32, u16, u16, u16),
            palette: &[[u16; 3]; 256],
            screen_event_count: u64,
            tick: u32,
            instructions: u64,
        ) -> Result<SnapshotEntry, String> {
            let mut cap = self.0.borrow_mut();
            let entry = SnapshotEntry {
                source: TraceSource::Systemless,
                capture_id: cap.snapshots.len() as u64,
                screen_event_count,
                tick,
                instructions,
                width: screen_mode.2 as u32,
                height: screen_mode.3 as u32,
                row_bytes: screen_mode.1,
                pixel_format: "indexed8".to_string(),
                palette: palette.to_vec(),
                pixels_file: format!("{:06}.bin", cap.snapshots.len()),
            };
            cap.snapshots.push(entry.clone());
            Ok(entry)
        }
    }

    #[test]
    fn screen_event_bumps_counter_and_captures_snapshot() {
        let cap = Rc::new(RefCell::new(Captured::default()));
        let mut dispatcher = TrapDispatcher::new();
        dispatcher.set_trace_sink(Box::new(MemSink(cap.clone())));
        dispatcher.set_screen_mode_for_test(0x2000, 800, 800, 600, 8);
        dispatcher.tick_count = 77;
        dispatcher.instruction_count = 1234;
        dispatcher.device_clut[7] = [0xFFFF, 0x0000, 0x0000];

        let mut bus = MacMemoryBus::new(0x2000 + 800 * 600 + 1024);
        bus.write_byte(0x2000, 7);

        dispatcher
            .record_trace_event(
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

        let cap = cap.borrow();
        assert_eq!(cap.events.len(), 1);
        assert_eq!(cap.events[0].screen_event_count, 1);
        assert_eq!(cap.events[0].pc, 0x0012_3456);
        assert_eq!(cap.snapshots.len(), 1);
        assert_eq!(cap.snapshots[0].screen_event_count, 1);
        assert_eq!(cap.snapshots[0].palette[7], [0xFFFF, 0x0000, 0x0000]);
    }

    #[test]
    fn non_screen_event_keeps_counter_stable_and_skips_snapshot() {
        let cap = Rc::new(RefCell::new(Captured::default()));
        let mut dispatcher = TrapDispatcher::new();
        dispatcher.set_trace_sink(Box::new(MemSink(cap.clone())));
        dispatcher.set_screen_mode_for_test(0x2000, 800, 800, 600, 8);

        let bus = MacMemoryBus::new(0x2000 + 800 * 600 + 1024);
        dispatcher
            .record_trace_event(
                &bus,
                0x1000,
                "delay",
                BTreeMap::from([("ticks".to_string(), "3".to_string())]),
                false,
            )
            .unwrap();

        let cap = cap.borrow();
        assert_eq!(cap.events.len(), 1);
        assert_eq!(cap.events[0].screen_event_count, 0);
        assert!(cap.snapshots.is_empty());
    }
}
