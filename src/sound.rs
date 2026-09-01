//! Sound Manager state and mixing engine.
//!
//! Holds per-channel playback state and produces mixed PCM output each frame.
//! Reference: *Inside Macintosh: Sound* (1994).

/// Output sample rate in Hz.
pub const OUTPUT_RATE: u32 = 22050;
/// Classic Sound Manager `rate22khz` fixed-point value (22254.54545 Hz).
pub const RATE_22KHZ_FIXED: u32 = 0x56EE_8BA3;
/// Classic Sound Manager `rate11khz` fixed-point value (11127.27273 Hz).
pub const RATE_11KHZ_FIXED: u32 = 0x2B77_45D1;

/// Standard command queue depth (Sound 1994, 2-93).
const STD_Q_LENGTH: usize = 128;
/// Full volume for one speaker in volumeCmd units (Sound 1994, 2-96).
const FULL_VOLUME: u16 = 0x0100;
/// Packed full-volume stereo value: high word = right, low word = left.
const FULL_STEREO_VOLUME: u32 = ((FULL_VOLUME as u32) << 16) | FULL_VOLUME as u32;
/// Unity playback rate in Sound Manager Fixed units (Sound 1994, 2-97).
const UNITY_RATE_FIXED: u32 = 0x0001_0000;
/// Maximum decoded double-buffer frames retained for waveform diagnostics.
pub(crate) const DEBUG_DOUBLE_BUFFER_CAPTURE_LIMIT: usize = OUTPUT_RATE as usize * 60;

/// Sound command constants (Sound 1994, 2-92 to 2-97).
pub mod cmd {
    pub const NULL: u16 = 0;
    pub const QUIET: u16 = 3;
    pub const FLUSH: u16 = 4;
    pub const CALLBACK: u16 = 13;
    pub const AVAILABLE: u16 = 24;
    pub const VERSION: u16 = 25;
    pub const TOTAL_LOAD: u16 = 26;
    pub const LOAD: u16 = 27;
    /// restCmd ($2B = 43) inserts a rest of `param1` half-frames in
    /// a sequence-channel (note channel for note/freq/wave synth).
    /// Sound 1994, 2-95. Sample-mixing channels (Marathon 1's case)
    /// receive restCmd as part of envelope sequencing but it has
    /// no effect on raw PCM playback — we accept it as a recognised
    /// no-op so the unhandled-cmds sentinel doesn't trip.
    pub const REST: u16 = 43;
    pub const VOLUME: u16 = 46;
    pub const SOUND: u16 = 80;
    pub const BUFFER: u16 = 81;
    pub const RATE: u16 = 82;
    pub const GET_RATE: u16 = 85;
}

/// A sound command extracted from a snd resource or queued via SndDoCommand.
/// Sound 1994, 2-92
#[derive(Clone, Debug)]
pub struct SndCommand {
    pub cmd: u16,
    pub param1: i16,
    pub param2: u32,
}

/// One host-side unsigned 8-bit stereo PCM frame (silence = 0x80).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StereoSample {
    pub left: u8,
    pub right: u8,
}

impl StereoSample {
    pub(crate) const SILENCE: Self = Self {
        left: 0x80,
        right: 0x80,
    };

    pub(crate) fn mono(sample: u8) -> Self {
        Self {
            left: sample,
            right: sample,
        }
    }

    pub(crate) fn downmix(self) -> u8 {
        let left = self.left as i32 - 0x80;
        let right = self.right as i32 - 0x80;
        ((left + right) / 2 + 0x80).clamp(0, 255) as u8
    }
}

/// Host-side copy of sample data currently being played on a channel.
#[derive(Clone, Debug)]
struct PlayingBuffer {
    /// Unsigned 8-bit stereo PCM frames (Mac format: silence = 0x80).
    samples: Vec<StereoSample>,
    /// Source sample rate as Mac Fixed 16.16.
    sample_rate_fixed: u32,
    /// Current playback position in samples (fixed-point 32.32).
    position: u64,
    /// Resampling step: source_rate / output_rate in fixed-point 32.32.
    step: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybackKind {
    Buffer,
    File,
}

/// Double-buffer playback state for SndPlayDoubleBuffer.
/// Sound 1994, 2-111 to 2-113
#[derive(Clone, Debug)]
pub struct DoubleBufferState {
    /// Guest pointer to the SndDoubleBufferHeader record.
    pub header_ptr: u32,
    /// Index of the buffer currently being played (0 or 1).
    pub current_buffer: usize,
    /// Guest pointer to the doubleback callback procedure.
    pub callback_addr: u32,
    /// Guest pointer to the channel.
    pub chan_ptr: u32,
    /// Sample rate as Mac Fixed 16.16.
    pub sample_rate: u32,
    /// Number of interleaved channels in each buffer.
    pub num_channels: usize,
    /// Bits per sample in each channel.
    pub sample_size: usize,
    /// Whether we've seen dbLastBuffer and should stop after this buffer.
    pub last_buffer_seen: bool,
    /// Whether we're waiting for the callback to finish refilling.
    pub waiting_for_callback: bool,
    /// Which buffer slots currently have an outstanding doubleback refill.
    pub pending_callback_buffers: [bool; 2],
}

impl DoubleBufferState {
    fn buffer_index(index: usize) -> usize {
        index & 1
    }

    fn callback_pending_for(&self, index: usize) -> bool {
        self.pending_callback_buffers[Self::buffer_index(index)]
    }

    fn arm_callback_for(&mut self, index: usize) -> bool {
        let index = Self::buffer_index(index);
        if self.pending_callback_buffers[index] {
            return false;
        }
        self.pending_callback_buffers[index] = true;
        self.waiting_for_callback = true;
        true
    }

    pub(crate) fn complete_callback_for(&mut self, index: usize) {
        let index = Self::buffer_index(index);
        self.pending_callback_buffers[index] = false;
        self.waiting_for_callback = self.pending_callback_buffers.iter().any(|pending| *pending);
    }
}

/// A pending double-buffer callback that the runner should fire.
#[derive(Clone, Debug)]
pub struct PendingDoubleBackCallback {
    /// Guest pointer to the doubleback procedure.
    pub callback_addr: u32,
    /// Guest pointer to the SndChannel record.
    pub chan_ptr: u32,
    /// Guest pointer to the SndDoubleBufferHeader.
    pub header_ptr: u32,
    /// Index of the exhausted buffer (0 or 1).
    pub exhausted_buffer_index: usize,
}

/// A pending callback or completion routine to fire from interrupt context.
#[derive(Clone, Debug)]
pub enum PendingSoundCallback {
    /// Callback procedure associated with a channel via SndNewChannel.
    /// Signature (Sound 1994, 2-152):
    ///   PROCEDURE MyCallbackProcedure(theChan: SndChannelPtr; theCmd: SndCommand);
    Command {
        callback_addr: u32,
        chan_ptr: u32,
        cmd: SndCommand,
    },
    /// Completion routine associated with SndStartFilePlay.
    /// Signature (Sound 1994, 2-151):
    ///   PROCEDURE MyFilePlayCompletionRoutine(chan: SndChannelPtr);
    FileCompletion { callback_addr: u32, chan_ptr: u32 },
}

/// Per-channel state.
#[derive(Clone, Debug)]
pub struct SndChannel {
    /// Guest memory pointer for this channel (returned to the game).
    pub guest_ptr: u32,
    /// Whether we allocated this channel (vs game-provided).
    pub allocated: bool,
    /// Command queue (circular buffer).
    queue: Vec<SndCommand>,
    q_head: usize,
    q_tail: usize,
    /// Currently playing buffer, if any.
    playing: Option<PlayingBuffer>,
    /// Whether the current playback came from bufferCmd/SndPlay or SndStartFilePlay.
    playback_kind: Option<PlaybackKind>,
    /// Callback procedure installed by SndNewChannel in the guest channel record.
    pub callback_addr: u32,
    /// Current channel volume as packed stereo values (right in high word).
    volume: u32,
    /// Current playback rate relative to the channel's base sample rate.
    rate_fixed: u32,
    /// callBackCmd commands waiting for the current playback to complete.
    pending_callback_cmds: Vec<SndCommand>,
    /// Completion routine for the current asynchronous SndStartFilePlay.
    file_completion_addr: u32,
    /// Whether file playback is currently paused.
    file_paused: bool,
    /// Active double-buffer state, if SndPlayDoubleBuffer is in use.
    pub double_buffer: Option<DoubleBufferState>,
    /// Number of SndDoubleBuffer records decoded into this channel.
    pub debug_double_buffer_loads: u32,
    /// Number of decoded SndDoubleBuffer records that contained at least one
    /// non-silent stereo frame.
    pub debug_double_buffer_non_silent_loads: u32,
    /// Total decoded SndDoubleBuffer frames for this channel.
    pub debug_double_buffer_frames_loaded: u64,
    /// Total decoded non-silent SndDoubleBuffer frames for this channel.
    pub debug_double_buffer_non_silent_frames: u64,
    /// Decoded SndDoubleBuffer frames captured as mono unsigned 8-bit PCM for
    /// waveform probes. This is diagnostic data; normal playback uses
    /// `playing`.
    pub debug_double_buffer_captured_samples: Vec<u8>,
    /// Sound Manager-owned temporary channels created for high-level calls
    /// such as NIL-channel SndPlay/SysBeep. These should be released after
    /// their queued playback drains, not immediately after the trap returns.
    auto_dispose_when_idle: bool,
}

impl SndChannel {
    pub(crate) fn set_file_paused(&mut self, paused: bool) {
        self.file_paused = paused;
    }
    pub fn new(guest_ptr: u32, allocated: bool) -> Self {
        Self {
            guest_ptr,
            allocated,
            queue: Vec::with_capacity(STD_Q_LENGTH),
            q_head: 0,
            q_tail: 0,
            playing: None,
            playback_kind: None,
            callback_addr: 0,
            volume: FULL_STEREO_VOLUME,
            rate_fixed: UNITY_RATE_FIXED,
            pending_callback_cmds: Vec::new(),
            file_completion_addr: 0,
            file_paused: false,
            double_buffer: None,
            debug_double_buffer_loads: 0,
            debug_double_buffer_non_silent_loads: 0,
            debug_double_buffer_frames_loaded: 0,
            debug_double_buffer_non_silent_frames: 0,
            debug_double_buffer_captured_samples: Vec::new(),
            auto_dispose_when_idle: false,
        }
    }

    /// Enqueue a command. Returns false if queue is full.
    pub fn enqueue(&mut self, cmd: SndCommand) -> bool {
        if self.queue.len() < STD_Q_LENGTH {
            self.queue.push(cmd);
            true
        } else {
            false
        }
    }

    /// Dequeue the next command, if any.
    fn dequeue(&mut self) -> Option<SndCommand> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    /// Clear the command queue.
    pub fn flush(&mut self) {
        self.queue.clear();
        self.q_head = 0;
        self.q_tail = 0;
    }

    /// Stop playback without flushing queued commands.
    pub fn quiet(&mut self) {
        self.playing = None;
        self.playback_kind = None;
        self.pending_callback_cmds.clear();
        self.file_completion_addr = 0;
        self.file_paused = false;
        self.rate_fixed = UNITY_RATE_FIXED;
        self.double_buffer = None;
    }

    /// Start playing a buffer of unsigned 8-bit samples.
    pub(crate) fn play_buffer(
        &mut self,
        samples: Vec<u8>,
        sample_rate_fixed: u32,
        kind: PlaybackKind,
        file_completion_addr: u32,
    ) {
        let samples = samples.into_iter().map(StereoSample::mono).collect();
        self.play_stereo_buffer(samples, sample_rate_fixed, kind, file_completion_addr);
    }

    /// Start playing a buffer of unsigned 8-bit stereo frames.
    pub(crate) fn play_stereo_buffer(
        &mut self,
        samples: Vec<StereoSample>,
        sample_rate_fixed: u32,
        kind: PlaybackKind,
        file_completion_addr: u32,
    ) {
        self.rate_fixed = UNITY_RATE_FIXED;
        self.playing = Some(PlayingBuffer {
            samples,
            sample_rate_fixed,
            position: 0,
            step: fixed_div(sample_rate_fixed as u64, (OUTPUT_RATE as u64) << 16),
        });
        self.playback_kind = Some(kind);
        self.file_completion_addr = file_completion_addr;
        self.file_paused = false;
    }

