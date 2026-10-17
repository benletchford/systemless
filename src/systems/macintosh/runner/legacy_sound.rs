//! Compatibility output for programs that synthesize directly into SoundBase.
//!
//! The original Macintosh consumes one high byte per word, 370 words per VBL;
//! low bytes belong to disk-speed control. VIA register B bit 7 disables
//! sound when set; register A bit 3 selects the main (set) or alternate
//! (clear) buffer, and bits 0..2 hold the volume. See Inside Macintosh
//! III-21--III-22 and Guide to the Macintosh Family Hardware, second edition,
//! chapter 13. This is a buffered HLE compatibility device, not
//! cycle-accurate PWM, a complete VIA, or an emulation of the Quadra ASC.
//!
//! Hardware reads the buffer while the VBL task rewrites it, so a driver may
//! write each frame as a ring that starts after the read position and wraps
//! to the start of the buffer. The bus records the order of each frame's
//! stores, and capture plays a single ascending pass from the first word
//! written. An untouched buffer keeps its last layout; anything else plays
//! in buffer order.
//!
//! As a startup-noise policy, an entirely zero first frame after sound is
//! enabled is queued as silence. Some drivers output an unprepared buffer
//! before their first mix. Only that first frame is eligible: zero is valid
//! full-negative-amplitude PCM, and later frames must preserve it. This can
//! suppress an intentional initial rectangular pulse as well.

use std::collections::VecDeque;

use crate::memory::bus::{
    WatchChange, LEGACY_SOUND_ALTERNATE_OFFSET, LEGACY_SOUND_BUFFER_BYTES,
    LEGACY_SOUND_BUFFER_WORDS, LEGACY_SOUND_PORT_A_OFFSET,
};
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::sound::OUTPUT_RATE;

const FRAME: usize = LEGACY_SOUND_BUFFER_WORDS as usize;
// Bound latency and memory when a headless caller advances the guest without
// consuming audio. On overflow retain the most recent eight guest frames.
const MAX_QUEUED_SAMPLES: usize = FRAME * 8;
const OUTPUT_STEP: u64 = (OUTPUT_RATE as u64) << 16;
/// Source samples per second in 16.16 fixed point. The phase advances by
/// this per output sample, and consuming one source sample costs
/// `OUTPUT_STEP`. Every guest tick captures one frame, so this consumes at
/// exactly the rate ticks produce them.
const SOURCE_STEP: u64 = (FRAME as f64 * super::DEFAULT_VBL_HZ * 65536.0) as u64;
/// Captures per latency decision, about a second of guest time. Each capture
/// adds a whole frame and the host mixes on its own schedule, so the queue
/// swings by at least a frame in normal operation. Its lowest point just
/// before a capture shows how much cushion that swing really needs: while
/// the low point stays above one spare frame for a whole window, a stall or
/// underflow has left extra latency, and the next window consumes 1/128
/// faster to drain it.
const TRIM_WINDOW: u32 = 60;

#[derive(Default)]
pub(super) struct LegacySound {
    samples: VecDeque<u8>,
    phase: u64,
    /// Whether a valid frame has been captured since the last observed
    /// disable. Host underflow, page flips and volume changes do not reset it.
    captured_since_enable: bool,
    /// The word each buffer's frames start at: [main, alternate].
    rotation: [u16; 2],
    /// Each buffer's last captured samples, to tell an untouched buffer from
    /// one rewritten by a store the bus could not observe.
    last: [Option<[u8; FRAME]>; 2],
    /// Register A at the last capture: the volume for frames still queued
    /// after the guest disables sound.
    port_a: u8,
    /// Lowest queue length seen before a capture in the current window.
    low_water: Option<usize>,
    window: u32,
    /// Consume slightly faster to shed latency the last window did not need.
    trim: bool,
    #[cfg(test)]
    underflows: usize,
}

impl LegacySound {
    #[cfg(test)]
    fn clear(&mut self) {
        self.samples.clear();
        self.phase = 0;
    }

    /// Stop capturing. Frames already queued are guest time that has passed,
    /// which hardware would already have played, so they still drain.
    fn stop(&mut self, bus: &mut MacMemoryBus) {
        bus.unwatch_legacy_sound();
        self.rotation = [0; 2];
        self.last = [None, None];
        self.low_water = None;
        self.window = 0;
        self.trim = false;
    }

