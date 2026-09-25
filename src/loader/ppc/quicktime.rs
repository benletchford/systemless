//! QuickTime and Movie Media state and tracking records.

use super::{
    format_ppc_fourcc, ppc_decoded_sound_data, ppc_draw_pict_bytes_to_16bpp,
    ppc_existing_path_for_fsspec, ppc_i16_result, ppc_live_front_buffer_for_gworld,
    ppc_memory_can_write_bytes, ppc_q3_write_software_pixel, ppc_read_be_u32_from_slice,
    ppc_read_rect, ppc_write_rect, qt_trace_enabled, PpcCpu, PpcDecodedAiffData,
    PpcDecodedAiffPlaybackRecord, PpcFrontBuffer, PpcGWorldRecord, PpcImportAction,
    PpcSectionMem, PpcSoundFilePlaybackRecord, PpcSoundState, PpcVfsDirectory, PpcVfsFileRecord,
    PpcVfsResourceFileRecord, PpcVfsResourceRecord, PPC_FIRST_FILE_REF_NUM,
    PPC_INVALID_COMPONENT_ID, PPC_PARAM_ERR, PPC_QT_GRAPHICS_IMPORTER, PPC_QT_MOVIE,
    PPC_QT_MOVIE_TASKS_PER_SECOND, PPC_RES_NOT_FOUND_ERR,
};
use crate::managers::resource::ResourceFork;
use crate::trap::TrapDispatcher;
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

pub(crate) fn ppc_qt_record_error(quicktime: &mut PpcQuickTimeState, error: i16) -> i16 {
    quicktime.movie_error = error;
    if error != PPC_NO_ERR && quicktime.movie_sticky_error == PPC_NO_ERR {
        quicktime.movie_sticky_error = error;
    }
    error
}

pub(crate) fn ppc_qt_enter_movies(quicktime: &mut PpcQuickTimeState) -> i16 {
    quicktime.movie_toolbox_enter_count = quicktime.movie_toolbox_enter_count.saturating_add(1);
    quicktime.movie_toolbox_init_depth = quicktime.movie_toolbox_init_depth.saturating_add(1);
    ppc_qt_record_error(quicktime, PPC_NO_ERR)
}

pub(crate) fn ppc_qt_exit_movies(quicktime: &mut PpcQuickTimeState) {
    quicktime.movie_toolbox_exit_count = quicktime.movie_toolbox_exit_count.saturating_add(1);
    quicktime.movie_toolbox_init_depth = quicktime.movie_toolbox_init_depth.saturating_sub(1);
    let _ = ppc_qt_record_error(quicktime, PPC_NO_ERR);
}

pub(crate) fn ppc_qt_get_movies_error(quicktime: &mut PpcQuickTimeState) -> i16 {
    let error = quicktime.movie_error;
    quicktime.movie_error = PPC_NO_ERR;
    error
}

pub(crate) fn ppc_qt_get_movies_sticky_error(quicktime: &mut PpcQuickTimeState) -> i16 {
    let error = quicktime.movie_sticky_error;
    let _ = ppc_qt_record_error(quicktime, PPC_NO_ERR);
    error
}

pub(crate) fn ppc_qt_clear_movies_sticky_error(quicktime: &mut PpcQuickTimeState) {
    quicktime.movie_sticky_error = PPC_NO_ERR;
    let _ = ppc_qt_record_error(quicktime, PPC_NO_ERR);
}

fn ppc_qt_clear_graphics_importer(quicktime: &mut PpcQuickTimeState) {
    quicktime.graphics_importer_open = false;
    quicktime.graphics_importer_gworld = 0;
    quicktime.graphics_importer_gdevice = 0;
    quicktime.graphics_importer_path.clear();
    quicktime.graphics_importer_data.clear();
    quicktime.graphics_importer_bounds = None;
}

pub(crate) fn ppc_close_component(cpu: &mut PpcCpu, quicktime: &mut PpcQuickTimeState) -> i16 {
    if cpu.gpr[3] == PPC_QT_GRAPHICS_IMPORTER && quicktime.graphics_importer_open {
        ppc_qt_clear_graphics_importer(quicktime);
        PPC_NO_ERR
    } else {
        PPC_INVALID_COMPONENT_ID
    }
}