    /// Returns true if this channel is currently producing audio.
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    pub fn has_active_playback(&self) -> bool {
        self.playing.is_some() || self.file_paused
    }

    pub fn queue_callback(&mut self, cmd: SndCommand) {
        self.pending_callback_cmds.push(cmd);
    }

    pub fn take_pending_callback_cmds(&mut self) -> Vec<SndCommand> {
        std::mem::take(&mut self.pending_callback_cmds)
    }

    /// Whether advancing this channel is required to release guest work that
    /// is waiting behind playback completion.
    fn has_playback_gated_callback(&self) -> bool {
        (self.has_active_playback()
            && ((self.callback_addr != 0 && !self.pending_callback_cmds.is_empty())
                || self.file_completion_addr != 0))
            || self.double_buffer.is_some()
    }

    pub fn set_volume(&mut self, packed_volume: u32) {
        self.volume = packed_volume;
    }

    pub fn set_rate(&mut self, rate_fixed: u32) {
        self.rate_fixed = rate_fixed;
        if let Some(ref mut playing) = self.playing {
            playing.step = playback_step(playing.sample_rate_fixed, rate_fixed);
        }
    }

    pub fn current_rate(&self) -> u32 {
        self.rate_fixed
    }

    pub fn pause_file_playback_toggle(&mut self) {
        if self.playback_kind == Some(PlaybackKind::File) {
            self.file_paused = !self.file_paused;
        }
    }

    pub(crate) fn mark_auto_dispose_when_idle(&mut self) {
        self.auto_dispose_when_idle = true;
    }

    fn is_ready_for_auto_dispose(&self) -> bool {
        self.auto_dispose_when_idle
            && self.playing.is_none()
            && !self.file_paused
            && self.double_buffer.is_none()
            && self.queue.is_empty()
            && self.pending_callback_cmds.is_empty()
    }
}

/// Top-level process-owned sound manager state shared by CPU adapters.
#[derive(Clone, Debug)]
pub struct SoundManager {
    pub channels: Vec<SndChannel>,
    /// Pending double-buffer callbacks to fire on the next frame.
    pub pending_callbacks: Vec<PendingDoubleBackCallback>,
    /// Pending sound callback procedures / completion routines.
    pub pending_sound_callbacks: Vec<PendingSoundCallback>,
    /// Debug counters for diagnosing sound issues.
    pub debug_cmd_count: u32,
    pub debug_buffer_cmd_count: u32,
    /// `SndPlayDoubleBuffer` submissions (SoundDispatch routine
    /// `$20`). Separate counter because double-buffered playback and
    /// bufferCmd-based games like EV both feed `mix_frame`, but through
    /// different dispatch.
    pub debug_double_buffer_count: u32,
    pub debug_samples_mixed: u64,
    pub debug_unhandled_cmds: Vec<u16>,
    /// Deduplicated list of distinct `SndCommand` cmd codes seen via
    /// `execute_sound_command`. Sibling of `debug_unhandled_cmds`
    /// (which only tracks the `_ =>` arm); this tracks ALL cmd codes
    /// including the matched ones.
    pub debug_cmd_codes_seen: Vec<u16>,
    /// `SndStartFilePlay` (SoundDispatch routine `$00`) submissions.
    /// Resource-backed calls can later execute bufferCmd internally,
    /// but they still count as reaching the file-play trap family.
    /// M2's primary audio path goes through this trap and DOES NOT
    /// increment `debug_buffer_cmd_count` or `debug_double_buffer_count`.
    /// Per-path visibility: EV uses bufferCmd
    /// (`debug_buffer_cmd_count`), M2 uses `SndStartFilePlay`
    /// (`debug_file_play_count`), other games can use
    /// `SndPlayDoubleBuffer` (`debug_double_buffer_count`).
    pub debug_file_play_count: u32,
    /// System alert volume exposed through Get/SetSysBeepVolume.
    sys_beep_volume: u32,
    /// Output-device default volume exposed through Get/SetDefaultOutputVolume.
    /// Sound 1994, 2-141 to 2-142 describes this as the device's default
    /// setting, distinct from channel `volumeCmd` gain and current
    /// output-port volume.
    default_output_volume: u32,
}

impl Default for SoundManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundManager {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            pending_callbacks: Vec::new(),
            pending_sound_callbacks: Vec::new(),
            debug_cmd_count: 0,
            debug_buffer_cmd_count: 0,
            debug_double_buffer_count: 0,
            debug_samples_mixed: 0,
            debug_unhandled_cmds: Vec::new(),
            debug_cmd_codes_seen: Vec::new(),
            debug_file_play_count: 0,
            sys_beep_volume: FULL_STEREO_VOLUME,
            default_output_volume: FULL_STEREO_VOLUME,
        }
    }

    pub(crate) fn is_pristine(&self) -> bool {
        self.channels.is_empty()
            && self.pending_callbacks.is_empty()
            && self.pending_sound_callbacks.is_empty()
            && self.debug_cmd_count == 0
            && self.debug_buffer_cmd_count == 0
            && self.debug_double_buffer_count == 0
            && self.debug_samples_mixed == 0
            && self.debug_unhandled_cmds.is_empty()
            && self.debug_cmd_codes_seen.is_empty()
            && self.debug_file_play_count == 0
            && self.sys_beep_volume == FULL_STEREO_VOLUME
            // Classic construction uses packed full stereo volume, while a
            // freshly loaded native process uses the Sound Manager's unity
            // 16.16 device default. Both are unattached launch defaults.
            && matches!(self.default_output_volume, FULL_STEREO_VOLUME | 0x0001_0000)
    }

    pub(crate) fn has_playback_gated_callback(&self) -> bool {
        self.channels
            .iter()
            .any(SndChannel::has_playback_gated_callback)
    }

    /// Register a guest-visible channel in the canonical process manager.
    ///
    /// `SndNewChannel` creates the channel and its FIFO command queue in the
    /// application's heap. Inside Macintosh: Sound (1994), pp. 2-19--2-21.
    pub(crate) fn register_channel(
        &mut self,
        guest_ptr: u32,
        allocated: bool,
        callback_addr: u32,
    ) {
        self.remove_channel(guest_ptr);
        let mut channel = SndChannel::new(guest_ptr, allocated);
        channel.callback_addr = callback_addr;
        self.channels.push(channel);
    }

    fn ensure_channel_mut(&mut self, guest_ptr: u32) -> &mut SndChannel {
        if let Some(index) = self
            .channels
            .iter()
            .position(|channel| channel.guest_ptr == guest_ptr)
        {
            return &mut self.channels[index];
        }
        self.channels.push(SndChannel::new(guest_ptr, false));
        self.channels.last_mut().unwrap()
    }

    fn record_command(&mut self, command: u16) {
        self.debug_cmd_count = self.debug_cmd_count.saturating_add(1);
        if !self.debug_cmd_codes_seen.contains(&command) {
            self.debug_cmd_codes_seen.push(command);
        }
    }

    /// Submit decoded `bufferCmd` samples directly to the process channel.
    /// Sound commands belong to the channel managed by the Sound Manager,
    /// irrespective of the ISA that submitted them. Sound 1994, pp. 2-92--2-97.
    pub(crate) fn play_buffer_command(
        &mut self,
        guest_ptr: u32,
        samples: Vec<u8>,
        sample_rate_fixed: u32,
    ) {
        self.record_command(cmd::BUFFER);
        self.debug_buffer_cmd_count = self.debug_buffer_cmd_count.saturating_add(1);
        self.ensure_channel_mut(guest_ptr).play_buffer(
            samples,
            sample_rate_fixed,
            PlaybackKind::Buffer,
            0,
        );
    }

    /// Apply a native immediate command to the canonical process channel.
    /// `SndDoImmediate` bypasses the FIFO while leaving queued commands in
    /// place. Sound 1994, pp. 2-93 and 2-128--2-130.
    pub(crate) fn execute_immediate_command(&mut self, guest_ptr: u32, command: SndCommand) {
        self.record_command(command.cmd);
        let channel = self.ensure_channel_mut(guest_ptr);
        match command.cmd {
            cmd::QUIET => channel.quiet(),
            cmd::FLUSH => channel.flush(),
            cmd::VOLUME => channel.set_volume(command.param2),
            cmd::RATE => channel.set_rate(command.param2),
            _ => {}
        }
    }

    /// Queue a native command on the canonical process channel FIFO.
    pub(crate) fn enqueue_command(&mut self, guest_ptr: u32, command: SndCommand) -> bool {
        self.record_command(command.cmd);
        self.ensure_channel_mut(guest_ptr).enqueue(command)
    }

    pub fn sys_beep_volume(&self) -> u32 {
        self.sys_beep_volume
    }

    pub fn set_sys_beep_volume(&mut self, volume: u32) {
        self.sys_beep_volume = volume;
    }

    pub fn default_output_volume(&self) -> u32 {
        self.default_output_volume
    }

    pub fn set_default_output_volume(&mut self, volume: u32) {
        self.default_output_volume = volume;
    }

    /// Find a channel by its guest pointer.
    pub fn find_channel_mut(&mut self, guest_ptr: u32) -> Option<&mut SndChannel> {
        self.channels.iter_mut().find(|c| c.guest_ptr == guest_ptr)
    }

    /// Remove and return a channel by guest pointer.
    pub fn take_channel(&mut self, guest_ptr: u32) -> Option<SndChannel> {
        self.channels
            .iter()
            .position(|c| c.guest_ptr == guest_ptr)
            .map(|idx| self.channels.remove(idx))
    }

    /// Remove a channel by guest pointer. Returns true if found.
    pub fn remove_channel(&mut self, guest_ptr: u32) -> bool {
        self.take_channel(guest_ptr).is_some()
    }

    pub(crate) fn idle_auto_dispose_channel_ptrs(&self) -> Vec<u32> {
        self.channels
            .iter()
            .filter(|chan| chan.is_ready_for_auto_dispose())
            .map(|chan| chan.guest_ptr)
            .collect()
    }

    /// Process pending commands on all channels, then mix `num_samples`
    /// of mono output into a buffer of unsigned 8-bit PCM (silence = 0x80).
    pub fn mix_frame(&mut self, num_samples: usize) -> Vec<u8> {
        self.mix_frame_stereo_frames(num_samples)
            .into_iter()
            .map(StereoSample::downmix)
            .collect()
    }

    /// Process pending commands on all channels, then mix `num_samples`
    /// of stereo output into interleaved unsigned 8-bit PCM
    /// (left, right, left, right; silence = 0x80).
    pub fn mix_frame_stereo(&mut self, num_samples: usize) -> Vec<u8> {
        let frames = self.mix_frame_stereo_frames(num_samples);
        let mut out = Vec::with_capacity(frames.len() * 2);
        for frame in frames {
            out.push(frame.left);
            out.push(frame.right);
        }
        out
    }

    fn mix_frame_stereo_frames(&mut self, num_samples: usize) -> Vec<StereoSample> {
        // Process queued commands only on idle channels. SndDoCommand feeds
        // the channel FIFO; SndDoImmediate is the bypass path for stopping
        // playback immediately.
        let mut queued_callbacks = Vec::new();
        for chan in &mut self.channels {
            if chan.has_active_playback() || chan.double_buffer.is_some() {
                continue;
            }

            while let Some(cmd) = chan.dequeue() {
                match cmd.cmd {
                    cmd::NULL => {}
                    cmd::QUIET => chan.quiet(),
                    cmd::FLUSH => chan.flush(),
                    cmd::CALLBACK => {
                        if chan.callback_addr != 0 {
                            queued_callbacks.push(PendingSoundCallback::Command {
                                callback_addr: chan.callback_addr,
                                chan_ptr: chan.guest_ptr,
                                cmd,
                            });
                        }
                    }
                    cmd::BUFFER | cmd::SOUND => {
                        // Buffer data should already be loaded by the trap handler.
                    }
                    _ => {}
                }
            }
        }
        self.pending_sound_callbacks.extend(queued_callbacks);

        // Mix all playing channels into the output buffer.
        let mut output = vec![StereoSample::SILENCE; num_samples];
        let mut any_active = false;

        // Collect double-buffer exhaustion events to process after mixing.
        let mut exhausted: Vec<(u32, u32, u32, usize)> = Vec::new(); // (callback, chan_ptr, header_ptr, exhausted_buf_idx)

        for chan in &mut self.channels {
            // If channel has a double-buffer but nothing playing, it means
            // we're waiting for the next buffer to be ready. Keep the stream
            // alive with silence, and if no refill callback is outstanding,
            // request one so an underrun cannot wedge the channel forever.
            if chan.playing.is_none() {
                let mut clear_double_buffer = false;
                if let Some(ref mut db) = chan.double_buffer {
                    if db.last_buffer_seen {
                        clear_double_buffer = true;
                    } else {
                        any_active = true;
                        if !db.callback_pending_for(db.current_buffer) {
                            db.arm_callback_for(db.current_buffer);
                            exhausted.push((
                                db.callback_addr,
                                db.chan_ptr,
                                db.header_ptr,
                                db.current_buffer,
                            ));
                        }
                    }
                }
                if clear_double_buffer {
                    chan.double_buffer = None;
                }
            }
            if chan.file_paused {
                any_active = true;
                continue;
            }

            if let Some(ref mut buf) = chan.playing {
                any_active = true;
                for slot in output.iter_mut().take(num_samples) {
                    let Some(source_sample) =
                        resampled_sample(&buf.samples, buf.position, buf.step)
                    else {
                        break;
                    };
                    let sample = apply_volume_stereo(source_sample, chan.volume);
                    let mixed_left = slot.left as i16 + sample.left as i16 - 0x80;
                    let mixed_right = slot.right as i16 + sample.right as i16 - 0x80;
                    slot.left = mixed_left.clamp(0, 255) as u8;
                    slot.right = mixed_right.clamp(0, 255) as u8;
                    buf.position += buf.step;
                }
                let final_idx = (buf.position >> 32) as usize;
                if final_idx >= buf.samples.len() {
                    let playback_kind = chan.playback_kind;
                    let callback_addr = chan.callback_addr;
                    let chan_ptr = chan.guest_ptr;
                    let file_completion_addr = chan.file_completion_addr;
                    let callback_cmds = chan.take_pending_callback_cmds();
                    chan.playing = None;
                    chan.playback_kind = None;
                    chan.file_completion_addr = 0;
                    // If this channel has a double-buffer, request callback for
                    // the exhausted buffer and switch to the other one.
                    if let Some(ref mut db) = chan.double_buffer {
                        if !db.last_buffer_seen {
                            let exhausted_idx = db.current_buffer;
                            db.current_buffer ^= 1; // switch to other buffer
                            if db.arm_callback_for(exhausted_idx) {
                                exhausted.push((
                                    db.callback_addr,
                                    db.chan_ptr,
                                    db.header_ptr,
                                    exhausted_idx,
                                ));
                            }
                        }
                    }
                    if callback_addr != 0 {
                        for cmd in callback_cmds {
                            self.pending_sound_callbacks
                                .push(PendingSoundCallback::Command {
                                    callback_addr,
                                    chan_ptr,
                                    cmd,
                                });
                        }
                    }
                    if playback_kind == Some(PlaybackKind::File) && file_completion_addr != 0 {
                        self.pending_sound_callbacks
                            .push(PendingSoundCallback::FileCompletion {
                                callback_addr: file_completion_addr,
                                chan_ptr,
                            });
                    }
                }
            }
        }

        // Queue pending callbacks for exhausted double buffers.
        for (callback_addr, chan_ptr, header_ptr, exhausted_buf_idx) in exhausted {
            // dbhBufferPtr[0] at header+12, dbhBufferPtr[1] at header+16
            // Sound 1994, 2-111
            self.pending_callbacks.push(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: exhausted_buf_idx,
            });
        }

        if any_active {
            self.debug_samples_mixed += output.len() as u64;
            output
        } else {
            Vec::new()
        }
    }

    /// Return the nearest output-sample boundary where an active playback
    /// buffer will exhaust. Callers that can load a queued follow-up buffer
    /// should split mixing at this point to avoid emitting silence between
    /// back-to-back Sound Manager buffers.
    pub fn samples_until_next_exhaustion(&self) -> Option<usize> {
        self.channels
            .iter()
            .filter_map(|chan| {
                let playing = chan.playing.as_ref()?;
                if playing.step == 0 {
                    return None;
                }
                let end = (playing.samples.len() as u128) << 32;
                let position = playing.position as u128;
                if position >= end {
                    return Some(0);
                }
                let step = playing.step as u128;
                let samples = (end - position).div_ceil(step);
                Some(samples.min(usize::MAX as u128) as usize)
            })
            .min()
    }
}