    fn controls(bus: &MacMemoryBus) -> Option<u8> {
        let port_a = bus.legacy_sound_port_a()?;
        let port_b = port_a - LEGACY_SOUND_PORT_A_OFFSET;
        (bus.read_byte(port_b) & 0x80 == 0).then(|| bus.read_byte(port_a))
    }

    /// Capture the completed guest frame before its next VBL callback can
    /// overwrite it. Host audio chunks must not repeatedly read the last RAM
    /// contents or collapse several guest frames into one repeated buffer.
    pub(super) fn capture_frame(&mut self, bus: &mut MacMemoryBus) {
        let Some(port_a) = Self::controls(bus) else {
            // Missing controls are not an observed disable. If the device
            // is addressable, controls() returned None because PB7 is set.
            if bus.legacy_sound_port_a().is_some() {
                self.captured_since_enable = false;
            }
            self.stop(bus);
            return;
        };
        self.port_a = port_a;
        let main = bus.read_long(addr::SOUND_BASE);
        let slot = usize::from(port_a & 0x08 == 0);
        let selected = if slot == 0 {
            Some(main)
        } else {
            main.checked_sub(LEGACY_SOUND_ALTERNATE_OFFSET)
        };
        let Some(base) = selected.filter(|&base| {
            base >= 0x1000
                && base
                    .checked_add(LEGACY_SOUND_BUFFER_BYTES)
                    .is_some_and(|end| end <= bus.ram_size())
                && bus.is_guest_address_mapped(base, LEGACY_SOUND_BUFFER_BYTES as usize)
        }) else {
            self.stop(bus);
            return;
        };

        let mut frame = [0u8; FRAME];
        for (word, sample) in frame.iter_mut().enumerate() {
            *sample = bus.read_byte(base + 2 * word as u32);
        }
        match bus.watch_legacy_sound() {
            // This frame's stores predate the watch, or cannot be observed.
            WatchChange::Armed | WatchChange::Refused => {
                self.rotation = [0; 2];
                self.last = [None, None];
            }
            WatchChange::Unchanged => {
                let order = bus.take_legacy_sound_order(slot == 1).unwrap_or_default();
                if usize::from(order.count) == FRAME && order.ascending {
                    self.rotation[slot] = order.first;
                } else if !(order.count == 0 && self.last[slot] == Some(frame)) {
                    // Partial, unordered or unobserved rewrites: buffer order.
                    self.rotation[slot] = 0;
                }
            }
        }
        self.last[slot] = Some(frame);

        if !self.captured_since_enable {
            self.captured_since_enable = true;
            if frame.iter().all(|&sample| sample == 0) {
                // Preserve guest RAM and the frame's duration. Keep `last`
                // above as the actual RAM image used by store-order tracking.
                frame.fill(128);
            }
        }

        let level = self.samples.len();
        self.low_water = Some(self.low_water.map_or(level, |low| low.min(level)));
        self.window += 1;
        if self.window == TRIM_WINDOW {
            self.trim = self.low_water.is_some_and(|low| low > FRAME);
            self.low_water = None;
            self.window = 0;
        }
        let excess = (self.samples.len() + FRAME).saturating_sub(MAX_QUEUED_SAMPLES);
        if excess != 0 {
            self.samples.drain(..excess);
            self.phase = 0;
        }
        let start = usize::from(self.rotation[slot]);
        self.samples.extend(&frame[start..]);
        self.samples.extend(&frame[..start]);
    }

    /// Sample-hold resampling from the legacy ~22.25 kHz clock to the common
    /// 22050 Hz mixer. Preserve fractional phase across host chunks.
    /// Missing guest frames produce silence instead of looping stale audio.
    pub(super) fn mix_stereo(
        &mut self,
        bus: &MacMemoryBus,
        output: &mut Vec<u8>,
        frames: usize,
        muted: bool,
    ) {
        if self.samples.is_empty() {
            return;
        }
        let port_a = Self::controls(bus).unwrap_or(self.port_a);
        output.resize(frames * 2, 128);
        for frame in output.chunks_exact_mut(2) {
            let advance = (self.phase / OUTPUT_STEP) as usize;
            let consumed = advance.min(self.samples.len());
            self.samples.drain(..consumed);
            self.phase -= consumed as u64 * OUTPUT_STEP;
            let Some(sample) = self.samples.front().copied() else {
                #[cfg(test)]
                {
                    self.underflows += 1;
                }
                break;
            };
            // Approximate the analog gain ladder monotonically; even PA=0
            // is nonzero. SetSoundVol's explicit mute is shared API state.
            // Apply current gain once to this stream, leaving Sound Manager
            // and movie channels alone and allowing immediate volume changes.
            let gain = if muted { 0 } else { i16::from(port_a & 7) + 1 };
            let sample = (i16::from(sample) - 128) * gain / 8;
            for channel in frame {
                *channel = (i16::from(*channel) + sample).clamp(0, 255) as u8;
            }
            self.phase += if self.trim {
                SOURCE_STEP + SOURCE_STEP / 128
            } else {
                SOURCE_STEP
            };
        }
    }

