//! QuickTime and Movie Media state and tracking records.

use crate::machine_profile::REFERENCE_MACHINE_PROFILE;

pub const PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE: u32 = 3;
pub const PPC_MAIN_SCREEN_WIDTH: u32 = REFERENCE_MACHINE_PROFILE.screen_width as u32;
pub const PPC_MAIN_SCREEN_HEIGHT: u32 = REFERENCE_MACHINE_PROFILE.screen_height as u32;
pub const PPC_NO_ERR: i16 = 0;

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
                PPC_MAIN_SCREEN_HEIGHT as i16,
                PPC_MAIN_SCREEN_WIDTH as i16,
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
