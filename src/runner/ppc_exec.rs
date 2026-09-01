//! PowerPC execution state and host synchronization records.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcProfileSample {
    pub(crate) max_steps: usize,
    pub(crate) cycles: u64,
    pub(crate) total_instructions: u64,
    pub(crate) tick: u32,
    pub(crate) pc: u32,
    pub(crate) lr: u32,
    pub(crate) current_gworld: u32,
    pub(crate) screen_events: u64,
    pub(crate) handled_import_count: u32,
    pub(crate) last_import_index: Option<u32>,
    pub(crate) unsupported_import_index: Option<u32>,
    pub(crate) q3_frames: usize,
    pub(crate) q3_frame_start: usize,
    pub(crate) q3_frame_end: usize,
    pub(crate) q3_commands: usize,
    pub(crate) q3_vertices: usize,
    pub(crate) q3_triangles: usize,
    pub(crate) q3_pixels: usize,
    pub(crate) dsp_front_gworld: u32,
    pub(crate) dsp_back_gworld: u32,
    pub(crate) run_us: u128,
    pub(crate) render_us: u128,
    pub(crate) sync_us: u128,
    pub(crate) total_us: u128,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PpcGWorldDumpState {
    pub(crate) current_gworld: u32,
    pub(crate) front_gworld: u32,
    pub(crate) back_gworld: u32,
    pub(crate) swap_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PpcHostSoundPlayback {
    pub(crate) file_playback_index: usize,
    pub(crate) channel: u32,
}

pub(crate) struct PpcDecodedDoubleBuffer {
    pub(crate) buffer_ptr: u32,
    pub(crate) flags: u32,
    pub(crate) samples: Vec<crate::sound::StereoSample>,
}