    #[cfg(test)]
    pub(super) fn queued(&self) -> Vec<u8> {
        self.samples.iter().copied().collect()
    }

    #[cfg(test)]
    pub(super) fn rotation(&self) -> [u16; 2] {
        self.rotation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> (MacMemoryBus, LegacySound) {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let registers = bus.read_long(addr::VIA_BASE);
        bus.write_byte(registers, 0); // enabled
        bus.write_byte(registers + LEGACY_SOUND_PORT_A_OFFSET, 0x0f); // main, full volume
        (bus, LegacySound::default())
    }

    fn buffer(bus: &MacMemoryBus, alternate: bool) -> u32 {
        let main = bus.read_long(addr::SOUND_BASE);
        if alternate {
            main - LEGACY_SOUND_ALTERNATE_OFFSET
        } else {
            main
        }
    }

    fn fill(bus: &mut MacMemoryBus, alternate: bool, value: u8) {
        let base = buffer(bus, alternate);
        for offset in (0..LEGACY_SOUND_BUFFER_BYTES).step_by(2) {
            bus.write_byte(base + offset, value);
            bus.write_byte(base + offset + 1, 255); // must not become audio
        }
    }

    /// Write stream samples `first..first + 370` as a ring starting at word
    /// `start`, the way a driver fills ahead of the hardware read position.
    fn write_ring(bus: &mut MacMemoryBus, alternate: bool, start: usize, first: usize) {
        let base = buffer(bus, alternate);
        for i in 0..FRAME {
            let word = ((start + i) % FRAME) as u32;
            bus.write_byte(base + 2 * word, ((first + i) % 256) as u8);
        }
    }

    fn assert_ramp(samples: &[u8], first: usize) {
        for (i, &sample) in samples.iter().enumerate() {
            assert_eq!(sample, ((first + i) % 256) as u8, "sample {i}");
        }
    }

    #[test]
    fn disabled_device_does_not_emit_buffer_contents() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut sound = LegacySound::default();
        fill(&mut bus, false, 255);
        sound.capture_frame(&mut bus);
        let mut pcm = Vec::new();
        sound.mix_stereo(&bus, &mut pcm, 100, false);
        assert!(pcm.is_empty());
    }