/// Fixed-point division: (x / y) with 32 fractional bits.
/// Reference: executor sound.cpp snd_fixed_div
fn fixed_div(x: u64, y: u64) -> u64 {
    if y == 0 {
        return 0;
    }
    let int_part = x / y;
    let remainder = x - y * int_part;
    let frac_part = (remainder << 32) / y;
    (int_part << 32) + frac_part
}

fn playback_step(sample_rate_fixed: u32, rate_fixed: u32) -> u64 {
    let base = fixed_div(sample_rate_fixed as u64, (OUTPUT_RATE as u64) << 16);
    ((base as u128 * rate_fixed as u128) >> 16) as u64
}

fn resampled_sample(samples: &[StereoSample], position: u64, step: u64) -> Option<StereoSample> {
    // Sound 1994 defines drop-sample conversion as using an existing
    // sample instead of a linear interpolated point. That preserves the
    // 8-bit edges of classic low-rate sampled effects during upsampling.
    if step < (1u64 << 32) {
        let sample_idx = (position >> 32) as usize;
        return samples.get(sample_idx).copied();
    }
    interpolated_sample(samples, position)
}

fn interpolated_sample(samples: &[StereoSample], position: u64) -> Option<StereoSample> {
    let sample_idx = (position >> 32) as usize;
    let first = *samples.get(sample_idx)?;
    let second = samples.get(sample_idx + 1).copied().unwrap_or(first);
    let frac = (position & 0xFFFF_FFFF) as i64;
    Some(StereoSample {
        left: interpolate_u8(first.left, second.left, frac),
        right: interpolate_u8(first.right, second.right, frac),
    })
}

fn interpolate_u8(first: u8, second: u8, frac: i64) -> u8 {
    let delta = second as i64 - first as i64;
    let interpolated = first as i64 + (delta * frac) / (1i64 << 32);
    interpolated.clamp(0, 255) as u8
}

#[cfg(test)]
fn apply_volume(sample: u8, packed_volume: u32) -> u8 {
    let left = (packed_volume & 0xFFFF) as i32;
    let right = ((packed_volume >> 16) & 0xFFFF) as i32;
    let average = (left + right) / 2;
    apply_volume_channel(sample, average)
}

fn apply_volume_stereo(sample: StereoSample, packed_volume: u32) -> StereoSample {
    let left_volume = (packed_volume & 0xFFFF) as i32;
    let right_volume = ((packed_volume >> 16) & 0xFFFF) as i32;
    StereoSample {
        left: apply_volume_channel(sample.left, left_volume),
        right: apply_volume_channel(sample.right, right_volume),
    }
}

