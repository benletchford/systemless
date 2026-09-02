//! PowerPC Sound Manager, Timer, and VBL task state and records.

use super::imports::PpcHleImportTraceEntry;
use crate::process_context::SharedProcessSoundManager;
pub use crate::sound::{
    PendingProcessSoundDoubleBack as PpcSoundDoubleBackRecord,
    ProcessSoundDoubleBufferPlayback as PpcSoundDoubleBufferPlaybackRecord,
};
use ppc::{PpcFetchHistogram, PpcRunResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcSndCommandRecord {
    pub channel: u32,
    pub command: u16,
    pub param1: i16,
    pub param2: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcAiffMetadata {
    pub form_type: u32,
    pub channel_count: u16,
    pub sample_frame_count: u32,
    pub sample_size: u16,
    pub sample_rate_hz: u32,
    pub compression_type: u32,
    pub sound_data_offset: u32,
    pub sound_data_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcDecodedAiffSamples {
    pub sample_rate_fixed: u32,
    pub sample_count: u32,
    pub preview_len: u8,
    pub preview: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcDecodedAiffPlaybackRecord {
    pub file_playback_index: u32,
    pub channel: u32,
    pub sample_rate_fixed: u32,
    pub samples: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcSoundFilePlaybackRecord {
    pub channel: u32,
    pub ref_num: i16,
    pub resource_id: i16,
    pub buffer_size: u32,
    pub buffer: u32,
    pub selection: u32,
    pub completion: u32,
    pub completion_command: Option<PpcSndCommandRecord>,
    pub async_play: bool,
    pub paused: bool,
    pub active: bool,
    pub quiet_now: bool,
    pub aiff: Option<PpcAiffMetadata>,
    pub decoded_aiff: Option<PpcDecodedAiffSamples>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcDecodedBufferCommandRecord {
    pub channel: u32,
    pub sample_rate_fixed: u32,
    pub samples: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcSoundCompletionRecord {
    pub file_playback_index: u32,
    pub channel: u32,
    pub completion: u32,
    pub command: Option<PpcSndCommandRecord>,
    pub tick: u32,
    pub instruction_count: u64,
    pub scheduled_tick: u32,
    pub scheduled_instruction_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcSoundCompletionInvocationRecord {
    pub file_playback_index: u32,
    pub channel: u32,
    pub completion: u32,
    pub callback_entry: u32,
    pub callback_rtoc: u32,
    pub tick: u32,
    pub instruction_count: u64,
    pub scheduled_tick: u32,
    pub scheduled_instruction_count: u64,
    pub cycles: u64,
    pub end_pc: u32,
    pub end_sp: u32,
    pub end_r3: u32,
    pub result: PpcRunResult,
    pub unsupported_import_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcSoundCompletionCallProbe {
    pub invocation: PpcSoundCompletionInvocationRecord,
    pub import_trace: Vec<PpcHleImportTraceEntry>,
    pub fetch_histogram: Option<PpcFetchHistogram>,
}

#[derive(Debug, Clone, Default)]
pub struct PpcSoundState {
    pub(crate) manager: SharedProcessSoundManager,
    pub queued_commands: Vec<PpcSndCommandRecord>,
    pub immediate_commands: Vec<PpcSndCommandRecord>,
    pub file_playbacks: Vec<PpcSoundFilePlaybackRecord>,
    pub decoded_file_playbacks: Vec<PpcDecodedAiffPlaybackRecord>,
    pub completion_invocations: Vec<PpcSoundCompletionInvocationRecord>,
    pub sys_beep_count: u32,
    pub last_sys_beep_duration: i16,
    pub start_count: u32,
    pub pause_count: u32,
    pub stop_count: u32,
    pub double_buffer_play_count: u32,
    pub last_double_buffer_channel: u32,
    pub last_double_buffer_header: u32,
}

impl PartialEq for PpcSoundState {
    fn eq(&self, other: &Self) -> bool {
        self.queued_commands == other.queued_commands
            && self.immediate_commands == other.immediate_commands
            && self.file_playbacks == other.file_playbacks
            && self.decoded_file_playbacks == other.decoded_file_playbacks
            && self.completion_invocations == other.completion_invocations
            && self.sys_beep_count == other.sys_beep_count
            && self.last_sys_beep_duration == other.last_sys_beep_duration
            && self.start_count == other.start_count
            && self.pause_count == other.pause_count
            && self.stop_count == other.stop_count
            && self.double_buffer_play_count == other.double_buffer_play_count
            && self.last_double_buffer_channel == other.last_double_buffer_channel
            && self.last_double_buffer_header == other.last_double_buffer_header
    }
}

impl Eq for PpcSoundState {}

pub use crate::callback_manager::{
    ProcessTimerTask as PpcTimerTaskRecord, ProcessVblTask as PpcVblTaskRecord,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcTimerCallbackInvocationRecord {
    pub task_ptr: u32,
    pub callback: u32,
    pub callback_entry: u32,
    pub callback_rtoc: u32,
    pub tick: u32,
    pub cycles: u64,
    pub end_pc: u32,
    pub end_sp: u32,
    pub end_r3: u32,
    pub result: PpcRunResult,
    pub unsupported_import_index: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PpcTimerCallbackProbe {
    pub invocation: PpcTimerCallbackInvocationRecord,
    pub import_trace: Vec<PpcHleImportTraceEntry>,
    pub fetch_histogram: Option<PpcFetchHistogram>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcVblCallbackInvocationRecord {
    pub task_ptr: u32,
    pub callback: u32,
    pub callback_entry: u32,
    pub callback_rtoc: u32,
    pub tick: u32,
    pub cycles: u64,
    pub end_pc: u32,
    pub end_sp: u32,
    pub end_r3: u32,
    pub result: PpcRunResult,
    pub unsupported_import_index: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PpcVblCallbackProbe {
    pub invocation: PpcVblCallbackInvocationRecord,
    pub import_trace: Vec<PpcHleImportTraceEntry>,
    pub fetch_histogram: Option<PpcFetchHistogram>,
}