    #[test]
    fn zero_first_frame_is_silenced_without_changing_guest_memory_or_duration() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.queued(), vec![128; FRAME]);
        let base = buffer(&bus, false);
        for word in 0..FRAME as u32 {
            assert_eq!(bus.read_word(base + 2 * word), 0x00ff);
        }

        // An untouched second frame is real full-negative-amplitude PCM.
        sound.capture_frame(&mut bus);
        assert_eq!(&sound.queued()[FRAME..], vec![0; FRAME]);
        assert_eq!(sound.samples.len(), 2 * FRAME);
    }

    #[test]
    fn nonzero_first_frame_preserves_later_zero_frames() {
        for first in [1, 127, 128, 129, 255] {
            let (mut bus, mut sound) = device();
            fill(&mut bus, false, first);
            sound.capture_frame(&mut bus);
            fill(&mut bus, false, 0);
            sound.capture_frame(&mut bus);
            assert_eq!(&sound.queued()[..FRAME], vec![first; FRAME]);
            assert_eq!(&sound.queued()[FRAME..], vec![0; FRAME]);
        }
    }

    #[test]
    fn a_partial_zero_first_frame_keeps_every_sample() {
        for word in [0, FRAME / 2, FRAME - 1] {
            let (mut bus, mut sound) = device();
            fill(&mut bus, false, 0);
            bus.write_byte(buffer(&bus, false) + 2 * word as u32, 1);
            sound.capture_frame(&mut bus);
            let mut expected = vec![0; FRAME];
            expected[word] = 1;
            assert_eq!(sound.queued(), expected);
        }
    }

    #[test]
    fn zero_startup_policy_rearms_on_disable_without_dropping_queued_audio() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        sound.capture_frame(&mut bus);
        let registers = bus.read_long(addr::VIA_BASE);
        bus.write_byte(registers, 0x80);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.samples.len(), 2 * FRAME);
        bus.write_byte(registers, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.queued(), [vec![128; FRAME], vec![0; FRAME], vec![128; FRAME]].concat());
    }

    #[test]
    fn startup_policy_checks_selected_page_and_does_not_rearm_on_page_or_volume_changes() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 255);
        fill(&mut bus, true, 0);
        let port_a = bus.read_long(addr::VIA_BASE) + LEGACY_SOUND_PORT_A_OFFSET;
        bus.write_byte(port_a, 7); // alternate page, full volume
        sound.capture_frame(&mut bus);
        assert_eq!(sound.queued(), vec![128; FRAME]);
        fill(&mut bus, false, 0);
        bus.write_byte(port_a, 8); // main page, minimum volume
        sound.capture_frame(&mut bus);
        assert_eq!(&sound.queued()[FRAME..], vec![0; FRAME]);
    }

    #[test]
    fn silenced_startup_frame_keeps_resampling_phase_across_host_chunks() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        write_ring(&mut bus, false, 0, 0);
        sound.capture_frame(&mut bus);
        let mut actual = Vec::new();
        for count in [1, 31, 367, 2, 300] {
            let mut chunk = Vec::new();
            sound.mix_stereo(&bus, &mut chunk, count, false);
            actual.extend(chunk);
        }
        assert_eq!(actual.len(), 701 * 2);
        for (i, frame) in actual.chunks_exact(2).enumerate() {
            let source = (i as u64 * SOURCE_STEP / OUTPUT_STEP) as usize;
            let expected = if source < FRAME { 128 } else { ((source - FRAME) % 256) as u8 };
            assert_eq!(frame, [expected; 2], "output frame {i}");
        }
    }

    #[test]
    fn host_underflow_and_invalid_pointer_do_not_rearm_startup_policy() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus); // initialized silence consumes the first frame
        let mut output = Vec::new();
        sound.mix_stereo(&bus, &mut output, 500, false);
        assert!(sound.samples.is_empty());
        let main = bus.read_long(addr::SOUND_BASE);
        bus.write_long(addr::SOUND_BASE, 0);
        sound.capture_frame(&mut bus);
        bus.write_long(addr::SOUND_BASE, main);
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.queued(), vec![0; FRAME]);
        output.clear();
        sound.mix_stereo(&bus, &mut output, 100, false);
        assert_eq!(output, vec![0; 200]);
    }

    #[test]
    fn unavailable_controls_do_not_rearm_startup_policy() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        let via = bus.read_long(addr::VIA_BASE);
        bus.write_long(addr::VIA_BASE, 0);
        sound.capture_frame(&mut bus);
        bus.write_long(addr::VIA_BASE, via);
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(&sound.queued()[FRAME..], vec![0; FRAME]);
    }

    #[test]
    fn invalid_initial_sound_base_does_not_consume_startup_check() {
        let (mut bus, mut sound) = device();
        let main = bus.read_long(addr::SOUND_BASE);
        bus.write_long(addr::SOUND_BASE, 0);
        sound.capture_frame(&mut bus);
        assert!(sound.samples.is_empty());
        bus.write_long(addr::SOUND_BASE, main);
        fill(&mut bus, false, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.queued(), vec![128; FRAME]);
    }

    #[test]
    fn successive_guest_frames_survive_host_chunking_and_resampling() {
        let (mut bus, mut sound) = device();
        for i in 0..2 {
            let main = bus.read_long(addr::SOUND_BASE);
            for j in 0..370 {
                bus.write_word(main + j * 2, (((i * 37 + j) % 256) << 8 | 255) as u16);
            }
            sound.capture_frame(&mut bus);
        }
        let mut actual = Vec::new();
        for n in [1, 31, 367, 2, 300] {
            let mut chunk = Vec::new();
            sound.mix_stereo(&bus, &mut chunk, n, false);
            actual.extend(chunk);
        }
        assert_eq!(actual.len(), 701 * 2);
        for (i, frame) in actual.chunks_exact(2).enumerate() {
            let source = (i as u64 * SOURCE_STEP / OUTPUT_STEP) as usize;
            let value = ((source / 370 * 37 + source % 370) % 256) as u8;
            assert_eq!(frame, [value, value]);
        }
    }

    #[test]
    fn interleaved_capture_and_mix_preserve_fractional_position() {
        let (mut bus, mut sound) = device();
        let mut output_count = 0;
        for frame in 0..40u64 {
            let main = bus.read_long(addr::SOUND_BASE);
            for j in 0..370u64 {
                bus.write_byte(main + j as u32 * 2, ((frame * 370 + j) % 256) as u8);
            }
            sound.capture_frame(&mut bus);
            let total = ((frame + 1) * 370 * OUTPUT_STEP).div_ceil(SOURCE_STEP);
            let mut output = Vec::new();
            sound.mix_stereo(&bus, &mut output, (total - output_count) as usize, false);
            for (i, pair) in output.chunks_exact(2).enumerate() {
                let source = (output_count + i as u64) * SOURCE_STEP / OUTPUT_STEP;
                assert_eq!(pair, [(source % 256) as u8; 2], "guest frame {frame}");
            }
            output_count = total;
        }
    }

    #[test]
    fn source_rate_matches_one_frame_per_guest_tick() {
        // Each output sample advances the source by SOURCE_STEP / OUTPUT_STEP.
        let per_second = SOURCE_STEP as f64 / 65536.0;
        let produced = FRAME as f64 * super::super::DEFAULT_VBL_HZ;
        assert!(
            (per_second - produced).abs() < 0.5,
            "{per_second} vs {produced}"
        );
    }

    #[test]
    fn ring_writer_frames_play_from_the_first_written_word() {
        let (mut bus, mut sound) = device();
        // The arming capture cannot know this frame's store order.
        write_ring(&mut bus, false, 32, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation, [0, 0]);
        let onset = sound.queued().len();
        for frame in 1..6 {
            write_ring(&mut bus, false, 32, frame * FRAME);
            sound.capture_frame(&mut bus);
            assert_eq!(sound.rotation, [32, 0]);
        }
        assert_ramp(&sound.queued()[onset..], FRAME);
    }

    #[test]
    fn ring_start_follows_the_writer_frame_by_frame() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        let onset = sound.queued().len();
        for (frame, start) in [32, 40, 40, 32, 0, 32].into_iter().enumerate() {
            write_ring(&mut bus, false, start, (frame + 1) * FRAME);
            sound.capture_frame(&mut bus);
            assert_eq!(usize::from(sound.rotation[0]), start);
        }
        assert_ramp(&sound.queued()[onset..], FRAME);
    }

    #[test]
    fn word_long_and_repeated_stores_are_noted_once() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        let onset = sound.queued().len();
        let base = buffer(&bus, false);
        // Word stores, ring from 40.
        for i in 0..FRAME {
            let word = ((40 + i) % FRAME) as u32;
            bus.write_word(base + 2 * word, ((FRAME + i) % 256) as u16 * 0x101);
        }
        sound.capture_frame(&mut bus);
        // Long stores cover two words each, ring from 32. Then rewrite the
        // last word with a byte store of the same value, which is a repeat
        // within an incomplete pass and must not reorder it.
        for pair in 0..FRAME / 2 {
            let i = 2 * pair;
            let word = ((32 + i) % FRAME) as u32;
            let hi = ((2 * FRAME + i) % 256) as u32;
            let lo = ((2 * FRAME + i + 1) % 256) as u32;
            bus.write_long(base + 2 * word, hi << 24 | lo << 8);
        }
        bus.write_byte(base + 2 * 31, ((2 * FRAME + 369) % 256) as u8);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);
        assert_ramp(&sound.queued()[onset..], FRAME);
    }

    #[test]
    fn bulk_stores_are_noted() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        let base = buffer(&bus, false);
        let staging = 0x0010_0000;
        type Store = fn(&mut MacMemoryBus, u32, &[u8], u32);
        let stores: [(&str, Store); 5] = [
            ("write_bytes", |bus, dst, data, _| {
                bus.write_bytes(dst, data)
            }),
            ("block_move", |bus, dst, data, staging| {
                bus.write_bytes(staging, data);
                bus.block_move(staging, dst, data.len() as u32);
            }),
            ("copy_ram_bytes", |bus, dst, data, staging| {
                bus.write_bytes(staging, data);
                assert!(bus.copy_ram_bytes(staging, dst, data.len() as u32));
            }),
            ("copy_mapped_ram_bytes", |bus, dst, data, staging| {
                let identity: [u8; 256] = std::array::from_fn(|i| i as u8);
                bus.write_bytes(staging, data);
                assert!(bus.copy_mapped_ram_bytes(staging, dst, data.len() as u32, &identity));
            }),
            ("presented copy", |bus, dst, data, _| {
                bus.with_presentation_and_ram(|_, store| store(dst, data));
            }),
        ];
        // Each path writes different samples as a ring with its own start, so
        // a path that is not noted cannot pass on an earlier path's record.
        for (path, (name, store)) in stores.into_iter().enumerate() {
            let start = 32 + 8 * path;
            let mut image = vec![0u8; LEGACY_SOUND_BUFFER_BYTES as usize];
            for i in 0..FRAME {
                image[2 * ((start + i) % FRAME)] = ((path * FRAME + i) % 256) as u8;
            }
            let split = 2 * start;
            sound.clear();
            store(&mut bus, base + split as u32, &image[split..], staging);
            store(&mut bus, base, &image[..split], staging);
            sound.capture_frame(&mut bus);
            assert_eq!(usize::from(sound.rotation[0]), start, "{name}");
            assert_ramp(&sound.queued(), path * FRAME);
        }
        // Fills store one value per call; their order is still a ring's.
        bus.fill_bytes_strided(base + 2 * 72, 2, FRAME as u32 - 72, 7);
        bus.fill_bytes_strided(base, 2, 72, 9);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 72);
        bus.fill_bytes(base + 2 * 80, LEGACY_SOUND_BUFFER_BYTES - 2 * 80, 5);
        bus.fill_zeros(base, 2 * 80);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 80);
    }

    #[test]
    fn page_flipping_driver_plays_each_page_from_its_ring_start() {
        let (mut bus, mut sound) = device();
        let port_a = bus.read_long(addr::VIA_BASE) + LEGACY_SOUND_PORT_A_OFFSET;
        sound.capture_frame(&mut bus);
        sound.clear();
        // Each tick, fill the page that is not playing, then show it.
        for frame in 0..6 {
            let alternate = frame % 2 == 0;
            let start = if alternate { 40 } else { 32 };
            write_ring(&mut bus, alternate, start, frame * FRAME);
            bus.write_byte(port_a, if alternate { 0x07 } else { 0x0F });
            sound.capture_frame(&mut bus);
        }
        assert_eq!(sound.rotation, [32, 40]);
        assert_ramp(&sound.queued(), 0);

        // A page rewritten before it is shown reports its newest pass.
        write_ring(&mut bus, true, 40, 0);
        bus.write_byte(port_a, 0x0F);
        sound.capture_frame(&mut bus); // the untouched main page replays
        assert_eq!(sound.rotation[0], 32);
        write_ring(&mut bus, true, 48, 0);
        bus.write_byte(port_a, 0x07);
        sound.clear();
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[1], 48);
        assert_ramp(&sound.queued(), 0);

        // A stray store after a complete pass keeps that pass's layout.
        write_ring(&mut bus, true, 56, 0);
        bus.write_byte(buffer(&bus, true) + 2 * 100, 44);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[1], 56);
    }

    #[test]
    fn a_clear_followed_by_a_ring_plays_from_the_ring_start() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        fill(&mut bus, false, 128);
        write_ring(&mut bus, false, 32, 0);
        sound.clear();
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);
        assert_ramp(&sound.queued(), 0);
    }

    #[test]
    fn guest_moved_sound_base_plays_in_buffer_order() {
        let (mut bus, mut sound) = device();
        write_ring(&mut bus, false, 32, 0);
        sound.capture_frame(&mut bus);
        write_ring(&mut bus, false, 32, FRAME);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);
        let moved = 0x0002_0000;
        for word in 0..FRAME as u32 {
            bus.write_byte(moved + 2 * word, word as u8);
        }
        bus.write_long(addr::SOUND_BASE, moved);
        sound.clear();
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation, [0, 0]);
        assert_eq!(bus.take_legacy_sound_order(false), None, "not watched");
        assert!(bus.fast_mem_window().is_some());
        assert_ramp(&sound.queued(), 0);
    }

    #[test]
    fn inactive_buffer_ring_is_honoured_when_selected() {
        let (mut bus, mut sound) = device();
        sound.capture_frame(&mut bus);
        write_ring(&mut bus, true, 40, 0);
        let port_a = bus.read_long(addr::VIA_BASE) + LEGACY_SOUND_PORT_A_OFFSET;
        bus.write_byte(port_a, 0x07); // alternate buffer, full volume
        sound.clear();
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[1], 40);
        assert_ramp(&sound.queued(), 0);
    }

    #[test]
    fn unordered_partial_and_unobserved_rewrites_play_in_buffer_order() {
        let (mut bus, mut sound) = device();
        write_ring(&mut bus, false, 32, 0);
        sound.capture_frame(&mut bus);
        write_ring(&mut bus, false, 32, FRAME);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);
        let base = buffer(&bus, false);

        // An untouched buffer replays with the layout it last had.
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);

        // Descending stores are not one ascending pass.
        for word in (0..FRAME as u32).rev() {
            bus.write_byte(base + 2 * word, word as u8);
        }
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 0);

        // A partial pass plays in buffer order.
        write_ring(&mut bus, false, 32, 0);
        sound.capture_frame(&mut bus);
        for word in 32..300 {
            bus.write_byte(base + 2 * word, 1);
        }
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 0);

        // A rewrite the bus could not observe also plays in buffer order.
        write_ring(&mut bus, false, 32, 0);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 32);
        bus.write_readonly_code_word(base, 0x5500);
        sound.capture_frame(&mut bus);
        assert_eq!(sound.rotation[0], 0);
    }

    #[test]
    fn high_ram_device_remains_addressable_in_24_bit_mode() {
        let mut bus = MacMemoryBus::new(32 * 1024 * 1024);
        let mut sound = LegacySound::default();
        let main = bus.read_long(addr::SOUND_BASE);
        let via = bus.read_long(addr::VIA_BASE);
        bus.set_addressing_32_bit(false);
        bus.write_byte(via, 0);
        bus.write_byte(via + LEGACY_SOUND_PORT_A_OFFSET, 15);
        fill(&mut bus, false, 170);
        sound.capture_frame(&mut bus);
        let mut output = Vec::new();
        sound.mix_stereo(&bus, &mut output, 100, false);
        assert_eq!(output, vec![170; 200]);
        bus.set_addressing_32_bit(true);
        assert_eq!(bus.read_byte(main), 170);
        assert_eq!(bus.read_byte(main & 0x00ff_ffff), 0);
    }

    #[test]
    fn alternate_buffer_volume_stereo_saturation_and_underflow() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 0);
        fill(&mut bus, true, 198);
        let registers = bus.read_long(addr::VIA_BASE);
        bus.write_byte(registers + LEGACY_SOUND_PORT_A_OFFSET, 3); // alternate, volume 3/7
        sound.capture_frame(&mut bus);
        let mut output = vec![240, 100];
        sound.mix_stereo(&bus, &mut output, 1, false);
        assert_eq!(output, [255, 135]);
        let mut rest = Vec::new();
        sound.mix_stereo(&bus, &mut rest, 500, false);
        assert_eq!(&rest[..2], [163, 163]);
        assert_eq!(&rest[800..], [128; 200]);
        let mut empty = Vec::new();
        sound.mix_stereo(&bus, &mut empty, 500, false);
        assert!(
            empty.is_empty(),
            "never replay the same guest buffer on host underflow"
        );
        let alternate = bus.read_long(addr::SOUND_BASE) - LEGACY_SOUND_ALTERNATE_OFFSET;
        assert_eq!(bus.read_byte(alternate + 1), 255, "preserve disk bytes");
    }

    #[test]
    fn disable_stops_capture_and_drains_frames_already_queued() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 255);
        sound.capture_frame(&mut bus);
        assert!(bus.fast_mem_window().is_none(), "armed");
        let registers = bus.read_long(addr::VIA_BASE);
        bus.write_byte(registers, 0x80);
        sound.capture_frame(&mut bus);
        assert!(bus.fast_mem_window().is_some(), "disarmed");
        assert_eq!(bus.take_legacy_sound_order(false), None);
        assert_eq!(sound.samples.len(), FRAME, "nothing new, nothing dropped");
        // The queued frame is guest time that has passed; it still plays at
        // the volume it was captured with.
        let mut output = vec![140; 4];
        sound.mix_stereo(&bus, &mut output, 2, false);
        assert_eq!(output, [255; 4]);
        let mut rest = Vec::new();
        sound.mix_stereo(&bus, &mut rest, 400, false);
        assert!(rest[..700].iter().all(|&sample| sample == 255));
        assert!(rest[740..].iter().all(|&sample| sample == 128));
        assert!(sound.samples.is_empty());
    }

    #[test]
    fn malformed_pointers_capture_nothing() {
        let (mut bus, mut sound) = device();
        for pointer in [0, 0x800, u32::MAX, bus.ram_size() - 2] {
            bus.write_long(addr::SOUND_BASE, pointer);
            sound.capture_frame(&mut bus);
            assert!(sound.samples.is_empty());
        }
        bus.write_long(addr::VIA_BASE, 0);
        assert!(LegacySound::controls(&bus).is_none());
    }

    #[test]
    fn unconsumed_guest_audio_is_bounded() {
        let (mut bus, mut sound) = device();
        for value in 0..20 {
            fill(&mut bus, false, value);
            sound.capture_frame(&mut bus);
        }
        assert_eq!(sound.samples.len(), MAX_QUEUED_SAMPLES);
        assert_eq!(sound.samples.front(), Some(&12)); // oldest retained frame = 12
        assert_eq!(sound.samples.back(), Some(&19)); // latest frame = 19
    }

    #[test]
    fn latency_left_by_a_stall_drains_to_one_spare_frame() {
        let (mut bus, mut sound) = device();
        // A host stall: six frames arrive before any audio is mixed.
        for value in 0..6 {
            fill(&mut bus, false, value);
            sound.capture_frame(&mut bus);
        }
        // 366 output samples per tick consume slightly less than a frame, so
        // without trimming the backlog would grow rather than drain.
        let mut levels = Vec::new();
        for _ in 0..3000 {
            fill(&mut bus, false, 1);
            sound.capture_frame(&mut bus);
            let mut output = Vec::new();
            sound.mix_stereo(&bus, &mut output, 366, false);
            levels.push(sound.samples.len());
        }
        let settled = &levels[2400..];
        // Trimming stops once a window's low point is within a frame, after
        // up to a window of trimming below it.
        assert!(
            settled
                .iter()
                .all(|&level| (FRAME / 2..=2 * FRAME).contains(&level)),
            "{settled:?}"
        );
        assert_eq!(sound.underflows, 0);
    }

    /// The web frontend mixes on display frames (60 Hz) with timing jitter,
    /// while guest ticks capture at 60.15 Hz at varying points within those
    /// frames. After warming up, the queue must not run dry.
    #[test]
    fn jittered_host_schedule_settles_without_underflow() {
        let (mut bus, mut sound) = device();
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut random = move |range: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % range
        };
        let (mut elapsed, mut ticks, mut carry) = (0.0f64, 0u64, 0.0f64);
        for host_frame in 0..3600 {
            // About +-1 ms of jitter around a 60 Hz frame.
            let wanted = f64::from(OUTPUT_RATE) / 60.0 + random(45) as f64 - 22.0 + carry;
            let samples = wanted.floor() as usize;
            carry = wanted - samples as f64;
            elapsed += samples as f64 / f64::from(OUTPUT_RATE);
            let due = (elapsed * super::super::DEFAULT_VBL_HZ) as u64;
            let split = random(samples as u64 + 1) as usize;
            let mut output = Vec::new();
            sound.mix_stereo(&bus, &mut output, split, false);
            while ticks < due {
                fill(&mut bus, false, (ticks % 200) as u8 + 20);
                sound.capture_frame(&mut bus);
                ticks += 1;
            }
            output.clear();
            sound.mix_stereo(&bus, &mut output, samples - split, false);
            if host_frame == 1200 {
                sound.underflows = 0;
            }
        }
        assert_eq!(sound.underflows, 0);
        assert!(sound.samples.len() <= 4 * FRAME, "{}", sound.samples.len());
    }

    #[test]
    fn device_is_absent_where_a_24_bit_client_could_not_reach_it() {
        let mut bus = MacMemoryBus::new(40 * 1024 * 1024);
        assert_eq!(bus.legacy_sound_register_base(), None);
        assert_eq!(bus.read_long(addr::VIA_BASE), 0);
        assert!(LegacySound::controls(&bus).is_none());
        let mut sound = LegacySound::default();
        bus.set_addressing_32_bit(false);
        sound.capture_frame(&mut bus);
        assert!(sound.samples.is_empty());
    }

    #[test]
    fn volume_changes_affect_already_queued_audio_and_api_zero_mutes() {
        let (mut bus, mut sound) = device();
        fill(&mut bus, false, 192);
        sound.capture_frame(&mut bus);
        let mut output = Vec::new();
        sound.mix_stereo(&bus, &mut output, 1, false);
        assert_eq!(output, [192; 2]);
        let port_a = bus.read_long(addr::VIA_BASE) + LEGACY_SOUND_PORT_A_OFFSET;
        bus.write_byte(port_a, 8); // main, minimum hardware gain, not mute
        output.clear();
        sound.mix_stereo(&bus, &mut output, 1, false);
        assert_eq!(output, [136; 2]);
        output.clear();
        sound.mix_stereo(&bus, &mut output, 1, true);
        assert_eq!(output, [128; 2]);
    }
}