fn apply_volume_channel(sample: u8, volume: i32) -> u8 {
    let centered = sample as i32 - 0x80;
    let scaled = centered * volume / FULL_VOLUME as i32;
    (scaled + 0x80).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression gate for `SoundManager::new()` default field values:
    ///   - `channels` / `pending_callbacks` /
    ///     `pending_sound_callbacks` start empty (SndManager starts
    ///     with no allocated channels)
    ///   - all debug counters start at 0
    ///
    /// A future refactor that reorders fields or changes defaults
    /// would silently break callers that depend on a clean initial
    /// state.
    #[test]
    fn sound_manager_new_zero_initialized() {
        let sm = SoundManager::new();
        assert!(sm.channels.is_empty(), "channels must start empty");
        assert!(
            sm.pending_callbacks.is_empty(),
            "pending_callbacks must start empty"
        );
        assert!(
            sm.pending_sound_callbacks.is_empty(),
            "pending_sound_callbacks must start empty"
        );
        assert_eq!(sm.debug_cmd_count, 0, "debug_cmd_count must start at 0");
        assert_eq!(
            sm.debug_buffer_cmd_count, 0,
            "debug_buffer_cmd_count must start at 0"
        );
        assert_eq!(
            sm.debug_double_buffer_count, 0,
            "debug_double_buffer_count must start at 0"
        );
        assert_eq!(
            sm.debug_samples_mixed, 0,
            "debug_samples_mixed must start at 0"
        );
        assert!(
            sm.debug_unhandled_cmds.is_empty(),
            "debug_unhandled_cmds must start empty"
        );
        assert!(
            sm.debug_cmd_codes_seen.is_empty(),
            "debug_cmd_codes_seen must start empty"
        );
        assert_eq!(
            sm.debug_file_play_count, 0,
            "debug_file_play_count must start at 0"
        );
        assert_eq!(
            sm.sys_beep_volume(),
            FULL_STEREO_VOLUME,
            "system beep volume starts at full L+R"
        );
        assert_eq!(
            sm.default_output_volume(),
            FULL_STEREO_VOLUME,
            "default output volume starts at full L+R"
        );
    }

    /// Module-level constants encode Mac Sound Manager invariants
    /// that the rest of the sound pipeline silently depends on:
    ///   - OUTPUT_RATE = 22050 Hz (the mix-frame sample rate
    ///     EV + M2 both produce at; changing this invalidates
    ///     every `samples_mixed` floor in the ungated gates).
    ///   - FULL_VOLUME = 0x0100 (8 bits of volume range per IM:
    ///     Sound 2-9).
    ///   - UNITY_RATE_FIXED = 0x0001_0000 (1.0 as Mac Fixed
    ///     16.16 — 1:1 sample-rate playback).
    ///   - STD_Q_LENGTH = 128 (per-channel command queue depth,
    ///     default per IM:Sound 2-107).
    #[test]
    fn sound_module_constants_match_mac_sound_manager() {
        assert_eq!(OUTPUT_RATE, 22050, "OUTPUT_RATE = 22050 Hz");
        assert_eq!(
            FULL_VOLUME, 0x0100,
            "FULL_VOLUME = 256 (8-bit volume range)"
        );
        assert_eq!(
            UNITY_RATE_FIXED, 0x0001_0000,
            "UNITY_RATE_FIXED = 1.0 as 16.16 Fixed"
        );
        assert_eq!(STD_Q_LENGTH, 128, "STD_Q_LENGTH = 128 queue slots");
    }

    /// `sound::cmd::*` constants must match IM:Sound 1994's documented
    /// command codes. A drift here would corrupt every
    /// `bufferCmd` / `soundCmd` / `rateCmd` dispatch without any
    /// downstream test noticing — `execute_sound_command`'s match
    /// statement would silently route the wrong command to the wrong
    /// arm.
    ///
    /// References:
    ///   Sound 1994, 2-126 (SndCommand cmd field table)
    ///   Executor sound.cpp cmd table
    #[test]
    fn sound_cmd_constants_match_ism_sound_1994() {
        assert_eq!(cmd::NULL, 0, "nullCmd per IM:Sound 2-126");
        assert_eq!(cmd::QUIET, 3, "quietCmd per IM:Sound 2-126");
        assert_eq!(cmd::FLUSH, 4, "flushCmd per IM:Sound 2-126");
        assert_eq!(cmd::CALLBACK, 13, "callBackCmd per IM:Sound 2-126");
        assert_eq!(cmd::AVAILABLE, 24, "availableCmd per IM:Sound 2-92");
        assert_eq!(cmd::VERSION, 25, "versionCmd per IM:Sound 2-92");
        assert_eq!(cmd::TOTAL_LOAD, 26, "totalLoadCmd per IM:Sound 2-92");
        assert_eq!(cmd::LOAD, 27, "loadCmd per IM:Sound 2-92");
        assert_eq!(cmd::REST, 43, "restCmd per IM:Sound 2-95");
        assert_eq!(cmd::VOLUME, 46, "volumeCmd per IM:Sound 2-126");
        assert_eq!(cmd::SOUND, 80, "soundCmd per IM:Sound 2-126");
        assert_eq!(cmd::BUFFER, 81, "bufferCmd per IM:Sound 2-126");
        assert_eq!(cmd::RATE, 82, "rateCmd per IM:Sound 2-126");
        assert_eq!(cmd::GET_RATE, 85, "getRateCmd per IM:Sound 2-126");
    }

    /// `mix_frame` leaves queued commands behind active playback.
    /// `SndDoCommand` feeds a FIFO, so a queued `quietCmd` must not
    /// preempt the current `bufferCmd`; `SndDoImmediate` is the
    /// documented queue-bypass path.
    #[test]
    fn mix_frame_defers_queued_quiet_until_playback_finishes() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0x80; 128], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        assert!(chan.is_playing(), "channel active pre-queue");

        chan.enqueue(SndCommand {
            cmd: cmd::QUIET,
            param1: 0,
            param2: 0,
        });
        sm.channels.push(chan);

        let output = sm.mix_frame(64);
        assert_eq!(
            output.len(),
            64,
            "active buffer must mix before queued QUIET"
        );
        assert_eq!(sm.debug_samples_mixed, 64);
        assert_eq!(sm.channels[0].queue.len(), 1);
        assert!(sm.channels[0].is_playing());

        sm.mix_frame(64);
        assert_eq!(
            sm.channels[0].queue.len(),
            1,
            "command remains queued until the next idle drain"
        );
        assert!(!sm.channels[0].is_playing());

        let output = sm.mix_frame(64);
        assert!(
            output.is_empty(),
            "idle queued QUIET drains with no playback"
        );
        assert!(sm.channels[0].queue.is_empty());
    }

    /// `SoundManager::mix_frame` positive case — an active playing
    /// buffer must produce a `Vec` of `num_samples` bytes AND advance
    /// `debug_samples_mixed` by that count. Sibling to the empty-case
    /// test.
    #[test]
    fn mix_frame_advances_samples_mixed_for_active_channel() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        // Install an active buffer at native OUTPUT_RATE so step = 1.0.
        chan.play_buffer(vec![0x80; 128], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(chan);

        let pre = sm.debug_samples_mixed;
        let output = sm.mix_frame(64);
        assert_eq!(
            output.len(),
            64,
            "mix_frame(64) with active channel must produce 64 bytes"
        );
        assert_eq!(
            sm.debug_samples_mixed,
            pre + 64,
            "debug_samples_mixed must advance by output.len()"
        );
    }

    /// `SoundManager::mix_frame` has two terminating behaviours:
    ///   (a) No active channels → return empty `Vec`.
    ///   (b) At least one active channel → return `Vec` of
    ///       `num_samples` bytes, add `output.len()` to
    ///       `debug_samples_mixed`.
    /// This test locks in case (a): an empty `SoundManager` returns an
    /// empty `Vec`, and `debug_samples_mixed` stays at 0.
    #[test]
    fn mix_frame_returns_empty_when_no_active_channels() {
        let mut sm = SoundManager::new();
        let output = sm.mix_frame(256);
        assert!(
            output.is_empty(),
            "mix_frame with no channels must return empty Vec (got len {})",
            output.len()
        );
        assert_eq!(sm.debug_samples_mixed, 0);

        // With channels but none playing/double-buffered: still empty.
        sm.channels.push(SndChannel::new(0x1234_0000, true));
        let output = sm.mix_frame(256);
        assert!(
            output.is_empty(),
            "mix_frame with idle channels must return empty Vec (got len {})",
            output.len()
        );
        assert_eq!(sm.debug_samples_mixed, 0);
    }

    /// `SndChannel::play_buffer` installs a new `PlayingBuffer`,
    /// resets `rate_fixed` to unity (caller must re-apply rate via
    /// `set_rate` AFTER `play_buffer` if needed), sets `playback_kind`,
    /// stores `file_completion_addr`, and clears `file_paused`. A
    /// regression that forgets any of these initialisations would
    /// corrupt subsequent `mix_frame` output.
    #[test]
    fn play_buffer_installs_playing_and_resets_state() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.rate_fixed = 0x0000_4000; // non-unity pre-state
        chan.file_paused = true; // pre-state that should clear

        let samples = vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
        let sample_rate = 11025 << 16; // half OUTPUT_RATE for step = 0.5
        chan.play_buffer(
            samples.clone(),
            sample_rate,
            PlaybackKind::File,
            0xABCD_1234,
        );

        assert_eq!(
            chan.rate_fixed, UNITY_RATE_FIXED,
            "rate_fixed reset to unity"
        );
        assert!(!chan.file_paused, "file_paused cleared");
        assert_eq!(chan.playback_kind, Some(PlaybackKind::File));
        assert_eq!(chan.file_completion_addr, 0xABCD_1234);
        let playing = chan.playing.as_ref().expect("playing installed");
        assert_eq!(
            playing.samples,
            samples
                .iter()
                .copied()
                .map(StereoSample::mono)
                .collect::<Vec<_>>()
        );
        assert_eq!(playing.sample_rate_fixed, sample_rate);
        assert_eq!(playing.position, 0, "position starts at 0");
        // Step at 11025 source / 22050 output = 0.5 → \$0_8000_0000.
        assert_eq!(
            playing.step, 0x8000_0000,
            "step must be fixed_div(sample_rate, OUTPUT_RATE<<16)"
        );
    }

    /// `playback_step` computes how far (in 32.32 fixed-point) the
    /// sample index should advance each `OUTPUT_RATE` tick given the
    /// source sample rate and the user rate multiplier (both 16.16
    /// Fixed). The formula is:
    ///   base = fixed_div(sample_rate, OUTPUT_RATE << 16)
    ///   step = (base * rate_fixed) >> 16
    ///
    /// Critical invariants:
    ///   - Source rate == OUTPUT_RATE with UNITY rate → step
    ///     should be exactly 1 sample per output sample (1.0 in
    ///     32.32 = \$1_0000_0000).
    ///   - Source rate == 2 * OUTPUT_RATE → step = 2.0.
    ///   - Rate multiplier 0.5 → half the base step.
    #[test]
    fn playback_step_at_unity_matches_sample_rate_ratio() {
        // Source == OUTPUT_RATE (22050), unity rate multiplier:
        //   base = 22050 << 16 / 22050 << 16 = 1.0
        //   step = 1.0 * 1.0 = 1.0 = \$1_0000_0000
        assert_eq!(
            playback_step(OUTPUT_RATE << 16, UNITY_RATE_FIXED),
            0x1_0000_0000,
            "22050 Hz source + unity rate = step 1.0"
        );

        // Source = 2 * OUTPUT_RATE (44100), unity rate:
        //   base = 2.0; step = 2.0 = \$2_0000_0000
        assert_eq!(
            playback_step((2 * OUTPUT_RATE) << 16, UNITY_RATE_FIXED),
            0x2_0000_0000,
            "44100 Hz source + unity rate = step 2.0"
        );

        // Source = OUTPUT_RATE, rate multiplier = 0.5:
        //   base = 1.0; step = 1.0 * 0.5 = 0.5 = \$0_8000_0000
        let half_rate = UNITY_RATE_FIXED / 2;
        assert_eq!(
            playback_step(OUTPUT_RATE << 16, half_rate),
            0x8000_0000,
            "22050 Hz source + 0.5x rate = step 0.5"
        );
    }

    /// `fixed_div(x, y)` returns `x / y` with 32 fractional
    /// bits. Used by `playback_step` to compute the resampling
    /// step. The result format is: upper 32 bits are the integer
    /// quotient, lower 32 are the fractional part.
    ///
    /// Divide-by-zero must return 0 (guard against malformed
    /// sound headers producing an infinite step).
    #[test]
    fn fixed_div_contract() {
        // 2 / 1 = 2.0 → upper = 2, lower = 0 → 0x2_0000_0000
        assert_eq!(fixed_div(2, 1), 0x2_0000_0000);
        // 1 / 2 = 0.5 → upper = 0, lower = 0x8000_0000
        assert_eq!(fixed_div(1, 2), 0x8000_0000);
        // 3 / 4 = 0.75 → upper = 0, lower = 0xC000_0000
        assert_eq!(fixed_div(3, 4), 0xC000_0000);
        // 5 / 2 = 2.5 → upper = 2, lower = 0x8000_0000
        assert_eq!(fixed_div(5, 2), 0x2_8000_0000);
        // Guard: divide-by-zero returns 0.
        assert_eq!(fixed_div(1, 0), 0);
        assert_eq!(fixed_div(0, 0), 0);
        // Zero numerator → zero result.
        assert_eq!(fixed_div(0, 5), 0);
    }

    /// `apply_volume` scales an unsigned 8-bit sample by the packed
    /// L/R volume, centering around 0x80 before scaling and re-
    /// centering after. The math is:
    ///   average = (left + right) / 2
    ///   scaled  = ((sample - 0x80) * average / FULL_VOLUME) + 0x80
    /// clamped to 0..=255.
    ///
    /// Regression gate for the scaling math itself — a bug here
    /// would either silence everything (too-small average), clip
    /// loudly (too-large), or flip polarity (reverse signed-vs-
    /// unsigned conversion).
    #[test]
    fn apply_volume_scales_around_0x80_center() {
        // FULL volume (L=0x100, R=0x100) preserves input.
        let full_lr = ((FULL_VOLUME as u32) << 16) | FULL_VOLUME as u32;
        assert_eq!(apply_volume(0x80, full_lr), 0x80, "silence stays silent");
        assert_eq!(apply_volume(0xFF, full_lr), 0xFF, "max positive stays max");
        assert_eq!(apply_volume(0x00, full_lr), 0x00, "max negative stays max");

        // Half volume (L=0x080, R=0x080) halves the excursion.
        let half = ((FULL_VOLUME as u32 / 2) << 16) | (FULL_VOLUME as u32 / 2);
        assert_eq!(
            apply_volume(0x80, half),
            0x80,
            "silence at any volume stays silent"
        );
        // 0xC0 = +0x40 from center → halved → +0x20 → 0xA0
        assert_eq!(apply_volume(0xC0, half), 0xA0);
        // 0x40 = -0x40 from center → halved → -0x20 → 0x60
        assert_eq!(apply_volume(0x40, half), 0x60);

        // Zero volume silences everything.
        assert_eq!(apply_volume(0xFF, 0), 0x80);
        assert_eq!(apply_volume(0x00, 0), 0x80);
    }

    /// `SndChannel::set_volume` stores the packed L/R volume directly
    /// into `chan.volume`. The `apply_volume` function consumes this
    /// packed value to scale each mixed sample; a regression that
    /// masked or shifted the stored value would change the effective
    /// playback loudness without any trap handler misbehaving.
    #[test]
    fn set_volume_stores_packed_lr() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        // Initial value is FULL L+R packed.
        let full = ((FULL_VOLUME as u32) << 16) | FULL_VOLUME as u32;
        assert_eq!(chan.volume, full, "default volume must be FULL L+R");

        // Test a non-uniform L=0x40 R=0xC0 pack.
        let packed = 0x00C0_0040u32;
        chan.set_volume(packed);
        assert_eq!(chan.volume, packed, "set_volume must store the exact u32");

        // Overwriting replaces (no merging).
        chan.set_volume(0);
        assert_eq!(chan.volume, 0, "set_volume must replace, not merge");
    }

    /// `SndChannel::queue_callback` appends;
    /// `take_pending_callback_cmds` drains. The pair is used by
    /// `execute_sound_command`'s `callBackCmd` path to defer user
    /// callback execution until the main thread services the channel.
    /// A regression that swaps push for replace (or take for clone)
    /// would break the defer-and-drain semantics.
    #[test]
    fn queue_callback_and_take_drain_semantics() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        assert!(chan.pending_callback_cmds.is_empty());

        chan.queue_callback(SndCommand {
            cmd: 11,
            param1: 1,
            param2: 0,
        });
        chan.queue_callback(SndCommand {
            cmd: 11,
            param1: 2,
            param2: 0,
        });
        assert_eq!(
            chan.pending_callback_cmds.len(),
            2,
            "queue_callback must append (not replace)"
        );
        assert_eq!(chan.pending_callback_cmds[0].param1, 1);
        assert_eq!(chan.pending_callback_cmds[1].param1, 2);

        let drained = chan.take_pending_callback_cmds();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].param1, 1);
        assert_eq!(drained[1].param1, 2);
        assert!(
            chan.pending_callback_cmds.is_empty(),
            "take must drain the Vec (mem::take semantics)"
        );

        // Double-take is empty, no panic.
        let second_drain = chan.take_pending_callback_cmds();
        assert!(second_drain.is_empty());
    }

    /// `SndChannel::set_rate` stores the new rate in `rate_fixed` AND
    /// recomputes `playing.step` if a buffer is playing. The step
    /// recomputation is what makes pitch-change during active playback
    /// actually affect output; forgetting it would keep the channel
    /// playing at the previous rate until the next buffer replaces the
    /// `PlayingBuffer` entirely.
    #[test]
    fn set_rate_updates_step_on_active_playing() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        // Set up active playback at 22050 Hz source.
        chan.playing = Some(PlayingBuffer {
            samples: vec![StereoSample::SILENCE; 64],
            sample_rate_fixed: 22050 << 16,
            position: 0,
            step: fixed_div(22050 << 16, (OUTPUT_RATE as u64) << 16),
        });
        let original_step = chan.playing.as_ref().unwrap().step;

        // set_rate to unity → step should match original (source == output).
        chan.set_rate(UNITY_RATE_FIXED);
        assert_eq!(chan.rate_fixed, UNITY_RATE_FIXED);
        let new_step = chan.playing.as_ref().unwrap().step;
        assert_eq!(
            new_step, original_step,
            "set_rate(UNITY) must recompute step to original for unity rate"
        );

        // Set rate to half — step should halve.
        let half_rate = UNITY_RATE_FIXED / 2;
        chan.set_rate(half_rate);
        assert_eq!(chan.rate_fixed, half_rate);
        let halved_step = chan.playing.as_ref().unwrap().step;
        assert!(
            halved_step < original_step,
            "set_rate(half) must reduce step; got {:#x} (orig {:#x})",
            halved_step,
            original_step
        );
    }

    /// `SndChannel::set_rate` without an active playing buffer must
    /// still store `rate_fixed` (so the NEXT `play_buffer` call can
    /// honour the pre-configured rate). The absence of a `playing`
    /// value must not prevent the store — a regression that added
    /// `if let Some(ref mut playing) = self.playing` as the outer
    /// guard of the whole function would break this.
    #[test]
    fn set_rate_stores_rate_without_active_playing() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        assert!(chan.playing.is_none());

        let target_rate = 0x0000_C000;
        chan.set_rate(target_rate);

        assert_eq!(
            chan.rate_fixed, target_rate,
            "set_rate must store rate_fixed even when no buffer is playing"
        );
        assert_eq!(
            chan.current_rate(),
            target_rate,
            "current_rate() must reflect the stored rate_fixed"
        );
    }

    /// `SndChannel::pause_file_playback_toggle` toggles `file_paused`
    /// ONLY when `playback_kind == File`. Calls on a Buffer-playback
    /// channel (or a channel with no active playback) must be no-ops:
    /// pause-file semantics are per IM:Sound 2-139 file-playback-
    /// specific.
    #[test]
    fn pause_file_playback_toggle_gated_on_playback_kind() {
        let mut chan = SndChannel::new(0x1234_0000, true);

        // No playback_kind set: toggle is no-op.
        assert!(!chan.file_paused);
        chan.pause_file_playback_toggle();
        assert!(
            !chan.file_paused,
            "toggle on non-file channel must not flip file_paused"
        );

        // Buffer playback: toggle still no-op.
        chan.playback_kind = Some(PlaybackKind::Buffer);
        chan.pause_file_playback_toggle();
        assert!(
            !chan.file_paused,
            "toggle on Buffer-kind channel must not flip file_paused"
        );

        // File playback: toggle flips.
        chan.playback_kind = Some(PlaybackKind::File);
        chan.pause_file_playback_toggle();
        assert!(
            chan.file_paused,
            "toggle on File-kind channel must flip file_paused (first call)"
        );
        chan.pause_file_playback_toggle();
        assert!(
            !chan.file_paused,
            "toggle on File-kind channel must flip file_paused (second call)"
        );
    }

    /// `is_playing` and `has_active_playback` have a subtle
    /// distinction. A paused file-play channel is "active playback"
    /// (SndPauseFilePlay stored the file-play state, waiting for
    /// SndPauseFilePlay(FALSE) to resume) but NOT "playing" (not
    /// producing samples this frame). `mix_frame` uses
    /// `has_active_playback` as an "is this channel alive" indicator
    /// so it knows not to free resources under the channel's feet;
    /// `is_playing` is used to decide whether to emit samples on this
    /// specific frame. Mixing these up would cause paused channels to
    /// either get torn down prematurely or to keep emitting samples
    /// while paused.
    #[test]
    fn is_playing_vs_has_active_playback_distinction() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        // Empty channel: neither.
        assert!(!chan.is_playing(), "fresh channel: is_playing = false");
        assert!(
            !chan.has_active_playback(),
            "fresh channel: has_active_playback = false"
        );

        // Active playback buffer: both true.
        chan.playing = Some(PlayingBuffer {
            samples: vec![StereoSample::SILENCE; 8],
            sample_rate_fixed: 22050 << 16,
            position: 0,
            step: 0x0001_0000,
        });
        assert!(chan.is_playing(), "active buffer: is_playing = true");
        assert!(
            chan.has_active_playback(),
            "active buffer: has_active_playback = true"
        );

        // Pause (file_paused = true, but playing still Some):
        // still playing AND still active.
        chan.file_paused = true;
        assert!(chan.is_playing(), "playing+paused: is_playing = true");
        assert!(
            chan.has_active_playback(),
            "playing+paused: has_active_playback = true"
        );

        // Pause-only (no playing): only has_active_playback.
        chan.playing = None;
        assert!(
            !chan.is_playing(),
            "paused without buffer: is_playing = false"
        );
        assert!(
            chan.has_active_playback(),
            "paused without buffer: has_active_playback = true \
             (pause state alone keeps channel alive for resume)"
        );
    }

    /// `SndChannel::flush` clears ONLY the command queue, `q_head`,
    /// and `q_tail`. It does NOT clear playback state (`playing`,
    /// `playback_kind`, `file_completion_addr`, `file_paused`,
    /// `rate_fixed`, `pending_callback_cmds`, `double_buffer`).
    /// Games issue QUIET + FLUSH pairs during channel init; a
    /// regression that made flush mistakenly stop playback mid-sample
    /// would cut active music short.
    #[test]
    fn flush_clears_queue_only_not_playback_state() {
        let mut chan = SndChannel::new(0x1234_0000, true);

        // Pre-populate every piece of playback state flush should
        // leave alone.
        chan.enqueue(SndCommand {
            cmd: 81,
            param1: 0,
            param2: 0,
        });
        chan.q_head = 7;
        chan.q_tail = 11;
        chan.playing = Some(PlayingBuffer {
            samples: vec![StereoSample::SILENCE; 16],
            sample_rate_fixed: 22050 << 16,
            position: 0,
            step: 0x0001_0000,
        });
        chan.playback_kind = Some(PlaybackKind::Buffer);
        chan.file_completion_addr = 0xABCD_1234;
        chan.file_paused = true;
        chan.rate_fixed = 0x0000_8000;
        chan.pending_callback_cmds.push(SndCommand {
            cmd: 11,
            param1: 0,
            param2: 0,
        });

        chan.flush();

        assert!(chan.queue.is_empty(), "flush must clear the queue");
        assert_eq!(chan.q_head, 0, "flush must reset q_head");
        assert_eq!(chan.q_tail, 0, "flush must reset q_tail");

        // Must NOT touch playback state.
        assert!(chan.playing.is_some(), "flush must NOT clear playing");
        assert!(
            chan.playback_kind.is_some(),
            "flush must NOT clear playback_kind"
        );
        assert_eq!(
            chan.file_completion_addr, 0xABCD_1234,
            "flush must NOT clear file_completion_addr"
        );
        assert!(chan.file_paused, "flush must NOT clear file_paused");
        assert_eq!(
            chan.rate_fixed, 0x0000_8000,
            "flush must NOT reset rate_fixed"
        );
        assert_eq!(
            chan.pending_callback_cmds.len(),
            1,
            "flush must NOT clear pending_callback_cmds"
        );
    }

    /// `SndChannel::quiet` stops active playback without flushing the FIFO.
    /// Specifically quiet clears:
    ///   - `playing` (current playback buffer) → None
    ///   - `playback_kind` → None
    ///   - `pending_callback_cmds` → empty
    ///   - `file_completion_addr` → 0
    ///   - `file_paused` → false
    ///   - `rate_fixed` → UNITY_RATE_FIXED
    ///   - `double_buffer` → None
    /// A regression that makes quiet a no-op (or that makes it only
    /// call flush without the extra state clears) would silently corrupt
    /// channel-reset flows.
    #[test]
    fn quiet_clears_all_playback_state() {
        let mut chan = SndChannel::new(0x1234_0000, true);

        // Simulate an active channel: pending cmd in queue, active playback,
        // file-play state, non-unity rate, and a double-buffer handle.
        chan.enqueue(SndCommand {
            cmd: 81,
            param1: 0,
            param2: 0,
        });
        chan.playing = Some(PlayingBuffer {
            samples: vec![StereoSample::SILENCE; 16],
            sample_rate_fixed: 22050 << 16,
            position: 0,
            step: 0x0001_0000,
        });
        chan.playback_kind = Some(PlaybackKind::File);
        chan.file_completion_addr = 0xABCD_1234;
        chan.file_paused = true;
        chan.rate_fixed = 0x0000_8000; // non-unity
        chan.pending_callback_cmds.push(SndCommand {
            cmd: 11, // CALLBACK
            param1: 0,
            param2: 0,
        });
        // Don't hand-construct DoubleBufferState (fields are private
        // implementation details and can drift). Test via a
        // `.is_some()` marker using `quiet`'s behaviour instead:
        // after quiet, `double_buffer` must end at None regardless
        // of what it was before. Skip the pre-state setup.

        chan.quiet();

        assert_eq!(chan.queue.len(), 1, "quiet must not flush queued commands");
        assert_eq!(chan.q_head, 0);
        assert_eq!(chan.q_tail, 0);
        assert!(chan.playing.is_none(), "quiet must clear playing");
        assert!(
            chan.playback_kind.is_none(),
            "quiet must clear playback_kind"
        );
        assert!(
            chan.pending_callback_cmds.is_empty(),
            "quiet must clear pending_callback_cmds"
        );
        assert_eq!(chan.file_completion_addr, 0);
        assert!(!chan.file_paused, "quiet must clear file_paused");
        assert_eq!(
            chan.rate_fixed, UNITY_RATE_FIXED,
            "quiet must reset rate_fixed to unity"
        );
        assert!(
            chan.double_buffer.is_none(),
            "quiet must clear double_buffer"
        );
    }

    /// `SndChannel::enqueue` returns false when the queue is at
    /// `STD_Q_LENGTH` capacity. Under normal conditions the queue
    /// never fills. If `service_guest_sound_queues` had a regression
    /// that stopped draining the queue, this would silently cause
    /// enqueue rejections.
    #[test]
    fn enqueue_returns_false_when_queue_full() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        // Fill the queue to STD_Q_LENGTH.
        for i in 0..STD_Q_LENGTH {
            assert!(
                chan.enqueue(SndCommand {
                    cmd: 3, // QUIET
                    param1: i as i16,
                    param2: 0,
                }),
                "enqueue at slot {} must succeed while queue has space",
                i
            );
        }
        // Queue is now at capacity — next enqueue must fail.
        assert!(
            !chan.enqueue(SndCommand {
                cmd: 3,
                param1: STD_Q_LENGTH as i16,
                param2: 0,
            }),
            "enqueue must return false when queue is full"
        );
        // And the queue state is unchanged.
        assert_eq!(chan.queue.len(), STD_Q_LENGTH);
    }

    /// Locks in `SoundManager::find_channel_mut`'s contract: returns
    /// `Some(&mut SndChannel)` iff the `guest_ptr` matches an existing
    /// channel, `None` otherwise. Returning the wrong channel (or
    /// `None` when the ptr matches) would silently break
    /// `execute_sound_command` on per-channel cmds (QUIET, FLUSH,
    /// VOLUME, RATE) which rely on `find_channel_mut` to locate the
    /// target.
    #[test]
    fn find_channel_mut_matches_on_guest_ptr() {
        let mut sm = SoundManager::new();
        sm.channels.push(SndChannel::new(0xAAAA_0000, true));
        sm.channels.push(SndChannel::new(0xBBBB_0000, true));

        // Hit: returns Some and matches the requested ptr.
        let found = sm.find_channel_mut(0xBBBB_0000);
        assert!(
            found.is_some(),
            "find_channel_mut must return Some for known ptr"
        );
        assert_eq!(found.unwrap().guest_ptr, 0xBBBB_0000);

        // Miss: unknown ptr returns None.
        assert!(
            sm.find_channel_mut(0xCCCC_0000).is_none(),
            "find_channel_mut must return None for unknown ptr"
        );

        // NIL ptr (zero) also misses by default.
        assert!(
            sm.find_channel_mut(0).is_none(),
            "find_channel_mut must return None for zero ptr"
        );
    }

    /// Locks in `SoundManager::remove_channel`'s contract: returns
    /// `true` iff the `guest_ptr` matched an existing channel, and on
    /// a hit, shrinks the channel list by 1. A `false` return when
    /// the ptr matches (or `true` when it doesn't) would silently
    /// corrupt the channel-list invariant.
    #[test]
    fn remove_channel_returns_true_and_shrinks_on_hit() {
        let mut sm = SoundManager::new();
        sm.channels.push(SndChannel::new(0x1234_0000, true));
        sm.channels.push(SndChannel::new(0x1234_1000, true));
        assert_eq!(sm.channels.len(), 2);

        // Miss: unknown ptr returns false, list unchanged.
        assert!(!sm.remove_channel(0xDEAD_0000));
        assert_eq!(sm.channels.len(), 2);

        // Hit: known ptr returns true, list shrinks.
        assert!(sm.remove_channel(0x1234_0000));
        assert_eq!(sm.channels.len(), 1);
        assert_eq!(sm.channels[0].guest_ptr, 0x1234_1000);

        // Double-remove of the same ptr: second call returns false.
        assert!(!sm.remove_channel(0x1234_0000));
        assert_eq!(sm.channels.len(), 1);
    }

    /// Locks in `mix_frame`'s multi-call playback-continuity contract.
    /// Real games call `mix_frame` repeatedly with small `num_samples`
    /// (host audio callback window, typically 512-4096 samples per
    /// call); a single `snd` resource can span hundreds of calls. The
    /// position must accumulate across calls — a regression that
    /// reset position at frame entry would loop the first slice
    /// forever; one that reset step would repeat the first sample.
    #[test]
    fn mix_frame_continues_playback_position_across_calls() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(
            vec![0x90, 0xA0, 0xB0, 0xC0],
            OUTPUT_RATE << 16,
            PlaybackKind::Buffer,
            0,
        );
        sm.channels.push(chan);

        // Call 1: consumes samples[0..2].
        let out = sm.mix_frame(2);
        assert_eq!(out, vec![0x90, 0xA0], "first mix_frame emits head half");
        assert!(sm.channels[0].is_playing(), "playback continues mid-buffer");

        // Call 2: consumes samples[2..4], buffer exhausts at end.
        let out = sm.mix_frame(2);
        assert_eq!(out, vec![0xB0, 0xC0], "second mix_frame emits tail half");
        assert!(
            !sm.channels[0].is_playing(),
            "playback cleared on final_idx >= samples.len()"
        );

        // Call 3: nothing playing → empty Vec sentinel.
        let out = sm.mix_frame(2);
        assert!(out.is_empty(), "post-exhaust mix_frame returns empty");

        // debug_samples_mixed accumulates across the two active
        // frames (2 + 2 = 4), not the empty third.
        assert_eq!(sm.debug_samples_mixed, 4);
    }

    #[test]
    fn samples_until_next_exhaustion_tracks_resampled_boundary() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(
            vec![0x90, 0xA0],
            (OUTPUT_RATE / 2) << 16,
            PlaybackKind::Buffer,
            0,
        );
        sm.channels.push(chan);

        assert_eq!(
            sm.samples_until_next_exhaustion(),
            Some(4),
            "half-rate two-sample buffer emits four output samples"
        );

        let output = sm.mix_frame(1);
        assert_eq!(output, vec![0x90]);
        assert_eq!(
            sm.samples_until_next_exhaustion(),
            Some(3),
            "boundary query must follow playback position across calls"
        );
    }

    /// Locks in the integration between `mix_frame` and
    /// `apply_volume`. The mixer calls
    ///   `apply_volume(buf.samples[sample_idx], chan.volume)`
    /// before the additive-sum step. A half-volume channel
    /// (0x00800080 packed = 128 left/right, avg=128, half of
    /// FULL_VOLUME=256) should halve the centered sample amplitude
    /// before it hits the output. A regression that inlined the
    /// sample lookup bypassing `apply_volume` would silently break
    /// volumeCmd-driven volume fades per IM:Sound 2-96.
    #[test]
    fn mix_frame_applies_channel_volume_to_sample() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        chan.play_buffer(
            vec![0xA0; 8], // centered amplitude +0x20
            OUTPUT_RATE << 16,
            PlaybackKind::Buffer,
            0,
        );
        // Half volume: packed 0x00800080 (128 left/right → avg 128
        // = FULL_VOLUME/2). 0x20 centered → +0x10 scaled → 0x90.
        chan.set_volume(0x0080_0080);
        sm.channels.push(chan);

        let output = sm.mix_frame(4);

        assert!(
            output.iter().all(|&b| b == 0x90),
            "half-volume must halve centered amplitude (0xA0 → 0x90), got {:02X}",
            output[0]
        );

        // Sanity: at full volume the same buffer produces 0xA0.
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0xA0; 8], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        // Default volume from SndChannel::new is 0x0100_0100 (full).
        sm.channels.push(chan);
        let output = sm.mix_frame(4);
        assert!(
            output.iter().all(|&b| b == 0xA0),
            "full volume passes sample through unchanged, got {:02X}",
            output[0]
        );

        // Zero volume collapses any source to silence (0x80).
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0xA0; 8], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        chan.set_volume(0);
        sm.channels.push(chan);
        let output = sm.mix_frame(4);
        assert!(
            output.iter().all(|&b| b == 0x80),
            "zero volume collapses sample to silence (0x80), got {:02X}",
            output[0]
        );
    }

    #[test]
    fn mix_frame_preserves_audio_when_default_output_volume_changes() {
        let mut sm = SoundManager::new();
        sm.set_default_output_volume(0x0080_0080);
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0xA0; 8], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(chan);

        let output = sm.mix_frame(4);

        assert!(
            output.iter().all(|&b| b == 0xA0),
            "default output volume stores the device default and must not attenuate the current mixed stream, got {:02X}",
            output[0]
        );

        let mut sm = SoundManager::new();
        sm.set_default_output_volume(0);
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0xA0; 8], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(chan);

        let output = sm.mix_frame(4);

        assert!(
            output.iter().all(|&b| b == 0xA0),
            "zero default output volume must not silence active channel audio, got {:02X}",
            output[0]
        );
    }

    /// Mirror of the resampling test for the upsampling direction. When a
    /// buffer's `sample_rate_fixed` is BELOW OUTPUT_RATE (e.g.
    /// 0.5× = 11025 Hz source at 22050 Hz output), the computed
    /// step is 0.5 in 32.32 fixed-point. Sample-and-hold preserves the
    /// original 8-bit sample edges instead of linearly smoothing them; that
    /// matters for classic low-rate sound effects where interpolation sounds
    /// muffled. A regression that truncated the fractional part of `step` to
    /// 0 would freeze playback forever on the first sample; a regression
    /// doubling the step would halve the pitch of every under-sample-rate snd
    /// resource.
    #[test]
    fn mix_frame_resamples_half_rate_with_sample_hold() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        // 2-sample source at 0.5× OUTPUT_RATE → step = 0.5.
        // With 4 output samples, source[0] plays at positions
        // 0.0 and 0.5, source[1] at positions 1.0 and 1.5, and
        // position 2.0 exhausts the buffer (break).
        chan.play_buffer(
            vec![0x90, 0xA0],
            (OUTPUT_RATE / 2) << 16,
            PlaybackKind::Buffer,
            0,
        );
        sm.channels.push(chan);

        let output = sm.mix_frame(6);

        // Expected:
        //   output[0] = source[0] = 0x90
        //   output[1] = source[0] held at fractional position 0.5
        //   output[2] = source[1] = 0xA0
        //   output[3] = source[1] held at tail = 0xA0
        //   output[4] = untouched silence (position 2.0 → idx 2, break)
        //   output[5] = untouched silence
        assert_eq!(output.len(), 6);
        assert_eq!(output[0], 0x90, "source[0] at step 0");
        assert_eq!(
            output[1], 0x90,
            "low-rate upsampling must hold the source sample, not smooth it"
        );
        assert_eq!(output[2], 0xA0, "source[1] at step 2 (position 1.0)");
        assert_eq!(output[3], 0xA0, "tail sample held at step 3 (position 1.5)");
        assert_eq!(output[4], 0x80, "break leaves default silence");
        assert_eq!(output[5], 0x80, "break leaves default silence");

        // Playback cleared on overflow past the 2-sample buffer.
        assert!(!sm.channels[0].is_playing());
    }

    #[test]
    fn mix_frame_stereo_preserves_channel_separation() {
        let stereo_samples = vec![
            StereoSample {
                left: 0x00,
                right: 0xFF,
            },
            StereoSample {
                left: 0x40,
                right: 0xC0,
            },
        ];

        let mut stereo_sm = SoundManager::new();
        let mut stereo_chan = SndChannel::new(0x1234_0000, true);
        stereo_chan.play_stereo_buffer(
            stereo_samples.clone(),
            OUTPUT_RATE << 16,
            PlaybackKind::Buffer,
            0,
        );
        stereo_sm.channels.push(stereo_chan);

        assert_eq!(stereo_sm.mix_frame_stereo(2), vec![0x00, 0xFF, 0x40, 0xC0]);

        let mut mono_sm = SoundManager::new();
        let mut mono_chan = SndChannel::new(0x1234_0000, true);
        mono_chan.play_stereo_buffer(stereo_samples, OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        mono_sm.channels.push(mono_chan);

        assert_eq!(mono_sm.mix_frame(2), vec![0x80, 0x80]);
    }

    #[test]
    fn mix_frame_resamples_classic_rate22khz_with_fractional_interpolation() {
        // EV/EVO 'snd ' resources commonly use Sound Manager's documented
        // rate22khz value: 22,254.54545 Hz. The mixer output contract is
        // 22,050 Hz, so playback advances by just over one source sample
        // per output sample. This must interpolate the fractional position
        // instead of periodically dropping source samples as nearest-neighbor
        // resampling would.
        const RATE_22KHZ_FIXED: u32 = 0x56EE_8BA3;

        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(
            vec![0x80, 0x00, 0xFF, 0x80],
            RATE_22KHZ_FIXED,
            PlaybackKind::Buffer,
            0,
        );
        sm.channels.push(chan);

        let output = sm.mix_frame(3);

        assert_eq!(output[0], 0x80, "position 0.0 reads source[0]");
        assert!(
            (0x01..=0x0F).contains(&output[1]),
            "position just after source[1] should interpolate toward source[2], got {:#04X}",
            output[1]
        );
        assert!(
            output[2] > 0x80,
            "next fractional sample stays on the rising edge"
        );
    }

    /// Locks in `mix_frame`'s resampling step for non-unity source
    /// sample rates. `play_buffer` computes
    ///   step = fixed_div(sample_rate_fixed, OUTPUT_RATE << 16)
    /// in 32.32 fixed-point, so playback advances the source position
    /// by that step per output sample. For a buffer whose
    /// `sample_rate_fixed` is 2× `OUTPUT_RATE`, every other source
    /// sample should be selected because the step lands on integer
    /// source positions. A regression breaking the step calculation
    /// would pitch-shift every non-unity-rate snd resource.
    #[test]
    fn mix_frame_resamples_2x_source_via_step_advance() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        // 4-sample buffer at 2× OUTPUT_RATE → step = 2.0. Two
        // output samples pull source[0] and source[2]; next
        // iteration hits source[4] (out of bounds) and breaks.
        chan.play_buffer(
            vec![0x90, 0xA0, 0xB0, 0xC0],
            (OUTPUT_RATE * 2) << 16,
            PlaybackKind::Buffer,
            0,
        );
        sm.channels.push(chan);

        // Request 3 samples so we can see:
        //   output[0] = source[0] = 0x90
        //   output[1] = source[2] = 0xB0
        //   output[2] = untouched silence 0x80 (break triggered)
        let output = sm.mix_frame(3);

        assert_eq!(output.len(), 3);
        assert_eq!(output[0], 0x90, "source[0] at step 0");
        assert_eq!(output[1], 0xB0, "source[2] at step 1 (2.0 advance)");
        assert_eq!(output[2], 0x80, "break left default silence");

        // Playback exhausted on overflow.
        assert!(
            !sm.channels[0].is_playing(),
            "playback cleared once source position >= samples.len()"
        );
        // samples_mixed counts OUTPUT bytes emitted (all 3,
        // including the silence-by-default slot).
        assert_eq!(sm.debug_samples_mixed, 3);
    }

    /// Extends `apply_volume` coverage to the AMPLIFICATION case
    /// (volume > FULL_VOLUME). Some games boost above unity. The math:
    ///   centered * average / FULL_VOLUME
    /// permits avg > FULL_VOLUME, scaling above unity. The
    /// `clamp(0, 255)` on the result is the safety net that prevents
    /// wraparound when amplified samples exceed [0, 255]. A
    /// regression that integer-overflowed in the multiply or
    /// truncated the avg to FULL_VOLUME would silently break boosted-
    /// volume playback.
    #[test]
    fn apply_volume_amplifies_above_full_volume_and_clamps() {
        // 2× FULL_VOLUME: L=R=0x200 → avg=0x200.
        let two_x = ((FULL_VOLUME as u32 * 2) << 16) | (FULL_VOLUME as u32 * 2);

        // sample=0xA0 (+0x20 centered) → 0x20 × 0x200 / 0x100 = 0x40
        // → result = 0x40 + 0x80 = 0xC0.
        assert_eq!(
            apply_volume(0xA0, two_x),
            0xC0,
            "+0x20 doubled = +0x40 → 0xC0"
        );

        // sample=0x60 (-0x20 centered) → -0x20 × 0x200 / 0x100 = -0x40
        // → result = -0x40 + 0x80 = 0x40.
        assert_eq!(
            apply_volume(0x60, two_x),
            0x40,
            "-0x20 doubled = -0x40 → 0x40"
        );

        // sample=0xFF (+0x7F centered) → 0x7F × 2 = 0xFE → +0x80 = 0x17E
        // → clamps to 0xFF (upper saturation).
        assert_eq!(
            apply_volume(0xFF, two_x),
            0xFF,
            "+0x7F doubled saturates at 0xFF"
        );

        // sample=0x00 (-0x80 centered) → -0x80 × 2 = -0x100 → +0x80 = -0x80
        // → clamps to 0x00 (lower saturation).
        assert_eq!(
            apply_volume(0x00, two_x),
            0x00,
            "-0x80 doubled saturates at 0x00"
        );

        // sample=0x80 (silence) at any volume → still silence.
        assert_eq!(
            apply_volume(0x80, two_x),
            0x80,
            "silence is silence regardless of gain"
        );
    }

    /// Locks in the interaction between `pause_file_playback_toggle`
    /// and `quiet`. `quiet()` must clear `file_paused` along with all
    /// other playback state. A regression that omitted the
    /// `file_paused`-clear would leave the channel "paused" even
    /// after quiet, causing `mix_frame` to keep producing silence
    /// indefinitely.
    #[test]
    fn quiet_clears_file_paused_after_pause_toggle() {
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0x80; 16], OUTPUT_RATE << 16, PlaybackKind::File, 0);
        // Toggle paused on.
        chan.pause_file_playback_toggle();
        assert!(
            chan.has_active_playback(),
            "playing OR file_paused → active"
        );

        // quiet must wipe everything, including file_paused.
        chan.quiet();
        assert!(!chan.is_playing(), "quiet clears playing");
        assert!(
            !chan.has_active_playback(),
            "quiet must clear file_paused too — has_active_playback = playing OR file_paused"
        );
    }

    /// Locks in `mix_frame`'s waiting-for-refill silence contract.
    /// When a channel has `playing=None` but `double_buffer=Some`,
    /// `mix_frame` must set `any_active=true` without mixing anything,
    /// so the returned output is `num_samples` of silence (0x80)
    /// rather than empty. This is the "buffer exhausted, waiting for
    /// callback to refill" steady-state that occurs EVERY `mix_frame`
    /// between a double-buffer exhaustion and the guest's doubleback
    /// proc firing. Without this contract, the output stream would
    /// briefly go empty (underrunning the host audio callback) every
    /// time a DB buffer runs out. Matches IM:Sound 2-111 seamless-
    /// double-buffer semantics.
    #[test]
    fn mix_frame_channel_with_db_but_no_playing_outputs_silence() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        // playing stays None; attach DB in waiting state.
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr: 0x0070_0000,
            current_buffer: 1,
            callback_addr: 0xCAFE_0000,
            chan_ptr: 0x1234_0000,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [false, true],
        });
        assert!(!chan.is_playing(), "playing stays None pre-mix");
        sm.channels.push(chan);

        let output = sm.mix_frame(32);

        assert_eq!(
            output.len(),
            32,
            "DB-waiting channel must still produce num_samples (non-empty)"
        );
        assert!(
            output.iter().all(|&b| b == 0x80),
            "no playing buffer → output is pure silence (0x80), got {:02X}",
            output[0]
        );
        // Channel state unchanged: DB still present, no callback
        // re-triggered (waiting_for_callback still true).
        let db = sm.channels[0]
            .double_buffer
            .as_ref()
            .expect("double_buffer must remain installed");
        assert!(
            db.waiting_for_callback,
            "waiting_for_callback must stay true across idle mix_frame"
        );
        assert_eq!(db.current_buffer, 1, "current_buffer must not flip on idle");
        assert!(
            sm.pending_callbacks.is_empty(),
            "no new callback pushed on idle-wait frame"
        );
    }

    #[test]
    fn mix_frame_idle_double_buffer_requests_refill_once() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr: 0x0070_0000,
            current_buffer: 1,
            callback_addr: 0xCAFE_0000,
            chan_ptr: 0x1234_0000,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        sm.channels.push(chan);

        let output = sm.mix_frame(32);

        assert_eq!(
            output.len(),
            32,
            "idle DB channel stays active while requesting a refill"
        );
        assert!(
            output.iter().all(|&b| b == 0x80),
            "no ready buffer means the active output is silence"
        );
        assert_eq!(sm.pending_callbacks.len(), 1);
        let callback = &sm.pending_callbacks[0];
        assert_eq!(callback.callback_addr, 0xCAFE_0000);
        assert_eq!(callback.chan_ptr, 0x1234_0000);
        assert_eq!(callback.header_ptr, 0x0070_0000);
        assert_eq!(
            callback.exhausted_buffer_index, 1,
            "retry asks the guest to refill the current missing buffer"
        );
        let db = sm.channels[0].double_buffer.as_ref().unwrap();
        assert!(db.waiting_for_callback);

        sm.mix_frame(32);
        assert_eq!(
            sm.pending_callbacks.len(),
            1,
            "waiting_for_callback prevents refill callback spam"
        );
    }

    /// Locks in `mix_frame`'s multi-channel additive-mix contract.
    /// The mixer loops over each channel summing its volume-scaled
    /// sample into the output via:
    ///   mixed = output[i] + sample - 0x80; output[i] = clamp(mixed, 0, 255)
    /// A regression that dropped the `- 0x80` offset would double the
    /// silence baseline, and a regression that removed `.clamp(0, 255)`
    /// would wrap u8 on overflow, producing audible glitches.
    #[test]
    fn mix_frame_two_active_channels_sum_arithmetically_and_clamp() {
        // Pass-through test: A=0x90 (+0x10), B=0xA0 (+0x20)
        // → output[i] = 0x80 + 0x10 + 0x20 = 0xB0.
        let mut sm = SoundManager::new();
        let mut a = SndChannel::new(0x1000_0000, true);
        let mut b = SndChannel::new(0x2000_0000, true);
        a.play_buffer(vec![0x90; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        b.play_buffer(vec![0xA0; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(a);
        sm.channels.push(b);

        let output = sm.mix_frame(16);

        assert_eq!(output.len(), 16, "active-channel mix produces num_samples");
        assert!(
            output.iter().all(|&b| b == 0xB0),
            "two-channel sum: 0x80 + (0x90-0x80) + (0xA0-0x80) = 0xB0, got {:02X}",
            output[0]
        );
        // Both channels contributed to samples_mixed (one count,
        // not two — samples_mixed tracks output byte count).
        assert_eq!(sm.debug_samples_mixed, 16);

        // Positive-clip case: two channels at 0xFF clamp to 0xFF.
        let mut sm = SoundManager::new();
        let mut a = SndChannel::new(0x1000_0000, true);
        let mut b = SndChannel::new(0x2000_0000, true);
        a.play_buffer(vec![0xFF; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        b.play_buffer(vec![0xFF; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(a);
        sm.channels.push(b);
        let output = sm.mix_frame(4);
        assert!(
            output.iter().all(|&v| v == 0xFF),
            "0xFF + 0xFF saturates to 0xFF (upper clamp), got {:02X}",
            output[0]
        );

        // Negative-clip case: two channels at 0x00 clamp to 0x00.
        let mut sm = SoundManager::new();
        let mut a = SndChannel::new(0x1000_0000, true);
        let mut b = SndChannel::new(0x2000_0000, true);
        a.play_buffer(vec![0x00; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        b.play_buffer(vec![0x00; 32], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        sm.channels.push(a);
        sm.channels.push(b);
        let output = sm.mix_frame(4);
        assert!(
            output.iter().all(|&v| v == 0x00),
            "0x00 + 0x00 saturates to 0x00 (lower clamp), got {:02X}",
            output[0]
        );
    }

    /// Locks in the guard flags that inhibit duplicate double-buffer
    /// callback queueing in `mix_frame`:
    ///   - `last_buffer_seen=true`: the guest already told us via
    ///     `dbLastBuffer` that no more data will come; don't ask
    ///     for a refill we'll never get.
    ///   - `pending_callback_buffers[n]=true`: we already asked for
    ///     that specific slot to be refilled; don't queue a duplicate
    ///     for the same slot.
    ///
    /// A regression removing either guard would cause callback spam or
    /// a callback request after the guest marked the stream complete.
    #[test]
    fn mix_frame_double_buffer_guards_inhibit_callback_push() {
        // Case A: last_buffer_seen=true → no callback.
        {
            let mut sm = SoundManager::new();
            let mut chan = SndChannel::new(0x1234_0000, true);
            chan.play_buffer(vec![0x80, 0x80], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
            chan.double_buffer = Some(DoubleBufferState {
                header_ptr: 0x0070_0000,
                current_buffer: 0,
                callback_addr: 0xCAFE_0000,
                chan_ptr: 0x1234_0000,
                sample_rate: OUTPUT_RATE << 16,
                num_channels: 1,
                sample_size: 8,
                last_buffer_seen: true,
                waiting_for_callback: false,
                pending_callback_buffers: [false; 2],
            });
            sm.channels.push(chan);

            sm.mix_frame(4);

            assert!(
                sm.pending_callbacks.is_empty(),
                "last_buffer_seen=true must inhibit callback push"
            );
            // current_buffer must NOT flip since we skipped the
            // whole guarded block.
            assert_eq!(
                sm.channels[0]
                    .double_buffer
                    .as_ref()
                    .unwrap()
                    .current_buffer,
                0,
                "current_buffer must NOT flip when last_buffer_seen=true"
            );
        }

        // Case B: this same buffer already has a pending callback → no duplicate.
        {
            let mut sm = SoundManager::new();
            let mut chan = SndChannel::new(0x1234_0000, true);
            chan.play_buffer(vec![0x80, 0x80], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
            chan.double_buffer = Some(DoubleBufferState {
                header_ptr: 0x0070_0000,
                current_buffer: 0,
                callback_addr: 0xCAFE_0000,
                chan_ptr: 0x1234_0000,
                sample_rate: OUTPUT_RATE << 16,
                num_channels: 1,
                sample_size: 8,
                last_buffer_seen: false,
                waiting_for_callback: true,
                pending_callback_buffers: [true, false],
            });
            sm.channels.push(chan);

            sm.mix_frame(4);

            assert!(
                sm.pending_callbacks.is_empty(),
                "pending_callback_buffers[current]=true must inhibit duplicate callback push"
            );
            assert_eq!(
                sm.channels[0]
                    .double_buffer
                    .as_ref()
                    .unwrap()
                    .current_buffer,
                1,
                "current_buffer still advances to the paired slot"
            );
        }
    }

    #[test]
    fn mix_frame_allows_other_double_buffer_callback_while_one_slot_is_pending() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.play_buffer(vec![0x80, 0x80], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr: 0x0070_0000,
            current_buffer: 1,
            callback_addr: 0xCAFE_0000,
            chan_ptr: 0x1234_0000,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [true, false],
        });
        sm.channels.push(chan);

        sm.mix_frame(4);

        assert_eq!(
            sm.pending_callbacks.len(),
            1,
            "pending refill for buffer 0 must not suppress buffer 1's doubleback"
        );
        assert_eq!(sm.pending_callbacks[0].exhausted_buffer_index, 1);
        let db = sm.channels[0].double_buffer.as_ref().unwrap();
        assert_eq!(db.current_buffer, 0);
        assert_eq!(
            db.pending_callback_buffers,
            [true, true],
            "both slots can have outstanding refills independently"
        );
        assert!(db.waiting_for_callback);
    }

    /// Locks in `mix_frame`'s double-buffer exhaustion contract. When
    /// a channel with an active `double_buffer`
    /// (`last_buffer_seen=false`, `waiting_for_callback=false`)
    /// exhausts its current playback, `mix_frame` must:
    ///   - flip `current_buffer` to the other slot (0↔1)
    ///   - set `waiting_for_callback = true` (so the exhausted slot
    ///     isn't re-triggered on the next frame before the refill
    ///     callback has had a chance to run)
    ///   - push a `PendingDoubleBackCallback` carrying
    ///     `callback_addr`, `chan_ptr`, `header_ptr`, and the
    ///     *just-exhausted* buffer index (not the newly-flipped one)
    ///     to `pending_callbacks`
    ///
    /// Matches IM:Sound 2-111..113: the doubleback proc receives the
    /// `DbhBufferPtr` for the buffer it should now refill (the one
    /// that just finished).
    #[test]
    fn mix_frame_double_buffer_exhaust_queues_callback_and_flips_slot() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        // Install a short Buffer playback so it exhausts in 2
        // samples; attach double_buffer state around it.
        chan.play_buffer(vec![0x80, 0x80], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr: 0x0070_0000,
            current_buffer: 0,
            callback_addr: 0xCAFE_0000,
            chan_ptr: 0x1234_0000,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        sm.channels.push(chan);

        sm.mix_frame(4); // overshoots — buffer exhausts

        // Exactly one PendingDoubleBackCallback pushed.
        assert_eq!(
            sm.pending_callbacks.len(),
            1,
            "one double-back callback queued"
        );
        let p = &sm.pending_callbacks[0];
        assert_eq!(p.callback_addr, 0xCAFE_0000);
        assert_eq!(p.chan_ptr, 0x1234_0000);
        assert_eq!(p.header_ptr, 0x0070_0000);
        assert_eq!(
            p.exhausted_buffer_index, 0,
            "exhausted index is the OLD current_buffer, not the flipped one"
        );

        // Channel's double_buffer state flipped and armed.
        let db = sm.channels[0]
            .double_buffer
            .as_ref()
            .expect("db still present");
        assert_eq!(db.current_buffer, 1, "current_buffer flipped 0 → 1");
        assert!(
            db.waiting_for_callback,
            "waiting_for_callback armed so next frame doesn't re-trigger"
        );
        assert_eq!(db.pending_callback_buffers, [true, false]);
    }

    /// Locks in `mix_frame`'s file-playback completion contract. When
    /// a channel with `playback_kind == File` and a non-zero
    /// `file_completion_addr` exhausts, `mix_frame` must push one
    /// `PendingSoundCallback::FileCompletion` (carrying the
    /// `file_completion_addr` as `callback_addr` and the channel
    /// `guest_ptr`) to `pending_sound_callbacks`, AND clear
    /// `file_completion_addr` on the channel. Per IM:Sound 2-151,
    /// `MyFilePlayCompletionRoutine(chan: SndChannelPtr)` is the
    /// signature the trap layer dispatches.
    #[test]
    fn mix_frame_file_playback_exhaust_queues_file_completion_callback() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        chan.play_buffer(
            vec![0x80; 2],
            OUTPUT_RATE << 16,
            PlaybackKind::File,
            0xABCD_1234, // file_completion_addr
        );
        assert_eq!(chan.file_completion_addr, 0xABCD_1234);
        sm.channels.push(chan);

        sm.mix_frame(4); // overshoots

        // Exactly one FileCompletion queued, no Command variants.
        assert_eq!(sm.pending_sound_callbacks.len(), 1);
        match &sm.pending_sound_callbacks[0] {
            PendingSoundCallback::FileCompletion {
                callback_addr,
                chan_ptr,
            } => {
                assert_eq!(
                    *callback_addr, 0xABCD_1234,
                    "file_completion_addr propagates"
                );
                assert_eq!(*chan_ptr, 0x1234_0000, "chan guest_ptr propagates");
            }
            other => panic!("expected FileCompletion, got {:?}", other),
        }
        // Channel's file_completion_addr cleared so we don't
        // double-fire next frame.
        assert_eq!(
            sm.channels[0].file_completion_addr, 0,
            "file_completion_addr must be cleared after push"
        );
        // Playback state cleared.
        assert!(!sm.channels[0].is_playing());
        assert!(!sm.channels[0].has_active_playback());
    }

    /// Locks in `mix_frame`'s buffer-exhaustion-callback contract.
    /// When a channel with a non-zero `callback_addr` and queued
    /// `pending_callback_cmds` finishes playback (position past end
    /// of samples), `mix_frame` must:
    ///   - drain `pending_callback_cmds` via `take_pending_callback_cmds`
    ///   - push one `PendingSoundCallback::Command` per drained cmd
    ///     to `SoundManager::pending_sound_callbacks`
    ///   - clear `chan.playing` / `chan.playback_kind`
    ///
    /// The trap layer then fires each queued guest callback per
    /// IM:Sound 2-152 (`callBackCmd` / `MyCallbackProcedure`).
    #[test]
    fn mix_frame_buffer_exhaust_queues_pending_sound_callback_per_cmd() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        // 2-sample playback at unity rate so 2 mix_frame samples
        // exhaust it fully.
        chan.play_buffer(vec![0x80, 0x80], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        chan.callback_addr = 0xBEEF_0000;
        chan.queue_callback(SndCommand {
            cmd: cmd::CALLBACK,
            param1: 7,
            param2: 0x1111,
        });
        chan.queue_callback(SndCommand {
            cmd: cmd::CALLBACK,
            param1: 9,
            param2: 0x2222,
        });
        sm.channels.push(chan);

        sm.mix_frame(4); // overshoots; buffer exhausts

        // Playback cleared.
        assert!(
            !sm.channels[0].is_playing(),
            "playback cleared on exhaustion"
        );
        // Both queued callback cmds pushed to pending list.
        assert_eq!(
            sm.pending_sound_callbacks.len(),
            2,
            "one PendingSoundCallback::Command per queued callback cmd"
        );
        for (i, pending) in sm.pending_sound_callbacks.iter().enumerate() {
            match pending {
                PendingSoundCallback::Command {
                    callback_addr,
                    chan_ptr,
                    cmd,
                } => {
                    assert_eq!(*callback_addr, 0xBEEF_0000, "callback_addr propagates");
                    assert_eq!(*chan_ptr, 0x1234_0000, "chan_ptr propagates");
                    let expected_param1 = if i == 0 { 7 } else { 9 };
                    assert_eq!(cmd.param1, expected_param1, "cmd ordering preserved");
                }
                _ => panic!("expected Command variant, got {:?}", pending),
            }
        }
        // pending_callback_cmds drained from the channel.
        assert!(
            sm.channels[0].take_pending_callback_cmds().is_empty(),
            "pending_callback_cmds drained on exhaustion"
        );
    }

    /// Locks in `mix_frame`'s file-paused-channel contract. When a
    /// channel has `file_paused=true` (via
    /// `pause_file_playback_toggle`), `mix_frame` must skip mixing
    /// it BUT still treat the manager as `any_active=true`, so the
    /// returned `Vec` is `num_samples` of silence (0x80) rather than
    /// the empty-`Vec` "no channels active" sentinel. This mirrors
    /// Mac Sound Manager semantics per IM:Sound 2-139: a paused
    /// file-playback channel holds its slot in the output stream;
    /// output doesn't vanish from the user's perspective.
    #[test]
    fn mix_frame_paused_file_channel_outputs_silence_not_empty() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);

        // Install a File-kind playback, then toggle paused on.
        chan.play_buffer(vec![0x80; 128], OUTPUT_RATE << 16, PlaybackKind::File, 0);
        chan.pause_file_playback_toggle();
        assert!(chan.file_paused, "file_paused must be set after toggle");
        sm.channels.push(chan);

        let output = sm.mix_frame(64);

        assert_eq!(
            output.len(),
            64,
            "paused file channel must still produce num_samples of output"
        );
        assert!(
            output.iter().all(|&b| b == 0x80),
            "paused file channel output must be pure silence (0x80)"
        );
        // Paused channel contributes silence; debug_samples_mixed
        // still tracks that samples flowed through the mixer.
        assert_eq!(
            sm.debug_samples_mixed, 64,
            "debug_samples_mixed tracks samples even for paused channels"
        );
    }

    /// Locks in `mix_frame`'s FLUSH-ordering semantics. When a FLUSH
    /// command is queued BEFORE other commands, running `mix_frame`
    /// must drain FLUSH (which calls `chan.flush()` clearing the
    /// queue) and any subsequent queued commands are discarded rather
    /// than executed. A regression that reordered the match arms
    /// (e.g. processed all cmds first, then flush-after) would
    /// silently corrupt the Sound Manager's FIFO-drop-after-flush
    /// contract per IM:Sound 2-93 (`flushCmd`).
    #[test]
    fn mix_frame_flush_cmd_discards_subsequent_queued_cmds() {
        let mut sm = SoundManager::new();
        let mut chan = SndChannel::new(0x1234_0000, true);
        chan.callback_addr = 0x00AB_CDEF;

        // Queue [FLUSH, CALLBACK]. If CALLBACK runs, it will post a
        // pending sound callback. FLUSH must discard it instead.
        chan.enqueue(SndCommand {
            cmd: cmd::FLUSH,
            param1: 0,
            param2: 0,
        });
        chan.enqueue(SndCommand {
            cmd: cmd::CALLBACK,
            param1: 7,
            param2: 0x1111_2222,
        });
        assert_eq!(chan.queue.len(), 2, "two cmds queued pre mix_frame");
        sm.channels.push(chan);

        let output = sm.mix_frame(64);

        assert!(output.is_empty(), "idle command drain produces no audio");
        assert!(
            sm.channels[0].queue.is_empty(),
            "queue must be empty after mix_frame"
        );
        assert!(
            sm.pending_sound_callbacks.is_empty(),
            "callback after FLUSH must be discarded, not executed"
        );
    }

    /// Locks in `SndChannel::new`'s observable initial-state contract.
    /// A refactor that flipped `allocated` semantics, changed the
    /// default rate off unity, or pre-populated callbacks would
    /// silently break the Sound Manager contract per IM:Sound 2-80
    /// (`SndNewChannel` initial state) and IM:Sound 2-97 (unity rate
    /// default).
    #[test]
    fn sndchannel_new_initial_state_matches_mac_defaults() {
        let mut chan = SndChannel::new(0x1234_0000, true);

        // Constructor args pass through unchanged.
        assert_eq!(
            chan.guest_ptr, 0x1234_0000,
            "guest_ptr must match constructor arg"
        );
        assert!(chan.allocated, "allocated=true must propagate");

        // Fields that start zero / None per IM:Sound 2-80.
        assert_eq!(
            chan.callback_addr, 0,
            "callback_addr starts 0 (no userRoutine yet)"
        );
        assert!(
            chan.double_buffer.is_none(),
            "double_buffer starts None (not in SndPlayDoubleBuffer)"
        );

        // Playback accessors report no activity on a fresh channel.
        assert!(!chan.is_playing(), "fresh channel reports is_playing=false");
        assert!(
            !chan.has_active_playback(),
            "fresh channel reports has_active_playback=false"
        );

        // Rate defaults to unity — a channel playing a buffer at the
        // buffer's sample_rate with no explicit rateCmd must play at
        // that rate (IM:Sound 2-97).
        assert_eq!(
            chan.current_rate(),
            0x0001_0000,
            "rate_fixed must default to UNITY_RATE_FIXED (0x0001_0000)"
        );

        // No pending callbacks queued.
        assert!(
            chan.take_pending_callback_cmds().is_empty(),
            "pending_callback_cmds must start empty"
        );

        // `allocated=false` path (game-provided channel record).
        let guest_alloc = SndChannel::new(0xDEAD_0000, false);
        assert_eq!(guest_alloc.guest_ptr, 0xDEAD_0000);
        assert!(!guest_alloc.allocated, "allocated=false must propagate");
    }
}
