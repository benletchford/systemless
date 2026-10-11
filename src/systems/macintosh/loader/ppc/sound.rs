//! PowerPC Sound Manager, Timer, and VBL task state and records.

use super::{
    imports::PpcHleImportTraceEntry, ppc_handle_bytes, ppc_i16_result,
    ppc_memory_can_write_bytes, ppc_memory_read_bytes, ppc_process_heap_alloc,
    ppc_sound_trace_enabled, ppc_vfs_resource_index, PpcCpu, PpcFileRecord, PpcHandleRecord,
    PpcImportAction, PpcSectionMem, PpcVfsFileRecord, PpcVfsResourceRecord, PPC_BAD_FORMAT,
    PPC_FNF_ERR, PPC_MEM_FULL_ERR, PPC_NOT_ENOUGH_HARDWARE_ERR, PPC_NO_ERR, PPC_PARAM_ERR,
    PPC_RES_PROBLEM, PPC_RF_NUM_ERR,
};
use crate::callback_manager::CallbackTaskArchitecture;
use crate::process_context::{ProcessNativeMemoryManager, SharedProcessSoundManager};
use ppc::PpcMemory;
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

// --- Sound Constants ---

pub const PPC_SOUND_MANAGER_VERSION: u32 = 0x0333_8000;
pub const PPC_DEFAULT_OUTPUT_VOLUME: u32 = 0x0001_0000;
pub const PPC_GUEST_SND_CHANNEL_SIZE: u32 = 1088;
pub const PPC_SQUARE_WAVE_SYNTH_ID: i16 = 1;
pub const PPC_WAVE_TABLE_SYNTH_ID: i16 = 3;
pub const PPC_SAMPLED_SYNTH_ID: i16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSoundInputCompatibilityOperation {
    CloseDevice,
    GetDeviceInfo,
    OpenDevice,
    Record,
    SetDeviceInfo,
    StopRecording,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSpeechCompatibilityOperation {
    CountVoices,
    DisposeSpeechChannel,
    GetIndVoice,
    GetVoiceDescription,
    NewSpeechChannel,
    SpeakString,
    SpeakText,
    SpeechBusy,
}

// --- Compatibility Dispatch ---

pub fn ppc_dispatch_sound_input_compatibility(
    operation: PpcSoundInputCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    if operation == PpcSoundInputCompatibilityOperation::OpenDevice && cpu.gpr[5] != 0 {
        let _ = memory.write_u32_be(cpu.gpr[5], 0);
    }
    PpcImportAction::Return(ppc_i16_result(PPC_NOT_ENOUGH_HARDWARE_ERR))
}

pub fn ppc_dispatch_speech_compatibility(
    operation: PpcSpeechCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    match operation {
        PpcSpeechCompatibilityOperation::CountVoices => {
            let result = if memory.write_u16_be(cpu.gpr[3], 0).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSpeechCompatibilityOperation::SpeechBusy => PpcImportAction::Return(0),
        PpcSpeechCompatibilityOperation::DisposeSpeechChannel => {
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
        PpcSpeechCompatibilityOperation::NewSpeechChannel => {
            if cpu.gpr[4] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[4], 0);
            }
            PpcImportAction::Return(ppc_i16_result(PPC_NOT_ENOUGH_HARDWARE_ERR))
        }
        PpcSpeechCompatibilityOperation::GetIndVoice
        | PpcSpeechCompatibilityOperation::GetVoiceDescription
        | PpcSpeechCompatibilityOperation::SpeakString
        | PpcSpeechCompatibilityOperation::SpeakText => {
            PpcImportAction::Return(ppc_i16_result(PPC_NOT_ENOUGH_HARDWARE_ERR))
        }
    }
}

// --- Sound Manager Implementation ---

pub(crate) fn ppc_snd_new_channel(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    sound: &mut PpcSoundState,
) -> i16 {
    let channel_out_ptr = cpu.gpr[3];
    if channel_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, channel_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let synth = cpu.gpr[4] as i16;
    if !matches!(
        synth,
        0 | PPC_SQUARE_WAVE_SYNTH_ID | PPC_WAVE_TABLE_SYNTH_ID | PPC_SAMPLED_SYNTH_ID
    ) {
        return PPC_RES_PROBLEM;
    }
    let user_routine = cpu.gpr[6];
    let existing_channel = memory.read_u32_be(channel_out_ptr).unwrap_or(0);
    let allocated = existing_channel == 0;
    let (channel, preserved_user_info, q_length) = if existing_channel != 0 {
        if !ppc_memory_can_write_bytes(memory, existing_channel, 36) {
            return PPC_PARAM_ERR;
        }
        let Some(preserved_user_info) = memory.read_u32_be(existing_channel + 12) else {
            return PPC_PARAM_ERR;
        };
        let Some(q_length) = memory.read_u16_be(existing_channel + 30) else {
            return PPC_PARAM_ERR;
        };
        (existing_channel, preserved_user_info, q_length.max(1))
    } else {
        let channel = ppc_process_heap_alloc(
            process_memory_manager,
            memory,
            heap_cursor,
            PPC_GUEST_SND_CHANNEL_SIZE,
            true,
        );
        if channel == 0 {
            *last_mem_error = PPC_MEM_FULL_ERR;
            return PPC_MEM_FULL_ERR;
        }
        (channel, 0, 128)
    };

    let writes_ok = memory.write_u32_be(channel, 0).is_some()
        && memory.write_u32_be(channel + 4, 0).is_some()
        && memory.write_u32_be(channel + 8, user_routine).is_some()
        && memory
            .write_u32_be(channel + 12, preserved_user_info)
            .is_some()
        && memory.write_u32_be(channel + 16, 0).is_some()
        && memory.write_u32_be(channel + 20, 0).is_some()
        && memory.write_u32_be(channel + 24, 0).is_some()
        && memory.write_u16_be(channel + 28, 0).is_some()
        && memory.write_u16_be(channel + 30, q_length).is_some()
        && memory.write_u16_be(channel + 32, 0).is_some()
        && memory.write_u16_be(channel + 34, 0).is_some()
        && memory.write_u32_be(channel_out_ptr, channel).is_some();
    if !writes_ok {
        return PPC_PARAM_ERR;
    }
    *last_mem_error = PPC_NO_ERR;
    sound.manager.register_channel(
        channel,
        allocated,
        user_routine,
        CallbackTaskArchitecture::PowerPc,
    );
    if ppc_sound_trace_enabled() {
        eprintln!(
            "[PPC-SOUND] SndNewChannel chan=${channel:08X} synth={synth} init=${:08X} user_routine=${user_routine:08X}",
            cpu.gpr[5]
        );
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_snd_channel_status(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    sound: &PpcSoundState,
) -> i16 {
    let channel = cpu.gpr[3];
    let byte_count = cpu.gpr[4];
    let status_ptr = cpu.gpr[5];
    if byte_count > 0
        && (status_ptr == 0 || !ppc_memory_can_write_bytes(memory, status_ptr, byte_count))
    {
        return PPC_PARAM_ERR;
    }
    for offset in 0..byte_count {
        let Some(addr) = status_ptr.checked_add(offset) else {
            return PPC_PARAM_ERR;
        };
        if memory.write_u8(addr, 0).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    if let Some(busy) = sound.manager.channel_busy(channel) {
        if byte_count > 12 && memory.write_u8(status_ptr + 12, u8::from(busy)).is_none() {
            return PPC_PARAM_ERR;
        }
        if byte_count > 14
            && memory
                .write_u8(
                    status_ptr + 14,
                    u8::from(sound.manager.file_playback_paused(channel).unwrap_or(false)),
                )
                .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_snd_get_info(cpu: &PpcCpu, memory: &mut PpcSectionMem, sound: &PpcSoundState) -> i16 {
    const SI_UNKNOWN_INFO_TYPE: i16 = -231;
    let channel = cpu.gpr[3];
    let selector = cpu.gpr[4];
    let info_ptr = cpu.gpr[5];
    if channel == 0 || info_ptr == 0 {
        return PPC_PARAM_ERR;
    }

    // Universal Interfaces 3.4.1 Sound.h identifies SndGetInfo as a Sound
    // Manager 3.1 SoundLib call. These scalar selectors default to the native
    // HLE mixer's canonical mono, 8-bit, 22.254 kHz channel; siSampleRate
    // reflects a value set on this channel. Unknown queries return the
    // documented siUnknownInfoType instead of fabricating data.
    let result = match &selector.to_be_bytes() {
        b"srat" => memory.write_u32_be(
            info_ptr,
            sound
                .manager
                .find_channel(channel)
                .map(|channel| channel.sample_rate())
                .unwrap_or(crate::sound::RATE_22KHZ_FIXED),
        ),
        b"ssiz" => memory.write_u16_be(info_ptr, 8),
        b"chan" => memory.write_u16_be(info_ptr, 1),
        b"hwbs" => {
            memory.write_u8(
                info_ptr,
                u8::from(sound.manager.channel_busy(channel).unwrap_or(false)),
            )
        }
        _ => return SI_UNKNOWN_INFO_TYPE,
    };
    if result.is_some() {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    }
}

pub(crate) fn ppc_snd_set_info(cpu: &PpcCpu, memory: &mut PpcSectionMem, sound: &mut PpcSoundState) -> i16 {
    const SI_UNKNOWN_INFO_TYPE: i16 = -231;
    let channel = cpu.gpr[3];
    let selector = cpu.gpr[4];
    let info_ptr = cpu.gpr[5];
    if channel == 0 || info_ptr == 0 {
        return PPC_PARAM_ERR;
    }

    // SndSetInfo(SndChannelPtr, OSType, const void *) is a SoundLib 3.1
    // call. siSampleRate ('srat') uses a pointer to an unsigned 16.16 Fixed
    // sample rate; an unmapped pointer is an invalid argument, not a value.
    // Universal Interfaces 3.4.1, Sound.h; Inside Macintosh: Sound (1994),
    // Sound Input Manager Reference, siSampleRate.
    match &selector.to_be_bytes() {
        b"srat" => {
            let Some(rate) = memory.read_u32_be(info_ptr) else {
                return PPC_PARAM_ERR;
            };
            let Some(()) = sound
                .manager
                .with_channel_mut(channel, |channel| channel.set_sample_rate(rate))
            else {
                return PPC_PARAM_ERR;
            };
            PPC_NO_ERR
        }
        _ => SI_UNKNOWN_INFO_TYPE,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcParsedSndHeader {
    format: u32,
    num_channels: u16,
    sample_size: u16,
    sample_rate: u32,
    sample_count: u32,
    buffer: u32,
    num_frames: u32,
    data_offset: u32,
}

/// SetupSndHeader (SoundDispatch selector $0D480014)
/// Constructs a format 1 sound resource with a bufferCmd and sampled header.
/// FUNCTION SetupSndHeader (sndHandle: Handle; numChannels: Integer;
/// sampleRate: Fixed; sampleSize: Integer; compressionType: OSType;
/// baseFrequency: Integer; numBytes: LongInt; VAR headerLen: Integer): OSErr;
/// Inside Macintosh: Sound (1994), pp. 3-44--3-45.
pub(crate) fn ppc_setup_snd_header(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
) -> i16 {
    const NONE: u32 = u32::from_be_bytes(*b"NONE");
    const MAC3: u32 = u32::from_be_bytes(*b"MAC3");
    const MAC6: u32 = u32::from_be_bytes(*b"MAC6");
    const STANDARD_HEADER_LEN: u32 = 42;
    const EXTENDED_HEADER_LEN: u32 = 84;
    let handle = cpu.gpr[3];
    let channels = cpu.gpr[4] as u16;
    let sample_rate = cpu.gpr[5];
    let sample_size = cpu.gpr[6] as u16;
    let compression = cpu.gpr[7];
    let base_frequency = cpu.gpr[8] as u16;
    let num_bytes = cpu.gpr[9];
    let header_len_out = cpu.gpr[10];
    let standard = compression == NONE && channels == 1 && sample_size == 8;
    let extended = compression == NONE
        && matches!(channels, 1 | 2)
        && matches!(sample_size, 8 | 16)
        && !standard;
    let compressed =
        matches!(compression, MAC3 | MAC6) && matches!(channels, 1 | 2) && sample_size == 8;
    if !matches!(compression, NONE | MAC3 | MAC6) {
        return -223; // siInvalidCompression
    }
    if (!standard && !extended && !compressed)
        || !(1..=127).contains(&base_frequency)
        || header_len_out == 0
    {
        return PPC_PARAM_ERR;
    }
    let header_len = if standard {
        STANDARD_HEADER_LEN
    } else {
        EXTENDED_HEADER_LEN
    };
    let Some(record) = handles.iter().find(|record| record.handle == handle) else {
        return PPC_PARAM_ERR;
    };
    let Some(ptr) = memory.read_u32_be(handle) else {
        return PPC_PARAM_ERR;
    };
    if ptr == 0
        || ptr != record.ptr
        || !ppc_memory_can_write_bytes(memory, ptr, header_len)
        || !ppc_memory_can_write_bytes(memory, header_len_out, 2)
    {
        return PPC_PARAM_ERR;
    }
    if record.size < header_len {
        return PPC_MEM_FULL_ERR;
    }
    let mut header = [0u8; EXTENDED_HEADER_LEN as usize];
    header[0..2].copy_from_slice(&1u16.to_be_bytes()); // firstSoundFormat
    header[2..4].copy_from_slice(&1u16.to_be_bytes()); // one synth
    header[4..6].copy_from_slice(&5u16.to_be_bytes()); // sampledSynth
    let channel_init = if channels == 1 { 0x80u32 } else { 0xC0 };
    let compression_init = match compression {
        MAC3 => 0x0300,
        MAC6 => 0x0400,
        _ => 0,
    };
    header[6..10].copy_from_slice(&(channel_init | compression_init).to_be_bytes());
    header[10..12].copy_from_slice(&1u16.to_be_bytes()); // one command
    header[12..14].copy_from_slice(&0x8051u16.to_be_bytes()); // bufferCmd + offset flag
    header[16..20].copy_from_slice(&20u32.to_be_bytes()); // SoundHeader offset
    if standard {
        header[24..28].copy_from_slice(&num_bytes.to_be_bytes());
    } else {
        let num_frames = match compression {
            MAC3 => {
                ((u64::from(num_bytes) * 3 / u64::from(channels)).min(u64::from(u32::MAX))) as u32
            }
            MAC6 => {
                ((u64::from(num_bytes) * 6 / u64::from(channels)).min(u64::from(u32::MAX))) as u32
            }
            _ => num_bytes / (u32::from(channels) * u32::from(sample_size / 8)),
        };
        header[24..28].copy_from_slice(&u32::from(channels).to_be_bytes());
        header[40] = if compressed { 0xFE } else { 0xFF }; // cmpSH or extSH
        header[42..46].copy_from_slice(&num_frames.to_be_bytes());
        let rate = super::Extended80::from(f64::from(sample_rate) / 65536.0);
        let exponent = (if rate.sign { 0x8000 } else { 0 }) | rate.exponent;
        header[46..48].copy_from_slice(&exponent.to_be_bytes());
        header[48..56].copy_from_slice(&rate.significand.to_be_bytes());
        if compressed {
            header[60..64].copy_from_slice(&compression.to_be_bytes());
            header[76..78].copy_from_slice(&u16::MAX.to_be_bytes()); // fixedCompression
            header[78..80]
                .copy_from_slice(&(if compression == MAC3 { 16u16 } else { 8 }).to_be_bytes());
            header[82..84].copy_from_slice(&sample_size.to_be_bytes());
        } else {
            header[68..70].copy_from_slice(&sample_size.to_be_bytes());
        }
    }
    header[28..32].copy_from_slice(&sample_rate.to_be_bytes());
    header[41] = base_frequency as u8;
    if memory
        .write_bytes(ptr, &header[..header_len as usize])
        .is_none()
        || memory
            .write_u16_be(header_len_out, header_len as u16)
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

/// Sound Manager 3.3, pp. 343–344: describe an open AIFF data fork without
/// decoding it. Compressed frame counts and format tags remain unchanged.
pub(crate) fn ppc_parse_aiff_header(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &[PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let info = cpu.gpr[4];
    let frames = cpu.gpr[5];
    let offset = cpu.gpr[6];
    if info == 0
        || frames == 0
        || offset == 0
        || !ppc_memory_can_write_bytes(memory, info, 28)
        || !ppc_memory_can_write_bytes(memory, frames, 4)
        || !ppc_memory_can_write_bytes(memory, offset, 4)
    {
        return PPC_PARAM_ERR;
    }
    let Some(data) = ppc_file_data_for_refnum(cpu.gpr[3] as u16 as i16, files, vfs_files) else {
        return PPC_RF_NUM_ERR;
    };
    let Some(parsed) = ppc_parse_aiff_header_fields(data) else {
        return PPC_BAD_FORMAT;
    };
    let mut record = [0u8; 28];
    record[4..8].copy_from_slice(&parsed.format.to_be_bytes());
    record[8..10].copy_from_slice(&parsed.num_channels.to_be_bytes());
    record[10..12].copy_from_slice(&parsed.sample_size.to_be_bytes());
    record[12..16].copy_from_slice(&parsed.sample_rate.to_be_bytes());
    record[16..20].copy_from_slice(&parsed.sample_count.to_be_bytes());
    // flags, buffer and reserved are explicitly zero for ParseAIFFHeader.
    let _ = memory.write_bytes(info, &record);
    let _ = memory.write_u32_be(frames, parsed.num_frames);
    let _ = memory.write_u32_be(offset, parsed.data_offset);
    PPC_NO_ERR
}

pub(crate) fn ppc_parse_aiff_header_fields(data: &[u8]) -> Option<PpcParsedSndHeader> {
    if data.get(0..4)? != b"FORM" {
        return None;
    }
    let aifc = match data.get(8..12)? {
        b"AIFF" => false,
        b"AIFC" => true,
        _ => return None,
    };
    let end = 8usize.checked_add(usize::try_from(ppc_read_be_u32_from_slice(data, 4)?).ok()?)?;
    if end < 12 || end > data.len() {
        return None;
    }
    let mut cursor = 12usize;
    let mut common = None;
    let mut sound = None;
    let mut version = false;
    while cursor < end {
        let start = cursor.checked_add(8)?;
        if start > end {
            return None;
        }
        let size = usize::try_from(ppc_read_be_u32_from_slice(data, cursor + 4)?).ok()?;
        let chunk_end = start.checked_add(size)?;
        let next = chunk_end.checked_add(size & 1)?;
        if next > end {
            return None;
        }
        let chunk = data.get(start..chunk_end)?;
        match data.get(cursor..cursor + 4)? {
            b"COMM" => {
                if common.is_some() || size < if aifc { 22 } else { 18 } {
                    return None;
                }
                let channels = ppc_read_be_u16_from_slice(chunk, 0)?;
                let frames = ppc_read_be_u32_from_slice(chunk, 2)?;
                let bits = ppc_read_be_u16_from_slice(chunk, 6)?;
                if channels == 0 || bits == 0 || bits > 32 || frames > i32::MAX as u32 {
                    return None;
                }
                let rate = ppc_extended_sample_rate_fixed(chunk.get(8..18)?)?;
                let compression = if aifc {
                    ppc_read_be_u32_from_slice(chunk, 18)?
                } else {
                    u32::from_be_bytes(*b"NONE")
                };
                let format = if compression == u32::from_be_bytes(*b"NONE") {
                    u32::from_be_bytes(*b"twos") // AIFF PCM is signed, including 8-bit samples.
                } else {
                    compression
                };
                common = Some((channels, frames, bits, rate, format));
            }
            b"SSND" => {
                if sound.is_some() || size < 8 {
                    return None;
                }
                let skip = usize::try_from(ppc_read_be_u32_from_slice(chunk, 0)?).ok()?;
                let first = start.checked_add(8)?.checked_add(skip)?;
                if first > chunk_end {
                    return None;
                }
                sound = Some((u32::try_from(first).ok()?, chunk_end - first));
            }
            b"FVER" if aifc => {
                if version || size != 4 || ppc_read_be_u32_from_slice(chunk, 0)? != 0xa280_5140 {
                    return None;
                }
                version = true;
            }
            _ => {}
        }
        cursor = next;
    }
    if aifc && !version {
        return None;
    }
    let (channels, frames, bits, rate, format) = common?;
    let (first, bytes) = sound.or_else(|| (frames == 0).then_some((0, 0)))?;
    if frames != 0 && bytes == 0 {
        return None;
    }
    Some(PpcParsedSndHeader {
        format,
        num_channels: channels,
        sample_size: bits,
        sample_rate: rate,
        sample_count: frames,
        buffer: 0,
        num_frames: frames,
        data_offset: first,
    })
}

/// Convert positive normalized AIFF 80-bit rates directly to unsigned 16.16,
/// retaining fractional hertz rather than rounding through integer hertz.
fn ppc_extended_sample_rate_fixed(bytes: &[u8]) -> Option<u32> {
    let exponent = u16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?);
    if exponent & 0x8000 != 0 || exponent == 0 || exponent == 0x7fff {
        return None;
    }
    let mantissa = u64::from_be_bytes(bytes.get(2..10)?.try_into().ok()?);
    if mantissa & (1u64 << 63) == 0 {
        return None;
    }
    let shift = i32::from(exponent) - 16383 - 63 + 16;
    let fixed = if shift >= 0 {
        u128::from(mantissa).checked_shl(u32::try_from(shift).ok()?)?
    } else {
        u128::from(mantissa)
            .checked_shr(shift.unsigned_abs())
            .unwrap_or(0)
    };
    let fixed = u32::try_from(fixed).ok()?;
    (fixed != 0).then_some(fixed)
}

pub(crate) fn ppc_parse_snd_header(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
) -> i16 {
    let handle = cpu.gpr[3];
    let info_ptr = cpu.gpr[4];
    let num_frames_ptr = cpu.gpr[5];
    let data_offset_ptr = cpu.gpr[6];
    if info_ptr == 0
        || num_frames_ptr == 0
        || data_offset_ptr == 0
        || !ppc_memory_can_write_bytes(memory, info_ptr, 28)
        || !ppc_memory_can_write_bytes(memory, num_frames_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, data_offset_ptr, 4)
    {
        return PPC_PARAM_ERR;
    }
    let Some(record) = handles.iter().find(|record| record.handle == handle) else {
        return PPC_PARAM_ERR;
    };
    let Some(resource_ptr) = memory.read_u32_be(handle) else {
        return PPC_PARAM_ERR;
    };
    if resource_ptr == 0 || resource_ptr != record.ptr {
        return PPC_PARAM_ERR;
    }
    let Some(header_offset) = ppc_sound_header_offset(memory, resource_ptr) else {
        return PPC_BAD_FORMAT;
    };
    let Some(parsed) = ppc_parse_snd_header_fields(memory, resource_ptr, header_offset) else {
        return PPC_BAD_FORMAT;
    };

    let writes = [
        memory.write_u32_be(info_ptr, 0),
        memory.write_u32_be(info_ptr + 4, parsed.format),
        memory.write_u16_be(info_ptr + 8, parsed.num_channels),
        memory.write_u16_be(info_ptr + 10, parsed.sample_size),
        memory.write_u32_be(info_ptr + 12, parsed.sample_rate),
        memory.write_u32_be(info_ptr + 16, parsed.sample_count),
        memory.write_u32_be(info_ptr + 20, parsed.buffer),
        memory.write_u32_be(info_ptr + 24, 0),
        memory.write_u32_be(num_frames_ptr, parsed.num_frames),
        memory.write_u32_be(data_offset_ptr, parsed.data_offset),
    ];
    if writes.iter().all(Option::is_some) {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    }
}

/// Parse the sampled-sound header embedded in a format-1 or format-2 `snd `
/// resource. Sound Manager 3.0's ParseSndHeader reports a SoundComponentData
/// record plus the frame count and resource-relative start of the sample data.
pub(crate) fn ppc_parse_snd_header_fields(
    memory: &mut PpcSectionMem,
    resource_ptr: u32,
    header_offset: u32,
) -> Option<PpcParsedSndHeader> {
    const STD_SH: u8 = 0x00;
    const CMP_SH: u8 = 0xfe;
    const EXT_SH: u8 = 0xff;
    const RAW_FORMAT: u32 = u32::from_be_bytes(*b"raw ");
    const TWOS_FORMAT: u32 = u32::from_be_bytes(*b"twos");

    let header = resource_ptr.checked_add(header_offset)?;
    let sample_ptr = memory.read_u32_be(header)?;
    let sample_rate = memory.read_u32_be(header.checked_add(8)?)?;
    let encode = memory.read_u8(header.checked_add(20)?)?;
    let (format, num_channels, sample_size, sample_count, num_frames, inline_offset) = match encode
    {
        STD_SH => {
            let length = memory.read_u32_be(header.checked_add(4)?)?;
            (RAW_FORMAT, 1, 8, length, length, 22)
        }
        EXT_SH => {
            let channels = memory.read_u32_be(header.checked_add(4)?)?;
            let channels = u16::try_from(channels).ok()?;
            let frames = memory.read_u32_be(header.checked_add(22)?)?;
            let sample_size = memory.read_u16_be(header.checked_add(48)?)?;
            let format = match sample_size {
                8 => RAW_FORMAT,
                16 => TWOS_FORMAT,
                _ => return None,
            };
            (format, channels, sample_size, frames, frames, 64)
        }
        CMP_SH => {
            let channels = memory.read_u32_be(header.checked_add(4)?)?;
            let channels = u16::try_from(channels).ok()?;
            let frames = memory.read_u32_be(header.checked_add(22)?)?;
            let format = memory.read_u32_be(header.checked_add(40)?)?;
            let sample_size = memory.read_u16_be(header.checked_add(62)?)?;
            (format, channels, sample_size, frames, frames, 64)
        }
        _ => return None,
    };
    let inline_data = header.checked_add(inline_offset)?;
    let buffer = if sample_ptr == 0 {
        inline_data
    } else {
        sample_ptr
    };
    let data_offset = if sample_ptr == 0 {
        header_offset.checked_add(inline_offset)?
    } else {
        sample_ptr.checked_sub(resource_ptr).unwrap_or(0)
    };
    Some(PpcParsedSndHeader {
        format,
        num_channels,
        sample_size,
        sample_rate,
        sample_count,
        buffer,
        num_frames,
        data_offset,
    })
}

pub(crate) fn ppc_snd_do_immediate(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    sound: &mut PpcSoundState,
) -> i16 {
    let channel = cpu.gpr[3];
    let cmd_ptr = cpu.gpr[4];
    let Some(command) = ppc_read_snd_command(memory, cmd_ptr) else {
        return PPC_PARAM_ERR;
    };
    if command.command == crate::sound::cmd::GET_RATE && command.param2 != 0 {
        if !ppc_memory_can_write_bytes(memory, command.param2, 4)
            || memory
                .write_u32_be(
                    command.param2,
                    sound
                        .manager
                        .channel_rate(channel)
                        .unwrap_or(0x0001_0000),
                )
                .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    if command.command == crate::sound::cmd::SOUND {
        ppc_install_sampled_sound(memory, sound, channel, command.param2);
    }
    if ppc_sound_trace_enabled()
        && matches!(
            command.command,
            crate::sound::cmd::FREQ | crate::sound::cmd::FREQ_DURATION
        )
    {
        eprintln!(
            "[PPC-SOUND] SndDoImmediate note chan=${channel:08X} cmd={} param1={} note={}",
            command.command, command.param1, command.param2
        );
    }
    if let Some(decoded) = ppc_decode_buffer_command(memory, channel, command) {
        sound.manager.play_buffer_command_for_architecture(
            channel,
            decoded.samples,
            decoded.sample_rate_fixed,
            CallbackTaskArchitecture::PowerPc,
        );
    } else {
        sound.manager.execute_immediate_command(
            channel,
            crate::sound::SndCommand {
                cmd: command.command,
                param1: command.param1,
                param2: command.param2,
            },
        );
    }
    sound
        .immediate_commands
        .push(PpcSndCommandRecord { channel, ..command });
    PPC_NO_ERR
}

/// soundCmd: decode the SoundHeader at `header` and install it as the
/// channel's sampled-sound voice. Installing does not start playback; a
/// later freqCmd or freqDurationCmd plays it (Inside Macintosh: Sound
/// (1994), pp. 2-44 and 2-97). The book limits soundCmd to noncompressed
/// data and expects compressed sounds to be expanded first (p. 2-109);
/// compressed headers are accepted here because they decode to the same
/// samples. A header that cannot be decoded leaves the channel without a
/// voice.
fn ppc_install_sampled_sound(
    memory: &mut PpcSectionMem,
    sound: &mut PpcSoundState,
    channel: u32,
    header: u32,
) {
    let instrument = ppc_sampled_instrument_from_header(memory, header);
    if instrument.is_none() && ppc_sound_trace_enabled() {
        eprintln!("[PPC-SOUND] could not decode soundCmd header=${header:08X}");
    }
    sound.manager.with_mut(|manager| match instrument {
        Some(instrument) => manager.install_instrument(channel, instrument),
        None => manager.remove_instrument(channel),
    });
}

/// The voice described by a SoundHeader: its decoded samples, base note,
/// and loop points. The three header forms share loopStart (offset 12),
/// loopEnd (16), and baseFrequency (21). Inside Macintosh: Sound (1994),
/// pp. 2-104, 2-106, and 2-108.
fn ppc_sampled_instrument_from_header(
    memory: &mut PpcSectionMem,
    header: u32,
) -> Option<crate::sound::SampledInstrument> {
    if header == 0 {
        return None;
    }
    let loop_start = memory.read_u32_be(header.checked_add(12)?)?;
    let loop_end = memory.read_u32_be(header.checked_add(16)?)?;
    let base_note = memory.read_u8(header.checked_add(21)?)?;
    let decoded = ppc_decode_snd_header_from_memory(memory, header)?;
    Some(crate::sound::SampledInstrument {
        samples: decoded.samples,
        sample_rate_fixed: decoded.summary.sample_rate_fixed,
        base_note,
        loop_start,
        loop_end,
    })
}

pub(crate) fn ppc_snd_do_command(cpu: &PpcCpu, memory: &mut PpcSectionMem, sound: &mut PpcSoundState) -> i16 {
    let channel = cpu.gpr[3];
    let Some(command) = ppc_read_snd_command(memory, cpu.gpr[4]) else {
        return PPC_PARAM_ERR;
    };
    if ppc_sound_trace_enabled() {
        eprintln!(
            "[PPC-SOUND] SndDoCommand chan=${channel:08X} cmd={} param1={} param2=${:08X} noWait={}",
            command.command, command.param1, command.param2, cpu.gpr[5] != 0
        );
    }

    // Inside Macintosh: Sound (1994), pp. 2-130–2-131: SndDoCommand appends
    // the eight-byte SndCommand to the channel's FIFO queue. A callbackCmd
    // following sampled sound runs only when that sound has completed, with
    // a copy of the queued command as the callback's second argument. The
    // process manager retains the command and selects the native callback ABI
    // from the channel created by SndNewChannel; it is not a file-play
    // completion routine.
    // `noWait` controls whether the classic implementation may sleep until
    // a FIFO slot opens. The host runner cannot block the guest, so both
    // variants return queueFull when the process-owned FIFO is full.
    // soundCmd's header is decoded now, while the guest's pointer is known
    // to be current; the queue installs the voice in order.
    if command.command == crate::sound::cmd::SOUND {
        let instrument = ppc_sampled_instrument_from_header(memory, command.param2);
        if instrument.is_none() && ppc_sound_trace_enabled() {
            eprintln!(
                "[PPC-SOUND] could not decode soundCmd header=${:08X}",
                command.param2
            );
        }
        if !sound
            .manager
            .with_mut(|manager| manager.enqueue_instrument(channel, instrument))
        {
            return -203; // queueFull
        }
    } else if let Some(decoded) = ppc_decode_buffer_command(memory, channel, command) {
        if !sound.manager.enqueue_buffer_command_for_architecture(
            channel,
            decoded.samples,
            decoded.sample_rate_fixed,
            CallbackTaskArchitecture::PowerPc,
        ) {
            return -203; // queueFull
        }
    } else {
        if !sound.manager.enqueue_command(
            channel,
            crate::sound::SndCommand {
                cmd: command.command,
                param1: command.param1,
                param2: command.param2,
            },
        ) {
            return -203; // queueFull
        }
    }
    sound
        .queued_commands
        .push(PpcSndCommandRecord { channel, ..command });
    PPC_NO_ERR
}

pub(crate) fn ppc_snd_dispose_channel(cpu: &mut PpcCpu, sound: &mut PpcSoundState) -> i16 {
    let channel = cpu.gpr[3];
    sound.manager.remove_channel(channel);
    sound
        .decoded_file_playbacks
        .retain(|record| record.channel != channel);
    sound.file_playbacks.retain(|record| record.channel != channel);
    PPC_NO_ERR
}

pub(crate) fn ppc_snd_play(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    sound: &mut PpcSoundState,
) -> i16 {
    const RES_PROBLEM: i16 = -204;
    const BAD_FORMAT: i16 = -206;
    let channel = cpu.gpr[3];
    let sound_handle = cpu.gpr[4];
    let async_play = cpu.gpr[5] != 0;
    if sound_handle == 0 || memory.read_u32_be(sound_handle).unwrap_or(0) == 0 {
        return RES_PROBLEM;
    }
    let Some(bytes) = ppc_handle_bytes(memory, handles, sound_handle) else {
        return RES_PROBLEM;
    };
    let Some(commands) = ppc_decode_snd_resource_commands(&bytes) else {
        return BAD_FORMAT;
    };
    let effective_channel = if channel == 0 {
        // Sound 1994, p. 2-122: NIL ignores async and uses an internal
        // channel for the duration of the sampled playback.
        sound.manager.create_internal_channel()
    } else {
        channel
    };
    if ppc_sound_trace_enabled() {
        eprintln!(
            "[PPC-SOUND] SndPlay chan=${channel:08X} effective=${effective_channel:08X} handle=${sound_handle:08X} async={} commands={}",
            async_play,
            commands.len()
        );
    }

    // Inside Macintosh: Sound (1994), pp. 2-121--2-123: preserve every
    // resource command in order. An idle first buffer can start immediately;
    // commands after it (and all commands on a busy channel) remain in the
    // process-owned FIFO until the current buffer completes.
    for command in commands {
        match command {
            PpcDecodedSndResourceCommand::Buffer {
                samples,
                sample_rate_fixed,
            } => {
                let busy = sound.manager.channel_busy(effective_channel).unwrap_or(false);
                let queued = sound
                    .manager
                    .channel_has_queued_commands(effective_channel)
                    .unwrap_or(false);
                if busy || queued {
                    if !sound.manager.enqueue_buffer_command_for_architecture(
                        effective_channel,
                        samples,
                        sample_rate_fixed,
                        CallbackTaskArchitecture::PowerPc,
                    ) {
                        return -203; // queueFull
                    }
                } else {
                    sound.manager.play_buffer_command_for_architecture(
                        effective_channel,
                        samples,
                        sample_rate_fixed,
                        CallbackTaskArchitecture::PowerPc,
                    );
                }
            }
            PpcDecodedSndResourceCommand::Command(command) => {
                let busy = sound.manager.channel_busy(effective_channel).unwrap_or(false);
                let queued = sound
                    .manager
                    .channel_has_queued_commands(effective_channel)
                    .unwrap_or(false);
                if busy || queued || command.cmd == crate::sound::cmd::CALLBACK {
                    if !sound.manager.enqueue_command(effective_channel, command) {
                        return -203; // queueFull
                    }
                } else {
                    sound
                        .manager
                        .execute_immediate_command(effective_channel, command);
                }
            }
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_decode_buffer_command(
    memory: &mut PpcSectionMem,
    channel: u32,
    command: PpcSndCommandRecord,
) -> Option<PpcDecodedBufferCommandRecord> {
    const BUFFER_CMD: u16 = 81;

    if command.command != BUFFER_CMD || command.param2 == 0 {
        return None;
    }
    let Some(decoded) = ppc_decode_snd_header_from_memory(memory, command.param2) else {
        if ppc_sound_trace_enabled() {
            eprintln!(
                "[PPC-SOUND] could not decode bufferCmd header=${:08X}",
                command.param2
            );
        }
        return None;
    };
    Some(PpcDecodedBufferCommandRecord {
        channel,
        sample_rate_fixed: decoded.summary.sample_rate_fixed,
        samples: decoded.samples,
    })
}

/// Decode a Sound Manager SoundHeader referenced by bufferCmd.
///
/// Inside Macintosh: Sound (1994), pp. 2-104, 2-106, and 2-109 define the
/// standard, extended, and compressed headers. A non-NIL samplePtr addresses
/// the samples independently of the header; otherwise sampleArea follows the
/// fixed header. Extended PCM is interleaved and 8- or 16-bit, while the
/// compressed form identifies MACE through compressionID/format.
pub(crate) fn ppc_decode_snd_header_from_memory(
    memory: &mut PpcSectionMem,
    header_addr: u32,
) -> Option<PpcDecodedAiffData> {
    const STD_SH: u8 = 0x00;
    const CMP_SH: u8 = 0xfe;
    const EXT_SH: u8 = 0xff;
    const MAX_RETAINED_SAMPLE_BYTES: u32 = 64 * 1024 * 1024;

    let sample_ptr = memory.read_u32_be(header_addr)?;
    let sample_rate_fixed = memory.read_u32_be(header_addr.checked_add(8)?)?;
    let encode = memory.read_u8(header_addr.checked_add(20)?)?;
    let samples = match encode {
        STD_SH => {
            let length = memory.read_u32_be(header_addr.checked_add(4)?)?;
            if length > MAX_RETAINED_SAMPLE_BYTES {
                return None;
            }
            let data_addr = if sample_ptr != 0 {
                sample_ptr
            } else {
                header_addr.checked_add(22)?
            };
            ppc_memory_read_bytes(memory, data_addr, length)?
        }
        EXT_SH => {
            let channels =
                usize::try_from(memory.read_u32_be(header_addr.checked_add(4)?)?).ok()?;
            let frames = usize::try_from(memory.read_u32_be(header_addr.checked_add(22)?)?).ok()?;
            let sample_size = usize::from(memory.read_u16_be(header_addr.checked_add(48)?)?);
            let bytes_per_sample = match sample_size {
                8 => 1usize,
                16 => 2usize,
                _ => return None,
            };
            let byte_count = frames
                .checked_mul(channels)?
                .checked_mul(bytes_per_sample)?;
            let byte_count = u32::try_from(byte_count).ok()?;
            if byte_count > MAX_RETAINED_SAMPLE_BYTES {
                return None;
            }
            let data_addr = if sample_ptr != 0 {
                sample_ptr
            } else {
                header_addr.checked_add(64)?
            };
            let raw = ppc_memory_read_bytes(memory, data_addr, byte_count)?;
            ppc_decode_interleaved_pcm_samples(&raw, 0, frames, channels, sample_size)?
        }
        CMP_SH => {
            let channels =
                usize::try_from(memory.read_u32_be(header_addr.checked_add(4)?)?).ok()?;
            let frames = usize::try_from(memory.read_u32_be(header_addr.checked_add(22)?)?).ok()?;
            let compression_format = memory.read_u32_be(header_addr.checked_add(40)?)?;
            let compression_id = memory.read_u16_be(header_addr.checked_add(56)?)? as i16;
            let packet_size = usize::from(memory.read_u16_be(header_addr.checked_add(58)?)?);
            let sample_size = usize::from(memory.read_u16_be(header_addr.checked_add(62)?)?);
            let codec =
                ppc_sound_codec(compression_id, compression_format, sample_size as u16)?;
            // A zero packetSize means the codec's standard packet.
            let packet_bits = usize::from(codec.packet_geometry().1) * 8;
            if packet_size != 0 && packet_size != packet_bits {
                return None;
            }
            let byte_count = frames.checked_mul(codec.bytes_per_frame(channels)?)?;
            let byte_count = u32::try_from(byte_count).ok()?;
            if byte_count > MAX_RETAINED_SAMPLE_BYTES {
                return None;
            }
            let data_addr = if sample_ptr != 0 {
                sample_ptr
            } else {
                header_addr.checked_add(64)?
            };
            let compressed = ppc_memory_read_bytes(memory, data_addr, byte_count)?;
            codec.to_mono_u8(&compressed, frames, channels)?
        }
        _ => return None,
    };
    ppc_decoded_sound_data(samples, sample_rate_fixed)
}

pub(crate) fn ppc_snd_play_double_buffer(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    sound: &mut PpcSoundState,
) -> i16 {
    let channel = cpu.gpr[3];
    let header = cpu.gpr[4];
    if channel == 0 || header == 0 || !ppc_memory_can_write_bytes(memory, header, 24) {
        return PPC_PARAM_ERR;
    }
    let buffer0 = memory.read_u32_be(header + 12).unwrap_or(0);
    let buffer1 = memory.read_u32_be(header + 16).unwrap_or(0);
    let callback = memory.read_u32_be(header + 20).unwrap_or(0);
    let num_channels = memory.read_u16_be(header).unwrap_or(0);
    let sample_size = memory.read_u16_be(header + 2).unwrap_or(0);
    let compression_id = memory.read_u16_be(header + 4).unwrap_or(0) as i16;
    let packet_size = memory.read_u16_be(header + 6).unwrap_or(0);
    let raw_sample_rate = memory.read_u32_be(header + 8).unwrap_or(0);
    // SndDoubleBufferHeader2 appends dbhFormat, which is used only when
    // dbhCompressionID is fixedCompression. Inside Macintosh: Sound (1994),
    // pp. 2-69 and 2-111.
    let format = if compression_id == FIXED_COMPRESSION {
        memory.read_u32_be(header + 24).unwrap_or(0)
    } else {
        0
    };
    let sample_rate_fixed = if raw_sample_rate == 0 {
        crate::sound::RATE_22KHZ_FIXED
    } else {
        raw_sample_rate
    };
    if ppc_sound_trace_enabled() {
        eprintln!(
            "[PPC-SOUND] SndPlayDoubleBuffer chan=${:08X} header=${:08X} channels={} bits={} rate=${:08X} buf0=${:08X} buf1=${:08X} callback=${:08X}",
            channel,
            header,
            num_channels,
            sample_size,
            sample_rate_fixed,
            buffer0,
            buffer1,
            callback
        );
    }
    sound.manager.stop_double_buffer_playbacks(channel);
    let mut playback = PpcSoundDoubleBufferPlaybackRecord {
        channel,
        header,
        buffers: [buffer0, buffer1],
        callback,
        callback_architecture: CallbackTaskArchitecture::PowerPc,
        sample_rate_fixed,
        num_channels,
        sample_size,
        compression_id,
        format,
        packet_size,
        current_buffer_index: 0,
        callback_pending_mask: 0,
        active: true,
        host_initialized: false,
        host_buffer_loaded: false,
    };
    sound.manager.note_double_buffer_submission();
    playback.host_initialized = true;
    if let Some(samples) = ppc_decode_ready_double_buffer(memory, playback) {
        sound
            .manager
            .play_double_buffer_samples(channel, samples, sample_rate_fixed);
        playback.host_buffer_loaded = true;
    }
    sound
        .manager
        .with_mut(|manager| manager.double_buffer_playbacks.push(playback));
    sound.double_buffer_play_count = sound.double_buffer_play_count.saturating_add(1);
    sound.last_double_buffer_channel = channel;
    sound.last_double_buffer_header = header;
    PPC_NO_ERR
}

pub(crate) fn ppc_decode_ready_double_buffer(
    memory: &mut PpcSectionMem,
    playback: PpcSoundDoubleBufferPlaybackRecord,
) -> Option<Vec<crate::sound::StereoSample>> {
    let buffer_ptr = playback.buffers[usize::from(playback.current_buffer_index & 1)];
    if buffer_ptr == 0 {
        return None;
    }
    let num_frames = usize::try_from(memory.read_u32_be(buffer_ptr)?).ok()?;
    let flags = memory.read_u32_be(buffer_ptr.checked_add(4)?)?;
    if flags & 0x01 == 0 || num_frames == 0 {
        return None;
    }
    ppc_read_double_buffer_samples(memory, playback, buffer_ptr, num_frames)
}

/// Decode `num_frames` frames of a SndDoubleBuffer's dbSoundData.
///
/// For compressed data a frame is one packet across all channels, the unit
/// GetCompressionInfo reports as bytesPerFrame.
pub(crate) fn ppc_read_double_buffer_samples(
    memory: &mut PpcSectionMem,
    playback: PpcSoundDoubleBufferPlaybackRecord,
    buffer_ptr: u32,
    num_frames: usize,
) -> Option<Vec<crate::sound::StereoSample>> {
    const MAX_RETAINED_SAMPLE_BYTES: usize = 64 * 1024 * 1024;

    let channels = usize::from(playback.num_channels);
    let codec = ppc_sound_codec(playback.compression_id, playback.format, playback.sample_size)?;
    let byte_count = num_frames
        .checked_mul(codec.bytes_per_frame(channels)?)?;
    if byte_count > MAX_RETAINED_SAMPLE_BYTES {
        return None;
    }
    let mut raw = vec![0; byte_count];
    memory.read_bytes_into(buffer_ptr.checked_add(16)?, &mut raw)?;
    let (pcm, sample_size) = codec.to_pcm(&raw, num_frames, channels)?;
    let pcm_frames = pcm.len() / channels.checked_mul(sample_size / 8)?;
    crate::trap::decode_interleaved_stereo_samples(&pcm, pcm_frames, channels, sample_size)
}

/// Sound Manager compressionID for a header whose format field names the codec.
const FIXED_COMPRESSION: i16 = -1;

/// A Sound Manager sample encoding, identified by compressionID and format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PpcSoundCodec {
    /// Uncompressed big-endian PCM of this many bits per sample.
    Pcm(usize),
    /// Little-endian 16-bit PCM ('sowt').
    PcmLittleEndian16,
    /// IMA 4:1 ADPCM: per channel, 64 samples in a 34-byte packet (two
    /// predictor bytes, then 4-bit codes); stereo puts the left channel's
    /// packet before the right's. Technical Note TN1081.
    Ima4,
    /// G.711 mu-law: one byte per sample.
    MuLaw,
    /// MACE 3:1, mono only: 2-byte packets of 6 samples.
    /// Inside Macintosh: Sound (1994), p. 2-11.
    Mace3,
    /// MACE 6:1, mono only: 1-byte packets of 6 samples.
    Mace6,
}

pub(crate) fn ppc_sound_codec(compression_id: i16, format: u32, sample_size: u16) -> Option<PpcSoundCodec> {
    let pcm = match sample_size {
        8 | 16 => Some(PpcSoundCodec::Pcm(usize::from(sample_size))),
        _ => None,
    };
    match compression_id {
        0 => pcm,
        // threeToOne and sixToOne predate the format field.
        3 => Some(PpcSoundCodec::Mace3),
        4 => Some(PpcSoundCodec::Mace6),
        // fixedCompression names the codec by format; the Sound Manager does
        // not use variableCompression. Inside Macintosh: Sound (1994),
        // p. 2-110. Format codes from Universal Interfaces Sound.h.
        FIXED_COMPRESSION => match &format.to_be_bytes() {
            b"MAC3" => Some(PpcSoundCodec::Mace3),
            b"MAC6" => Some(PpcSoundCodec::Mace6),
            b"ima4" => Some(PpcSoundCodec::Ima4),
            b"ulaw" => Some(PpcSoundCodec::MuLaw),
            b"twos" | b"raw " | b"NONE" => pcm,
            b"sowt" if sample_size == 16 => Some(PpcSoundCodec::PcmLittleEndian16),
            _ => None,
        },
        _ => None,
    }
}

impl PpcSoundCodec {
    /// (samplesPerPacket, bytesPerPacket, bytesPerSample) as GetCompressionInfo
    /// reports them for one channel.
    pub(crate) fn packet_geometry(self) -> (u16, u16, u16) {
        match self {
            Self::Pcm(bits) => (1, (bits / 8) as u16, (bits / 8) as u16),
            Self::PcmLittleEndian16 => (1, 2, 2),
            Self::Ima4 => (64, 34, 2),
            Self::MuLaw => (1, 1, 2),
            Self::Mace3 => (6, 2, 1),
            Self::Mace6 => (6, 1, 1),
        }
    }

    fn bytes_per_frame(self, channels: usize) -> Option<usize> {
        if channels == 0 || (matches!(self, Self::Mace3 | Self::Mace6) && channels != 1) {
            return None;
        }
        channels.checked_mul(usize::from(self.packet_geometry().1))
    }

    /// Expand `frames` frames of `raw` into interleaved big-endian PCM,
    /// returning the PCM bytes and their sample size in bits.
    fn to_pcm(self, raw: &[u8], frames: usize, channels: usize) -> Option<(Vec<u8>, usize)> {
        let raw = raw.get(..frames.checked_mul(self.bytes_per_frame(channels)?)?)?;
        match self {
            Self::Pcm(bits) => Some((raw.to_vec(), bits)),
            Self::PcmLittleEndian16 => Some((
                raw.chunks_exact(2).flat_map(|pair| [pair[1], pair[0]]).collect(),
                16,
            )),
            Self::MuLaw => Some((
                raw.iter()
                    .flat_map(|byte| ppc_mu_law_to_i16(*byte).to_be_bytes())
                    .collect(),
                16,
            )),
            Self::Ima4 => {
                // Each frame holds one 34-byte packet per channel, in channel
                // order; interleave their 64 decoded samples.
                let mut pcm = Vec::with_capacity(frames * 64 * channels * 2);
                for frame in raw.chunks_exact(34 * channels) {
                    let decoded = frame
                        .chunks_exact(34)
                        .map(super::quicktime::ppc_qt_decode_ima4_channel_packet)
                        .collect::<Option<Vec<_>>>()?;
                    for index in 0..64 {
                        for channel in &decoded {
                            pcm.extend_from_slice(&channel[index].to_be_bytes());
                        }
                    }
                }
                Some((pcm, 16))
            }
            Self::Mace3 => Some((crate::trap::decode_mace3_mono_to_u8(raw), 8)),
            Self::Mace6 => Some((crate::trap::decode_mace6_mono_to_u8(raw), 8)),
        }
    }

    /// Decode `frames` frames to the mono unsigned 8-bit samples the
    /// SoundHeader paths retain.
    fn to_mono_u8(self, raw: &[u8], frames: usize, channels: usize) -> Option<Vec<u8>> {
        let (pcm, sample_size) = self.to_pcm(raw, frames, channels)?;
        let pcm_frames = pcm.len() / channels.checked_mul(sample_size / 8)?;
        ppc_decode_interleaved_pcm_samples(&pcm, 0, pcm_frames, channels, sample_size)
    }
}

/// G.711 mu-law expansion to 16-bit linear PCM.
fn ppc_mu_law_to_i16(byte: u8) -> i16 {
    let value = !byte;
    let exponent = (value >> 4) & 0x07;
    let mantissa = i32::from(value & 0x0f);
    let magnitude = (((mantissa << 3) + 0x84) << exponent) - 0x84;
    (if value & 0x80 != 0 { -magnitude } else { magnitude }) as i16
}

pub(crate) fn ppc_read_snd_command(memory: &mut PpcSectionMem, cmd_ptr: u32) -> Option<PpcSndCommandRecord> {
    Some(PpcSndCommandRecord {
        channel: 0,
        command: memory.read_u16_be(cmd_ptr)?,
        param1: memory.read_u16_be(cmd_ptr + 2)? as i16,
        param2: memory.read_u32_be(cmd_ptr + 4)?,
    })
}

pub(crate) fn ppc_snd_start_file_play(
    cpu: &mut PpcCpu,
    files: &[PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    sound: &mut PpcSoundState,
) -> i16 {
    // Inside Macintosh: Sound (1994), p. 2-125: badFileFormat (-208) means
    // the file is corrupt or unusable, or not AIFF or AIFF-C.
    const BAD_FILE_FORMAT: i16 = -208;

    let channel = cpu.gpr[3];
    let ref_num = cpu.gpr[4] as u16 as i16;
    let resource_id = cpu.gpr[5] as u16 as i16;
    // A zero file reference selects an 'snd ' resource, including ID zero.
    // Sound 1994, pp. 2-124--2-125: loading failures do not start playback
    // or run a completion routine (resProblem=-204, badFormat=-206).
    let playing_file = ref_num != 0;
    if playing_file {
        if !files.iter().any(|file| file.ref_num == ref_num) {
            return PPC_RF_NUM_ERR;
        }
        let Some(data) = ppc_file_data_for_refnum(ref_num, files, vfs_files) else {
            return PPC_FNF_ERR;
        };
        if !ppc_is_aiff_form(data) {
            return BAD_FILE_FORMAT;
        }
    } else if ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"snd "),
        resource_id,
        false,
    )
    .is_none()
    {
        return PPC_RES_PROBLEM;
    }
    let file_playback_index = u32::try_from(sound.file_playbacks.len()).ok();
    let (aiff, decoded_sound) = if !playing_file {
        (
            None,
            ppc_snd_resource_playback_for_id(resource_id, vfs_resources, current_resource_refnum),
        )
    } else {
        ppc_aiff_playback_for_refnum(ref_num, files, vfs_files)
    };
    if decoded_sound.is_none() {
        return if !playing_file {
            PPC_BAD_FORMAT
        } else if aiff.is_some_and(|metadata| {
            ppc_sound_codec(
                FIXED_COMPRESSION,
                metadata.compression_type,
                metadata.sample_size,
            )
            .is_none()
        }) {
            -223 // siInvalidCompression
        } else {
            BAD_FILE_FORMAT
        };
    }
    let decoded_aiff_summary = decoded_sound.as_ref().map(|decoded| decoded.summary);
    if ppc_sound_trace_enabled() {
        let path = files
            .iter()
            .find(|file| file.ref_num == ref_num)
            .map(|file| file.path.as_str());
        eprintln!(
            "[PPC-SOUND] SndStartFilePlay channel=${:08X} ref={} resource={} path={:?} aiff={:?} decoded={:?} completion=${:08X} async={}",
            channel, ref_num, resource_id, path, aiff, decoded_aiff_summary, cpu.gpr[9], cpu.gpr[10]
        );
    }
    let record = PpcSoundFilePlaybackRecord {
        channel,
        ref_num,
        resource_id,
        buffer_size: cpu.gpr[6],
        buffer: cpu.gpr[7],
        selection: cpu.gpr[8],
        completion: cpu.gpr[9],
        completion_command: None,
        async_play: cpu.gpr[10] != 0,
        aiff,
        decoded_aiff: decoded_aiff_summary,
    };
    let completion = Some((
        CallbackTaskArchitecture::PowerPc,
        if record.async_play {
            record.completion
        } else {
            0
        },
    ));
    if let Some(decoded_sound) = decoded_sound {
        sound.manager.play_file_buffer(
            channel,
            decoded_sound.samples.clone(),
            decoded_sound.summary.sample_rate_fixed,
            completion,
        );
        if let Some(file_playback_index) = file_playback_index {
            sound
                .decoded_file_playbacks
                .push(PpcDecodedAiffPlaybackRecord {
                    file_playback_index,
                    channel,
                    sample_rate_fixed: decoded_sound.summary.sample_rate_fixed,
                    samples: decoded_sound.samples,
                });
        }
    } else {
        sound.manager.play_file_buffer(
            channel,
            Vec::new(),
            crate::sound::OUTPUT_RATE << 16,
            completion,
        );
    }
    sound.file_playbacks.push(record);
    sound.start_count = sound.start_count.saturating_add(1);
    PPC_NO_ERR
}

pub(crate) fn ppc_aiff_playback_for_refnum(
    ref_num: i16,
    files: &[PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> (Option<PpcAiffMetadata>, Option<PpcDecodedAiffData>) {
    let Some(data) = ppc_file_data_for_refnum(ref_num, files, vfs_files) else {
        return (None, None);
    };
    (ppc_parse_aiff_metadata(data), ppc_decode_aiff_samples(data))
}

fn ppc_file_data_for_refnum<'a>(
    ref_num: i16,
    files: &[PpcFileRecord],
    vfs_files: &'a [PpcVfsFileRecord],
) -> Option<&'a [u8]> {
    let path = files
        .iter()
        .find(|file| file.ref_num == ref_num)
        .map(|file| file.path.as_str())?;
    vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map(|file| file.data.as_slice())
}

/// Whether a data fork begins with an AIFF or AIFF-C FORM header.
fn ppc_is_aiff_form(data: &[u8]) -> bool {
    data.get(0..4) == Some(b"FORM".as_slice()) && matches!(data.get(8..12), Some(b"AIFF" | b"AIFC"))
}

pub(crate) fn ppc_snd_resource_playback_for_id(
    resource_id: i16,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcDecodedAiffData> {
    let index = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"snd "),
        resource_id,
        false,
    )?;
    ppc_decode_snd_resource_samples(&vfs_resources[index].data)
}

pub(crate) fn ppc_decode_aiff_samples(data: &[u8]) -> Option<PpcDecodedAiffData> {
    if let Some((samples, sample_rate_fixed)) = crate::trap::parse_aiff_samples(data) {
        return ppc_decoded_sound_data(samples, sample_rate_fixed);
    }
    ppc_decode_compressed_aifc_samples(data)
}

/// Decode an AIFF-C file whose COMM compressionType names a Sound Manager
/// codec (for example 'ima4' or 'MAC3'), as SndStartFilePlay streams them.
///
/// numSampleFrames counts sample frames (Inside Macintosh: Sound (1994),
/// p. 2-86), and a frame of compressed data holds one packet per channel
/// (p. 2-10), which is the frame unit `PpcSoundCodec` decodes. When
/// numSampleFrames claims more packets than the SSND chunk holds, the whole
/// packets present play (a partial trailing packet is dropped).
fn ppc_decode_compressed_aifc_samples(data: &[u8]) -> Option<PpcDecodedAiffData> {
    const AIFC: u32 = u32::from_be_bytes(*b"AIFC");

    let metadata = ppc_parse_aiff_metadata(data)?;
    if metadata.form_type != AIFC {
        return None;
    }
    let codec = ppc_sound_codec(
        FIXED_COMPRESSION,
        metadata.compression_type,
        metadata.sample_size,
    )?;
    if matches!(codec, PpcSoundCodec::Pcm(_)) {
        // parse_aiff_samples decodes 'NONE' and 'raw ' AIFF-C; other
        // uncompressed formats such as 'twos' are not supported here.
        return None;
    }
    let channels = usize::from(metadata.channel_count);
    let bytes_per_frame = codec.bytes_per_frame(channels)?;
    let start = usize::try_from(metadata.sound_data_offset).ok()?;
    let size = usize::try_from(metadata.sound_data_size).ok()?;
    let sound_data = data.get(start..start.checked_add(size)?)?;
    let frames = usize::try_from(metadata.sample_frame_count)
        .ok()?
        .min(sound_data.len() / bytes_per_frame);
    let samples = codec.to_mono_u8(sound_data, frames, channels)?;
    let sample_rate_fixed =
        (u64::from(metadata.sample_rate_hz) << 16).min(u64::from(u32::MAX)) as u32;
    ppc_decoded_sound_data(samples, sample_rate_fixed)
}

#[derive(Debug)]
pub(crate) enum PpcDecodedSndResourceCommand {
    Command(crate::sound::SndCommand),
    Buffer {
        samples: Vec<u8>,
        sample_rate_fixed: u32,
    },
}

pub(crate) fn ppc_decode_snd_resource_commands(
    data: &[u8],
) -> Option<Vec<PpcDecodedSndResourceCommand>> {
    const SOUND_CMD: u16 = 80;
    const BUFFER_CMD: u16 = 81;
    const DATA_OFFSET_FLAG: u16 = 0x8000;

    let format = ppc_read_be_u16_from_slice(data, 0)?;
    let mut command_offset = match format {
        1 => {
            let synth_count = usize::from(ppc_read_be_u16_from_slice(data, 2)?);
            4usize.checked_add(synth_count.checked_mul(6)?)?
        }
        2 => 4,
        _ => return None,
    };
    let command_count = usize::from(ppc_read_be_u16_from_slice(data, command_offset)?);
    command_offset = command_offset.checked_add(2)?;
    let commands_end = command_offset.checked_add(command_count.checked_mul(8)?)?;
    if commands_end > data.len() {
        return None;
    }

    let mut commands = Vec::with_capacity(command_count);
    let mut has_buffer = false;

    for index in 0..command_count {
        let command_ptr = command_offset.checked_add(index.checked_mul(8)?)?;
        let raw_command = ppc_read_be_u16_from_slice(data, command_ptr)?;
        let mut command = raw_command & !DATA_OFFSET_FLAG;
        let param1 = ppc_read_be_u16_from_slice(data, command_ptr.checked_add(2)?)? as i16;
        let parameter = ppc_read_be_u32_from_slice(data, command_ptr.checked_add(4)?)?;
        let has_data_offset = (raw_command & DATA_OFFSET_FLAG) != 0;
        if format == 2 && index == 0 && command == SOUND_CMD {
            command = BUFFER_CMD;
        }
        if has_data_offset && (command == BUFFER_CMD || command == SOUND_CMD) {
            let mut header_offset = parameter as usize;
            if format == 2
                && index == 0
                && command == BUFFER_CMD
                && header_offset == commands_end.checked_add(6)?
            {
                header_offset = commands_end;
            }
            let decoded = ppc_decode_snd_header_at(data, header_offset)?;
            has_buffer = true;
            commands.push(PpcDecodedSndResourceCommand::Buffer {
                samples: decoded.samples,
                sample_rate_fixed: decoded.summary.sample_rate_fixed,
            });
        } else {
            commands.push(PpcDecodedSndResourceCommand::Command(
                crate::sound::SndCommand {
                    cmd: command,
                    param1,
                    param2: parameter,
                },
            ));
        }
    }
    has_buffer.then_some(commands)
}

pub(crate) fn ppc_decode_snd_resource_samples(data: &[u8]) -> Option<PpcDecodedAiffData> {
    ppc_decode_snd_resource_commands(data)?.into_iter().find_map(|command| {
        let PpcDecodedSndResourceCommand::Buffer {
            samples,
            sample_rate_fixed,
        } = command
        else {
            return None;
        };
        ppc_decoded_sound_data(samples, sample_rate_fixed)
    })
}

pub(crate) fn ppc_decode_snd_header_at(data: &[u8], header_offset: usize) -> Option<PpcDecodedAiffData> {
    const STD_SH: u8 = 0x00;
    const CMP_SH: u8 = 0xfe;
    const EXT_SH: u8 = 0xff;

    let sample_ptr = ppc_read_be_u32_from_slice(data, header_offset)? as usize;
    let sample_rate_fixed = ppc_read_be_u32_from_slice(data, header_offset.checked_add(8)?)?;
    let encode = *data.get(header_offset.checked_add(20)?)?;
    let samples = match encode {
        STD_SH => {
            let length = ppc_read_be_u32_from_slice(data, header_offset.checked_add(4)?)? as usize;
            let data_start = if sample_ptr != 0 && sample_ptr < data.len() {
                sample_ptr
            } else {
                header_offset.checked_add(22)?
            };
            data.get(data_start..data_start.checked_add(length)?)?
                .to_vec()
        }
        EXT_SH => {
            let channels =
                ppc_read_be_u32_from_slice(data, header_offset.checked_add(4)?)? as usize;
            let frames = ppc_read_be_u32_from_slice(data, header_offset.checked_add(22)?)? as usize;
            let sample_size = usize::from(ppc_read_be_u16_from_slice(
                data,
                header_offset.checked_add(48)?,
            )?);
            let data_start = if sample_ptr != 0 && sample_ptr < data.len() {
                sample_ptr
            } else {
                header_offset.checked_add(64)?
            };
            ppc_decode_interleaved_pcm_samples(data, data_start, frames, channels, sample_size)?
        }
        CMP_SH => {
            let channels =
                ppc_read_be_u32_from_slice(data, header_offset.checked_add(4)?)? as usize;
            let frames = ppc_read_be_u32_from_slice(data, header_offset.checked_add(22)?)? as usize;
            let compression_format =
                ppc_read_be_u32_from_slice(data, header_offset.checked_add(40)?)?;
            let compression_id =
                ppc_read_be_u16_from_slice(data, header_offset.checked_add(56)?)? as i16;
            let packet_size = usize::from(ppc_read_be_u16_from_slice(
                data,
                header_offset.checked_add(58)?,
            )?);
            let sample_size = usize::from(ppc_read_be_u16_from_slice(
                data,
                header_offset.checked_add(62)?,
            )?);
            let codec =
                ppc_sound_codec(compression_id, compression_format, sample_size as u16)?;
            // A zero packetSize means the codec's standard packet.
            let packet_bits = usize::from(codec.packet_geometry().1) * 8;
            if packet_size != 0 && packet_size != packet_bits {
                return None;
            }
            let data_start = if sample_ptr != 0 && sample_ptr < data.len() {
                sample_ptr
            } else {
                header_offset.checked_add(64)?
            };
            let byte_count = frames.checked_mul(codec.bytes_per_frame(channels)?)?;
            let compressed = data.get(data_start..data_start.checked_add(byte_count)?)?;
            codec.to_mono_u8(compressed, frames, channels)?
        }
        _ => return None,
    };
    ppc_decoded_sound_data(samples, sample_rate_fixed)
}

pub(crate) fn ppc_decode_interleaved_pcm_samples(
    data: &[u8],
    data_start: usize,
    frames: usize,
    channels: usize,
    sample_size: usize,
) -> Option<Vec<u8>> {
    if frames == 0 || channels == 0 {
        return Some(Vec::new());
    }
    let bytes_per_sample = sample_size / 8;
    if bytes_per_sample == 0 {
        return None;
    }
    let frame_bytes = channels.checked_mul(bytes_per_sample)?;
    let byte_count = frames.checked_mul(frame_bytes)?;
    let raw = data.get(data_start..data_start.checked_add(byte_count)?)?;
    let mut samples = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut accum = 0i32;
        let frame_start = frame.checked_mul(frame_bytes)?;
        for channel in 0..channels {
            let offset = frame_start.checked_add(channel.checked_mul(bytes_per_sample)?)?;
            let sample = match sample_size {
                8 => raw[offset] as i32 - 128,
                16 => i16::from_be_bytes([raw[offset], raw[offset + 1]]) as i32 >> 8,
                _ => return None,
            };
            accum += sample;
        }
        samples.push((accum / channels as i32 + 128).clamp(0, 255) as u8);
    }
    Some(samples)
}

pub(crate) fn ppc_decoded_sound_data(samples: Vec<u8>, sample_rate_fixed: u32) -> Option<PpcDecodedAiffData> {
    let sample_count = u32::try_from(samples.len()).ok()?;
    let preview_len = samples.len().min(16);
    let mut preview = [0; 16];
    preview[..preview_len].copy_from_slice(&samples[..preview_len]);
    Some(PpcDecodedAiffData {
        summary: PpcDecodedAiffSamples {
            sample_rate_fixed,
            sample_count,
            preview_len: preview_len as u8,
            preview,
        },
        samples,
    })
}

pub(crate) struct PpcDecodedAiffData {
    pub(crate) summary: PpcDecodedAiffSamples,
    pub(crate) samples: Vec<u8>,
}

pub(crate) fn ppc_parse_aiff_metadata(data: &[u8]) -> Option<PpcAiffMetadata> {
    const FORM: u32 = u32::from_be_bytes(*b"FORM");
    const AIFF: u32 = u32::from_be_bytes(*b"AIFF");
    const AIFC: u32 = u32::from_be_bytes(*b"AIFC");
    const COMM: u32 = u32::from_be_bytes(*b"COMM");
    const SSND: u32 = u32::from_be_bytes(*b"SSND");
    const NONE: u32 = u32::from_be_bytes(*b"NONE");

    if ppc_read_be_u32_from_slice(data, 0)? != FORM {
        return None;
    }
    let form_size = ppc_read_be_u32_from_slice(data, 4)? as usize;
    let form_type = ppc_read_be_u32_from_slice(data, 8)?;
    if form_type != AIFF && form_type != AIFC {
        return None;
    }
    let form_end = 8usize.checked_add(form_size)?.min(data.len());
    let mut offset = 12usize;
    let mut channel_count = None;
    let mut sample_frame_count = None;
    let mut sample_size = None;
    let mut sample_rate_hz = None;
    let mut compression_type = if form_type == AIFF { Some(NONE) } else { None };
    let mut sound_data_offset = None;
    let mut sound_data_size = None;

    while offset.checked_add(8)? <= form_end {
        let chunk_id = ppc_read_be_u32_from_slice(data, offset)?;
        let chunk_size = ppc_read_be_u32_from_slice(data, offset + 4)? as usize;
        let chunk_data = offset.checked_add(8)?;
        let chunk_end = chunk_data.checked_add(chunk_size)?;
        if chunk_end > data.len() {
            return None;
        }
        match chunk_id {
            COMM if chunk_size >= 18 => {
                channel_count = Some(ppc_read_be_u16_from_slice(data, chunk_data)?);
                sample_frame_count = Some(ppc_read_be_u32_from_slice(data, chunk_data + 2)?);
                sample_size = Some(ppc_read_be_u16_from_slice(data, chunk_data + 6)?);
                sample_rate_hz = Some(ppc_read_extended_sample_rate(data, chunk_data + 8)?);
                if form_type == AIFC && chunk_size >= 22 {
                    compression_type = Some(ppc_read_be_u32_from_slice(data, chunk_data + 18)?);
                }
            }
            SSND if chunk_size >= 8 => {
                let offset_to_sound = ppc_read_be_u32_from_slice(data, chunk_data)? as usize;
                let data_start = chunk_data.checked_add(8)?.checked_add(offset_to_sound)?;
                if data_start > chunk_end {
                    return None;
                }
                sound_data_offset = Some(u32::try_from(data_start).ok()?);
                sound_data_size = Some(u32::try_from(chunk_end - data_start).ok()?);
            }
            _ => {}
        }
        offset = chunk_end.checked_add(chunk_size & 1)?;
    }

    Some(PpcAiffMetadata {
        form_type,
        channel_count: channel_count?,
        sample_frame_count: sample_frame_count?,
        sample_size: sample_size?,
        sample_rate_hz: sample_rate_hz?,
        compression_type: compression_type?,
        sound_data_offset: sound_data_offset?,
        sound_data_size: sound_data_size?,
    })
}

pub(crate) fn ppc_read_be_u16_from_slice(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset.checked_add(1)?)?,
    ]))
}

pub(crate) fn ppc_read_be_u32_from_slice(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset.checked_add(1)?)?,
        *data.get(offset.checked_add(2)?)?,
        *data.get(offset.checked_add(3)?)?,
    ]))
}

pub(crate) fn ppc_read_extended_sample_rate(data: &[u8], offset: usize) -> Option<u32> {
    let exponent = ppc_read_be_u16_from_slice(data, offset)?;
    let sign = exponent & 0x8000 != 0;
    let exponent = i32::from(exponent & 0x7fff);
    if sign || exponent == 0 {
        return Some(0);
    }
    let mut mantissa = 0u64;
    for byte_offset in 0..8usize {
        mantissa = (mantissa << 8) | u64::from(*data.get(offset.checked_add(2 + byte_offset)?)?);
    }
    let value = (mantissa as f64) * 2f64.powi(exponent - 16383 - 63);
    Some(value.round().clamp(0.0, f64::from(u32::MAX)) as u32)
}

pub(crate) fn ppc_snd_pause_file_play(cpu: &mut PpcCpu, sound: &mut PpcSoundState) -> i16 {
    let channel = cpu.gpr[3];
    sound.pause_count = sound.pause_count.saturating_add(1);
    sound.manager.toggle_file_paused(channel);
    PPC_NO_ERR
}

pub(crate) fn ppc_snd_stop_file_play(cpu: &mut PpcCpu, sound: &mut PpcSoundState) -> i16 {
    let channel = cpu.gpr[3];
    let _quiet_now = cpu.gpr[4] != 0;
    sound.stop_count = sound.stop_count.saturating_add(1);
    sound.manager.quiet_channel(channel);
    PPC_NO_ERR
}

pub(crate) fn ppc_get_sound_header_offset(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let handle = cpu.gpr[3];
    let offset_ptr = cpu.gpr[4];
    if offset_ptr == 0 || !ppc_memory_can_write_bytes(memory, offset_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let Some(resource_ptr) = memory.read_u32_be(handle) else {
        let _ = memory.write_u32_be(offset_ptr, 0);
        return PPC_PARAM_ERR;
    };
    match ppc_sound_header_offset(memory, resource_ptr) {
        Some(offset) if memory.write_u32_be(offset_ptr, offset).is_some() => PPC_NO_ERR,
        Some(_) => PPC_PARAM_ERR,
        None => {
            let _ = memory.write_u32_be(offset_ptr, 0);
            PPC_BAD_FORMAT
        }
    }
}

pub(crate) fn ppc_sound_header_offset(memory: &mut PpcSectionMem, resource_ptr: u32) -> Option<u32> {
    const PPC_SOUND_CMD_SOUND: u16 = 80;
    const PPC_SOUND_CMD_BUFFER: u16 = 81;
    const PPC_SOUND_DATA_OFFSET_FLAG: u16 = 0x8000;

    let format = memory.read_u16_be(resource_ptr)?;
    let mut command_offset = match format {
        1 => {
            let synth_count = memory.read_u16_be(resource_ptr + 2)? as u32;
            4u32.checked_add(synth_count.checked_mul(6)?)?
        }
        2 => 4,
        _ => return None,
    };
    let command_count = memory.read_u16_be(resource_ptr.checked_add(command_offset)?)? as u32;
    command_offset = command_offset.checked_add(2)?;
    for index in 0..command_count {
        let cmd_offset = command_offset.checked_add((index as u32).checked_mul(8)?)?;
        let cmd = memory.read_u16_be(resource_ptr.checked_add(cmd_offset)?)?;
        if cmd == (PPC_SOUND_CMD_SOUND | PPC_SOUND_DATA_OFFSET_FLAG)
            || cmd == (PPC_SOUND_CMD_BUFFER | PPC_SOUND_DATA_OFFSET_FLAG)
        {
            return memory.read_u32_be(resource_ptr.checked_add(cmd_offset.checked_add(4)?)?);
        }
    }
    None
}
