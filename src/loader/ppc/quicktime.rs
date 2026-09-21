//! QuickTime and Movie Media state and tracking records.

use super::{
    ppc_i16_result, ppc_qt_record_error, ppc_qt_reset_movie_video_decode_cache, PpcCpu,
    PpcImportAction, PpcSectionMem, PPC_PARAM_ERR, PPC_QT_MOVIE,
};
use ppc::PpcMemory;

pub const PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE: u32 = 3;
pub const PPC_NO_ERR: i16 = 0;

/// Width of the synthesized PowerPC main screen. Reads the active machine
/// profile so a `SYSTEMLESS_SCREEN_WIDTH` override reaches the GDevice,
/// window, and DrawSprocket geometry the guest sees.
pub fn ppc_main_screen_width() -> u32 {
    u32::from(crate::machine_profile::reference_machine_profile().screen_width)
}

/// Height of the synthesized PowerPC main screen. See
/// [`ppc_main_screen_width`].
pub fn ppc_main_screen_height() -> u32 {
    u32::from(crate::machine_profile::reference_machine_profile().screen_height)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQuickTimeCompatibilityOperation {
    GetMovieTimeBase,
    GetMovieVolume,
    NewMovieFromDataFork,
    PrerollMovie,
    SetMovieVolume,
    SetTimeBaseFlags,
    UpdateMovie,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcQuickTimeCinepakStripState {
    pub(crate) v4_codebook: [[u8; 12]; 256],
    pub(crate) v1_codebook: [[u8; 12]; 256],
}

impl Default for PpcQuickTimeCinepakStripState {
    fn default() -> Self {
        Self {
            v4_codebook: [[0; 12]; 256],
            v1_codebook: [[0; 12]; 256],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQuickTimeVideoTrackRecord {
    pub media_time_scale: u32,
    pub media_duration: u64,
    pub sample_count: u32,
    pub first_sample_duration: u32,
    pub first_sample_size: u32,
    pub first_chunk_offset: u64,
    pub first_sample_data_len: u32,
    pub first_sample_checksum: u32,
    pub first_sample_preview_len: u8,
    pub first_sample_preview: [u8; 16],
    pub first_samples_per_chunk: u32,
    pub sample_description_id: u32,
    pub codec: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQuickTimeVideoSampleRecord {
    pub offset: u64,
    pub size: u32,
    pub media_start_time: u64,
    pub duration: u32,
    pub data_len: u32,
    pub checksum: u32,
    pub preview_len: u8,
    pub preview: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQuickTimeVideoSampleTableRecord {
    pub media_time_scale: u32,
    pub media_duration: u64,
    pub sample_count: u32,
    pub codec: u32,
    pub samples: Vec<PpcQuickTimeVideoSampleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQuickTimeVideoDecodeCacheRecord {
    pub(crate) codec: u32,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) sample_index: usize,
    pub(crate) rgb: Vec<u8>,
    pub(crate) cinepak_strips: Vec<PpcQuickTimeCinepakStripState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQuickTimeAudioTrackRecord {
    pub media_time_scale: u32,
    pub media_duration: u64,
    pub sample_count: u32,
    pub first_sample_duration: u32,
    pub first_sample_size: u32,
    pub first_chunk_offset: u64,
    pub first_sample_data_len: u32,
    pub first_sample_checksum: u32,
    pub first_sample_preview_len: u8,
    pub first_sample_preview: [u8; 16],
    pub first_samples_per_chunk: u32,
    pub sample_description_id: u32,
    pub codec: u32,
    pub channel_count: u16,
    pub sample_size_bits: u16,
    pub sample_rate_fixed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQuickTimeState {
    pub movie_toolbox_enter_count: u32,
    pub movie_toolbox_exit_count: u32,
    pub movie_toolbox_init_depth: u32,
    pub movie_error: i16,
    pub movie_sticky_error: i16,
    pub movie_file_open_count: u32,
    pub movie_file_close_count: u32,
    pub movie_file_ref_num: i16,
    pub movie_file_last_closed_ref_num: i16,
    pub movie_file_path: String,
    pub movie_file_data: Vec<u8>,
    pub movie_file_bounds: Option<(i16, i16, i16, i16)>,
    pub movie_file_time_scale: u32,
    pub movie_file_duration: u64,
    pub movie_file_tasks_until_done: u32,
    pub movie_file_video_track: Option<PpcQuickTimeVideoTrackRecord>,
    pub movie_file_video_samples: Option<PpcQuickTimeVideoSampleTableRecord>,
    pub movie_file_audio_track: Option<PpcQuickTimeAudioTrackRecord>,
    pub graphics_importer_gworld: u32,
    pub graphics_importer_gdevice: u32,
    pub graphics_importer_open: bool,
    pub graphics_import_draw_count: u32,
    pub graphics_import_source_draw_count: u32,
    pub graphics_importer_path: String,
    pub graphics_importer_data: Vec<u8>,
    pub graphics_importer_bounds: Option<(i16, i16, i16, i16)>,
    pub movie_gworld: u32,
    pub movie_gdevice: u32,
    pub movie_box: (i16, i16, i16, i16),
    pub movie_set_box_count: u32,
    pub movie_beginning_count: u32,
    pub movie_at_beginning: bool,
    pub movie_started: bool,
    pub movie_task_count: u32,
    pub movie_tasks_until_done: u32,
    pub movie_video_track: Option<PpcQuickTimeVideoTrackRecord>,
    pub movie_video_samples: Option<PpcQuickTimeVideoSampleTableRecord>,
    pub movie_video_decode_cache: Option<PpcQuickTimeVideoDecodeCacheRecord>,
    pub movie_audio_track: Option<PpcQuickTimeAudioTrackRecord>,
    pub movie_disposed: bool,
    pub movie_volume: i16,
    pub movie_time_base_flags: u32,
}

impl Default for PpcQuickTimeState {
    fn default() -> Self {
        Self {
            movie_toolbox_enter_count: 0,
            movie_toolbox_exit_count: 0,
            movie_toolbox_init_depth: 0,
            movie_error: PPC_NO_ERR,
            movie_sticky_error: PPC_NO_ERR,
            movie_file_open_count: 0,
            movie_file_close_count: 0,
            movie_file_ref_num: 0,
            movie_file_last_closed_ref_num: 0,
            movie_file_path: String::new(),
            movie_file_data: Vec::new(),
            movie_file_bounds: None,
            movie_file_time_scale: 0,
            movie_file_duration: 0,
            movie_file_tasks_until_done: PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE,
            movie_file_video_track: None,
            movie_file_video_samples: None,
            movie_file_audio_track: None,
            graphics_importer_gworld: 0,
            graphics_importer_gdevice: 0,
            graphics_importer_open: false,
            graphics_import_draw_count: 0,
            graphics_import_source_draw_count: 0,
            graphics_importer_path: String::new(),
            graphics_importer_data: Vec::new(),
            graphics_importer_bounds: None,
            movie_gworld: 0,
            movie_gdevice: 0,
            movie_box: (
                0,
                0,
                ppc_main_screen_height() as i16,
                ppc_main_screen_width() as i16,
            ),
            movie_set_box_count: 0,
            movie_beginning_count: 0,
            movie_at_beginning: true,
            movie_started: false,
            movie_task_count: 0,
            movie_tasks_until_done: PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE,
            movie_video_track: None,
            movie_video_samples: None,
            movie_video_decode_cache: None,
            movie_audio_track: None,
            movie_disposed: false,
            movie_volume: 0x0100,
            movie_time_base_flags: 0,
        }
    }
}

fn compatibility_valid_movie(quicktime: &PpcQuickTimeState, movie: u32) -> bool {
    movie == PPC_QT_MOVIE && !quicktime.movie_disposed
}

pub(super) fn dispatch_quicktime_compatibility(
    operation: PpcQuickTimeCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
) -> PpcImportAction {
    const PPC_QT_TIME_BASE: u32 = PPC_QT_MOVIE + 0x10;
    match operation {
        PpcQuickTimeCompatibilityOperation::GetMovieTimeBase => {
            PpcImportAction::Return(if compatibility_valid_movie(quicktime, cpu.gpr[3]) {
                PPC_QT_TIME_BASE
            } else {
                0
            })
        }
        PpcQuickTimeCompatibilityOperation::GetMovieVolume => {
            PpcImportAction::Return(if compatibility_valid_movie(quicktime, cpu.gpr[3]) {
                quicktime.movie_volume as u16 as u32
            } else {
                0
            })
        }
        PpcQuickTimeCompatibilityOperation::SetMovieVolume => {
            if compatibility_valid_movie(quicktime, cpu.gpr[3]) {
                quicktime.movie_volume = cpu.gpr[4] as u16 as i16;
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickTimeCompatibilityOperation::SetTimeBaseFlags => {
            if cpu.gpr[3] == PPC_QT_TIME_BASE {
                quicktime.movie_time_base_flags = cpu.gpr[4];
            }
            PpcImportAction::ReturnPreserve
        }
        PpcQuickTimeCompatibilityOperation::PrerollMovie => {
            let result = if compatibility_valid_movie(quicktime, cpu.gpr[3]) {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(ppc_qt_record_error(quicktime, result)))
        }
        PpcQuickTimeCompatibilityOperation::UpdateMovie => {
            let result = if compatibility_valid_movie(quicktime, cpu.gpr[3]) {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            let _ = ppc_qt_record_error(quicktime, result);
            PpcImportAction::ReturnPreserve
        }
        PpcQuickTimeCompatibilityOperation::NewMovieFromDataFork => {
            let movie_out = cpu.gpr[3];
            let changed_out = cpu.gpr[7];
            if movie_out == 0
                || memory.write_u32_be(movie_out, PPC_QT_MOVIE).is_none()
                || (changed_out != 0 && memory.write_u8(changed_out, 0).is_none())
            {
                return PpcImportAction::Return(ppc_i16_result(ppc_qt_record_error(
                    quicktime,
                    PPC_PARAM_ERR,
                )));
            }
            if cpu.gpr[4] as u16 as i16 == quicktime.movie_file_ref_num {
                if let Some(bounds) = quicktime.movie_file_bounds {
                    quicktime.movie_box = bounds;
                }
                quicktime.movie_tasks_until_done = quicktime.movie_file_tasks_until_done;
                quicktime.movie_video_track = quicktime.movie_file_video_track;
                quicktime.movie_video_samples = quicktime.movie_file_video_samples.clone();
                quicktime.movie_audio_track = quicktime.movie_file_audio_track;
            } else {
                quicktime.movie_tasks_until_done = PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE;
                quicktime.movie_video_track = None;
                quicktime.movie_video_samples = None;
                quicktime.movie_audio_track = None;
            }
            quicktime.movie_started = false;
            quicktime.movie_task_count = 0;
            quicktime.movie_disposed = false;
            quicktime.movie_at_beginning = true;
            ppc_qt_reset_movie_video_decode_cache(quicktime);
            PpcImportAction::Return(ppc_i16_result(ppc_qt_record_error(quicktime, PPC_NO_ERR)))
        }
    }
}