pub(crate) fn ppc_qt_get_graphics_importer_for_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    quicktime: &mut PpcQuickTimeState,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let importer_out_ptr = cpu.gpr[4];
    if importer_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, importer_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    if memory
        .write_u32_be(importer_out_ptr, PPC_QT_GRAPHICS_IMPORTER)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }

    ppc_qt_clear_graphics_importer(quicktime);
    quicktime.graphics_importer_open = true;
    if spec_ptr == 0 {
        return PPC_NO_ERR;
    }

    let Ok(path) = ppc_existing_path_for_fsspec(memory, vfs_directories, vfs_files, &[], spec_ptr)
    else {
        return PPC_NO_ERR;
    };
    let Some(file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    else {
        if qt_trace_enabled() {
            eprintln!(
                "[QT-TRACE] GetGraphicsImporterForFile path='{}' missing",
                path
            );
        }
        return PPC_NO_ERR;
    };
    quicktime.graphics_importer_path = path;
    quicktime.graphics_importer_data = file.data.to_vec();
    quicktime.graphics_importer_bounds = ppc_qt_pict_bounds(&quicktime.graphics_importer_data);
    if qt_trace_enabled() {
        eprintln!(
            "[QT-TRACE] GetGraphicsImporterForFile path='{}' data_len={} bounds={:?}",
            quicktime.graphics_importer_path,
            quicktime.graphics_importer_data.len(),
            quicktime.graphics_importer_bounds
        );
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_graphics_import_get_bounds_rect(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &PpcQuickTimeState,
) -> i16 {
    let bounds_ptr = cpu.gpr[4];
    if cpu.gpr[3] != PPC_QT_GRAPHICS_IMPORTER
        || !quicktime.graphics_importer_open
        || bounds_ptr == 0
    {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, bounds_ptr, 8) {
        return PPC_PARAM_ERR;
    }
    let (top, left, bottom, right) = quicktime.graphics_importer_bounds.unwrap_or((
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    ));
    if ppc_write_rect(memory, bounds_ptr, top, left, bottom, right).is_none() {
        PPC_PARAM_ERR
    } else {
        PPC_NO_ERR
    }
}

pub(crate) fn ppc_qt_graphics_import_set_gworld(cpu: &mut PpcCpu, quicktime: &mut PpcQuickTimeState) -> i16 {
    if cpu.gpr[3] != PPC_QT_GRAPHICS_IMPORTER || !quicktime.graphics_importer_open {
        return PPC_PARAM_ERR;
    }
    quicktime.graphics_importer_gworld = cpu.gpr[4];
    quicktime.graphics_importer_gdevice = cpu.gpr[5];
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_graphics_import_draw(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_GRAPHICS_IMPORTER || !quicktime.graphics_importer_open {
        return PPC_PARAM_ERR;
    }
    let gworld = if quicktime.graphics_importer_gworld != 0 {
        quicktime.graphics_importer_gworld
    } else {
        current_gworld
    };
    let Some(front_buffer) = ppc_live_front_buffer_for_gworld(memory, gworlds, gworld) else {
        if qt_trace_enabled() {
            eprintln!(
                "[QT-TRACE] GraphicsImportDraw path='{}' gworld=${:08X} result=paramErr no-front-buffer",
                quicktime.graphics_importer_path, gworld
            );
        }
        return PPC_PARAM_ERR;
    };
    quicktime.graphics_import_draw_count = quicktime.graphics_import_draw_count.saturating_add(1);
    let source_drawn =
        ppc_qt_draw_pict_source_to_16bpp(memory, front_buffer, &quicktime.graphics_importer_data);
    if source_drawn {
        quicktime.graphics_import_source_draw_count = quicktime
            .graphics_import_source_draw_count
            .saturating_add(1);
        if qt_trace_enabled() {
            eprintln!(
                "[QT-TRACE] GraphicsImportDraw path='{}' gworld=${:08X} data_len={} source=true fallback=false draw_count={} source_count={}",
                quicktime.graphics_importer_path,
                gworld,
                quicktime.graphics_importer_data.len(),
                quicktime.graphics_import_draw_count,
                quicktime.graphics_import_source_draw_count
            );
        }
        return PPC_NO_ERR;
    }
    let fallback_drawn = ppc_qt_draw_visible_16bpp_frame(
        memory,
        front_buffer,
        0x10u32.saturating_add(quicktime.graphics_import_draw_count),
    );
    if qt_trace_enabled() {
        eprintln!(
            "[QT-TRACE] GraphicsImportDraw path='{}' gworld=${:08X} data_len={} source=false fallback={} draw_count={} source_count={}",
            quicktime.graphics_importer_path,
            gworld,
            quicktime.graphics_importer_data.len(),
            fallback_drawn,
            quicktime.graphics_import_draw_count,
            quicktime.graphics_import_source_draw_count
        );
    }
    if fallback_drawn {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    }
}

pub(crate) fn ppc_qt_pict_record_offset_and_bounds(data: &[u8]) -> Option<(usize, (i16, i16, i16, i16))> {
    [0usize, 512usize].into_iter().find_map(|offset| {
        let header = data.get(offset..offset.checked_add(10)?)?;
        let top = i16::from_be_bytes(header.get(2..4)?.try_into().ok()?);
        let left = i16::from_be_bytes(header.get(4..6)?.try_into().ok()?);
        let bottom = i16::from_be_bytes(header.get(6..8)?.try_into().ok()?);
        let right = i16::from_be_bytes(header.get(8..10)?.try_into().ok()?);
        let width = i32::from(right) - i32::from(left);
        let height = i32::from(bottom) - i32::from(top);
        (width > 0 && height > 0 && width <= 8192 && height <= 8192)
            .then_some((offset, (top, left, bottom, right)))
    })
}

pub(crate) fn ppc_qt_pict_bounds(data: &[u8]) -> Option<(i16, i16, i16, i16)> {
    ppc_qt_pict_record_offset_and_bounds(data).map(|(_, bounds)| bounds)
}


pub(crate) fn ppc_qt_movie_bounds(data: &[u8]) -> Option<(i16, i16, i16, i16)> {
    ppc_qt_scan_movie_bounds(data, 0, data.len(), 0)
}

pub(crate) fn ppc_qt_movie_duration(data: &[u8]) -> Option<(u32, u64)> {
    ppc_qt_scan_movie_duration(data, 0, data.len(), 0)
}

pub(crate) fn ppc_qt_movie_first_video_track(data: &[u8]) -> Option<PpcQuickTimeVideoTrackRecord> {
    ppc_qt_scan_movie_video_track(data, 0, data.len(), 0)
}

pub(crate) fn ppc_qt_movie_video_samples(data: &[u8]) -> Option<PpcQuickTimeVideoSampleTableRecord> {
    ppc_qt_scan_movie_video_samples(data, 0, data.len(), 0)
}

pub(crate) fn ppc_qt_movie_first_audio_track(data: &[u8]) -> Option<PpcQuickTimeAudioTrackRecord> {
    ppc_qt_scan_movie_audio_track(data, 0, data.len(), 0)
}

fn ppc_qt_atom_range(data: &[u8], offset: usize, end: usize) -> Option<(&[u8], usize, usize)> {
    if offset.checked_add(8)? > end || end > data.len() {
        return None;
    }
    let size = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
    let atom_type = data.get(offset + 4..offset + 8)?;
    let (content_start, atom_end) = if size == 1 {
        let extended_size = u64::from_be_bytes(data.get(offset + 8..offset + 16)?.try_into().ok()?);
        let extended_size = usize::try_from(extended_size).ok()?;
        if extended_size < 16 {
            return None;
        }
        (offset + 16, offset.checked_add(extended_size)?)
    } else if size == 0 {
        (offset + 8, end)
    } else {
        let size = usize::try_from(size).ok()?;
        if size < 8 {
            return None;
        }
        (offset + 8, offset.checked_add(size)?)
    };
    if atom_end > end || content_start > atom_end {
        return None;
    }
    Some((atom_type, content_start, atom_end))
}

fn ppc_qt_scan_movie_bounds(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<(i16, i16, i16, i16)> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    let mut bounds = None;
    while offset.checked_add(8)? <= end {
        let size = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
        let atom_type = data.get(offset + 4..offset + 8)?;
        let (content_start, atom_end) = if size == 1 {
            let extended_size =
                u64::from_be_bytes(data.get(offset + 8..offset + 16)?.try_into().ok()?);
            let extended_size = usize::try_from(extended_size).ok()?;
            if extended_size < 16 {
                return bounds;
            }
            (offset + 16, offset.checked_add(extended_size)?)
        } else if size == 0 {
            (offset + 8, end)
        } else {
            let size = usize::try_from(size).ok()?;
            if size < 8 {
                return bounds;
            }
            (offset + 8, offset.checked_add(size)?)
        };
        if atom_end > end || content_start > atom_end {
            return bounds;
        }

        if atom_type == b"tkhd" {
            bounds = ppc_qt_union_bounds(bounds, ppc_qt_tkhd_bounds(data, content_start, atom_end));
        } else if ppc_qt_is_container_atom(atom_type) {
            bounds = ppc_qt_union_bounds(
                bounds,
                ppc_qt_scan_movie_bounds(data, content_start, atom_end, depth + 1),
            );
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    bounds
}

fn ppc_qt_scan_movie_duration(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<(u32, u64)> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let size = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
        let atom_type = data.get(offset + 4..offset + 8)?;
        let (content_start, atom_end) = if size == 1 {
            let extended_size =
                u64::from_be_bytes(data.get(offset + 8..offset + 16)?.try_into().ok()?);
            let extended_size = usize::try_from(extended_size).ok()?;
            if extended_size < 16 {
                return None;
            }
            (offset + 16, offset.checked_add(extended_size)?)
        } else if size == 0 {
            (offset + 8, end)
        } else {
            let size = usize::try_from(size).ok()?;
            if size < 8 {
                return None;
            }
            (offset + 8, offset.checked_add(size)?)
        };
        if atom_end > end || content_start > atom_end {
            return None;
        }

        if atom_type == b"mvhd" {
            if let Some(duration) = ppc_qt_mvhd_duration(data, content_start, atom_end) {
                return Some(duration);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(duration) =
                ppc_qt_scan_movie_duration(data, content_start, atom_end, depth + 1)
            {
                return Some(duration);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

#[derive(Default)]
struct PpcQuickTimeTrackSampleInfo {
    handler_subtype: u32,
    media_time_scale: u32,
    media_duration: u64,
    sample_count: u32,
    first_sample_duration: u32,
    first_sample_size: u32,
    first_chunk_offset: u64,
    first_samples_per_chunk: u32,
    sample_description_id: u32,
    codec: u32,
    audio_channel_count: u16,
    audio_sample_size_bits: u16,
    audio_sample_rate_fixed: u32,
}

impl PpcQuickTimeTrackSampleInfo {
    fn is_complete_for(&self, handler_subtype: u32) -> bool {
        self.handler_subtype == handler_subtype
            && self.media_time_scale != 0
            && self.media_duration != 0
            && self.sample_count != 0
            && self.first_sample_duration != 0
            && self.first_sample_size != 0
            && self.first_samples_per_chunk != 0
            && self.sample_description_id != 0
            && self.codec != 0
    }

    fn into_video_track(self, data: &[u8]) -> Option<PpcQuickTimeVideoTrackRecord> {
        if !self.is_complete_for(u32::from_be_bytes(*b"vide")) {
            return None;
        }
        let (
            first_sample_data_len,
            first_sample_checksum,
            first_sample_preview_len,
            first_sample_preview,
        ) = ppc_qt_sample_payload_summary(data, self.first_chunk_offset, self.first_sample_size);
        Some(PpcQuickTimeVideoTrackRecord {
            media_time_scale: self.media_time_scale,
            media_duration: self.media_duration,
            sample_count: self.sample_count,
            first_sample_duration: self.first_sample_duration,
            first_sample_size: self.first_sample_size,
            first_chunk_offset: self.first_chunk_offset,
            first_sample_data_len,
            first_sample_checksum,
            first_sample_preview_len,
            first_sample_preview,
            first_samples_per_chunk: self.first_samples_per_chunk,
            sample_description_id: self.sample_description_id,
            codec: self.codec,
        })
    }

    fn into_audio_track(self, data: &[u8]) -> Option<PpcQuickTimeAudioTrackRecord> {
        if !self.is_complete_for(u32::from_be_bytes(*b"soun"))
            || self.audio_channel_count == 0
            || self.audio_sample_size_bits == 0
            || self.audio_sample_rate_fixed == 0
        {
            return None;
        }
        let (
            first_sample_data_len,
            first_sample_checksum,
            first_sample_preview_len,
            first_sample_preview,
        ) = ppc_qt_sample_payload_summary(data, self.first_chunk_offset, self.first_sample_size);
        Some(PpcQuickTimeAudioTrackRecord {
            media_time_scale: self.media_time_scale,
            media_duration: self.media_duration,
            sample_count: self.sample_count,
            first_sample_duration: self.first_sample_duration,
            first_sample_size: self.first_sample_size,
            first_chunk_offset: self.first_chunk_offset,
            first_sample_data_len,
            first_sample_checksum,
            first_sample_preview_len,
            first_sample_preview,
            first_samples_per_chunk: self.first_samples_per_chunk,
            sample_description_id: self.sample_description_id,
            codec: self.codec,
            channel_count: self.audio_channel_count,
            sample_size_bits: self.audio_sample_size_bits,
            sample_rate_fixed: self.audio_sample_rate_fixed,
        })
    }
}

#[derive(Default)]
struct PpcQuickTimeAudioDecodeInfo {
    handler_subtype: u32,
    sample_count: u32,
    codec: u32,
    channel_count: u16,
    sample_size_bits: u16,
    sample_rate_fixed: u32,
    stsc_entries: Vec<PpcQuickTimeStscEntry>,
    chunk_offsets: Vec<u64>,
}

#[derive(Default)]
pub(crate) struct PpcQuickTimeMusicDecodeInfo {
    pub(crate) handler_subtype: u32,
    pub(crate) media_time_scale: u32,
    pub(crate) media_duration: u64,
    pub(crate) sample_durations: Vec<u32>,
    pub(crate) sample_sizes: Vec<u32>,
    pub(crate) stsc_entries: Vec<PpcQuickTimeStscEntry>,
    pub(crate) chunk_offsets: Vec<u64>,
    pub(crate) sample_descriptions: Vec<Vec<PpcQuickTimeMusicPart>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcQuickTimeMusicPart {
    pub(crate) part: u16,
    pub(crate) instrument_number: u32,
    pub(crate) gm_number: u32,
}

#[derive(Default)]
struct PpcQuickTimeVideoSampleTableInfo {
    handler_subtype: u32,
    media_time_scale: u32,
    media_duration: u64,
    codec: u32,
    sample_durations: Vec<u32>,
    sample_sizes: Vec<u32>,
    stsc_entries: Vec<PpcQuickTimeStscEntry>,
    chunk_offsets: Vec<u64>,
}

impl PpcQuickTimeVideoSampleTableInfo {
    fn is_complete(&self) -> bool {
        self.handler_subtype == u32::from_be_bytes(*b"vide")
            && self.media_time_scale != 0
            && self.media_duration != 0
            && self.codec != 0
            && !self.sample_sizes.is_empty()
            && !self.stsc_entries.is_empty()
            && !self.chunk_offsets.is_empty()
    }
}

impl PpcQuickTimeAudioDecodeInfo {
    fn is_complete(&self) -> bool {
        self.handler_subtype == u32::from_be_bytes(*b"soun")
            && self.sample_count != 0
            && self.codec != 0
            && self.channel_count != 0
            && self.sample_rate_fixed != 0
            && !self.stsc_entries.is_empty()
            && !self.chunk_offsets.is_empty()
    }
}

impl PpcQuickTimeMusicDecodeInfo {
    fn is_complete(&self) -> bool {
        self.handler_subtype == u32::from_be_bytes(*b"musi")
            && self.media_time_scale != 0
            && self.media_duration != 0
            && self.sample_durations.len() == self.sample_sizes.len()
            && !self.sample_sizes.is_empty()
            && !self.stsc_entries.is_empty()
            && !self.chunk_offsets.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcQuickTimeStscEntry {
    pub(crate) first_chunk: u32,
    pub(crate) samples_per_chunk: u32,
    pub(crate) sample_description_id: u32,
}

fn ppc_qt_scan_movie_video_track(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeVideoTrackRecord> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"trak" {
            if let Some(track) = ppc_qt_parse_video_track(data, content_start, atom_end, depth + 1)
            {
                return Some(track);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(track) =
                ppc_qt_scan_movie_video_track(data, content_start, atom_end, depth + 1)
            {
                return Some(track);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_parse_video_track(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeVideoTrackRecord> {
    let mut info = PpcQuickTimeTrackSampleInfo::default();
    ppc_qt_collect_track_sample_info(data, start, end, depth, &mut info)?;
    info.into_video_track(data)
}

fn ppc_qt_scan_movie_video_samples(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeVideoSampleTableRecord> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"trak" {
            if let Some(samples) =
                ppc_qt_parse_video_track_samples(data, content_start, atom_end, depth + 1)
            {
                return Some(samples);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(samples) =
                ppc_qt_scan_movie_video_samples(data, content_start, atom_end, depth + 1)
            {
                return Some(samples);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_parse_video_track_samples(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeVideoSampleTableRecord> {
    let mut info = PpcQuickTimeVideoSampleTableInfo::default();
    ppc_qt_collect_video_sample_table_info(data, start, end, depth, &mut info)?;
    if !info.is_complete() {
        return None;
    }
    ppc_qt_build_video_sample_table(data, &info)
}

fn ppc_qt_scan_movie_audio_track(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeAudioTrackRecord> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"trak" {
            if let Some(track) = ppc_qt_parse_audio_track(data, content_start, atom_end, depth + 1)
            {
                return Some(track);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(track) =
                ppc_qt_scan_movie_audio_track(data, content_start, atom_end, depth + 1)
            {
                return Some(track);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_parse_audio_track(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcQuickTimeAudioTrackRecord> {
    let mut info = PpcQuickTimeTrackSampleInfo::default();
    ppc_qt_collect_track_sample_info(data, start, end, depth, &mut info)?;
    info.into_audio_track(data)
}

pub(crate) fn ppc_qt_movie_audio_samples(data: &[u8]) -> Option<PpcDecodedAiffData> {
    ppc_qt_scan_movie_audio_samples(data, 0, data.len(), 0)
        .or_else(|| ppc_qt_scan_movie_music_samples(data, 0, data.len(), 0))
}

fn ppc_qt_scan_movie_music_samples(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcDecodedAiffData> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"trak" {
            let mut info = PpcQuickTimeMusicDecodeInfo::default();
            ppc_qt_collect_music_decode_info(data, content_start, atom_end, depth + 1, &mut info)?;
            if info.is_complete() {
                return ppc_qt_synthesize_music_track(data, &info);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(samples) =
                ppc_qt_scan_movie_music_samples(data, content_start, atom_end, depth + 1)
            {
                return Some(samples);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_scan_movie_audio_samples(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcDecodedAiffData> {
    if depth > 8 || start >= end || end > data.len() {
        return None;
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"trak" {
            if let Some(samples) =
                ppc_qt_parse_audio_track_samples(data, content_start, atom_end, depth + 1)
            {
                return Some(samples);
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            if let Some(samples) =
                ppc_qt_scan_movie_audio_samples(data, content_start, atom_end, depth + 1)
            {
                return Some(samples);
            }
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_parse_audio_track_samples(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
) -> Option<PpcDecodedAiffData> {
    let mut info = PpcQuickTimeAudioDecodeInfo::default();
    ppc_qt_collect_audio_decode_info(data, start, end, depth, &mut info)?;
    if !info.is_complete() {
        return None;
    }
    ppc_qt_decode_audio_chunks(data, &info)
}

fn ppc_qt_sample_payload_summary(data: &[u8], offset: u64, size: u32) -> (u32, u32, u8, [u8; 16]) {
    let mut preview = [0; 16];
    let Ok(offset) = usize::try_from(offset) else {
        return (0, 0, 0, preview);
    };
    let Ok(size) = usize::try_from(size) else {
        return (0, 0, 0, preview);
    };
    let Some(end) = offset.checked_add(size) else {
        return (0, 0, 0, preview);
    };
    let Some(sample) = data.get(offset..end) else {
        return (0, 0, 0, preview);
    };

    let preview_len = sample.len().min(preview.len());
    preview[..preview_len].copy_from_slice(&sample[..preview_len]);
    let checksum = sample
        .iter()
        .fold(0u32, |acc, byte| acc.wrapping_add(u32::from(*byte)));
    (
        u32::try_from(sample.len()).unwrap_or(u32::MAX),
        checksum,
        u8::try_from(preview_len).unwrap_or(u8::MAX),
        preview,
    )
}

fn ppc_qt_collect_track_sample_info(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
    info: &mut PpcQuickTimeTrackSampleInfo,
) -> Option<()> {
    if depth > 8 || start >= end || end > data.len() {
        return Some(());
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"hdlr" {
            if let Some(subtype) = ppc_qt_hdlr_subtype(data, content_start, atom_end) {
                if subtype == u32::from_be_bytes(*b"vide")
                    || subtype == u32::from_be_bytes(*b"soun")
                {
                    info.handler_subtype = subtype;
                }
            }
        } else if atom_type == b"mdhd" {
            if let Some((time_scale, duration)) =
                ppc_qt_mvhd_duration(data, content_start, atom_end)
            {
                info.media_time_scale = time_scale;
                info.media_duration = duration;
            }
        } else if atom_type == b"stsd" {
            if let Some(codec) = ppc_qt_stsd_first_codec(data, content_start, atom_end) {
                info.codec = codec;
            }
            if let Some((channel_count, sample_size_bits, sample_rate_fixed)) =
                ppc_qt_stsd_first_sound_description(data, content_start, atom_end)
            {
                info.audio_channel_count = channel_count;
                info.audio_sample_size_bits = sample_size_bits;
                info.audio_sample_rate_fixed = sample_rate_fixed;
            }
        } else if atom_type == b"stts" {
            if let Some((sample_count, first_sample_duration)) =
                ppc_qt_stts_sample_summary(data, content_start, atom_end)
            {
                info.sample_count = sample_count;
                info.first_sample_duration = first_sample_duration;
            }
        } else if atom_type == b"stsc" {
            if let Some((samples_per_chunk, sample_description_id)) =
                ppc_qt_stsc_first_chunk(data, content_start, atom_end)
            {
                info.first_samples_per_chunk = samples_per_chunk;
                info.sample_description_id = sample_description_id;
            }
        } else if atom_type == b"stsz" {
            if let Some((sample_count, first_sample_size)) =
                ppc_qt_stsz_sample_summary(data, content_start, atom_end)
            {
                if info.sample_count == 0 {
                    info.sample_count = sample_count;
                }
                info.first_sample_size = first_sample_size;
            }
        } else if atom_type == b"stco" {
            if let Some(first_chunk_offset) =
                ppc_qt_stco_first_offset(data, content_start, atom_end)
            {
                info.first_chunk_offset = first_chunk_offset;
            }
        } else if atom_type == b"co64" {
            if let Some(first_chunk_offset) =
                ppc_qt_co64_first_offset(data, content_start, atom_end)
            {
                info.first_chunk_offset = first_chunk_offset;
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            ppc_qt_collect_track_sample_info(data, content_start, atom_end, depth + 1, info)?;
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    Some(())
}

fn ppc_qt_collect_video_sample_table_info(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
    info: &mut PpcQuickTimeVideoSampleTableInfo,
) -> Option<()> {
    if depth > 8 || start >= end || end > data.len() {
        return Some(());
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"hdlr" {
            if let Some(subtype) = ppc_qt_hdlr_subtype(data, content_start, atom_end) {
                if subtype == u32::from_be_bytes(*b"vide") {
                    info.handler_subtype = subtype;
                }
            }
        } else if atom_type == b"mdhd" {
            if let Some((time_scale, duration)) =
                ppc_qt_mvhd_duration(data, content_start, atom_end)
            {
                info.media_time_scale = time_scale;
                info.media_duration = duration;
            }
        } else if atom_type == b"stsd" {
            if let Some(codec) = ppc_qt_stsd_first_codec(data, content_start, atom_end) {
                info.codec = codec;
            }
        } else if atom_type == b"stts" {
            if let Some(sample_durations) =
                ppc_qt_stts_sample_durations(data, content_start, atom_end)
            {
                info.sample_durations = sample_durations;
            }
        } else if atom_type == b"stsc" {
            if let Some(entries) = ppc_qt_stsc_entries(data, content_start, atom_end) {
                info.stsc_entries = entries;
            }
        } else if atom_type == b"stsz" {
            if let Some(sample_sizes) = ppc_qt_stsz_sample_sizes(data, content_start, atom_end) {
                info.sample_sizes = sample_sizes;
            }
        } else if atom_type == b"stco" {
            if let Some(offsets) = ppc_qt_stco_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if atom_type == b"co64" {
            if let Some(offsets) = ppc_qt_co64_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            ppc_qt_collect_video_sample_table_info(data, content_start, atom_end, depth + 1, info)?;
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    Some(())
}

fn ppc_qt_collect_audio_decode_info(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
    info: &mut PpcQuickTimeAudioDecodeInfo,
) -> Option<()> {
    if depth > 8 || start >= end || end > data.len() {
        return Some(());
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"hdlr" {
            if let Some(subtype) = ppc_qt_hdlr_subtype(data, content_start, atom_end) {
                if subtype == u32::from_be_bytes(*b"soun") {
                    info.handler_subtype = subtype;
                }
            }
        } else if atom_type == b"stsd" {
            if let Some(codec) = ppc_qt_stsd_first_codec(data, content_start, atom_end) {
                info.codec = codec;
            }
            if let Some((channel_count, sample_size_bits, sample_rate_fixed)) =
                ppc_qt_stsd_first_sound_description(data, content_start, atom_end)
            {
                info.channel_count = channel_count;
                info.sample_size_bits = sample_size_bits;
                info.sample_rate_fixed = sample_rate_fixed;
            }
        } else if atom_type == b"stts" {
            if let Some((sample_count, _first_sample_duration)) =
                ppc_qt_stts_sample_summary(data, content_start, atom_end)
            {
                info.sample_count = sample_count;
            }
        } else if atom_type == b"stsz" {
            if let Some((sample_count, _first_sample_size)) =
                ppc_qt_stsz_sample_summary(data, content_start, atom_end)
            {
                if info.sample_count == 0 {
                    info.sample_count = sample_count;
                }
            }
        } else if atom_type == b"stsc" {
            if let Some(entries) = ppc_qt_stsc_entries(data, content_start, atom_end) {
                info.stsc_entries = entries;
            }
        } else if atom_type == b"stco" {
            if let Some(offsets) = ppc_qt_stco_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if atom_type == b"co64" {
            if let Some(offsets) = ppc_qt_co64_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            ppc_qt_collect_audio_decode_info(data, content_start, atom_end, depth + 1, info)?;
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    Some(())
}

fn ppc_qt_collect_music_decode_info(
    data: &[u8],
    start: usize,
    end: usize,
    depth: u8,
    info: &mut PpcQuickTimeMusicDecodeInfo,
) -> Option<()> {
    if depth > 8 || start >= end || end > data.len() {
        return Some(());
    }

    let mut offset = start;
    while offset.checked_add(8)? <= end {
        let (atom_type, content_start, atom_end) = ppc_qt_atom_range(data, offset, end)?;
        if atom_type == b"hdlr" {
            if let Some(subtype) = ppc_qt_hdlr_subtype(data, content_start, atom_end) {
                if subtype == u32::from_be_bytes(*b"musi") {
                    info.handler_subtype = subtype;
                }
            }
        } else if atom_type == b"mdhd" {
            if let Some((time_scale, duration)) =
                ppc_qt_mvhd_duration(data, content_start, atom_end)
            {
                info.media_time_scale = time_scale;
                info.media_duration = duration;
            }
        } else if atom_type == b"stsd" {
            if let Some(sample_descriptions) =
                ppc_qt_music_sample_descriptions(data, content_start, atom_end)
            {
                info.sample_descriptions = sample_descriptions;
            }
        } else if atom_type == b"stts" {
            if let Some(sample_durations) =
                ppc_qt_stts_sample_durations(data, content_start, atom_end)
            {
                info.sample_durations = sample_durations;
            }
        } else if atom_type == b"stsz" {
            if let Some(sample_sizes) = ppc_qt_stsz_sample_sizes(data, content_start, atom_end) {
                info.sample_sizes = sample_sizes;
            }
        } else if atom_type == b"stsc" {
            if let Some(entries) = ppc_qt_stsc_entries(data, content_start, atom_end) {
                info.stsc_entries = entries;
            }
        } else if atom_type == b"stco" {
            if let Some(offsets) = ppc_qt_stco_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if atom_type == b"co64" {
            if let Some(offsets) = ppc_qt_co64_offsets(data, content_start, atom_end) {
                info.chunk_offsets = offsets;
            }
        } else if ppc_qt_is_container_atom(atom_type) {
            ppc_qt_collect_music_decode_info(data, content_start, atom_end, depth + 1, info)?;
        }

        if atom_end == end {
            break;
        }
        offset = atom_end;
    }
    Some(())
}

pub(crate) fn ppc_qt_music_sample_descriptions(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<Vec<Vec<PpcQuickTimeMusicPart>>> {
    let entry_count = usize::try_from(ppc_read_be_u32_from_slice(data, content_start + 4)?).ok()?;
    let maximum_entries = atom_end.checked_sub(content_start.checked_add(8)?)? / 16;
    if entry_count > maximum_entries {
        return None;
    }
    let mut entry_offset = content_start.checked_add(8)?;
    let mut sample_descriptions = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        let entry_size = usize::try_from(ppc_read_be_u32_from_slice(data, entry_offset)?).ok()?;
        let entry_end = entry_offset.checked_add(entry_size)?;
        if entry_size < 20 || entry_end > atom_end {
            return None;
        }
        if data.get(entry_offset + 4..entry_offset + 8)? != b"musi" {
            return None;
        }
        let mut parts = Vec::new();
        let mut event_offset = entry_offset + 20;
        while event_offset.checked_add(8)? <= entry_end {
            let head = ppc_read_be_u32_from_slice(data, event_offset)?;
            if head == 0 {
                event_offset += 4;
                continue;
            }
            if head >> 28 != 15 {
                break;
            }
            let word_count = usize::try_from(head & 0xffff).ok()?;
            let byte_count = word_count.checked_mul(4)?;
            let event_end = event_offset.checked_add(byte_count)?;
            if word_count < 2 || event_end > entry_end {
                return None;
            }
            let tail = ppc_read_be_u32_from_slice(data, event_end - 4)?;
            if tail >> 30 != 3 || tail & 0xffff != head & 0xffff {
                return None;
            }
            let subtype = (tail >> 16) & 0x0fff;
            if subtype == 1 && word_count >= 23 {
                let request = event_offset + 4;
                parts.push(PpcQuickTimeMusicPart {
                    part: ((head >> 16) & 0x0fff) as u16,
                    instrument_number: ppc_read_be_u32_from_slice(data, request + 76)?,
                    gm_number: ppc_read_be_u32_from_slice(data, request + 80)?,
                });
            }
            event_offset = event_end;
        }
        sample_descriptions.push(parts);
        entry_offset = entry_end;
    }
    Some(sample_descriptions)
}

fn ppc_qt_is_container_atom(atom_type: &[u8]) -> bool {
    atom_type == b"moov"
        || atom_type == b"trak"
        || atom_type == b"mdia"
        || atom_type == b"minf"
        || atom_type == b"stbl"
        || atom_type == b"edts"
}

fn ppc_qt_hdlr_subtype(data: &[u8], content_start: usize, atom_end: usize) -> Option<u32> {
    let subtype_offset = content_start.checked_add(8)?;
    if subtype_offset.checked_add(4)? > atom_end {
        return None;
    }
    Some(u32::from_be_bytes(
        data.get(subtype_offset..subtype_offset + 4)?
            .try_into()
            .ok()?,
    ))
}

fn ppc_qt_stsd_first_codec(data: &[u8], content_start: usize, atom_end: usize) -> Option<u32> {
    if content_start.checked_add(16)? > atom_end {
        return None;
    }
    let entry_count = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    if entry_count == 0 {
        return None;
    }
    let entry_start = content_start + 8;
    let entry_size = usize::try_from(u32::from_be_bytes(
        data.get(entry_start..entry_start + 4)?.try_into().ok()?,
    ))
    .ok()?;
    if entry_size < 8 || entry_start.checked_add(entry_size)? > atom_end {
        return None;
    }
    Some(u32::from_be_bytes(
        data.get(entry_start + 4..entry_start + 8)?
            .try_into()
            .ok()?,
    ))
}

fn ppc_qt_stsd_first_sound_description(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<(u16, u16, u32)> {
    if content_start.checked_add(16)? > atom_end {
        return None;
    }
    let entry_count = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    if entry_count == 0 {
        return None;
    }
    let entry_start = content_start + 8;
    let entry_size = usize::try_from(u32::from_be_bytes(
        data.get(entry_start..entry_start + 4)?.try_into().ok()?,
    ))
    .ok()?;
    if entry_size < 36 || entry_start.checked_add(entry_size)? > atom_end {
        return None;
    }

    let channel_count = u16::from_be_bytes(
        data.get(entry_start + 24..entry_start + 26)?
            .try_into()
            .ok()?,
    );
    let sample_size_bits = u16::from_be_bytes(
        data.get(entry_start + 26..entry_start + 28)?
            .try_into()
            .ok()?,
    );
    let sample_rate_fixed = u32::from_be_bytes(
        data.get(entry_start + 32..entry_start + 36)?
            .try_into()
            .ok()?,
    );
    (channel_count != 0 && sample_size_bits != 0 && sample_rate_fixed != 0).then_some((
        channel_count,
        sample_size_bits,
        sample_rate_fixed,
    ))
}

fn ppc_qt_stts_sample_summary(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<(u32, u32)> {
    let entry_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if entry_count == 0 || entry_count > 1_000_000 {
        return None;
    }
    let table_start = content_start.checked_add(8)?;
    if table_start.checked_add(entry_count.checked_mul(8)?)? > atom_end {
        return None;
    }

    let mut sample_count = 0u32;
    let mut first_sample_duration = 0u32;
    for entry in 0..entry_count {
        let entry_start = table_start + entry * 8;
        let count = u32::from_be_bytes(data.get(entry_start..entry_start + 4)?.try_into().ok()?);
        let duration = u32::from_be_bytes(
            data.get(entry_start + 4..entry_start + 8)?
                .try_into()
                .ok()?,
        );
        if count != 0 && duration != 0 && first_sample_duration == 0 {
            first_sample_duration = duration;
        }
        sample_count = sample_count.saturating_add(count);
    }
    (sample_count != 0 && first_sample_duration != 0)
        .then_some((sample_count, first_sample_duration))
}

fn ppc_qt_stsc_first_chunk(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<(u32, u32)> {
    let entry_count = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    if entry_count == 0 || content_start.checked_add(20)? > atom_end {
        return None;
    }
    let first_chunk = u32::from_be_bytes(
        data.get(content_start + 8..content_start + 12)?
            .try_into()
            .ok()?,
    );
    let samples_per_chunk = u32::from_be_bytes(
        data.get(content_start + 12..content_start + 16)?
            .try_into()
            .ok()?,
    );
    let sample_description_id = u32::from_be_bytes(
        data.get(content_start + 16..content_start + 20)?
            .try_into()
            .ok()?,
    );
    (first_chunk != 0 && samples_per_chunk != 0 && sample_description_id != 0)
        .then_some((samples_per_chunk, sample_description_id))
}

pub(crate) fn ppc_qt_stsc_entries(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<Vec<PpcQuickTimeStscEntry>> {
    let entry_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if entry_count == 0 || entry_count > 100_000 {
        return None;
    }
    let table_start = content_start.checked_add(8)?;
    if table_start.checked_add(entry_count.checked_mul(12)?)? > atom_end {
        return None;
    }

    let mut entries: Vec<PpcQuickTimeStscEntry> = Vec::with_capacity(entry_count);
    for entry in 0..entry_count {
        let entry_start = table_start + entry * 12;
        let first_chunk =
            u32::from_be_bytes(data.get(entry_start..entry_start + 4)?.try_into().ok()?);
        let samples_per_chunk = u32::from_be_bytes(
            data.get(entry_start + 4..entry_start + 8)?
                .try_into()
                .ok()?,
        );
        let sample_description_id = u32::from_be_bytes(
            data.get(entry_start + 8..entry_start + 12)?
                .try_into()
                .ok()?,
        );
        if first_chunk == 0
            || samples_per_chunk == 0
            || sample_description_id == 0
            || entries
                .last()
                .is_some_and(|previous| previous.first_chunk >= first_chunk)
        {
            return None;
        }
        entries.push(PpcQuickTimeStscEntry {
            first_chunk,
            samples_per_chunk,
            sample_description_id,
        });
    }
    Some(entries)
}

fn ppc_qt_stsz_sample_summary(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<(u32, u32)> {
    if content_start.checked_add(12)? > atom_end {
        return None;
    }
    let sample_size = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    let sample_count = u32::from_be_bytes(
        data.get(content_start + 8..content_start + 12)?
            .try_into()
            .ok()?,
    );
    if sample_count == 0 {
        return None;
    }
    if sample_size != 0 {
        return Some((sample_count, sample_size));
    }
    if content_start.checked_add(16)? > atom_end {
        return None;
    }
    let first_size = u32::from_be_bytes(
        data.get(content_start + 12..content_start + 16)?
            .try_into()
            .ok()?,
    );
    (first_size != 0).then_some((sample_count, first_size))
}

fn ppc_qt_stsz_sample_sizes(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<Vec<u32>> {
    if content_start.checked_add(12)? > atom_end {
        return None;
    }
    let sample_size = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    let sample_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 8..content_start + 12)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if sample_count == 0 || sample_count > 1_000_000 {
        return None;
    }
    if sample_size != 0 {
        return Some(vec![sample_size; sample_count]);
    }
    let table_start = content_start.checked_add(12)?;
    if table_start.checked_add(sample_count.checked_mul(4)?)? > atom_end {
        return None;
    }
    let mut sizes = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let offset = table_start + index * 4;
        let size = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
        if size == 0 {
            return None;
        }
        sizes.push(size);
    }
    Some(sizes)
}

pub(crate) fn ppc_qt_stts_sample_durations(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<Vec<u32>> {
    if content_start.checked_add(8)? > atom_end {
        return None;
    }
    let entry_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if entry_count == 0 || entry_count > 1_000_000 {
        return None;
    }
    let table_start = content_start.checked_add(8)?;
    if table_start.checked_add(entry_count.checked_mul(8)?)? > atom_end {
        return None;
    }
    let mut durations = Vec::new();
    for index in 0..entry_count {
        let offset = table_start + index * 8;
        let sample_count = usize::try_from(u32::from_be_bytes(
            data.get(offset..offset + 4)?.try_into().ok()?,
        ))
        .ok()?;
        let sample_duration =
            u32::from_be_bytes(data.get(offset + 4..offset + 8)?.try_into().ok()?);
        if sample_count == 0 || sample_duration == 0 {
            return None;
        }
        let new_len = durations.len().checked_add(sample_count)?;
        if new_len > 1_000_000 {
            return None;
        }
        durations.resize(new_len, sample_duration);
    }
    (!durations.is_empty()).then_some(durations)
}

fn ppc_qt_stco_first_offset(data: &[u8], content_start: usize, atom_end: usize) -> Option<u64> {
    if content_start.checked_add(12)? > atom_end {
        return None;
    }
    let entry_count = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    if entry_count == 0 {
        return None;
    }
    Some(u64::from(u32::from_be_bytes(
        data.get(content_start + 8..content_start + 12)?
            .try_into()
            .ok()?,
    )))
}

fn ppc_qt_stco_offsets(data: &[u8], content_start: usize, atom_end: usize) -> Option<Vec<u64>> {
    let entry_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if entry_count == 0 || entry_count > 1_000_000 {
        return None;
    }
    let table_start = content_start.checked_add(8)?;
    if table_start.checked_add(entry_count.checked_mul(4)?)? > atom_end {
        return None;
    }
    let mut offsets = Vec::with_capacity(entry_count);
    for entry in 0..entry_count {
        let entry_start = table_start + entry * 4;
        offsets.push(u64::from(u32::from_be_bytes(
            data.get(entry_start..entry_start + 4)?.try_into().ok()?,
        )));
    }
    Some(offsets)
}

fn ppc_qt_co64_first_offset(data: &[u8], content_start: usize, atom_end: usize) -> Option<u64> {
    if content_start.checked_add(16)? > atom_end {
        return None;
    }
    let entry_count = u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    );
    if entry_count == 0 {
        return None;
    }
    Some(u64::from_be_bytes(
        data.get(content_start + 8..content_start + 16)?
            .try_into()
            .ok()?,
    ))
}

fn ppc_qt_co64_offsets(data: &[u8], content_start: usize, atom_end: usize) -> Option<Vec<u64>> {
    let entry_count = usize::try_from(u32::from_be_bytes(
        data.get(content_start + 4..content_start + 8)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    if entry_count == 0 || entry_count > 1_000_000 {
        return None;
    }
    let table_start = content_start.checked_add(8)?;
    if table_start.checked_add(entry_count.checked_mul(8)?)? > atom_end {
        return None;
    }
    let mut offsets = Vec::with_capacity(entry_count);
    for entry in 0..entry_count {
        let entry_start = table_start + entry * 8;
        offsets.push(u64::from_be_bytes(
            data.get(entry_start..entry_start + 8)?.try_into().ok()?,
        ));
    }
    Some(offsets)
}

fn ppc_qt_union_bounds(
    lhs: Option<(i16, i16, i16, i16)>,
    rhs: Option<(i16, i16, i16, i16)>,
) -> Option<(i16, i16, i16, i16)> {
    match (lhs, rhs) {
        (Some((top, left, bottom, right)), Some((rhs_top, rhs_left, rhs_bottom, rhs_right))) => {
            Some((
                top.min(rhs_top),
                left.min(rhs_left),
                bottom.max(rhs_bottom),
                right.max(rhs_right),
            ))
        }
        (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
        (None, None) => None,
    }
}

fn ppc_qt_tkhd_bounds(
    data: &[u8],
    content_start: usize,
    atom_end: usize,
) -> Option<(i16, i16, i16, i16)> {
    let version = *data.get(content_start)?;
    let dimensions_offset = match version {
        0 => content_start.checked_add(76)?,
        1 => content_start.checked_add(88)?,
        _ => return None,
    };
    if dimensions_offset.checked_add(8)? > atom_end {
        return None;
    }
    let width_fixed = u32::from_be_bytes(
        data.get(dimensions_offset..dimensions_offset + 4)?
            .try_into()
            .ok()?,
    );
    let height_fixed = u32::from_be_bytes(
        data.get(dimensions_offset + 4..dimensions_offset + 8)?
            .try_into()
            .ok()?,
    );
    let width = width_fixed >> 16;
    let height = height_fixed >> 16;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return None;
    }
    Some((
        0,
        0,
        i16::try_from(height).ok()?,
        i16::try_from(width).ok()?,
    ))
}

fn ppc_qt_mvhd_duration(data: &[u8], content_start: usize, atom_end: usize) -> Option<(u32, u64)> {
    let version = *data.get(content_start)?;
    let (time_scale_offset, duration_offset, duration_size) = match version {
        0 => (
            content_start.checked_add(12)?,
            content_start.checked_add(16)?,
            4usize,
        ),
        1 => (
            content_start.checked_add(20)?,
            content_start.checked_add(24)?,
            8usize,
        ),
        _ => return None,
    };
    if time_scale_offset.checked_add(4)? > atom_end
        || duration_offset.checked_add(duration_size)? > atom_end
    {
        return None;
    }
    let time_scale = u32::from_be_bytes(
        data.get(time_scale_offset..time_scale_offset + 4)?
            .try_into()
            .ok()?,
    );
    let duration = if duration_size == 4 {
        u64::from(u32::from_be_bytes(
            data.get(duration_offset..duration_offset + 4)?
                .try_into()
                .ok()?,
        ))
    } else {
        u64::from_be_bytes(
            data.get(duration_offset..duration_offset + 8)?
                .try_into()
                .ok()?,
        )
    };
    if time_scale == 0 || duration == 0 {
        return None;
    }
    Some((time_scale, duration))
}

fn ppc_qt_movie_tasks_until_done(time_scale: u32, duration: u64) -> u32 {
    if time_scale == 0 || duration == 0 {
        return PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE;
    }
    let numerator = duration
        .saturating_mul(PPC_QT_MOVIE_TASKS_PER_SECOND)
        .saturating_add(u64::from(time_scale).saturating_sub(1));
    let tasks = numerator / u64::from(time_scale);
    u32::try_from(tasks.max(1)).unwrap_or(u32::MAX)
}

pub(crate) fn ppc_qt_draw_pict_source_to_16bpp(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    data: &[u8],
) -> bool {
    let Some((_pict_offset, (top, left, bottom, right))) =
        ppc_qt_pict_record_offset_and_bounds(data)
    else {
        return false;
    };
    let natural_width = u32::try_from(i32::from(right) - i32::from(left)).unwrap_or(0);
    let natural_height = u32::try_from(i32::from(bottom) - i32::from(top)).unwrap_or(0);
    if natural_width == 0 || natural_height == 0 {
        return false;
    }
    let dst_bottom = i16::try_from(natural_height.min(front_buffer.height)).unwrap_or(i16::MAX);
    let dst_right = i16::try_from(natural_width.min(front_buffer.width)).unwrap_or(i16::MAX);
    ppc_draw_pict_bytes_to_16bpp(
        memory,
        front_buffer,
        data,
        (0, 0, dst_bottom, dst_right),
        &TrapDispatcher::standard_mac_8bpp_clut(),
        0,
        false,
    )
}

pub(crate) fn ppc_qt_new_movie_from_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    let movie_out_ptr = cpu.gpr[3];
    let res_id_ptr = cpu.gpr[5];
    let data_ref_was_changed_ptr = cpu.gpr[8];
    if movie_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, movie_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    if res_id_ptr != 0 && !ppc_memory_can_write_bytes(memory, res_id_ptr, 2) {
        return PPC_PARAM_ERR;
    }
    if data_ref_was_changed_ptr != 0
        && !ppc_memory_can_write_bytes(memory, data_ref_was_changed_ptr, 1)
    {
        return PPC_PARAM_ERR;
    }
    // Inside Macintosh: QuickTime (1993), pp. 4-5–4-6: NewMovieFromFile
    // returns NIL in the movie output when it cannot create the movie.
    if memory.write_u32_be(movie_out_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    let requested_res_id = if res_id_ptr == 0 {
        None
    } else {
        Some(memory.read_u16_be(res_id_ptr).unwrap_or(0) as i16)
    };
    if res_id_ptr != 0 && memory.write_u16_be(res_id_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    ppc_qt_stop_movie_audio(sound);
    let ref_num = cpu.gpr[4] as i16;
    if ref_num != 0 && ref_num == quicktime.movie_file_ref_num {
        let mut selected_data = vfs_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&quicktime.movie_file_path))
            .map(|file| file.data.to_vec())
            .unwrap_or_else(|| quicktime.movie_file_data.clone());
        let Some(selected_res_id) = ppc_qt_append_movie_resource(
            &mut selected_data,
            vfs_resource_files,
            vfs_resources,
            &quicktime.movie_file_path,
            requested_res_id,
        ) else {
            return PPC_RES_NOT_FOUND_ERR;
        };
        quicktime.movie_file_data = selected_data;
        ppc_qt_refresh_open_movie_metadata(quicktime);
        if res_id_ptr != 0
            && memory
                .write_u16_be(res_id_ptr, selected_res_id as u16)
                .is_none()
        {
            return PPC_PARAM_ERR;
        }
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
    if memory.write_u32_be(movie_out_ptr, PPC_QT_MOVIE).is_none() {
        return PPC_PARAM_ERR;
    }
    if data_ref_was_changed_ptr != 0 && memory.write_u8(data_ref_was_changed_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_started = false;
    quicktime.movie_task_count = 0;
    quicktime.movie_disposed = false;
    quicktime.movie_at_beginning = true;
    ppc_qt_reset_movie_video_decode_cache(quicktime);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_open_movie_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    quicktime: &mut PpcQuickTimeState,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let ref_num_out_ptr = cpu.gpr[4];
    if ref_num_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, ref_num_out_ptr, 2) {
        return PPC_PARAM_ERR;
    }

    let mut movie_file_path = String::new();
    let mut movie_file_data = Vec::new();
    let mut movie_file_bounds = None;
    let mut movie_file_time_scale = 0;
    let mut movie_file_duration = 0;
    let mut movie_file_tasks_until_done = PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE;
    let mut movie_file_video_track = None;
    let mut movie_file_video_samples = None;
    let mut movie_file_audio_track = None;
    if spec_ptr != 0 {
        if let Ok(path) =
            ppc_existing_path_for_fsspec(memory, vfs_directories, vfs_files, &[], spec_ptr)
        {
            if let Some(file) = vfs_files
                .iter()
                .find(|record| record.path.eq_ignore_ascii_case(&path))
            {
                movie_file_path = path;
                movie_file_data = file.data.to_vec();
                movie_file_bounds = ppc_qt_movie_bounds(&movie_file_data);
                movie_file_video_track = ppc_qt_movie_first_video_track(&movie_file_data);
                movie_file_video_samples = ppc_qt_movie_video_samples(&movie_file_data);
                movie_file_audio_track = ppc_qt_movie_first_audio_track(&movie_file_data);
                if let Some((time_scale, duration)) = ppc_qt_movie_duration(&movie_file_data) {
                    movie_file_time_scale = time_scale;
                    movie_file_duration = duration;
                    movie_file_tasks_until_done =
                        ppc_qt_movie_tasks_until_done(time_scale, duration);
                }
            }
        }
    }

    if memory
        .write_u16_be(ref_num_out_ptr, PPC_FIRST_FILE_REF_NUM as u16)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_file_open_count = quicktime.movie_file_open_count.saturating_add(1);
    quicktime.movie_file_ref_num = PPC_FIRST_FILE_REF_NUM;
    quicktime.movie_file_path = movie_file_path;
    quicktime.movie_file_data = movie_file_data;
    quicktime.movie_file_bounds = movie_file_bounds;
    quicktime.movie_file_time_scale = movie_file_time_scale;
    quicktime.movie_file_duration = movie_file_duration;
    quicktime.movie_file_tasks_until_done = movie_file_tasks_until_done;
    quicktime.movie_file_video_track = movie_file_video_track;
    quicktime.movie_file_video_samples = movie_file_video_samples;
    quicktime.movie_file_audio_track = movie_file_audio_track;
    if qt_trace_enabled() {
        let video = quicktime
            .movie_file_video_samples
            .as_ref()
            .map(|samples| {
                format!(
                    "{} samples={} duration={}",
                    format_ppc_fourcc(samples.codec),
                    samples.sample_count,
                    samples.media_duration
                )
            })
            .unwrap_or_else(|| "none".to_string());
        eprintln!(
            "[QT-TRACE] OpenMovieFile path='{}' data_len={} bounds={:?} time_scale={} duration={} tasks_until_done={} video={}",
            quicktime.movie_file_path,
            quicktime.movie_file_data.len(),
            quicktime.movie_file_bounds,
            quicktime.movie_file_time_scale,
            quicktime.movie_file_duration,
            quicktime.movie_file_tasks_until_done,
            video
        );
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_append_movie_resource(
    movie_file_data: &mut Vec<u8>,
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    path: &str,
    requested_res_id: Option<i16>,
) -> Option<i16> {
    // Inside Macintosh: QuickTime (1993), pp. 4-3–4-6.
    let data_movie_range = ppc_qt_top_level_atom_offsets(movie_file_data, b"moov");
    if data_movie_range.is_some() && !matches!(requested_res_id, Some(1..=i16::MAX)) {
        return requested_res_id
            .map_or(true, |res_id| res_id == 0 || res_id == -1)
            .then_some(-1);
    }
    if requested_res_id == Some(-1) {
        return None;
    }
    let moov_type = u32::from_be_bytes(*b"moov");
    let resource = vfs_resources
        .iter()
        .filter(|resource| {
            resource.path.eq_ignore_ascii_case(path) && resource.res_type == moov_type
        })
        .find(|resource| requested_res_id.map_or(true, |id| id == 0 || resource.res_id == id))
        .map(|resource| (resource.res_id, resource.data.clone()))
        .or_else(|| {
            let raw_data = vfs_resource_files
                .iter()
                .find(|file| file.path.eq_ignore_ascii_case(path))?
                .raw_data
                .as_ref()?;
            let fork = ResourceFork::parse(raw_data)?;
            fork.resources()
                .values()
                .filter(|resource| resource.res_type == *b"moov")
                .filter(|resource| requested_res_id.map_or(true, |id| id == 0 || resource.id == id))
                .min_by_key(|resource| resource.id)
                .map(|resource| (resource.id, resource.data.clone()))
        });
    let Some((res_id, resource_data)) = resource else {
        return None;
    };

    if let Some((atom_type, _, atom_end)) =
        ppc_qt_atom_range(&resource_data, 0, resource_data.len())
    {
        if atom_type == b"moov" && atom_end == resource_data.len() {
            if let Some((start, end)) = data_movie_range {
                movie_file_data.drain(start..end);
            }
            movie_file_data.extend_from_slice(&resource_data);
            return Some(res_id);
        }
    }
    None
}

fn ppc_qt_top_level_atom_offsets(data: &[u8], expected_type: &[u8; 4]) -> Option<(usize, usize)> {
    let mut offset = 0usize;
    while offset.checked_add(8)? <= data.len() {
        let (atom_type, _, atom_end) = ppc_qt_atom_range(data, offset, data.len())?;
        if atom_type == expected_type {
            return Some((offset, atom_end));
        }
        if atom_end == data.len() {
            break;
        }
        offset = atom_end;
    }
    None
}

fn ppc_qt_refresh_open_movie_metadata(quicktime: &mut PpcQuickTimeState) {
    quicktime.movie_file_bounds = None;
    quicktime.movie_file_time_scale = 0;
    quicktime.movie_file_duration = 0;
    quicktime.movie_file_tasks_until_done = PPC_QT_FALLBACK_MOVIE_TASKS_UNTIL_DONE;
    quicktime.movie_file_video_track = None;
    quicktime.movie_file_video_samples = None;
    quicktime.movie_file_audio_track = None;
    quicktime.movie_file_bounds = ppc_qt_movie_bounds(&quicktime.movie_file_data);
    quicktime.movie_file_video_track = ppc_qt_movie_first_video_track(&quicktime.movie_file_data);
    quicktime.movie_file_video_samples = ppc_qt_movie_video_samples(&quicktime.movie_file_data);
    quicktime.movie_file_audio_track = ppc_qt_movie_first_audio_track(&quicktime.movie_file_data);
    if let Some((time_scale, duration)) = ppc_qt_movie_duration(&quicktime.movie_file_data) {
        quicktime.movie_file_time_scale = time_scale;
        quicktime.movie_file_duration = duration;
        quicktime.movie_file_tasks_until_done = ppc_qt_movie_tasks_until_done(time_scale, duration);
    }
}

#[cfg(test)]
pub(crate) fn ppc_qt_top_level_atom<'a>(data: &'a [u8], expected_type: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 0usize;
    while offset.checked_add(8)? <= data.len() {
        let (atom_type, _, atom_end) = ppc_qt_atom_range(data, offset, data.len())?;
        if atom_type == expected_type {
            return data.get(offset..atom_end);
        }
        if atom_end == data.len() {
            break;
        }
        offset = atom_end;
    }
    None
}

pub(crate) fn ppc_qt_close_movie_file(cpu: &mut PpcCpu, quicktime: &mut PpcQuickTimeState) -> i16 {
    let ref_num = cpu.gpr[3] as i16;
    if ref_num == 0 {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_file_close_count = quicktime.movie_file_close_count.saturating_add(1);
    quicktime.movie_file_last_closed_ref_num = ref_num;
    if quicktime.movie_file_ref_num == ref_num {
        quicktime.movie_file_ref_num = 0;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_set_movie_gworld(cpu: &mut PpcCpu, quicktime: &mut PpcQuickTimeState) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_gworld = cpu.gpr[4];
    quicktime.movie_gdevice = cpu.gpr[5];
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_set_movie_box(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    let box_ptr = cpu.gpr[4];
    let Some(rect) = ppc_read_rect(memory, box_ptr) else {
        return PPC_PARAM_ERR;
    };
    quicktime.movie_box = rect;
    quicktime.movie_set_box_count = quicktime.movie_set_box_count.saturating_add(1);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_get_movie_box(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &PpcQuickTimeState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    if cpu.gpr[4] == 0 || !ppc_memory_can_write_bytes(memory, cpu.gpr[4], 8) {
        return PPC_PARAM_ERR;
    }
    let (top, left, bottom, right) = quicktime.movie_box;
    if ppc_write_rect(memory, cpu.gpr[4], top, left, bottom, right).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_go_to_beginning_of_movie(
    cpu: &mut PpcCpu,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_beginning_count = quicktime.movie_beginning_count.saturating_add(1);
    quicktime.movie_at_beginning = true;
    quicktime.movie_started = false;
    quicktime.movie_task_count = 0;
    ppc_qt_reset_movie_video_decode_cache(quicktime);
    ppc_qt_stop_movie_audio(sound);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_go_to_end_of_movie(
    cpu: &mut PpcCpu,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_at_beginning = false;
    quicktime.movie_started = false;
    quicktime.movie_task_count = quicktime.movie_tasks_until_done.max(1);
    ppc_qt_reset_movie_video_decode_cache(quicktime);
    ppc_qt_stop_movie_audio(sound);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_get_movie_duration(cpu: &mut PpcCpu, quicktime: &PpcQuickTimeState) -> (i16, u32) {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return (PPC_PARAM_ERR, 0);
    }
    let duration = if quicktime.movie_file_duration > 0 {
        quicktime.movie_file_duration as u32
    } else {
        600
    };
    (PPC_NO_ERR, duration)
}

pub(crate) fn ppc_qt_load_movie_into_ram(cpu: &mut PpcCpu, quicktime: &PpcQuickTimeState) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_start_movie(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_started = true;
    quicktime.movie_at_beginning = false;
    let audio_started = ppc_qt_start_movie_audio(quicktime, sound);
    let cache_before = quicktime
        .movie_video_decode_cache
        .as_ref()
        .map(|cache| cache.sample_index);
    let drawn = ppc_qt_draw_movie_frame(memory, gworlds, current_gworld, quicktime, 0x21);
    if qt_trace_enabled() {
        let cache_after = quicktime
            .movie_video_decode_cache
            .as_ref()
            .map(|cache| cache.sample_index);
        eprintln!(
            "[QT-TRACE] StartMovie path='{}' task_count={} drawn={} decoded={} audio={} cache_before={:?} cache_after={:?}",
            quicktime.movie_file_path,
            quicktime.movie_task_count,
            drawn,
            cache_after.is_some() && cache_after != cache_before,
            audio_started,
            cache_before,
            cache_after
        );
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_stop_movie(
    cpu: &mut PpcCpu,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_started = false;
    ppc_qt_stop_movie_audio(sound);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_movies_task(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_task_count = quicktime.movie_task_count.saturating_add(1);
    if quicktime.movie_started {
        quicktime.movie_at_beginning = false;
        let cache_before = quicktime
            .movie_video_decode_cache
            .as_ref()
            .map(|cache| cache.sample_index);
        let drawn = ppc_qt_draw_movie_frame(
            memory,
            gworlds,
            current_gworld,
            quicktime,
            0x30u32.saturating_add(quicktime.movie_task_count),
        );
        if qt_trace_enabled() {
            let cache_after = quicktime
                .movie_video_decode_cache
                .as_ref()
                .map(|cache| cache.sample_index);
            eprintln!(
                "[QT-TRACE] MoviesTask path='{}' task_count={} drawn={} decoded={} cache_before={:?} cache_after={:?}",
                quicktime.movie_file_path,
                quicktime.movie_task_count,
                drawn,
                cache_after.is_some() && cache_after != cache_before,
                cache_before,
                cache_after
            );
        }
        if quicktime.movie_task_count >= quicktime.movie_tasks_until_done.max(1) {
            quicktime.movie_started = false;
            ppc_qt_stop_movie_audio(sound);
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_is_movie_done(cpu: &mut PpcCpu, quicktime: &PpcQuickTimeState) -> (i16, bool) {
    if cpu.gpr[3] != PPC_QT_MOVIE || quicktime.movie_disposed {
        return (PPC_PARAM_ERR, true);
    }
    (
        PPC_NO_ERR,
        !quicktime.movie_started
            || quicktime.movie_task_count >= quicktime.movie_tasks_until_done.max(1),
    )
}

pub(crate) fn ppc_qt_dispose_movie(
    cpu: &mut PpcCpu,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
) -> i16 {
    if cpu.gpr[3] != PPC_QT_MOVIE {
        return PPC_PARAM_ERR;
    }
    quicktime.movie_started = false;
    quicktime.movie_disposed = true;
    quicktime.movie_video_track = None;
    quicktime.movie_video_samples = None;
    quicktime.movie_video_decode_cache = None;
    quicktime.movie_audio_track = None;
    ppc_qt_stop_movie_audio(sound);
    PPC_NO_ERR
}

pub(crate) fn ppc_qt_reset_movie_video_decode_cache(quicktime: &mut PpcQuickTimeState) {
    quicktime.movie_video_decode_cache = None;
}

pub(crate) fn ppc_qt_start_movie_audio(quicktime: &PpcQuickTimeState, sound: &mut PpcSoundState) -> bool {
    ppc_qt_stop_movie_audio(sound);
    let Some(decoded_sound) = ppc_qt_decode_movie_audio_samples(quicktime) else {
        return false;
    };
    let Some(file_playback_index) = u32::try_from(sound.file_playbacks.len()).ok() else {
        return false;
    };

    let summary = decoded_sound.summary;
    let samples = decoded_sound.samples;
    if qt_trace_enabled() {
        let non_silent = samples.iter().filter(|sample| **sample != 0x80).count();
        let first_non_silent = samples.iter().position(|sample| *sample != 0x80);
        let last_non_silent = samples.iter().rposition(|sample| *sample != 0x80);
        eprintln!(
            "[QT-TRACE] MovieAudio samples={} non_silent={} first={:?} last={:?} rate={}",
            samples.len(),
            non_silent,
            first_non_silent,
            last_non_silent,
            summary.sample_rate_fixed
        );
    }
    sound.manager.play_file_buffer(
        PPC_QT_MOVIE,
        samples.clone(),
        summary.sample_rate_fixed,
        None,
    );
    sound
        .decoded_file_playbacks
        .push(PpcDecodedAiffPlaybackRecord {
            file_playback_index,
            channel: PPC_QT_MOVIE,
            sample_rate_fixed: summary.sample_rate_fixed,
            samples,
        });
    sound.file_playbacks.push(PpcSoundFilePlaybackRecord {
        channel: PPC_QT_MOVIE,
        ref_num: quicktime.movie_file_ref_num,
        resource_id: 0,
        buffer_size: 0,
        buffer: 0,
        selection: 0,
        completion: 0,
        completion_command: None,
        async_play: false,
        aiff: None,
        decoded_aiff: Some(summary),
    });
    true
}

fn ppc_qt_stop_movie_audio(sound: &mut PpcSoundState) {
    sound.manager.quiet_channel(PPC_QT_MOVIE);
}

pub(crate) fn ppc_qt_decode_movie_audio_samples(quicktime: &PpcQuickTimeState) -> Option<PpcDecodedAiffData> {
    if let Some(decoded) = ppc_qt_movie_audio_samples(&quicktime.movie_file_data) {
        return Some(decoded);
    }

    let track = quicktime.movie_audio_track?;
    let sample_count = track.sample_count.min(track.first_samples_per_chunk);
    let offset = usize::try_from(track.first_chunk_offset).ok()?;
    let frames = usize::try_from(sample_count).ok()?;
    let channels = usize::from(track.channel_count);
    let samples = match track.codec {
        codec if codec == u32::from_be_bytes(*b"ima4") => ppc_qt_decode_ima4_movie_audio_samples(
            &quicktime.movie_file_data,
            offset,
            frames,
            channels,
        )?,
        _ => ppc_qt_decode_pcm_movie_audio_samples(
            &quicktime.movie_file_data,
            offset,
            frames,
            channels,
            usize::from(track.sample_size_bits),
            track.codec,
        )?,
    };
    ppc_decoded_sound_data(samples, track.sample_rate_fixed)
}

fn ppc_qt_build_video_sample_table(
    data: &[u8],
    info: &PpcQuickTimeVideoSampleTableInfo,
) -> Option<PpcQuickTimeVideoSampleTableRecord> {
    let mut sample_index = 0usize;
    let mut media_start_time = 0u64;
    let mut samples = Vec::with_capacity(info.sample_sizes.len());
    for (chunk_index, chunk_offset) in info.chunk_offsets.iter().copied().enumerate() {
        if sample_index >= info.sample_sizes.len() {
            break;
        }
        let chunk_number = u32::try_from(chunk_index).ok()?.saturating_add(1);
        let samples_per_chunk = usize::try_from(ppc_qt_samples_per_chunk_for_chunk(
            &info.stsc_entries,
            chunk_number,
        )?)
        .ok()?;
        let mut sample_offset = chunk_offset;
        for _ in 0..samples_per_chunk {
            if sample_index >= info.sample_sizes.len() {
                break;
            }
            let size = info.sample_sizes[sample_index];
            let duration = ppc_qt_video_sample_duration(info, sample_index)?;
            let (data_len, checksum, preview_len, preview) =
                ppc_qt_sample_payload_summary(data, sample_offset, size);
            samples.push(PpcQuickTimeVideoSampleRecord {
                offset: sample_offset,
                size,
                media_start_time,
                duration,
                data_len,
                checksum,
                preview_len,
                preview,
            });
            sample_offset = sample_offset.checked_add(u64::from(size))?;
            media_start_time = media_start_time.checked_add(u64::from(duration))?;
            sample_index += 1;
        }
    }
    if samples.len() != info.sample_sizes.len() {
        return None;
    }
    Some(PpcQuickTimeVideoSampleTableRecord {
        media_time_scale: info.media_time_scale,
        media_duration: info.media_duration,
        sample_count: u32::try_from(samples.len()).ok()?,
        codec: info.codec,
        samples,
    })
}

fn ppc_qt_video_sample_duration(
    info: &PpcQuickTimeVideoSampleTableInfo,
    sample_index: usize,
) -> Option<u32> {
    if let Some(duration) = info.sample_durations.get(sample_index).copied() {
        if duration != 0 {
            return Some(duration);
        }
    }
    let sample_count = u64::try_from(info.sample_sizes.len()).ok()?;
    if sample_count == 0 || info.media_duration == 0 {
        return None;
    }
    u32::try_from((info.media_duration / sample_count).max(1)).ok()
}

fn ppc_qt_decode_audio_chunks(
    data: &[u8],
    info: &PpcQuickTimeAudioDecodeInfo,
) -> Option<PpcDecodedAiffData> {
    let mut remaining_frames = usize::try_from(info.sample_count).ok()?;
    let mut samples = Vec::with_capacity(remaining_frames);
    for (chunk_index, chunk_offset) in info.chunk_offsets.iter().copied().enumerate() {
        if remaining_frames == 0 {
            break;
        }
        let chunk_number = u32::try_from(chunk_index).ok()?.saturating_add(1);
        let samples_per_chunk = usize::try_from(ppc_qt_samples_per_chunk_for_chunk(
            &info.stsc_entries,
            chunk_number,
        )?)
        .ok()?;
        let frames = remaining_frames.min(samples_per_chunk);
        let offset = usize::try_from(chunk_offset).ok()?;
        let mut chunk_samples = match info.codec {
            codec if codec == u32::from_be_bytes(*b"ima4") => {
                ppc_qt_decode_ima4_movie_audio_samples(
                    data,
                    offset,
                    frames,
                    usize::from(info.channel_count),
                )?
            }
            _ => ppc_qt_decode_pcm_movie_audio_samples(
                data,
                offset,
                frames,
                usize::from(info.channel_count),
                usize::from(info.sample_size_bits),
                info.codec,
            )?,
        };
        remaining_frames -= chunk_samples.len();
        samples.append(&mut chunk_samples);
    }
    if samples.is_empty() {
        return None;
    }
    ppc_decoded_sound_data(samples, info.sample_rate_fixed)
}

pub(crate) fn ppc_qt_synthesize_music_track(
    data: &[u8],
    info: &PpcQuickTimeMusicDecodeInfo,
) -> Option<PpcDecodedAiffData> {
    const SAMPLE_RATE: u32 = 22_050;
    const MAX_MUSIC_SECONDS: u64 = 600;

    let total_samples = info
        .media_duration
        .checked_mul(u64::from(SAMPLE_RATE))?
        .checked_div(u64::from(info.media_time_scale))?
        .min(u64::from(SAMPLE_RATE) * MAX_MUSIC_SECONDS);
    let mut mixed = vec![0i32; usize::try_from(total_samples).ok()?];
    let mut sample_index = 0usize;
    let mut sample_start_time = 0u64;
    let mut parts = Vec::<PpcQuickTimeMusicPartState>::new();

    for (chunk_index, chunk_offset) in info.chunk_offsets.iter().copied().enumerate() {
        if sample_index >= info.sample_sizes.len() {
            break;
        }
        let chunk_number = u32::try_from(chunk_index).ok()?.saturating_add(1);
        let chunk_description = ppc_qt_stsc_entry_for_chunk(&info.stsc_entries, chunk_number)?;
        let samples_per_chunk = usize::try_from(chunk_description.samples_per_chunk).ok()?;
        if let Some(description_index) = usize::try_from(chunk_description.sample_description_id)
            .ok()?
            .checked_sub(1)
        {
            let description = info.sample_descriptions.get(description_index)?;
            for requested in description {
                ppc_qt_music_part_state(&mut parts, requested.part).instrument =
                    ppc_qt_music_instrument(*requested);
            }
        }
        let mut sample_offset = usize::try_from(chunk_offset).ok()?;
        for _ in 0..samples_per_chunk {
            let sample_size = usize::try_from(*info.sample_sizes.get(sample_index)?).ok()?;
            let sample_end = sample_offset.checked_add(sample_size)?;
            let sample = data.get(sample_offset..sample_end)?;
            ppc_qt_synthesize_music_events(
                sample,
                info.media_time_scale,
                sample_start_time,
                &mut mixed,
                SAMPLE_RATE,
                &mut parts,
            )?;
            sample_offset = sample_end;
            sample_start_time = sample_start_time
                .checked_add(u64::from(*info.sample_durations.get(sample_index)?))?;
            sample_index += 1;
            if sample_index >= info.sample_sizes.len() {
                break;
            }
        }
    }
    if sample_index != info.sample_sizes.len() {
        return None;
    }
    if !mixed.iter().any(|sample| *sample != 0) {
        return None;
    }

    let samples = mixed
        .into_iter()
        .map(|sample| (sample + 128).clamp(0, 255) as u8)
        .collect::<Vec<_>>();
    ppc_decoded_sound_data(samples, SAMPLE_RATE << 16)
}

pub(crate) fn ppc_qt_synthesize_music_events(
    data: &[u8],
    media_time_scale: u32,
    sample_start_time: u64,
    mixed: &mut [i32],
    sample_rate: u32,
    parts: &mut Vec<PpcQuickTimeMusicPartState>,
) -> Option<()> {
    // QuickTime Music Architecture (1997), pp. 19-30.
    let mut offset = 0usize;
    let mut local_time = 0u64;
    let mut reserved_end_marker_seen = false;
    while offset.checked_add(4)? <= data.len() {
        let word = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
        let short_type = (word >> 29) & 0x7;
        match short_type {
            0 => {
                local_time = local_time.checked_add(u64::from(word & 0x00ff_ffff))?;
                offset += 4;
            }
            1 => {
                let part = (word >> 24) & 0x1f;
                let pitch = ((word >> 18) & 0x3f) + 32;
                let velocity = (word >> 11) & 0x7f;
                let duration = word & 0x7ff;
                let state = *ppc_qt_music_part_state(parts, part as u16);
                ppc_qt_mix_music_note(
                    mixed,
                    sample_start_time.checked_add(local_time)?,
                    duration,
                    pitch as f64,
                    velocity,
                    state,
                    media_time_scale,
                    sample_rate,
                )?;
                offset += 4;
            }
            2 => {
                let part = ((word >> 24) & 0x1f) as u16;
                let controller = ((word >> 16) & 0xff) as u16;
                let value = word as u16 as i16;
                ppc_qt_apply_music_controller(
                    ppc_qt_music_part_state(parts, part),
                    controller,
                    value,
                );
                offset += 4;
            }
            3 => {
                if ((word >> 16) & 0xff) == 0 {
                    if word & 0xffff == 0 {
                        return (offset + 4 == data.len()).then_some(());
                    }
                    reserved_end_marker_seen = true;
                }
                offset += 4;
            }
            _ => {
                let extended_type = word >> 28;
                if extended_type == 9 {
                    let tail =
                        u32::from_be_bytes(data.get(offset + 4..offset + 8)?.try_into().ok()?);
                    if tail >> 30 != 2 {
                        return None;
                    }
                    let part = (word >> 16) & 0x0fff;
                    let pitch_bits = word & 0xffff;
                    let pitch = if pitch_bits < 128 {
                        f64::from(pitch_bits)
                    } else {
                        f64::from(pitch_bits) / 256.0
                    };
                    let velocity = (tail >> 22) & 0x7f;
                    let duration = tail & 0x003f_ffff;
                    let state = *ppc_qt_music_part_state(parts, part as u16);
                    ppc_qt_mix_music_note(
                        mixed,
                        sample_start_time.checked_add(local_time)?,
                        duration,
                        pitch,
                        velocity,
                        state,
                        media_time_scale,
                        sample_rate,
                    )?;
                    offset += 8;
                } else if extended_type == 10 {
                    let tail =
                        u32::from_be_bytes(data.get(offset + 4..offset + 8)?.try_into().ok()?);
                    if tail >> 30 != 2 {
                        return None;
                    }
                    let part = ((word >> 16) & 0x0fff) as u16;
                    ppc_qt_apply_music_controller(
                        ppc_qt_music_part_state(parts, part),
                        (word & 0xffff) as u16,
                        tail as u16 as i16,
                    );
                    offset += 8;
                } else if extended_type == 15 {
                    let word_count = usize::try_from(word & 0xffff).ok()?;
                    if word_count < 2 {
                        return None;
                    }
                    let event_end = offset.checked_add(word_count.checked_mul(4)?)?;
                    let tail = u32::from_be_bytes(
                        data.get(event_end.checked_sub(4)?..event_end)?
                            .try_into()
                            .ok()?,
                    );
                    if tail >> 30 != 3 || tail & 0xffff != word & 0xffff {
                        return None;
                    }
                    offset = event_end;
                } else {
                    let tail =
                        u32::from_be_bytes(data.get(offset + 4..offset + 8)?.try_into().ok()?);
                    if tail >> 30 != 2 {
                        return None;
                    }
                    offset += 8;
                }
            }
        }
    }
    (offset == data.len() && !reserved_end_marker_seen).then_some(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PpcQuickTimeMusicPartState {
    pub(crate) part: u16,
    pub(crate) instrument: u32,
    pub(crate) volume: i16,
    pub(crate) pitch_bend: i16,
    pub(crate) sustain: bool,
}

fn ppc_qt_music_part_state(
    parts: &mut Vec<PpcQuickTimeMusicPartState>,
    part: u16,
) -> &mut PpcQuickTimeMusicPartState {
    if let Some(index) = parts.iter().position(|state| state.part == part) {
        return &mut parts[index];
    }
    parts.push(PpcQuickTimeMusicPartState {
        part,
        instrument: 1,
        volume: i16::MAX,
        pitch_bend: 0,
        sustain: false,
    });
    parts.last_mut().unwrap()
}

fn ppc_qt_music_instrument(part: PpcQuickTimeMusicPart) -> u32 {
    if (1..=128).contains(&part.instrument_number)
        || (16_384..=16_512).contains(&part.instrument_number)
    {
        part.instrument_number
    } else if (1..=128).contains(&part.gm_number) || (16_384..=16_512).contains(&part.gm_number) {
        part.gm_number
    } else {
        1
    }
}

fn ppc_qt_apply_music_controller(
    state: &mut PpcQuickTimeMusicPartState,
    controller: u16,
    value: i16,
) {
    match controller {
        7 => state.volume = value.max(0),
        32 => state.pitch_bend = value,
        64 => state.sustain = value > 0,
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_qt_mix_music_note(
    mixed: &mut [i32],
    start_time: u64,
    duration: u32,
    pitch: f64,
    velocity: u32,
    state: PpcQuickTimeMusicPartState,
    media_time_scale: u32,
    sample_rate: u32,
) -> Option<()> {
    // QuickTime Musical Instruments playback uses most of the signed 8-bit
    // output range. Keep headroom for overlapping notes while matching that
    // mixer level instead of leaving each synthesized part near silence.
    const OUTPUT_GAIN: i32 = 6;
    if duration == 0 || velocity == 0 || media_time_scale == 0 {
        return Some(());
    }
    let start = start_time
        .checked_mul(u64::from(sample_rate))?
        .checked_div(u64::from(media_time_scale))?;
    let length = u64::from(duration)
        .checked_mul(u64::from(sample_rate))?
        .checked_div(u64::from(media_time_scale))?
        .max(1);
    let start = usize::try_from(start).ok()?.min(mixed.len());
    let end = usize::try_from(u64::try_from(start).ok()?.checked_add(length)?)
        .ok()?
        .min(mixed.len());
    if !pitch.is_finite() {
        return Some(());
    }
    let pitch_fixed = (pitch * 256.0) as i32 + i32::from(state.pitch_bend);
    let phase_step = ppc_qt_music_phase_step(pitch_fixed, sample_rate)?;
    let note_len = end.saturating_sub(start);
    let mut phase = 0u32;
    let instrument = state.instrument;
    let drum = (16_384..=16_512).contains(&instrument);
    let family = instrument.saturating_sub(1).min(127) / 8;
    let mut noise = (start_time as u32)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add((pitch_fixed as u32).rotate_left(13))
        .wrapping_add(instrument);
    for (index, destination) in mixed[start..end].iter_mut().enumerate() {
        let position = (phase >> 16) as i32;
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        let triangle = if position < 32_768 {
            position * 2 - 32_768
        } else {
            98_303 - position * 2
        };
        let saw = position - 32_768;
        let square = if position < 32_768 { 24_576 } else { -24_576 };
        let random = (noise >> 16) as i16 as i32;
        let waveform = if drum {
            match pitch.round() as i32 {
                35 | 36 => triangle * 3 / 4 + square / 4,
                38 | 40 => random * 3 / 4 + triangle / 4,
                42 | 44 | 46 | 49 | 51 | 52 | 55 | 57 | 59 => random,
                _ => random / 2 + triangle / 2,
            }
        } else {
            match family {
                0 => triangle * 3 / 4 + saw / 4,
                1 => triangle / 2 + square / 2,
                2 => square * 3 / 4 + triangle / 4,
                3 => saw / 2 + triangle / 2,
                4 => square / 2 + saw / 2,
                5 | 6 => triangle * 3 / 4 + square / 4,
                7 | 8 => saw * 3 / 4 + square / 4,
                9 => triangle,
                10 => saw,
                11 => triangle * 3 / 4 + saw / 4,
                12 => saw / 2 + random / 2,
                13 => triangle / 2 + saw / 2,
                14 => random / 2 + square / 2,
                _ => random,
            }
        };
        let edge_envelope = index.min(note_len.saturating_sub(index + 1)).min(63) as i32 + 1;
        let decay_envelope = match family {
            0 | 1 | 3 | 14 => i32::try_from(note_len.saturating_sub(index)).unwrap_or(i32::MAX),
            _ => i32::try_from(note_len).unwrap_or(i32::MAX),
        }
        .max(1);
        let volume = i32::from(state.volume).clamp(0, i32::from(i16::MAX));
        let amplitude = waveform * velocity as i32 * edge_envelope / (127 * 64 * 4096) * volume
            / i32::from(i16::MAX)
            * decay_envelope
            / i32::try_from(note_len.max(1)).unwrap_or(i32::MAX)
            * OUTPUT_GAIN;
        *destination = destination.saturating_add(amplitude);
        phase = phase.wrapping_add(phase_step);
    }
    Some(())
}

fn ppc_qt_music_phase_step(pitch_fixed: i32, sample_rate: u32) -> Option<u32> {
    const STEPS_22_050: [u32; 128] = [
        1592507, 1687203, 1787529, 1893821, 2006434, 2125742, 2252146, 2386065, 2527948, 2678268,
        2837526, 3006254, 3185015, 3374406, 3575058, 3787642, 4012867, 4251485, 4504291, 4772130,
        5055896, 5356535, 5675051, 6012507, 6370030, 6748811, 7150117, 7575285, 8025735, 8502970,
        9008582, 9544261, 10111792, 10713070, 11350103, 12025015, 12740059, 13497623, 14300233,
        15150569, 16051469, 17005939, 18017165, 19088521, 20223584, 21426141, 22700205, 24050030,
        25480119, 26995246, 28600467, 30301139, 32102938, 34011878, 36034330, 38177043, 40447168,
        42852281, 45400411, 48100060, 50960238, 53990491, 57200933, 60602278, 64205876, 68023757,
        72068660, 76354085, 80894335, 85704563, 90800821, 96200119, 101920476, 107980983,
        114401866, 121204555, 128411753, 136047513, 144137319, 152708170, 161788671, 171409126,
        181601643, 192400238, 203840952, 215961966, 228803732, 242409110, 256823506, 272095026,
        288274639, 305416341, 323577341, 342818251, 363203285, 384800477, 407681904, 431923931,
        457607465, 484818220, 513647012, 544190053, 576549277, 610832681, 647154683, 685636503,
        726406571, 769600953, 815363807, 863847862, 915214929, 969636441, 1027294024, 1088380105,
        1153098554, 1221665363, 1294309365, 1371273005, 1452813141, 1539201906, 1630727614,
        1727695724, 1830429858, 1939272882, 2054588048, 2176760211, 2306197109, 2443330725,
    ];

    if sample_rate == 0 {
        return None;
    }
    let pitch_fixed = pitch_fixed.clamp(0, 127 * 256);
    let note = usize::try_from(pitch_fixed / 256).ok()?;
    let fraction = u64::try_from(pitch_fixed % 256).ok()?;
    let first = u64::from(STEPS_22_050[note]);
    let second = u64::from(*STEPS_22_050.get(note + 1).unwrap_or(&STEPS_22_050[note]));
    let interpolated = first.checked_add(second.saturating_sub(first) * fraction / 256)?;
    u32::try_from(
        interpolated
            .checked_mul(22_050)?
            .checked_div(u64::from(sample_rate))?,
    )
    .ok()
}

fn ppc_qt_stsc_entry_for_chunk(
    entries: &[PpcQuickTimeStscEntry],
    chunk_number: u32,
) -> Option<PpcQuickTimeStscEntry> {
    for (index, entry) in entries.iter().copied().enumerate() {
        let next_first_chunk = entries
            .get(index + 1)
            .map(|next| next.first_chunk)
            .unwrap_or(u32::MAX);
        if chunk_number >= entry.first_chunk && chunk_number < next_first_chunk {
            return Some(entry);
        }
    }
    None
}

fn ppc_qt_samples_per_chunk_for_chunk(
    entries: &[PpcQuickTimeStscEntry],
    chunk_number: u32,
) -> Option<u32> {
    let mut result = None;
    for (index, entry) in entries.iter().enumerate() {
        let next_first_chunk = entries
            .get(index + 1)
            .map(|next| next.first_chunk)
            .unwrap_or(u32::MAX);
        if chunk_number >= entry.first_chunk && chunk_number < next_first_chunk {
            result = Some(entry.samples_per_chunk);
            break;
        }
    }
    result
}

fn ppc_qt_decode_pcm_movie_audio_samples(
    data: &[u8],
    data_start: usize,
    frames: usize,
    channels: usize,
    sample_size_bits: usize,
    codec: u32,
) -> Option<Vec<u8>> {
    if frames == 0 || channels == 0 {
        return None;
    }
    let bytes_per_sample = match sample_size_bits {
        8 => 1usize,
        16 => 2usize,
        _ => return None,
    };
    let frame_bytes = channels.checked_mul(bytes_per_sample)?;
    let byte_count = frames.checked_mul(frame_bytes)?;
    let raw = data.get(data_start..data_start.checked_add(byte_count)?)?;
    let mut samples = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut accum = 0i32;
        let frame_start = frame.checked_mul(frame_bytes)?;
        for channel in 0..channels {
            let offset = frame_start.checked_add(channel.checked_mul(bytes_per_sample)?)?;
            let sample = match (codec, sample_size_bits) {
                (codec, 8) if codec == u32::from_be_bytes(*b"raw ") => raw[offset] as i32 - 128,
                (codec, 8) if codec == u32::from_be_bytes(*b"twos") => raw[offset] as i8 as i32,
                (codec, 16) if codec == u32::from_be_bytes(*b"twos") => {
                    i16::from_be_bytes([raw[offset], raw[offset + 1]]) as i32 >> 8
                }
                _ => return None,
            };
            accum += sample;
        }
        samples.push((accum / channels as i32 + 128).clamp(0, 255) as u8);
    }
    Some(samples)
}

fn ppc_qt_decode_ima4_movie_audio_samples(
    data: &[u8],
    data_start: usize,
    frames: usize,
    channels: usize,
) -> Option<Vec<u8>> {
    const FRAMES_PER_PACKET: usize = 64;
    const BYTES_PER_CHANNEL_PACKET: usize = 34;

    if frames == 0 || channels == 0 {
        return None;
    }
    let packets = frames
        .checked_add(FRAMES_PER_PACKET - 1)?
        .checked_div(FRAMES_PER_PACKET)?;
    let packet_bytes = channels.checked_mul(BYTES_PER_CHANNEL_PACKET)?;
    let byte_count = packets.checked_mul(packet_bytes)?;
    let raw = data.get(data_start..data_start.checked_add(byte_count)?)?;
    let mut samples = Vec::with_capacity(frames);
    for packet in 0..packets {
        let packet_start = packet.checked_mul(packet_bytes)?;
        let mut decoded_channels = Vec::with_capacity(channels);
        for channel in 0..channels {
            let block_start =
                packet_start.checked_add(channel.checked_mul(BYTES_PER_CHANNEL_PACKET)?)?;
            decoded_channels.push(ppc_qt_decode_ima4_channel_packet(
                raw.get(block_start..block_start + BYTES_PER_CHANNEL_PACKET)?,
            )?);
        }

        let frames_this_packet = (frames - samples.len()).min(FRAMES_PER_PACKET);
        for frame in 0..frames_this_packet {
            let mut accum = 0i32;
            for channel_samples in &decoded_channels {
                accum += i32::from(channel_samples[frame]);
            }
            let mono = (((accum / channels as i32) >> 8) + 128).clamp(0, 255) as u8;
            samples.push(mono);
        }
    }
    Some(samples)
}

fn ppc_qt_decode_ima4_channel_packet(packet: &[u8]) -> Option<[i16; 64]> {
    const STEP_TABLE: [i32; 89] = [
        7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60,
        66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371,
        408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878,
        2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845,
        8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086,
        29794, 32767,
    ];
    const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

    if packet.len() != 34 {
        return None;
    }
    let header = u16::from_be_bytes([packet[0], packet[1]]);
    let mut predictor = i32::from((header & 0xff80) as i16);
    let mut step_index = usize::from(header & 0x007f).min(STEP_TABLE.len() - 1);
    let mut decoded = [0i16; 64];
    let mut output_index = 0usize;
    for byte in packet.iter().copied().skip(2) {
        for nibble in [byte & 0x0f, byte >> 4] {
            let step = STEP_TABLE[step_index];
            let mut diff = step >> 3;
            if (nibble & 0x01) != 0 {
                diff += step >> 2;
            }
            if (nibble & 0x02) != 0 {
                diff += step >> 1;
            }
            if (nibble & 0x04) != 0 {
                diff += step;
            }
            if (nibble & 0x08) != 0 {
                predictor -= diff;
            } else {
                predictor += diff;
            }
            predictor = predictor.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            let next_index = step_index as i32 + INDEX_TABLE[usize::from(nibble)];
            step_index = next_index.clamp(0, (STEP_TABLE.len() - 1) as i32) as usize;
            decoded[output_index] = predictor as i16;
            output_index += 1;
        }
    }
    Some(decoded)
}



pub(crate) struct PpcQuickTimeCinepakDecoder {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) frame: Vec<u8>,
    pub(crate) strips: Vec<PpcQuickTimeCinepakStripState>,
}

pub(crate) struct PpcQuickTimeDecodedVideoFrame {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) rgb: Vec<u8>,
}

impl PpcQuickTimeCinepakDecoder {
    pub(crate) const MAX_STRIPS: usize = 32;

    pub(crate) fn new(width: usize, height: usize) -> Option<Self> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return None;
        }
        let pixel_bytes = width.checked_mul(height)?.checked_mul(3)?;
        Some(Self {
            width,
            height,
            frame: vec![0; pixel_bytes],
            strips: vec![PpcQuickTimeCinepakStripState::default(); Self::MAX_STRIPS],
        })
    }

    pub(crate) fn decode_sample(&mut self, sample: &[u8]) -> Option<()> {
        if sample.len() < 10 {
            return None;
        }
        let frame_flags = sample[0];
        let encoded_size = ppc_qt_read_u24(sample, 1)?;
        if encoded_size < 10 || encoded_size > sample.len() {
            return None;
        }
        let width = usize::from(u16::from_be_bytes([sample[4], sample[5]]));
        let height = usize::from(u16::from_be_bytes([sample[6], sample[7]]));
        if width != self.width || height != self.height {
            return None;
        }
        let strip_count =
            usize::from(u16::from_be_bytes([sample[8], sample[9]])).min(Self::MAX_STRIPS);
        let mut offset = 10usize;
        let end = encoded_size;
        let mut y_cursor = 0usize;
        for strip_index in 0..strip_count {
            if offset.checked_add(12)? > end {
                return None;
            }
            let strip_id = sample[offset];
            if strip_id != 0x10 && strip_id != 0x11 {
                return None;
            }
            let strip_size = ppc_qt_read_u24(sample, offset + 1)?;
            if strip_size < 12 {
                return None;
            }
            let raw_y1 = usize::from(u16::from_be_bytes([sample[offset + 4], sample[offset + 5]]));
            let x1 = usize::from(u16::from_be_bytes([sample[offset + 6], sample[offset + 7]]));
            let raw_y2 = usize::from(u16::from_be_bytes([sample[offset + 8], sample[offset + 9]]));
            let x2 = usize::from(u16::from_be_bytes([
                sample[offset + 10],
                sample[offset + 11],
            ]));
            let (y1, y2) = if raw_y1 == 0 {
                (y_cursor, y_cursor.checked_add(raw_y2)?)
            } else {
                (raw_y1, raw_y2)
            };
            if x1 >= x2 || y1 >= y2 || x1 >= self.width || y1 >= self.height {
                return None;
            }
            offset += 12;
            let strip_payload_size = strip_size - 12;
            let strip_end = offset.checked_add(strip_payload_size)?;
            if strip_end > end {
                return None;
            }
            if strip_index > 0 && (frame_flags & 0x01) == 0 {
                self.strips[strip_index] = self.strips[strip_index - 1].clone();
            }
            ppc_qt_decode_cinepak_strip(
                &mut self.frame,
                self.width,
                self.height,
                &mut self.strips[strip_index],
                sample.get(offset..strip_end)?,
                (x1, y1, x2.min(self.width), y2.min(self.height)),
            )?;
            offset = strip_end;
            y_cursor = y2;
        }
        Some(())
    }

    pub(crate) fn decoded_frame(&self) -> PpcQuickTimeDecodedVideoFrame {
        PpcQuickTimeDecodedVideoFrame {
            width: self.width,
            height: self.height,
            rgb: self.frame.clone(),
        }
    }

    fn from_cache(cache: &PpcQuickTimeVideoDecodeCacheRecord) -> Option<Self> {
        if cache.width == 0
            || cache.height == 0
            || cache.rgb.len() != cache.width.checked_mul(cache.height)?.checked_mul(3)?
            || cache.cinepak_strips.len() != Self::MAX_STRIPS
        {
            return None;
        }
        Some(Self {
            width: cache.width,
            height: cache.height,
            frame: cache.rgb.clone(),
            strips: cache.cinepak_strips.clone(),
        })
    }

    fn cache_record(&self, codec: u32, sample_index: usize) -> PpcQuickTimeVideoDecodeCacheRecord {
        PpcQuickTimeVideoDecodeCacheRecord {
            codec,
            width: self.width,
            height: self.height,
            sample_index,
            rgb: self.frame.clone(),
            cinepak_strips: self.strips.clone(),
        }
    }
}

fn ppc_qt_read_u24(data: &[u8], offset: usize) -> Option<usize> {
    let bytes = data.get(offset..offset.checked_add(3)?)?;
    Some((usize::from(bytes[0]) << 16) | (usize::from(bytes[1]) << 8) | usize::from(bytes[2]))
}

pub(crate) fn ppc_qt_decode_current_movie_video_frame(
    quicktime: &mut PpcQuickTimeState,
) -> Option<PpcQuickTimeDecodedVideoFrame> {
    let samples = quicktime.movie_video_samples.clone()?;
    if samples.codec != u32::from_be_bytes(*b"cvid") || samples.samples.is_empty() {
        return None;
    }
    let sample_index = ppc_qt_movie_timed_sample_index(quicktime, &samples)?;
    let first_sample = ppc_qt_movie_video_sample_bytes(&quicktime.movie_file_data, &samples, 0)?;
    if first_sample.len() < 10 {
        return None;
    }
    let width = usize::from(u16::from_be_bytes([first_sample[4], first_sample[5]]));
    let height = usize::from(u16::from_be_bytes([first_sample[6], first_sample[7]]));

    let mut start_index = 0usize;
    let mut decoder = if let Some(cache) = quicktime
        .movie_video_decode_cache
        .as_ref()
        .filter(|cache| {
            cache.codec == samples.codec
                && cache.width == width
                && cache.height == height
                && cache.sample_index <= sample_index
        })
        .and_then(PpcQuickTimeCinepakDecoder::from_cache)
    {
        start_index = quicktime
            .movie_video_decode_cache
            .as_ref()?
            .sample_index
            .saturating_add(1);
        cache
    } else {
        PpcQuickTimeCinepakDecoder::new(width, height)?
    };
    for index in start_index..=sample_index {
        let sample = ppc_qt_movie_video_sample_bytes(&quicktime.movie_file_data, &samples, index)?;
        decoder.decode_sample(sample)?;
    }
    let frame = decoder.decoded_frame();
    quicktime.movie_video_decode_cache = Some(decoder.cache_record(samples.codec, sample_index));
    Some(frame)
}

fn ppc_qt_movie_video_sample_bytes<'a>(
    data: &'a [u8],
    samples: &PpcQuickTimeVideoSampleTableRecord,
    index: usize,
) -> Option<&'a [u8]> {
    let sample = samples.samples.get(index)?;
    let offset = usize::try_from(sample.offset).ok()?;
    let size = usize::try_from(sample.size).ok()?;
    data.get(offset..offset.checked_add(size)?)
}

fn ppc_qt_decode_cinepak_strip(
    frame: &mut [u8],
    width: usize,
    height: usize,
    strip: &mut PpcQuickTimeCinepakStripState,
    data: &[u8],
    bounds: (usize, usize, usize, usize),
) -> Option<()> {
    let mut offset = 0usize;
    while offset.checked_add(4)? <= data.len() {
        let chunk_id = data[offset];
        let chunk_size = ppc_qt_read_u24(data, offset + 1)?;
        if chunk_size < 4 {
            return None;
        }
        offset += 4;
        let payload_size = chunk_size - 4;
        let payload_end = offset.checked_add(payload_size)?;
        let payload = data.get(offset..payload_end)?;
        match chunk_id {
            0x20 | 0x21 | 0x24 | 0x25 => {
                ppc_qt_decode_cinepak_codebook(&mut strip.v4_codebook, chunk_id, payload);
            }
            0x22 | 0x23 | 0x26 | 0x27 => {
                ppc_qt_decode_cinepak_codebook(&mut strip.v1_codebook, chunk_id, payload);
            }
            0x30 | 0x31 | 0x32 => {
                ppc_qt_decode_cinepak_vectors(
                    frame, width, height, strip, chunk_id, payload, bounds,
                )?;
                return Some(());
            }
            _ => {}
        }
        offset = payload_end;
    }
    None
}

fn ppc_qt_decode_cinepak_codebook(codebook: &mut [[u8; 12]; 256], chunk_id: u8, data: &[u8]) {
    let entry_len = if (chunk_id & 0x04) != 0 { 4 } else { 6 };
    let flagged = (chunk_id & 0x01) != 0;
    let mut offset = 0usize;
    let mut flags = 0u32;
    let mut mask = 0u32;
    for entry in codebook.iter_mut() {
        if flagged {
            mask >>= 1;
            if mask == 0 {
                let Some(flag_bytes) = data.get(offset..offset.saturating_add(4)) else {
                    break;
                };
                flags = u32::from_be_bytes([
                    flag_bytes[0],
                    flag_bytes[1],
                    flag_bytes[2],
                    flag_bytes[3],
                ]);
                offset += 4;
                mask = 0x8000_0000;
            }
            if (flags & mask) == 0 {
                continue;
            }
        }
        let Some(raw) = data.get(offset..offset.saturating_add(entry_len)) else {
            break;
        };
        offset += entry_len;
        let y = [raw[0], raw[1], raw[2], raw[3]];
        if entry_len == 4 {
            for (index, luma) in y.iter().copied().enumerate() {
                let base = index * 3;
                entry[base] = luma;
                entry[base + 1] = luma;
                entry[base + 2] = luma;
            }
        } else {
            let u = raw[4] as i8 as i32;
            let v = raw[5] as i8 as i32;
            for (index, luma) in y.iter().copied().enumerate() {
                let luma = i32::from(luma);
                let base = index * 3;
                entry[base] = luma.saturating_add(v.saturating_mul(2)).clamp(0, 255) as u8;
                entry[base + 1] = luma.saturating_sub(u / 2).saturating_sub(v).clamp(0, 255) as u8;
                entry[base + 2] = luma.saturating_add(u.saturating_mul(2)).clamp(0, 255) as u8;
            }
        }
    }
}

fn ppc_qt_decode_cinepak_vectors(
    frame: &mut [u8],
    width: usize,
    height: usize,
    strip: &PpcQuickTimeCinepakStripState,
    chunk_id: u8,
    data: &[u8],
    (x1, y1, x2, y2): (usize, usize, usize, usize),
) -> Option<()> {
    let mut offset = 0usize;
    let mut flags = 0u32;
    let mut mask = 0u32;
    for y in (y1..y2).step_by(4) {
        for x in (x1..x2).step_by(4) {
            let should_decode = if (chunk_id & 0x01) != 0 {
                ppc_qt_next_cinepak_flag(data, &mut offset, &mut flags, &mut mask)?
            } else {
                true
            };
            if !should_decode {
                continue;
            }
            let use_v1 = if (chunk_id & 0x02) != 0 {
                true
            } else {
                !ppc_qt_next_cinepak_flag(data, &mut offset, &mut flags, &mut mask)?
            };
            if use_v1 {
                let index = usize::from(*data.get(offset)?);
                offset += 1;
                ppc_qt_put_cinepak_v1_block(frame, width, height, x, y, &strip.v1_codebook[index])?;
            } else {
                let indices = data.get(offset..offset.checked_add(4)?)?;
                offset += 4;
                ppc_qt_put_cinepak_v4_block(
                    frame,
                    width,
                    height,
                    x,
                    y,
                    [
                        &strip.v4_codebook[usize::from(indices[0])],
                        &strip.v4_codebook[usize::from(indices[1])],
                        &strip.v4_codebook[usize::from(indices[2])],
                        &strip.v4_codebook[usize::from(indices[3])],
                    ],
                )?;
            }
        }
    }
    Some(())
}

fn ppc_qt_next_cinepak_flag(
    data: &[u8],
    offset: &mut usize,
    flags: &mut u32,
    mask: &mut u32,
) -> Option<bool> {
    *mask >>= 1;
    if *mask == 0 {
        let flag_bytes = data.get(*offset..(*offset).checked_add(4)?)?;
        *flags = u32::from_be_bytes([flag_bytes[0], flag_bytes[1], flag_bytes[2], flag_bytes[3]]);
        *offset = (*offset).checked_add(4)?;
        *mask = 0x8000_0000;
    }
    Some((*flags & *mask) != 0)
}

fn ppc_qt_put_cinepak_v1_block(
    frame: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    codebook: &[u8; 12],
) -> Option<()> {
    for dy in 0..4usize {
        for dx in 0..4usize {
            let codebook_index = ((dy / 2) * 2) + (dx / 2);
            let color_offset = codebook_index.checked_mul(3)?;
            ppc_qt_write_cinepak_rgb(
                frame,
                width,
                height,
                x.checked_add(dx)?,
                y.checked_add(dy)?,
                [
                    codebook[color_offset],
                    codebook[color_offset + 1],
                    codebook[color_offset + 2],
                ],
            )?;
        }
    }
    Some(())
}

fn ppc_qt_put_cinepak_v4_block(
    frame: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    codebooks: [&[u8; 12]; 4],
) -> Option<()> {
    for dy in 0..4usize {
        for dx in 0..4usize {
            let codebook_index = if dy >= 2 { 2 } else { 0 } + if dx >= 2 { 1 } else { 0 };
            let pixel_index = ((dy % 2) * 2) + (dx % 2);
            let color_offset = pixel_index.checked_mul(3)?;
            let codebook = codebooks[codebook_index];
            ppc_qt_write_cinepak_rgb(
                frame,
                width,
                height,
                x.checked_add(dx)?,
                y.checked_add(dy)?,
                [
                    codebook[color_offset],
                    codebook[color_offset + 1],
                    codebook[color_offset + 2],
                ],
            )?;
        }
    }
    Some(())
}

fn ppc_qt_write_cinepak_rgb(
    frame: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rgb: [u8; 3],
) -> Option<()> {
    if x >= width || y >= height {
        return Some(());
    }
    let offset = y.checked_mul(width)?.checked_add(x)?.checked_mul(3)?;
    let pixel = frame.get_mut(offset..offset.checked_add(3)?)?;
    pixel.copy_from_slice(&rgb);
    Some(())
}

pub(crate) fn ppc_qt_draw_movie_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
    salt: u32,
) -> bool {
    let gworld = if quicktime.movie_gworld != 0 {
        quicktime.movie_gworld
    } else {
        current_gworld
    };
    let Some(front_buffer) = ppc_live_front_buffer_for_gworld(memory, gworlds, gworld) else {
        return false;
    };
    if ppc_qt_draw_decoded_movie_frame(memory, front_buffer, quicktime) {
        return true;
    }
    ppc_qt_draw_visible_16bpp_frame(
        memory,
        front_buffer,
        ppc_qt_movie_frame_salt(quicktime, salt),
    )
}

fn ppc_qt_draw_decoded_movie_frame(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    quicktime: &mut PpcQuickTimeState,
) -> bool {
    if front_buffer.depth != 16
        || front_buffer.base_addr == 0
        || front_buffer.width == 0
        || front_buffer.height == 0
        || front_buffer.row_bytes < front_buffer.width.saturating_mul(2)
    {
        return false;
    }
    let Some(frame) = ppc_qt_decode_current_movie_video_frame(quicktime) else {
        return false;
    };
    let (top, left, bottom, right) = quicktime.movie_box;
    let box_width = i32::from(right) - i32::from(left);
    let box_height = i32::from(bottom) - i32::from(top);
    if box_width <= 0 || box_height <= 0 {
        return false;
    }
    let dst_left = i32::from(left).max(0).min(front_buffer.width as i32);
    let dst_top = i32::from(top).max(0).min(front_buffer.height as i32);
    let dst_right = i32::from(right).max(0).min(front_buffer.width as i32);
    let dst_bottom = i32::from(bottom).max(0).min(front_buffer.height as i32);
    if dst_left >= dst_right || dst_top >= dst_bottom {
        return false;
    }

    let mut wrote_any = false;
    for dst_y in dst_top..dst_bottom {
        let src_y = usize::try_from(
            ((dst_y - i32::from(top)) as i64)
                .saturating_mul(frame.height as i64)
                .checked_div(i64::from(box_height))
                .unwrap_or(0)
                .clamp(0, frame.height.saturating_sub(1) as i64),
        )
        .unwrap_or(0);
        for dst_x in dst_left..dst_right {
            let src_x = usize::try_from(
                ((dst_x - i32::from(left)) as i64)
                    .saturating_mul(frame.width as i64)
                    .checked_div(i64::from(box_width))
                    .unwrap_or(0)
                    .clamp(0, frame.width.saturating_sub(1) as i64),
            )
            .unwrap_or(0);
            let Some(rgb_offset) = src_y
                .checked_mul(frame.width)
                .and_then(|base| base.checked_add(src_x))
                .and_then(|pixel| pixel.checked_mul(3))
            else {
                continue;
            };
            let Some(rgb) = frame.rgb.get(rgb_offset..rgb_offset.saturating_add(3)) else {
                continue;
            };
            let pixel = ppc_qt_rgb555_from_u8(rgb[0], rgb[1], rgb[2]);
            if ppc_q3_write_software_pixel(memory, front_buffer, (dst_x, dst_y), pixel) {
                wrote_any = true;
            }
        }
    }
    wrote_any
}

pub(crate) fn ppc_qt_rgb555_from_u8(red: u8, green: u8, blue: u8) -> u16 {
    fn component(value: u8) -> u16 {
        ((u16::from(value) * 31) + 127) / 255
    }
    (component(red) << 10) | (component(green) << 5) | component(blue)
}

pub(crate) fn ppc_qt_movie_frame_salt(quicktime: &PpcQuickTimeState, fallback_salt: u32) -> u32 {
    if let Some(samples) = &quicktime.movie_video_samples {
        if !samples.samples.is_empty() {
            let sample_index = ppc_qt_movie_timed_sample_index(quicktime, samples).unwrap_or(0);
            let sample = &samples.samples[sample_index];
            return 0x70u32
                .saturating_add(u32::try_from(sample_index).unwrap_or(u32::MAX))
                .saturating_add(sample.size & 0x0f)
                .saturating_add((sample.offset as u32) & 0x0f)
                .saturating_add(sample.checksum & 0x0f)
                .saturating_add(u32::from(sample.preview[0] & 0x0f))
                .saturating_add(samples.codec.rotate_left(7) & 0x0f);
        }
    }
    let Some(track) = quicktime.movie_video_track else {
        return fallback_salt;
    };
    let sample_index = ppc_qt_movie_sample_index(
        quicktime,
        usize::try_from(track.sample_count.max(1)).unwrap_or(usize::MAX),
    )
    .and_then(|index| u32::try_from(index).ok())
    .unwrap_or(u32::MAX);
    0x70u32
        .saturating_add(sample_index)
        .saturating_add(track.first_sample_size & 0x0f)
        .saturating_add((track.first_chunk_offset as u32) & 0x0f)
        .saturating_add(track.first_sample_checksum & 0x0f)
        .saturating_add(u32::from(track.first_sample_preview[0] & 0x0f))
        .saturating_add(track.codec.rotate_left(7) & 0x0f)
}

fn ppc_qt_movie_sample_index(quicktime: &PpcQuickTimeState, sample_count: usize) -> Option<usize> {
    if sample_count == 0 {
        return None;
    }
    let sample_count_u64 = u64::try_from(sample_count).ok()?;
    usize::try_from(
        u64::from(quicktime.movie_task_count)
            .saturating_mul(sample_count_u64)
            .checked_div(u64::from(quicktime.movie_tasks_until_done.max(1)))
            .unwrap_or(0)
            .min(sample_count_u64.saturating_sub(1)),
    )
    .ok()
}

pub(crate) fn ppc_qt_movie_timed_sample_index(
    quicktime: &PpcQuickTimeState,
    samples: &PpcQuickTimeVideoSampleTableRecord,
) -> Option<usize> {
    if samples.samples.is_empty() {
        return None;
    }
    let media_duration = samples.media_duration.max(1);
    let movie_time = u64::from(quicktime.movie_task_count)
        .saturating_mul(media_duration)
        .checked_div(u64::from(quicktime.movie_tasks_until_done.max(1)))
        .unwrap_or(0)
        .min(media_duration.saturating_sub(1));
    samples
        .samples
        .iter()
        .position(|sample| {
            let sample_end = sample
                .media_start_time
                .saturating_add(u64::from(sample.duration));
            movie_time >= sample.media_start_time && movie_time < sample_end
        })
        .or_else(|| samples.samples.len().checked_sub(1))
}

fn ppc_qt_draw_visible_16bpp_frame(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    salt: u32,
) -> bool {
    if front_buffer.depth != 16
        || front_buffer.base_addr == 0
        || front_buffer.width == 0
        || front_buffer.height == 0
        || front_buffer.row_bytes < front_buffer.width.saturating_mul(2)
    {
        return false;
    }

    let x_denom = front_buffer.width.saturating_sub(1).max(1);
    let y_denom = front_buffer.height.saturating_sub(1).max(1);
    let mut wrote_any = false;
    for y in 0..front_buffer.height {
        for x in 0..front_buffer.width {
            let red = ((x.saturating_mul(31)) / x_denom) as u16;
            let green = ((y.saturating_mul(31)) / y_denom) as u16;
            let blue = ((x ^ y ^ salt) & 0x1f) as u16;
            let pixel = (red << 10) | (green << 5) | blue;
            if ppc_q3_write_software_pixel(memory, front_buffer, (x as i32, y as i32), pixel) {
                wrote_any = true;
            }
        }
    }
    wrote_any
}
