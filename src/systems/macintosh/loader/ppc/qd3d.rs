//! QuickDraw 3D types, records, scene representation, and GPU frame generation.

use super::{
    format_ppc_fourcc,
    graphics::{PpcFrontBuffer, PpcGWorldRecord},
    ppc_existing_path_for_fsspec, ppc_front_buffer_for_gworld, ppc_heap_can_alloc_sequence,
    ppc_hle_trace_enabled, ppc_live_quickdraw_surface, ppc_main_screen_height,
    ppc_main_screen_width, ppc_memory_can_read_bytes, ppc_memory_can_write_bytes,
    ppc_process_heap_alloc, ppc_quickdraw_read_pixel, ppc_quickdraw_write_raw_pixel,
    ppc_res_type_text, ppc_rgb555_to_clut_index, qd3d_collision_trace_enabled,
    qd3d_dump_frame_enabled, qd3d_text, qd3d_trace_enabled, qd3d_trimesh_trace_enabled, BLR,
    PpcCpu, PpcImportAction, PpcSectionMem, PpcVfsDirectory, PpcVfsFileRecord,
    ppc_i16_result, PPC_MEM_FULL_ERR, PPC_NO_ERR, PPC_PARAM_ERR,
};
use crate::memory::GuestWritableSpan;
use crate::process_context::ProcessNativeMemoryManager;
use ppc::PpcMemory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub const PPC_Q3_ERROR_NONE: u32 = 0;
pub const PPC_Q3_ILLUMINATION_TYPE_PHONG: u32 = u32::from_be_bytes(*b"phil");
pub const PPC_Q3_SHADER_UV_BOUNDARY_WRAP: u32 = 0;

pub fn ppc_q3_matrix4x4_identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQ3ObjectKind {
    Generic,
    MemoryStorage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PpcQ3ObjectSource {
    pub file: u32,
    pub offset: u32,
    pub parent_group_type: u32,
    pub group_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3ObjectRecord {
    pub object: u32,
    pub kind: PpcQ3ObjectKind,
    pub object_type: u32,
    pub source: PpcQ3ObjectSource,
    pub data_ptr: u32,
    pub data_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3ObjectReferenceRecord {
    pub object: u32,
    pub ref_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3ErrorState {
    pub first_error: u32,
    pub last_error: u32,
    pub clear_on_next_q3_call: bool,
}

impl Default for PpcQ3ErrorState {
    fn default() -> Self {
        Self {
            first_error: PPC_Q3_ERROR_NONE,
            last_error: PPC_Q3_ERROR_NONE,
            clear_on_next_q3_call: false,
        }
    }
}

impl PpcQ3ErrorState {
    pub fn clear(&mut self) {
        self.first_error = PPC_Q3_ERROR_NONE;
        self.last_error = PPC_Q3_ERROR_NONE;
        self.clear_on_next_q3_call = false;
    }

    pub fn post(&mut self, error: u32) {
        if error == PPC_Q3_ERROR_NONE {
            return;
        }
        if self.first_error == PPC_Q3_ERROR_NONE {
            self.first_error = error;
        }
        self.last_error = error;
        self.clear_on_next_q3_call = false;
    }

    pub fn get(&mut self) -> (u32, u32) {
        let errors = (self.first_error, self.last_error);
        if self.last_error != PPC_Q3_ERROR_NONE {
            self.clear_on_next_q3_call = true;
        }
        errors
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PpcQ3LifecycleState {
    pub initialize_count: u32,
    pub exit_count: u32,
    pub initialized_depth: u32,
}

impl PpcQ3LifecycleState {
    pub fn initialized(&self) -> bool {
        self.initialized_depth != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3MemoryStorageRecord {
    pub storage: u32,
    pub buffer_ptr: u32,
    pub valid_size: u32,
    pub buffer_size: u32,
    pub owns_buffer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3FileRecord {
    pub file: u32,
    pub storage: u32,
    pub is_open: bool,
    pub object_type: u32,
    pub read_offset: u32,
    pub read_object: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3GroupMembershipRecord {
    pub group: u32,
    pub object: u32,
    pub before: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3FileGroupRecord {
    pub file: u32,
    pub offset: u32,
    pub group: u32,
    pub group_type: u32,
    pub parent_group: u32,
    pub group_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PpcQ3SubmissionKind {
    Shader,
    Style,
    FogStyle,
    TriMesh,
    MatrixTransform,
    ResetTransform,
    Push,
    Pop,
    Object,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3SubmissionRecord {
    pub view: u32,
    pub kind: PpcQ3SubmissionKind,
    pub primary: u32,
    pub secondary: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3ViewTransformRecord {
    pub view: u32,
    pub stack: Vec<[[f32; 4]; 4]>,
    pub local_to_world: [[f32; 4]; 4],
}

impl PpcQ3ViewTransformRecord {
    pub fn new(view: u32) -> Self {
        Self {
            view,
            stack: Vec::new(),
            local_to_world: ppc_q3_matrix4x4_identity(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SubmissionTransformRecord {
    pub view: u32,
    pub kind: PpcQ3SubmissionKind,
    pub primary: u32,
    pub secondary: u32,
    pub local_to_world: [[f32; 4]; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3ViewMaterialRecord {
    pub view: u32,
    pub shader: u32,
    pub illumination_type: u32,
    pub styles: Vec<PpcQ3StyleRecord>,
    pub fog_style: Option<PpcQ3FogStyleData>,
    pub attributes: Vec<PpcQ3AttributeRecord>,
}

impl PpcQ3ViewMaterialRecord {
    pub fn new(view: u32) -> Self {
        Self {
            view,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SubmissionMaterialRecord {
    pub view: u32,
    pub kind: PpcQ3SubmissionKind,
    pub primary: u32,
    pub secondary: u32,
    pub shader: u32,
    pub illumination_type: u32,
    pub styles: Vec<PpcQ3StyleRecord>,
    pub fog_style: Option<PpcQ3FogStyleData>,
    pub attributes: Vec<PpcQ3AttributeRecord>,
    pub shader_uv_transform: Option<PpcQ3ShaderUvTransformRecord>,
    pub shader_boundary: Option<PpcQ3ShaderBoundaryRecord>,
    pub texture_shader: Option<PpcQ3TextureShaderRecord>,
    pub mipmap_texture: Option<PpcQ3MipmapTextureRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SubmissionLightRecord {
    pub view: u32,
    pub kind: PpcQ3SubmissionKind,
    pub primary: u32,
    pub secondary: u32,
    pub light_group: u32,
    pub lights: Vec<PpcQ3LightRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3ViewStateSnapshotRecord {
    pub view: u32,
    pub transform: Option<PpcQ3ViewTransformRecord>,
    pub material: Option<PpcQ3ViewMaterialRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3CompletedFrameRecord {
    pub view: u32,
    pub submissions: Vec<PpcQ3SubmissionRecord>,
    pub submission_transforms: Vec<PpcQ3SubmissionTransformRecord>,
    pub submission_materials: Vec<PpcQ3SubmissionMaterialRecord>,
    pub submission_lights: Vec<PpcQ3SubmissionLightRecord>,
    pub retained_trimeshes: Vec<PpcQ3TriMeshRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3RetainedFrameRecord {
    pub view: u32,
    pub frame: PpcQ3CompletedFrameRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PpcQ3SceneCommand {
    TriMesh(PpcQ3SceneTriMeshCommand),
    Submission(PpcQ3SceneSubmissionCommand),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SceneTriMeshCommand {
    pub submission_index: usize,
    pub view_state: PpcQ3ViewStateRecord,
    pub camera: Option<PpcQ3CameraRecord>,
    pub geometry: PpcQ3SceneTriMeshGeometry,
    pub local_to_world: [[f32; 4]; 4],
    pub material: PpcQ3SubmissionMaterialRecord,
    pub lights: PpcQ3SubmissionLightRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SceneSubmissionCommand {
    pub submission_index: usize,
    pub view_state: PpcQ3ViewStateRecord,
    pub submission: PpcQ3SubmissionRecord,
    pub local_to_world: [[f32; 4]; 4],
    pub material: PpcQ3SubmissionMaterialRecord,
    pub lights: PpcQ3SubmissionLightRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3SceneTriMeshGeometry {
    pub source: PpcQ3SceneTriMeshSource,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PpcQ3SceneTriMeshSource {
    Object(u32),
    DataPtr(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3SceneReplayMemoryRegion {
    pub base_addr: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3SceneReplay {
    pub commands: Vec<PpcQ3SceneCommand>,
    pub memory_regions: Vec<PpcQ3SceneReplayMemoryRegion>,
}

impl PpcQ3SceneReplay {
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json_str(value: &str) -> serde_json::Result<Self> {
        serde_json::from_str(value)
    }
}

/// A browser-friendly QD3D frame whose guest geometry has already been
/// transformed, clipped, lit, and decoded for direct GPU rasterization.
#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3GpuFrame {
    pub width: u32,
    pub height: u32,
    pub viewport: [i32; 4],
    pub clear_color: Option<[f32; 4]>,
    pub textures: Vec<PpcQ3GpuTexture>,
    pub draws: Vec<PpcQ3GpuDraw>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3GpuTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub wrap_u: bool,
    pub wrap_v: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3GpuDraw {
    pub texture: Option<usize>,
    pub vertices: Vec<PpcQ3GpuVertex>,
    pub blend: bool,
    pub write_depth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3GpuVertex {
    pub screen_x: f32,
    pub screen_y: f32,
    pub depth: f32,
    pub reciprocal_w: f32,
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQ3RenderTargetSource {
    PixmapDrawContext,
    MacDrawContext,
    CurrentGWorld,
}

impl PpcQ3RenderTargetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PixmapDrawContext => "pixmap_draw_context",
            Self::MacDrawContext => "mac_draw_context",
            Self::CurrentGWorld => "current_gworld",
        }
    }

    pub fn from_str(value: &'static str) -> Option<Self> {
        match value {
            "pixmap_draw_context" => Some(Self::PixmapDrawContext),
            "mac_draw_context" => Some(Self::MacDrawContext),
            "current_gworld" => Some(Self::CurrentGWorld),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3ViewportRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl PpcQ3ViewportRect {
    pub fn full(front_buffer: PpcFrontBuffer) -> Self {
        Self {
            left: 0,
            top: 0,
            right: front_buffer.width,
            bottom: front_buffer.height,
        }
    }

    pub fn from_q3_area(
        front_buffer: PpcFrontBuffer,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Option<Self> {
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return None;
        }
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        let width = front_buffer.width as f32;
        let height = front_buffer.height as f32;
        let left = min_x.floor().clamp(0.0, width) as u32;
        let right = max_x.ceil().clamp(0.0, width) as u32;
        let top = min_y.floor().clamp(0.0, height) as u32;
        let bottom = max_y.ceil().clamp(0.0, height) as u32;
        if right <= left || bottom <= top {
            return None;
        }
        Some(Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub fn inclusive_bounds(self) -> Option<(i32, i32, i32, i32)> {
        if self.right <= self.left || self.bottom <= self.top {
            return None;
        }
        Some((
            i32::try_from(self.left).ok()?,
            i32::try_from(self.top).ok()?,
            i32::try_from(self.right.checked_sub(1)?).ok()?,
            i32::try_from(self.bottom.checked_sub(1)?).ok()?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3RenderTarget {
    pub front_buffer: PpcFrontBuffer,
    pub viewport: Option<PpcQ3ViewportRect>,
    pub clear_color: Option<u16>,
    pub source: PpcQ3RenderTargetSource,
    pub draw_context: Option<u32>,
    pub gworld: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3StateOnlyCompletedFrameBatch {
    pub frames: usize,
    pub target: Option<PpcQ3RenderTarget>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PpcQ3SoftwareRenderStats {
    pub frames: usize,
    pub commands: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub pixels: usize,
    pub target_base: Option<u32>,
    pub target_row_bytes: Option<u32>,
    pub target_width: Option<u32>,
    pub target_height: Option<u32>,
    pub target_depth: Option<u32>,
    pub target_source: Option<&'static str>,
    pub target_draw_context: Option<u32>,
    pub target_gworld: Option<u32>,
    pub target_consistent: bool,
}

impl PpcQ3SoftwareRenderStats {
    pub fn add_empty_frames(&mut self, frames: usize) {
        self.frames = self.frames.saturating_add(frames);
    }

    pub fn add_state_only_batches(&mut self, batches: &[PpcQ3StateOnlyCompletedFrameBatch]) {
        for batch in batches {
            if let Some(target) = batch.target {
                self.record_target(target);
            }
            self.add_empty_frames(batch.frames);
        }
    }

    pub fn record_target(&mut self, target_record: PpcQ3RenderTarget) {
        let front_buffer = target_record.front_buffer;
        let target = (
            front_buffer.base_addr,
            front_buffer.row_bytes,
            front_buffer.width,
            front_buffer.height,
            front_buffer.depth,
            Some(target_record.source.as_str()),
            target_record.draw_context,
            target_record.gworld,
        );
        let current = (
            self.target_base,
            self.target_row_bytes,
            self.target_width,
            self.target_height,
            self.target_depth,
            self.target_source,
            self.target_draw_context,
            self.target_gworld,
        );
        if self.target_base.is_none() {
            self.target_base = Some(target.0);
            self.target_row_bytes = Some(target.1);
            self.target_width = Some(target.2);
            self.target_height = Some(target.3);
            self.target_depth = Some(target.4);
            self.target_source = target.5;
            self.target_draw_context = target.6;
            self.target_gworld = target.7;
            self.target_consistent = true;
        } else if current
            != (
                Some(target.0),
                Some(target.1),
                Some(target.2),
                Some(target.3),
                Some(target.4),
                target.5,
                target.6,
                target.7,
            )
        {
            self.target_consistent = false;
        }
    }

    pub fn merge_frame_stats(
        &mut self,
        frame_stats: PpcQ3SoftwareRenderStats,
        saw_missing_target: &mut bool,
    ) {
        self.frames = self.frames.saturating_add(1);
        self.commands = self.commands.saturating_add(frame_stats.commands);
        self.vertices = self.vertices.saturating_add(frame_stats.vertices);
        self.triangles = self.triangles.saturating_add(frame_stats.triangles);
        self.pixels = self.pixels.saturating_add(frame_stats.pixels);
        if let (
            Some(target_base),
            Some(target_row_bytes),
            Some(target_width),
            Some(target_height),
            Some(target_depth),
            Some(target_source),
        ) = (
            frame_stats.target_base,
            frame_stats.target_row_bytes,
            frame_stats.target_width,
            frame_stats.target_height,
            frame_stats.target_depth,
            frame_stats.target_source,
        ) {
            if let Some(source) = PpcQ3RenderTargetSource::from_str(target_source) {
                self.record_target(PpcQ3RenderTarget {
                    front_buffer: PpcFrontBuffer {
                        base_addr: target_base,
                        row_bytes: target_row_bytes,
                        width: target_width,
                        height: target_height,
                        depth: target_depth,
                    },
                    viewport: None,
                    clear_color: None,
                    source,
                    draw_context: frame_stats.target_draw_context,
                    gworld: frame_stats.target_gworld,
                });
                self.target_consistent &= frame_stats.target_consistent;
                if *saw_missing_target {
                    self.target_consistent = false;
                }
            } else {
                *saw_missing_target = true;
                if self.target_base.is_some() {
                    self.target_consistent = false;
                }
            }
        } else {
            *saw_missing_target = true;
            if self.target_base.is_some() {
                self.target_consistent = false;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3FogStyleData {
    pub state: u32,
    pub mode: u32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub density: f32,
    pub color: (f32, f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3FogStyleRecord {
    pub view: u32,
    pub data_ptr: u32,
    pub data: PpcQ3FogStyleData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3AttributeRecord {
    pub attribute_set: u32,
    pub attribute_type: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3ShaderUvTransformRecord {
    pub shader: u32,
    pub matrix: [[f32; 3]; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3ShaderBoundaryRecord {
    pub shader: u32,
    pub u_boundary: u32,
    pub v_boundary: u32,
}

impl PpcQ3ShaderBoundaryRecord {
    pub fn new(shader: u32) -> Self {
        Self {
            shader,
            u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
            v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3MipmapTextureRecord {
    pub texture: u32,
    pub mipmap: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3TextureShaderRecord {
    pub shader: u32,
    pub texture: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3RendererPreferenceRecord {
    pub renderer: u32,
    pub double_buffer_bypass: Option<u32>,
    pub preference_vendor: Option<u32>,
    pub preference_engine: Option<u32>,
    pub rave_context_hints: Option<u32>,
    pub rave_texture_filter: Option<u32>,
}

impl PpcQ3RendererPreferenceRecord {
    pub fn new(renderer: u32) -> Self {
        Self {
            renderer,
            double_buffer_bypass: None,
            preference_vendor: None,
            preference_engine: None,
            rave_context_hints: None,
            rave_texture_filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3DrawContextRecord {
    pub draw_context: u32,
    pub draw_context_type: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3TriMeshRecord {
    pub trimesh: u32,
    pub data: Vec<u8>,
    pub triangle_attribute_sets: Vec<u32>,
    pub get_data_copies: Vec<PpcQ3TriMeshGetDataCopyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3TriMeshGetDataCopyRecord {
    pub data_out_ptr: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PpcQ3StyleKind {
    Backfacing,
    Interpolation,
    Fill,
    Orientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3StyleRecord {
    pub style: u32,
    pub kind: PpcQ3StyleKind,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3CameraPlacement {
    pub camera_location: (f32, f32, f32),
    pub point_of_interest: (f32, f32, f32),
    pub up_vector: (f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PpcQ3CameraProjection {
    ViewAngleAspect {
        fov: f32,
        aspect_ratio_x_to_y: f32,
    },
    Orthographic {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    ViewPlane {
        view_plane: f32,
        half_width_at_view_plane: f32,
        half_height_at_view_plane: f32,
        center_x_on_view_plane: f32,
        center_y_on_view_plane: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PpcQ3CameraCommonData {
    pub(crate) placement: PpcQ3CameraPlacement,
    pub(crate) range_hither: f32,
    pub(crate) range_yon: f32,
    pub(crate) viewport_origin: (f32, f32),
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3CameraRecord {
    pub camera: u32,
    pub camera_type: u32,
    pub placement: PpcQ3CameraPlacement,
    pub range_hither: f32,
    pub range_yon: f32,
    pub viewport_origin: (f32, f32),
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub projection: PpcQ3CameraProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3LightData {
    pub is_on: u32,
    pub brightness: f32,
    pub color: (f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PpcQ3LightKind {
    Ambient,
    Directional {
        casts_shadows: u32,
        direction: (f32, f32, f32),
    },
    Point {
        casts_shadows: u32,
        attenuation: u32,
        location: (f32, f32, f32),
    },
    Spot {
        casts_shadows: u32,
        attenuation: u32,
        location: (f32, f32, f32),
        direction: (f32, f32, f32),
        hot_angle: f32,
        outer_angle: f32,
        fall_off: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpcQ3LightRecord {
    pub light: u32,
    pub light_type: u32,
    pub data: PpcQ3LightData,
    pub kind: PpcQ3LightKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PpcQ3ViewStateRecord {
    pub view: u32,
    pub renderer: u32,
    pub light_group: u32,
    pub draw_context: u32,
    pub camera: u32,
    pub rendering_depth: u32,
    pub bounding_box_depth: u32,
    pub cancelled: bool,
}

impl PpcQ3ViewStateRecord {
    pub fn new(view: u32) -> Self {
        Self {
            view,
            renderer: 0,
            light_group: 0,
            draw_context: 0,
            camera: 0,
            rendering_depth: 0,
            bounding_box_depth: 0,
            cancelled: false,
        }
    }
}

// --- QuickDraw 3D Constants ---

pub const PPC_Q3_OBJECT_BASE: u32 = 0x0400_0000;
pub const PPC_Q3_OBJECT_STRIDE: u32 = 4;
pub const PPC_Q3_MIPMAP_COPY_SIZE: u32 = 536;
pub const PPC_Q3_TYPE_NONE: u32 = 0;
pub const PPC_Q3_TYPE_3DMF: u32 = u32::from_be_bytes(*b"3DMF");
pub const PPC_Q3_TYPE_BEGIN_GROUP: u32 = u32::from_be_bytes(*b"bgng");
pub const PPC_Q3_TYPE_END_GROUP: u32 = u32::from_be_bytes(*b"endg");
pub const PPC_Q3_TYPE_TOC: u32 = u32::from_be_bytes(*b"toc ");
pub const PPC_Q3_TYPE_REFERENCE: u32 = u32::from_be_bytes(*b"rfrn");
pub const PPC_Q3_TYPE_CONTAINER: u32 = u32::from_be_bytes(*b"cntr");
pub const PPC_Q3_TYPE_TRIMESH: u32 = u32::from_be_bytes(*b"tmsh");
pub const PPC_Q3_TYPE_ATTRIBUTE_ARRAY: u32 = u32::from_be_bytes(*b"atar");
pub const PPC_Q3_TYPE_FILE: u32 = u32::from_be_bytes(*b"file");
pub const PPC_Q3_TYPE_VIEW: u32 = u32::from_be_bytes(*b"view");
pub const PPC_Q3_OBJECT_TYPE_SHARED: u32 = u32::from_be_bytes(*b"shrd");
pub const PPC_Q3_SHARED_TYPE_RENDERER: u32 = u32::from_be_bytes(*b"rddr");
pub const PPC_Q3_RENDERER_TYPE_WIREFRAME: u32 = u32::from_be_bytes(*b"wrfr");
pub const PPC_Q3_RENDERER_TYPE_GENERIC: u32 = u32::from_be_bytes(*b"gnrr");
pub const PPC_Q3_RENDERER_TYPE_INTERACTIVE: u32 = u32::from_be_bytes(*b"irnd");
pub const PPC_Q3_RENDERER_TYPE_INTERACTIVE_QUESA: u32 = u32::from_be_bytes(*b"ctwn");
pub const PPC_Q3_RENDERER_TYPE_OPENGL: u32 = u32::from_be_bytes(*b"oglr");
pub const PPC_Q3_RENDERER_TYPE_CARTOON: u32 = u32::from_be_bytes(*b"toon");
pub const PPC_Q3_RENDERER_TYPE_HIDDEN_LINE: u32 = u32::from_be_bytes(*b"hdnl");
pub const PPC_Q3_SHARED_TYPE_SHAPE: u32 = u32::from_be_bytes(*b"shap");
pub const PPC_Q3_SHAPE_TYPE_GEOMETRY: u32 = u32::from_be_bytes(*b"gmtr");
pub const PPC_Q3_SHAPE_TYPE_SHADER: u32 = u32::from_be_bytes(*b"shdr");
pub const PPC_Q3_SHADER_TYPE_SURFACE: u32 = u32::from_be_bytes(*b"sush");
pub const PPC_Q3_SHADER_TYPE_ILLUMINATION: u32 = u32::from_be_bytes(*b"ilsh");
pub const PPC_Q3_SHAPE_TYPE_STYLE: u32 = u32::from_be_bytes(*b"styl");
pub const PPC_Q3_SHAPE_TYPE_TRANSFORM: u32 = u32::from_be_bytes(*b"xfrm");
pub const PPC_Q3_SHAPE_TYPE_LIGHT: u32 = u32::from_be_bytes(*b"lght");
pub const PPC_Q3_SHAPE_TYPE_CAMERA: u32 = u32::from_be_bytes(*b"cmra");
pub const PPC_Q3_SHAPE_TYPE_GROUP: u32 = u32::from_be_bytes(*b"grup");
pub const PPC_Q3_SHARED_TYPE_SET: u32 = u32::from_be_bytes(*b"set ");
pub const PPC_Q3_SHARED_TYPE_DRAW_CONTEXT: u32 = u32::from_be_bytes(*b"dctx");
pub const PPC_Q3_SHARED_TYPE_TEXTURE: u32 = u32::from_be_bytes(*b"txtr");
pub const PPC_Q3_SHARED_TYPE_STORAGE: u32 = u32::from_be_bytes(*b"strg");
pub const PPC_Q3_GROUP_TYPE_DISPLAY: u32 = u32::from_be_bytes(*b"dspg");
pub const PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY: u32 = u32::from_be_bytes(*b"iopx");
pub const PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY: u32 = u32::from_be_bytes(*b"ordg");
pub const PPC_Q3_GROUP_TYPE_LIGHT: u32 = u32::from_be_bytes(*b"lghg");
pub const PPC_Q3_TYPE_ATTRIBUTE_SET: u32 = u32::from_be_bytes(*b"attr");
pub const PPC_Q3_ILLUMINATION_TYPE_LAMBERT: u32 = u32::from_be_bytes(*b"lmil");
pub const PPC_Q3_ILLUMINATION_TYPE_NULL: u32 = u32::from_be_bytes(*b"nuil");
pub const PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT_3DMF: u32 = u32::from_be_bytes(*b"camb");
pub const PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR_3DMF: u32 = u32::from_be_bytes(*b"kdif");
pub const PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR_3DMF: u32 = u32::from_be_bytes(*b"kspc");
pub const PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL_3DMF: u32 = u32::from_be_bytes(*b"cspc");
pub const PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR_3DMF: u32 = u32::from_be_bytes(*b"kxpr");
pub const PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE: u32 = u32::from_be_bytes(*b"txsu");
pub const PPC_Q3_TEXTURE_TYPE_MIPMAP: u32 = u32::from_be_bytes(*b"txmm");
pub const PPC_Q3_PIXEL_TYPE_RGB32: u32 = 0;
pub const PPC_Q3_PIXEL_TYPE_ARGB32: u32 = 1;
pub const PPC_Q3_PIXEL_TYPE_RGB16: u32 = 2;
pub const PPC_Q3_PIXEL_TYPE_ARGB16: u32 = 3;
pub const PPC_Q3_PIXEL_TYPE_RGB16_565: u32 = 4;
pub const PPC_Q3_PIXEL_TYPE_RGB24: u32 = 5;
pub const PPC_Q3_ENDIAN_BIG: u32 = 0;
pub const PPC_Q3_ENDIAN_LITTLE: u32 = 1;
pub const PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER: u32 = 1;
pub const PPC_Q3_VIEW_STATUS_DONE: u32 = 0;
pub const PPC_Q3_VIEW_STATUS_ERROR: u32 = 2;
pub const PPC_Q3_STORAGE_TYPE_MEMORY: u32 = u32::from_be_bytes(*b"mems");
pub const PPC_Q3_STORAGE_TYPE_MACINTOSH: u32 = u32::from_be_bytes(*b"macn");
pub const PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE: u32 = u32::from_be_bytes(*b"hndl");
pub const PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT: u32 = u32::from_be_bytes(*b"vana");
pub const PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC: u32 = u32::from_be_bytes(*b"orth");
pub const PPC_Q3_CAMERA_TYPE_VIEW_PLANE: u32 = u32::from_be_bytes(*b"vwpl");
pub const PPC_Q3_LIGHT_TYPE_AMBIENT: u32 = u32::from_be_bytes(*b"ambn");
pub const PPC_Q3_LIGHT_TYPE_DIRECTIONAL: u32 = u32::from_be_bytes(*b"drct");
pub const PPC_Q3_LIGHT_TYPE_POINT: u32 = u32::from_be_bytes(*b"pntl");
pub const PPC_Q3_LIGHT_TYPE_SPOT: u32 = u32::from_be_bytes(*b"spot");
pub const PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP: u32 = u32::from_be_bytes(*b"dpxp");
pub const PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH: u32 = u32::from_be_bytes(*b"dmac");
pub const PPC_Q3_TRANSFORM_TYPE_MATRIX: u32 = u32::from_be_bytes(*b"mtrx");
pub const PPC_Q3_STYLE_TYPE_BACKFACING: u32 = u32::from_be_bytes(*b"bckf");
pub const PPC_Q3_STYLE_TYPE_INTERPOLATION: u32 = u32::from_be_bytes(*b"intp");
pub const PPC_Q3_STYLE_TYPE_FILL: u32 = u32::from_be_bytes(*b"fist");
pub const PPC_Q3_STYLE_TYPE_ORIENTATION: u32 = u32::from_be_bytes(*b"fdir");
pub const PPC_Q3_TRIMESH_DATA_SIZE: u32 = 80;
pub const PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET: u32 = 0;
pub const PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET: u32 = 4;
pub const PPC_Q3_TRIMESH_TRIANGLES_OFFSET: u32 = 8;
pub const PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET: u32 = 12;
pub const PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET: u32 = 16;
pub const PPC_Q3_TRIMESH_NUM_EDGES_OFFSET: u32 = 20;
pub const PPC_Q3_TRIMESH_EDGES_OFFSET: u32 = 24;
pub const PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET: u32 = 28;
pub const PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET: u32 = 32;
pub const PPC_Q3_TRIMESH_NUM_POINTS_OFFSET: u32 = 36;
pub const PPC_Q3_TRIMESH_POINTS_OFFSET: u32 = 40;
pub const PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET: u32 = 44;
pub const PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET: u32 = 48;
pub const PPC_Q3_TRIMESH_TRIANGLE_DATA_SIZE: u32 = 12;
pub const PPC_Q3_TRIMESH_EDGE_DATA_SIZE: u32 = 8;
pub const PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE: u32 = 12;
pub const PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_TRIANGLE: u32 = 0;
pub const PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_EDGE: u32 = 1;
pub const PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX: u32 = 2;
pub const PPC_Q3_POINT3D_SIZE: u32 = 12;
pub const PPC_Q3_VECTOR2D_SIZE: u32 = 8;
pub const PPC_Q3_VECTOR3D_SIZE: u32 = 12;
pub const PPC_Q3_BOUNDING_BOX_SIZE: u32 = PPC_Q3_POINT3D_SIZE * 2 + 4;
pub const PPC_Q3_BOUNDING_BOX_MAX_OFFSET: u32 = PPC_Q3_POINT3D_SIZE;
pub const PPC_Q3_BOUNDING_BOX_IS_EMPTY_OFFSET: u32 = PPC_Q3_POINT3D_SIZE * 2;
pub const PPC_Q3_BOUNDING_SPHERE_SIZE: u32 = PPC_Q3_POINT3D_SIZE + 8;
pub const PPC_Q3_BOUNDING_SPHERE_RADIUS_OFFSET: u32 = PPC_Q3_POINT3D_SIZE;
pub const PPC_Q3_BOUNDING_SPHERE_IS_EMPTY_OFFSET: u32 = PPC_Q3_POINT3D_SIZE + 4;
pub const PPC_Q3_SOFTWARE_RENDER_MAX_POINTS: u32 = 65_536;
pub const PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES: u32 = 131_072;
pub const PPC_Q3_SOFTWARE_RENDER_MAX_EDGES: u32 = 196_608;
pub const PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES: u32 = 64;
pub const PPC_Q3_SOFTWARE_DEPTH_NEAR: f32 = -1.0;
pub const PPC_Q3_SOFTWARE_DEPTH_FAR: f32 = 1.0;
pub const PPC_Q3_SOFTWARE_CLIP_EPSILON: f32 = 1.0e-5;
pub const PPC_Q3_MIPMAP_IMAGE_OFFSET: u32 = 0;
pub const PPC_Q3_MIPMAP_USE_MIPMAPPING_OFFSET: u32 = 4;
pub const PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET: u32 = 8;
pub const PPC_Q3_MIPMAP_BIT_ORDER_OFFSET: u32 = 12;
pub const PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET: u32 = 16;
pub const PPC_Q3_MIPMAP_RESERVED_OFFSET: u32 = 20;
pub const PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET: u32 = 24;
pub const PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET: u32 = 28;
pub const PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET: u32 = 32;
pub const PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET: u32 = 36;
pub const PPC_Q3_DRAW_CONTEXT_DATA_SIZE: u32 = 68;
pub const PPC_Q3_DRAW_CONTEXT_CLEAR_METHOD_OFFSET: u32 = 0;
pub const PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_RED_OFFSET: u32 = 8;
pub const PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_GREEN_OFFSET: u32 = 12;
pub const PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_BLUE_OFFSET: u32 = 16;
pub const PPC_Q3_CLEAR_METHOD_WITH_COLOR: u32 = 1;
pub const PPC_Q3_DRAW_CONTEXT_PANE_OFFSET: u32 = 20;
pub const PPC_Q3_DRAW_CONTEXT_PANE_SIZE: u32 = 16;
pub const PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET: u32 = 36;
pub const PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE: u32 = 100;
pub const PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET: u32 = PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 4;
pub const PPC_Q3_PIXMAP_DRAW_CONTEXT_HEIGHT_OFFSET: u32 = PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 8;
pub const PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE: u32 = 84;
pub const PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET: u32 = PPC_Q3_DRAW_CONTEXT_DATA_SIZE;
pub const PPC_Q3_CAMERA_DATA_SIZE: u32 = 60;
pub const PPC_Q3_CAMERA_PLACEMENT_SIZE: u32 = 36;
pub const PPC_Q3_CAMERA_RANGE_SIZE: u32 = 8;
pub const PPC_Q3_CAMERA_VIEWPORT_SIZE: u32 = 16;
pub const PPC_Q3_CAMERA_PLACEMENT_OFFSET: u32 = 0;
pub const PPC_Q3_CAMERA_RANGE_OFFSET: u32 = 36;
pub const PPC_Q3_CAMERA_VIEWPORT_OFFSET: u32 = 44;
pub const PPC_Q3_CAMERA_VIEWPORT_WIDTH_OFFSET: u32 = 8;
pub const PPC_Q3_CAMERA_VIEWPORT_HEIGHT_OFFSET: u32 = 12;
pub const PPC_Q3_VIEW_ANGLE_ASPECT_CAMERA_DATA_SIZE: u32 = 68;
pub const PPC_Q3_ORTHOGRAPHIC_CAMERA_DATA_SIZE: u32 = 76;
pub const PPC_Q3_VIEW_PLANE_CAMERA_DATA_SIZE: u32 = 80;
pub const PPC_Q3_LIGHT_DATA_SIZE: u32 = 20;
pub const PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE: u32 = 36;
pub const PPC_Q3_POINT_LIGHT_DATA_SIZE: u32 = 40;
pub const PPC_Q3_SPOT_LIGHT_DATA_SIZE: u32 = 64;
pub const PPC_Q3_ATTENUATION_TYPE_NONE: u32 = 0;
pub const PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE: u32 = 1;
pub const PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE_SQUARED: u32 = 2;
pub const PPC_Q3_FALL_OFF_TYPE_NONE: u32 = 0;
pub const PPC_Q3_FALL_OFF_TYPE_LINEAR: u32 = 1;
pub const PPC_Q3_FALL_OFF_TYPE_EXPONENTIAL: u32 = 2;
pub const PPC_Q3_FALL_OFF_TYPE_COSINE: u32 = 3;
pub const PPC_Q3_FOG_STYLE_DATA_SIZE: u32 = 36;
pub const PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE: u32 = 4096;
pub const PPC_Q3_FOG_MODE_LINEAR: u32 = 0;
pub const PPC_Q3_FOG_MODE_EXPONENTIAL: u32 = 1;
pub const PPC_Q3_FOG_MODE_EXPONENTIAL_SQUARED: u32 = 2;
pub const PPC_Q3_FOG_MODE_ALPHA: u32 = 3;
pub const PPC_Q3_BACKFACING_STYLE_BOTH: u32 = 0;
pub const PPC_Q3_BACKFACING_STYLE_REMOVE: u32 = 1;
pub const PPC_Q3_BACKFACING_STYLE_FLIP: u32 = 2;
pub const PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE: u32 = 0;
pub const PPC_Q3_ORIENTATION_STYLE_CLOCKWISE: u32 = 1;
pub const PPC_Q3_INTERPOLATION_STYLE_NONE: u32 = 0;
pub const PPC_Q3_INTERPOLATION_STYLE_VERTEX: u32 = 1;
pub const PPC_Q3_INTERPOLATION_STYLE_PIXEL: u32 = 2;
pub const PPC_Q3_FILL_STYLE_FILLED: u32 = 0;
pub const PPC_Q3_FILL_STYLE_EDGES: u32 = 1;
pub const PPC_Q3_FILL_STYLE_POINTS: u32 = 2;
pub const PPC_Q3_ATTRIBUTE_TYPE_NONE: u32 = 0;
pub const PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV: u32 = 1;
pub const PPC_Q3_ATTRIBUTE_TYPE_SHADING_UV: u32 = 2;
pub const PPC_Q3_ATTRIBUTE_TYPE_NORMAL: u32 = 3;
pub const PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT: u32 = 4;
pub const PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR: u32 = 5;
pub const PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR: u32 = 6;
pub const PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL: u32 = 7;
pub const PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR: u32 = 8;
pub const PPC_Q3_ATTRIBUTE_TYPE_SURFACE_TANGENT: u32 = 9;
pub const PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE: u32 = 10;
pub const PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER: u32 = 11;
pub const PPC_Q3_SHADER_UV_BOUNDARY_CLAMP: u32 = 1;
pub const PPC_Q3_MATRIX3X3_SIZE: u32 = 36;
pub const PPC_Q3_MATRIX4X4_SIZE: u32 = 64;

// --- QuickDraw 3D FrontBuffer Surface ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3SoftwareFrontBufferSurface {
    pub front_buffer: PpcFrontBuffer,
    pub span: Option<GuestWritableSpan>,
    pub indexed_clut: Option<[[u16; 3]; 256]>,
}

impl PpcQ3SoftwareFrontBufferSurface {
    pub fn new(
        memory: &mut PpcSectionMem,
        front_buffer: PpcFrontBuffer,
        indexed_clut: Option<[[u16; 3]; 256]>,
    ) -> Self {
        let span = ppc_q3_front_buffer_span_len(front_buffer)
            .and_then(|len| memory.writable_span(front_buffer.base_addr, len));
        Self {
            front_buffer,
            span,
            indexed_clut,
        }
    }

    pub fn read_pixel(&self, memory: &mut PpcSectionMem, point: (i32, i32)) -> Option<u16> {
        if self.front_buffer.depth == 8 {
            let index = usize::from(ppc_quickdraw_read_pixel(memory, self.front_buffer, point)?);
            let [red, green, blue] = *self.indexed_clut.as_ref()?.get(index)?;
            return Some(((red >> 11) << 10) | ((green >> 11) << 5) | (blue >> 11));
        }
        if let (Some(span), Some(offset)) = (self.span, self.pixel_relative_offset(point)) {
            return memory.read_u16_be_in_span(span, offset);
        }
        ppc_q3_read_software_pixel(memory, self.front_buffer, point)
    }

    pub fn write_pixel(&self, memory: &mut PpcSectionMem, point: (i32, i32), color: u16) -> bool {
        if self.front_buffer.depth == 8 {
            let Some(clut) = self.indexed_clut.as_ref() else {
                return false;
            };
            return ppc_quickdraw_write_raw_pixel(
                memory,
                self.front_buffer,
                point,
                u16::from(ppc_rgb555_to_clut_index(color, clut)),
            );
        }
        if let (Some(span), Some(offset)) = (self.span, self.pixel_relative_offset(point)) {
            return memory.write_u16_be_in_span(span, offset, color).is_some();
        }
        ppc_q3_write_software_pixel(memory, self.front_buffer, point, color)
    }

    pub fn pixel_relative_offset(&self, point: (i32, i32)) -> Option<usize> {
        ppc_q3_front_buffer_pixel_relative_offset(self.front_buffer, point)
    }
}

// --- QuickDraw 3D Software Rasterizer & Depth Buffer ---

pub fn ppc_q3_submission_transform_matches(
    record: &PpcQ3SubmissionTransformRecord,
    submission: &PpcQ3SubmissionRecord,
) -> bool {
    record.view == submission.view
        && record.kind == submission.kind
        && record.primary == submission.primary
        && record.secondary == submission.secondary
}

pub fn ppc_q3_submission_material_matches(
    record: &PpcQ3SubmissionMaterialRecord,
    submission: &PpcQ3SubmissionRecord,
) -> bool {
    record.view == submission.view
        && record.kind == submission.kind
        && record.primary == submission.primary
        && record.secondary == submission.secondary
}

pub fn ppc_q3_submission_light_matches(
    record: &PpcQ3SubmissionLightRecord,
    submission: &PpcQ3SubmissionRecord,
) -> bool {
    record.view == submission.view
        && record.kind == submission.kind
        && record.primary == submission.primary
        && record.secondary == submission.secondary
}

pub fn ppc_q3_trimesh_header_u32(data: &[u8], offset: u32) -> Option<u32> {
    let offset = usize::try_from(offset).ok()?;
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

pub fn ppc_q3_trace_trimesh_header(action: &str, cpu: &PpcCpu, trimesh: u32, data: &[u8]) {
    if !qd3d_trimesh_trace_enabled() {
        return;
    }
    let field = |offset| ppc_q3_trimesh_header_u32(data, offset).unwrap_or(0);
    eprintln!(
        "[QD3D-TRIMESH] {} lr=${:08X} mesh=${:08X} r4=${:08X} tri={}/${:08X} tri_attr={}/${:08X} edges={}/${:08X} edge_attr={}/${:08X} points={}/${:08X} vertex_attr={}/${:08X}",
        action,
        cpu.lr,
        trimesh,
        cpu.gpr[4],
        field(PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET),
        field(PPC_Q3_TRIMESH_TRIANGLES_OFFSET),
        field(PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET),
        field(PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET),
        field(PPC_Q3_TRIMESH_NUM_EDGES_OFFSET),
        field(PPC_Q3_TRIMESH_EDGES_OFFSET),
        field(PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET),
        field(PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET),
        field(PPC_Q3_TRIMESH_NUM_POINTS_OFFSET),
        field(PPC_Q3_TRIMESH_POINTS_OFFSET),
        field(PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET),
        field(PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET),
    );
}

pub fn ppc_q3_point_finite((x, y, z): (f32, f32, f32)) -> bool {
    x.is_finite() && y.is_finite() && z.is_finite()
}

pub fn ppc_q3_color_finite((red, green, blue): (f32, f32, f32)) -> bool {
    red.is_finite() && green.is_finite() && blue.is_finite()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3SoftwareTexture {
    pub image_base: u32,
    pub valid_size: u32,
    pub image_offset: u32,
    pub width: u32,
    pub height: u32,
    pub row_bytes: u32,
    pub pixel_type: u32,
    pub byte_order: u32,
    pub pixel_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareTextureSample {
    pub color: (f32, f32, f32),
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareMaterialSample {
    pub diffuse: (f32, f32, f32),
    pub ambient_coefficient: f32,
    pub specular_color: (f32, f32, f32),
    pub specular_control: f32,
    pub highlight_state: bool,
    pub transparency: (f32, f32, f32),
    pub illumination_type: u32,
    pub texture: Option<PpcQ3SoftwareTexture>,
    pub uv_transform: Option<[[f32; 3]; 3]>,
    pub shader_boundary: Option<PpcQ3ShaderBoundaryRecord>,
    pub fog_style: Option<PpcQ3FogStyleData>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareMaterialOutput {
    pub color: (f32, f32, f32),
    pub opacity: f32,
}

pub fn ppc_q3_software_material_sample(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    material: &PpcQ3SubmissionMaterialRecord,
) -> PpcQ3SoftwareMaterialSample {
    PpcQ3SoftwareMaterialSample {
        diffuse: ppc_q3_software_material_diffuse_color(material),
        ambient_coefficient: ppc_q3_software_material_ambient_coefficient(material),
        specular_color: ppc_q3_software_material_specular_color(material),
        specular_control: ppc_q3_software_material_specular_control(material),
        highlight_state: ppc_q3_software_material_highlight_state(material),
        transparency: ppc_q3_software_material_transparency_color(material),
        illumination_type: ppc_q3_software_illumination_type(material.illumination_type),
        texture: ppc_q3_software_material_texture(q3_objects, q3_memory_storages, material),
        uv_transform: material.shader_uv_transform.map(|record| record.matrix),
        shader_boundary: material.shader_boundary,
        fog_style: material.fog_style,
    }
}

pub fn ppc_q3_software_uses_ambient_coefficients(
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> bool {
    material.illumination_type != PPC_Q3_ILLUMINATION_TYPE_NULL
        && lights
            .lights
            .iter()
            .any(|light| light.data.is_on != 0 && matches!(light.kind, PpcQ3LightKind::Ambient))
}

pub fn ppc_q3_software_uses_normals(
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> bool {
    material.illumination_type != PPC_Q3_ILLUMINATION_TYPE_NULL
        && lights
            .lights
            .iter()
            .any(|light| light.data.is_on != 0 && !matches!(light.kind, PpcQ3LightKind::Ambient))
}

pub fn ppc_q3_software_uses_specular_attributes(
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> bool {
    material.illumination_type == PPC_Q3_ILLUMINATION_TYPE_PHONG
        && ppc_q3_software_uses_normals(material, lights)
}

pub fn ppc_q3_software_fill_style(material: &PpcQ3SubmissionMaterialRecord) -> u32 {
    material
        .styles
        .iter()
        .find(|record| record.kind == PpcQ3StyleKind::Fill)
        .map(|record| record.value)
        .unwrap_or(PPC_Q3_FILL_STYLE_FILLED)
}

pub fn ppc_q3_software_backfacing_style(material: &PpcQ3SubmissionMaterialRecord) -> u32 {
    material
        .styles
        .iter()
        .find(|record| record.kind == PpcQ3StyleKind::Backfacing)
        .map(|record| record.value)
        .unwrap_or(PPC_Q3_BACKFACING_STYLE_BOTH)
}

pub fn ppc_q3_software_orientation_style(material: &PpcQ3SubmissionMaterialRecord) -> u32 {
    material
        .styles
        .iter()
        .find(|record| record.kind == PpcQ3StyleKind::Orientation)
        .map(|record| record.value)
        .unwrap_or(PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE)
}

pub fn ppc_q3_software_interpolation_style(material: &PpcQ3SubmissionMaterialRecord) -> u32 {
    material
        .styles
        .iter()
        .find(|record| record.kind == PpcQ3StyleKind::Interpolation)
        .map(|record| record.value)
        .unwrap_or(PPC_Q3_INTERPOLATION_STYLE_PIXEL)
}

pub fn ppc_q3_software_material_diffuse_color(
    material: &PpcQ3SubmissionMaterialRecord,
) -> (f32, f32, f32) {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR)
        .and_then(|record| {
            Some((
                ppc_q3_read_f32_from_slice(&record.data, 0)?,
                ppc_q3_read_f32_from_slice(&record.data, 4)?,
                ppc_q3_read_f32_from_slice(&record.data, 8)?,
            ))
        })
        // QuickDraw 3D 1.5.4, View Objects, p. 907: default view attributes.
        .unwrap_or((0.5, 0.5, 0.5))
}

pub fn ppc_q3_software_material_ambient_coefficient(material: &PpcQ3SubmissionMaterialRecord) -> f32 {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT)
        .and_then(|record| ppc_q3_read_f32_from_slice(&record.data, 0))
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0))
        .unwrap_or(1.0)
}

pub fn ppc_q3_software_material_specular_color(
    material: &PpcQ3SubmissionMaterialRecord,
) -> (f32, f32, f32) {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR)
        .and_then(|record| {
            Some((
                ppc_q3_read_f32_from_slice(&record.data, 0)?,
                ppc_q3_read_f32_from_slice(&record.data, 4)?,
                ppc_q3_read_f32_from_slice(&record.data, 8)?,
            ))
        })
        // QuickDraw 3D 1.5.4, View Objects, p. 907: default view attributes.
        .unwrap_or((0.5, 0.5, 0.5))
}

pub fn ppc_q3_software_material_specular_control(material: &PpcQ3SubmissionMaterialRecord) -> f32 {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL)
        .and_then(|record| ppc_q3_read_f32_from_slice(&record.data, 0))
        .filter(|value| value.is_finite() && *value >= 0.0)
        // QuickDraw 3D 1.5.4, View Objects, p. 907: default view attributes.
        .unwrap_or(4.0)
}

pub fn ppc_q3_software_material_highlight_state(material: &PpcQ3SubmissionMaterialRecord) -> bool {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE)
        .and_then(|record| ppc_q3_read_u32_from_slice(&record.data, 0))
        .map(|value| value != 0)
        .unwrap_or(true)
}

pub fn ppc_q3_software_material_transparency_color(
    material: &PpcQ3SubmissionMaterialRecord,
) -> (f32, f32, f32) {
    material
        .attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR)
        .and_then(|record| {
            Some((
                ppc_q3_software_transparency_component(ppc_q3_read_f32_from_slice(
                    &record.data,
                    0,
                )?),
                ppc_q3_software_transparency_component(ppc_q3_read_f32_from_slice(
                    &record.data,
                    4,
                )?),
                ppc_q3_software_transparency_component(ppc_q3_read_f32_from_slice(
                    &record.data,
                    8,
                )?),
            ))
        })
        .unwrap_or((1.0, 1.0, 1.0))
}

pub fn ppc_q3_software_transparency_component(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn ppc_q3_software_transparency_color_alpha(color: (f32, f32, f32)) -> Option<f32> {
    if !color.0.is_finite() || !color.1.is_finite() || !color.2.is_finite() {
        return None;
    }
    Some(
        (ppc_q3_software_transparency_component(color.0)
            + ppc_q3_software_transparency_component(color.1)
            + ppc_q3_software_transparency_component(color.2))
            / 3.0,
    )
}

pub fn ppc_q3_software_material_uses_alpha_fog(fog_style: Option<PpcQ3FogStyleData>) -> bool {
    matches!(
        fog_style,
        Some(PpcQ3FogStyleData {
            state: 1,
            mode: PPC_Q3_FOG_MODE_ALPHA,
            ..
        })
    )
}

#[cfg(test)]
pub fn ppc_q3_software_material_color(
    memory: &mut PpcSectionMem,
    sample: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
    uv: Option<(f32, f32)>,
    diffuse: Option<(f32, f32, f32)>,
    normal: Option<(f32, f32, f32)>,
    specular_color: Option<(f32, f32, f32)>,
    specular_control: Option<f32>,
    fog_depth: f32,
    vertex_alpha: Option<f32>,
) -> (f32, f32, f32) {
    ppc_q3_software_material_shaded_output(
        memory,
        sample,
        lights,
        uv,
        diffuse,
        None,
        normal,
        specular_color,
        specular_control,
        None,
        None,
        Some((0.0, 0.0, 1.0)),
        fog_depth,
        vertex_alpha,
    )
    .color
}

pub fn ppc_q3_software_material_shaded_output(
    memory: &mut PpcSectionMem,
    sample: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
    uv: Option<(f32, f32)>,
    diffuse: Option<(f32, f32, f32)>,
    ambient_coefficient: Option<f32>,
    normal: Option<(f32, f32, f32)>,
    specular_color: Option<(f32, f32, f32)>,
    specular_control: Option<f32>,
    highlight_state: Option<bool>,
    world: Option<(f32, f32, f32)>,
    view_direction: Option<(f32, f32, f32)>,
    fog_depth: f32,
    vertex_alpha: Option<f32>,
) -> PpcQ3SoftwareMaterialOutput {
    let diffuse = diffuse.unwrap_or(sample.diffuse);
    let texture_sample = sample.texture.and_then(|texture| {
        uv.and_then(|uv| ppc_q3_software_transform_uv(uv, sample.uv_transform))
            .and_then(|uv| {
                ppc_q3_software_texture_sample(memory, texture, sample.shader_boundary, uv.0, uv.1)
            })
            .or_else(|| ppc_q3_software_texture_pixel_at(memory, texture, 0, 0))
    });
    let texture_opacity = texture_sample.map(|texture| texture.opacity).unwrap_or(1.0);
    let color = texture_sample
        .map(|texture| {
            (
                diffuse.0 * texture.color.0,
                diffuse.1 * texture.color.1,
                diffuse.2 * texture.color.2,
            )
        })
        .unwrap_or(diffuse);
    let specular_color = if highlight_state.unwrap_or(sample.highlight_state) {
        specular_color.unwrap_or(sample.specular_color)
    } else {
        (0.0, 0.0, 0.0)
    };
    let color = ppc_q3_software_apply_lights(
        color,
        ambient_coefficient.unwrap_or(sample.ambient_coefficient),
        specular_color,
        specular_control.unwrap_or(sample.specular_control),
        sample.illumination_type,
        lights,
        normal,
        world,
        view_direction,
    );
    PpcQ3SoftwareMaterialOutput {
        color: ppc_q3_software_apply_fog(color, sample.fog_style, fog_depth, vertex_alpha),
        opacity: texture_opacity,
    }
}

pub fn ppc_q3_software_apply_texture_and_fog(
    memory: &mut PpcSectionMem,
    sample: &PpcQ3SoftwareMaterialSample,
    uv: Option<(f32, f32)>,
    fog_depth: f32,
    vertex_alpha: Option<f32>,
    output: PpcQ3SoftwareMaterialOutput,
) -> PpcQ3SoftwareMaterialOutput {
    let texture_sample = sample.texture.and_then(|texture| {
        uv.and_then(|uv| ppc_q3_software_transform_uv(uv, sample.uv_transform))
            .and_then(|uv| {
                ppc_q3_software_texture_sample(memory, texture, sample.shader_boundary, uv.0, uv.1)
            })
            .or_else(|| ppc_q3_software_texture_pixel_at(memory, texture, 0, 0))
    });
    let color = texture_sample
        .map(|texture| {
            (
                output.color.0 * texture.color.0,
                output.color.1 * texture.color.1,
                output.color.2 * texture.color.2,
            )
        })
        .unwrap_or(output.color);
    PpcQ3SoftwareMaterialOutput {
        color: ppc_q3_software_apply_fog(color, sample.fog_style, fog_depth, vertex_alpha),
        opacity: output.opacity * texture_sample.map(|texture| texture.opacity).unwrap_or(1.0),
    }
}

pub fn ppc_q3_software_transform_uv(
    (u, v): (f32, f32),
    uv_transform: Option<[[f32; 3]; 3]>,
) -> Option<(f32, f32)> {
    if !u.is_finite() || !v.is_finite() {
        return None;
    }
    let Some(matrix) = uv_transform else {
        return Some((u, v));
    };
    let tu = u.mul_add(matrix[0][0], v.mul_add(matrix[1][0], matrix[2][0]));
    let tv = u.mul_add(matrix[0][1], v.mul_add(matrix[1][1], matrix[2][1]));
    let tw = u.mul_add(matrix[0][2], v.mul_add(matrix[1][2], matrix[2][2]));
    if !tu.is_finite() || !tv.is_finite() || !tw.is_finite() || tw == 0.0 {
        return None;
    }
    if tw != 1.0 {
        Some((tu / tw, tv / tw))
    } else {
        Some((tu, tv))
    }
}

pub fn ppc_q3_software_geometric_normal(
    points: [(f32, f32, f32); 3],
    orientation: u32,
) -> Option<(f32, f32, f32)> {
    let normal = ppc_q3_vector3d_cross_values(
        ppc_q3_vector3d_sub(points[1], points[0]),
        ppc_q3_vector3d_sub(points[2], points[0]),
    );
    let normal = if orientation == PPC_Q3_ORIENTATION_STYLE_CLOCKWISE {
        (-normal.0, -normal.1, -normal.2)
    } else {
        normal
    };
    ppc_q3_vector3d_normalized_value(normal)
}

pub fn ppc_q3_software_transform_normal(
    normal: (f32, f32, f32),
    local_to_world: [[f32; 4]; 4],
) -> Option<(f32, f32, f32)> {
    let normal_matrix = ppc_q3_matrix4x4_invert_values(local_to_world)
        .map(ppc_q3_matrix4x4_transpose_values)
        .unwrap_or(local_to_world);
    ppc_q3_vector3d_normalized_value(ppc_q3_vector3d_transform_values(normal, normal_matrix))
}

pub fn ppc_q3_software_material_texture(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    material: &PpcQ3SubmissionMaterialRecord,
) -> Option<PpcQ3SoftwareTexture> {
    let mipmap = material.mipmap_texture.as_ref()?;
    let storage = ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_IMAGE_OFFSET)?;
    let pixel_type = ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET)?;
    let byte_order = ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET)?;
    let width = ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET)?;
    let height = ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET)?;
    let row_bytes =
        ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET)?;
    let image_offset =
        ppc_q3_read_u32_from_slice(&mipmap.mipmap, PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET)?;
    if storage == 0 || width == 0 || height == 0 || row_bytes == 0 {
        return None;
    }
    let pixel_bytes = ppc_q3_software_texture_pixel_bytes(pixel_type)?;
    let min_row_bytes = width.checked_mul(pixel_bytes)?;
    if row_bytes < min_row_bytes {
        return None;
    }
    let (image_base, valid_size) =
        ppc_q3_software_storage_buffer(q3_objects, q3_memory_storages, storage)?;
    let last_row_offset = height.checked_sub(1)?.checked_mul(row_bytes)?;
    let last_pixel_offset = width.checked_sub(1)?.checked_mul(pixel_bytes)?;
    let image_end = image_offset
        .checked_add(last_row_offset)?
        .checked_add(last_pixel_offset)?
        .checked_add(pixel_bytes)?;
    if image_end > valid_size {
        return None;
    }
    Some(PpcQ3SoftwareTexture {
        image_base,
        valid_size,
        image_offset,
        width,
        height,
        row_bytes,
        pixel_type,
        byte_order,
        pixel_bytes,
    })
}

pub fn ppc_q3_software_texture_sample(
    memory: &mut PpcSectionMem,
    texture: PpcQ3SoftwareTexture,
    shader_boundary: Option<PpcQ3ShaderBoundaryRecord>,
    u: f32,
    v: f32,
) -> Option<PpcQ3SoftwareTextureSample> {
    let u_boundary = shader_boundary
        .map(|record| record.u_boundary)
        .unwrap_or(PPC_Q3_SHADER_UV_BOUNDARY_WRAP);
    let v_boundary = shader_boundary
        .map(|record| record.v_boundary)
        .unwrap_or(PPC_Q3_SHADER_UV_BOUNDARY_WRAP);
    let u = ppc_q3_software_texture_coordinate(u, u_boundary)?;
    let v = ppc_q3_software_texture_coordinate(v, v_boundary)?;
    let x = ((u * texture.width as f32).floor() as u32).min(texture.width.saturating_sub(1));
    let y = ((v * texture.height as f32).floor() as u32).min(texture.height.saturating_sub(1));
    ppc_q3_software_texture_pixel_at(memory, texture, x, y)
}

pub fn ppc_q3_software_texture_coordinate(value: f32, boundary: u32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    match boundary {
        PPC_Q3_SHADER_UV_BOUNDARY_CLAMP => Some(value.clamp(0.0, 1.0)),
        PPC_Q3_SHADER_UV_BOUNDARY_WRAP => Some(value - value.floor()),
        _ => None,
    }
}

pub fn ppc_q3_software_texture_pixel_at(
    memory: &mut PpcSectionMem,
    texture: PpcQ3SoftwareTexture,
    x: u32,
    y: u32,
) -> Option<PpcQ3SoftwareTextureSample> {
    if x >= texture.width || y >= texture.height {
        return None;
    }
    let row_offset = y.checked_mul(texture.row_bytes)?;
    let pixel_offset = x.checked_mul(texture.pixel_bytes)?;
    let storage_offset = texture
        .image_offset
        .checked_add(row_offset)?
        .checked_add(pixel_offset)?;
    let pixel_end = storage_offset.checked_add(texture.pixel_bytes)?;
    if pixel_end > texture.valid_size {
        return None;
    }
    let pixel_ptr = texture.image_base.checked_add(storage_offset)?;
    let mut pixel = [0u8; 4];
    let pixel_bytes = usize::try_from(texture.pixel_bytes).ok()?;
    memory.read_bytes_into(pixel_ptr, pixel.get_mut(..pixel_bytes)?)?;
    ppc_q3_software_texture_pixel_color(texture.pixel_type, texture.byte_order, &pixel)
}

pub fn ppc_q3_gpu_texture_from_software(
    memory: &mut PpcSectionMem,
    texture: PpcQ3SoftwareTexture,
    shader_boundary: Option<PpcQ3ShaderBoundaryRecord>,
) -> Option<PpcQ3GpuTexture> {
    let pixel_count = usize::try_from(texture.width.checked_mul(texture.height)?).ok()?;
    let mut rgba = Vec::with_capacity(pixel_count.checked_mul(4)?);
    for y in 0..texture.height {
        for x in 0..texture.width {
            let sample = ppc_q3_software_texture_pixel_at(memory, texture, x, y)?;
            rgba.extend([
                (sample.color.0.clamp(0.0, 1.0) * 255.0).round() as u8,
                (sample.color.1.clamp(0.0, 1.0) * 255.0).round() as u8,
                (sample.color.2.clamp(0.0, 1.0) * 255.0).round() as u8,
                (sample.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    let u_boundary = shader_boundary
        .map(|record| record.u_boundary)
        .unwrap_or(PPC_Q3_SHADER_UV_BOUNDARY_WRAP);
    let v_boundary = shader_boundary
        .map(|record| record.v_boundary)
        .unwrap_or(PPC_Q3_SHADER_UV_BOUNDARY_WRAP);
    if !matches!(
        u_boundary,
        PPC_Q3_SHADER_UV_BOUNDARY_WRAP | PPC_Q3_SHADER_UV_BOUNDARY_CLAMP
    ) || !matches!(
        v_boundary,
        PPC_Q3_SHADER_UV_BOUNDARY_WRAP | PPC_Q3_SHADER_UV_BOUNDARY_CLAMP
    ) {
        return None;
    }
    if (u_boundary == PPC_Q3_SHADER_UV_BOUNDARY_WRAP && !texture.width.is_power_of_two())
        || (v_boundary == PPC_Q3_SHADER_UV_BOUNDARY_WRAP && !texture.height.is_power_of_two())
    {
        // WebGL 1 only permits REPEAT on power-of-two texture dimensions.
        return None;
    }
    Some(PpcQ3GpuTexture {
        width: texture.width,
        height: texture.height,
        rgba,
        wrap_u: u_boundary == PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
        wrap_v: v_boundary == PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
    })
}

pub fn ppc_q3_read_u32_from_slice(data: &[u8], offset: u32) -> Option<u32> {
    let offset = usize::try_from(offset).ok()?;
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

pub fn ppc_q3_software_storage_buffer(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    storage: u32,
) -> Option<(u32, u32)> {
    q3_memory_storages
        .iter()
        .find(|record| record.storage == storage)
        .map(|record| (record.buffer_ptr, record.valid_size))
        .or_else(|| {
            q3_objects
                .iter()
                .find(|record| record.object == storage)
                .map(|record| (record.data_ptr, record.data_size))
        })
        .filter(|(buffer_ptr, valid_size)| *buffer_ptr != 0 && *valid_size != 0)
}

pub fn ppc_q3_software_texture_pixel_bytes(pixel_type: u32) -> Option<u32> {
    match pixel_type {
        PPC_Q3_PIXEL_TYPE_RGB32 | PPC_Q3_PIXEL_TYPE_ARGB32 => Some(4),
        PPC_Q3_PIXEL_TYPE_RGB16 | PPC_Q3_PIXEL_TYPE_ARGB16 | PPC_Q3_PIXEL_TYPE_RGB16_565 => Some(2),
        PPC_Q3_PIXEL_TYPE_RGB24 => Some(3),
        _ => None,
    }
}

pub fn ppc_q3_software_texture_pixel_color(
    pixel_type: u32,
    byte_order: u32,
    pixel: &[u8; 4],
) -> Option<PpcQ3SoftwareTextureSample> {
    match pixel_type {
        PPC_Q3_PIXEL_TYPE_RGB24 => {
            let (red, green, blue) = if byte_order == PPC_Q3_ENDIAN_LITTLE {
                (pixel[2], pixel[1], pixel[0])
            } else {
                (pixel[0], pixel[1], pixel[2])
            };
            Some(PpcQ3SoftwareTextureSample {
                color: ppc_q3_color_rgb_from_u8(red, green, blue),
                opacity: 1.0,
            })
        }
        PPC_Q3_PIXEL_TYPE_RGB32 => {
            let (red, green, blue) = if byte_order == PPC_Q3_ENDIAN_LITTLE {
                (pixel[2], pixel[1], pixel[0])
            } else {
                (pixel[1], pixel[2], pixel[3])
            };
            Some(PpcQ3SoftwareTextureSample {
                color: ppc_q3_color_rgb_from_u8(red, green, blue),
                opacity: 1.0,
            })
        }
        PPC_Q3_PIXEL_TYPE_ARGB32 => {
            let (alpha, red, green, blue) = if byte_order == PPC_Q3_ENDIAN_LITTLE {
                (pixel[3], pixel[2], pixel[1], pixel[0])
            } else {
                (pixel[0], pixel[1], pixel[2], pixel[3])
            };
            Some(PpcQ3SoftwareTextureSample {
                color: ppc_q3_color_rgb_from_u8(red, green, blue),
                opacity: f32::from(alpha) / 255.0,
            })
        }
        PPC_Q3_PIXEL_TYPE_RGB16 => {
            let value = ppc_q3_software_texture_u16(byte_order, pixel)?;
            Some(PpcQ3SoftwareTextureSample {
                color: ppc_q3_rgb555_to_color(value),
                opacity: 1.0,
            })
        }
        PPC_Q3_PIXEL_TYPE_ARGB16 => {
            let value = ppc_q3_software_texture_u16(byte_order, pixel)?;
            Some(PpcQ3SoftwareTextureSample {
                color: ppc_q3_rgb555_to_color(value & 0x7fff),
                opacity: if (value & 0x8000) != 0 { 1.0 } else { 0.0 },
            })
        }
        PPC_Q3_PIXEL_TYPE_RGB16_565 => {
            let value = ppc_q3_software_texture_u16(byte_order, pixel)?;
            Some(PpcQ3SoftwareTextureSample {
                color: (
                    f32::from((value >> 11) & 0x1f) / 31.0,
                    f32::from((value >> 5) & 0x3f) / 63.0,
                    f32::from(value & 0x1f) / 31.0,
                ),
                opacity: 1.0,
            })
        }
        _ => None,
    }
}

pub fn ppc_q3_color_rgb_from_u8(red: u8, green: u8, blue: u8) -> (f32, f32, f32) {
    (
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
    )
}

pub fn ppc_q3_software_texture_u16(byte_order: u32, pixel: &[u8; 4]) -> Option<u16> {
    match byte_order {
        PPC_Q3_ENDIAN_BIG => Some(u16::from_be_bytes([pixel[0], pixel[1]])),
        PPC_Q3_ENDIAN_LITTLE => Some(u16::from_le_bytes([pixel[0], pixel[1]])),
        _ => None,
    }
}

pub fn ppc_q3_software_illumination_type(illumination_type: u32) -> u32 {
    if ppc_q3_object_type_is_illumination_shader(illumination_type) {
        illumination_type
    } else {
        PPC_Q3_ILLUMINATION_TYPE_PHONG
    }
}

pub fn ppc_q3_software_apply_lights(
    diffuse: (f32, f32, f32),
    ambient_coefficient: f32,
    specular_color: (f32, f32, f32),
    specular_control: f32,
    illumination_type: u32,
    lights: &PpcQ3SubmissionLightRecord,
    normal: Option<(f32, f32, f32)>,
    world: Option<(f32, f32, f32)>,
    view_direction: Option<(f32, f32, f32)>,
) -> (f32, f32, f32) {
    if illumination_type == PPC_Q3_ILLUMINATION_TYPE_NULL {
        return diffuse;
    }
    if lights.lights.is_empty() {
        return diffuse;
    }

    let use_specular = illumination_type == PPC_Q3_ILLUMINATION_TYPE_PHONG;
    let mut factor = (0.0, 0.0, 0.0);
    let mut specular = (0.0, 0.0, 0.0);
    for light in &lights.lights {
        if light.data.is_on == 0 {
            continue;
        }
        let contribution = match light.kind {
            PpcQ3LightKind::Ambient => {
                ppc_q3_software_light_contribution(light, ambient_coefficient)
            }
            PpcQ3LightKind::Directional { direction, .. } => {
                let intensity = normal
                    .and_then(|normal| {
                        ppc_q3_vector3d_normalized_value(direction)
                            .map(|direction| (normal, direction))
                    })
                    .map(|(normal, direction)| {
                        let diffuse_intensity = (-ppc_q3_vector3d_dot(normal, direction)).max(0.0);
                        if use_specular {
                            if let Some(specular_contribution) =
                                ppc_q3_software_specular_contribution(
                                    light,
                                    normal,
                                    direction,
                                    view_direction,
                                    diffuse_intensity,
                                    specular_color,
                                    specular_control,
                                )
                            {
                                specular.0 += specular_contribution.0;
                                specular.1 += specular_contribution.1;
                                specular.2 += specular_contribution.2;
                            }
                        }
                        diffuse_intensity
                    })
                    .unwrap_or(1.0);
                ppc_q3_software_light_contribution(light, intensity)
            }
            PpcQ3LightKind::Point {
                attenuation,
                location,
                ..
            } => {
                let Some((light_direction, attenuation_factor)) = world.and_then(|world| {
                    ppc_q3_software_point_light_direction_and_attenuation(
                        world,
                        location,
                        attenuation,
                    )
                }) else {
                    continue;
                };
                let diffuse_intensity =
                    ppc_q3_software_surface_light_intensity(normal, light_direction);
                if use_specular {
                    ppc_q3_software_accumulate_specular_contribution(
                        &mut specular,
                        light,
                        normal,
                        light_direction,
                        view_direction,
                        diffuse_intensity,
                        specular_color,
                        specular_control,
                        attenuation_factor,
                    );
                }
                ppc_q3_software_light_contribution(light, diffuse_intensity * attenuation_factor)
            }
            PpcQ3LightKind::Spot {
                attenuation,
                location,
                direction,
                hot_angle,
                outer_angle,
                fall_off,
                ..
            } => {
                let Some((light_direction, attenuation_factor)) = world.and_then(|world| {
                    ppc_q3_software_point_light_direction_and_attenuation(
                        world,
                        location,
                        attenuation,
                    )
                }) else {
                    continue;
                };
                let Some(spot_factor) = ppc_q3_software_spot_factor(
                    light_direction,
                    direction,
                    hot_angle,
                    outer_angle,
                    fall_off,
                ) else {
                    continue;
                };
                if spot_factor <= 0.0 {
                    continue;
                }
                let diffuse_intensity =
                    ppc_q3_software_surface_light_intensity(normal, light_direction);
                let light_factor = attenuation_factor * spot_factor;
                if use_specular {
                    ppc_q3_software_accumulate_specular_contribution(
                        &mut specular,
                        light,
                        normal,
                        light_direction,
                        view_direction,
                        diffuse_intensity,
                        specular_color,
                        specular_control,
                        light_factor,
                    );
                }
                ppc_q3_software_light_contribution(light, diffuse_intensity * light_factor)
            }
        };
        factor.0 += contribution.0;
        factor.1 += contribution.1;
        factor.2 += contribution.2;
    }

    (
        diffuse.0.mul_add(factor.0, specular.0),
        diffuse.1.mul_add(factor.1, specular.1),
        diffuse.2.mul_add(factor.2, specular.2),
    )
}

pub fn ppc_q3_software_surface_light_intensity(
    normal: Option<(f32, f32, f32)>,
    light_direction: (f32, f32, f32),
) -> f32 {
    normal
        .map(|normal| (-ppc_q3_vector3d_dot(normal, light_direction)).max(0.0))
        .unwrap_or(1.0)
}

pub fn ppc_q3_software_point_light_direction_and_attenuation(
    world: (f32, f32, f32),
    location: (f32, f32, f32),
    attenuation: u32,
) -> Option<((f32, f32, f32), f32)> {
    let offset = ppc_q3_vector3d_sub(world, location);
    let distance_squared = ppc_q3_vector3d_dot(offset, offset);
    if !distance_squared.is_finite() || distance_squared <= 0.0 {
        return None;
    }
    let distance = distance_squared.sqrt();
    let light_direction = (
        offset.0 / distance,
        offset.1 / distance,
        offset.2 / distance,
    );
    let attenuation_factor = ppc_q3_software_attenuation_factor(distance, attenuation)?;
    Some((light_direction, attenuation_factor))
}

pub fn ppc_q3_software_attenuation_factor(distance: f32, attenuation: u32) -> Option<f32> {
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    let factor = match attenuation {
        PPC_Q3_ATTENUATION_TYPE_NONE => 1.0,
        PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE => 1.0 / distance,
        PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE_SQUARED => 1.0 / (distance * distance),
        _ => return None,
    };
    if factor.is_finite() && factor >= 0.0 {
        Some(factor)
    } else {
        None
    }
}

pub fn ppc_q3_software_spot_factor(
    light_direction: (f32, f32, f32),
    direction: (f32, f32, f32),
    hot_angle: f32,
    outer_angle: f32,
    fall_off: u32,
) -> Option<f32> {
    if !hot_angle.is_finite()
        || !outer_angle.is_finite()
        || hot_angle < 0.0
        || outer_angle < hot_angle
    {
        return None;
    }
    let spot_direction = ppc_q3_vector3d_normalized_value(direction)?;
    let angle = ppc_q3_vector3d_dot(light_direction, spot_direction)
        .clamp(-1.0, 1.0)
        .acos();
    if !angle.is_finite() {
        return None;
    }
    if angle > outer_angle {
        return Some(0.0);
    }
    if angle <= hot_angle {
        return Some(1.0);
    }
    let span = outer_angle - hot_angle;
    if span <= 0.0 {
        return Some(0.0);
    }
    let t = ((outer_angle - angle) / span).clamp(0.0, 1.0);
    let factor = match fall_off {
        PPC_Q3_FALL_OFF_TYPE_NONE => 1.0,
        PPC_Q3_FALL_OFF_TYPE_LINEAR => t,
        PPC_Q3_FALL_OFF_TYPE_EXPONENTIAL => t * t,
        PPC_Q3_FALL_OFF_TYPE_COSINE => 0.5 - 0.5 * (std::f32::consts::PI * t).cos(),
        _ => return None,
    };
    if factor.is_finite() {
        Some(factor)
    } else {
        None
    }
}

pub fn ppc_q3_software_accumulate_specular_contribution(
    specular: &mut (f32, f32, f32),
    light: &PpcQ3LightRecord,
    normal: Option<(f32, f32, f32)>,
    light_direction: (f32, f32, f32),
    view_direction: Option<(f32, f32, f32)>,
    diffuse_intensity: f32,
    specular_color: (f32, f32, f32),
    specular_control: f32,
    scale: f32,
) {
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let Some(normal) = normal else {
        return;
    };
    let Some(specular_contribution) = ppc_q3_software_specular_contribution(
        light,
        normal,
        light_direction,
        view_direction,
        diffuse_intensity,
        specular_color,
        specular_control,
    ) else {
        return;
    };
    specular.0 += specular_contribution.0 * scale;
    specular.1 += specular_contribution.1 * scale;
    specular.2 += specular_contribution.2 * scale;
}

pub fn ppc_q3_software_specular_contribution(
    light: &PpcQ3LightRecord,
    normal: (f32, f32, f32),
    light_direction: (f32, f32, f32),
    view_direction: Option<(f32, f32, f32)>,
    diffuse_intensity: f32,
    specular_color: (f32, f32, f32),
    specular_control: f32,
) -> Option<(f32, f32, f32)> {
    // QuickDraw 3D 1.5.4, 3D Metafile Reference, p. 1396 permits exponent zero.
    if diffuse_intensity <= 0.0 || specular_control < 0.0 {
        return None;
    }
    let view_direction = view_direction.and_then(ppc_q3_vector3d_normalized_value)?;
    let surface_to_light = (-light_direction.0, -light_direction.1, -light_direction.2);
    let reflection = ppc_q3_vector3d_normalized_value((
        (2.0 * diffuse_intensity).mul_add(normal.0, -surface_to_light.0),
        (2.0 * diffuse_intensity).mul_add(normal.1, -surface_to_light.1),
        (2.0 * diffuse_intensity).mul_add(normal.2, -surface_to_light.2),
    ))?;
    let specular_intensity = ppc_q3_vector3d_dot(reflection, view_direction)
        .max(0.0)
        .powf(specular_control);
    if !specular_intensity.is_finite() || specular_intensity <= 0.0 {
        return None;
    }
    Some((
        light.data.color.0 * light.data.brightness * specular_color.0 * specular_intensity,
        light.data.color.1 * light.data.brightness * specular_color.1 * specular_intensity,
        light.data.color.2 * light.data.brightness * specular_color.2 * specular_intensity,
    ))
}

pub fn ppc_q3_software_light_contribution(light: &PpcQ3LightRecord, intensity: f32) -> (f32, f32, f32) {
    (
        light.data.color.0 * light.data.brightness * intensity,
        light.data.color.1 * light.data.brightness * intensity,
        light.data.color.2 * light.data.brightness * intensity,
    )
}

pub fn ppc_q3_software_apply_fog(
    color: (f32, f32, f32),
    fog_style: Option<PpcQ3FogStyleData>,
    fog_depth: f32,
    vertex_alpha: Option<f32>,
) -> (f32, f32, f32) {
    let Some(fog_style) = fog_style else {
        return color;
    };
    if fog_style.state == 0 || !fog_depth.is_finite() {
        return color;
    }
    let Some(factor) = ppc_q3_software_fog_color_factor(fog_style, fog_depth, vertex_alpha) else {
        return color;
    };
    let factor = factor.clamp(0.0, 1.0);
    (
        color.0.mul_add(factor, fog_style.color.0 * (1.0 - factor)),
        color.1.mul_add(factor, fog_style.color.1 * (1.0 - factor)),
        color.2.mul_add(factor, fog_style.color.2 * (1.0 - factor)),
    )
}

pub fn ppc_q3_software_fog_color_factor(
    fog_style: PpcQ3FogStyleData,
    fog_depth: f32,
    vertex_alpha: Option<f32>,
) -> Option<f32> {
    let factor = match fog_style.mode {
        PPC_Q3_FOG_MODE_LINEAR => {
            let range = fog_style.fog_end - fog_style.fog_start;
            if range == 0.0 {
                if fog_depth < fog_style.fog_end {
                    1.0
                } else {
                    0.0
                }
            } else {
                (fog_style.fog_end - fog_depth) / range
            }
        }
        PPC_Q3_FOG_MODE_EXPONENTIAL => (-fog_style.density * fog_depth).exp(),
        PPC_Q3_FOG_MODE_EXPONENTIAL_SQUARED => {
            let value = fog_style.density * fog_depth;
            (-(value * value)).exp()
        }
        PPC_Q3_FOG_MODE_ALPHA => vertex_alpha?,
        _ => return None,
    };
    if factor.is_finite() {
        Some(factor)
    } else {
        None
    }
}

pub fn ppc_q3_read_f32_from_slice(data: &[u8], offset: usize) -> Option<f32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(f32::from_bits(u32::from_be_bytes(bytes.try_into().ok()?)))
}

pub fn ppc_q3_rgb555((red, green, blue): (f32, f32, f32)) -> u16 {
    pub fn component(value: f32) -> u16 {
        if !value.is_finite() {
            return 0;
        }
        (value.clamp(0.0, 1.0) * 31.0).round() as u16
    }
    (component(red) << 10) | (component(green) << 5) | component(blue)
}

pub fn ppc_q3_rgb555_to_color(pixel: u16) -> (f32, f32, f32) {
    (
        f32::from((pixel >> 10) & 0x1f) / 31.0,
        f32::from((pixel >> 5) & 0x1f) / 31.0,
        f32::from(pixel & 0x1f) / 31.0,
    )
}

pub fn ppc_q3_blend_transparency_color(
    source: (f32, f32, f32),
    destination: (f32, f32, f32),
    transparency: (f32, f32, f32),
) -> (f32, f32, f32) {
    (
        source
            .0
            .mul_add(transparency.0, destination.0 * (1.0 - transparency.0)),
        source
            .1
            .mul_add(transparency.1, destination.1 * (1.0 - transparency.1)),
        source
            .2
            .mul_add(transparency.2, destination.2 * (1.0 - transparency.2)),
    )
}

pub fn ppc_q3_software_effective_transparency(
    transparency: (f32, f32, f32),
    opacity: f32,
) -> (f32, f32, f32) {
    if !opacity.is_finite() {
        return (0.0, 0.0, 0.0);
    }
    let opacity = opacity.clamp(0.0, 1.0);
    (
        transparency.0 * opacity,
        transparency.1 * opacity,
        transparency.2 * opacity,
    )
}

pub fn ppc_q3_software_blend_opacity(
    material: &PpcQ3SoftwareMaterialSample,
    texture_opacity: f32,
    vertex_alpha: Option<f32>,
) -> f32 {
    if ppc_q3_software_material_uses_alpha_fog(material.fog_style) {
        return texture_opacity;
    }
    match vertex_alpha {
        Some(vertex_alpha) => texture_opacity * vertex_alpha,
        None => texture_opacity,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3SoftwareTriangle {
    pub index: usize,
    pub points: [usize; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3SoftwareEdge {
    pub index: usize,
    pub points: [usize; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareEdgeMaterial {
    pub diffuse: Option<(f32, f32, f32)>,
    pub ambient_coefficient: Option<f32>,
    pub specular_color: Option<(f32, f32, f32)>,
    pub specular_control: Option<f32>,
    pub highlight_state: Option<bool>,
    pub normal: Option<(f32, f32, f32)>,
    pub vertex_alpha: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareProjectedVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub clip: Option<(f32, f32, f32, f32)>,
    pub world: (f32, f32, f32),
    pub view_direction: Option<(f32, f32, f32)>,
    pub fog_depth: f32,
    pub uv: Option<(f32, f32)>,
    pub diffuse: Option<(f32, f32, f32)>,
    pub ambient_coefficient: Option<f32>,
    pub normal: Option<(f32, f32, f32)>,
    pub specular_color: Option<(f32, f32, f32)>,
    pub specular_control: Option<f32>,
    pub highlight_state: Option<bool>,
    pub vertex_alpha: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PpcQ3SoftwareProjectedPoint {
    pub x: i32,
    pub y: i32,
    pub z: f32,
    pub reciprocal_w: f32,
    pub world: (f32, f32, f32),
    pub view_direction: Option<(f32, f32, f32)>,
    pub fog_depth: f32,
    pub uv: Option<(f32, f32)>,
    pub diffuse: Option<(f32, f32, f32)>,
    pub ambient_coefficient: Option<f32>,
    pub normal: Option<(f32, f32, f32)>,
    pub specular_color: Option<(f32, f32, f32)>,
    pub specular_control: Option<f32>,
    pub highlight_state: Option<bool>,
    pub vertex_alpha: Option<f32>,
}

pub enum PpcQ3SoftwareDeferredPrimitive {
    Triangle {
        vertices: [PpcQ3SoftwareProjectedPoint; 3],
        material: PpcQ3SoftwareMaterialSample,
        lights: PpcQ3SubmissionLightRecord,
        interpolation_style: u32,
        sort_depth: f32,
    },
    Point {
        point: PpcQ3SoftwareProjectedPoint,
        material: PpcQ3SoftwareMaterialSample,
        lights: PpcQ3SubmissionLightRecord,
        sort_depth: f32,
    },
    Edge {
        start: PpcQ3SoftwareProjectedPoint,
        end: PpcQ3SoftwareProjectedPoint,
        material: PpcQ3SoftwareMaterialSample,
        lights: PpcQ3SubmissionLightRecord,
        sort_depth: f32,
    },
}

impl PpcQ3SoftwareDeferredPrimitive {
    pub fn sort_depth(&self) -> f32 {
        match self {
            PpcQ3SoftwareDeferredPrimitive::Triangle { sort_depth, .. }
            | PpcQ3SoftwareDeferredPrimitive::Point { sort_depth, .. }
            | PpcQ3SoftwareDeferredPrimitive::Edge { sort_depth, .. } => *sort_depth,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PpcQ3SoftwareDepthBuffer {
    pub width: u32,
    pub height: u32,
    pub depths: Vec<f32>,
}

impl PpcQ3SoftwareDepthBuffer {
    pub fn new(front_buffer: PpcFrontBuffer) -> Self {
        let len = front_buffer
            .width
            .checked_mul(front_buffer.height)
            .and_then(|pixels| usize::try_from(pixels).ok())
            .unwrap_or(0);
        Self {
            width: front_buffer.width,
            height: front_buffer.height,
            depths: vec![f32::INFINITY; len],
        }
    }

    pub fn test(&self, x: u32, y: u32, z: f32) -> bool {
        if x >= self.width || y >= self.height || !z.is_finite() {
            return false;
        }
        let Some(index) = y
            .checked_mul(self.width)
            .and_then(|row| row.checked_add(x))
            .and_then(|index| usize::try_from(index).ok())
        else {
            return false;
        };
        self.depths.get(index).is_some_and(|depth| z <= *depth)
    }

    pub fn test_and_update(&mut self, x: u32, y: u32, z: f32) -> bool {
        if x >= self.width || y >= self.height || !z.is_finite() {
            return false;
        }
        let Some(index) = y
            .checked_mul(self.width)
            .and_then(|row| row.checked_add(x))
            .and_then(|index| usize::try_from(index).ok())
        else {
            return false;
        };
        let Some(depth) = self.depths.get_mut(index) else {
            return false;
        };
        if z > *depth {
            return false;
        }
        *depth = z;
        true
    }
}

pub fn ppc_q3_software_project_point(
    command: &PpcQ3SceneTriMeshCommand,
    front_buffer: PpcFrontBuffer,
    viewport: PpcQ3ViewportRect,
    point: (f32, f32, f32),
) -> Option<PpcQ3SoftwareProjectedVertex> {
    let world = ppc_q3_point3d_transform_values(point, command.local_to_world);
    let view_direction = ppc_q3_software_view_direction(command.camera, world);
    let (projected, fog_depth, clip) = if let Some(camera) = command.camera {
        let world_to_view = ppc_q3_camera_world_to_view_matrix(camera.placement)?;
        let view_to_frustum = ppc_q3_camera_view_to_frustum_matrix(&camera)?;
        let view = ppc_q3_point3d_transform_values(world, world_to_view);
        let mut clip = ppc_q3_point3d_transform_4d_values(view, view_to_frustum);
        // QuickDraw 3D exposes frustum depth in [-1, 0], with the hither
        // plane at 0.  Keep that guest-visible matrix authentic, then remap
        // depth to the software rasterizer's OpenGL-style [-1, 1] interval.
        clip.2 = (-2.0_f32).mul_add(clip.2, -clip.3);
        (
            ppc_q3_software_clip_to_projected(clip)?,
            -view.2,
            Some(clip),
        )
    } else {
        (world, world.2, None)
    };
    if !ppc_q3_point_finite(projected) || !fog_depth.is_finite() {
        return None;
    }

    let (screen_x, screen_y) =
        ppc_q3_software_projected_to_screen(projected, command.camera, front_buffer, viewport);
    Some(PpcQ3SoftwareProjectedVertex {
        x: screen_x,
        y: screen_y,
        z: projected.2,
        clip,
        world,
        view_direction,
        fog_depth,
        uv: None,
        diffuse: None,
        ambient_coefficient: None,
        normal: None,
        specular_color: None,
        specular_control: None,
        highlight_state: None,
        vertex_alpha: None,
    })
}

pub fn ppc_q3_software_view_direction(
    camera: Option<PpcQ3CameraRecord>,
    world: (f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    camera
        .and_then(|camera| {
            ppc_q3_vector3d_normalized_value(ppc_q3_vector3d_sub(
                camera.placement.camera_location,
                world,
            ))
        })
        .or(Some((0.0, 0.0, 1.0)))
}

pub fn ppc_q3_software_projected_to_screen(
    projected: (f32, f32, f32),
    camera: Option<PpcQ3CameraRecord>,
    front_buffer: PpcFrontBuffer,
    viewport: PpcQ3ViewportRect,
) -> (f32, f32) {
    let (normalized_x, normalized_y) = if let Some(camera) = camera {
        let viewport_x = (projected.0 + 1.0) * 0.5;
        let viewport_y = (projected.1 + 1.0) * 0.5;
        (
            camera.viewport_origin.0 + viewport_x * camera.viewport_width,
            // QD3D camera viewport origins are the upper-left corner of the pane.
            camera.viewport_origin.1 - camera.viewport_height + viewport_y * camera.viewport_height,
        )
    } else {
        (projected.0, projected.1)
    };
    let viewport = if camera.is_some() {
        viewport
    } else {
        PpcQ3ViewportRect::full(front_buffer)
    };
    let max_x = viewport.right.saturating_sub(1) as f32;
    let max_y = viewport.bottom.saturating_sub(1) as f32;
    let width = (max_x - viewport.left as f32).max(0.0);
    let height = (max_y - viewport.top as f32).max(0.0);
    (
        viewport.left as f32 + (normalized_x + 1.0) * 0.5 * width,
        viewport.top as f32 + (1.0 - ((normalized_y + 1.0) * 0.5)) * height,
    )
}

pub fn ppc_q3_software_clip_to_projected(clip: (f32, f32, f32, f32)) -> Option<(f32, f32, f32)> {
    if !clip.0.is_finite() || !clip.1.is_finite() || !clip.2.is_finite() || !clip.3.is_finite() {
        return None;
    }
    if clip.3 == 0.0 {
        return None;
    }
    let projected = (clip.0 / clip.3, clip.1 / clip.3, clip.2 / clip.3);
    if ppc_q3_point_finite(projected) {
        Some(projected)
    } else {
        None
    }
}

pub fn ppc_q3_software_projected_vertex_depth_visible(
    depth_clip: bool,
    vertex: PpcQ3SoftwareProjectedVertex,
) -> bool {
    if !depth_clip {
        return true;
    }
    if let Some(clip) = vertex.clip {
        return ppc_q3_software_clip_inside_frustum(clip);
    }
    vertex.z >= PPC_Q3_SOFTWARE_DEPTH_NEAR && vertex.z <= PPC_Q3_SOFTWARE_DEPTH_FAR
}

pub fn ppc_q3_software_projected_vertex_to_point(
    vertex: PpcQ3SoftwareProjectedVertex,
) -> PpcQ3SoftwareProjectedPoint {
    PpcQ3SoftwareProjectedPoint {
        x: vertex.x.round() as i32,
        y: vertex.y.round() as i32,
        z: vertex.z,
        reciprocal_w: vertex
            .clip
            .map(|clip| 1.0 / clip.3)
            .filter(|value| value.is_finite())
            .unwrap_or(1.0),
        world: vertex.world,
        view_direction: vertex.view_direction,
        fog_depth: vertex.fog_depth,
        uv: vertex.uv,
        diffuse: vertex.diffuse,
        ambient_coefficient: vertex.ambient_coefficient,
        normal: vertex.normal,
        specular_color: vertex.specular_color,
        specular_control: vertex.specular_control,
        highlight_state: vertex.highlight_state,
        vertex_alpha: vertex.vertex_alpha,
    }
}

pub fn ppc_q3_software_clip_projected_triangle(
    camera: Option<PpcQ3CameraRecord>,
    front_buffer: PpcFrontBuffer,
    viewport: PpcQ3ViewportRect,
    vertices: [PpcQ3SoftwareProjectedVertex; 3],
) -> Vec<PpcQ3SoftwareProjectedVertex> {
    let Some(camera) = camera else {
        return vertices.to_vec();
    };
    if vertices.iter().any(|vertex| vertex.clip.is_none()) {
        let near_clipped =
            ppc_q3_software_clip_projected_polygon_z(&vertices, PPC_Q3_SOFTWARE_DEPTH_NEAR, true);
        if near_clipped.len() < 3 {
            return Vec::new();
        }
        return ppc_q3_software_clip_projected_polygon_z(
            &near_clipped,
            PPC_Q3_SOFTWARE_DEPTH_FAR,
            false,
        );
    }

    let mut clipped = vertices.to_vec();
    for plane in [
        PpcQ3SoftwareClipPlane::Left,
        PpcQ3SoftwareClipPlane::Right,
        PpcQ3SoftwareClipPlane::Bottom,
        PpcQ3SoftwareClipPlane::Top,
        PpcQ3SoftwareClipPlane::Near,
        PpcQ3SoftwareClipPlane::Far,
    ] {
        clipped = ppc_q3_software_clip_projected_polygon_clip(
            &clipped,
            plane,
            camera,
            front_buffer,
            viewport,
        );
        if clipped.len() < 3 {
            return Vec::new();
        }
    }
    clipped
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQ3SoftwareClipPlane {
    Left,
    Right,
    Bottom,
    Top,
    Near,
    Far,
}

pub fn ppc_q3_software_clip_inside_frustum(clip: (f32, f32, f32, f32)) -> bool {
    [
        PpcQ3SoftwareClipPlane::Left,
        PpcQ3SoftwareClipPlane::Right,
        PpcQ3SoftwareClipPlane::Bottom,
        PpcQ3SoftwareClipPlane::Top,
        PpcQ3SoftwareClipPlane::Near,
        PpcQ3SoftwareClipPlane::Far,
    ]
    .into_iter()
    .all(|plane| {
        ppc_q3_software_clip_plane_value(clip, plane)
            .is_some_and(|value| value >= -PPC_Q3_SOFTWARE_CLIP_EPSILON)
    })
}

pub fn ppc_q3_software_clip_plane_value(
    (x, y, z, w): (f32, f32, f32, f32),
    plane: PpcQ3SoftwareClipPlane,
) -> Option<f32> {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() || !w.is_finite() {
        return None;
    }
    Some(match plane {
        PpcQ3SoftwareClipPlane::Left => x + w,
        PpcQ3SoftwareClipPlane::Right => w - x,
        PpcQ3SoftwareClipPlane::Bottom => y + w,
        PpcQ3SoftwareClipPlane::Top => w - y,
        PpcQ3SoftwareClipPlane::Near => z + w,
        PpcQ3SoftwareClipPlane::Far => w - z,
    })
}

pub fn ppc_q3_software_clip_projected_polygon_clip(
    vertices: &[PpcQ3SoftwareProjectedVertex],
    plane: PpcQ3SoftwareClipPlane,
    camera: PpcQ3CameraRecord,
    front_buffer: PpcFrontBuffer,
    viewport: PpcQ3ViewportRect,
) -> Vec<PpcQ3SoftwareProjectedVertex> {
    let mut clipped = Vec::new();
    let Some(mut previous) = vertices.last().copied() else {
        return clipped;
    };
    let mut previous_value = previous
        .clip
        .and_then(|clip| ppc_q3_software_clip_plane_value(clip, plane))
        .unwrap_or(f32::NEG_INFINITY);
    let mut previous_inside = previous_value >= -PPC_Q3_SOFTWARE_CLIP_EPSILON;
    for &current in vertices {
        let current_value = current
            .clip
            .and_then(|clip| ppc_q3_software_clip_plane_value(clip, plane))
            .unwrap_or(f32::NEG_INFINITY);
        let current_inside = current_value >= -PPC_Q3_SOFTWARE_CLIP_EPSILON;
        if current_inside != previous_inside {
            if let Some(intersection) = ppc_q3_software_projected_vertex_intersect_clip(
                previous,
                current,
                previous_value,
                current_value,
                camera,
                front_buffer,
                viewport,
            ) {
                clipped.push(intersection);
            }
        }
        if current_inside {
            clipped.push(current);
        }
        previous = current;
        previous_value = current_value;
        previous_inside = current_inside;
    }
    clipped
}

pub fn ppc_q3_software_projected_vertex_intersect_clip(
    start: PpcQ3SoftwareProjectedVertex,
    end: PpcQ3SoftwareProjectedVertex,
    start_value: f32,
    end_value: f32,
    camera: PpcQ3CameraRecord,
    front_buffer: PpcFrontBuffer,
    viewport: PpcQ3ViewportRect,
) -> Option<PpcQ3SoftwareProjectedVertex> {
    let denominator = start_value - end_value;
    if denominator == 0.0 || !denominator.is_finite() {
        return None;
    }
    let t = (start_value / denominator).clamp(0.0, 1.0);
    let start_clip = start.clip?;
    let end_clip = end.clip?;
    let clip = (
        start_clip.0.mul_add(1.0 - t, end_clip.0 * t),
        start_clip.1.mul_add(1.0 - t, end_clip.1 * t),
        start_clip.2.mul_add(1.0 - t, end_clip.2 * t),
        start_clip.3.mul_add(1.0 - t, end_clip.3 * t),
    );
    let projected = ppc_q3_software_clip_to_projected(clip)?;
    let (x, y) =
        ppc_q3_software_projected_to_screen(projected, Some(camera), front_buffer, viewport);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let mut vertex = ppc_q3_software_projected_vertex_interpolate(start, end, t);
    vertex.x = x;
    vertex.y = y;
    vertex.z = projected.2;
    vertex.clip = Some(clip);
    Some(vertex)
}

pub fn ppc_q3_software_clip_projected_polygon_z(
    vertices: &[PpcQ3SoftwareProjectedVertex],
    plane_z: f32,
    keep_greater_or_equal: bool,
) -> Vec<PpcQ3SoftwareProjectedVertex> {
    let mut clipped = Vec::new();
    let Some(mut previous) = vertices.last().copied() else {
        return clipped;
    };
    let mut previous_inside =
        ppc_q3_software_projected_vertex_inside_z(previous, plane_z, keep_greater_or_equal);
    for &current in vertices {
        let current_inside =
            ppc_q3_software_projected_vertex_inside_z(current, plane_z, keep_greater_or_equal);
        if current_inside != previous_inside {
            if let Some(intersection) =
                ppc_q3_software_projected_vertex_intersect_z(previous, current, plane_z)
            {
                clipped.push(intersection);
            }
        }
        if current_inside {
            clipped.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    clipped
}

pub fn ppc_q3_software_projected_vertex_inside_z(
    vertex: PpcQ3SoftwareProjectedVertex,
    plane_z: f32,
    keep_greater_or_equal: bool,
) -> bool {
    if keep_greater_or_equal {
        vertex.z >= plane_z
    } else {
        vertex.z <= plane_z
    }
}

pub fn ppc_q3_software_projected_vertex_intersect_z(
    start: PpcQ3SoftwareProjectedVertex,
    end: PpcQ3SoftwareProjectedVertex,
    plane_z: f32,
) -> Option<PpcQ3SoftwareProjectedVertex> {
    let dz = end.z - start.z;
    if dz == 0.0 {
        return None;
    }
    let t = ((plane_z - start.z) / dz).clamp(0.0, 1.0);
    let mut vertex = ppc_q3_software_projected_vertex_interpolate(start, end, t);
    vertex.z = plane_z;
    if vertex.x.is_finite() && vertex.y.is_finite() {
        Some(vertex)
    } else {
        None
    }
}

pub fn ppc_q3_software_projected_vertex_interpolate(
    start: PpcQ3SoftwareProjectedVertex,
    end: PpcQ3SoftwareProjectedVertex,
    t: f32,
) -> PpcQ3SoftwareProjectedVertex {
    PpcQ3SoftwareProjectedVertex {
        x: start.x.mul_add(1.0 - t, end.x * t),
        y: start.y.mul_add(1.0 - t, end.y * t),
        z: start.z.mul_add(1.0 - t, end.z * t),
        clip: match (start.clip, end.clip) {
            (Some(start_clip), Some(end_clip)) => Some((
                start_clip.0.mul_add(1.0 - t, end_clip.0 * t),
                start_clip.1.mul_add(1.0 - t, end_clip.1 * t),
                start_clip.2.mul_add(1.0 - t, end_clip.2 * t),
                start_clip.3.mul_add(1.0 - t, end_clip.3 * t),
            )),
            _ => None,
        },
        world: (
            start.world.0.mul_add(1.0 - t, end.world.0 * t),
            start.world.1.mul_add(1.0 - t, end.world.1 * t),
            start.world.2.mul_add(1.0 - t, end.world.2 * t),
        ),
        view_direction: match (start.view_direction, end.view_direction) {
            (Some(start_view), Some(end_view)) => ppc_q3_vector3d_normalized_value((
                start_view.0.mul_add(1.0 - t, end_view.0 * t),
                start_view.1.mul_add(1.0 - t, end_view.1 * t),
                start_view.2.mul_add(1.0 - t, end_view.2 * t),
            )),
            _ => None,
        },
        fog_depth: start.fog_depth.mul_add(1.0 - t, end.fog_depth * t),
        uv: match (start.uv, end.uv) {
            (Some(start_uv), Some(end_uv)) => Some((
                start_uv.0.mul_add(1.0 - t, end_uv.0 * t),
                start_uv.1.mul_add(1.0 - t, end_uv.1 * t),
            )),
            _ => None,
        },
        diffuse: match (start.diffuse, end.diffuse) {
            (Some(start_diffuse), Some(end_diffuse)) => Some((
                start_diffuse.0.mul_add(1.0 - t, end_diffuse.0 * t),
                start_diffuse.1.mul_add(1.0 - t, end_diffuse.1 * t),
                start_diffuse.2.mul_add(1.0 - t, end_diffuse.2 * t),
            )),
            _ => None,
        },
        ambient_coefficient: match (start.ambient_coefficient, end.ambient_coefficient) {
            (Some(start_coefficient), Some(end_coefficient)) => {
                Some(start_coefficient.mul_add(1.0 - t, end_coefficient * t))
            }
            _ => None,
        },
        normal: match (start.normal, end.normal) {
            (Some(start_normal), Some(end_normal)) => ppc_q3_vector3d_normalized_value((
                start_normal.0.mul_add(1.0 - t, end_normal.0 * t),
                start_normal.1.mul_add(1.0 - t, end_normal.1 * t),
                start_normal.2.mul_add(1.0 - t, end_normal.2 * t),
            )),
            _ => None,
        },
        specular_color: match (start.specular_color, end.specular_color) {
            (Some(start_specular), Some(end_specular)) => Some((
                start_specular.0.mul_add(1.0 - t, end_specular.0 * t),
                start_specular.1.mul_add(1.0 - t, end_specular.1 * t),
                start_specular.2.mul_add(1.0 - t, end_specular.2 * t),
            )),
            _ => None,
        },
        specular_control: match (start.specular_control, end.specular_control) {
            (Some(start_control), Some(end_control)) => {
                Some(start_control.mul_add(1.0 - t, end_control * t))
            }
            _ => None,
        },
        highlight_state: match (start.highlight_state, end.highlight_state) {
            (Some(start_state), Some(end_state)) => Some(start_state && end_state),
            _ => None,
        },
        vertex_alpha: match (start.vertex_alpha, end.vertex_alpha) {
            (Some(start_alpha), Some(end_alpha)) => {
                Some(start_alpha.mul_add(1.0 - t, end_alpha * t))
            }
            _ => None,
        },
    }
}


pub fn ppc_q3_draw_context_render_target(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
    draw_context: u32,
) -> Option<PpcQ3RenderTarget> {
    let object = q3_objects
        .iter()
        .find(|record| record.object == draw_context)?;
    let record = q3_draw_contexts
        .iter()
        .find(|record| record.draw_context == draw_context)?;
    if object.object_type != record.draw_context_type {
        return None;
    }
    let clear_color =
        (ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_CLEAR_METHOD_OFFSET)
            == Some(PPC_Q3_CLEAR_METHOD_WITH_COLOR))
        .then(|| {
            Some(ppc_q3_rgb555((
                ppc_q3_read_f32_from_slice(
                    &record.data,
                    PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_RED_OFFSET as usize,
                )?,
                ppc_q3_read_f32_from_slice(
                    &record.data,
                    PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_GREEN_OFFSET as usize,
                )?,
                ppc_q3_read_f32_from_slice(
                    &record.data,
                    PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_BLUE_OFFSET as usize,
                )?,
            )))
        })
        .flatten();
    match record.draw_context_type {
        PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP => {
            let base_addr =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_DATA_SIZE)?;
            let width =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET)?;
            let height =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_PIXMAP_DRAW_CONTEXT_HEIGHT_OFFSET)?;
            let row_bytes =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 12)?;
            let pixel_size =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 16)?;
            let pixel_type =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 20)?;
            if base_addr == 0
                || width == 0
                || height == 0
                || pixel_size != 16
                || !matches!(
                    pixel_type,
                    PPC_Q3_PIXEL_TYPE_RGB16 | PPC_Q3_PIXEL_TYPE_ARGB16
                )
                || row_bytes < width.checked_mul(2)?
            {
                return None;
            }
            Some(PpcQ3RenderTarget {
                front_buffer: PpcFrontBuffer {
                    base_addr,
                    row_bytes,
                    width,
                    height,
                    depth: 16,
                },
                viewport: ppc_q3_draw_context_viewport(
                    record,
                    PpcFrontBuffer {
                        base_addr,
                        row_bytes,
                        width,
                        height,
                        depth: 16,
                    },
                ),
                clear_color,
                source: PpcQ3RenderTargetSource::PixmapDrawContext,
                draw_context: Some(draw_context),
                gworld: None,
            })
        }
        PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH => {
            let port =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET)?;
            ppc_front_buffer_for_gworld(gworlds, port).map(|front_buffer| PpcQ3RenderTarget {
                front_buffer,
                viewport: ppc_q3_draw_context_viewport(record, front_buffer),
                clear_color,
                source: PpcQ3RenderTargetSource::MacDrawContext,
                draw_context: Some(draw_context),
                gworld: Some(port),
            })
        }
        _ => None,
    }
}

pub fn ppc_q3_live_draw_context_render_target(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
    draw_context: u32,
) -> Option<PpcQ3RenderTarget> {
    let mut target =
        ppc_q3_draw_context_render_target(q3_objects, q3_draw_contexts, gworlds, draw_context)?;
    if target.source != PpcQ3RenderTargetSource::MacDrawContext {
        return Some(target);
    }
    let port = target.gworld?;
    let gworld = gworlds.iter().find(|record| record.port == port)?;
    if gworld.pixmap_handle == 0 && gworld.pixmap == 0 {
        return Some(target);
    }
    let surface = ppc_live_quickdraw_surface(memory, gworlds, port)?;
    let record = q3_draw_contexts
        .iter()
        .find(|record| record.draw_context == draw_context)?;
    target.front_buffer = surface.front_buffer;
    target.viewport = ppc_q3_draw_context_viewport_with_origin(
        record,
        surface.front_buffer,
        -i32::from(surface.left),
        -i32::from(surface.top),
    );
    Some(target)
}

pub fn ppc_q3_draw_context_viewport(
    record: &PpcQ3DrawContextRecord,
    front_buffer: PpcFrontBuffer,
) -> Option<PpcQ3ViewportRect> {
    ppc_q3_draw_context_viewport_with_origin(record, front_buffer, 0, 0)
}

pub fn ppc_q3_draw_context_viewport_with_origin(
    record: &PpcQ3DrawContextRecord,
    front_buffer: PpcFrontBuffer,
    origin_x: i32,
    origin_y: i32,
) -> Option<PpcQ3ViewportRect> {
    let pane_state =
        ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET)
            .unwrap_or(0);
    if pane_state == 0 {
        return None;
    }
    let pane_start = usize::try_from(PPC_Q3_DRAW_CONTEXT_PANE_OFFSET).ok()?;
    let min_x = ppc_q3_read_f32_from_slice(&record.data, pane_start)?;
    let min_y = ppc_q3_read_f32_from_slice(&record.data, pane_start.checked_add(4)?)?;
    let max_x = ppc_q3_read_f32_from_slice(&record.data, pane_start.checked_add(8)?)?;
    let max_y = ppc_q3_read_f32_from_slice(&record.data, pane_start.checked_add(12)?)?;
    PpcQ3ViewportRect::from_q3_area(
        front_buffer,
        min_x + origin_x as f32,
        min_y + origin_y as f32,
        max_x + origin_x as f32,
        max_y + origin_y as f32,
    )
}

pub fn ppc_q3_front_buffer_pixel_addr(front_buffer: PpcFrontBuffer, (x, y): (i32, i32)) -> Option<u32> {
    let offset = ppc_q3_front_buffer_pixel_relative_offset(front_buffer, (x, y))?;
    front_buffer
        .base_addr
        .checked_add(u32::try_from(offset).ok()?)
}

pub fn ppc_q3_front_buffer_pixel_relative_offset(
    front_buffer: PpcFrontBuffer,
    (x, y): (i32, i32),
) -> Option<usize> {
    if x < 0 || y < 0 {
        return None;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= front_buffer.width || y >= front_buffer.height {
        return None;
    }
    let pixel_offset = x.checked_mul(2)?;
    let pixel_end = pixel_offset.checked_add(2)?;
    if pixel_end > front_buffer.row_bytes {
        return None;
    }
    let row_offset = y.checked_mul(front_buffer.row_bytes)?;
    let offset = row_offset.checked_add(pixel_offset)?;
    usize::try_from(offset).ok()
}

pub fn ppc_q3_front_buffer_span_len(front_buffer: PpcFrontBuffer) -> Option<usize> {
    if front_buffer.depth != 16 || front_buffer.height == 0 {
        return None;
    }
    let rows_before_last = front_buffer.height.checked_sub(1)?;
    let last_row_offset = rows_before_last.checked_mul(front_buffer.row_bytes)?;
    let last_row_bytes = front_buffer.width.checked_mul(2)?;
    let len = last_row_offset.checked_add(last_row_bytes)?;
    usize::try_from(len).ok()
}

pub fn ppc_q3_read_software_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
) -> Option<u16> {
    memory.read_u16_be(ppc_q3_front_buffer_pixel_addr(front_buffer, point)?)
}

pub fn ppc_q3_write_software_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
    color: u16,
) -> bool {
    let Some(addr) = ppc_q3_front_buffer_pixel_addr(front_buffer, point) else {
        return false;
    };
    memory.write_u16_be(addr, color).is_some()
}


pub fn ppc_q3_software_material_output_pixel_color(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    point: (i32, i32),
    source: (f32, f32, f32),
    transparency: (f32, f32, f32),
) -> u16 {
    if transparency == (1.0, 1.0, 1.0) {
        return ppc_q3_rgb555(source);
    }
    let destination = surface
        .read_pixel(memory, point)
        .map(ppc_q3_rgb555_to_color)
        .unwrap_or(source);
    ppc_q3_rgb555(ppc_q3_blend_transparency_color(
        source,
        destination,
        transparency,
    ))
}

pub fn ppc_q3_write_software_depth_material_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &mut PpcQ3SoftwareDepthBuffer,
    point: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> bool {
    if point.x < 0 || point.y < 0 {
        return false;
    }
    let (x, y) = (point.x as u32, point.y as u32);
    if !depth_buffer.test_and_update(x, y, point.z) {
        return false;
    }
    let output = ppc_q3_software_material_shaded_output(
        memory,
        material,
        lights,
        point.uv,
        point.diffuse,
        point.ambient_coefficient,
        point.normal,
        point.specular_color,
        point.specular_control,
        point.highlight_state,
        Some(point.world),
        point.view_direction,
        point.fog_depth,
        point.vertex_alpha,
    );
    ppc_q3_write_software_material_output_pixel(memory, surface, point, material, output)
}

pub fn ppc_q3_draw_deferred_software_point(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &mut PpcQ3SoftwareDepthBuffer,
    point: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> bool {
    let output = ppc_q3_software_material_shaded_output(
        memory,
        material,
        lights,
        point.uv,
        point.diffuse,
        point.ambient_coefficient,
        point.normal,
        point.specular_color,
        point.specular_control,
        point.highlight_state,
        Some(point.world),
        point.view_direction,
        point.fog_depth,
        point.vertex_alpha,
    );
    ppc_q3_write_software_depth_material_output_pixel(
        memory,
        surface,
        depth_buffer,
        point,
        material,
        output,
        false,
    )
}

pub fn ppc_q3_write_software_depth_material_output_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &mut PpcQ3SoftwareDepthBuffer,
    point: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    output: PpcQ3SoftwareMaterialOutput,
    write_depth: bool,
) -> bool {
    if point.x < 0 || point.y < 0 {
        return false;
    }
    let (x, y) = (point.x as u32, point.y as u32);
    let depth_visible = if write_depth {
        depth_buffer.test_and_update(x, y, point.z)
    } else {
        depth_buffer.test(x, y, point.z)
    };
    if !depth_visible {
        return false;
    }
    ppc_q3_write_software_material_output_pixel(memory, surface, point, material, output)
}

pub fn ppc_q3_write_software_material_output_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    point: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    output: PpcQ3SoftwareMaterialOutput,
) -> bool {
    let opacity = ppc_q3_software_blend_opacity(material, output.opacity, point.vertex_alpha);
    let transparency = ppc_q3_software_effective_transparency(material.transparency, opacity);
    let color = ppc_q3_software_material_output_pixel_color(
        memory,
        surface,
        (point.x, point.y),
        output.color,
        transparency,
    );
    surface.write_pixel(memory, (point.x, point.y), color)
}

pub fn ppc_q3_draw_software_triangle(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &mut PpcQ3SoftwareDepthBuffer,
    vertices: [PpcQ3SoftwareProjectedPoint; 3],
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
    interpolation_style: u32,
    viewport: PpcQ3ViewportRect,
    write_depth: bool,
) -> usize {
    let Some((viewport_left, viewport_top, viewport_right, viewport_bottom)) =
        viewport.inclusive_bounds()
    else {
        return 0;
    };
    let [a, b, c] = vertices;
    let area = ppc_q3_software_edge_value(a, b, c);
    if area == 0 {
        let point =
            ppc_q3_software_interpolate_projected_point(&vertices, [1.0 / 3.0; 3], a.x, a.y);
        let output = ppc_q3_software_material_shaded_output(
            memory,
            material,
            lights,
            point.uv,
            point.diffuse,
            point.ambient_coefficient,
            point.normal,
            point.specular_color,
            point.specular_control,
            point.highlight_state,
            Some(point.world),
            point.view_direction,
            point.fog_depth,
            point.vertex_alpha,
        );
        let opacity = ppc_q3_software_blend_opacity(material, output.opacity, point.vertex_alpha);
        let transparency = ppc_q3_software_effective_transparency(material.transparency, opacity);
        return ppc_q3_draw_software_line(memory, surface, a, b, output.color, transparency)
            .saturating_add(ppc_q3_draw_software_line(
                memory,
                surface,
                b,
                c,
                output.color,
                transparency,
            ))
            .saturating_add(ppc_q3_draw_software_line(
                memory,
                surface,
                c,
                a,
                output.color,
                transparency,
            ));
    }

    let min_x = a.x.min(b.x).min(c.x).max(viewport_left);
    let min_y = a.y.min(b.y).min(c.y).max(viewport_top);
    let max_x = a.x.max(b.x).max(c.x).min(viewport_right);
    let max_y = a.y.max(b.y).max(c.y).min(viewport_bottom);
    if min_x > max_x || min_y > max_y {
        return 0;
    }

    // QD3D interpolation styles select where illumination is evaluated.
    // Texturing and fog remain per-pixel even for flat and vertex lighting.
    let mut lighting_material = *material;
    lighting_material.texture = None;
    lighting_material.fog_style = None;
    let flat_sample = if interpolation_style == PPC_Q3_INTERPOLATION_STYLE_NONE {
        let point =
            ppc_q3_software_interpolate_projected_point(&vertices, [1.0 / 3.0; 3], a.x, a.y);
        let output = ppc_q3_software_material_shaded_output(
            memory,
            &lighting_material,
            lights,
            point.uv,
            point.diffuse,
            point.ambient_coefficient,
            point.normal,
            point.specular_color,
            point.specular_control,
            point.highlight_state,
            Some(point.world),
            point.view_direction,
            point.fog_depth,
            point.vertex_alpha,
        );
        Some((point, output))
    } else {
        None
    };
    let vertex_outputs = if interpolation_style == PPC_Q3_INTERPOLATION_STYLE_VERTEX {
        Some(vertices.map(|point| {
            ppc_q3_software_material_shaded_output(
                memory,
                &lighting_material,
                lights,
                point.uv,
                point.diffuse,
                point.ambient_coefficient,
                point.normal,
                point.specular_color,
                point.specular_control,
                point.highlight_state,
                Some(point.world),
                point.view_direction,
                point.fog_depth,
                point.vertex_alpha,
            )
        }))
    } else {
        None
    };

    let mut pixels = 0usize;
    let area_f = area as f32;
    let (mut row_edge_weights, edge_x_steps, edge_y_steps) =
        ppc_q3_software_triangle_edge_walker(vertices, min_x, min_y);
    for y in min_y..=max_y {
        let mut edge_weights = row_edge_weights;
        for x in min_x..=max_x {
            let [w0, w1, w2] = edge_weights;
            edge_weights[0] += edge_x_steps[0];
            edge_weights[1] += edge_x_steps[1];
            edge_weights[2] += edge_x_steps[2];
            if !ppc_q3_software_weights_inside(area, w0, w1, w2) {
                continue;
            }
            let weight0 = w0 as f32 / area_f;
            let weight1 = w1 as f32 / area_f;
            let weight2 = w2 as f32 / area_f;
            let perspective_weights =
                ppc_q3_software_perspective_weights(&vertices, [weight0, weight1, weight2]);
            let point = ppc_q3_software_interpolate_projected_point_with_weights(
                &vertices,
                [weight0, weight1, weight2],
                perspective_weights,
                x,
                y,
            );
            let (write_point, output, needs_texture_and_fog) = match interpolation_style {
                PPC_Q3_INTERPOLATION_STYLE_NONE => {
                    let (flat_point, output) =
                        flat_sample.expect("flat interpolation output should be precomputed");
                    (
                        PpcQ3SoftwareProjectedPoint {
                            diffuse: flat_point.diffuse,
                            normal: flat_point.normal,
                            specular_color: flat_point.specular_color,
                            specular_control: flat_point.specular_control,
                            highlight_state: flat_point.highlight_state,
                            vertex_alpha: flat_point.vertex_alpha,
                            ..point
                        },
                        output,
                        true,
                    )
                }
                PPC_Q3_INTERPOLATION_STYLE_VERTEX => (
                    point,
                    ppc_q3_software_interpolate_material_output(
                        vertex_outputs.expect("vertex interpolation outputs should be precomputed"),
                        perspective_weights[0],
                        perspective_weights[1],
                        perspective_weights[2],
                    ),
                    true,
                ),
                _ => (
                    point,
                    ppc_q3_software_material_shaded_output(
                        memory,
                        material,
                        lights,
                        point.uv,
                        point.diffuse,
                        point.ambient_coefficient,
                        point.normal,
                        point.specular_color,
                        point.specular_control,
                        point.highlight_state,
                        Some(point.world),
                        point.view_direction,
                        point.fog_depth,
                        point.vertex_alpha,
                    ),
                    false,
                ),
            };
            let output = if needs_texture_and_fog {
                ppc_q3_software_apply_texture_and_fog(
                    memory,
                    material,
                    write_point.uv,
                    write_point.fog_depth,
                    write_point.vertex_alpha,
                    output,
                )
            } else {
                output
            };
            if ppc_q3_write_software_depth_material_output_pixel(
                memory,
                surface,
                depth_buffer,
                write_point,
                material,
                output,
                write_depth,
            ) {
                pixels = pixels.saturating_add(1);
            }
        }
        row_edge_weights[0] += edge_y_steps[0];
        row_edge_weights[1] += edge_y_steps[1];
        row_edge_weights[2] += edge_y_steps[2];
    }
    pixels
}

pub fn ppc_q3_software_primitive_deferred_for_transparency(
    material: &PpcQ3SoftwareMaterialSample,
    vertices: &[PpcQ3SoftwareProjectedPoint],
) -> bool {
    if material.transparency.0 < 0.999
        || material.transparency.1 < 0.999
        || material.transparency.2 < 0.999
    {
        return true;
    }
    if ppc_q3_software_texture_may_blend(material.texture) {
        return true;
    }
    if !ppc_q3_software_material_uses_alpha_fog(material.fog_style) {
        return vertices
            .iter()
            .any(|vertex| vertex.vertex_alpha.is_some_and(|alpha| alpha < 0.999));
    }
    false
}

pub fn ppc_q3_software_texture_may_blend(texture: Option<PpcQ3SoftwareTexture>) -> bool {
    matches!(
        texture.map(|texture| texture.pixel_type),
        Some(PPC_Q3_PIXEL_TYPE_ARGB32 | PPC_Q3_PIXEL_TYPE_ARGB16)
    )
}

pub fn ppc_q3_software_triangle_sort_depth(vertices: [PpcQ3SoftwareProjectedPoint; 3]) -> f32 {
    (vertices[0].z + vertices[1].z + vertices[2].z) / 3.0
}

pub fn ppc_q3_software_line_sort_depth(
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
) -> f32 {
    (start.z + end.z) * 0.5
}

pub fn ppc_q3_software_interpolate_projected_point(
    vertices: &[PpcQ3SoftwareProjectedPoint; 3],
    weights: [f32; 3],
    x: i32,
    y: i32,
) -> PpcQ3SoftwareProjectedPoint {
    let perspective_weights = ppc_q3_software_perspective_weights(vertices, weights);
    ppc_q3_software_interpolate_projected_point_with_weights(
        vertices,
        weights,
        perspective_weights,
        x,
        y,
    )
}

pub fn ppc_q3_software_interpolate_projected_point_with_weights(
    vertices: &[PpcQ3SoftwareProjectedPoint; 3],
    weights: [f32; 3],
    perspective_weights: [f32; 3],
    x: i32,
    y: i32,
) -> PpcQ3SoftwareProjectedPoint {
    let [a, b, c] = vertices;
    let [affine_weight0, affine_weight1, affine_weight2] = weights;
    let [weight0, weight1, weight2] = perspective_weights;
    PpcQ3SoftwareProjectedPoint {
        x,
        y,
        z: affine_weight0.mul_add(a.z, affine_weight1.mul_add(b.z, affine_weight2 * c.z)),
        reciprocal_w: affine_weight0.mul_add(
            a.reciprocal_w,
            affine_weight1.mul_add(b.reciprocal_w, affine_weight2 * c.reciprocal_w),
        ),
        world: (
            weight0.mul_add(a.world.0, weight1.mul_add(b.world.0, weight2 * c.world.0)),
            weight0.mul_add(a.world.1, weight1.mul_add(b.world.1, weight2 * c.world.1)),
            weight0.mul_add(a.world.2, weight1.mul_add(b.world.2, weight2 * c.world.2)),
        ),
        view_direction: match (a.view_direction, b.view_direction, c.view_direction) {
            (Some(view_a), Some(view_b), Some(view_c)) => ppc_q3_vector3d_normalized_value((
                weight0.mul_add(view_a.0, weight1.mul_add(view_b.0, weight2 * view_c.0)),
                weight0.mul_add(view_a.1, weight1.mul_add(view_b.1, weight2 * view_c.1)),
                weight0.mul_add(view_a.2, weight1.mul_add(view_b.2, weight2 * view_c.2)),
            )),
            _ => None,
        },
        fog_depth: weight0.mul_add(
            a.fog_depth,
            weight1.mul_add(b.fog_depth, weight2 * c.fog_depth),
        ),
        uv: match (a.uv, b.uv, c.uv) {
            (Some(uv_a), Some(uv_b), Some(uv_c)) => Some((
                weight0.mul_add(uv_a.0, weight1.mul_add(uv_b.0, weight2 * uv_c.0)),
                weight0.mul_add(uv_a.1, weight1.mul_add(uv_b.1, weight2 * uv_c.1)),
            )),
            _ => None,
        },
        diffuse: match (a.diffuse, b.diffuse, c.diffuse) {
            (Some(diffuse_a), Some(diffuse_b), Some(diffuse_c)) => Some((
                weight0.mul_add(
                    diffuse_a.0,
                    weight1.mul_add(diffuse_b.0, weight2 * diffuse_c.0),
                ),
                weight0.mul_add(
                    diffuse_a.1,
                    weight1.mul_add(diffuse_b.1, weight2 * diffuse_c.1),
                ),
                weight0.mul_add(
                    diffuse_a.2,
                    weight1.mul_add(diffuse_b.2, weight2 * diffuse_c.2),
                ),
            )),
            _ => None,
        },
        ambient_coefficient: match (
            a.ambient_coefficient,
            b.ambient_coefficient,
            c.ambient_coefficient,
        ) {
            (Some(coefficient_a), Some(coefficient_b), Some(coefficient_c)) => {
                Some(weight0.mul_add(
                    coefficient_a,
                    weight1.mul_add(coefficient_b, weight2 * coefficient_c),
                ))
            }
            _ => None,
        },
        normal: match (a.normal, b.normal, c.normal) {
            (Some(normal_a), Some(normal_b), Some(normal_c)) => ppc_q3_vector3d_normalized_value((
                weight0.mul_add(
                    normal_a.0,
                    weight1.mul_add(normal_b.0, weight2 * normal_c.0),
                ),
                weight0.mul_add(
                    normal_a.1,
                    weight1.mul_add(normal_b.1, weight2 * normal_c.1),
                ),
                weight0.mul_add(
                    normal_a.2,
                    weight1.mul_add(normal_b.2, weight2 * normal_c.2),
                ),
            )),
            _ => None,
        },
        specular_color: match (a.specular_color, b.specular_color, c.specular_color) {
            (Some(specular_a), Some(specular_b), Some(specular_c)) => Some((
                weight0.mul_add(
                    specular_a.0,
                    weight1.mul_add(specular_b.0, weight2 * specular_c.0),
                ),
                weight0.mul_add(
                    specular_a.1,
                    weight1.mul_add(specular_b.1, weight2 * specular_c.1),
                ),
                weight0.mul_add(
                    specular_a.2,
                    weight1.mul_add(specular_b.2, weight2 * specular_c.2),
                ),
            )),
            _ => None,
        },
        specular_control: match (a.specular_control, b.specular_control, c.specular_control) {
            (Some(control_a), Some(control_b), Some(control_c)) => {
                Some(weight0.mul_add(control_a, weight1.mul_add(control_b, weight2 * control_c)))
            }
            _ => None,
        },
        highlight_state: match (a.highlight_state, b.highlight_state, c.highlight_state) {
            (Some(state_a), Some(state_b), Some(state_c)) => Some(state_a && state_b && state_c),
            _ => None,
        },
        vertex_alpha: match (a.vertex_alpha, b.vertex_alpha, c.vertex_alpha) {
            (Some(alpha_a), Some(alpha_b), Some(alpha_c)) => {
                Some(weight0.mul_add(alpha_a, weight1.mul_add(alpha_b, weight2 * alpha_c)))
            }
            _ => None,
        },
    }
}

pub fn ppc_q3_software_perspective_weights(
    vertices: &[PpcQ3SoftwareProjectedPoint; 3],
    weights: [f32; 3],
) -> [f32; 3] {
    let weighted = [
        weights[0] * vertices[0].reciprocal_w,
        weights[1] * vertices[1].reciprocal_w,
        weights[2] * vertices[2].reciprocal_w,
    ];
    let sum = weighted[0] + weighted[1] + weighted[2];
    if !sum.is_finite() || sum.abs() <= f32::EPSILON {
        return weights;
    }
    [weighted[0] / sum, weighted[1] / sum, weighted[2] / sum]
}

pub fn ppc_q3_software_interpolate_material_output(
    outputs: [PpcQ3SoftwareMaterialOutput; 3],
    weight0: f32,
    weight1: f32,
    weight2: f32,
) -> PpcQ3SoftwareMaterialOutput {
    let [a, b, c] = outputs;
    PpcQ3SoftwareMaterialOutput {
        color: (
            weight0.mul_add(a.color.0, weight1.mul_add(b.color.0, weight2 * c.color.0)),
            weight0.mul_add(a.color.1, weight1.mul_add(b.color.1, weight2 * c.color.1)),
            weight0.mul_add(a.color.2, weight1.mul_add(b.color.2, weight2 * c.color.2)),
        ),
        opacity: weight0.mul_add(a.opacity, weight1.mul_add(b.opacity, weight2 * c.opacity)),
    }
}

pub fn ppc_q3_software_edge_value(
    a: PpcQ3SoftwareProjectedPoint,
    b: PpcQ3SoftwareProjectedPoint,
    c: PpcQ3SoftwareProjectedPoint,
) -> i64 {
    i64::from(c.x - a.x) * i64::from(b.y - a.y) - i64::from(c.y - a.y) * i64::from(b.x - a.x)
}

pub fn ppc_q3_software_triangle_edge_walker(
    [a, b, c]: [PpcQ3SoftwareProjectedPoint; 3],
    x: i32,
    y: i32,
) -> ([i64; 3], [i64; 3], [i64; 3]) {
    let point = PpcQ3SoftwareProjectedPoint {
        x,
        y,
        z: 0.0,
        reciprocal_w: 1.0,
        world: (0.0, 0.0, 0.0),
        view_direction: None,
        fog_depth: 0.0,
        uv: None,
        diffuse: None,
        ambient_coefficient: None,
        normal: None,
        specular_color: None,
        specular_control: None,
        highlight_state: None,
        vertex_alpha: None,
    };
    (
        [
            ppc_q3_software_edge_value(b, c, point),
            ppc_q3_software_edge_value(c, a, point),
            ppc_q3_software_edge_value(a, b, point),
        ],
        [
            i64::from(c.y - b.y),
            i64::from(a.y - c.y),
            i64::from(b.y - a.y),
        ],
        [
            -i64::from(c.x - b.x),
            -i64::from(a.x - c.x),
            -i64::from(b.x - a.x),
        ],
    )
}

pub fn ppc_q3_software_is_backfacing(
    orientation_style: u32,
    vertices: &[PpcQ3SoftwareProjectedPoint],
) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    // Screen-space y points downward, reversing QD3D's projected winding.
    let edge = ppc_q3_software_edge_value(vertices[0], vertices[1], vertices[2]);
    if orientation_style == PPC_Q3_ORIENTATION_STYLE_CLOCKWISE {
        edge < 0
    } else {
        edge > 0
    }
}

pub fn ppc_q3_software_culls_backfacing(
    backfacing_style: u32,
    orientation_style: u32,
    vertices: &[PpcQ3SoftwareProjectedPoint],
) -> bool {
    match backfacing_style {
        PPC_Q3_BACKFACING_STYLE_REMOVE => {
            ppc_q3_software_is_backfacing(orientation_style, vertices)
        }
        PPC_Q3_BACKFACING_STYLE_BOTH | PPC_Q3_BACKFACING_STYLE_FLIP => false,
        _ => false,
    }
}

pub fn ppc_q3_software_flip_backfacing_normals(
    backfacing_style: u32,
    orientation_style: u32,
    vertices: &mut [PpcQ3SoftwareProjectedPoint],
) {
    if backfacing_style != PPC_Q3_BACKFACING_STYLE_FLIP
        || !ppc_q3_software_is_backfacing(orientation_style, vertices)
    {
        return;
    }
    for vertex in vertices {
        if let Some((x, y, z)) = vertex.normal {
            vertex.normal = Some((-x, -y, -z));
        }
    }
}

pub fn ppc_q3_software_weights_inside(area: i64, w0: i64, w1: i64, w2: i64) -> bool {
    if area > 0 {
        w0 >= 0 && w1 >= 0 && w2 >= 0
    } else {
        w0 <= 0 && w1 <= 0 && w2 <= 0
    }
}

pub fn ppc_q3_draw_software_line(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
    source: (f32, f32, f32),
    transparency: (f32, f32, f32),
) -> usize {
    let (mut x0, mut y0) = (start.x, start.y);
    let (x1, y1) = (end.x, end.y);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -((y1 - y0).abs());
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut pixels = 0usize;

    loop {
        let color = ppc_q3_software_material_output_pixel_color(
            memory,
            surface,
            (x0, y0),
            source,
            transparency,
        );
        if surface.write_pixel(memory, (x0, y0), color) {
            pixels = pixels.saturating_add(1);
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err.saturating_mul(2);
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    pixels
}

pub fn ppc_q3_draw_software_line_depth_test(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &PpcQ3SoftwareDepthBuffer,
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
    source: (f32, f32, f32),
    transparency: (f32, f32, f32),
) -> usize {
    let (mut x0, mut y0) = (start.x, start.y);
    let (x1, y1) = (end.x, end.y);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy_abs = (y1 - y0).abs();
    let dy = -dy_abs;
    let sy = if y0 < y1 { 1 } else { -1 };
    let steps = dx.max(dy_abs).max(1) as f32;
    let mut err = dx + dy;
    let mut step = 0usize;
    let mut pixels = 0usize;

    loop {
        let t = (step as f32 / steps).clamp(0.0, 1.0);
        let z = start.z.mul_add(1.0 - t, end.z * t);
        if x0 >= 0 && y0 >= 0 && depth_buffer.test(x0 as u32, y0 as u32, z) {
            let color = ppc_q3_software_material_output_pixel_color(
                memory,
                surface,
                (x0, y0),
                source,
                transparency,
            );
            if surface.write_pixel(memory, (x0, y0), color) {
                pixels = pixels.saturating_add(1);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err.saturating_mul(2);
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
        step = step.saturating_add(1);
    }
    pixels
}

pub fn ppc_q3_draw_or_defer_software_edge_loop(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    vertices: &[PpcQ3SoftwareProjectedPoint],
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
    edge_materials: &[Option<PpcQ3SoftwareEdgeMaterial>],
    deferred_primitives: &mut Vec<PpcQ3SoftwareDeferredPrimitive>,
) -> usize {
    if vertices.len() < 2 {
        return 0;
    }
    let mut pixels = 0usize;
    for index in 0..vertices.len() {
        let (start, end) = ppc_q3_software_edge_material_points(
            vertices[index],
            vertices[(index + 1) % vertices.len()],
            edge_materials.get(index).and_then(|record| *record),
        );
        if ppc_q3_software_primitive_deferred_for_transparency(material, &[start, end]) {
            deferred_primitives.push(PpcQ3SoftwareDeferredPrimitive::Edge {
                start,
                end,
                material: *material,
                lights: lights.clone(),
                sort_depth: ppc_q3_software_line_sort_depth(start, end),
            });
        } else {
            pixels = pixels.saturating_add(ppc_q3_draw_software_material_line(
                memory, surface, start, end, material, lights,
            ));
        }
    }
    pixels
}

pub fn ppc_q3_software_edge_material_points(
    mut start: PpcQ3SoftwareProjectedPoint,
    mut end: PpcQ3SoftwareProjectedPoint,
    edge_material: Option<PpcQ3SoftwareEdgeMaterial>,
) -> (PpcQ3SoftwareProjectedPoint, PpcQ3SoftwareProjectedPoint) {
    if let Some(edge_material) = edge_material {
        if let Some(diffuse) = edge_material.diffuse {
            start.diffuse = Some(diffuse);
            end.diffuse = Some(diffuse);
        }
        if let Some(ambient_coefficient) = edge_material.ambient_coefficient {
            start.ambient_coefficient = Some(ambient_coefficient);
            end.ambient_coefficient = Some(ambient_coefficient);
        }
        if let Some(vertex_alpha) = edge_material.vertex_alpha {
            start.vertex_alpha = Some(vertex_alpha);
            end.vertex_alpha = Some(vertex_alpha);
        }
        if let Some(specular_color) = edge_material.specular_color {
            start.specular_color = Some(specular_color);
            end.specular_color = Some(specular_color);
        }
        if let Some(specular_control) = edge_material.specular_control {
            start.specular_control = Some(specular_control);
            end.specular_control = Some(specular_control);
        }
        if let Some(highlight_state) = edge_material.highlight_state {
            start.highlight_state = Some(highlight_state);
            end.highlight_state = Some(highlight_state);
        }
        if let Some(normal) = edge_material.normal {
            start.normal = Some(normal);
            end.normal = Some(normal);
        }
    }
    (start, end)
}

pub fn ppc_q3_software_triangle_edge_materials(
    edge_indices: &HashMap<u64, usize>,
    edge_diffuse_colors: Option<&[(f32, f32, f32)]>,
    edge_ambient_coefficients: Option<&[f32]>,
    edge_specular_colors: Option<&[(f32, f32, f32)]>,
    edge_specular_controls: Option<&[f32]>,
    edge_highlight_states: Option<&[bool]>,
    edge_normals: Option<&[(f32, f32, f32)]>,
    edge_alphas: Option<&[f32]>,
    points: [usize; 3],
) -> [Option<PpcQ3SoftwareEdgeMaterial>; 3] {
    [
        ppc_q3_software_edge_material(
            edge_indices,
            edge_diffuse_colors,
            edge_ambient_coefficients,
            edge_specular_colors,
            edge_specular_controls,
            edge_highlight_states,
            edge_normals,
            edge_alphas,
            [points[0], points[1]],
        ),
        ppc_q3_software_edge_material(
            edge_indices,
            edge_diffuse_colors,
            edge_ambient_coefficients,
            edge_specular_colors,
            edge_specular_controls,
            edge_highlight_states,
            edge_normals,
            edge_alphas,
            [points[1], points[2]],
        ),
        ppc_q3_software_edge_material(
            edge_indices,
            edge_diffuse_colors,
            edge_ambient_coefficients,
            edge_specular_colors,
            edge_specular_controls,
            edge_highlight_states,
            edge_normals,
            edge_alphas,
            [points[2], points[0]],
        ),
    ]
}

pub fn ppc_q3_software_edge_index_map(edges: &[PpcQ3SoftwareEdge]) -> HashMap<u64, usize> {
    let mut edge_indices = HashMap::with_capacity(edges.len());
    for edge in edges {
        edge_indices.insert(
            ppc_q3_software_edge_key(edge.points[0], edge.points[1]),
            edge.index,
        );
    }
    edge_indices
}

pub fn ppc_q3_software_edge_key(a: usize, b: usize) -> u64 {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    ((a as u64) << 32) | b as u64
}

pub fn ppc_q3_software_edge_material(
    edge_indices: &HashMap<u64, usize>,
    edge_diffuse_colors: Option<&[(f32, f32, f32)]>,
    edge_ambient_coefficients: Option<&[f32]>,
    edge_specular_colors: Option<&[(f32, f32, f32)]>,
    edge_specular_controls: Option<&[f32]>,
    edge_highlight_states: Option<&[bool]>,
    edge_normals: Option<&[(f32, f32, f32)]>,
    edge_alphas: Option<&[f32]>,
    points: [usize; 2],
) -> Option<PpcQ3SoftwareEdgeMaterial> {
    let edge_index = *edge_indices.get(&ppc_q3_software_edge_key(points[0], points[1]))?;
    let diffuse = edge_diffuse_colors.and_then(|colors| colors.get(edge_index).copied());
    let ambient_coefficient =
        edge_ambient_coefficients.and_then(|coefficients| coefficients.get(edge_index).copied());
    let specular_color = edge_specular_colors.and_then(|colors| colors.get(edge_index).copied());
    let specular_control =
        edge_specular_controls.and_then(|controls| controls.get(edge_index).copied());
    let highlight_state = edge_highlight_states.and_then(|states| states.get(edge_index).copied());
    let normal = edge_normals.and_then(|normals| normals.get(edge_index).copied());
    let vertex_alpha = edge_alphas.and_then(|alphas| alphas.get(edge_index).copied());
    if diffuse.is_none()
        && ambient_coefficient.is_none()
        && specular_color.is_none()
        && specular_control.is_none()
        && highlight_state.is_none()
        && normal.is_none()
        && vertex_alpha.is_none()
    {
        None
    } else {
        Some(PpcQ3SoftwareEdgeMaterial {
            diffuse,
            ambient_coefficient,
            specular_color,
            specular_control,
            highlight_state,
            normal,
            vertex_alpha,
        })
    }
}

pub fn ppc_q3_draw_software_material_line(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> usize {
    let midpoint = ppc_q3_software_projected_line_midpoint(start, end);
    let output = ppc_q3_software_material_shaded_output(
        memory,
        material,
        lights,
        midpoint.uv,
        midpoint.diffuse,
        midpoint.ambient_coefficient,
        midpoint.normal,
        midpoint.specular_color,
        midpoint.specular_control,
        midpoint.highlight_state,
        Some(midpoint.world),
        midpoint.view_direction,
        midpoint.fog_depth,
        midpoint.vertex_alpha,
    );
    let opacity = ppc_q3_software_blend_opacity(material, output.opacity, midpoint.vertex_alpha);
    let transparency = ppc_q3_software_effective_transparency(material.transparency, opacity);
    ppc_q3_draw_software_line(memory, surface, start, end, output.color, transparency)
}

pub fn ppc_q3_draw_deferred_software_material_line(
    memory: &mut PpcSectionMem,
    surface: PpcQ3SoftwareFrontBufferSurface,
    depth_buffer: &PpcQ3SoftwareDepthBuffer,
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
    material: &PpcQ3SoftwareMaterialSample,
    lights: &PpcQ3SubmissionLightRecord,
) -> usize {
    let midpoint = ppc_q3_software_projected_line_midpoint(start, end);
    let output = ppc_q3_software_material_shaded_output(
        memory,
        material,
        lights,
        midpoint.uv,
        midpoint.diffuse,
        midpoint.ambient_coefficient,
        midpoint.normal,
        midpoint.specular_color,
        midpoint.specular_control,
        midpoint.highlight_state,
        Some(midpoint.world),
        midpoint.view_direction,
        midpoint.fog_depth,
        midpoint.vertex_alpha,
    );
    let opacity = ppc_q3_software_blend_opacity(material, output.opacity, midpoint.vertex_alpha);
    let transparency = ppc_q3_software_effective_transparency(material.transparency, opacity);
    ppc_q3_draw_software_line_depth_test(
        memory,
        surface,
        depth_buffer,
        start,
        end,
        output.color,
        transparency,
    )
}

pub fn ppc_q3_software_projected_line_midpoint(
    start: PpcQ3SoftwareProjectedPoint,
    end: PpcQ3SoftwareProjectedPoint,
) -> PpcQ3SoftwareProjectedPoint {
    PpcQ3SoftwareProjectedPoint {
        x: ((start.x as f32 + end.x as f32) * 0.5).round() as i32,
        y: ((start.y as f32 + end.y as f32) * 0.5).round() as i32,
        z: start.z.mul_add(0.5, end.z * 0.5),
        reciprocal_w: start.reciprocal_w.mul_add(0.5, end.reciprocal_w * 0.5),
        world: (
            start.world.0.mul_add(0.5, end.world.0 * 0.5),
            start.world.1.mul_add(0.5, end.world.1 * 0.5),
            start.world.2.mul_add(0.5, end.world.2 * 0.5),
        ),
        view_direction: match (start.view_direction, end.view_direction) {
            (Some(start_view), Some(end_view)) => ppc_q3_vector3d_normalized_value((
                start_view.0.mul_add(0.5, end_view.0 * 0.5),
                start_view.1.mul_add(0.5, end_view.1 * 0.5),
                start_view.2.mul_add(0.5, end_view.2 * 0.5),
            )),
            _ => None,
        },
        fog_depth: start.fog_depth.mul_add(0.5, end.fog_depth * 0.5),
        uv: match (start.uv, end.uv) {
            (Some(start_uv), Some(end_uv)) => Some((
                start_uv.0.mul_add(0.5, end_uv.0 * 0.5),
                start_uv.1.mul_add(0.5, end_uv.1 * 0.5),
            )),
            _ => None,
        },
        diffuse: match (start.diffuse, end.diffuse) {
            (Some(start_diffuse), Some(end_diffuse)) => Some((
                start_diffuse.0.mul_add(0.5, end_diffuse.0 * 0.5),
                start_diffuse.1.mul_add(0.5, end_diffuse.1 * 0.5),
                start_diffuse.2.mul_add(0.5, end_diffuse.2 * 0.5),
            )),
            _ => None,
        },
        ambient_coefficient: match (start.ambient_coefficient, end.ambient_coefficient) {
            (Some(start_coefficient), Some(end_coefficient)) => {
                Some(start_coefficient.mul_add(0.5, end_coefficient * 0.5))
            }
            _ => None,
        },
        normal: match (start.normal, end.normal) {
            (Some(start_normal), Some(end_normal)) => ppc_q3_vector3d_normalized_value((
                start_normal.0.mul_add(0.5, end_normal.0 * 0.5),
                start_normal.1.mul_add(0.5, end_normal.1 * 0.5),
                start_normal.2.mul_add(0.5, end_normal.2 * 0.5),
            )),
            _ => None,
        },
        specular_color: match (start.specular_color, end.specular_color) {
            (Some(start_specular), Some(end_specular)) => Some((
                start_specular.0.mul_add(0.5, end_specular.0 * 0.5),
                start_specular.1.mul_add(0.5, end_specular.1 * 0.5),
                start_specular.2.mul_add(0.5, end_specular.2 * 0.5),
            )),
            _ => None,
        },
        specular_control: match (start.specular_control, end.specular_control) {
            (Some(start_control), Some(end_control)) => {
                Some(start_control.mul_add(0.5, end_control * 0.5))
            }
            _ => None,
        },
        highlight_state: match (start.highlight_state, end.highlight_state) {
            (Some(start_state), Some(end_state)) => Some(start_state && end_state),
            _ => None,
        },
        vertex_alpha: match (start.vertex_alpha, end.vertex_alpha) {
            (Some(start_alpha), Some(end_alpha)) => Some(start_alpha.mul_add(0.5, end_alpha * 0.5)),
            _ => None,
        },
    }
}

// --- QuickDraw 3D Acceleration (RAVE) Constants ---

pub const PPC_QA_ENGINE: u32 = 0x0500_0100;
pub const PPC_QA_DRAW_CONTEXT: u32 = 0x0500_0110;
pub const PPC_QA_DRAW_CONTEXT_PRIVATE: u32 = 0x0500_0190;
pub const PPC_QA_DRAW_CONTEXT_METHOD_SLOT_COUNT: u32 = 31;
pub const PPC_QA_METHOD_RETURN_ZERO_TVECTOR: u32 = 0x0500_0200;
pub const PPC_QA_METHOD_RETURN_ZERO_ENTRY: u32 = 0x0500_0210;
pub const PPC_QA_OBJECTS_SIZE: usize = 16 * 1024;
pub const PPC_QA_OPTIONAL_TEXTURE: u32 = 1 << 1;
pub const PPC_QA_OPTIONAL_TEXTURE_COLOR: u32 = 1 << 3;
pub const PPC_QA_OPTIONAL_PERSPECTIVE_Z: u32 = 1 << 8;
pub const PPC_QA_FAST_LINE: u32 = 1 << 0;
pub const PPC_QA_FAST_GOURAUD: u32 = 1 << 1;
pub const PPC_QA_FAST_TEXTURE: u32 = 1 << 2;
pub const PPC_QA_VENDOR_APPLE: u32 = 0;
pub const PPC_QA_ENGINE_APPLE_SW: u32 = 0;
pub const PPC_QA_AVAILABLE_TEXTURE_MEMORY: u32 = 8 * 1024 * 1024;
pub const PPC_QA_ENGINE_NAME: &[u8] = b"Apple Software Renderer";

pub const PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES: u64 = 416_000;
pub const PPC_Q3_HOT_IMPORT_EXTRA_CYCLES: u64 = 1_536;
pub const PPC_Q3_RETAINED_BOUNDING_TRIMESH_PREVIEW_LIMIT: usize = 16;

// --- QuickDraw 3D Scene Graph, Objects, and 3DMF Parser ---

pub fn is_quickdraw_3d_library(library_name: &str) -> bool {
    library_name.starts_with("QuickDraw") && library_name.ends_with(" 3D")
}

pub fn is_quickdraw_3d_accelerator_library(library_name: &str) -> bool {
    library_name == "QuickDraw\u{2122} 3D Accelerator"
}

pub fn is_quickdraw_3d_status_success_import(_symbol_name: &str) -> bool {
    false
}

pub fn ppc_q3_alloc_object(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    kind: PpcQ3ObjectKind,
    object_type: u32,
    data_ptr: u32,
    data_size: u32,
) -> u32 {
    let object = *next_q3_object;
    let Some(next) = next_q3_object.checked_add(PPC_Q3_OBJECT_STRIDE) else {
        return 0;
    };
    *next_q3_object = next;
    q3_objects.push(PpcQ3ObjectRecord {
        object,
        kind,
        object_type,
        source: PpcQ3ObjectSource::default(),
        data_ptr,
        data_size,
    });
    object
}

pub fn ppc_q3_light_group_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
) -> u32 {
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_GROUP_TYPE_LIGHT,
        0,
        0,
    )
}

pub fn ppc_q3_display_group_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
) -> u32 {
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_GROUP_TYPE_DISPLAY,
        0,
        0,
    )
}

pub fn ppc_q3_ordered_display_group_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
) -> u32 {
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY,
        0,
        0,
    )
}

pub fn ppc_q3_file_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_files: &mut Vec<PpcQ3FileRecord>,
) -> u32 {
    let file = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TYPE_FILE,
        0,
        0,
    );
    if file != 0 {
        let _ = ppc_q3_file_mut(q3_files, file);
    }
    file
}

pub fn ppc_q3_view_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
) -> u32 {
    let view = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TYPE_VIEW,
        0,
        0,
    );
    if view != 0 {
        let _ = ppc_q3_view_mut(q3_views, view);
    }
    view
}

pub fn ppc_q3_object_type(cpu: &PpcCpu, q3_objects: &[PpcQ3ObjectRecord]) -> u32 {
    q3_objects
        .iter()
        .find(|record| record.object == cpu.gpr[3])
        .map(|record| record.object_type)
        .unwrap_or(PPC_Q3_TYPE_NONE)
}

pub fn ppc_q3_class_object_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    class_matches: fn(u32) -> bool,
) -> u32 {
    let object_type = ppc_q3_object_type(cpu, q3_objects);
    if class_matches(object_type) {
        object_type
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_object_type_matches(actual_type: u32, requested_type: u32) -> bool {
    if actual_type == PPC_Q3_TYPE_NONE || requested_type == PPC_Q3_TYPE_NONE {
        return false;
    }
    if actual_type == requested_type {
        return true;
    }
    match requested_type {
        PPC_Q3_OBJECT_TYPE_SHARED => ppc_q3_object_type_is_shared(actual_type),
        PPC_Q3_SHARED_TYPE_RENDERER => ppc_q3_object_type_is_renderer(actual_type),
        PPC_Q3_SHARED_TYPE_SHAPE => ppc_q3_object_type_is_shape(actual_type),
        PPC_Q3_SHAPE_TYPE_GEOMETRY => ppc_q3_object_type_is_geometry(actual_type),
        PPC_Q3_SHAPE_TYPE_SHADER => ppc_q3_object_type_is_shader(actual_type),
        PPC_Q3_SHADER_TYPE_SURFACE => ppc_q3_object_type_is_surface_shader(actual_type),
        PPC_Q3_SHADER_TYPE_ILLUMINATION => ppc_q3_object_type_is_illumination_shader(actual_type),
        PPC_Q3_SHAPE_TYPE_STYLE => ppc_q3_object_type_is_style(actual_type),
        PPC_Q3_SHAPE_TYPE_TRANSFORM => ppc_q3_object_type_is_transform(actual_type),
        PPC_Q3_SHAPE_TYPE_LIGHT => ppc_q3_object_type_is_light(actual_type),
        PPC_Q3_SHAPE_TYPE_CAMERA => ppc_q3_object_type_is_camera(actual_type),
        PPC_Q3_SHAPE_TYPE_GROUP => ppc_q3_object_type_is_group(actual_type),
        PPC_Q3_GROUP_TYPE_DISPLAY => ppc_q3_object_type_is_display_group(actual_type),
        PPC_Q3_SHARED_TYPE_SET => ppc_q3_object_type_is_set(actual_type),
        PPC_Q3_SHARED_TYPE_DRAW_CONTEXT => ppc_q3_object_type_is_draw_context(actual_type),
        PPC_Q3_SHARED_TYPE_TEXTURE => ppc_q3_object_type_is_texture(actual_type),
        PPC_Q3_SHARED_TYPE_STORAGE => ppc_q3_object_type_is_storage(actual_type),
        PPC_Q3_STORAGE_TYPE_MEMORY => {
            actual_type == PPC_Q3_STORAGE_TYPE_MEMORY
                || actual_type == PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE
        }
        _ => false,
    }
}

pub fn ppc_q3_object_type_is_shared(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_SHARED_TYPE_RENDERER
            | PPC_Q3_SHARED_TYPE_SHAPE
            | PPC_Q3_SHARED_TYPE_SET
            | PPC_Q3_SHARED_TYPE_DRAW_CONTEXT
            | PPC_Q3_SHARED_TYPE_TEXTURE
            | PPC_Q3_TYPE_FILE
            | PPC_Q3_SHARED_TYPE_STORAGE
    ) || ppc_q3_object_type_is_renderer(object_type)
        || ppc_q3_object_type_is_shape(object_type)
        || ppc_q3_object_type_is_set(object_type)
        || ppc_q3_object_type_is_draw_context(object_type)
        || ppc_q3_object_type_is_texture(object_type)
        || ppc_q3_object_type_is_storage(object_type)
}

pub fn ppc_q3_shared_type_for_object_type(object_type: u32) -> u32 {
    if ppc_q3_object_type_is_renderer(object_type) {
        PPC_Q3_SHARED_TYPE_RENDERER
    } else if ppc_q3_object_type_is_shape(object_type) {
        PPC_Q3_SHARED_TYPE_SHAPE
    } else if ppc_q3_object_type_is_set(object_type) {
        PPC_Q3_SHARED_TYPE_SET
    } else if ppc_q3_object_type_is_draw_context(object_type) {
        PPC_Q3_SHARED_TYPE_DRAW_CONTEXT
    } else if ppc_q3_object_type_is_texture(object_type) {
        PPC_Q3_SHARED_TYPE_TEXTURE
    } else if object_type == PPC_Q3_TYPE_FILE {
        PPC_Q3_TYPE_FILE
    } else if ppc_q3_object_type_is_storage(object_type) {
        PPC_Q3_SHARED_TYPE_STORAGE
    } else {
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_shape_type_for_object_type(object_type: u32) -> u32 {
    match object_type {
        PPC_Q3_SHAPE_TYPE_GEOMETRY
        | PPC_Q3_SHAPE_TYPE_SHADER
        | PPC_Q3_SHAPE_TYPE_STYLE
        | PPC_Q3_SHAPE_TYPE_TRANSFORM
        | PPC_Q3_SHAPE_TYPE_LIGHT
        | PPC_Q3_SHAPE_TYPE_CAMERA
        | PPC_Q3_SHAPE_TYPE_GROUP => object_type,
        _ if ppc_q3_object_type_is_geometry(object_type) => PPC_Q3_SHAPE_TYPE_GEOMETRY,
        _ if ppc_q3_object_type_is_shader(object_type) => PPC_Q3_SHAPE_TYPE_SHADER,
        _ if ppc_q3_object_type_is_style(object_type) => PPC_Q3_SHAPE_TYPE_STYLE,
        _ if ppc_q3_object_type_is_transform(object_type) => PPC_Q3_SHAPE_TYPE_TRANSFORM,
        _ if ppc_q3_object_type_is_light(object_type) => PPC_Q3_SHAPE_TYPE_LIGHT,
        _ if ppc_q3_object_type_is_camera(object_type) => PPC_Q3_SHAPE_TYPE_CAMERA,
        _ if ppc_q3_object_type_is_group(object_type) => PPC_Q3_SHAPE_TYPE_GROUP,
        _ => PPC_Q3_TYPE_NONE,
    }
}

pub fn ppc_q3_object_type_is_renderer(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_RENDERER_TYPE_WIREFRAME
            | PPC_Q3_RENDERER_TYPE_GENERIC
            | PPC_Q3_RENDERER_TYPE_INTERACTIVE
            | PPC_Q3_RENDERER_TYPE_INTERACTIVE_QUESA
            | PPC_Q3_RENDERER_TYPE_OPENGL
            | PPC_Q3_RENDERER_TYPE_CARTOON
            | PPC_Q3_RENDERER_TYPE_HIDDEN_LINE
    )
}

pub fn ppc_q3_object_type_is_shape(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_SHAPE_TYPE_GEOMETRY
            | PPC_Q3_SHAPE_TYPE_SHADER
            | PPC_Q3_SHADER_TYPE_SURFACE
            | PPC_Q3_SHADER_TYPE_ILLUMINATION
            | PPC_Q3_SHAPE_TYPE_STYLE
            | PPC_Q3_SHAPE_TYPE_TRANSFORM
            | PPC_Q3_SHAPE_TYPE_LIGHT
            | PPC_Q3_SHAPE_TYPE_CAMERA
            | PPC_Q3_SHAPE_TYPE_GROUP
    ) || ppc_q3_object_type_is_geometry(object_type)
        || ppc_q3_object_type_is_shader(object_type)
        || ppc_q3_object_type_is_style(object_type)
        || ppc_q3_object_type_is_transform(object_type)
        || ppc_q3_object_type_is_light(object_type)
        || ppc_q3_object_type_is_camera(object_type)
        || ppc_q3_object_type_is_group(object_type)
}

pub fn ppc_q3_object_type_is_geometry(object_type: u32) -> bool {
    object_type == PPC_Q3_TYPE_TRIMESH
}

pub fn ppc_q3_object_type_is_shader(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_SHADER_TYPE_SURFACE | PPC_Q3_SHADER_TYPE_ILLUMINATION
    ) || ppc_q3_object_type_is_surface_shader(object_type)
        || ppc_q3_object_type_is_illumination_shader(object_type)
}

pub fn ppc_q3_object_type_is_surface_shader(object_type: u32) -> bool {
    object_type == PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE
}

pub fn ppc_q3_object_type_is_illumination_shader(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_ILLUMINATION_TYPE_PHONG
            | PPC_Q3_ILLUMINATION_TYPE_LAMBERT
            | PPC_Q3_ILLUMINATION_TYPE_NULL
    )
}

pub fn ppc_q3_object_type_is_style(object_type: u32) -> bool {
    ppc_q3_style_kind_for_type(object_type).is_some()
}

pub fn ppc_q3_object_type_is_transform(object_type: u32) -> bool {
    object_type == PPC_Q3_TRANSFORM_TYPE_MATRIX
}

pub fn ppc_q3_object_type_is_light(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_LIGHT_TYPE_AMBIENT
            | PPC_Q3_LIGHT_TYPE_DIRECTIONAL
            | PPC_Q3_LIGHT_TYPE_POINT
            | PPC_Q3_LIGHT_TYPE_SPOT
    )
}

pub fn ppc_q3_object_type_is_camera(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT
            | PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC
            | PPC_Q3_CAMERA_TYPE_VIEW_PLANE
    )
}

pub fn ppc_q3_object_type_is_group(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_GROUP_TYPE_DISPLAY
            | PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY
            | PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY
            | PPC_Q3_GROUP_TYPE_LIGHT
    )
}

pub fn ppc_q3_object_type_is_display_group(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_GROUP_TYPE_DISPLAY
            | PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY
            | PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY
    )
}

pub fn ppc_q3_object_type_is_set(object_type: u32) -> bool {
    object_type == PPC_Q3_TYPE_ATTRIBUTE_SET
}

pub fn ppc_q3_object_type_is_draw_context(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP | PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH
    )
}

pub fn ppc_q3_object_type_is_texture(object_type: u32) -> bool {
    object_type == PPC_Q3_TEXTURE_TYPE_MIPMAP
}

pub fn ppc_q3_object_type_is_storage(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_STORAGE_TYPE_MEMORY
            | PPC_Q3_STORAGE_TYPE_MACINTOSH
            | PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE
    )
}

pub fn ppc_q3_object_type_is_drawable(object_type: u32) -> bool {
    object_type == PPC_Q3_TYPE_CONTAINER
        || ppc_q3_object_type_is_geometry(object_type)
        || ppc_q3_object_type_is_shader(object_type)
        || ppc_q3_object_type_is_style(object_type)
        || ppc_q3_object_type_is_transform(object_type)
        || ppc_q3_object_type_is_display_group(object_type)
        || ppc_q3_object_type_is_set(object_type)
}

pub fn ppc_q3_object_is_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let object = cpu.gpr[3];
    let requested_type = cpu.gpr[4];
    let Some(actual_type) = ppc_q3_known_object_type_for_handle(q3_objects, object) else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    };
    ppc_q3_object_type_matches(actual_type, requested_type)
}

pub fn ppc_q3_object_is_drawable(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let object = cpu.gpr[3];
    let Some(object_type) = ppc_q3_known_object_type_for_handle(q3_objects, object) else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    };
    ppc_q3_object_type_is_drawable(object_type)
}

pub fn ppc_q3_renderer_new_from_type(
    cpu: &PpcCpu,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    next_q3_object: &mut u32,
) -> u32 {
    let renderer_type = cpu.gpr[3];
    if !ppc_q3_object_type_is_renderer(renderer_type) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        renderer_type,
        0,
        0,
    )
}

pub fn ppc_q3_renderer_get_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    let renderer = cpu.gpr[3];
    let object_type = ppc_q3_object_type_for_handle(q3_objects, renderer);
    if ppc_q3_object_type_is_renderer(object_type) {
        object_type
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_renderer_is_valid(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, cpu.gpr[3])
}

pub fn ppc_q3_interactive_renderer_set_double_buffer_bypass(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
) -> bool {
    let renderer = cpu.gpr[3];
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    ppc_q3_renderer_preference_record_mut(q3_renderer_preferences, renderer).double_buffer_bypass =
        Some(cpu.gpr[4]);
    true
}

pub fn ppc_q3_interactive_renderer_set_preferences(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
) -> bool {
    let renderer = cpu.gpr[3];
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    let record = ppc_q3_renderer_preference_record_mut(q3_renderer_preferences, renderer);
    record.preference_vendor = Some(cpu.gpr[4]);
    record.preference_engine = Some(cpu.gpr[5]);
    true
}

pub fn ppc_q3_interactive_renderer_set_rave_context_hints(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
) -> bool {
    let renderer = cpu.gpr[3];
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    ppc_q3_renderer_preference_record_mut(q3_renderer_preferences, renderer).rave_context_hints =
        Some(cpu.gpr[4]);
    true
}

pub fn ppc_q3_interactive_renderer_get_rave_context_hints(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_renderer_preferences: &[PpcQ3RendererPreferenceRecord],
) -> bool {
    let renderer = cpu.gpr[3];
    let hints_out_ptr = cpu.gpr[4];
    if hints_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, hints_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    let hints = q3_renderer_preferences
        .iter()
        .find(|record| record.renderer == renderer)
        .and_then(|record| record.rave_context_hints)
        .unwrap_or(0);
    memory.write_u32_be(hints_out_ptr, hints).is_some()
}

pub fn ppc_seed_qa_rave_objects(memory: &mut PpcSectionMem) {
    const ADDI_R3_ZERO_ZERO: u32 = 0x3860_0000;

    let _ = memory.write_u32_be(
        PPC_QA_METHOD_RETURN_ZERO_TVECTOR,
        PPC_QA_METHOD_RETURN_ZERO_ENTRY,
    );
    let _ = memory.write_u32_be(PPC_QA_METHOD_RETURN_ZERO_TVECTOR + 4, 0);
    let _ = memory.write_u32_be(PPC_QA_METHOD_RETURN_ZERO_ENTRY, ADDI_R3_ZERO_ZERO);
    let _ = memory.write_u32_be(PPC_QA_METHOD_RETURN_ZERO_ENTRY + 4, BLR);

    let _ = memory.write_u32_be(PPC_QA_DRAW_CONTEXT, PPC_QA_DRAW_CONTEXT_PRIVATE);
    for slot in 0..PPC_QA_DRAW_CONTEXT_METHOD_SLOT_COUNT {
        let _ = memory.write_u32_be(
            PPC_QA_DRAW_CONTEXT + 4 + slot * 4,
            PPC_QA_METHOD_RETURN_ZERO_TVECTOR,
        );
    }
}

pub fn ppc_q3_interactive_renderer_get_rave_draw_contexts(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let renderer = cpu.gpr[3];
    let draw_contexts_out_ptr = cpu.gpr[4];
    let engines_out_ptr = cpu.gpr[5];
    let count_out_ptr = cpu.gpr[6];
    if draw_contexts_out_ptr == 0
        || engines_out_ptr == 0
        || count_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, draw_contexts_out_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, engines_out_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, count_out_ptr, 4)
    {
        return false;
    }
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    memory
        .write_u32_be(draw_contexts_out_ptr, PPC_QA_DRAW_CONTEXT)
        .is_some()
        && memory
            .write_u32_be(engines_out_ptr, PPC_QA_ENGINE)
            .is_some()
        && memory.write_u32_be(count_out_ptr, 1).is_some()
}

pub fn ppc_q3_interactive_renderer_set_rave_texture_filter(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
) -> bool {
    let renderer = cpu.gpr[3];
    if !ppc_q3_validate_renderer_handle(q3_objects, q3_error_state, renderer) {
        return false;
    }
    ppc_q3_renderer_preference_record_mut(q3_renderer_preferences, renderer).rave_texture_filter =
        Some(cpu.gpr[4]);
    true
}

pub fn ppc_q3_renderer_preference_record_mut(
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    renderer: u32,
) -> &mut PpcQ3RendererPreferenceRecord {
    if let Some(index) = q3_renderer_preferences
        .iter()
        .position(|record| record.renderer == renderer)
    {
        return q3_renderer_preferences
            .get_mut(index)
            .expect("Q3 renderer preference index should be valid");
    }
    q3_renderer_preferences.push(PpcQ3RendererPreferenceRecord::new(renderer));
    q3_renderer_preferences
        .last_mut()
        .expect("just pushed Q3 renderer preference record")
}

pub fn ppc_q3_validate_renderer_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    renderer: u32,
) -> bool {
    if ppc_q3_object_type_is_renderer(ppc_q3_object_type_for_handle(q3_objects, renderer)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_mipmap_texture_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    next_q3_object: &mut u32,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
) -> u32 {
    let mipmap_ptr = cpu.gpr[3];
    if mipmap_ptr == 0 {
        return 0;
    }
    let Some(mipmap) = ppc_q3_read_bytes(memory, mipmap_ptr, PPC_Q3_MIPMAP_COPY_SIZE) else {
        return 0;
    };
    let image_storage = ppc_q3_mipmap_texture_image_storage(&mipmap);
    if !ppc_q3_validate_mipmap_image_storage(q3_objects, q3_error_state, image_storage) {
        return 0;
    }
    let texture = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TEXTURE_TYPE_MIPMAP,
        0,
        PPC_Q3_MIPMAP_COPY_SIZE,
    );
    if texture != 0 {
        ppc_q3_mipmap_texture_store_data(q3_mipmap_textures, texture, mipmap);
        ppc_q3_retain_mipmap_image_storage(q3_objects, q3_object_refs, image_storage);
    }
    texture
}

pub fn ppc_q3_mipmap_texture_image_storage(mipmap: &[u8]) -> u32 {
    ppc_q3_read_u32_from_slice(mipmap, PPC_Q3_MIPMAP_IMAGE_OFFSET).unwrap_or(0)
}

pub fn ppc_q3_mipmap_texture_store_data(
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    texture: u32,
    mipmap: Vec<u8>,
) {
    if let Some(record) = q3_mipmap_textures
        .iter_mut()
        .find(|record| record.texture == texture)
    {
        record.mipmap = mipmap;
    } else {
        q3_mipmap_textures.push(PpcQ3MipmapTextureRecord { texture, mipmap });
    }
}

pub fn ppc_q3_mipmap_texture_get_mipmap(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
) -> bool {
    let texture = cpu.gpr[3];
    let mipmap_out_ptr = cpu.gpr[4];
    if mipmap_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, mipmap_out_ptr, PPC_Q3_MIPMAP_COPY_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_texture_handle(q3_objects, q3_error_state, texture) {
        return false;
    }
    if let Some(record) = q3_mipmap_textures
        .iter()
        .find(|record| record.texture == texture)
    {
        let image_storage = ppc_q3_mipmap_texture_image_storage(&record.mipmap);
        if !ppc_q3_validate_mipmap_image_storage(q3_objects, q3_error_state, image_storage) {
            return false;
        }
        if !ppc_q3_write_bytes(memory, mipmap_out_ptr, &record.mipmap) {
            return false;
        }
        ppc_q3_retain_mipmap_image_storage(q3_objects, q3_object_refs, image_storage);
        return true;
    }
    let mipmap_ptr = q3_objects
        .iter()
        .find(|record| record.object == texture)
        .map(|record| record.data_ptr)
        .unwrap_or(0);
    let image_storage = if mipmap_ptr == 0 {
        0
    } else {
        memory
            .read_u32_be(mipmap_ptr + PPC_Q3_MIPMAP_IMAGE_OFFSET)
            .unwrap_or(0)
    };
    if !ppc_q3_validate_mipmap_image_storage(q3_objects, q3_error_state, image_storage) {
        return false;
    }
    if !ppc_q3_copy_or_zero(memory, mipmap_ptr, mipmap_out_ptr, PPC_Q3_MIPMAP_COPY_SIZE) {
        return false;
    }
    ppc_q3_retain_mipmap_image_storage(q3_objects, q3_object_refs, image_storage);
    true
}

pub fn ppc_q3_validate_mipmap_image_storage(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    image_storage: u32,
) -> bool {
    if image_storage == 0 {
        return true;
    }
    match ppc_q3_known_object_type_for_handle(q3_objects, image_storage) {
        Some(object_type) if ppc_q3_object_type_is_storage(object_type) => true,
        Some(_) => {
            q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
            false
        }
        None => true,
    }
}

pub fn ppc_q3_retain_mipmap_image_storage(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    image_storage: u32,
) {
    if image_storage == 0 {
        return;
    }
    if ppc_q3_known_object_type_for_handle(q3_objects, image_storage)
        .is_some_and(ppc_q3_object_type_is_storage)
    {
        let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, image_storage);
    }
}

pub fn ppc_q3_mipmap_image_storage_is_known_storage(
    q3_objects: &[PpcQ3ObjectRecord],
    image_storage: u32,
) -> bool {
    ppc_q3_known_object_type_for_handle(q3_objects, image_storage)
        .is_some_and(ppc_q3_object_type_is_storage)
}

pub fn ppc_q3_validate_texture_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    texture: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, texture) == PPC_Q3_TEXTURE_TYPE_MIPMAP {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_texture_shader_new(
    cpu: &PpcCpu,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    next_q3_object: &mut u32,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
) -> u32 {
    let texture = cpu.gpr[3];
    if !ppc_q3_validate_texture_shader_texture(q3_objects, q3_error_state, texture) {
        return 0;
    }
    let shader = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        texture,
        0,
    );
    if shader != 0 {
        ppc_q3_texture_shader_store_link(q3_texture_shaders, shader, texture);
        let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, texture);
    }
    shader
}

pub fn ppc_q3_illumination_shader_new(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    illumination_type: u32,
) -> u32 {
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        illumination_type,
        0,
        0,
    )
}

pub fn ppc_q3_texture_shader_store_link(
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    shader: u32,
    texture: u32,
) {
    if let Some(record) = q3_texture_shaders
        .iter_mut()
        .find(|record| record.shader == shader)
    {
        record.texture = texture;
    } else {
        q3_texture_shaders.push(PpcQ3TextureShaderRecord { shader, texture });
    }
}

pub fn ppc_q3_texture_shader_get_texture(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
) -> bool {
    let shader = cpu.gpr[3];
    let texture_out_ptr = cpu.gpr[4];
    if texture_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, texture_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_texture_shader_handle(q3_objects, q3_error_state, shader) {
        return false;
    }
    let texture = q3_texture_shaders
        .iter()
        .find(|record| record.shader == shader)
        .map(|record| record.texture)
        .or_else(|| {
            q3_objects
                .iter()
                .find(|record| record.object == shader)
                .map(|record| record.data_ptr)
        })
        .unwrap_or(0);
    if !ppc_q3_validate_texture_shader_texture(q3_objects, q3_error_state, texture) {
        return false;
    }
    if memory.write_u32_be(texture_out_ptr, texture).is_none() {
        return false;
    }
    if texture != 0 {
        let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, texture);
    }
    true
}

pub fn ppc_q3_validate_texture_shader_texture(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    texture: u32,
) -> bool {
    match ppc_q3_known_object_type_for_handle(q3_objects, texture) {
        Some(object_type) if !ppc_q3_object_type_is_texture(object_type) => {
            q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
            false
        }
        _ => true,
    }
}

pub fn ppc_q3_validate_texture_shader_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    shader: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, shader) == PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_copy_or_zero(
    memory: &mut PpcSectionMem,
    source_ptr: u32,
    dest_ptr: u32,
    byte_count: u32,
) -> bool {
    if dest_ptr == 0 {
        return false;
    }
    for offset in 0..byte_count {
        let byte = if source_ptr == 0 {
            0
        } else {
            memory.read_u8(source_ptr + offset).unwrap_or(0)
        };
        if memory.write_u8(dest_ptr + offset, byte).is_none() {
            return false;
        }
    }
    true
}

pub fn ppc_q3_draw_context_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    draw_context_type: u32,
    data_size: u32,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some(data) = ppc_q3_read_bytes(memory, data_ptr, data_size) else {
        return 0;
    };
    let draw_context = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        draw_context_type,
        0,
        data_size,
    );
    if draw_context != 0 {
        q3_draw_contexts.push(PpcQ3DrawContextRecord {
            draw_context,
            draw_context_type,
            data,
        });
    }
    draw_context
}

pub fn ppc_q3_draw_context_get_pane(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
) -> bool {
    let draw_context = cpu.gpr[3];
    let pane_out_ptr = cpu.gpr[4];
    if draw_context == 0
        || pane_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, pane_out_ptr, PPC_Q3_DRAW_CONTEXT_PANE_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_draw_context_handle(q3_objects, q3_error_state, draw_context) {
        return false;
    }
    let Some(object) = q3_objects
        .iter()
        .find(|record| record.object == draw_context)
    else {
        return false;
    };
    let Some(record) = q3_draw_contexts
        .iter()
        .find(|record| record.draw_context == draw_context)
    else {
        return false;
    };
    if object.object_type != record.draw_context_type
        || record.data.len() < PPC_Q3_DRAW_CONTEXT_DATA_SIZE as usize
    {
        return false;
    }
    let pane_state =
        ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET)
            .unwrap_or(0);
    if pane_state != 0 {
        let pane_start = PPC_Q3_DRAW_CONTEXT_PANE_OFFSET as usize;
        let pane_end = pane_start + PPC_Q3_DRAW_CONTEXT_PANE_SIZE as usize;
        return ppc_q3_write_bytes(memory, pane_out_ptr, &record.data[pane_start..pane_end]);
    }
    match record.draw_context_type {
        PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP => {
            let width =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET);
            let height =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_PIXMAP_DRAW_CONTEXT_HEIGHT_OFFSET);
            let Some((width, height)) = width.zip(height) else {
                return false;
            };
            ppc_write_q3_area(memory, pane_out_ptr, 0.0, 0.0, width as f32, height as f32).is_some()
        }
        PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH => {
            let port =
                ppc_q3_read_u32_from_slice(&record.data, PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET)
                    .unwrap_or(0);
            let (width, height) = gworlds
                .iter()
                .find(|gworld| gworld.port == port)
                .map(|gworld| (gworld.width, gworld.height))
                .unwrap_or((ppc_main_screen_width(), ppc_main_screen_height()));
            ppc_write_q3_area(memory, pane_out_ptr, 0.0, 0.0, width as f32, height as f32).is_some()
        }
        _ => false,
    }
}

pub fn ppc_q3_validate_draw_context_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    draw_context: u32,
) -> bool {
    if ppc_q3_object_type_is_draw_context(ppc_q3_object_type_for_handle(q3_objects, draw_context)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub(crate) fn ppc_q3_trimesh_new(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    next_q3_object: &mut u32,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some(data) = ppc_q3_read_bytes(memory, data_ptr, PPC_Q3_TRIMESH_DATA_SIZE) else {
        return 0;
    };
    let Some(data) = ppc_q3_trimesh_copy_get_data_arrays(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &data,
    ) else {
        return 0;
    };
    let trimesh = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TYPE_TRIMESH,
        0,
        PPC_Q3_TRIMESH_DATA_SIZE,
    );
    if trimesh != 0 {
        let triangle_attribute_sets = ppc_q3_trimesh_retain_triangle_attribute_sets(
            memory,
            q3_objects,
            q3_object_refs,
            &data,
        );
        ppc_q3_trimesh_store_data(q3_trimeshes, trimesh, data, triangle_attribute_sets);
    }
    trimesh
}

pub(crate) fn ppc_q3_trimesh_get_data(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_trimeshes: &mut [PpcQ3TriMeshRecord],
) -> bool {
    let trimesh = cpu.gpr[3];
    let data_out_ptr = cpu.gpr[4];
    if trimesh == 0
        || data_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, data_out_ptr, PPC_Q3_TRIMESH_DATA_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_trimesh_handle(q3_objects, q3_error_state, trimesh) {
        return false;
    }
    let Some(record) = q3_objects.iter().find(|record| record.object == trimesh) else {
        return false;
    };
    if let Some(record_index) = q3_trimeshes
        .iter()
        .position(|record| record.trimesh == trimesh)
    {
        let copy_size = PPC_Q3_TRIMESH_DATA_SIZE as usize;
        if q3_trimeshes[record_index].data.len() < copy_size {
            return false;
        }
        let source_data = q3_trimeshes[record_index].data[..copy_size].to_vec();
        ppc_q3_trace_trimesh_header(
            "GetData stored copy-arrays source",
            cpu,
            trimesh,
            &source_data,
        );
        // Quesa's Q3TriMesh_GetData implementation copies every TriMesh array,
        // and Q3TriMesh_EmptyData releases those allocations.  Reusing the
        // private allocation set associated with the same output structure
        // models a normal allocator returning recently freed blocks without
        // making API semantics depend on an application's return address.
        // Source: Quesa E3GeometryTriMesh.cpp, e3geom_nakedtrimesh_copydata and
        // E3TriMesh_GetData (https://github.com/jwwalker/Quesa).
        let cached_copy = q3_trimeshes[record_index]
            .get_data_copies
            .iter()
            .find(|copy| copy.data_out_ptr == data_out_ptr)
            .cloned();
        let data = cached_copy
            .and_then(|copy| {
                ppc_q3_trimesh_refresh_get_data_arrays(memory, &source_data, &copy.data)
            })
            .or_else(|| {
                ppc_q3_trimesh_copy_get_data_arrays(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    &source_data,
                )
            });
        let Some(data) = data else {
            return false;
        };
        if let Some(copy) = q3_trimeshes[record_index]
            .get_data_copies
            .iter_mut()
            .find(|copy| copy.data_out_ptr == data_out_ptr)
        {
            copy.data.clone_from(&data);
        } else {
            q3_trimeshes[record_index]
                .get_data_copies
                .push(PpcQ3TriMeshGetDataCopyRecord {
                    data_out_ptr,
                    data: data.clone(),
                });
        }
        ppc_q3_trace_trimesh_header("GetData stored copy-arrays result", cpu, trimesh, &data);
        return ppc_q3_write_trimesh_get_data(
            memory,
            data_out_ptr,
            &data,
            q3_objects,
            q3_object_refs,
            q3_attributes,
            q3_texture_shaders,
        );
    }
    if record.data_ptr == 0 || record.data_size < PPC_Q3_TRIMESH_DATA_SIZE {
        return false;
    }
    let Some(data) = ppc_q3_read_bytes(memory, record.data_ptr, PPC_Q3_TRIMESH_DATA_SIZE) else {
        return false;
    };
    ppc_q3_trace_trimesh_header("GetData object copy-arrays source", cpu, trimesh, &data);
    let Some(data) = ppc_q3_trimesh_copy_get_data_arrays(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &data,
    ) else {
        return false;
    };
    ppc_q3_trace_trimesh_header("GetData object copy-arrays result", cpu, trimesh, &data);
    ppc_q3_write_trimesh_get_data(
        memory,
        data_out_ptr,
        &data,
        q3_objects,
        q3_object_refs,
        q3_attributes,
        q3_texture_shaders,
    )
}

pub fn ppc_q3_write_trimesh_get_data(
    memory: &mut PpcSectionMem,
    data_out_ptr: u32,
    data: &[u8],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
) -> bool {
    if !ppc_q3_write_bytes(memory, data_out_ptr, data) {
        return false;
    }
    if let Some(attribute_set) =
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET)
            .filter(|attribute_set| *attribute_set != 0)
    {
        let mut copied_attributes = ppc_q3_attributes_for_set(q3_attributes, attribute_set);
        let retained_shaders: Vec<u32> = copied_attributes
            .iter()
            .filter_map(ppc_q3_attribute_surface_shader_handle)
            .collect();
        q3_attributes.retain(|record| record.attribute_set != data_out_ptr);
        for attribute in &mut copied_attributes {
            attribute.attribute_set = data_out_ptr;
        }
        q3_attributes.extend(copied_attributes);
        for shader in retained_shaders {
            let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, shader);
            if let Some(texture) = q3_texture_shaders
                .iter()
                .find(|record| record.shader == shader)
                .map(|record| record.texture)
            {
                let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, texture);
            }
        }
    }
    let _ = ppc_q3_trimesh_retain_triangle_attribute_sets(memory, q3_objects, q3_object_refs, data);
    true
}

pub fn ppc_q3_trimesh_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> bool {
    let trimesh = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if trimesh == 0 || data_ptr == 0 {
        return false;
    }
    let Some(data) = ppc_q3_read_bytes(memory, data_ptr, PPC_Q3_TRIMESH_DATA_SIZE) else {
        return false;
    };
    ppc_q3_trace_trimesh_header("SetData input", cpu, trimesh, &data);
    if !ppc_q3_validate_trimesh_handle(q3_objects, q3_error_state, trimesh) {
        return false;
    }
    let Some(record) = q3_objects
        .iter_mut()
        .find(|record| record.object == trimesh)
    else {
        return false;
    };
    record.object_type = PPC_Q3_TYPE_TRIMESH;
    record.data_ptr = 0;
    record.data_size = PPC_Q3_TRIMESH_DATA_SIZE;
    let old_triangle_attribute_sets = q3_trimeshes
        .iter()
        .find(|record| record.trimesh == trimesh)
        .map(|record| record.triangle_attribute_sets.clone())
        .unwrap_or_default();
    let triangle_attribute_sets =
        ppc_q3_trimesh_retain_triangle_attribute_sets(memory, q3_objects, q3_object_refs, &data);
    {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        for attribute_set in old_triangle_attribute_sets {
            let _ = ppc_q3_object_release_reference(&mut stores, attribute_set);
        }
    }
    ppc_q3_trimesh_store_data(q3_trimeshes, trimesh, data, triangle_attribute_sets);
    true
}

pub fn ppc_q3_validate_trimesh_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    trimesh: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, trimesh) == PPC_Q3_TYPE_TRIMESH {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_trimesh_empty_data(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> bool {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return false;
    }
    for offset in [
        PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_POINTS_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
    ] {
        if memory.write_u32_be(data_ptr + offset, 0).is_none() {
            return false;
        }
    }
    true
}

pub(crate) fn ppc_q3_trimesh_copy_get_data_arrays(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    data: &[u8],
) -> Option<Vec<u8>> {
    let mut data = data.to_vec();
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_DATA_SIZE,
        PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES,
    )?;
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE,
        PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES,
    )?;
    ppc_q3_trimesh_copy_get_data_attribute_payloads(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES,
    )?;
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_EDGES_OFFSET,
        PPC_Q3_TRIMESH_EDGES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_DATA_SIZE,
        PPC_Q3_SOFTWARE_RENDER_MAX_EDGES,
    )?;
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE,
        PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES,
    )?;
    ppc_q3_trimesh_copy_get_data_attribute_payloads(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_EDGES_OFFSET,
        PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_SOFTWARE_RENDER_MAX_EDGES,
    )?;
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
        PPC_Q3_TRIMESH_POINTS_OFFSET,
        12,
        PPC_Q3_SOFTWARE_RENDER_MAX_POINTS,
    )?;
    ppc_q3_trimesh_copy_get_data_array(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE,
        PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES,
    )?;
    ppc_q3_trimesh_copy_get_data_attribute_payloads(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        &mut data,
        PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
        PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_SOFTWARE_RENDER_MAX_POINTS,
    )?;
    *last_mem_error = PPC_NO_ERR;
    Some(data)
}

pub fn ppc_q3_trimesh_refresh_get_data_arrays(
    memory: &mut PpcSectionMem,
    source: &[u8],
    cached: &[u8],
) -> Option<Vec<u8>> {
    if source.len() < PPC_Q3_TRIMESH_DATA_SIZE as usize
        || cached.len() < PPC_Q3_TRIMESH_DATA_SIZE as usize
    {
        return None;
    }
    let mut refreshed = source.to_vec();
    for (count_offset, ptr_offset, element_size, max_count) in [
        (
            PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET,
            PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            PPC_Q3_TRIMESH_TRIANGLE_DATA_SIZE,
            PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES,
        ),
        (
            PPC_Q3_TRIMESH_NUM_EDGES_OFFSET,
            PPC_Q3_TRIMESH_EDGES_OFFSET,
            PPC_Q3_TRIMESH_EDGE_DATA_SIZE,
            PPC_Q3_SOFTWARE_RENDER_MAX_EDGES,
        ),
        (
            PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
            PPC_Q3_TRIMESH_POINTS_OFFSET,
            PPC_Q3_POINT3D_SIZE,
            PPC_Q3_SOFTWARE_RENDER_MAX_POINTS,
        ),
    ] {
        let count = ppc_q3_trimesh_header_u32(source, count_offset)?;
        if count != ppc_q3_trimesh_header_u32(cached, count_offset)? || count > max_count {
            return None;
        }
        let source_ptr = ppc_q3_trimesh_header_u32(source, ptr_offset)?;
        let cached_ptr = ppc_q3_trimesh_header_u32(cached, ptr_offset)?;
        if count != 0 {
            let byte_count = count.checked_mul(element_size)?;
            if source_ptr == 0
                || cached_ptr == 0
                || !ppc_q3_copy_or_zero(memory, source_ptr, cached_ptr, byte_count)
            {
                return None;
            }
        }
        ppc_q3_write_u32_to_slice(&mut refreshed, ptr_offset, cached_ptr)?;
    }
    for (element_count_offset, attr_count_offset, attr_ptr_offset, max_element_count) in [
        (
            PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES,
        ),
        (
            PPC_Q3_TRIMESH_NUM_EDGES_OFFSET,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_SOFTWARE_RENDER_MAX_EDGES,
        ),
        (
            PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            PPC_Q3_SOFTWARE_RENDER_MAX_POINTS,
        ),
    ] {
        let element_count = ppc_q3_trimesh_header_u32(source, element_count_offset)?;
        let attr_count = ppc_q3_trimesh_header_u32(source, attr_count_offset)?;
        if element_count != ppc_q3_trimesh_header_u32(cached, element_count_offset)?
            || attr_count != ppc_q3_trimesh_header_u32(cached, attr_count_offset)?
            || element_count > max_element_count
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
        {
            return None;
        }
        let source_table = ppc_q3_trimesh_header_u32(source, attr_ptr_offset)?;
        let cached_table = ppc_q3_trimesh_header_u32(cached, attr_ptr_offset)?;
        if attr_count != 0 && (source_table == 0 || cached_table == 0) {
            return None;
        }
        for attr_index in 0..attr_count {
            let record_offset = attr_index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?;
            let source_record = source_table.checked_add(record_offset)?;
            let cached_record = cached_table.checked_add(record_offset)?;
            let attr_type = memory.read_u32_be(source_record)?;
            if attr_type != memory.read_u32_be(cached_record)? {
                return None;
            }
            let attr_size = ppc_q3_attribute_data_size(attr_type)?;
            let source_data_ptr = memory.read_u32_be(source_record.checked_add(4)?)?;
            let cached_data_ptr = memory.read_u32_be(cached_record.checked_add(4)?)?;
            if element_count != 0 {
                let byte_count = element_count.checked_mul(attr_size)?;
                if source_data_ptr == 0
                    || cached_data_ptr == 0
                    || !ppc_q3_copy_or_zero(memory, source_data_ptr, cached_data_ptr, byte_count)
                {
                    return None;
                }
            }
            let source_use_ptr = memory.read_u32_be(source_record.checked_add(8)?)?;
            let cached_use_ptr = memory.read_u32_be(cached_record.checked_add(8)?)?;
            if source_use_ptr != 0
                && (cached_use_ptr == 0
                    || !ppc_q3_copy_or_zero(memory, source_use_ptr, cached_use_ptr, element_count))
            {
                return None;
            }
            memory.write_u32_be(cached_record, attr_type)?;
        }
        ppc_q3_write_u32_to_slice(&mut refreshed, attr_ptr_offset, cached_table)?;
    }
    Some(refreshed)
}

pub(crate) fn ppc_q3_trimesh_copy_get_data_array(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    data: &mut [u8],
    count_offset: u32,
    ptr_offset: u32,
    element_size: u32,
    max_count: u32,
) -> Option<()> {
    let count = ppc_q3_trimesh_header_u32(data, count_offset)?;
    let source_ptr = ppc_q3_trimesh_header_u32(data, ptr_offset)?;
    if count == 0 || count > max_count || source_ptr == 0 {
        return Some(());
    }
    let byte_count = count.checked_mul(element_size)?;
    if !ppc_memory_can_read_bytes(memory, source_ptr, byte_count) {
        return Some(());
    }
    let dest_ptr = ppc_process_heap_alloc(
        process_memory_manager,
        memory,
        heap_cursor,
        byte_count,
        false,
    );
    if dest_ptr == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return None;
    }
    if !ppc_q3_copy_or_zero(memory, source_ptr, dest_ptr, byte_count) {
        return None;
    }
    ppc_q3_write_u32_to_slice(data, ptr_offset, dest_ptr)
}

pub(crate) fn ppc_q3_trimesh_copy_get_data_attribute_payloads(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    data: &mut [u8],
    element_count_offset: u32,
    attr_count_offset: u32,
    attr_ptr_offset: u32,
    max_element_count: u32,
) -> Option<()> {
    let element_count = ppc_q3_trimesh_header_u32(data, element_count_offset)?;
    let attr_count = ppc_q3_trimesh_header_u32(data, attr_count_offset)?;
    let attr_ptr = ppc_q3_trimesh_header_u32(data, attr_ptr_offset)?;
    if element_count == 0
        || element_count > max_element_count
        || attr_count == 0
        || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
        || attr_ptr == 0
    {
        return Some(());
    }
    for attr_index in 0..attr_count {
        let entry_ptr =
            attr_ptr.checked_add(attr_index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
        let Some(attr_type) = memory.read_u32_be(entry_ptr) else {
            continue;
        };
        let Some(attr_data_size) = ppc_q3_attribute_data_size(attr_type) else {
            continue;
        };
        let Some(source_data_ptr) = memory.read_u32_be(entry_ptr.checked_add(4)?) else {
            continue;
        };
        let data_byte_count = element_count.checked_mul(attr_data_size)?;
        if source_data_ptr != 0
            && ppc_memory_can_read_bytes(memory, source_data_ptr, data_byte_count)
        {
            let dest_data_ptr = ppc_process_heap_alloc(
                process_memory_manager,
                memory,
                heap_cursor,
                data_byte_count,
                false,
            );
            if dest_data_ptr == 0 {
                *last_mem_error = PPC_MEM_FULL_ERR;
                return None;
            }
            if !ppc_q3_copy_or_zero(memory, source_data_ptr, dest_data_ptr, data_byte_count) {
                return None;
            }
            memory.write_u32_be(entry_ptr.checked_add(4)?, dest_data_ptr)?;
        }
        let Some(source_use_ptr) = memory.read_u32_be(entry_ptr.checked_add(8)?) else {
            continue;
        };
        if source_use_ptr != 0 && ppc_memory_can_read_bytes(memory, source_use_ptr, element_count) {
            let dest_use_ptr = ppc_process_heap_alloc(
                process_memory_manager,
                memory,
                heap_cursor,
                element_count,
                false,
            );
            if dest_use_ptr == 0 {
                *last_mem_error = PPC_MEM_FULL_ERR;
                return None;
            }
            if !ppc_q3_copy_or_zero(memory, source_use_ptr, dest_use_ptr, element_count) {
                return None;
            }
            memory.write_u32_be(entry_ptr.checked_add(8)?, dest_use_ptr)?;
        }
    }
    Some(())
}

pub fn ppc_q3_write_u32_to_slice(data: &mut [u8], offset: u32, value: u32) -> Option<()> {
    let offset = usize::try_from(offset).ok()?;
    data.get_mut(offset..offset.checked_add(4)?)?
        .copy_from_slice(&value.to_be_bytes());
    Some(())
}

pub fn ppc_q3_trimesh_store_data(
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    trimesh: u32,
    data: Vec<u8>,
    triangle_attribute_sets: Vec<u32>,
) {
    if let Some(record) = q3_trimeshes
        .iter_mut()
        .find(|record| record.trimesh == trimesh)
    {
        record.data = data;
        record.triangle_attribute_sets = triangle_attribute_sets;
    } else {
        q3_trimeshes.push(PpcQ3TriMeshRecord {
            trimesh,
            data,
            triangle_attribute_sets,
            get_data_copies: Vec::new(),
        });
    }
}

pub fn ppc_q3_trimesh_retain_triangle_attribute_sets(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    data: &[u8],
) -> Vec<u32> {
    let mut attribute_sets = Vec::new();
    if let Some(attribute_set) =
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET)
    {
        if attribute_set != 0
            && ppc_q3_object_type_for_handle(q3_objects, attribute_set) == PPC_Q3_TYPE_ATTRIBUTE_SET
            && ppc_q3_object_retain(q3_objects, q3_object_refs, attribute_set)
        {
            attribute_sets.push(attribute_set);
        }
    }
    let Some(num_triangles) = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
    else {
        return attribute_sets;
    };
    let Some(triangles_ptr) = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_TRIANGLES_OFFSET)
    else {
        return attribute_sets;
    };
    if num_triangles == 0
        || num_triangles > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
        || triangles_ptr == 0
    {
        return attribute_sets;
    }
    ppc_q3_trimesh_retain_attribute_set_values_from_array(
        memory,
        q3_objects,
        q3_object_refs,
        data,
        PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        num_triangles,
        &mut attribute_sets,
    );
    let num_edges = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET).unwrap_or(0);
    ppc_q3_trimesh_retain_attribute_set_values_from_array(
        memory,
        q3_objects,
        q3_object_refs,
        data,
        PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        num_edges,
        &mut attribute_sets,
    );
    let num_points = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET).unwrap_or(0);
    ppc_q3_trimesh_retain_attribute_set_values_from_array(
        memory,
        q3_objects,
        q3_object_refs,
        data,
        PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        num_points,
        &mut attribute_sets,
    );
    attribute_sets
}

pub fn ppc_q3_trimesh_retain_attribute_set_values_from_array(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    data: &[u8],
    count_offset: u32,
    table_offset: u32,
    element_count: u32,
    attribute_sets: &mut Vec<u32>,
) {
    if element_count == 0 || element_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES {
        return;
    }
    let attr_count = ppc_q3_trimesh_header_u32(data, count_offset).unwrap_or(0);
    let attr_ptr = ppc_q3_trimesh_header_u32(data, table_offset).unwrap_or(0);
    if attr_count == 0 || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES || attr_ptr == 0 {
        return;
    }
    for attr_index in 0..attr_count {
        let Some(entry_ptr) =
            attr_ptr.checked_add(attr_index.saturating_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE))
        else {
            return;
        };
        let Some(attr_type) = memory.read_u32_be(entry_ptr) else {
            continue;
        };
        let Some(attr_data_size) = ppc_q3_attribute_data_size(attr_type) else {
            continue;
        };
        if attr_data_size != 4 {
            continue;
        }
        let Some(data_ptr) = memory.read_u32_be(entry_ptr.saturating_add(4)) else {
            continue;
        };
        if data_ptr == 0 {
            continue;
        }
        for element_index in 0..element_count {
            let Some(value_ptr) =
                data_ptr.checked_add(element_index.saturating_mul(attr_data_size))
            else {
                return;
            };
            let Some(attribute_set) = memory.read_u32_be(value_ptr) else {
                continue;
            };
            if attribute_set == 0
                || attribute_sets.contains(&attribute_set)
                || ppc_q3_object_type_for_handle(q3_objects, attribute_set)
                    != PPC_Q3_TYPE_ATTRIBUTE_SET
            {
                continue;
            }
            if ppc_q3_object_retain(q3_objects, q3_object_refs, attribute_set) {
                attribute_sets.push(attribute_set);
            }
        }
    }
}

pub fn ppc_q3_view_angle_aspect_camera_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
) -> u32 {
    ppc_q3_camera_new(
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_cameras,
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
    )
}

pub fn ppc_q3_orthographic_camera_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
) -> u32 {
    ppc_q3_camera_new(
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_cameras,
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC,
    )
}

pub fn ppc_q3_view_plane_camera_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
) -> u32 {
    ppc_q3_camera_new(
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_cameras,
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE,
    )
}

pub fn ppc_q3_camera_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    camera_type: u32,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some(data_size) = ppc_q3_camera_data_size(camera_type) else {
        return 0;
    };
    let Some(mut record) = ppc_read_q3_camera(memory, data_ptr, camera_type) else {
        return 0;
    };
    if !ppc_q3_camera_record_valid(&record) {
        return 0;
    }
    let camera = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        camera_type,
        0,
        data_size,
    );
    if camera != 0 {
        record.camera = camera;
        q3_cameras.push(record);
    }
    camera
}

pub fn ppc_q3_camera_get_placement(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let placement_out_ptr = cpu.gpr[4];
    if placement_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, placement_out_ptr, PPC_Q3_CAMERA_PLACEMENT_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter().find(|record| record.camera == camera) else {
        return false;
    };
    ppc_write_q3_camera_placement(memory, placement_out_ptr, record.placement).is_some()
}

pub fn ppc_q3_camera_set_placement(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &mut [PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let placement_ptr = cpu.gpr[4];
    if placement_ptr == 0 {
        return false;
    }
    let Some(placement) = ppc_read_q3_camera_placement(memory, placement_ptr) else {
        return false;
    };
    if !ppc_q3_camera_placement_valid(placement) {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter_mut().find(|record| record.camera == camera) else {
        return false;
    };
    record.placement = placement;
    true
}

pub fn ppc_q3_camera_get_range(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let range_out_ptr = cpu.gpr[4];
    if range_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, range_out_ptr, PPC_Q3_CAMERA_RANGE_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter().find(|record| record.camera == camera) else {
        return false;
    };
    ppc_write_q3_camera_range(memory, range_out_ptr, record.range_hither, record.range_yon)
        .is_some()
}

pub fn ppc_q3_camera_set_range(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &mut [PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let range_ptr = cpu.gpr[4];
    if range_ptr == 0 {
        return false;
    }
    let Some((hither, yon)) = ppc_read_q3_camera_range(memory, range_ptr) else {
        return false;
    };
    if !ppc_q3_camera_range_valid(hither, yon) {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter_mut().find(|record| record.camera == camera) else {
        return false;
    };
    record.range_hither = hither;
    record.range_yon = yon;
    true
}

pub fn ppc_q3_camera_get_viewport(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let viewport_out_ptr = cpu.gpr[4];
    if viewport_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, viewport_out_ptr, PPC_Q3_CAMERA_VIEWPORT_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter().find(|record| record.camera == camera) else {
        return false;
    };
    ppc_write_q3_camera_viewport(
        memory,
        viewport_out_ptr,
        record.viewport_origin,
        record.viewport_width,
        record.viewport_height,
    )
    .is_some()
}

pub fn ppc_q3_camera_set_viewport(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &mut [PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let viewport_ptr = cpu.gpr[4];
    if viewport_ptr == 0 {
        return false;
    }
    let Some((origin, width, height)) = ppc_read_q3_camera_viewport(memory, viewport_ptr) else {
        return false;
    };
    if !ppc_q3_camera_viewport_valid(origin, width, height) {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter_mut().find(|record| record.camera == camera) else {
        return false;
    };
    record.viewport_origin = origin;
    record.viewport_width = width;
    record.viewport_height = height;
    true
}

pub fn ppc_q3_camera_get_world_to_view(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX4X4_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter().find(|record| record.camera == camera) else {
        return false;
    };
    let Some(matrix) = ppc_q3_camera_world_to_view_matrix(record.placement) else {
        return false;
    };
    ppc_write_q3_matrix4x4(memory, matrix_out_ptr, &matrix).is_some()
}

pub fn ppc_q3_camera_get_view_to_frustum(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let camera = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX4X4_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, camera) {
        return false;
    }
    let Some(record) = q3_cameras.iter().find(|record| record.camera == camera) else {
        return false;
    };
    let Some(matrix) = ppc_q3_camera_view_to_frustum_matrix(record) else {
        return false;
    };
    ppc_write_q3_matrix4x4(memory, matrix_out_ptr, &matrix).is_some()
}

pub fn ppc_q3_view_get_world_to_frustum_matrix_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_views: &[PpcQ3ViewStateRecord],
    q3_cameras: &[PpcQ3CameraRecord],
) -> bool {
    let view = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX4X4_SIZE)
        || !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view)
    {
        return false;
    }
    let Some(view_record) = q3_views.iter().find(|record| record.view == view) else {
        return false;
    };
    if view_record.rendering_depth == 0 {
        return false;
    }
    if !ppc_q3_validate_camera_handle(q3_objects, q3_error_state, view_record.camera) {
        return false;
    }
    let Some(camera) = q3_cameras
        .iter()
        .find(|record| record.camera == view_record.camera)
    else {
        return false;
    };
    let Some(world_to_view) = ppc_q3_camera_world_to_view_matrix(camera.placement) else {
        return false;
    };
    let Some(view_to_frustum) = ppc_q3_camera_view_to_frustum_matrix(camera) else {
        return false;
    };
    let matrix = ppc_q3_matrix4x4_multiply_values(world_to_view, view_to_frustum);
    ppc_write_q3_matrix4x4(memory, matrix_out_ptr, &matrix).is_some()
}

pub fn ppc_q3_frustum_to_window_matrix(viewport: PpcQ3ViewportRect) -> Option<[[f32; 4]; 4]> {
    if viewport.right <= viewport.left || viewport.bottom <= viewport.top {
        return None;
    }
    let half_width = (viewport.right - viewport.left) as f32 * 0.5;
    let half_height = (viewport.bottom - viewport.top) as f32 * 0.5;
    Some([
        [half_width, 0.0, 0.0, 0.0],
        [0.0, -half_height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [viewport.left as f32 + half_width, viewport.top as f32 + half_height, 0.0, 1.0],
    ])
}

pub fn ppc_q3_view_get_frustum_to_window_matrix_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_views: &[PpcQ3ViewStateRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
) -> bool {
    let view = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX4X4_SIZE)
        || !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view)
    {
        return false;
    }
    let Some(view_record) = q3_views.iter().find(|record| record.view == view) else {
        return false;
    };
    if view_record.rendering_depth == 0 {
        return false;
    }
    let Some(target) = ppc_q3_draw_context_render_target(
        q3_objects,
        q3_draw_contexts,
        gworlds,
        view_record.draw_context,
    ) else {
        return false;
    };
    let viewport = target
        .viewport
        .unwrap_or_else(|| PpcQ3ViewportRect::full(target.front_buffer));
    let Some(matrix) = ppc_q3_frustum_to_window_matrix(viewport) else {
        return false;
    };
    ppc_write_q3_matrix4x4(memory, matrix_out_ptr, &matrix).is_some()
}

pub fn ppc_q3_validate_camera_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    camera: u32,
) -> bool {
    if ppc_q3_object_type_is_camera(ppc_q3_object_type_for_handle(q3_objects, camera)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_read_q3_camera(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    camera_type: u32,
) -> Option<PpcQ3CameraRecord> {
    let common = ppc_read_q3_camera_common_data(memory, data_ptr)?;
    let projection = match camera_type {
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT => PpcQ3CameraProjection::ViewAngleAspect {
            fov: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE)?)?,
            aspect_ratio_x_to_y: ppc_read_f32_be(
                memory,
                data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 4)?,
            )?,
        },
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC => PpcQ3CameraProjection::Orthographic {
            left: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE)?)?,
            top: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 4)?)?,
            right: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 8)?)?,
            bottom: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 12)?)?,
        },
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE => PpcQ3CameraProjection::ViewPlane {
            view_plane: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE)?)?,
            half_width_at_view_plane: ppc_read_f32_be(
                memory,
                data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 4)?,
            )?,
            half_height_at_view_plane: ppc_read_f32_be(
                memory,
                data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 8)?,
            )?,
            center_x_on_view_plane: ppc_read_f32_be(
                memory,
                data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 12)?,
            )?,
            center_y_on_view_plane: ppc_read_f32_be(
                memory,
                data_ptr.checked_add(PPC_Q3_CAMERA_DATA_SIZE + 16)?,
            )?,
        },
        _ => return None,
    };
    Some(PpcQ3CameraRecord {
        camera: 0,
        camera_type,
        placement: common.placement,
        range_hither: common.range_hither,
        range_yon: common.range_yon,
        viewport_origin: common.viewport_origin,
        viewport_width: common.viewport_width,
        viewport_height: common.viewport_height,
        projection,
    })
}

pub(crate) fn ppc_read_q3_camera_common_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
) -> Option<PpcQ3CameraCommonData> {
    Some(PpcQ3CameraCommonData {
        placement: ppc_read_q3_camera_placement(
            memory,
            data_ptr.checked_add(PPC_Q3_CAMERA_PLACEMENT_OFFSET)?,
        )?,
        range_hither: ppc_read_f32_be(memory, data_ptr.checked_add(PPC_Q3_CAMERA_RANGE_OFFSET)?)?,
        range_yon: ppc_read_f32_be(
            memory,
            data_ptr.checked_add(PPC_Q3_CAMERA_RANGE_OFFSET + 4)?,
        )?,
        viewport_origin: ppc_read_q3_vector2d(
            memory,
            data_ptr.checked_add(PPC_Q3_CAMERA_VIEWPORT_OFFSET)?,
        )?,
        viewport_width: ppc_read_f32_be(
            memory,
            data_ptr
                .checked_add(PPC_Q3_CAMERA_VIEWPORT_OFFSET)?
                .checked_add(PPC_Q3_CAMERA_VIEWPORT_WIDTH_OFFSET)?,
        )?,
        viewport_height: ppc_read_f32_be(
            memory,
            data_ptr
                .checked_add(PPC_Q3_CAMERA_VIEWPORT_OFFSET)?
                .checked_add(PPC_Q3_CAMERA_VIEWPORT_HEIGHT_OFFSET)?,
        )?,
    })
}

pub fn ppc_q3_camera_data_size(camera_type: u32) -> Option<u32> {
    match camera_type {
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT => Some(PPC_Q3_VIEW_ANGLE_ASPECT_CAMERA_DATA_SIZE),
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC => Some(PPC_Q3_ORTHOGRAPHIC_CAMERA_DATA_SIZE),
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE => Some(PPC_Q3_VIEW_PLANE_CAMERA_DATA_SIZE),
        _ => None,
    }
}

pub fn ppc_read_q3_camera_placement(
    memory: &mut PpcSectionMem,
    placement_ptr: u32,
) -> Option<PpcQ3CameraPlacement> {
    Some(PpcQ3CameraPlacement {
        camera_location: ppc_read_q3_vector3d(memory, placement_ptr)?,
        point_of_interest: ppc_read_q3_vector3d(memory, placement_ptr.checked_add(12)?)?,
        up_vector: ppc_read_q3_vector3d(memory, placement_ptr.checked_add(24)?)?,
    })
}

pub fn ppc_write_q3_camera_placement(
    memory: &mut PpcSectionMem,
    placement_ptr: u32,
    placement: PpcQ3CameraPlacement,
) -> Option<()> {
    ppc_write_q3_vector3d(memory, placement_ptr, placement.camera_location)?;
    ppc_write_q3_vector3d(
        memory,
        placement_ptr.checked_add(12)?,
        placement.point_of_interest,
    )?;
    ppc_write_q3_vector3d(memory, placement_ptr.checked_add(24)?, placement.up_vector)?;
    Some(())
}

pub fn ppc_read_q3_camera_range(memory: &mut PpcSectionMem, range_ptr: u32) -> Option<(f32, f32)> {
    Some((
        ppc_read_f32_be(memory, range_ptr)?,
        ppc_read_f32_be(memory, range_ptr.checked_add(4)?)?,
    ))
}

pub fn ppc_write_q3_camera_range(
    memory: &mut PpcSectionMem,
    range_ptr: u32,
    hither: f32,
    yon: f32,
) -> Option<()> {
    ppc_write_f32_be(memory, range_ptr, hither)?;
    ppc_write_f32_be(memory, range_ptr.checked_add(4)?, yon)?;
    Some(())
}

pub fn ppc_read_q3_camera_viewport(
    memory: &mut PpcSectionMem,
    viewport_ptr: u32,
) -> Option<((f32, f32), f32, f32)> {
    Some((
        ppc_read_q3_vector2d(memory, viewport_ptr)?,
        ppc_read_f32_be(
            memory,
            viewport_ptr.checked_add(PPC_Q3_CAMERA_VIEWPORT_WIDTH_OFFSET)?,
        )?,
        ppc_read_f32_be(
            memory,
            viewport_ptr.checked_add(PPC_Q3_CAMERA_VIEWPORT_HEIGHT_OFFSET)?,
        )?,
    ))
}

pub fn ppc_write_q3_camera_viewport(
    memory: &mut PpcSectionMem,
    viewport_ptr: u32,
    origin: (f32, f32),
    width: f32,
    height: f32,
) -> Option<()> {
    ppc_write_q3_vector2d(memory, viewport_ptr, origin)?;
    ppc_write_f32_be(
        memory,
        viewport_ptr.checked_add(PPC_Q3_CAMERA_VIEWPORT_WIDTH_OFFSET)?,
        width,
    )?;
    ppc_write_f32_be(
        memory,
        viewport_ptr.checked_add(PPC_Q3_CAMERA_VIEWPORT_HEIGHT_OFFSET)?,
        height,
    )?;
    Some(())
}

pub fn ppc_q3_camera_record_valid(record: &PpcQ3CameraRecord) -> bool {
    ppc_q3_camera_projection_matches_type(record.projection, record.camera_type)
        && ppc_q3_camera_placement_valid(record.placement)
        && ppc_q3_camera_range_valid(record.range_hither, record.range_yon)
        && ppc_q3_camera_viewport_valid(
            record.viewport_origin,
            record.viewport_width,
            record.viewport_height,
        )
        && ppc_q3_camera_projection_valid(record.projection)
}

pub fn ppc_q3_camera_range_valid(hither: f32, yon: f32) -> bool {
    hither.is_finite() && yon.is_finite() && hither > 0.0 && yon > hither
}

pub fn ppc_q3_camera_viewport_valid(origin: (f32, f32), width: f32, height: f32) -> bool {
    origin.0.is_finite()
        && origin.1.is_finite()
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
}

pub fn ppc_q3_camera_projection_matches_type(
    projection: PpcQ3CameraProjection,
    camera_type: u32,
) -> bool {
    matches!(
        (camera_type, projection),
        (
            PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
            PpcQ3CameraProjection::ViewAngleAspect { .. }
        ) | (
            PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC,
            PpcQ3CameraProjection::Orthographic { .. }
        ) | (
            PPC_Q3_CAMERA_TYPE_VIEW_PLANE,
            PpcQ3CameraProjection::ViewPlane { .. }
        )
    )
}

pub fn ppc_q3_camera_projection_valid(projection: PpcQ3CameraProjection) -> bool {
    match projection {
        PpcQ3CameraProjection::ViewAngleAspect {
            fov,
            aspect_ratio_x_to_y,
        } => {
            fov.is_finite()
                && fov > 0.0
                && aspect_ratio_x_to_y.is_finite()
                && aspect_ratio_x_to_y > 0.0
        }
        PpcQ3CameraProjection::Orthographic {
            left,
            top,
            right,
            bottom,
        } => {
            left.is_finite()
                && top.is_finite()
                && right.is_finite()
                && bottom.is_finite()
                && left < right
                && bottom < top
        }
        PpcQ3CameraProjection::ViewPlane {
            view_plane,
            half_width_at_view_plane,
            half_height_at_view_plane,
            center_x_on_view_plane,
            center_y_on_view_plane,
        } => {
            view_plane.is_finite()
                && view_plane > 0.0
                && half_width_at_view_plane.is_finite()
                && half_width_at_view_plane > 0.0
                && half_height_at_view_plane.is_finite()
                && half_height_at_view_plane > 0.0
                && center_x_on_view_plane.is_finite()
                && center_y_on_view_plane.is_finite()
        }
    }
}

pub fn ppc_q3_camera_placement_valid(placement: PpcQ3CameraPlacement) -> bool {
    let direction = ppc_q3_vector3d_sub(placement.point_of_interest, placement.camera_location);
    let Some(forward) = ppc_q3_vector3d_normalized_value(direction) else {
        return false;
    };
    let Some(up) = ppc_q3_vector3d_normalized_value(placement.up_vector) else {
        return false;
    };
    ppc_q3_vector3d_normalized_value(ppc_q3_vector3d_cross_values(forward, up)).is_some()
}

pub fn ppc_q3_camera_world_to_view_matrix(placement: PpcQ3CameraPlacement) -> Option<[[f32; 4]; 4]> {
    let eye = placement.camera_location;
    let forward =
        ppc_q3_vector3d_normalized_value(ppc_q3_vector3d_sub(placement.point_of_interest, eye))?;
    let up = ppc_q3_vector3d_normalized_value(placement.up_vector)?;
    let right = ppc_q3_vector3d_normalized_value(ppc_q3_vector3d_cross_values(forward, up))?;
    let true_up = ppc_q3_vector3d_cross_values(right, forward);
    Some([
        [right.0, true_up.0, -forward.0, 0.0],
        [right.1, true_up.1, -forward.1, 0.0],
        [right.2, true_up.2, -forward.2, 0.0],
        [
            -ppc_q3_vector3d_dot(eye, right),
            -ppc_q3_vector3d_dot(eye, true_up),
            ppc_q3_vector3d_dot(eye, forward),
            1.0,
        ],
    ])
}

pub fn ppc_q3_camera_view_to_frustum_matrix(record: &PpcQ3CameraRecord) -> Option<[[f32; 4]; 4]> {
    if !ppc_q3_camera_record_valid(record) {
        return None;
    }
    let hither = record.range_hither;
    let yon = record.range_yon;
    let depth = hither - yon;
    match record.projection {
        PpcQ3CameraProjection::ViewAngleAspect {
            fov,
            aspect_ratio_x_to_y,
        } => {
            let half_fov_tan = (fov * 0.5).tan();
            if !half_fov_tan.is_finite() || half_fov_tan <= 0.0 {
                return None;
            }
            let max_axis_scale = 1.0 / half_fov_tan;
            let (x_scale, y_scale) = if aspect_ratio_x_to_y >= 1.0 {
                (max_axis_scale / aspect_ratio_x_to_y, max_axis_scale)
            } else {
                (max_axis_scale, max_axis_scale * aspect_ratio_x_to_y)
            };
            Some(ppc_q3_perspective_frustum_matrix(
                x_scale, y_scale, 0.0, 0.0, hither, yon, depth,
            ))
        }
        PpcQ3CameraProjection::Orthographic {
            left,
            top,
            right,
            bottom,
        } => Some([
            [2.0 / (right - left), 0.0, 0.0, 0.0],
            [0.0, 2.0 / (top - bottom), 0.0, 0.0],
            [0.0, 0.0, -1.0 / depth, 0.0],
            [
                -(right + left) / (right - left),
                -(top + bottom) / (top - bottom),
                -hither / depth,
                1.0,
            ],
        ]),
        PpcQ3CameraProjection::ViewPlane {
            view_plane,
            half_width_at_view_plane,
            half_height_at_view_plane,
            center_x_on_view_plane,
            center_y_on_view_plane,
        } => Some(ppc_q3_perspective_frustum_matrix(
            view_plane / half_width_at_view_plane,
            view_plane / half_height_at_view_plane,
            center_x_on_view_plane / half_width_at_view_plane,
            center_y_on_view_plane / half_height_at_view_plane,
            hither,
            yon,
            depth,
        )),
    }
}

pub fn ppc_q3_perspective_frustum_matrix(
    x_scale: f32,
    y_scale: f32,
    x_offset: f32,
    y_offset: f32,
    hither: f32,
    yon: f32,
    _depth: f32,
) -> [[f32; 4]; 4] {
    let q = yon / (yon - hither);
    [
        [x_scale, 0.0, 0.0, 0.0],
        [0.0, y_scale, 0.0, 0.0],
        [x_offset, y_offset, q, -1.0],
        [0.0, 0.0, q * hither, 0.0],
    ]
}

pub fn ppc_q3_ambient_light_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some(data) = ppc_read_q3_light_data(memory, data_ptr) else {
        return 0;
    };
    if !ppc_q3_light_data_valid(data) {
        return 0;
    }
    let light = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_LIGHT_TYPE_AMBIENT,
        0,
        PPC_Q3_LIGHT_DATA_SIZE,
    );
    if light != 0 {
        q3_lights.push(PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
            data,
            kind: PpcQ3LightKind::Ambient,
        });
    }
    light
}

pub fn ppc_q3_directional_light_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some((data, casts_shadows, direction)) =
        ppc_read_q3_directional_light_data(memory, data_ptr)
    else {
        return 0;
    };
    if !ppc_q3_light_data_valid(data)
        || !ppc_q3_boolean_valid(casts_shadows)
        || ppc_q3_vector3d_normalized_value(direction).is_none()
    {
        return 0;
    }
    let light = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
        0,
        PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE,
    );
    if light != 0 {
        q3_lights.push(PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data,
            kind: PpcQ3LightKind::Directional {
                casts_shadows,
                direction,
            },
        });
    }
    light
}

pub fn ppc_q3_light_get_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    _q3_lights: &[PpcQ3LightRecord],
) -> u32 {
    let light = cpu.gpr[3];
    let light_type = ppc_q3_object_type_for_handle(q3_objects, light);
    if ppc_q3_object_type_is_light(light_type) {
        light_type
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_light_get_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let state_out_ptr = cpu.gpr[4];
    if state_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, state_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    memory
        .write_u32_be(state_out_ptr, record.data.is_on)
        .is_some()
}

pub fn ppc_q3_light_set_state(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let is_on = cpu.gpr[4];
    if !ppc_q3_boolean_valid(is_on) {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    record.data.is_on = is_on;
    true
}

pub fn ppc_q3_light_get_brightness(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let brightness_out_ptr = cpu.gpr[4];
    if brightness_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, brightness_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    ppc_write_f32_be(memory, brightness_out_ptr, record.data.brightness).is_some()
}

pub fn ppc_q3_light_set_brightness(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
    brightness: f32,
) -> bool {
    if !brightness.is_finite() {
        return false;
    }
    let light = cpu.gpr[3];
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    record.data.brightness = brightness;
    true
}

pub fn ppc_q3_light_get_color(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let color_out_ptr = cpu.gpr[4];
    if color_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, color_out_ptr, PPC_Q3_VECTOR3D_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    ppc_write_q3_color_rgb(memory, color_out_ptr, record.data.color).is_some()
}

pub fn ppc_q3_light_set_color(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let color_ptr = cpu.gpr[4];
    if color_ptr == 0 {
        return false;
    }
    let Some(color) = ppc_read_q3_color_rgb(memory, color_ptr) else {
        return false;
    };
    if !ppc_q3_color_rgb_valid(color) {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    record.data.color = color;
    true
}

pub fn ppc_q3_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_out_ptr = cpu.gpr[4];
    if data_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, data_out_ptr, PPC_Q3_LIGHT_DATA_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    ppc_write_q3_light_data(memory, data_out_ptr, record.data).is_some()
}

pub fn ppc_q3_light_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if data_ptr == 0 {
        return false;
    }
    let Some(data) = ppc_read_q3_light_data(memory, data_ptr) else {
        return false;
    };
    if !ppc_q3_light_data_valid(data) {
        return false;
    }
    if !ppc_q3_validate_light_handle(q3_objects, q3_error_state, light) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    record.data = data;
    true
}

pub fn ppc_q3_validate_light_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    light: u32,
) -> bool {
    if ppc_q3_object_type_is_light(ppc_q3_object_type_for_handle(q3_objects, light)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_validate_typed_light_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    light: u32,
    light_type: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, light) == light_type {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_ambient_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    ppc_q3_typed_light_get_data(
        cpu,
        memory,
        q3_objects,
        q3_error_state,
        q3_lights,
        PPC_Q3_LIGHT_TYPE_AMBIENT,
        PPC_Q3_LIGHT_DATA_SIZE,
    )
}

pub fn ppc_q3_ambient_light_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if data_ptr == 0 {
        return false;
    }
    let Some(data) = ppc_read_q3_light_data(memory, data_ptr) else {
        return false;
    };
    if !ppc_q3_light_data_valid(data) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_AMBIENT,
    ) {
        return false;
    }
    let Some(record) = q3_lights
        .iter_mut()
        .find(|record| record.light == light && record.light_type == PPC_Q3_LIGHT_TYPE_AMBIENT)
    else {
        return false;
    };
    record.data = data;
    true
}

pub fn ppc_q3_directional_light_get_cast_shadows_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_out_ptr = cpu.gpr[4];
    if casts_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, casts_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Directional { casts_shadows, .. } = record.kind else {
        return false;
    };
    memory.write_u32_be(casts_out_ptr, casts_shadows).is_some()
}

pub fn ppc_q3_directional_light_set_cast_shadows_state(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_shadows = cpu.gpr[4];
    if !ppc_q3_boolean_valid(casts_shadows) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Directional {
        casts_shadows: record_casts_shadows,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_casts_shadows = casts_shadows;
    true
}

pub fn ppc_q3_directional_light_get_direction(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let direction_out_ptr = cpu.gpr[4];
    if direction_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, direction_out_ptr, PPC_Q3_VECTOR3D_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Directional { direction, .. } = record.kind else {
        return false;
    };
    ppc_write_q3_vector3d(memory, direction_out_ptr, direction).is_some()
}

pub fn ppc_q3_directional_light_set_direction(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let direction_ptr = cpu.gpr[4];
    if direction_ptr == 0 {
        return false;
    }
    let Some(direction) = ppc_read_q3_vector3d(memory, direction_ptr) else {
        return false;
    };
    if ppc_q3_vector3d_normalized_value(direction).is_none() {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Directional {
        direction: record_direction,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_direction = direction;
    true
}

pub fn ppc_q3_directional_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    ppc_q3_typed_light_get_data(
        cpu,
        memory,
        q3_objects,
        q3_error_state,
        q3_lights,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
        PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE,
    )
}

pub fn ppc_q3_directional_light_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if data_ptr == 0 {
        return false;
    }
    let Some((data, casts_shadows, direction)) =
        ppc_read_q3_directional_light_data(memory, data_ptr)
    else {
        return false;
    };
    if !ppc_q3_light_data_valid(data)
        || !ppc_q3_boolean_valid(casts_shadows)
        || ppc_q3_vector3d_normalized_value(direction).is_none()
    {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
    ) {
        return false;
    }
    let Some(record) = q3_lights
        .iter_mut()
        .find(|record| record.light == light && record.light_type == PPC_Q3_LIGHT_TYPE_DIRECTIONAL)
    else {
        return false;
    };
    record.data = data;
    record.kind = PpcQ3LightKind::Directional {
        casts_shadows,
        direction,
    };
    true
}

pub fn ppc_q3_point_light_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some((data, casts_shadows, attenuation, location)) =
        ppc_read_q3_point_light_data(memory, data_ptr)
    else {
        return 0;
    };
    if !ppc_q3_point_light_data_valid(data, casts_shadows, attenuation, location) {
        return 0;
    }
    let light = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_LIGHT_TYPE_POINT,
        0,
        PPC_Q3_POINT_LIGHT_DATA_SIZE,
    );
    if light != 0 {
        q3_lights.push(PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_POINT,
            data,
            kind: PpcQ3LightKind::Point {
                casts_shadows,
                attenuation,
                location,
            },
        });
    }
    light
}

pub fn ppc_q3_point_light_get_cast_shadows_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_out_ptr = cpu.gpr[4];
    if casts_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, casts_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point { casts_shadows, .. } = record.kind else {
        return false;
    };
    memory.write_u32_be(casts_out_ptr, casts_shadows).is_some()
}

pub fn ppc_q3_point_light_set_cast_shadows_state(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_shadows = cpu.gpr[4];
    if !ppc_q3_boolean_valid(casts_shadows) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point {
        casts_shadows: record_casts_shadows,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_casts_shadows = casts_shadows;
    true
}

pub fn ppc_q3_point_light_get_attenuation(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let attenuation_out_ptr = cpu.gpr[4];
    if attenuation_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, attenuation_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point { attenuation, .. } = record.kind else {
        return false;
    };
    memory
        .write_u32_be(attenuation_out_ptr, attenuation)
        .is_some()
}

pub fn ppc_q3_point_light_set_attenuation(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let attenuation = cpu.gpr[4];
    if !ppc_q3_attenuation_type_valid(attenuation) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point {
        attenuation: record_attenuation,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_attenuation = attenuation;
    true
}

pub fn ppc_q3_point_light_get_location(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let location_out_ptr = cpu.gpr[4];
    if location_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, location_out_ptr, PPC_Q3_VECTOR3D_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point { location, .. } = record.kind else {
        return false;
    };
    ppc_write_q3_vector3d(memory, location_out_ptr, location).is_some()
}

pub fn ppc_q3_point_light_set_location(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let location_ptr = cpu.gpr[4];
    if location_ptr == 0 {
        return false;
    }
    let Some(location) = ppc_read_q3_vector3d(memory, location_ptr) else {
        return false;
    };
    if !ppc_q3_triplet_valid(location) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Point {
        location: record_location,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_location = location;
    true
}

pub fn ppc_q3_point_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    ppc_q3_typed_light_get_data(
        cpu,
        memory,
        q3_objects,
        q3_error_state,
        q3_lights,
        PPC_Q3_LIGHT_TYPE_POINT,
        PPC_Q3_POINT_LIGHT_DATA_SIZE,
    )
}

pub fn ppc_q3_point_light_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if data_ptr == 0 {
        return false;
    }
    let Some((data, casts_shadows, attenuation, location)) =
        ppc_read_q3_point_light_data(memory, data_ptr)
    else {
        return false;
    };
    if !ppc_q3_point_light_data_valid(data, casts_shadows, attenuation, location) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_POINT,
    ) {
        return false;
    }
    let Some(record) = q3_lights
        .iter_mut()
        .find(|record| record.light == light && record.light_type == PPC_Q3_LIGHT_TYPE_POINT)
    else {
        return false;
    };
    record.data = data;
    record.kind = PpcQ3LightKind::Point {
        casts_shadows,
        attenuation,
        location,
    };
    true
}

pub fn ppc_q3_spot_light_new(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> u32 {
    let data_ptr = cpu.gpr[3];
    if data_ptr == 0 {
        return 0;
    }
    let Some((
        data,
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    )) = ppc_read_q3_spot_light_data(memory, data_ptr)
    else {
        return 0;
    };
    if !ppc_q3_spot_light_data_valid(
        data,
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    ) {
        return 0;
    }
    let light = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_LIGHT_TYPE_SPOT,
        0,
        PPC_Q3_SPOT_LIGHT_DATA_SIZE,
    );
    if light != 0 {
        q3_lights.push(PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_SPOT,
            data,
            kind: PpcQ3LightKind::Spot {
                casts_shadows,
                attenuation,
                location,
                direction,
                hot_angle,
                outer_angle,
                fall_off,
            },
        });
    }
    light
}

pub fn ppc_q3_spot_light_get_cast_shadows_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_out_ptr = cpu.gpr[4];
    if casts_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, casts_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { casts_shadows, .. } = record.kind else {
        return false;
    };
    memory.write_u32_be(casts_out_ptr, casts_shadows).is_some()
}

pub fn ppc_q3_spot_light_set_cast_shadows_state(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let casts_shadows = cpu.gpr[4];
    if !ppc_q3_boolean_valid(casts_shadows) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        casts_shadows: record_casts_shadows,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_casts_shadows = casts_shadows;
    true
}

pub fn ppc_q3_spot_light_get_attenuation(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let attenuation_out_ptr = cpu.gpr[4];
    if attenuation_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, attenuation_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { attenuation, .. } = record.kind else {
        return false;
    };
    memory
        .write_u32_be(attenuation_out_ptr, attenuation)
        .is_some()
}

pub fn ppc_q3_spot_light_set_attenuation(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let attenuation = cpu.gpr[4];
    if !ppc_q3_attenuation_type_valid(attenuation) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        attenuation: record_attenuation,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_attenuation = attenuation;
    true
}

pub fn ppc_q3_spot_light_get_location(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let location_out_ptr = cpu.gpr[4];
    if location_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, location_out_ptr, PPC_Q3_VECTOR3D_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { location, .. } = record.kind else {
        return false;
    };
    ppc_write_q3_vector3d(memory, location_out_ptr, location).is_some()
}

pub fn ppc_q3_spot_light_set_location(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let location_ptr = cpu.gpr[4];
    if location_ptr == 0 {
        return false;
    }
    let Some(location) = ppc_read_q3_vector3d(memory, location_ptr) else {
        return false;
    };
    if !ppc_q3_triplet_valid(location) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        location: record_location,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_location = location;
    true
}

pub fn ppc_q3_spot_light_get_direction(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let direction_out_ptr = cpu.gpr[4];
    if direction_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, direction_out_ptr, PPC_Q3_VECTOR3D_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { direction, .. } = record.kind else {
        return false;
    };
    ppc_write_q3_vector3d(memory, direction_out_ptr, direction).is_some()
}

pub fn ppc_q3_spot_light_set_direction(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let direction_ptr = cpu.gpr[4];
    if direction_ptr == 0 {
        return false;
    }
    let Some(direction) = ppc_read_q3_vector3d(memory, direction_ptr) else {
        return false;
    };
    if ppc_q3_vector3d_normalized_value(direction).is_none() {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        direction: record_direction,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_direction = direction;
    true
}

pub fn ppc_q3_spot_light_get_hot_angle(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let angle_out_ptr = cpu.gpr[4];
    if angle_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, angle_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { hot_angle, .. } = record.kind else {
        return false;
    };
    ppc_write_f32_be(memory, angle_out_ptr, hot_angle).is_some()
}

pub fn ppc_q3_spot_light_set_hot_angle(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
    hot_angle: f32,
) -> bool {
    if !ppc_q3_light_angle_valid(hot_angle) {
        return false;
    }
    let light = cpu.gpr[3];
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        hot_angle: record_hot_angle,
        outer_angle,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    if hot_angle > *outer_angle {
        return false;
    }
    *record_hot_angle = hot_angle;
    true
}

pub fn ppc_q3_spot_light_get_outer_angle(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let angle_out_ptr = cpu.gpr[4];
    if angle_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, angle_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { outer_angle, .. } = record.kind else {
        return false;
    };
    ppc_write_f32_be(memory, angle_out_ptr, outer_angle).is_some()
}

pub fn ppc_q3_spot_light_set_outer_angle(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
    outer_angle: f32,
) -> bool {
    if !ppc_q3_light_angle_valid(outer_angle) {
        return false;
    }
    let light = cpu.gpr[3];
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        hot_angle,
        outer_angle: record_outer_angle,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    if outer_angle < *hot_angle {
        return false;
    }
    *record_outer_angle = outer_angle;
    true
}

pub fn ppc_q3_spot_light_get_fall_off(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let fall_off_out_ptr = cpu.gpr[4];
    if fall_off_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, fall_off_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot { fall_off, .. } = record.kind else {
        return false;
    };
    memory.write_u32_be(fall_off_out_ptr, fall_off).is_some()
}

pub fn ppc_q3_spot_light_set_fall_off(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let fall_off = cpu.gpr[4];
    if !ppc_q3_fall_off_type_valid(fall_off) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights.iter_mut().find(|record| record.light == light) else {
        return false;
    };
    let PpcQ3LightKind::Spot {
        fall_off: record_fall_off,
        ..
    } = &mut record.kind
    else {
        return false;
    };
    *record_fall_off = fall_off;
    true
}

pub fn ppc_q3_spot_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
) -> bool {
    ppc_q3_typed_light_get_data(
        cpu,
        memory,
        q3_objects,
        q3_error_state,
        q3_lights,
        PPC_Q3_LIGHT_TYPE_SPOT,
        PPC_Q3_SPOT_LIGHT_DATA_SIZE,
    )
}

pub fn ppc_q3_spot_light_set_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &mut [PpcQ3LightRecord],
) -> bool {
    let light = cpu.gpr[3];
    let data_ptr = cpu.gpr[4];
    if data_ptr == 0 {
        return false;
    }
    let Some((
        data,
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    )) = ppc_read_q3_spot_light_data(memory, data_ptr)
    else {
        return false;
    };
    if !ppc_q3_spot_light_data_valid(
        data,
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    ) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(
        q3_objects,
        q3_error_state,
        light,
        PPC_Q3_LIGHT_TYPE_SPOT,
    ) {
        return false;
    }
    let Some(record) = q3_lights
        .iter_mut()
        .find(|record| record.light == light && record.light_type == PPC_Q3_LIGHT_TYPE_SPOT)
    else {
        return false;
    };
    record.data = data;
    record.kind = PpcQ3LightKind::Spot {
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    };
    true
}

pub fn ppc_q3_typed_light_get_data(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_lights: &[PpcQ3LightRecord],
    light_type: u32,
    data_size: u32,
) -> bool {
    let light = cpu.gpr[3];
    let data_out_ptr = cpu.gpr[4];
    if data_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, data_out_ptr, data_size) {
        return false;
    }
    if !ppc_q3_validate_typed_light_handle(q3_objects, q3_error_state, light, light_type) {
        return false;
    }
    let Some(record) = q3_lights
        .iter()
        .find(|record| record.light == light && record.light_type == light_type)
    else {
        return false;
    };
    match data_size {
        PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE => {
            ppc_write_q3_directional_light_data(memory, data_out_ptr, record).is_some()
        }
        PPC_Q3_POINT_LIGHT_DATA_SIZE => {
            ppc_write_q3_point_light_data(memory, data_out_ptr, record).is_some()
        }
        PPC_Q3_SPOT_LIGHT_DATA_SIZE => {
            ppc_write_q3_spot_light_data(memory, data_out_ptr, record).is_some()
        }
        _ => ppc_write_q3_light_data(memory, data_out_ptr, record.data).is_some(),
    }
}

pub fn ppc_read_q3_light_data(memory: &mut PpcSectionMem, data_ptr: u32) -> Option<PpcQ3LightData> {
    Some(PpcQ3LightData {
        is_on: memory.read_u32_be(data_ptr)?,
        brightness: ppc_read_f32_be(memory, data_ptr.checked_add(4)?)?,
        color: ppc_read_q3_color_rgb(memory, data_ptr.checked_add(8)?)?,
    })
}

pub fn ppc_write_q3_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    data: PpcQ3LightData,
) -> Option<()> {
    memory.write_u32_be(data_ptr, data.is_on)?;
    ppc_write_f32_be(memory, data_ptr.checked_add(4)?, data.brightness)?;
    ppc_write_q3_color_rgb(memory, data_ptr.checked_add(8)?, data.color)?;
    Some(())
}

pub fn ppc_read_q3_directional_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
) -> Option<(PpcQ3LightData, u32, (f32, f32, f32))> {
    Some((
        ppc_read_q3_light_data(memory, data_ptr)?,
        memory.read_u32_be(data_ptr.checked_add(20)?)?,
        ppc_read_q3_vector3d(memory, data_ptr.checked_add(24)?)?,
    ))
}

pub fn ppc_write_q3_directional_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    record: &PpcQ3LightRecord,
) -> Option<()> {
    let PpcQ3LightKind::Directional {
        casts_shadows,
        direction,
    } = record.kind
    else {
        return None;
    };
    ppc_write_q3_light_data(memory, data_ptr, record.data)?;
    memory.write_u32_be(data_ptr.checked_add(20)?, casts_shadows)?;
    ppc_write_q3_vector3d(memory, data_ptr.checked_add(24)?, direction)?;
    Some(())
}

pub fn ppc_read_q3_point_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
) -> Option<(PpcQ3LightData, u32, u32, (f32, f32, f32))> {
    Some((
        ppc_read_q3_light_data(memory, data_ptr)?,
        memory.read_u32_be(data_ptr.checked_add(20)?)?,
        memory.read_u32_be(data_ptr.checked_add(24)?)?,
        ppc_read_q3_vector3d(memory, data_ptr.checked_add(28)?)?,
    ))
}

pub fn ppc_write_q3_point_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    record: &PpcQ3LightRecord,
) -> Option<()> {
    let PpcQ3LightKind::Point {
        casts_shadows,
        attenuation,
        location,
    } = record.kind
    else {
        return None;
    };
    ppc_write_q3_light_data(memory, data_ptr, record.data)?;
    memory.write_u32_be(data_ptr.checked_add(20)?, casts_shadows)?;
    memory.write_u32_be(data_ptr.checked_add(24)?, attenuation)?;
    ppc_write_q3_vector3d(memory, data_ptr.checked_add(28)?, location)?;
    Some(())
}

#[allow(clippy::type_complexity)]
pub fn ppc_read_q3_spot_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
) -> Option<(
    PpcQ3LightData,
    u32,
    u32,
    (f32, f32, f32),
    (f32, f32, f32),
    f32,
    f32,
    u32,
)> {
    Some((
        ppc_read_q3_light_data(memory, data_ptr)?,
        memory.read_u32_be(data_ptr.checked_add(20)?)?,
        memory.read_u32_be(data_ptr.checked_add(24)?)?,
        ppc_read_q3_vector3d(memory, data_ptr.checked_add(28)?)?,
        ppc_read_q3_vector3d(memory, data_ptr.checked_add(40)?)?,
        ppc_read_f32_be(memory, data_ptr.checked_add(52)?)?,
        ppc_read_f32_be(memory, data_ptr.checked_add(56)?)?,
        memory.read_u32_be(data_ptr.checked_add(60)?)?,
    ))
}

pub fn ppc_write_q3_spot_light_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    record: &PpcQ3LightRecord,
) -> Option<()> {
    let PpcQ3LightKind::Spot {
        casts_shadows,
        attenuation,
        location,
        direction,
        hot_angle,
        outer_angle,
        fall_off,
    } = record.kind
    else {
        return None;
    };
    ppc_write_q3_light_data(memory, data_ptr, record.data)?;
    memory.write_u32_be(data_ptr.checked_add(20)?, casts_shadows)?;
    memory.write_u32_be(data_ptr.checked_add(24)?, attenuation)?;
    ppc_write_q3_vector3d(memory, data_ptr.checked_add(28)?, location)?;
    ppc_write_q3_vector3d(memory, data_ptr.checked_add(40)?, direction)?;
    ppc_write_f32_be(memory, data_ptr.checked_add(52)?, hot_angle)?;
    ppc_write_f32_be(memory, data_ptr.checked_add(56)?, outer_angle)?;
    memory.write_u32_be(data_ptr.checked_add(60)?, fall_off)?;
    Some(())
}

pub fn ppc_q3_light_data_valid(data: PpcQ3LightData) -> bool {
    ppc_q3_boolean_valid(data.is_on)
        && data.brightness.is_finite()
        && ppc_q3_color_rgb_valid(data.color)
}

pub fn ppc_q3_point_light_data_valid(
    data: PpcQ3LightData,
    casts_shadows: u32,
    attenuation: u32,
    location: (f32, f32, f32),
) -> bool {
    ppc_q3_light_data_valid(data)
        && ppc_q3_boolean_valid(casts_shadows)
        && ppc_q3_attenuation_type_valid(attenuation)
        && ppc_q3_triplet_valid(location)
}

pub fn ppc_q3_spot_light_data_valid(
    data: PpcQ3LightData,
    casts_shadows: u32,
    attenuation: u32,
    location: (f32, f32, f32),
    direction: (f32, f32, f32),
    hot_angle: f32,
    outer_angle: f32,
    fall_off: u32,
) -> bool {
    ppc_q3_light_data_valid(data)
        && ppc_q3_boolean_valid(casts_shadows)
        && ppc_q3_attenuation_type_valid(attenuation)
        && ppc_q3_triplet_valid(location)
        && ppc_q3_vector3d_normalized_value(direction).is_some()
        && ppc_q3_light_angle_valid(hot_angle)
        && ppc_q3_light_angle_valid(outer_angle)
        && hot_angle <= outer_angle
        && ppc_q3_fall_off_type_valid(fall_off)
}

pub fn ppc_q3_boolean_valid(value: u32) -> bool {
    value <= 1
}

pub fn ppc_q3_color_rgb_valid((red, green, blue): (f32, f32, f32)) -> bool {
    red.is_finite() && green.is_finite() && blue.is_finite()
}

pub fn ppc_q3_triplet_valid((x, y, z): (f32, f32, f32)) -> bool {
    x.is_finite() && y.is_finite() && z.is_finite()
}

pub fn ppc_q3_attenuation_type_valid(value: u32) -> bool {
    value <= PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE_SQUARED
}

pub fn ppc_q3_light_angle_valid(value: f32) -> bool {
    value.is_finite() && value >= 0.0 && value <= std::f32::consts::FRAC_PI_2
}

pub fn ppc_q3_fall_off_type_valid(value: u32) -> bool {
    value <= PPC_Q3_FALL_OFF_TYPE_COSINE
}

pub fn ppc_q3_shader_set_uv_transform(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
) -> bool {
    let shader = cpu.gpr[3];
    let matrix_ptr = cpu.gpr[4];
    if matrix_ptr == 0 {
        return false;
    }
    if !ppc_q3_validate_shader_handle(q3_objects, q3_error_state, shader) {
        return false;
    }
    let Some(matrix) = ppc_read_q3_matrix3x3(memory, matrix_ptr) else {
        return false;
    };
    if let Some(record) = q3_shader_uv_transforms
        .iter_mut()
        .find(|record| record.shader == shader)
    {
        record.matrix = matrix;
    } else {
        q3_shader_uv_transforms.push(PpcQ3ShaderUvTransformRecord { shader, matrix });
    }
    true
}

pub fn ppc_q3_shader_get_uv_transform(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
) -> bool {
    let shader = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX3X3_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_shader_handle(q3_objects, q3_error_state, shader) {
        return false;
    }
    let matrix = q3_shader_uv_transforms
        .iter()
        .find(|record| record.shader == shader)
        .map(|record| record.matrix)
        .unwrap_or_else(ppc_q3_matrix3x3_identity);
    ppc_write_q3_matrix3x3(memory, matrix_out_ptr, &matrix).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQ3ShaderBoundaryAxis {
    U,
    V,
}

pub fn ppc_q3_shader_set_boundary(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    axis: PpcQ3ShaderBoundaryAxis,
) -> bool {
    let shader = cpu.gpr[3];
    let boundary = cpu.gpr[4];
    if !ppc_q3_shader_boundary_valid(boundary) {
        return false;
    }
    if !ppc_q3_validate_shader_handle(q3_objects, q3_error_state, shader) {
        return false;
    }
    let record = ppc_q3_shader_boundary_record_mut(q3_shader_boundaries, shader);
    match axis {
        PpcQ3ShaderBoundaryAxis::U => record.u_boundary = boundary,
        PpcQ3ShaderBoundaryAxis::V => record.v_boundary = boundary,
    }
    true
}

pub fn ppc_q3_shader_get_boundary(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    axis: PpcQ3ShaderBoundaryAxis,
) -> bool {
    let shader = cpu.gpr[3];
    let boundary_out_ptr = cpu.gpr[4];
    if boundary_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, boundary_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_shader_handle(q3_objects, q3_error_state, shader) {
        return false;
    }
    let record = q3_shader_boundaries
        .iter()
        .find(|record| record.shader == shader)
        .copied()
        .unwrap_or_else(|| PpcQ3ShaderBoundaryRecord::new(shader));
    let boundary = match axis {
        PpcQ3ShaderBoundaryAxis::U => record.u_boundary,
        PpcQ3ShaderBoundaryAxis::V => record.v_boundary,
    };
    memory.write_u32_be(boundary_out_ptr, boundary).is_some()
}

pub fn ppc_q3_shader_boundary_record_mut(
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    shader: u32,
) -> &mut PpcQ3ShaderBoundaryRecord {
    if let Some(index) = q3_shader_boundaries
        .iter()
        .position(|record| record.shader == shader)
    {
        return &mut q3_shader_boundaries[index];
    }
    q3_shader_boundaries.push(PpcQ3ShaderBoundaryRecord::new(shader));
    q3_shader_boundaries
        .last_mut()
        .expect("just pushed Q3 shader boundary record")
}

pub fn ppc_q3_shader_boundary_valid(boundary: u32) -> bool {
    matches!(
        boundary,
        PPC_Q3_SHADER_UV_BOUNDARY_WRAP | PPC_Q3_SHADER_UV_BOUNDARY_CLAMP
    )
}

pub fn ppc_q3_validate_shader_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    shader: u32,
) -> bool {
    if ppc_q3_object_type_is_shader(ppc_q3_object_type_for_handle(q3_objects, shader)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_style_kind_for_type(object_type: u32) -> Option<PpcQ3StyleKind> {
    match object_type {
        PPC_Q3_STYLE_TYPE_BACKFACING => Some(PpcQ3StyleKind::Backfacing),
        PPC_Q3_STYLE_TYPE_INTERPOLATION => Some(PpcQ3StyleKind::Interpolation),
        PPC_Q3_STYLE_TYPE_FILL => Some(PpcQ3StyleKind::Fill),
        PPC_Q3_STYLE_TYPE_ORIENTATION => Some(PpcQ3StyleKind::Orientation),
        _ => None,
    }
}

pub fn ppc_q3_style_new(
    cpu: &PpcCpu,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    kind: PpcQ3StyleKind,
    object_type: u32,
) -> u32 {
    let value = cpu.gpr[3];
    if !ppc_q3_style_value_valid(kind, value) {
        return 0;
    }
    let style = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        object_type,
        0,
        0,
    );
    if style != 0 {
        ppc_q3_style_store_data(q3_styles, style, kind, value);
    }
    style
}

pub fn ppc_q3_style_store_data(
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    style: u32,
    kind: PpcQ3StyleKind,
    value: u32,
) {
    if style == 0 || !ppc_q3_style_value_valid(kind, value) {
        return;
    }
    if let Some(record) = q3_styles
        .iter_mut()
        .find(|record| record.style == style && record.kind == kind)
    {
        record.value = value;
    } else {
        q3_styles.push(PpcQ3StyleRecord { style, kind, value });
    }
}

pub fn ppc_q3_style_get(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_styles: &[PpcQ3StyleRecord],
    kind: PpcQ3StyleKind,
) -> bool {
    let style = cpu.gpr[3];
    let value_out_ptr = cpu.gpr[4];
    if value_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, value_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_style_handle(q3_objects, q3_error_state, style, kind) {
        return false;
    }
    let Some(record) = q3_styles
        .iter()
        .find(|record| record.style == style && record.kind == kind)
    else {
        return false;
    };
    memory.write_u32_be(value_out_ptr, record.value).is_some()
}

pub fn ppc_q3_style_set(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_styles: &mut [PpcQ3StyleRecord],
    kind: PpcQ3StyleKind,
) -> bool {
    let style = cpu.gpr[3];
    let value = cpu.gpr[4];
    if !ppc_q3_style_value_valid(kind, value) {
        return false;
    }
    if !ppc_q3_validate_style_handle(q3_objects, q3_error_state, style, kind) {
        return false;
    }
    let Some(record) = q3_styles
        .iter_mut()
        .find(|record| record.style == style && record.kind == kind)
    else {
        return false;
    };
    record.value = value;
    true
}

pub fn ppc_q3_validate_style_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    style: u32,
    kind: PpcQ3StyleKind,
) -> bool {
    if ppc_q3_style_kind_for_type(ppc_q3_object_type_for_handle(q3_objects, style)) == Some(kind) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_style_value_valid(kind: PpcQ3StyleKind, value: u32) -> bool {
    match kind {
        PpcQ3StyleKind::Backfacing => value <= PPC_Q3_BACKFACING_STYLE_FLIP,
        PpcQ3StyleKind::Interpolation => value <= PPC_Q3_INTERPOLATION_STYLE_PIXEL,
        PpcQ3StyleKind::Fill => value <= PPC_Q3_FILL_STYLE_POINTS,
        PpcQ3StyleKind::Orientation => value <= PPC_Q3_ORIENTATION_STYLE_CLOCKWISE,
    }
}

pub fn ppc_q3_attribute_set_add(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> bool {
    let attribute_set = cpu.gpr[3];
    let attribute_type = cpu.gpr[4];
    let data_ptr = cpu.gpr[5];
    if data_ptr == 0 {
        return false;
    }
    let Some(data_size) = ppc_q3_attribute_data_size(attribute_type) else {
        return false;
    };
    let Some(data) = ppc_q3_read_bytes(memory, data_ptr, data_size) else {
        return false;
    };
    if !ppc_q3_validate_attribute_set_handle(q3_objects, q3_error_state, attribute_set) {
        return false;
    }
    let Some(surface_shader) =
        ppc_q3_attribute_surface_shader_handle_for_data(attribute_type, &data)
    else {
        ppc_q3_attribute_store_data(q3_attributes, attribute_set, attribute_type, data);
        return true;
    };
    if !ppc_q3_attribute_surface_shader_accepts_object(q3_objects, q3_error_state, surface_shader) {
        return false;
    }
    let old_surface_shader = q3_attributes
        .iter()
        .find(|record| {
            record.attribute_set == attribute_set && record.attribute_type == attribute_type
        })
        .and_then(ppc_q3_attribute_surface_shader_handle)
        .unwrap_or(0);
    if old_surface_shader == surface_shader {
        ppc_q3_attribute_store_data(q3_attributes, attribute_set, attribute_type, data);
        return true;
    }
    let surface_shader_exists = ppc_q3_object_exists(q3_objects, surface_shader);
    let retained = ppc_q3_object_retain(q3_objects, q3_object_refs, surface_shader);
    ppc_q3_attribute_store_data(q3_attributes, attribute_set, attribute_type, data);
    if old_surface_shader != 0 && old_surface_shader != surface_shader {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, old_surface_shader);
    }
    retained || !surface_shader_exists
}

pub fn ppc_q3_attribute_store_data(
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    attribute_set: u32,
    attribute_type: u32,
    data: Vec<u8>,
) {
    if let Some(record) = q3_attributes.iter_mut().find(|record| {
        record.attribute_set == attribute_set && record.attribute_type == attribute_type
    }) {
        record.data = data;
    } else {
        q3_attributes.push(PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        });
    }
}

pub fn ppc_q3_attribute_set_get(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &[PpcQ3AttributeRecord],
) -> bool {
    let attribute_set = cpu.gpr[3];
    let attribute_type = cpu.gpr[4];
    let data_out_ptr = cpu.gpr[5];
    let Some(data_size) = ppc_q3_attribute_data_size(attribute_type) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Q3AttributeSet_Get failed reason=unknown-size set=${:08X} type={} out=${:08X}",
                attribute_set, attribute_type, data_out_ptr
            );
        }
        return false;
    };
    if data_out_ptr != 0 && !ppc_memory_can_write_bytes(memory, data_out_ptr, data_size) {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Q3AttributeSet_Get failed reason=bad-out set=${:08X} type={} out=${:08X} size={}",
                attribute_set, attribute_type, data_out_ptr, data_size
            );
        }
        return false;
    }
    if !ppc_q3_validate_attribute_set_handle(q3_objects, q3_error_state, attribute_set) {
        if ppc_hle_trace_enabled() {
            let actual_type = ppc_q3_object_type_for_handle(q3_objects, attribute_set);
            eprintln!(
                "[PPC-TRACE] Q3AttributeSet_Get failed reason=bad-set set=${:08X} set_type='{}' type={} out=${:08X}",
                attribute_set,
                format_ppc_fourcc(actual_type),
                attribute_type,
                data_out_ptr
            );
        }
        return false;
    }
    let Some(record) = q3_attributes.iter().find(|record| {
        record.attribute_set == attribute_set && record.attribute_type == attribute_type
    }) else {
        if ppc_hle_trace_enabled() {
            let known_types: Vec<u32> = q3_attributes
                .iter()
                .filter(|record| record.attribute_set == attribute_set)
                .map(|record| record.attribute_type)
                .collect();
            eprintln!(
                "[PPC-TRACE] Q3AttributeSet_Get failed reason=missing-record set=${:08X} type={} out=${:08X} known_types={:?}",
                attribute_set, attribute_type, data_out_ptr, known_types
            );
        }
        return false;
    };
    if data_out_ptr == 0 {
        return true;
    }
    if record.data.len() != data_size as usize {
        return false;
    }
    let surface_shader = ppc_q3_attribute_surface_shader_handle(record);
    if let Some(surface_shader) = surface_shader {
        if !ppc_q3_attribute_surface_shader_accepts_object(
            q3_objects,
            q3_error_state,
            surface_shader,
        ) {
            return false;
        }
    }
    if !ppc_q3_write_bytes(memory, data_out_ptr, &record.data) {
        return false;
    }
    if let Some(surface_shader) = surface_shader {
        let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, surface_shader);
    }
    true
}

pub fn ppc_q3_attribute_set_clear(
    cpu: &PpcCpu,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> bool {
    let attribute_set = cpu.gpr[3];
    let attribute_type = cpu.gpr[4];
    if attribute_type == PPC_Q3_ATTRIBUTE_TYPE_NONE {
        return false;
    }
    if !ppc_q3_validate_attribute_set_handle(q3_objects, q3_error_state, attribute_set) {
        return false;
    }
    let released_surface_shaders: Vec<u32> = q3_attributes
        .iter()
        .filter(|record| {
            record.attribute_set == attribute_set && record.attribute_type == attribute_type
        })
        .filter_map(ppc_q3_attribute_surface_shader_handle)
        .collect();
    q3_attributes.retain(|record| {
        !(record.attribute_set == attribute_set && record.attribute_type == attribute_type)
    });
    if !released_surface_shaders.is_empty() {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        for surface_shader in released_surface_shaders {
            let _ = ppc_q3_object_release_reference(&mut stores, surface_shader);
        }
    }
    true
}

pub fn ppc_q3_attribute_set_contains(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &[PpcQ3AttributeRecord],
) -> bool {
    let attribute_set = cpu.gpr[3];
    let attribute_type = cpu.gpr[4];
    if attribute_type == PPC_Q3_ATTRIBUTE_TYPE_NONE {
        return false;
    }
    if !ppc_q3_validate_attribute_set_handle(q3_objects, q3_error_state, attribute_set) {
        return false;
    }
    q3_attributes.iter().any(|record| {
        record.attribute_set == attribute_set && record.attribute_type == attribute_type
    })
}

pub fn ppc_q3_attribute_set_get_next_attribute_type(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_attributes: &[PpcQ3AttributeRecord],
) -> bool {
    let attribute_set = cpu.gpr[3];
    let type_ptr = cpu.gpr[4];
    if type_ptr == 0 || !ppc_memory_can_write_bytes(memory, type_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_attribute_set_handle(q3_objects, q3_error_state, attribute_set) {
        return false;
    }
    let Some(current_type) = memory.read_u32_be(type_ptr) else {
        return false;
    };
    let next_type = q3_attributes
        .iter()
        .filter(|record| {
            record.attribute_set == attribute_set && record.attribute_type > current_type
        })
        .map(|record| record.attribute_type)
        .min()
        .unwrap_or(PPC_Q3_ATTRIBUTE_TYPE_NONE);
    memory.write_u32_be(type_ptr, next_type).is_some()
}

pub fn ppc_q3_validate_attribute_set_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    attribute_set: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, attribute_set) == PPC_Q3_TYPE_ATTRIBUTE_SET {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_attribute_data_size(attribute_type: u32) -> Option<u32> {
    match attribute_type {
        PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV | PPC_Q3_ATTRIBUTE_TYPE_SHADING_UV => Some(8),
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL
        | PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR
        | PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR
        | PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR => Some(12),
        PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT
        | PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL
        | PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE
        | PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER => Some(4),
        PPC_Q3_ATTRIBUTE_TYPE_SURFACE_TANGENT => Some(24),
        _ => None,
    }
}

pub fn ppc_q3_object_exists(q3_objects: &[PpcQ3ObjectRecord], object: u32) -> bool {
    object != 0 && q3_objects.iter().any(|record| record.object == object)
}

pub struct PpcQ3ObjectStores<'a> {
    pub q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub q3_object_refs: &'a mut Vec<PpcQ3ObjectReferenceRecord>,
    pub q3_renderer_preferences: &'a mut Vec<PpcQ3RendererPreferenceRecord>,
    pub q3_files: &'a mut Vec<PpcQ3FileRecord>,
    pub q3_group_memberships: &'a mut Vec<PpcQ3GroupMembershipRecord>,
    pub q3_file_groups: &'a mut Vec<PpcQ3FileGroupRecord>,
    pub q3_views: &'a mut Vec<PpcQ3ViewStateRecord>,
    pub q3_submissions: &'a mut Vec<PpcQ3SubmissionRecord>,
    pub q3_view_transforms: &'a mut Vec<PpcQ3ViewTransformRecord>,
    pub q3_submission_transforms: &'a mut Vec<PpcQ3SubmissionTransformRecord>,
    pub q3_view_materials: &'a mut Vec<PpcQ3ViewMaterialRecord>,
    pub q3_submission_materials: &'a mut Vec<PpcQ3SubmissionMaterialRecord>,
    pub q3_submission_lights: &'a mut Vec<PpcQ3SubmissionLightRecord>,
    pub q3_view_state_stack: &'a mut Vec<PpcQ3ViewStateSnapshotRecord>,
    pub q3_completed_frames: &'a mut Vec<PpcQ3CompletedFrameRecord>,
    pub q3_retained_frames: &'a mut Vec<PpcQ3RetainedFrameRecord>,
    pub q3_fog_styles: &'a mut Vec<PpcQ3FogStyleRecord>,
    pub q3_memory_storages: &'a mut Vec<PpcQ3MemoryStorageRecord>,
    pub q3_attributes: &'a mut Vec<PpcQ3AttributeRecord>,
    pub q3_shader_uv_transforms: &'a mut Vec<PpcQ3ShaderUvTransformRecord>,
    pub q3_shader_boundaries: &'a mut Vec<PpcQ3ShaderBoundaryRecord>,
    pub q3_mipmap_textures: &'a mut Vec<PpcQ3MipmapTextureRecord>,
    pub q3_texture_shaders: &'a mut Vec<PpcQ3TextureShaderRecord>,
    pub q3_draw_contexts: &'a mut Vec<PpcQ3DrawContextRecord>,
    pub q3_trimeshes: &'a mut Vec<PpcQ3TriMeshRecord>,
    pub q3_styles: &'a mut Vec<PpcQ3StyleRecord>,
    pub q3_cameras: &'a mut Vec<PpcQ3CameraRecord>,
    pub q3_lights: &'a mut Vec<PpcQ3LightRecord>,
}

pub fn ppc_q3_object_reference_count(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &[PpcQ3ObjectReferenceRecord],
    object: u32,
) -> Option<u32> {
    if !ppc_q3_object_exists(q3_objects, object) {
        return None;
    }
    Some(
        q3_object_refs
            .iter()
            .find(|record| record.object == object)
            .map(|record| record.ref_count)
            .unwrap_or(1),
    )
}

pub fn ppc_q3_object_set_reference_count(
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    object: u32,
    ref_count: u32,
) {
    if ref_count <= 1 {
        q3_object_refs.retain(|record| record.object != object);
        return;
    }
    if let Some(record) = q3_object_refs
        .iter_mut()
        .find(|record| record.object == object)
    {
        record.ref_count = ref_count;
    } else {
        q3_object_refs.push(PpcQ3ObjectReferenceRecord { object, ref_count });
    }
}

pub fn ppc_q3_object_retain(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    object: u32,
) -> bool {
    let Some(ref_count) = ppc_q3_object_reference_count(q3_objects, q3_object_refs, object) else {
        return false;
    };
    ppc_q3_object_set_reference_count(q3_object_refs, object, ref_count.saturating_add(1));
    true
}

pub fn ppc_q3_object_release_reference(stores: &mut PpcQ3ObjectStores<'_>, object: u32) -> bool {
    let Some(ref_count) =
        ppc_q3_object_reference_count(stores.q3_objects, stores.q3_object_refs, object)
    else {
        stores
            .q3_object_refs
            .retain(|record| record.object != object);
        return false;
    };
    if ref_count > 1 {
        ppc_q3_object_set_reference_count(stores.q3_object_refs, object, ref_count - 1);
        return true;
    }
    ppc_q3_object_dispose_unreferenced(stores, object);
    true
}

pub fn ppc_q3_object_dispose_unreferenced(stores: &mut PpcQ3ObjectStores<'_>, object: u32) {
    let released_group_members: Vec<u32> = stores
        .q3_group_memberships
        .iter()
        .filter(|record| record.group == object)
        .map(|record| record.object)
        .collect();
    let released_view_slots: Vec<u32> = stores
        .q3_views
        .iter()
        .filter(|record| record.view == object)
        .flat_map(|record| {
            [
                record.renderer,
                record.light_group,
                record.draw_context,
                record.camera,
            ]
        })
        .filter(|referenced_object| *referenced_object != 0)
        .collect();
    let released_texture_shader_textures: Vec<u32> = stores
        .q3_texture_shaders
        .iter()
        .filter(|record| record.shader == object && record.texture != object)
        .map(|record| record.texture)
        .collect();
    let released_mipmap_texture_storages: Vec<u32> = stores
        .q3_mipmap_textures
        .iter()
        .filter(|record| record.texture == object)
        .map(|record| ppc_q3_mipmap_texture_image_storage(&record.mipmap))
        .filter(|storage| {
            *storage != 0
                && *storage != object
                && ppc_q3_mipmap_image_storage_is_known_storage(stores.q3_objects, *storage)
        })
        .collect();
    let released_file_storages: Vec<u32> = stores
        .q3_files
        .iter()
        .filter(|record| record.file == object && record.storage != object)
        .map(|record| record.storage)
        .collect();
    let released_attribute_surface_shaders: Vec<u32> = stores
        .q3_attributes
        .iter()
        .filter(|record| record.attribute_set == object)
        .filter_map(ppc_q3_attribute_surface_shader_handle)
        .collect();
    let released_trimesh_attribute_sets: Vec<u32> = stores
        .q3_trimeshes
        .iter()
        .filter(|record| record.trimesh == object)
        .flat_map(|record| record.triangle_attribute_sets.iter().copied())
        .collect();

    stores.q3_objects.retain(|record| record.object != object);
    stores
        .q3_object_refs
        .retain(|record| record.object != object);
    stores
        .q3_renderer_preferences
        .retain(|record| record.renderer != object);
    stores
        .q3_files
        .retain(|record| record.file != object && record.storage != object);
    for file_record in stores.q3_files.iter_mut() {
        if file_record.read_object == object {
            file_record.read_object = 0;
        }
    }
    stores
        .q3_group_memberships
        .retain(|record| record.group != object && record.object != object);
    // Objects read from a metafile retain their group hierarchy after the file object closes.
    // Drop group metadata only when the group itself (or its parent) is disposed.
    stores
        .q3_file_groups
        .retain(|record| record.group != object && record.parent_group != object);
    for view in stores.q3_views.iter_mut() {
        if view.renderer == object {
            view.renderer = 0;
        }
        if view.light_group == object {
            view.light_group = 0;
        }
        if view.draw_context == object {
            view.draw_context = 0;
        }
        if view.camera == object {
            view.camera = 0;
        }
    }
    stores.q3_views.retain(|record| record.view != object);
    stores.q3_submissions.retain(|record| {
        record.view != object && record.primary != object && record.secondary != object
    });
    stores
        .q3_view_transforms
        .retain(|record| record.view != object);
    stores.q3_submission_transforms.retain(|record| {
        record.view != object && record.primary != object && record.secondary != object
    });
    stores
        .q3_view_materials
        .retain(|record| record.view != object);
    for record in stores.q3_view_materials.iter_mut() {
        if record.shader == object {
            record.shader = 0;
        }
        record.styles.retain(|style| style.style != object);
        record
            .attributes
            .retain(|attribute| attribute.attribute_set != object);
    }
    stores.q3_submission_materials.retain(|record| {
        record.view != object
            && record.primary != object
            && record.secondary != object
            && record.shader != object
            && !record.styles.iter().any(|style| style.style == object)
            && !record
                .shader_uv_transform
                .map(|transform| transform.shader == object)
                .unwrap_or(false)
            && !record
                .shader_boundary
                .map(|boundary| boundary.shader == object)
                .unwrap_or(false)
            && !record
                .texture_shader
                .map(|shader| shader.shader == object || shader.texture == object)
                .unwrap_or(false)
            && !record
                .mipmap_texture
                .as_ref()
                .map(|texture| texture.texture == object)
                .unwrap_or(false)
            && !record
                .attributes
                .iter()
                .any(|attribute| attribute.attribute_set == object)
    });
    stores.q3_submission_lights.retain(|record| {
        record.view != object
            && record.primary != object
            && record.secondary != object
            && record.light_group != object
            && !record.lights.iter().any(|light| light.light == object)
    });
    stores
        .q3_view_state_stack
        .retain(|record| !ppc_q3_view_state_snapshot_references_object(record, object));
    stores
        .q3_completed_frames
        .retain(|frame| !ppc_q3_completed_frame_references_object(frame, object));
    stores
        .q3_retained_frames
        .retain(|record| !ppc_q3_completed_frame_references_object(&record.frame, object));
    stores.q3_fog_styles.retain(|record| record.view != object);
    stores
        .q3_memory_storages
        .retain(|record| record.storage != object);
    stores.q3_attributes.retain(|record| {
        record.attribute_set != object
            && ppc_q3_attribute_surface_shader_handle(record) != Some(object)
    });
    stores
        .q3_shader_uv_transforms
        .retain(|record| record.shader != object);
    stores
        .q3_shader_boundaries
        .retain(|record| record.shader != object);
    stores
        .q3_mipmap_textures
        .retain(|record| record.texture != object);
    stores
        .q3_texture_shaders
        .retain(|record| record.shader != object);
    stores
        .q3_draw_contexts
        .retain(|record| record.draw_context != object);
    stores
        .q3_trimeshes
        .retain(|record| record.trimesh != object);
    stores.q3_styles.retain(|record| record.style != object);
    stores.q3_cameras.retain(|record| record.camera != object);
    stores.q3_lights.retain(|record| record.light != object);

    for referenced_object in released_group_members
        .into_iter()
        .chain(released_view_slots)
        .chain(released_texture_shader_textures)
        .chain(released_mipmap_texture_storages)
        .chain(released_file_storages)
        .chain(released_attribute_surface_shaders)
        .chain(released_trimesh_attribute_sets)
        .filter(|referenced_object| *referenced_object != object)
    {
        let _ = ppc_q3_object_release_reference(stores, referenced_object);
    }
}

pub fn ppc_q3_read_bytes(
    memory: &mut PpcSectionMem,
    source_ptr: u32,
    byte_count: u32,
) -> Option<Vec<u8>> {
    let mut data = Vec::with_capacity(byte_count as usize);
    for offset in 0..byte_count {
        data.push(memory.read_u8(source_ptr.checked_add(offset)?)?);
    }
    Some(data)
}

pub fn ppc_q3_write_bytes(memory: &mut PpcSectionMem, dest_ptr: u32, data: &[u8]) -> bool {
    for (offset, byte) in data.iter().copied().enumerate() {
        let Ok(offset) = u32::try_from(offset) else {
            return false;
        };
        let Some(addr) = dest_ptr.checked_add(offset) else {
            return false;
        };
        if memory.write_u8(addr, byte).is_none() {
            return false;
        }
    }
    true
}

pub(crate) fn ppc_q3_memory_storage_new(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> u32 {
    let source_ptr = cpu.gpr[3];
    let requested_size = cpu.gpr[4];
    let buffer_size = if source_ptr == 0 {
        requested_size.max(PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE)
    } else {
        requested_size
    };
    let valid_size = if source_ptr == 0 { 0 } else { requested_size };
    let Some(buffer_ptr) = ppc_q3_allocate_owned_storage_buffer(
        process_memory_manager,
        memory,
        heap_cursor,
        buffer_size,
        last_mem_error,
    ) else {
        return 0;
    };
    if source_ptr != 0
        && valid_size != 0
        && !ppc_q3_copy_or_zero(memory, source_ptr, buffer_ptr, valid_size)
    {
        return 0;
    }
    ppc_q3_memory_storage_create(
        q3_objects,
        next_q3_object,
        q3_memory_storages,
        buffer_ptr,
        valid_size,
        buffer_size,
        true,
    )
}

pub(crate) fn ppc_q3_memory_storage_new_buffer(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> u32 {
    let buffer_ptr = cpu.gpr[3];
    let valid_size = cpu.gpr[4];
    let buffer_size = cpu.gpr[5];
    if buffer_ptr == 0 {
        let allocated_size = buffer_size
            .max(valid_size)
            .max(PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE);
        let Some(allocated_ptr) = ppc_q3_allocate_owned_storage_buffer(
            process_memory_manager,
            memory,
            heap_cursor,
            allocated_size,
            last_mem_error,
        ) else {
            return 0;
        };
        return ppc_q3_memory_storage_create(
            q3_objects,
            next_q3_object,
            q3_memory_storages,
            allocated_ptr,
            0,
            allocated_size,
            true,
        );
    }
    if valid_size > buffer_size {
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    ppc_q3_memory_storage_create(
        q3_objects,
        next_q3_object,
        q3_memory_storages,
        buffer_ptr,
        valid_size,
        buffer_size,
        false,
    )
}

pub(crate) fn ppc_q3_fsspec_storage_new(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> u32 {
    let spec_ptr = cpu.gpr[3];
    let Ok(path) = ppc_existing_path_for_fsspec(memory, vfs_directories, vfs_files, &[], spec_ptr)
    else {
        *last_mem_error = PPC_NO_ERR;
        return 0;
    };
    let Some(file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    else {
        *last_mem_error = PPC_NO_ERR;
        return 0;
    };
    let converted = file
        .data
        .starts_with(b"3DMetafile")
        .then(|| qd3d_text::to_binary(&file.data))
        .flatten();
    let data = converted.as_deref().unwrap_or(&file.data);
    let Ok(data_size) = u32::try_from(data.len()) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    let Some(buffer_ptr) = ppc_q3_allocate_owned_storage_buffer(
        process_memory_manager,
        memory,
        heap_cursor,
        data_size,
        last_mem_error,
    ) else {
        return 0;
    };
    if data_size != 0 && !ppc_q3_write_bytes(memory, buffer_ptr, data) {
        return 0;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3FSSpecStorage_New path=\"{}\" data_size={}",
            file.path,
            data.len()
        );
    }
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_STORAGE_TYPE_MACINTOSH,
        buffer_ptr,
        data_size,
    )
}

pub fn ppc_q3_memory_storage_create(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    buffer_ptr: u32,
    valid_size: u32,
    buffer_size: u32,
    owns_buffer: bool,
) -> u32 {
    let storage = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::MemoryStorage,
        PPC_Q3_STORAGE_TYPE_MEMORY,
        buffer_ptr,
        valid_size,
    );
    if storage != 0 {
        q3_memory_storages.push(PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr,
            valid_size,
            buffer_size,
            owns_buffer,
        });
    }
    storage
}

pub(crate) fn ppc_q3_memory_storage_set(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_files: &[PpcQ3FileRecord],
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> bool {
    let storage = cpu.gpr[3];
    let source_ptr = cpu.gpr[4];
    let requested_size = cpu.gpr[5];
    if !ppc_q3_validate_memory_storage_handle(
        q3_objects,
        q3_error_state,
        q3_memory_storages,
        storage,
    ) {
        return false;
    }
    if ppc_q3_storage_is_open(q3_files, storage) {
        return false;
    }
    let buffer_size = if source_ptr == 0 {
        requested_size.max(PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE)
    } else {
        requested_size
    };
    let valid_size = if source_ptr == 0 { 0 } else { requested_size };
    let Some(buffer_ptr) = ppc_q3_allocate_owned_storage_buffer(
        process_memory_manager,
        memory,
        heap_cursor,
        buffer_size,
        last_mem_error,
    ) else {
        return false;
    };
    if source_ptr != 0
        && valid_size != 0
        && !ppc_q3_copy_or_zero(memory, source_ptr, buffer_ptr, valid_size)
    {
        return false;
    }
    ppc_q3_memory_storage_update(
        q3_objects,
        q3_memory_storages,
        PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr,
            valid_size,
            buffer_size,
            owns_buffer: true,
        },
    )
}

pub fn ppc_q3_memory_storage_get_buffer(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
) -> bool {
    let storage = cpu.gpr[3];
    let buffer_out_ptr = cpu.gpr[4];
    let valid_size_out_ptr = cpu.gpr[5];
    let buffer_size_out_ptr = cpu.gpr[6];
    if buffer_out_ptr == 0
        || valid_size_out_ptr == 0
        || buffer_size_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, buffer_out_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, valid_size_out_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, buffer_size_out_ptr, 4)
    {
        return false;
    }
    if !ppc_q3_validate_memory_storage_handle(
        q3_objects,
        q3_error_state,
        q3_memory_storages,
        storage,
    ) {
        return false;
    }
    let Some(record) = q3_memory_storages
        .iter()
        .find(|record| record.storage == storage)
    else {
        return false;
    };
    memory
        .write_u32_be(buffer_out_ptr, record.buffer_ptr)
        .is_some()
        && memory
            .write_u32_be(valid_size_out_ptr, record.valid_size)
            .is_some()
        && memory
            .write_u32_be(buffer_size_out_ptr, record.buffer_size)
            .is_some()
}

pub(crate) fn ppc_q3_memory_storage_set_buffer(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_files: &[PpcQ3FileRecord],
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> bool {
    let storage = cpu.gpr[3];
    let buffer_ptr = cpu.gpr[4];
    let valid_size = cpu.gpr[5];
    let buffer_size = cpu.gpr[6];
    if !ppc_q3_validate_memory_storage_handle(
        q3_objects,
        q3_error_state,
        q3_memory_storages,
        storage,
    ) {
        return false;
    }
    if ppc_q3_storage_is_open(q3_files, storage) {
        return false;
    }
    if buffer_ptr == 0 {
        let allocated_size = buffer_size
            .max(valid_size)
            .max(PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE);
        let Some(allocated_ptr) = ppc_q3_allocate_owned_storage_buffer(
            process_memory_manager,
            memory,
            heap_cursor,
            allocated_size,
            last_mem_error,
        ) else {
            return false;
        };
        return ppc_q3_memory_storage_update(
            q3_objects,
            q3_memory_storages,
            PpcQ3MemoryStorageRecord {
                storage,
                buffer_ptr: allocated_ptr,
                valid_size: 0,
                buffer_size: allocated_size,
                owns_buffer: true,
            },
        );
    }
    if valid_size > buffer_size {
        return false;
    }
    *last_mem_error = PPC_NO_ERR;
    ppc_q3_memory_storage_update(
        q3_objects,
        q3_memory_storages,
        PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr,
            valid_size,
            buffer_size,
            owns_buffer: false,
        },
    )
}

pub fn ppc_q3_memory_storage_get_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
) -> u32 {
    if ppc_q3_validate_memory_storage_handle(
        q3_objects,
        q3_error_state,
        q3_memory_storages,
        cpu.gpr[3],
    ) {
        PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE
    } else {
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_validate_memory_storage_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    storage: u32,
) -> bool {
    let object_type = ppc_q3_object_type_for_handle(q3_objects, storage);
    if matches!(
        object_type,
        PPC_Q3_STORAGE_TYPE_MEMORY | PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE
    ) && ppc_q3_memory_storage_exists(q3_memory_storages, storage)
    {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_storage_get_type(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    let storage = cpu.gpr[3];
    let object_type = ppc_q3_object_type_for_handle(q3_objects, storage);
    if ppc_q3_object_type_is_storage(object_type) {
        object_type
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        PPC_Q3_TYPE_NONE
    }
}

pub fn ppc_q3_validate_storage_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    storage: u32,
) -> bool {
    if ppc_q3_object_type_is_storage(ppc_q3_object_type_for_handle(q3_objects, storage)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_storage_get_size(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    storage: u32,
    size_out_ptr: u32,
) -> bool {
    if size_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, size_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, storage) {
        return false;
    }
    let size = q3_memory_storages
        .iter()
        .find(|record| record.storage == storage)
        .map(|record| record.valid_size)
        .or_else(|| {
            q3_objects
                .iter()
                .find(|record| record.object == storage)
                .map(|record| record.data_size)
        })
        .unwrap_or(0);
    memory.write_u32_be(size_out_ptr, size).is_some()
}

pub fn ppc_q3_storage_get_data(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    storage: u32,
    offset: u32,
    requested_size: u32,
    dest_ptr: u32,
    size_read_ptr: u32,
) -> bool {
    if size_read_ptr == 0 || !ppc_memory_can_write_bytes(memory, size_read_ptr, 4) {
        return false;
    }
    let side_record = q3_memory_storages
        .iter()
        .find(|record| record.storage == storage);
    let (source_ptr, available_size) = side_record
        .map(|record| (record.buffer_ptr, record.valid_size))
        .or_else(|| {
            q3_objects
                .iter()
                .find(|record| record.object == storage)
                .map(|record| (record.data_ptr, record.data_size))
        })
        .unwrap_or((0, 0));
    let readable_size = available_size.saturating_sub(offset).min(requested_size);
    if readable_size != 0 && !ppc_memory_can_write_bytes(memory, dest_ptr, readable_size) {
        return false;
    }
    if !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, storage) {
        return false;
    }
    let slice_source_ptr = if readable_size == 0 {
        0
    } else {
        let Some(slice_source_ptr) = source_ptr.checked_add(offset) else {
            return false;
        };
        slice_source_ptr
    };
    if memory.write_u32_be(size_read_ptr, readable_size).is_none() {
        return false;
    }
    readable_size == 0 || ppc_q3_copy_or_zero(memory, slice_source_ptr, dest_ptr, readable_size)
}

pub(crate) fn ppc_q3_storage_set_data(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    storage: u32,
    offset: u32,
    requested_size: u32,
    source_ptr: u32,
    size_written_ptr: u32,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> bool {
    if size_written_ptr == 0 || !ppc_memory_can_write_bytes(memory, size_written_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, storage) {
        return false;
    }
    if memory
        .write_u32_be(size_written_ptr, requested_size)
        .is_none()
    {
        return false;
    }
    if let Some(side_index) = q3_memory_storages
        .iter()
        .position(|record| record.storage == storage)
    {
        let Some(end_offset) = offset.checked_add(requested_size) else {
            return false;
        };
        if end_offset > q3_memory_storages[side_index].buffer_size
            && !ppc_q3_memory_storage_grow_for_write(
                process_memory_manager,
                memory,
                q3_objects,
                q3_memory_storages,
                side_index,
                end_offset,
                heap_cursor,
                last_mem_error,
            )
        {
            return false;
        }
        let Some(dest_ptr) = q3_memory_storages[side_index]
            .buffer_ptr
            .checked_add(offset)
        else {
            return false;
        };
        for byte_offset in 0..requested_size {
            let byte = if source_ptr == 0 {
                0
            } else {
                memory.read_u8(source_ptr + byte_offset).unwrap_or(0)
            };
            if memory.write_u8(dest_ptr + byte_offset, byte).is_none() {
                return false;
            }
        }
        q3_memory_storages[side_index].valid_size =
            q3_memory_storages[side_index].valid_size.max(end_offset);
        ppc_q3_sync_memory_storage_object(q3_objects, q3_memory_storages[side_index]);
        return true;
    }
    let Some(record) = q3_objects
        .iter_mut()
        .find(|record| record.object == storage)
    else {
        return false;
    };
    let Some(dest_ptr) = record.data_ptr.checked_add(offset) else {
        return false;
    };
    for byte_offset in 0..requested_size {
        let byte = if source_ptr == 0 {
            0
        } else {
            memory.read_u8(source_ptr + byte_offset).unwrap_or(0)
        };
        if memory.write_u8(dest_ptr + byte_offset, byte).is_none() {
            return false;
        }
    }
    record.data_size = record.data_size.max(offset.saturating_add(requested_size));
    true
}

pub(crate) fn ppc_q3_allocate_owned_storage_buffer(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    buffer_size: u32,
    last_mem_error: &mut i16,
) -> Option<u32> {
    if buffer_size == 0 {
        *last_mem_error = PPC_NO_ERR;
        return Some(0);
    }
    let ptr = ppc_process_heap_alloc(
        process_memory_manager,
        memory,
        heap_cursor,
        buffer_size,
        true,
    );
    if ptr == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        None
    } else {
        *last_mem_error = PPC_NO_ERR;
        Some(ptr)
    }
}

pub fn ppc_q3_memory_storage_update(
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    new_record: PpcQ3MemoryStorageRecord,
) -> bool {
    let Some(index) = q3_memory_storages
        .iter()
        .position(|record| record.storage == new_record.storage)
    else {
        return false;
    };
    q3_memory_storages[index] = new_record;
    ppc_q3_sync_memory_storage_object(q3_objects, new_record)
}

pub fn ppc_q3_sync_memory_storage_object(
    q3_objects: &mut [PpcQ3ObjectRecord],
    storage_record: PpcQ3MemoryStorageRecord,
) -> bool {
    let Some(object_record) = q3_objects
        .iter_mut()
        .find(|record| record.object == storage_record.storage)
    else {
        return false;
    };
    object_record.kind = PpcQ3ObjectKind::MemoryStorage;
    object_record.object_type = PPC_Q3_STORAGE_TYPE_MEMORY;
    object_record.data_ptr = storage_record.buffer_ptr;
    object_record.data_size = storage_record.valid_size;
    true
}

pub fn ppc_q3_memory_storage_exists(
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    storage: u32,
) -> bool {
    storage != 0
        && q3_memory_storages
            .iter()
            .any(|record| record.storage == storage)
}

pub fn ppc_q3_storage_is_open(q3_files: &[PpcQ3FileRecord], storage: u32) -> bool {
    storage != 0
        && q3_files
            .iter()
            .any(|record| record.storage == storage && record.is_open)
}

pub(crate) fn ppc_q3_memory_storage_grow_for_write(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    side_index: usize,
    required_size: u32,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> bool {
    let old_record = q3_memory_storages[side_index];
    let grown_size = required_size
        .max(old_record.buffer_size.saturating_mul(2))
        .max(PPC_Q3_MEMORY_STORAGE_DEFAULT_BUFFER_SIZE);
    let Some(new_ptr) = ppc_q3_allocate_owned_storage_buffer(
        process_memory_manager,
        memory,
        heap_cursor,
        grown_size,
        last_mem_error,
    ) else {
        return false;
    };
    if old_record.valid_size != 0
        && !ppc_q3_copy_or_zero(
            memory,
            old_record.buffer_ptr,
            new_ptr,
            old_record.valid_size,
        )
    {
        return false;
    }
    q3_memory_storages[side_index].buffer_ptr = new_ptr;
    q3_memory_storages[side_index].buffer_size = grown_size;
    q3_memory_storages[side_index].owns_buffer = true;
    ppc_q3_sync_memory_storage_object(q3_objects, q3_memory_storages[side_index])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQ3ViewSlot {
    Renderer,
    LightGroup,
    DrawContext,
    Camera,
}

pub fn ppc_q3_file_mut(q3_files: &mut Vec<PpcQ3FileRecord>, file: u32) -> Option<&mut PpcQ3FileRecord> {
    if file == 0 {
        return None;
    }
    if let Some(index) = q3_files.iter().position(|record| record.file == file) {
        return q3_files.get_mut(index);
    }
    q3_files.push(PpcQ3FileRecord {
        file,
        storage: 0,
        is_open: false,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    q3_files.last_mut()
}

pub fn ppc_q3_validate_file_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    file: u32,
) -> bool {
    if q3_objects
        .iter()
        .any(|record| record.object == file && record.object_type == PPC_Q3_TYPE_FILE)
    {
        return true;
    }
    q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
    false
}

pub fn ppc_q3_file_set_storage(
    cpu: &PpcCpu,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
) -> bool {
    let file = cpu.gpr[3];
    let storage = cpu.gpr[4];
    if !ppc_q3_validate_file_handle(q3_objects, q3_error_state, file)
        || !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, storage)
    {
        return false;
    }
    let old_storage = q3_files
        .iter()
        .find(|record| record.file == file)
        .map(|record| record.storage)
        .unwrap_or(0);
    if old_storage == storage {
        return true;
    }
    let retained = ppc_q3_object_retain(q3_objects, q3_object_refs, storage);
    let Some(record) = ppc_q3_file_mut(q3_files, file) else {
        if retained {
            let mut stores = PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            };
            let _ = ppc_q3_object_release_reference(&mut stores, storage);
        }
        return false;
    };
    record.storage = storage;
    if old_storage != 0 {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, old_storage);
    }
    retained
}

pub fn ppc_q3_file_open_read(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_files: &mut Vec<PpcQ3FileRecord>,
) -> bool {
    let file = cpu.gpr[3];
    let object_type_out_ptr = cpu.gpr[4];
    if object_type_out_ptr != 0 && !ppc_memory_can_write_bytes(memory, object_type_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_file_handle(q3_objects, q3_error_state, file) {
        return false;
    }
    let Some(record) = ppc_q3_file_mut(q3_files, file) else {
        return false;
    };
    record.is_open = true;
    record.object_type = 0;
    record.read_offset = 0;
    record.read_object = 0;
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3File_OpenRead file=${:08X} storage=${:08X}",
            record.file, record.storage
        );
    }
    object_type_out_ptr == 0
        || memory
            .write_u32_be(object_type_out_ptr, record.object_type)
            .is_some()
}

pub(crate) fn ppc_q3_file_read_object(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    q3_error_state: &mut PpcQ3ErrorState,
    q3_files: &mut [PpcQ3FileRecord],
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
) -> u32 {
    let file = cpu.gpr[3];
    if !ppc_q3_validate_file_handle(q3_objects, q3_error_state, file) {
        return 0;
    }
    let Some(file_index) = q3_files.iter().position(|record| record.file == file) else {
        return 0;
    };
    let file_record = q3_files[file_index];
    if !file_record.is_open || file_record.storage == 0 {
        return 0;
    }
    if !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, file_record.storage) {
        return 0;
    }
    let Some(storage_record) = q3_objects
        .iter()
        .find(|record| record.object == file_record.storage)
        .copied()
    else {
        return 0;
    };
    let Some(chunk) =
        ppc_q3_file_next_object_chunk(memory, storage_record, file_record.read_offset)
    else {
        q3_files[file_index].read_offset = storage_record.data_size;
        return 0;
    };
    let Some(data_ptr) = storage_record.data_ptr.checked_add(chunk.offset) else {
        q3_files[file_index].read_offset = storage_record.data_size;
        return 0;
    };
    let object = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        chunk.object_type,
        data_ptr,
        chunk.total_size,
    );
    if object != 0 {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Q3File_ReadObject file=${:08X} object=${:08X} type='{}' offset={} body_size={} total_size={} parent='{}' depth={}",
                file,
                object,
                ppc_res_type_text(chunk.object_type),
                chunk.offset,
                chunk.body_size,
                chunk.total_size,
                ppc_res_type_text(chunk.parent_group_type),
                chunk.group_depth
            );
        }
        if let Some(record) = q3_objects.iter_mut().find(|record| record.object == object) {
            record.source = PpcQ3ObjectSource {
                file,
                offset: chunk.offset,
                parent_group_type: chunk.parent_group_type,
                group_depth: chunk.group_depth,
            };
        }
        let parent_group = ppc_q3_file_ensure_group_stack(
            q3_objects,
            next_q3_object,
            q3_group_memberships,
            q3_file_groups,
            file,
            storage_record,
            &chunk.group_stack,
        );
        if let Some(parent_group) = parent_group {
            ppc_q3_group_add_membership_once(q3_group_memberships, parent_group, object);
        }
        ppc_q3_file_record_object_body(
            process_memory_manager,
            memory,
            q3_objects,
            next_q3_object,
            object,
            chunk.object_type,
            data_ptr,
            chunk.body_size,
            heap_cursor,
            heap_limit,
            last_mem_error,
            q3_memory_storages,
            q3_trimeshes,
            q3_mipmap_textures,
            q3_styles,
        );
        if chunk.object_type == PPC_Q3_TYPE_CONTAINER {
            let container_state = ppc_q3_file_record_container_children(
                process_memory_manager,
                memory,
                q3_objects,
                next_q3_object,
                q3_memory_storages,
                q3_group_memberships,
                q3_attributes,
                q3_trimeshes,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_styles,
                heap_cursor,
                heap_limit,
                last_mem_error,
                file,
                storage_record,
                object,
                chunk.object_type,
                chunk.offset,
                chunk.body_size,
                chunk.group_depth,
            );
            if chunk.parent_group_type == PPC_Q3_TYPE_NONE && chunk.group_depth == 0 {
                if let Some(root_child) = container_state.root_child {
                    ppc_q3_file_promote_container_root_object(
                        q3_objects,
                        q3_attributes,
                        q3_trimeshes,
                        q3_mipmap_textures,
                        q3_texture_shaders,
                        q3_styles,
                        object,
                        root_child,
                    );
                }
            }
            if let Some(parent_group) = parent_group {
                ppc_q3_file_mirror_container_geometry_into_group(
                    q3_group_memberships,
                    q3_objects,
                    parent_group,
                    object,
                );
            }
        }
        q3_files[file_index].read_offset = chunk.next_offset;
        q3_files[file_index].read_object = object;
    }
    object
}

pub(crate) fn ppc_q3_file_record_object_body(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    object: u32,
    object_type: u32,
    data_ptr: u32,
    body_size: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
) {
    let Some(body_ptr) = data_ptr.checked_add(8) else {
        return;
    };
    if object_type == PPC_Q3_TYPE_TRIMESH {
        if let Some(data) = ppc_q3_file_decode_trimesh_3dmf_body(
            process_memory_manager,
            memory,
            body_ptr,
            body_size,
            heap_cursor,
            heap_limit,
            last_mem_error,
        )
        .or_else(|| {
            (body_size >= PPC_Q3_TRIMESH_DATA_SIZE)
                .then(|| ppc_q3_read_bytes(memory, body_ptr, PPC_Q3_TRIMESH_DATA_SIZE))
                .flatten()
        }) {
            if ppc_hle_trace_enabled() {
                let point_count =
                    ppc_q3_trimesh_header_u32(&data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET).unwrap_or(0);
                let triangle_count =
                    ppc_q3_trimesh_header_u32(&data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
                        .unwrap_or(0);
                eprintln!(
                    "[PPC-TRACE] Q3File body TriMesh object=${:08X} points={} triangles={} data_ptr=${:08X}",
                    object, point_count, triangle_count, body_ptr
                );
            }
            ppc_q3_trimesh_store_data(q3_trimeshes, object, data, Vec::new());
        }
    } else if object_type == PPC_Q3_TEXTURE_TYPE_MIPMAP && body_size > 0 {
        if let Some(data) = ppc_q3_file_decode_mipmap_3dmf_body(
            process_memory_manager,
            memory,
            q3_objects,
            next_q3_object,
            q3_memory_storages,
            body_ptr,
            body_size,
            heap_cursor,
            last_mem_error,
        )
        .or_else(|| ppc_q3_read_bytes(memory, body_ptr, body_size))
        {
            ppc_q3_mipmap_texture_store_data(q3_mipmap_textures, object, data);
        }
    } else if let Some(kind) = ppc_q3_style_kind_for_type(object_type) {
        if body_size >= 4 {
            if let Some(value) = memory.read_u32_be(body_ptr) {
                ppc_q3_style_store_data(q3_styles, object, kind, value);
            }
        }
    }
}

pub(crate) fn ppc_q3_file_decode_mipmap_3dmf_body(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    body_ptr: u32,
    body_size: u32,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> Option<Vec<u8>> {
    const HEADER_SIZE: u32 = 32;
    if body_size < HEADER_SIZE {
        return None;
    }
    let use_mipmapping = memory.read_u32_be(body_ptr)?;
    let pixel_type = memory.read_u32_be(body_ptr.checked_add(4)?)?;
    let bit_order = memory.read_u32_be(body_ptr.checked_add(8)?)?;
    let byte_order = memory.read_u32_be(body_ptr.checked_add(12)?)?;
    let width = memory.read_u32_be(body_ptr.checked_add(16)?)?;
    let height = memory.read_u32_be(body_ptr.checked_add(20)?)?;
    let row_bytes = memory.read_u32_be(body_ptr.checked_add(24)?)?;
    let image_offset = memory.read_u32_be(body_ptr.checked_add(28)?)?;
    if use_mipmapping != 0
        || width == 0
        || height == 0
        || row_bytes == 0
        || ppc_q3_software_texture_pixel_bytes(pixel_type).is_none()
    {
        return None;
    }
    let image_size = height.checked_mul(row_bytes)?;
    if HEADER_SIZE.checked_add(image_size)? > body_size {
        return None;
    }
    let image = ppc_q3_read_bytes(memory, body_ptr.checked_add(HEADER_SIZE)?, image_size)?;
    let image_ptr =
        ppc_q3_file_alloc_and_write_bytes(process_memory_manager, memory, heap_cursor, &image)?;
    let image_storage = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::MemoryStorage,
        PPC_Q3_STORAGE_TYPE_MEMORY,
        image_ptr,
        image_size,
    );
    if image_storage == 0 {
        return None;
    }
    q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage: image_storage,
        buffer_ptr: image_ptr,
        valid_size: image_size,
        buffer_size: image_size,
        owns_buffer: true,
    });

    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_IMAGE_OFFSET, image_storage)?;
    ppc_q3_write_u32_to_slice(
        &mut mipmap,
        PPC_Q3_MIPMAP_USE_MIPMAPPING_OFFSET,
        use_mipmapping,
    )?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET, pixel_type)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_BIT_ORDER_OFFSET, bit_order)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET, byte_order)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_RESERVED_OFFSET, 0)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET, width)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET, height)?;
    ppc_q3_write_u32_to_slice(&mut mipmap, PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET, row_bytes)?;
    ppc_q3_write_u32_to_slice(
        &mut mipmap,
        PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET,
        image_offset,
    )?;
    *last_mem_error = PPC_NO_ERR;
    Some(mipmap)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3Decoded3dmfTriMeshAttribute {
    pub which_array: u32,
    pub which_attr: u32,
    pub attribute_type: u32,
    pub data: Vec<u8>,
    pub use_array: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3Decoded3dmfTriMesh {
    pub num_triangles: u32,
    pub num_triangle_attribute_types: u32,
    pub num_edges: u32,
    pub num_edge_attribute_types: u32,
    pub num_points: u32,
    pub num_vertex_attribute_types: u32,
    pub triangles: Vec<u8>,
    pub edges: Vec<u8>,
    pub points: Vec<u8>,
    pub bbox: Vec<u8>,
    pub attributes: Vec<PpcQ3Decoded3dmfTriMeshAttribute>,
}

pub(crate) fn ppc_q3_file_decode_trimesh_3dmf_body(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    body_ptr: u32,
    body_size: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
) -> Option<Vec<u8>> {
    let decoded = ppc_q3_file_parse_trimesh_3dmf_body(memory, body_ptr, body_size)?;
    let triangle_size = u32::try_from(decoded.triangles.len()).ok()?;
    let edge_size = u32::try_from(decoded.edges.len()).ok()?;
    let point_size = u32::try_from(decoded.points.len()).ok()?;
    let triangle_attribute_types_size = decoded
        .num_triangle_attribute_types
        .checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?;
    let edge_attribute_types_size = decoded
        .num_edge_attribute_types
        .checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?;
    let vertex_attribute_types_size = decoded
        .num_vertex_attribute_types
        .checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?;
    let mut allocation_sizes = vec![triangle_size, point_size];
    if edge_size != 0 {
        allocation_sizes.push(edge_size);
    }
    if triangle_attribute_types_size != 0 {
        allocation_sizes.push(triangle_attribute_types_size);
    }
    if edge_attribute_types_size != 0 {
        allocation_sizes.push(edge_attribute_types_size);
    }
    if vertex_attribute_types_size != 0 {
        allocation_sizes.push(vertex_attribute_types_size);
    }
    for attribute in &decoded.attributes {
        allocation_sizes.push(u32::try_from(attribute.data.len()).ok()?);
        if !attribute.use_array.is_empty() {
            allocation_sizes.push(u32::try_from(attribute.use_array.len()).ok()?);
        }
    }
    if !ppc_heap_can_alloc_sequence(memory, *heap_cursor, heap_limit, &allocation_sizes) {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return None;
    }

    let triangles_ptr = ppc_q3_file_alloc_and_write_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        &decoded.triangles,
    )?;
    let points_ptr = ppc_q3_file_alloc_and_write_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        &decoded.points,
    )?;
    let edges_ptr = if decoded.edges.is_empty() {
        0
    } else {
        ppc_q3_file_alloc_and_write_bytes(
            process_memory_manager,
            memory,
            heap_cursor,
            &decoded.edges,
        )?
    };
    let triangle_attribute_types_ptr = ppc_q3_file_alloc_and_write_zeroes(
        process_memory_manager,
        memory,
        heap_cursor,
        triangle_attribute_types_size,
    )?;
    let edge_attribute_types_ptr = ppc_q3_file_alloc_and_write_zeroes(
        process_memory_manager,
        memory,
        heap_cursor,
        edge_attribute_types_size,
    )?;
    let vertex_attribute_types_ptr = ppc_q3_file_alloc_and_write_zeroes(
        process_memory_manager,
        memory,
        heap_cursor,
        vertex_attribute_types_size,
    )?;
    for attribute in &decoded.attributes {
        let data_ptr = ppc_q3_file_alloc_and_write_bytes(
            process_memory_manager,
            memory,
            heap_cursor,
            &attribute.data,
        )?;
        let use_array_ptr = if attribute.use_array.is_empty() {
            0
        } else {
            ppc_q3_file_alloc_and_write_bytes(
                process_memory_manager,
                memory,
                heap_cursor,
                &attribute.use_array,
            )?
        };
        let attribute_types_ptr = match attribute.which_array {
            PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_TRIANGLE => triangle_attribute_types_ptr,
            PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_EDGE => edge_attribute_types_ptr,
            PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX => vertex_attribute_types_ptr,
            _ => 0,
        };
        ppc_q3_file_write_trimesh_attribute_record(
            memory,
            attribute_types_ptr,
            attribute.which_attr,
            attribute.attribute_type,
            data_ptr,
            use_array_ptr,
        )?;
    }

    let mut data = vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize];
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET,
        decoded.num_triangles,
    )?;
    ppc_q3_trimesh_header_put_u32(&mut data, PPC_Q3_TRIMESH_TRIANGLES_OFFSET, triangles_ptr)?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        decoded.num_triangle_attribute_types,
    )?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        triangle_attribute_types_ptr,
    )?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_EDGES_OFFSET,
        decoded.num_edges,
    )?;
    ppc_q3_trimesh_header_put_u32(&mut data, PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        decoded.num_edge_attribute_types,
    )?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        edge_attribute_types_ptr,
    )?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
        decoded.num_points,
    )?;
    ppc_q3_trimesh_header_put_u32(&mut data, PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        decoded.num_vertex_attribute_types,
    )?;
    ppc_q3_trimesh_header_put_u32(
        &mut data,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        vertex_attribute_types_ptr,
    )?;
    if !decoded.bbox.is_empty() {
        let bbox_offset = (PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET + 4) as usize;
        let bbox_copy_size = data
            .len()
            .saturating_sub(bbox_offset)
            .min(decoded.bbox.len());
        if bbox_copy_size != 0 {
            data[bbox_offset..bbox_offset + bbox_copy_size]
                .copy_from_slice(&decoded.bbox[..bbox_copy_size]);
        }
    }
    *last_mem_error = PPC_NO_ERR;
    Some(data)
}

pub fn ppc_q3_file_parse_trimesh_3dmf_body(
    memory: &mut PpcSectionMem,
    body_ptr: u32,
    body_size: u32,
) -> Option<PpcQ3Decoded3dmfTriMesh> {
    if body_size < 24 {
        return None;
    }
    let body_end = body_ptr.checked_add(body_size)?;
    let num_triangles = memory.read_u32_be(body_ptr)?;
    let num_triangle_attribute_types = memory.read_u32_be(body_ptr.checked_add(4)?)?;
    let num_edges = memory.read_u32_be(body_ptr.checked_add(8)?)?;
    let num_edge_attribute_types = memory.read_u32_be(body_ptr.checked_add(12)?)?;
    let num_points = memory.read_u32_be(body_ptr.checked_add(16)?)?;
    let num_vertex_attribute_types = memory.read_u32_be(body_ptr.checked_add(20)?)?;
    if num_triangles == 0
        || num_points == 0
        || num_triangles > body_size / 3
        || num_edges > body_size / 4
        || num_points > body_size / PPC_Q3_POINT3D_SIZE
        || num_triangle_attribute_types > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
        || num_edge_attribute_types > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
        || num_vertex_attribute_types > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
    {
        return None;
    }

    let point_index_size = ppc_q3_3dmf_index_size(num_points);
    let triangle_index_size = ppc_q3_3dmf_index_size(num_triangles);
    let mut cursor = body_ptr.checked_add(24)?;

    let triangle_output_size = num_triangles.checked_mul(PPC_Q3_TRIMESH_TRIANGLE_DATA_SIZE)?;
    let mut triangles = Vec::with_capacity(usize::try_from(triangle_output_size).ok()?);
    for _ in 0..num_triangles {
        for _ in 0..3 {
            let index = ppc_q3_read_3dmf_index(memory, &mut cursor, body_end, point_index_size)?;
            if index >= num_points {
                return None;
            }
            triangles.extend_from_slice(&index.to_be_bytes());
        }
    }

    let edge_output_size = num_edges.checked_mul(PPC_Q3_TRIMESH_EDGE_DATA_SIZE)?;
    let mut edges = Vec::with_capacity(usize::try_from(edge_output_size).ok()?);
    for _ in 0..num_edges {
        let point_a = ppc_q3_read_3dmf_index(memory, &mut cursor, body_end, point_index_size)?;
        let point_b = ppc_q3_read_3dmf_index(memory, &mut cursor, body_end, point_index_size)?;
        if point_a >= num_points || point_b >= num_points {
            return None;
        }
        let _triangle_a =
            ppc_q3_read_3dmf_index(memory, &mut cursor, body_end, triangle_index_size)?;
        let _triangle_b =
            ppc_q3_read_3dmf_index(memory, &mut cursor, body_end, triangle_index_size)?;
        edges.extend_from_slice(&point_a.to_be_bytes());
        edges.extend_from_slice(&point_b.to_be_bytes());
    }

    let point_data_size = num_points.checked_mul(PPC_Q3_POINT3D_SIZE)?;
    let points = ppc_q3_read_3dmf_bytes(memory, &mut cursor, body_end, point_data_size)?;
    let bbox = ppc_q3_read_3dmf_bytes(memory, &mut cursor, body_end, PPC_Q3_BOUNDING_BOX_SIZE)?;
    let mut attributes = Vec::new();
    while cursor < body_end {
        let child_type = memory.read_u32_be(cursor)?;
        let child_body_size = memory.read_u32_be(cursor.checked_add(4)?)?;
        let child_body_start = cursor.checked_add(8)?;
        let child_end = child_body_start.checked_add(child_body_size)?;
        if child_end > body_end {
            return None;
        }
        if child_type == PPC_Q3_TYPE_ATTRIBUTE_ARRAY {
            if let Some(attribute) = ppc_q3_file_parse_trimesh_attribute_array(
                memory,
                child_body_start,
                child_body_size,
                num_triangles,
                num_triangle_attribute_types,
                num_edges,
                num_edge_attribute_types,
                num_points,
                num_vertex_attribute_types,
            ) {
                attributes.push(attribute);
            }
        }
        cursor = child_end;
    }
    Some(PpcQ3Decoded3dmfTriMesh {
        num_triangles,
        num_triangle_attribute_types,
        num_edges,
        num_edge_attribute_types,
        num_points,
        num_vertex_attribute_types,
        triangles,
        edges,
        points,
        bbox,
        attributes,
    })
}

pub fn ppc_q3_file_parse_trimesh_attribute_array(
    memory: &mut PpcSectionMem,
    body_ptr: u32,
    body_size: u32,
    num_triangles: u32,
    num_triangle_attribute_types: u32,
    num_edges: u32,
    num_edge_attribute_types: u32,
    num_points: u32,
    num_vertex_attribute_types: u32,
) -> Option<PpcQ3Decoded3dmfTriMeshAttribute> {
    if body_size < 20 {
        return None;
    }
    let body_end = body_ptr.checked_add(body_size)?;
    let attribute_type = memory.read_u32_be(body_ptr)?;
    let which_array = memory.read_u32_be(body_ptr.checked_add(8)?)?;
    let which_attr = memory.read_u32_be(body_ptr.checked_add(12)?)?;
    let use_array_flag = memory.read_u32_be(body_ptr.checked_add(16)?)?;
    let (num_elems, num_attributes) = match which_array {
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_TRIANGLE => (num_triangles, num_triangle_attribute_types),
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_EDGE => (num_edges, num_edge_attribute_types),
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX => (num_points, num_vertex_attribute_types),
        _ => return None,
    };
    if which_attr >= num_attributes {
        return None;
    }
    let attribute_data_size = ppc_q3_attribute_data_size(attribute_type)?;
    let mut cursor = body_ptr.checked_add(20)?;
    let use_array = if use_array_flag == 0 {
        Vec::new()
    } else {
        ppc_q3_read_3dmf_bytes(memory, &mut cursor, body_end, num_elems)?
    };
    let attribute_byte_count = num_elems.checked_mul(attribute_data_size)?;
    let data = ppc_q3_read_3dmf_bytes(memory, &mut cursor, body_end, attribute_byte_count)?;
    Some(PpcQ3Decoded3dmfTriMeshAttribute {
        which_array,
        which_attr,
        attribute_type,
        data,
        use_array,
    })
}

pub(crate) fn ppc_q3_file_alloc_and_write_bytes(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    data: &[u8],
) -> Option<u32> {
    if data.is_empty() {
        return Some(0);
    }
    let size = u32::try_from(data.len()).ok()?;
    let ptr = ppc_process_heap_alloc(process_memory_manager, memory, heap_cursor, size, true);
    if ptr == 0 || !ppc_q3_write_bytes(memory, ptr, data) {
        return None;
    }
    Some(ptr)
}

pub(crate) fn ppc_q3_file_alloc_and_write_zeroes(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    size: u32,
) -> Option<u32> {
    if size == 0 {
        return Some(0);
    }
    let data = vec![0; usize::try_from(size).ok()?];
    ppc_q3_file_alloc_and_write_bytes(process_memory_manager, memory, heap_cursor, &data)
}

pub fn ppc_q3_file_write_trimesh_attribute_record(
    memory: &mut PpcSectionMem,
    attribute_types_ptr: u32,
    which_attr: u32,
    attribute_type: u32,
    data_ptr: u32,
    use_array_ptr: u32,
) -> Option<()> {
    if attribute_types_ptr == 0 {
        return None;
    }
    let record_ptr = attribute_types_ptr
        .checked_add(which_attr.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
    memory.write_u32_be(record_ptr, attribute_type)?;
    memory.write_u32_be(record_ptr.checked_add(4)?, data_ptr)?;
    memory.write_u32_be(record_ptr.checked_add(8)?, use_array_ptr)?;
    Some(())
}

pub(crate) fn ppc_q3_file_attach_trimesh_attribute_array(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    target_trimesh: u32,
    body_ptr: u32,
    body_size: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    q3_trimeshes: &[PpcQ3TriMeshRecord],
) -> bool {
    let Some(record) = q3_trimeshes
        .iter()
        .find(|record| record.trimesh == target_trimesh)
    else {
        return false;
    };
    let Some(num_triangles) =
        ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
    else {
        return false;
    };
    let Some(num_triangle_attribute_types) = ppc_q3_trimesh_header_u32(
        &record.data,
        PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
    ) else {
        return false;
    };
    let Some(num_edges) = ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)
    else {
        return false;
    };
    let Some(num_edge_attribute_types) =
        ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET)
    else {
        return false;
    };
    let Some(num_points) =
        ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET)
    else {
        return false;
    };
    let Some(num_vertex_attribute_types) = ppc_q3_trimesh_header_u32(
        &record.data,
        PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
    ) else {
        return false;
    };
    let Some(attribute) = ppc_q3_file_parse_trimesh_attribute_array(
        memory,
        body_ptr,
        body_size,
        num_triangles,
        num_triangle_attribute_types,
        num_edges,
        num_edge_attribute_types,
        num_points,
        num_vertex_attribute_types,
    ) else {
        return false;
    };
    let attribute_types_ptr = match attribute.which_array {
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_TRIANGLE => {
            ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET)
        }
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_EDGE => {
            ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET)
        }
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX => {
            ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET)
        }
        _ => None,
    }
    .unwrap_or(0);
    if attribute_types_ptr == 0 {
        return false;
    }
    let mut allocation_sizes = vec![u32::try_from(attribute.data.len()).unwrap_or(u32::MAX)];
    if !attribute.use_array.is_empty() {
        allocation_sizes.push(u32::try_from(attribute.use_array.len()).unwrap_or(u32::MAX));
    }
    if !ppc_heap_can_alloc_sequence(memory, *heap_cursor, heap_limit, &allocation_sizes) {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return false;
    }
    let Some(data_ptr) = ppc_q3_file_alloc_and_write_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        &attribute.data,
    ) else {
        return false;
    };
    let use_array_ptr = if attribute.use_array.is_empty() {
        0
    } else {
        let Some(ptr) = ppc_q3_file_alloc_and_write_bytes(
            process_memory_manager,
            memory,
            heap_cursor,
            &attribute.use_array,
        ) else {
            return false;
        };
        ptr
    };
    if ppc_q3_file_write_trimesh_attribute_record(
        memory,
        attribute_types_ptr,
        attribute.which_attr,
        attribute.attribute_type,
        data_ptr,
        use_array_ptr,
    )
    .is_none()
    {
        return false;
    }
    *last_mem_error = PPC_NO_ERR;
    true
}

pub fn ppc_q3_3dmf_index_size(count: u32) -> u32 {
    if count <= 0xFF {
        1
    } else if count <= 0xFFFF {
        2
    } else {
        4
    }
}

pub fn ppc_q3_read_3dmf_index(
    memory: &mut PpcSectionMem,
    cursor: &mut u32,
    body_end: u32,
    byte_count: u32,
) -> Option<u32> {
    let start = *cursor;
    let next = start.checked_add(byte_count)?;
    if next > body_end {
        return None;
    }
    let value = match byte_count {
        1 => u32::from(memory.read_u8(start)?),
        2 => u32::from(memory.read_u16_be(start)?),
        4 => memory.read_u32_be(start)?,
        _ => return None,
    };
    *cursor = next;
    Some(value)
}

pub fn ppc_q3_read_3dmf_bytes(
    memory: &mut PpcSectionMem,
    cursor: &mut u32,
    body_end: u32,
    byte_count: u32,
) -> Option<Vec<u8>> {
    let start = *cursor;
    let next = start.checked_add(byte_count)?;
    if next > body_end {
        return None;
    }
    let bytes = ppc_q3_read_bytes(memory, start, byte_count)?;
    *cursor = next;
    Some(bytes)
}

pub fn ppc_q3_trimesh_header_put_u32(data: &mut [u8], offset: u32, value: u32) -> Option<()> {
    let offset = usize::try_from(offset).ok()?;
    data.get_mut(offset..offset.checked_add(4)?)?
        .copy_from_slice(&value.to_be_bytes());
    Some(())
}

pub fn ppc_q3_file_attribute_type_for_chunk(object_type: u32) -> Option<u32> {
    match object_type {
        PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT_3DMF => {
            Some(PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT)
        }
        PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR_3DMF => Some(PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR),
        PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR_3DMF => Some(PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR),
        PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL_3DMF => Some(PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL),
        PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR_3DMF => {
            Some(PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR)
        }
        _ => None,
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3ParsedContainerState {
    pub root_child: Option<u32>,
    pub attribute_set: Option<u32>,
    pub surface_shader: Option<u32>,
}

pub fn ppc_q3_file_attach_trimesh_attribute_set(
    q3_trimeshes: &mut [PpcQ3TriMeshRecord],
    trimesh: u32,
    attribute_set: u32,
) -> bool {
    let Some(record) = q3_trimeshes
        .iter_mut()
        .find(|record| record.trimesh == trimesh)
    else {
        return false;
    };
    ppc_q3_write_u32_to_slice(
        &mut record.data,
        PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET,
        attribute_set,
    )
    .is_some()
}

pub fn ppc_q3_file_reference_target_offset(
    memory: &mut PpcSectionMem,
    storage: PpcQ3ObjectRecord,
    reference_id: u32,
) -> Option<u32> {
    let mut offset = 0u32;
    while offset.checked_add(8)? <= storage.data_size {
        let header_ptr = storage.data_ptr.checked_add(offset)?;
        let object_type = memory.read_u32_be(header_ptr)?;
        let body_size = memory.read_u32_be(header_ptr.checked_add(4)?)?;
        let total_size = body_size.checked_add(8)?;
        let next_offset = offset.checked_add(total_size)?;
        if next_offset > storage.data_size {
            return None;
        }
        if object_type == PPC_Q3_TYPE_TOC && body_size >= 28 {
            let body_ptr = header_ptr.checked_add(8)?;
            let entry_type = memory.read_u32_be(body_ptr.checked_add(16)?)?;
            let entry_size = memory.read_u32_be(body_ptr.checked_add(20)?)?;
            let entry_count = memory.read_u32_be(body_ptr.checked_add(24)?)?;
            if !matches!((entry_type, entry_size), (0, 12) | (1, 16)) {
                return None;
            }
            let entries_size = entry_size.checked_mul(entry_count)?;
            if 28u32.checked_add(entries_size)? > body_size {
                return None;
            }
            for entry_index in 0..entry_count {
                let entry_offset = entry_size.checked_mul(entry_index)?;
                let entry_ptr = body_ptr.checked_add(28)?.checked_add(entry_offset)?;
                if memory.read_u32_be(entry_ptr)? != reference_id {
                    continue;
                }
                let location_high = memory.read_u32_be(entry_ptr.checked_add(4)?)?;
                let location_low = memory.read_u32_be(entry_ptr.checked_add(8)?)?;
                return (location_high == 0 && location_low < storage.data_size)
                    .then_some(location_low);
            }
        }
        offset = next_offset;
    }
    None
}

pub fn ppc_q3_file_reference_target_object(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    file: u32,
    storage: PpcQ3ObjectRecord,
    reference_id: u32,
) -> Option<u32> {
    let target_offset = ppc_q3_file_reference_target_offset(memory, storage, reference_id)?;
    let mut target = q3_objects
        .iter()
        .find(|record| record.source.file == file && record.source.offset == target_offset)
        .map(|record| record.object)?;
    // A binary 3DMF TOC points at the on-disk object header. References to
    // container objects resolve to the first object inside the container,
    // rather than to a separately visible container wrapper.
    for _ in 0..32 {
        if ppc_q3_object_type_for_handle(q3_objects, target) != PPC_Q3_TYPE_CONTAINER {
            return Some(target);
        }
        target = q3_group_memberships
            .iter()
            .find(|record| record.group == target)
            .map(|record| record.object)?;
    }
    None
}

pub fn ppc_q3_file_promote_container_root_object(
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    container: u32,
    root_child: u32,
) {
    let Some(root_record) = q3_objects
        .iter()
        .find(|record| record.object == root_child)
        .copied()
    else {
        return;
    };
    if let Some(container_record) = q3_objects
        .iter_mut()
        .find(|record| record.object == container)
    {
        container_record.object_type = root_record.object_type;
        container_record.data_ptr = root_record.data_ptr;
        container_record.data_size = root_record.data_size;
    }
    if let Some(record) = q3_trimeshes
        .iter()
        .find(|record| record.trimesh == root_child)
        .cloned()
    {
        ppc_q3_trimesh_store_data(
            q3_trimeshes,
            container,
            record.data,
            record.triangle_attribute_sets,
        );
    }
    if let Some(record) = q3_mipmap_textures
        .iter()
        .find(|record| record.texture == root_child)
        .cloned()
    {
        ppc_q3_mipmap_texture_store_data(q3_mipmap_textures, container, record.mipmap);
    }
    if let Some(record) = q3_texture_shaders
        .iter()
        .find(|record| record.shader == root_child)
        .copied()
    {
        ppc_q3_texture_shader_store_link(q3_texture_shaders, container, record.texture);
    }
    let root_styles: Vec<PpcQ3StyleRecord> = q3_styles
        .iter()
        .filter(|record| record.style == root_child)
        .copied()
        .collect();
    for style in root_styles {
        ppc_q3_style_store_data(q3_styles, container, style.kind, style.value);
    }
    let root_attributes: Vec<PpcQ3AttributeRecord> = q3_attributes
        .iter()
        .filter(|record| record.attribute_set == root_child)
        .cloned()
        .collect();
    for attribute in root_attributes {
        ppc_q3_attribute_store_data(
            q3_attributes,
            container,
            attribute.attribute_type,
            attribute.data,
        );
    }
}

pub(crate) fn ppc_q3_file_record_container_children(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    file: u32,
    storage: PpcQ3ObjectRecord,
    parent_object: u32,
    parent_type: u32,
    parent_offset: u32,
    parent_body_size: u32,
    parent_group_depth: u32,
) -> PpcQ3ParsedContainerState {
    let mut state = PpcQ3ParsedContainerState::default();
    let Some(body_start_offset) = parent_offset.checked_add(8) else {
        return state;
    };
    let mut relative_offset = 0u32;
    let mut current_attribute_set = None;
    let mut current_trimesh = None;
    let mut pending_texture_shader = None;
    while relative_offset
        .checked_add(8)
        .is_some_and(|end| end <= parent_body_size)
    {
        let Some(child_offset) = body_start_offset.checked_add(relative_offset) else {
            return state;
        };
        let Some(header_ptr) = storage.data_ptr.checked_add(child_offset) else {
            return state;
        };
        let Some(object_type) = memory.read_u32_be(header_ptr) else {
            return state;
        };
        let Some(body_size_ptr) = header_ptr.checked_add(4) else {
            return state;
        };
        let Some(body_size) = memory.read_u32_be(body_size_ptr) else {
            return state;
        };
        let Some(total_size) = body_size.checked_add(8) else {
            return state;
        };
        let Some(next_relative_offset) = relative_offset.checked_add(total_size) else {
            return state;
        };
        if next_relative_offset > parent_body_size {
            return state;
        }
        if object_type == PPC_Q3_TYPE_REFERENCE {
            let reference_id = header_ptr
                .checked_add(8)
                .filter(|_| body_size >= 4)
                .and_then(|body_ptr| memory.read_u32_be(body_ptr));
            let referenced_object = reference_id.and_then(|reference_id| {
                ppc_q3_file_reference_target_object(
                    memory,
                    q3_objects,
                    q3_group_memberships,
                    file,
                    storage,
                    reference_id,
                )
            });
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] Q3File reference id={:?} parent=${:08X} target={:?}",
                    reference_id,
                    parent_object,
                    referenced_object.map(|object| format!("${object:08X}")),
                );
            }
            if let Some(referenced_object) = referenced_object {
                state.root_child.get_or_insert(referenced_object);
                ppc_q3_group_add_membership_once(
                    q3_group_memberships,
                    parent_object,
                    referenced_object,
                );
                match ppc_q3_object_type_for_handle(q3_objects, referenced_object) {
                    PPC_Q3_TYPE_ATTRIBUTE_SET => {
                        current_attribute_set = Some(referenced_object);
                        state.attribute_set.get_or_insert(referenced_object);
                        if let Some(trimesh) = current_trimesh {
                            let _ = ppc_q3_file_attach_trimesh_attribute_set(
                                q3_trimeshes,
                                trimesh,
                                referenced_object,
                            );
                        }
                    }
                    PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE => {
                        state.surface_shader.get_or_insert(referenced_object);
                    }
                    PPC_Q3_TYPE_TRIMESH => current_trimesh = Some(referenced_object),
                    _ => {}
                }
            }
        } else if !ppc_q3_file_control_type(object_type) {
            let child = ppc_q3_alloc_object(
                q3_objects,
                next_q3_object,
                PpcQ3ObjectKind::Generic,
                object_type,
                header_ptr,
                total_size,
            );
            if child == 0 {
                return state;
            }
            state.root_child.get_or_insert(child);
            if let Some(record) = q3_objects.iter_mut().find(|record| record.object == child) {
                record.source = PpcQ3ObjectSource {
                    file,
                    offset: child_offset,
                    parent_group_type: parent_type,
                    group_depth: parent_group_depth.saturating_add(1),
                };
            }
            ppc_q3_group_add_membership_once(q3_group_memberships, parent_object, child);
            if object_type == PPC_Q3_TYPE_ATTRIBUTE_SET {
                current_attribute_set = Some(child);
                state.attribute_set.get_or_insert(child);
            }
            if let (Some(attribute_set), Some(attribute_type)) = (
                current_attribute_set,
                ppc_q3_file_attribute_type_for_chunk(object_type),
            ) {
                if let Some(data_size) = ppc_q3_attribute_data_size(attribute_type) {
                    if body_size >= data_size {
                        if let Some(body_ptr) = header_ptr.checked_add(8) {
                            if let Some(data) = ppc_q3_read_bytes(memory, body_ptr, data_size) {
                                ppc_q3_attribute_store_data(
                                    q3_attributes,
                                    attribute_set,
                                    attribute_type,
                                    data,
                                );
                            }
                        }
                    }
                }
            }
            ppc_q3_file_record_object_body(
                process_memory_manager,
                memory,
                q3_objects,
                next_q3_object,
                child,
                object_type,
                header_ptr,
                body_size,
                heap_cursor,
                heap_limit,
                last_mem_error,
                q3_memory_storages,
                q3_trimeshes,
                q3_mipmap_textures,
                q3_styles,
            );
            if object_type == PPC_Q3_TYPE_TRIMESH {
                current_trimesh = Some(child);
            } else if object_type == PPC_Q3_TYPE_ATTRIBUTE_ARRAY {
                if let Some(target_trimesh) = current_trimesh {
                    if let Some(body_ptr) = header_ptr.checked_add(8) {
                        let _ = ppc_q3_file_attach_trimesh_attribute_array(
                            process_memory_manager,
                            memory,
                            target_trimesh,
                            body_ptr,
                            body_size,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            q3_trimeshes,
                        );
                    }
                }
            }
            if object_type == PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE {
                pending_texture_shader = Some(child);
            } else if object_type == PPC_Q3_TEXTURE_TYPE_MIPMAP {
                if let Some(shader) = pending_texture_shader.take() {
                    ppc_q3_texture_shader_store_link(q3_texture_shaders, shader, child);
                    state.surface_shader.get_or_insert(shader);
                    if let Some(attribute_set) = current_attribute_set {
                        ppc_q3_attribute_store_data(
                            q3_attributes,
                            attribute_set,
                            PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                            shader.to_be_bytes().to_vec(),
                        );
                    }
                }
            }
            if object_type == PPC_Q3_TYPE_CONTAINER {
                let child_state = ppc_q3_file_record_container_children(
                    process_memory_manager,
                    memory,
                    q3_objects,
                    next_q3_object,
                    q3_memory_storages,
                    q3_group_memberships,
                    q3_attributes,
                    q3_trimeshes,
                    q3_mipmap_textures,
                    q3_texture_shaders,
                    q3_styles,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    file,
                    storage,
                    child,
                    object_type,
                    child_offset,
                    body_size,
                    parent_group_depth.saturating_add(1),
                );
                if let Some(shader) = child_state.surface_shader {
                    state.surface_shader.get_or_insert(shader);
                    if let Some(attribute_set) = current_attribute_set {
                        ppc_q3_attribute_store_data(
                            q3_attributes,
                            attribute_set,
                            PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                            shader.to_be_bytes().to_vec(),
                        );
                    }
                }
                if let (Some(trimesh), Some(attribute_set)) =
                    (current_trimesh, child_state.attribute_set)
                {
                    let _ = ppc_q3_file_attach_trimesh_attribute_set(
                        q3_trimeshes,
                        trimesh,
                        attribute_set,
                    );
                }
            }
        }
        relative_offset = next_relative_offset;
    }
    if let (Some(trimesh), Some(attribute_set)) = (current_trimesh, current_attribute_set) {
        let _ = ppc_q3_file_attach_trimesh_attribute_set(q3_trimeshes, trimesh, attribute_set);
    }
    state
}

pub fn ppc_q3_file_mirror_container_geometry_into_group(
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    group: u32,
    container: u32,
) {
    let child_objects: Vec<u32> = q3_group_memberships
        .iter()
        .filter(|record| record.group == container)
        .map(|record| record.object)
        .collect();
    for child in child_objects {
        let child_type = ppc_q3_object_type_for_handle(q3_objects, child);
        if ppc_q3_object_type_is_geometry(child_type) {
            ppc_q3_group_add_membership_once(q3_group_memberships, group, child);
        } else if child_type == PPC_Q3_TYPE_CONTAINER || ppc_q3_object_type_is_group(child_type) {
            ppc_q3_file_mirror_container_geometry_into_group(
                q3_group_memberships,
                q3_objects,
                group,
                child,
            );
        }
    }
}

pub fn ppc_q3_file_is_end_of_file(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_files: &[PpcQ3FileRecord],
) -> u32 {
    let file = cpu.gpr[3];
    if !ppc_q3_validate_file_handle(q3_objects, q3_error_state, file) {
        return 1;
    }
    let Some(file_record) = q3_files.iter().find(|record| record.file == file) else {
        return 1;
    };
    if !file_record.is_open || file_record.storage == 0 {
        return 1;
    }
    if !ppc_q3_validate_storage_handle(q3_objects, q3_error_state, file_record.storage) {
        return 1;
    }
    let Some(storage_record) = q3_objects
        .iter()
        .find(|record| record.object == file_record.storage)
    else {
        return 1;
    };
    u32::from(
        ppc_q3_file_next_object_chunk(memory, *storage_record, file_record.read_offset).is_none(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcQ3FileChunk {
    pub offset: u32,
    pub next_offset: u32,
    pub object_type: u32,
    pub body_size: u32,
    pub total_size: u32,
    pub parent_group_type: u32,
    pub group_depth: u32,
    pub group_stack: Vec<PpcQ3FileGroupChunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQ3FileGroupChunk {
    pub offset: u32,
    pub total_size: u32,
    pub group_type: u32,
}

pub fn ppc_q3_file_next_object_chunk(
    memory: &mut PpcSectionMem,
    storage: PpcQ3ObjectRecord,
    start_offset: u32,
) -> Option<PpcQ3FileChunk> {
    let mut offset = 0u32;
    let mut group_stack = Vec::new();
    while offset.checked_add(8)? <= storage.data_size {
        let header_ptr = storage.data_ptr.checked_add(offset)?;
        let object_type = memory.read_u32_be(header_ptr)?;
        let body_size = memory.read_u32_be(header_ptr.checked_add(4)?)?;
        let total_size = body_size.checked_add(8)?;
        let next_offset = offset.checked_add(total_size)?;
        if next_offset > storage.data_size {
            return None;
        }
        if object_type == PPC_Q3_TYPE_BEGIN_GROUP {
            group_stack.push(PpcQ3FileGroupChunk {
                offset,
                total_size,
                group_type: ppc_q3_file_group_type(memory, header_ptr, body_size)?,
            });
        } else if object_type == PPC_Q3_TYPE_END_GROUP {
            let _ = group_stack.pop();
        } else if !ppc_q3_file_control_type(object_type) && offset >= start_offset {
            return Some(PpcQ3FileChunk {
                offset,
                next_offset,
                object_type,
                body_size,
                total_size,
                parent_group_type: group_stack
                    .last()
                    .map(|group| group.group_type)
                    .unwrap_or(PPC_Q3_TYPE_NONE),
                group_depth: u32::try_from(group_stack.len()).unwrap_or(u32::MAX),
                group_stack: group_stack.clone(),
            });
        }
        offset = next_offset;
    }
    None
}

pub fn ppc_q3_file_ensure_group_stack(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    file: u32,
    storage: PpcQ3ObjectRecord,
    group_stack: &[PpcQ3FileGroupChunk],
) -> Option<u32> {
    let mut parent_group = 0;
    for (index, group_chunk) in group_stack.iter().copied().enumerate() {
        let group_depth = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
        let group = ppc_q3_file_group_for_chunk(
            q3_objects,
            next_q3_object,
            q3_file_groups,
            file,
            storage,
            group_chunk,
            parent_group,
            group_depth,
        )?;
        if parent_group != 0 {
            ppc_q3_group_add_membership_once(q3_group_memberships, parent_group, group);
        }
        parent_group = group;
    }
    (parent_group != 0).then_some(parent_group)
}

pub fn ppc_q3_file_group_for_chunk(
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    file: u32,
    storage: PpcQ3ObjectRecord,
    group_chunk: PpcQ3FileGroupChunk,
    parent_group: u32,
    group_depth: u32,
) -> Option<u32> {
    if let Some(record) = q3_file_groups
        .iter()
        .find(|record| record.file == file && record.offset == group_chunk.offset)
    {
        return Some(record.group);
    }
    let data_ptr = storage.data_ptr.checked_add(group_chunk.offset)?;
    let group = ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        group_chunk.group_type,
        data_ptr,
        group_chunk.total_size,
    );
    if group == 0 {
        return None;
    }
    if let Some(record) = q3_objects.iter_mut().find(|record| record.object == group) {
        record.source = PpcQ3ObjectSource {
            file,
            offset: group_chunk.offset,
            parent_group_type: q3_file_groups
                .iter()
                .find(|record| record.group == parent_group)
                .map(|record| record.group_type)
                .unwrap_or(PPC_Q3_TYPE_NONE),
            group_depth,
        };
    }
    q3_file_groups.push(PpcQ3FileGroupRecord {
        file,
        offset: group_chunk.offset,
        group,
        group_type: group_chunk.group_type,
        parent_group,
        group_depth,
    });
    Some(group)
}

pub fn ppc_q3_file_group_type(
    memory: &mut PpcSectionMem,
    header_ptr: u32,
    body_size: u32,
) -> Option<u32> {
    if body_size >= 4 {
        memory.read_u32_be(header_ptr.checked_add(8)?)
    } else {
        Some(PPC_Q3_TYPE_NONE)
    }
}

pub fn ppc_q3_file_control_type(object_type: u32) -> bool {
    matches!(
        object_type,
        PPC_Q3_TYPE_3DMF | PPC_Q3_TYPE_BEGIN_GROUP | PPC_Q3_TYPE_END_GROUP | PPC_Q3_TYPE_TOC
    )
}

pub fn ppc_q3_file_close(
    cpu: &PpcCpu,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    q3_files: &mut Vec<PpcQ3FileRecord>,
) -> bool {
    if !ppc_q3_validate_file_handle(q3_objects, q3_error_state, cpu.gpr[3]) {
        return false;
    }
    let Some(record) = ppc_q3_file_mut(q3_files, cpu.gpr[3]) else {
        return false;
    };
    record.is_open = false;
    record.read_offset = 0;
    record.read_object = 0;
    true
}

pub fn ppc_q3_group_add_object(
    group: u32,
    object: u32,
    before_position: Option<u32>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_lights: &[PpcQ3LightRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    if group == 0 || object == 0 {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    let group_type = ppc_q3_object_type_for_handle(q3_objects, group);
    if !ppc_q3_object_type_is_group(group_type) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    if !ppc_q3_object_exists(q3_objects, object) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    if group_type == PPC_Q3_GROUP_TYPE_LIGHT
        && !q3_lights.iter().any(|record| record.light == object)
    {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    let (insert_index, position) = if let Some(position) = before_position {
        let Some(index) =
            ppc_q3_group_record_index_for_position(q3_group_memberships, group, position)
        else {
            q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
            return 0;
        };
        (index, position)
    } else {
        let group_member_count = q3_group_memberships
            .iter()
            .filter(|record| record.group == group)
            .count();
        let Some(position) = ppc_q3_group_position_from_index(group_member_count) else {
            q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
            return 0;
        };
        (q3_group_memberships.len(), position)
    };
    if !ppc_q3_object_retain(q3_objects, q3_object_refs, object) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return 0;
    }
    let record = PpcQ3GroupMembershipRecord {
        group,
        object,
        before: before_position,
    };
    if before_position.is_some() {
        q3_group_memberships.insert(insert_index, record);
    } else {
        q3_group_memberships.push(record);
    }
    position
}

pub fn ppc_q3_group_add_membership_once(
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    group: u32,
    object: u32,
) {
    if group == 0 || object == 0 {
        return;
    }
    if q3_group_memberships
        .iter()
        .any(|record| record.group == group && record.object == object)
    {
        return;
    }
    q3_group_memberships.push(PpcQ3GroupMembershipRecord {
        group,
        object,
        before: None,
    });
}

pub fn ppc_q3_group_count_objects(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let group = cpu.gpr[3];
    let count_out_ptr = cpu.gpr[4];
    if group == 0 || count_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, count_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return false;
    }
    let count =
        ppc_q3_group_position_objects(q3_group_memberships, q3_objects, q3_file_groups, group)
            .len();
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3Group_CountObjects group=${:08X} type='{}' count={}",
            group,
            format_ppc_fourcc(ppc_q3_object_type_for_handle(q3_objects, group)),
            count
        );
    }
    memory
        .write_u32_be(count_out_ptr, u32::try_from(count).unwrap_or(u32::MAX))
        .is_some()
}

pub fn ppc_q3_group_get_first_position(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let group = cpu.gpr[3];
    let position_out_ptr = cpu.gpr[4];
    if group == 0
        || position_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, position_out_ptr, 4)
    {
        return false;
    }
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return false;
    }
    let position = ppc_q3_group_first_visible_position_matching(
        q3_group_memberships,
        q3_objects,
        q3_file_groups,
        group,
        |_| true,
    );
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3Group_GetFirstPosition group=${:08X} type='{}' position={}",
            group,
            format_ppc_fourcc(ppc_q3_object_type_for_handle(q3_objects, group)),
            position
        );
    }
    memory.write_u32_be(position_out_ptr, position).is_some()
}

pub fn ppc_q3_group_get_next_position(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let group = cpu.gpr[3];
    let position_ptr = cpu.gpr[4];
    if group == 0 || position_ptr == 0 || !ppc_memory_can_write_bytes(memory, position_ptr, 4) {
        return false;
    }
    let Some(position) = memory.read_u32_be(position_ptr) else {
        return false;
    };
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return false;
    }
    if position == 0 {
        return memory.write_u32_be(position_ptr, 0).is_some();
    }
    let Some(current_index) = ppc_q3_group_index_from_position(position) else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    };
    let visible_objects =
        ppc_q3_group_position_objects(q3_group_memberships, q3_objects, q3_file_groups, group);
    if current_index >= visible_objects.len() {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    let next_position = current_index
        .checked_add(1)
        .filter(|next_index| *next_index < visible_objects.len())
        .and_then(ppc_q3_group_position_from_index)
        .unwrap_or(0);
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3Group_GetNextPosition group=${:08X} type='{}' current={} next={}",
            group,
            format_ppc_fourcc(ppc_q3_object_type_for_handle(q3_objects, group)),
            position,
            next_position
        );
    }
    memory.write_u32_be(position_ptr, next_position).is_some()
}

pub fn ppc_q3_group_get_first_position_of_type(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let group = cpu.gpr[3];
    let object_type = cpu.gpr[4];
    let position_out_ptr = cpu.gpr[5];
    if group == 0
        || object_type == PPC_Q3_TYPE_NONE
        || position_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, position_out_ptr, 4)
    {
        return false;
    }
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return false;
    }
    let position = ppc_q3_group_first_visible_position_matching(
        q3_group_memberships,
        q3_objects,
        q3_file_groups,
        group,
        |object| {
            ppc_q3_object_type_matches(
                ppc_q3_object_type_for_handle(q3_objects, object),
                object_type,
            )
        },
    );
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Q3Group_GetFirstPositionOfType group=${:08X} group_type='{}' requested='{}' position={}",
            group,
            format_ppc_fourcc(ppc_q3_object_type_for_handle(q3_objects, group)),
            format_ppc_fourcc(object_type),
            position
        );
    }
    memory.write_u32_be(position_out_ptr, position).is_some()
}

pub fn ppc_q3_group_get_position_object(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_lights: &[PpcQ3LightRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let group = cpu.gpr[3];
    let position = cpu.gpr[4];
    let object_out_ptr = cpu.gpr[5];
    if group == 0 || object_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, object_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return false;
    }
    let Some(object) = ppc_q3_group_visible_object_for_position(
        q3_group_memberships,
        q3_objects,
        q3_file_groups,
        group,
        position,
    ) else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    };
    if !ppc_q3_group_member_accepts_object(q3_objects, q3_lights, q3_error_state, group, object) {
        return false;
    }
    if memory.write_u32_be(object_out_ptr, object).is_none() {
        return false;
    }
    if !ppc_q3_object_retain(q3_objects, q3_object_refs, object) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    if ppc_hle_trace_enabled() || qd3d_trace_enabled() {
        let object_type = ppc_q3_object_type_for_handle(q3_objects, object);
        let trimesh_counts = q3_trimeshes
            .iter()
            .find(|record| record.trimesh == object)
            .map(|record| {
                (
                    ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
                        .unwrap_or(0),
                    ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET)
                        .unwrap_or(0),
                )
            });
        if let Some((triangles, points)) = trimesh_counts {
            eprintln!(
                "[PPC-TRACE] Q3Group_GetPositionObject group=${:08X} position={} object=${:08X} type='{}' triangles={} points={}",
                group,
                position,
                object,
                format_ppc_fourcc(object_type),
                triangles,
                points
            );
        } else {
            eprintln!(
                "[PPC-TRACE] Q3Group_GetPositionObject group=${:08X} position={} object=${:08X} type='{}'",
                group,
                position,
                object,
                format_ppc_fourcc(object_type)
            );
        }
    }
    true
}

pub fn ppc_q3_group_member_accepts_object(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_lights: &[PpcQ3LightRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    group: u32,
    object: u32,
) -> bool {
    if !ppc_q3_object_exists(q3_objects, object) {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    let group_type = ppc_q3_object_type_for_handle(q3_objects, group);
    if group_type == PPC_Q3_GROUP_TYPE_LIGHT
        && !q3_lights.iter().any(|record| record.light == object)
    {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    true
}

pub fn ppc_q3_group_remove_position(
    cpu: &PpcCpu,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> Option<u32> {
    let group = cpu.gpr[3];
    let position = cpu.gpr[4];
    if !ppc_q3_validate_group_handle(q3_objects, q3_error_state, group) {
        return None;
    }
    let Some(index) = ppc_q3_group_record_index_for_position(q3_group_memberships, group, position)
    else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return None;
    };
    Some(q3_group_memberships.remove(index).object)
}

pub fn ppc_q3_validate_group_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    group: u32,
) -> bool {
    let group_type = ppc_q3_object_type_for_handle(q3_objects, group);
    if ppc_q3_object_type_is_group(group_type) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_group_first_visible_position_matching(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    group: u32,
    predicate: impl Fn(u32) -> bool,
) -> u32 {
    ppc_q3_group_position_objects(q3_group_memberships, q3_objects, q3_file_groups, group)
        .iter()
        .copied()
        .enumerate()
        .find(|(_, object)| predicate(*object))
        .and_then(|(index, _)| ppc_q3_group_position_from_index(index))
        .unwrap_or(0)
}

pub fn ppc_q3_group_visible_object_for_position(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    group: u32,
    position: u32,
) -> Option<u32> {
    let index = ppc_q3_group_index_from_position(position)?;
    ppc_q3_group_position_objects(q3_group_memberships, q3_objects, q3_file_groups, group)
        .get(index)
        .copied()
}

pub fn ppc_q3_group_position_objects(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    group: u32,
) -> Vec<u32> {
    if ppc_q3_group_uses_direct_positions(q3_group_memberships, q3_objects, group) {
        return ppc_q3_group_direct_position_objects(
            q3_group_memberships,
            q3_objects,
            q3_file_groups,
            group,
        );
    }
    ppc_q3_group_visible_objects(q3_group_memberships, q3_objects, group)
}

pub fn ppc_q3_group_uses_direct_positions(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    group: u32,
) -> bool {
    let Some(group_record) = q3_objects.iter().find(|record| record.object == group) else {
        return false;
    };
    if group_record.source.file != 0 || group_record.object_type != PPC_Q3_GROUP_TYPE_DISPLAY {
        return false;
    }
    let direct_objects = ppc_q3_group_direct_objects(q3_group_memberships, group);
    direct_objects.len() > 1
        && direct_objects.iter().all(|object| {
            q3_objects
                .iter()
                .find(|record| record.object == *object)
                .is_some_and(|record| {
                    record.source.file != 0
                        && !ppc_q3_object_type_is_transform(record.object_type)
                        && !ppc_q3_object_type_is_group(record.object_type)
                })
        })
}

pub fn ppc_q3_group_direct_objects(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    group: u32,
) -> Vec<u32> {
    q3_group_memberships
        .iter()
        .filter(|record| record.group == group)
        .map(|record| record.object)
        .collect()
}

pub fn ppc_q3_group_direct_position_objects(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    group: u32,
) -> Vec<u32> {
    let direct_objects = ppc_q3_group_direct_objects(q3_group_memberships, group);
    let top_level_objects =
        ppc_q3_group_direct_top_level_file_objects(&direct_objects, q3_objects, q3_file_groups);
    if top_level_objects.len() > 1 && top_level_objects.len() < direct_objects.len() {
        top_level_objects
    } else {
        direct_objects
    }
}

pub fn ppc_q3_group_direct_top_level_file_objects(
    direct_objects: &[u32],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
) -> Vec<u32> {
    let mut top_level_objects = Vec::new();
    for object in direct_objects {
        let Some(group) =
            ppc_q3_top_level_file_object_for_object(q3_objects, q3_file_groups, *object)
        else {
            return Vec::new();
        };
        ppc_q3_group_visible_push_unique(&mut top_level_objects, group);
    }
    top_level_objects
}

pub fn ppc_q3_top_level_file_object_for_object(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_file_groups: &[PpcQ3FileGroupRecord],
    object: u32,
) -> Option<u32> {
    let object_record = q3_objects.iter().find(|record| record.object == object)?;
    if object_record.source.file == 0 || object_record.source.group_depth == 0 {
        return None;
    }
    if object_record.source.group_depth == 1 {
        return Some(object);
    }
    q3_file_groups
        .iter()
        .filter(|file_group| {
            file_group.file == object_record.source.file
                && file_group.group_depth == 2
                && file_group.offset <= object_record.source.offset
        })
        .max_by_key(|file_group| file_group.offset)
        .map(|file_group| file_group.group)
}

pub fn ppc_q3_group_visible_objects(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    group: u32,
) -> Vec<u32> {
    let mut visible = Vec::new();
    let mut visited_groups = Vec::new();
    for object in q3_group_memberships
        .iter()
        .filter(|record| record.group == group)
        .map(|record| record.object)
    {
        ppc_q3_group_visible_push_unique(&mut visible, object);
        ppc_q3_group_collect_visible_geometry(
            q3_group_memberships,
            q3_objects,
            object,
            &mut visible,
            &mut visited_groups,
        );
    }
    visible
}

pub fn ppc_q3_group_collect_visible_geometry(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    object: u32,
    visible: &mut Vec<u32>,
    visited_groups: &mut Vec<u32>,
) {
    let object_type = ppc_q3_object_type_for_handle(q3_objects, object);
    if object_type != PPC_Q3_TYPE_CONTAINER && !ppc_q3_object_type_is_group(object_type) {
        return;
    }
    if visited_groups.contains(&object) {
        return;
    }
    visited_groups.push(object);
    let child_objects: Vec<u32> = q3_group_memberships
        .iter()
        .filter(|record| record.group == object)
        .map(|record| record.object)
        .collect();
    for child in child_objects {
        let child_type = ppc_q3_object_type_for_handle(q3_objects, child);
        if ppc_q3_object_type_is_geometry(child_type) {
            ppc_q3_group_visible_push_unique(visible, child);
        }
        ppc_q3_group_collect_visible_geometry(
            q3_group_memberships,
            q3_objects,
            child,
            visible,
            visited_groups,
        );
    }
}

pub fn ppc_q3_group_visible_push_unique(visible: &mut Vec<u32>, object: u32) {
    if !visible.contains(&object) {
        visible.push(object);
    }
}

pub fn ppc_q3_group_record_index_for_position(
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    group: u32,
    position: u32,
) -> Option<usize> {
    if group == 0 {
        return None;
    }
    let group_index = ppc_q3_group_index_from_position(position)?;
    q3_group_memberships
        .iter()
        .enumerate()
        .filter(|(_, record)| record.group == group)
        .nth(group_index)
        .map(|(index, _)| index)
}

pub fn ppc_q3_group_index_from_position(position: u32) -> Option<usize> {
    usize::try_from(position.checked_sub(1)?).ok()
}

pub fn ppc_q3_group_position_from_index(index: usize) -> Option<u32> {
    u32::try_from(index).ok()?.checked_add(1)
}

pub fn ppc_q3_view_mut(
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    view: u32,
) -> Option<&mut PpcQ3ViewStateRecord> {
    if view == 0 {
        return None;
    }
    if let Some(index) = q3_views.iter().position(|record| record.view == view) {
        return q3_views.get_mut(index);
    }
    q3_views.push(PpcQ3ViewStateRecord::new(view));
    q3_views.last_mut()
}

pub fn ppc_q3_known_view_mut<'a>(
    q3_views: &'a mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    view: u32,
) -> Option<&'a mut PpcQ3ViewStateRecord> {
    if view == 0 {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return None;
    }
    if let Some(index) = q3_views.iter().position(|record| record.view == view) {
        return q3_views.get_mut(index);
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return None;
    }
    q3_views.push(PpcQ3ViewStateRecord::new(view));
    q3_views.last_mut()
}

pub fn ppc_q3_view_set_light_group(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    let light_group = cpu.gpr[4];
    if view == 0 || light_group == 0 {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view)
        || !ppc_q3_view_slot_accepts_object(
            q3_objects,
            q3_error_state,
            PpcQ3ViewSlot::LightGroup,
            light_group,
        )
    {
        return false;
    }
    let old_light_group = q3_views
        .iter()
        .find(|record| record.view == view)
        .map(|record| record.light_group)
        .unwrap_or(0);
    if old_light_group == light_group {
        return true;
    }
    let retained = ppc_q3_object_retain(q3_objects, q3_object_refs, light_group);
    if !retained {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    if !ppc_q3_view_set_slot(cpu, q3_views, PpcQ3ViewSlot::LightGroup) {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, light_group);
        return false;
    }
    if old_light_group != 0 {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, old_light_group);
    }
    true
}

pub fn ppc_q3_view_set_owned_slot(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    q3_renderer_preferences: &mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_files: &mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &mut Vec<PpcQ3FileGroupRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_memory_storages: &mut Vec<PpcQ3MemoryStorageRecord>,
    q3_attributes: &mut Vec<PpcQ3AttributeRecord>,
    q3_shader_uv_transforms: &mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &mut Vec<PpcQ3TextureShaderRecord>,
    q3_draw_contexts: &mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &mut Vec<PpcQ3CameraRecord>,
    q3_lights: &mut Vec<PpcQ3LightRecord>,
    slot: PpcQ3ViewSlot,
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    let value = cpu.gpr[4];
    if view == 0 || value == 0 {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view)
        || !ppc_q3_view_slot_accepts_object(q3_objects, q3_error_state, slot, value)
    {
        return false;
    }
    let old_value = q3_views
        .iter()
        .find(|record| record.view == view)
        .map(|record| ppc_q3_view_slot_value(record, slot))
        .unwrap_or(0);
    if old_value == value {
        return true;
    }
    let retained = ppc_q3_object_retain(q3_objects, q3_object_refs, value);
    if !retained {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    if !ppc_q3_view_set_slot(cpu, q3_views, slot) {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, value);
        return false;
    }
    if old_value != 0 {
        let mut stores = PpcQ3ObjectStores {
            q3_objects,
            q3_object_refs,
            q3_renderer_preferences,
            q3_files,
            q3_group_memberships,
            q3_file_groups,
            q3_views,
            q3_submissions,
            q3_view_transforms,
            q3_submission_transforms,
            q3_view_materials,
            q3_submission_materials,
            q3_submission_lights,
            q3_view_state_stack,
            q3_completed_frames,
            q3_retained_frames,
            q3_fog_styles,
            q3_memory_storages,
            q3_attributes,
            q3_shader_uv_transforms,
            q3_shader_boundaries,
            q3_mipmap_textures,
            q3_texture_shaders,
            q3_draw_contexts,
            q3_trimeshes,
            q3_styles,
            q3_cameras,
            q3_lights,
        };
        let _ = ppc_q3_object_release_reference(&mut stores, old_value);
    }
    true
}

pub fn ppc_q3_view_slot_accepts_object(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    slot: PpcQ3ViewSlot,
    value: u32,
) -> bool {
    let object_type = ppc_q3_object_type_for_handle(q3_objects, value);
    let accepted = match slot {
        PpcQ3ViewSlot::Renderer => ppc_q3_object_type_is_renderer(object_type),
        PpcQ3ViewSlot::LightGroup => object_type == PPC_Q3_GROUP_TYPE_LIGHT,
        PpcQ3ViewSlot::DrawContext => ppc_q3_object_type_is_draw_context(object_type),
        PpcQ3ViewSlot::Camera => ppc_q3_object_type_is_camera(object_type),
    };
    if accepted {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_validate_view_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    view: u32,
) -> bool {
    if ppc_q3_object_type_for_handle(q3_objects, view) == PPC_Q3_TYPE_VIEW {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_view_slot_value(record: &PpcQ3ViewStateRecord, slot: PpcQ3ViewSlot) -> u32 {
    match slot {
        PpcQ3ViewSlot::Renderer => record.renderer,
        PpcQ3ViewSlot::LightGroup => record.light_group,
        PpcQ3ViewSlot::DrawContext => record.draw_context,
        PpcQ3ViewSlot::Camera => record.camera,
    }
}

pub fn ppc_q3_view_set_slot(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    slot: PpcQ3ViewSlot,
) -> bool {
    let value = cpu.gpr[4];
    if value == 0 {
        return false;
    }
    let Some(record) = ppc_q3_view_mut(q3_views, cpu.gpr[3]) else {
        return false;
    };
    match slot {
        PpcQ3ViewSlot::Renderer => record.renderer = value,
        PpcQ3ViewSlot::LightGroup => record.light_group = value,
        PpcQ3ViewSlot::DrawContext => record.draw_context = value,
        PpcQ3ViewSlot::Camera => record.camera = value,
    }
    true
}

pub fn ppc_q3_view_get_owned_slot(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_views: &[PpcQ3ViewStateRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_object_refs: &mut Vec<PpcQ3ObjectReferenceRecord>,
    slot: PpcQ3ViewSlot,
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    let value_out_ptr = cpu.gpr[4];
    if view == 0 || value_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, value_out_ptr, 4) {
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    let value = q3_views
        .iter()
        .find(|record| record.view == view)
        .map(|record| ppc_q3_view_slot_value(record, slot))
        .unwrap_or(0);
    if value != 0 && !ppc_q3_view_slot_accepts_object(q3_objects, q3_error_state, slot, value) {
        return false;
    }
    if memory.write_u32_be(value_out_ptr, value).is_none() {
        return false;
    }
    if value != 0 {
        let _ = ppc_q3_object_retain(q3_objects, q3_object_refs, value);
    }
    true
}

pub fn ppc_q3_view_start_rendering(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    let Some(record) = ppc_q3_known_view_mut(q3_views, q3_objects, q3_error_state, view) else {
        return false;
    };
    if record.rendering_depth == 0
        && ppc_q3_queued_frame_has_records_for_view(
            record.view,
            q3_submissions,
            q3_submission_transforms,
            q3_submission_materials,
            q3_submission_lights,
        )
    {
        ppc_q3_drain_queued_frame_for_view(
            record.view,
            q3_submissions,
            q3_submission_transforms,
            q3_submission_materials,
            q3_submission_lights,
        );
    }
    record.rendering_depth = record.rendering_depth.saturating_add(1);
    record.cancelled = false;
    true
}

pub fn ppc_q3_view_end_rendering(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_state_only_completed_frame_batches: &mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    let view = cpu.gpr[3];
    let Some(record) = ppc_q3_known_view_mut(q3_views, q3_objects, q3_error_state, view) else {
        return PPC_Q3_VIEW_STATUS_ERROR;
    };
    let was_rendering = record.rendering_depth > 0;
    record.rendering_depth = record.rendering_depth.saturating_sub(1);
    let completed = was_rendering && record.rendering_depth == 0;
    let cancelled = record.cancelled;
    let view = record.view;
    let draw_context = record.draw_context;
    if completed {
        if !qd3d_dump_frame_enabled() {
            let mut has_submission = false;
            let mut has_trimesh = false;
            for submission in q3_submissions
                .iter()
                .filter(|submission| submission.view == view)
            {
                has_submission = true;
                if submission.kind == PpcQ3SubmissionKind::TriMesh {
                    has_trimesh = true;
                    break;
                }
            }
            if !has_submission || cancelled {
                ppc_q3_drain_queued_frame_for_view(
                    view,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_submission_materials,
                    q3_submission_lights,
                );
                return PPC_Q3_VIEW_STATUS_DONE;
            }
            if !has_trimesh {
                if let Some(frame) = ppc_q3_take_retained_frame_for_view(q3_retained_frames, view) {
                    ppc_q3_drain_queued_frame_for_view(
                        view,
                        q3_submissions,
                        q3_submission_transforms,
                        q3_submission_materials,
                        q3_submission_lights,
                    );
                    q3_completed_frames.push(frame);
                    return PPC_Q3_VIEW_STATUS_DONE;
                }
                let target = ppc_q3_render_target_for_view_draw_context(
                    q3_objects,
                    q3_draw_contexts,
                    gworlds,
                    current_gworld,
                    draw_context,
                );
                push_q3_state_only_completed_frame_batch(
                    q3_state_only_completed_frame_batches,
                    target,
                );
                ppc_q3_drain_queued_frame_for_view(
                    view,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_submission_materials,
                    q3_submission_lights,
                );
                return PPC_Q3_VIEW_STATUS_DONE;
            }
        }
        let frame = ppc_q3_take_completed_frame(
            view,
            q3_submissions,
            q3_submission_transforms,
            q3_submission_materials,
            q3_submission_lights,
        );
        if !cancelled && !frame.submissions.is_empty() {
            q3_completed_frames.push(frame);
        }
    }
    PPC_Q3_VIEW_STATUS_DONE
}

pub fn dispatch_q3_view_end_rendering_import(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_completed_frames: &mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_state_only_completed_frame_batches: &mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    q3_error_state: &mut PpcQ3ErrorState,
    allow_idle_state_only_fast_forward: bool,
) -> PpcImportAction {
    let view = cpu.gpr[3];
    let idle_no_renderable_direct_trimesh_frame = allow_idle_state_only_fast_forward
        && q3_submissions
            .iter()
            .any(|submission| submission.view == view)
        && !ppc_q3_submissions_include_renderable_direct_trimesh(
            q3_submissions,
            q3_trimeshes,
            view,
        );
    let completed_frames_before = q3_completed_frames.len();
    let state_only_batches_before = q3_state_only_completed_frame_batches.len();
    let state_only_last_batch_frames_before = q3_state_only_completed_frame_batches
        .last()
        .map(|batch| batch.frames)
        .unwrap_or_default();
    let result = ppc_q3_view_end_rendering(
        cpu,
        q3_views,
        q3_objects,
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
        q3_completed_frames,
        q3_retained_frames,
        q3_state_only_completed_frame_batches,
        q3_draw_contexts,
        gworlds,
        current_gworld,
        q3_error_state,
    );
    let state_only_batch_added = q3_state_only_completed_frame_batches.len()
        > state_only_batches_before
        || q3_state_only_completed_frame_batches
            .last()
            .map(|batch| batch.frames > state_only_last_batch_frames_before)
            .unwrap_or(false);
    let retained_idle_frame_added = idle_no_renderable_direct_trimesh_frame
        && q3_completed_frames.len() > completed_frames_before;
    if allow_idle_state_only_fast_forward
        && result == PPC_Q3_VIEW_STATUS_DONE
        && (state_only_batch_added || retained_idle_frame_added)
    {
        PpcImportAction::ReturnWithExtraCycles(result, PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES)
    } else {
        PpcImportAction::Return(result)
    }
}

pub fn ppc_q3_submissions_include_renderable_direct_trimesh(
    q3_submissions: &[PpcQ3SubmissionRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    view: u32,
) -> bool {
    q3_submissions
        .iter()
        .filter(|submission| {
            submission.view == view && submission.kind == PpcQ3SubmissionKind::TriMesh
        })
        .any(|submission| {
            q3_trimeshes
                .iter()
                .find(|record| record.trimesh == submission.primary)
                .and_then(|record| {
                    ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
                })
                .unwrap_or(0)
                > 0
        })
}

pub fn ppc_q3_take_completed_frame(
    view: u32,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
) -> PpcQ3CompletedFrameRecord {
    PpcQ3CompletedFrameRecord {
        view,
        submissions: ppc_q3_take_records_for_view(q3_submissions, view, |record| record.view),
        submission_transforms: ppc_q3_take_records_for_view(
            q3_submission_transforms,
            view,
            |record| record.view,
        ),
        submission_materials: ppc_q3_take_records_for_view(
            q3_submission_materials,
            view,
            |record| record.view,
        ),
        submission_lights: ppc_q3_take_records_for_view(q3_submission_lights, view, |record| {
            record.view
        }),
        retained_trimeshes: Vec::new(),
    }
}

pub fn q3_state_only_completed_frame_total(batches: &[PpcQ3StateOnlyCompletedFrameBatch]) -> usize {
    batches
        .iter()
        .map(|batch| batch.frames)
        .fold(0usize, usize::saturating_add)
}

fn push_q3_state_only_completed_frame_batch(
    batches: &mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    target: Option<PpcQ3RenderTarget>,
) {
    if let Some(batch) = batches.last_mut().filter(|batch| batch.target == target) {
        batch.frames = batch.frames.saturating_add(1);
    } else {
        batches.push(PpcQ3StateOnlyCompletedFrameBatch { frames: 1, target });
    }
}

pub fn ppc_q3_completed_frame_render_target(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_views: &[PpcQ3ViewStateRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    frame: &PpcQ3CompletedFrameRecord,
) -> Option<PpcQ3RenderTarget> {
    frame
        .submissions
        .iter()
        .filter_map(|submission| {
            q3_views
                .iter()
                .find(|record| record.view == submission.view)
                .map(|view| view.draw_context)
        })
        .filter(|draw_context| *draw_context != 0)
        .find_map(|draw_context| {
            ppc_q3_draw_context_render_target(q3_objects, q3_draw_contexts, gworlds, draw_context)
        })
        .or_else(|| {
            ppc_front_buffer_for_gworld(gworlds, current_gworld).map(|front_buffer| {
                PpcQ3RenderTarget {
                    front_buffer,
                    viewport: None,
                    clear_color: None,
                    source: PpcQ3RenderTargetSource::CurrentGWorld,
                    draw_context: None,
                    gworld: Some(current_gworld),
                }
            })
        })
}

pub fn ppc_q3_completed_frame_has_trimesh(frame: &PpcQ3CompletedFrameRecord) -> bool {
    frame
        .submissions
        .iter()
        .any(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
}

pub fn ppc_q3_capture_retained_trimeshes(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    frame: &PpcQ3CompletedFrameRecord,
) -> Vec<PpcQ3TriMeshRecord> {
    let mut retained = Vec::new();
    for primary in frame
        .submissions
        .iter()
        .filter(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
        .map(|submission| submission.primary)
    {
        if retained
            .iter()
            .any(|record: &PpcQ3TriMeshRecord| record.trimesh == primary)
        {
            continue;
        }
        if let Some(record) = q3_trimeshes.iter().find(|record| record.trimesh == primary) {
            retained.push(record.clone());
            continue;
        }
        let data = q3_objects
            .iter()
            .find(|record| record.object == primary && record.object_type == PPC_Q3_TYPE_TRIMESH)
            .and_then(|object| {
                if object.data_ptr != 0 && object.data_size >= PPC_Q3_TRIMESH_DATA_SIZE {
                    ppc_q3_read_bytes(memory, object.data_ptr, PPC_Q3_TRIMESH_DATA_SIZE)
                } else {
                    None
                }
            })
            .or_else(|| ppc_q3_read_bytes(memory, primary, PPC_Q3_TRIMESH_DATA_SIZE));
        if let Some(data) = data {
            retained.push(PpcQ3TriMeshRecord {
                trimesh: primary,
                data,
                triangle_attribute_sets: Vec::new(),
                get_data_copies: Vec::new(),
            });
        }
    }
    retained
}

pub fn ppc_q3_limit_retained_frame_trimeshes(frame: &mut PpcQ3CompletedFrameRecord, limit: usize) {
    if limit == 0 {
        frame.submissions.clear();
        frame.submission_transforms.clear();
        frame.submission_materials.clear();
        frame.submission_lights.clear();
        frame.retained_trimeshes.clear();
        return;
    }
    let mut kept_submissions = Vec::new();
    let mut kept_transforms = Vec::new();
    let mut kept_materials = Vec::new();
    let mut kept_lights = Vec::new();
    let mut kept_trimeshes = Vec::new();
    for submission in frame
        .submissions
        .iter()
        .filter(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
        .take(limit)
    {
        kept_submissions.push(*submission);
        if let Some(transform) = frame
            .submission_transforms
            .iter()
            .find(|record| ppc_q3_submission_transform_matches(record, submission))
        {
            kept_transforms.push(*transform);
        }
        if let Some(material) = frame
            .submission_materials
            .iter()
            .find(|record| ppc_q3_submission_material_matches(record, submission))
        {
            kept_materials.push(material.clone());
        }
        if let Some(lights) = frame
            .submission_lights
            .iter()
            .find(|record| ppc_q3_submission_light_matches(record, submission))
        {
            kept_lights.push(lights.clone());
        }
        if let Some(trimesh) = frame
            .retained_trimeshes
            .iter()
            .find(|record| record.trimesh == submission.primary)
        {
            kept_trimeshes.push(trimesh.clone());
        }
    }
    frame.submissions = kept_submissions;
    frame.submission_transforms = kept_transforms;
    frame.submission_materials = kept_materials;
    frame.submission_lights = kept_lights;
    frame.retained_trimeshes = kept_trimeshes;
}

pub fn ppc_q3_replace_retained_frame(
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    frame: PpcQ3CompletedFrameRecord,
) {
    let view = frame.view;
    q3_retained_frames.retain(|record| record.view != view);
    q3_retained_frames.push(PpcQ3RetainedFrameRecord { view, frame });
}

pub fn ppc_q3_take_retained_frame_for_view(
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    view: u32,
) -> Option<PpcQ3CompletedFrameRecord> {
    let index = q3_retained_frames
        .iter()
        .rposition(|record| record.view == view)?;
    Some(q3_retained_frames.remove(index).frame)
}

pub fn ppc_q3_render_target_for_view_draw_context(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    draw_context: u32,
) -> Option<PpcQ3RenderTarget> {
    if draw_context != 0 {
        if let Some(target) =
            ppc_q3_draw_context_render_target(q3_objects, q3_draw_contexts, gworlds, draw_context)
        {
            return Some(target);
        }
    }
    ppc_front_buffer_for_gworld(gworlds, current_gworld).map(|front_buffer| PpcQ3RenderTarget {
        front_buffer,
        viewport: None,
        clear_color: None,
        source: PpcQ3RenderTargetSource::CurrentGWorld,
        draw_context: None,
        gworld: Some(current_gworld),
    })
}

pub fn ppc_q3_take_records_for_view<T>(
    records: &mut Vec<T>,
    view: u32,
    mut record_view: impl FnMut(&T) -> u32,
) -> Vec<T> {
    let mut taken = Vec::new();
    let mut kept = Vec::with_capacity(records.len());
    for record in records.drain(..) {
        if record_view(&record) == view {
            taken.push(record);
        } else {
            kept.push(record);
        }
    }
    *records = kept;
    taken
}

pub fn ppc_q3_drain_records_for_view<T>(
    records: &mut Vec<T>,
    view: u32,
    mut record_view: impl FnMut(&T) -> u32,
) {
    records.retain(|record| record_view(record) != view);
}

pub fn ppc_q3_records_have_view<T>(
    records: &[T],
    view: u32,
    mut record_view: impl FnMut(&T) -> u32,
) -> bool {
    records.iter().any(|record| record_view(record) == view)
}

pub fn ppc_q3_queued_frame_has_records_for_view(
    view: u32,
    q3_submissions: &[PpcQ3SubmissionRecord],
    q3_submission_transforms: &[PpcQ3SubmissionTransformRecord],
    q3_submission_materials: &[PpcQ3SubmissionMaterialRecord],
    q3_submission_lights: &[PpcQ3SubmissionLightRecord],
) -> bool {
    ppc_q3_records_have_view(q3_submissions, view, |record| record.view)
        || ppc_q3_records_have_view(q3_submission_transforms, view, |record| record.view)
        || ppc_q3_records_have_view(q3_submission_materials, view, |record| record.view)
        || ppc_q3_records_have_view(q3_submission_lights, view, |record| record.view)
}

pub fn ppc_q3_drain_queued_frame_for_view(
    view: u32,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
) {
    ppc_q3_drain_records_for_view(q3_submissions, view, |record| record.view);
    ppc_q3_drain_records_for_view(q3_submission_transforms, view, |record| record.view);
    ppc_q3_drain_records_for_view(q3_submission_materials, view, |record| record.view);
    ppc_q3_drain_records_for_view(q3_submission_lights, view, |record| record.view);
}

pub fn ppc_q3_completed_frame_references_object(
    frame: &PpcQ3CompletedFrameRecord,
    object: u32,
) -> bool {
    frame.view == object
        || frame
            .submissions
            .iter()
            .any(|record| ppc_q3_submission_record_references_object(record, object))
        || frame.submission_transforms.iter().any(|record| {
            record.view == object || record.primary == object || record.secondary == object
        })
        || frame
            .submission_materials
            .iter()
            .any(|record| ppc_q3_submission_material_references_object(record, object))
        || frame
            .submission_lights
            .iter()
            .any(|record| ppc_q3_submission_light_references_object(record, object))
}

pub fn ppc_q3_submission_record_references_object(record: &PpcQ3SubmissionRecord, object: u32) -> bool {
    record.view == object || record.primary == object || record.secondary == object
}

pub fn ppc_q3_submission_material_references_object(
    record: &PpcQ3SubmissionMaterialRecord,
    object: u32,
) -> bool {
    record.view == object
        || record.primary == object
        || record.secondary == object
        || record.shader == object
        || record.styles.iter().any(|style| style.style == object)
        || record
            .shader_uv_transform
            .map(|transform| transform.shader == object)
            .unwrap_or(false)
        || record
            .shader_boundary
            .map(|boundary| boundary.shader == object)
            .unwrap_or(false)
        || record
            .texture_shader
            .map(|shader| shader.shader == object || shader.texture == object)
            .unwrap_or(false)
        || record
            .mipmap_texture
            .as_ref()
            .map(|texture| texture.texture == object)
            .unwrap_or(false)
        || record
            .attributes
            .iter()
            .any(|attribute| attribute.attribute_set == object)
}

pub fn ppc_q3_submission_light_references_object(
    record: &PpcQ3SubmissionLightRecord,
    object: u32,
) -> bool {
    record.view == object
        || record.primary == object
        || record.secondary == object
        || record.light_group == object
        || record.lights.iter().any(|light| light.light == object)
}

pub fn ppc_q3_view_state_snapshot_references_object(
    record: &PpcQ3ViewStateSnapshotRecord,
    object: u32,
) -> bool {
    record.view == object
        || record
            .transform
            .as_ref()
            .map(|transform| transform.view == object)
            .unwrap_or(false)
        || record
            .material
            .as_ref()
            .map(|material| ppc_q3_view_material_references_object(material, object))
            .unwrap_or(false)
}

pub fn ppc_q3_view_material_references_object(record: &PpcQ3ViewMaterialRecord, object: u32) -> bool {
    record.view == object
        || record.shader == object
        || record.styles.iter().any(|style| style.style == object)
        || record
            .attributes
            .iter()
            .any(|attribute| attribute.attribute_set == object)
}

pub fn ppc_q3_view_start_bounding_box(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    let Some(record) = ppc_q3_view_mut(q3_views, view) else {
        return false;
    };
    record.bounding_box_depth = record.bounding_box_depth.saturating_add(1);
    true
}

pub fn ppc_q3_view_end_bounding_box(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    let bounding_box_out_ptr = cpu.gpr[4];
    if bounding_box_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, bounding_box_out_ptr, PPC_Q3_BOUNDING_BOX_SIZE)
    {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    let view = cpu.gpr[3];
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    let Some(record) = ppc_q3_view_mut(q3_views, view) else {
        return PPC_Q3_VIEW_STATUS_ERROR;
    };
    let was_bounding = record.bounding_box_depth > 0;
    record.bounding_box_depth = record.bounding_box_depth.saturating_sub(1);
    let completed = was_bounding && record.bounding_box_depth == 0;
    let view = record.view;
    if completed {
        let frame = ppc_q3_take_completed_frame(
            view,
            q3_submissions,
            q3_submission_transforms,
            q3_submission_materials,
            q3_submission_lights,
        );
        if ppc_q3_completed_frame_has_trimesh(&frame) {
            let mut frame = frame;
            frame.retained_trimeshes =
                ppc_q3_capture_retained_trimeshes(memory, q3_objects, q3_trimeshes, &frame);
            ppc_q3_limit_retained_frame_trimeshes(
                &mut frame,
                PPC_Q3_RETAINED_BOUNDING_TRIMESH_PREVIEW_LIMIT,
            );
            ppc_q3_replace_retained_frame(q3_retained_frames, frame);
        }
    }
    if ppc_q3_write_bounding_box(memory, bounding_box_out_ptr).is_none() {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    PPC_Q3_VIEW_STATUS_DONE
}

pub fn ppc_q3_write_bounding_box(memory: &mut PpcSectionMem, bounding_box_ptr: u32) -> Option<()> {
    ppc_write_q3_vector3d(memory, bounding_box_ptr, (-1.0, -1.0, -1.0))?;
    ppc_write_q3_vector3d(
        memory,
        bounding_box_ptr.checked_add(PPC_Q3_BOUNDING_BOX_MAX_OFFSET)?,
        (1.0, 1.0, 1.0),
    )?;
    memory.write_u32_be(
        bounding_box_ptr.checked_add(PPC_Q3_BOUNDING_BOX_IS_EMPTY_OFFSET)?,
        0,
    )?;
    Some(())
}

pub fn ppc_q3_view_end_bounding_sphere(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_retained_frames: &mut Vec<PpcQ3RetainedFrameRecord>,
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> u32 {
    let view = cpu.gpr[3];
    let sphere_ptr = cpu.gpr[4];
    if sphere_ptr == 0
        || !ppc_memory_can_write_bytes(memory, sphere_ptr, PPC_Q3_BOUNDING_SPHERE_SIZE)
        || !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view)
    {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    let Some(record) = ppc_q3_view_mut(q3_views, view) else {
        return PPC_Q3_VIEW_STATUS_ERROR;
    };
    if record.bounding_box_depth == 0 {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    record.bounding_box_depth -= 1;
    let completed = record.bounding_box_depth == 0;
    let sphere = if completed {
        let mut frame = ppc_q3_take_completed_frame(
            view,
            q3_submissions,
            q3_submission_transforms,
            q3_submission_materials,
            q3_submission_lights,
        );
        frame.retained_trimeshes =
            ppc_q3_capture_retained_trimeshes(memory, q3_objects, q3_trimeshes, &frame);
        let sphere = ppc_q3_bounding_sphere_from_frame(memory, &frame);
        if ppc_q3_completed_frame_has_trimesh(&frame) {
            ppc_q3_limit_retained_frame_trimeshes(
                &mut frame,
                PPC_Q3_RETAINED_BOUNDING_TRIMESH_PREVIEW_LIMIT,
            );
            ppc_q3_replace_retained_frame(q3_retained_frames, frame);
        }
        sphere
    } else {
        None
    };
    let (center, radius, is_empty) = match sphere {
        Some((center, radius)) => (center, radius, 0),
        None => ((0.0, 0.0, 0.0), 0.0, 1),
    };
    if ppc_write_q3_vector3d(memory, sphere_ptr, center).is_none()
        || ppc_write_f32_be(memory, sphere_ptr + PPC_Q3_BOUNDING_SPHERE_RADIUS_OFFSET, radius)
            .is_none()
        || memory
            .write_u32_be(sphere_ptr + PPC_Q3_BOUNDING_SPHERE_IS_EMPTY_OFFSET, is_empty)
            .is_none()
    {
        return PPC_Q3_VIEW_STATUS_ERROR;
    }
    PPC_Q3_VIEW_STATUS_DONE
}

pub fn ppc_q3_bounding_sphere_from_frame(
    memory: &mut PpcSectionMem,
    frame: &PpcQ3CompletedFrameRecord,
) -> Option<((f32, f32, f32), f32)> {
    let mut points = Vec::new();
    for submission in frame
        .submissions
        .iter()
        .filter(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
    {
        let Some(trimesh) = frame
            .retained_trimeshes
            .iter()
            .find(|record| record.trimesh == submission.primary)
        else {
            continue;
        };
        let Some(count) =
            ppc_q3_trimesh_header_u32(&trimesh.data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET)
        else {
            continue;
        };
        let Some(points_ptr) =
            ppc_q3_trimesh_header_u32(&trimesh.data, PPC_Q3_TRIMESH_POINTS_OFFSET)
        else {
            continue;
        };
        if count > PPC_Q3_SOFTWARE_RENDER_MAX_POINTS || points_ptr == 0 {
            continue;
        }
        let transform = frame
            .submission_transforms
            .iter()
            .find(|record| {
                record.view == submission.view
                    && record.kind == submission.kind
                    && record.primary == submission.primary
                    && record.secondary == submission.secondary
            })
            .map(|record| record.local_to_world)
            .unwrap_or_else(ppc_q3_matrix4x4_identity);
        for index in 0..count {
            let Some(point_ptr) = index
                .checked_mul(PPC_Q3_POINT3D_SIZE)
                .and_then(|offset| points_ptr.checked_add(offset))
            else {
                continue;
            };
            let Some(point) = ppc_read_q3_vector3d(memory, point_ptr) else {
                continue;
            };
            let world = ppc_q3_point3d_transform_values(point, transform);
            if ppc_q3_point_finite(world) {
                points.push(world);
            }
        }
    }
    if points.is_empty() {
        return None;
    }
    let mut min = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for point in &points {
        min.0 = min.0.min(point.0);
        min.1 = min.1.min(point.1);
        min.2 = min.2.min(point.2);
        max.0 = max.0.max(point.0);
        max.1 = max.1.max(point.1);
        max.2 = max.2.max(point.2);
    }
    let center = (
        min.0 + (max.0 - min.0) * 0.5,
        min.1 + (max.1 - min.1) * 0.5,
        min.2 + (max.2 - min.2) * 0.5,
    );
    let radius = points
        .iter()
        .map(|point| {
            let dx = point.0 - center.0;
            let dy = point.1 - center.1;
            let dz = point.2 - center.2;
            dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt()
        })
        .fold(0.0_f32, f32::max);
    Some((center, radius))
}

pub fn ppc_q3_view_cancel(
    cpu: &PpcCpu,
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let view = cpu.gpr[3];
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    let view = {
        let Some(record) = ppc_q3_view_mut(q3_views, view) else {
            return false;
        };
        record.cancelled = true;
        record.rendering_depth = 0;
        record.bounding_box_depth = 0;
        record.view
    };
    let _ = ppc_q3_take_completed_frame(
        view,
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
    );
    true
}

pub fn ppc_q3_submit(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_attributes: &[PpcQ3AttributeRecord],
    q3_styles: &[PpcQ3StyleRecord],
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_lights: &[PpcQ3LightRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    kind: PpcQ3SubmissionKind,
) -> bool {
    let (view, primary, secondary) = match kind {
        PpcQ3SubmissionKind::ResetTransform
        | PpcQ3SubmissionKind::Push
        | PpcQ3SubmissionKind::Pop => (cpu.gpr[3], 0, 0),
        _ => (cpu.gpr[4], cpu.gpr[3], 0),
    };
    if matches!(
        kind,
        PpcQ3SubmissionKind::Shader
            | PpcQ3SubmissionKind::Style
            | PpcQ3SubmissionKind::FogStyle
            | PpcQ3SubmissionKind::TriMesh
            | PpcQ3SubmissionKind::MatrixTransform
            | PpcQ3SubmissionKind::Object
    ) && primary == 0
    {
        return false;
    }
    if !ppc_q3_submit_primary_accepts_object(q3_objects, q3_error_state, kind, primary) {
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    if ppc_q3_view_mut(q3_views, view).is_none() {
        return false;
    }
    let local_to_world = match kind {
        PpcQ3SubmissionKind::MatrixTransform => {
            let Some(matrix) = ppc_read_q3_matrix4x4(memory, primary) else {
                return false;
            };
            ppc_q3_view_transform_push_matrix(q3_view_transforms, view, matrix)
        }
        PpcQ3SubmissionKind::ResetTransform => {
            ppc_q3_view_transform_reset(q3_view_transforms, view)
        }
        PpcQ3SubmissionKind::Push => ppc_q3_view_state_push(
            q3_view_state_stack,
            q3_view_transforms,
            q3_view_materials,
            view,
        ),
        PpcQ3SubmissionKind::Pop => {
            let Some(local_to_world) = ppc_q3_view_state_pop(
                q3_view_state_stack,
                q3_view_transforms,
                q3_view_materials,
                view,
            ) else {
                return false;
            };
            local_to_world
        }
        PpcQ3SubmissionKind::Shader
        | PpcQ3SubmissionKind::Style
        | PpcQ3SubmissionKind::FogStyle
        | PpcQ3SubmissionKind::TriMesh
        | PpcQ3SubmissionKind::Object => ppc_q3_view_transform_current(q3_view_transforms, view),
    };
    let light_snapshot = ppc_q3_submission_light_snapshot(
        q3_views,
        q3_group_memberships,
        q3_lights,
        view,
        kind,
        primary,
        secondary,
    );
    let mut material = ppc_q3_view_material_for_submission(
        q3_objects,
        q3_view_materials,
        q3_styles,
        view,
        kind,
        primary,
        None,
    );
    if kind == PpcQ3SubmissionKind::TriMesh {
        let merged_retained_attributes = ppc_q3_view_material_merge_trimesh_attributes(
            &mut material,
            q3_trimeshes,
            q3_attributes,
            primary,
        );
        if !merged_retained_attributes {
            let attribute_set = memory
                .read_u32_be(primary.saturating_add(PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET))
                .filter(|attribute_set| *attribute_set != 0);
            if let Some(attribute_set) = attribute_set {
                let mut attributes = ppc_q3_attributes_for_set(q3_attributes, attribute_set);
                if attributes.is_empty() {
                    attributes = ppc_q3_attributes_for_set(q3_attributes, primary);
                }
                ppc_q3_view_material_merge_attributes(&mut material, attributes);
            }
        }
    }
    ppc_q3_record_submission(
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
        view,
        kind,
        primary,
        secondary,
        local_to_world,
        material.clone(),
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        light_snapshot,
    );
    if kind == PpcQ3SubmissionKind::Object {
        let mut object_local_to_world = local_to_world;
        let mut object_material = material;
        if ppc_q3_object_type_for_handle(q3_objects, primary) == PPC_Q3_TYPE_TRIMESH {
            ppc_q3_record_trimesh_submission(
                q3_views,
                q3_group_memberships,
                q3_submissions,
                q3_submission_transforms,
                q3_submission_materials,
                q3_submission_lights,
                q3_attributes,
                q3_shader_boundaries,
                q3_shader_uv_transforms,
                q3_texture_shaders,
                q3_mipmap_textures,
                q3_trimeshes,
                q3_lights,
                view,
                primary,
                object_local_to_world,
                object_material,
            );
        } else {
            ppc_q3_record_object_submission_children(
                memory,
                q3_objects,
                q3_group_memberships,
                q3_views,
                q3_submissions,
                q3_submission_transforms,
                q3_submission_materials,
                q3_submission_lights,
                q3_attributes,
                q3_styles,
                q3_shader_boundaries,
                q3_shader_uv_transforms,
                q3_texture_shaders,
                q3_mipmap_textures,
                q3_trimeshes,
                q3_lights,
                view,
                primary,
                &mut object_local_to_world,
                &mut object_material,
                0,
            );
        }
    }
    true
}

pub fn ppc_q3_submit_primary_accepts_object(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    kind: PpcQ3SubmissionKind,
    primary: u32,
) -> bool {
    let accepts = match kind {
        PpcQ3SubmissionKind::Shader => {
            ppc_q3_object_type_is_shader(ppc_q3_object_type_for_handle(q3_objects, primary))
        }
        PpcQ3SubmissionKind::Style => {
            ppc_q3_object_type_is_style(ppc_q3_object_type_for_handle(q3_objects, primary))
        }
        PpcQ3SubmissionKind::Object => {
            ppc_q3_object_type_is_drawable(ppc_q3_object_type_for_handle(q3_objects, primary))
        }
        PpcQ3SubmissionKind::FogStyle
        | PpcQ3SubmissionKind::MatrixTransform
        | PpcQ3SubmissionKind::Pop
        | PpcQ3SubmissionKind::Push
        | PpcQ3SubmissionKind::ResetTransform
        | PpcQ3SubmissionKind::TriMesh => true,
    };
    if accepts {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_direct_style_submit(
    cpu: &PpcCpu,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_lights: &[PpcQ3LightRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    style_kind: PpcQ3StyleKind,
    style_type: u32,
) -> bool {
    let value = cpu.gpr[3];
    let view = cpu.gpr[4];
    if !ppc_q3_style_value_valid(style_kind, value) {
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    if ppc_q3_view_mut(q3_views, view).is_none() {
        return false;
    }
    let style = PpcQ3StyleRecord {
        style: 0,
        kind: style_kind,
        value,
    };
    let local_to_world = ppc_q3_view_transform_current(q3_view_transforms, view);
    let light_snapshot = ppc_q3_submission_light_snapshot(
        q3_views,
        q3_group_memberships,
        q3_lights,
        view,
        PpcQ3SubmissionKind::Style,
        value,
        style_type,
    );
    let material = {
        let material = ppc_q3_view_material_mut(q3_view_materials, view);
        ppc_q3_view_material_set_style(material, style);
        material.clone()
    };
    ppc_q3_record_submission(
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
        view,
        PpcQ3SubmissionKind::Style,
        value,
        style_type,
        local_to_world,
        material,
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        light_snapshot,
    );
    true
}

pub fn ppc_q3_fog_style_submit(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_views: &mut Vec<PpcQ3ViewStateRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_fog_styles: &mut Vec<PpcQ3FogStyleRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_lights: &[PpcQ3LightRecord],
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let data_ptr = cpu.gpr[3];
    let view = cpu.gpr[4];
    if data_ptr == 0 || view == 0 {
        return false;
    }
    let Some(data) = ppc_read_q3_fog_style_data(memory, data_ptr) else {
        return false;
    };
    if !ppc_q3_fog_style_data_valid(data) {
        return false;
    }
    if !ppc_q3_validate_view_handle(q3_objects, q3_error_state, view) {
        return false;
    }
    if ppc_q3_view_mut(q3_views, view).is_none() {
        return false;
    }
    let Some(secondary) = u32::try_from(q3_fog_styles.len())
        .ok()
        .and_then(|index| index.checked_add(1))
    else {
        return false;
    };
    q3_fog_styles.push(PpcQ3FogStyleRecord {
        view,
        data_ptr,
        data,
    });
    let local_to_world = ppc_q3_view_transform_current(q3_view_transforms, view);
    let light_snapshot = ppc_q3_submission_light_snapshot(
        q3_views,
        q3_group_memberships,
        q3_lights,
        view,
        PpcQ3SubmissionKind::FogStyle,
        data_ptr,
        secondary,
    );
    let material = ppc_q3_view_material_for_submission(
        &[],
        q3_view_materials,
        &[],
        view,
        PpcQ3SubmissionKind::FogStyle,
        data_ptr,
        Some(data),
    );
    ppc_q3_record_submission(
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
        view,
        PpcQ3SubmissionKind::FogStyle,
        data_ptr,
        secondary,
        local_to_world,
        material,
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        light_snapshot,
    );
    true
}

pub fn ppc_q3_record_submission(
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    view: u32,
    kind: PpcQ3SubmissionKind,
    primary: u32,
    secondary: u32,
    local_to_world: [[f32; 4]; 4],
    material: PpcQ3ViewMaterialRecord,
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    light_snapshot: PpcQ3SubmissionLightRecord,
) {
    let texture_shader = q3_texture_shaders
        .iter()
        .find(|record| record.shader == material.shader)
        .copied();
    let shader_boundary = q3_shader_boundaries
        .iter()
        .find(|record| record.shader == material.shader)
        .copied();
    let shader_uv_transform = q3_shader_uv_transforms
        .iter()
        .find(|record| record.shader == material.shader)
        .copied();
    let mipmap_texture = texture_shader.and_then(|shader| {
        q3_mipmap_textures
            .iter()
            .find(|record| record.texture == shader.texture)
            .cloned()
    });
    q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind,
        primary,
        secondary,
    });
    q3_submission_transforms.push(PpcQ3SubmissionTransformRecord {
        view,
        kind,
        primary,
        secondary,
        local_to_world,
    });
    q3_submission_materials.push(PpcQ3SubmissionMaterialRecord {
        view,
        kind,
        primary,
        secondary,
        shader: material.shader,
        illumination_type: material.illumination_type,
        styles: material.styles,
        fog_style: material.fog_style,
        attributes: material.attributes,
        shader_uv_transform,
        shader_boundary,
        texture_shader,
        mipmap_texture,
    });
    q3_submission_lights.push(light_snapshot);
}

pub fn ppc_q3_record_object_submission_children(
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_views: &[PpcQ3ViewStateRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_attributes: &[PpcQ3AttributeRecord],
    q3_styles: &[PpcQ3StyleRecord],
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_lights: &[PpcQ3LightRecord],
    view: u32,
    group: u32,
    local_to_world: &mut [[f32; 4]; 4],
    material: &mut PpcQ3ViewMaterialRecord,
    depth: u32,
) {
    if depth >= 64 {
        return;
    }
    let children = ppc_q3_object_submission_children(q3_objects, q3_group_memberships, group);
    for child in children {
        let child_object = q3_objects
            .iter()
            .find(|record| record.object == child)
            .copied();
        let object_type = child_object
            .map(|record| record.object_type)
            .unwrap_or(PPC_Q3_TYPE_NONE);
        if object_type == PPC_Q3_TYPE_ATTRIBUTE_SET {
            let attributes = ppc_q3_attributes_for_set(q3_attributes, child);
            if !attributes.is_empty() {
                ppc_q3_view_material_set_attributes(material, attributes);
            }
            continue;
        }
        if object_type == PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE {
            material.shader = child;
            continue;
        }
        if ppc_q3_object_type_is_illumination_shader(object_type) {
            material.illumination_type = object_type;
            continue;
        }
        if let Some(style_kind) = ppc_q3_style_kind_for_type(object_type) {
            if let Some(style) = q3_styles
                .iter()
                .find(|record| record.style == child && record.kind == style_kind)
                .copied()
            {
                ppc_q3_view_material_set_style(material, style);
            }
            continue;
        }
        if object_type == PPC_Q3_TRANSFORM_TYPE_MATRIX {
            if let Some(matrix) = child_object
                .as_ref()
                .and_then(ppc_q3_matrix_transform_matrix_ptr)
                .and_then(|matrix_ptr| ppc_read_q3_matrix4x4(memory, matrix_ptr))
            {
                *local_to_world = ppc_q3_matrix4x4_multiply_values(*local_to_world, matrix);
            }
            continue;
        }
        if object_type == PPC_Q3_TYPE_TRIMESH {
            ppc_q3_record_trimesh_submission(
                q3_views,
                q3_group_memberships,
                q3_submissions,
                q3_submission_transforms,
                q3_submission_materials,
                q3_submission_lights,
                q3_attributes,
                q3_shader_boundaries,
                q3_shader_uv_transforms,
                q3_texture_shaders,
                q3_mipmap_textures,
                q3_trimeshes,
                q3_lights,
                view,
                child,
                *local_to_world,
                material.clone(),
            );
            continue;
        }
        if q3_group_memberships
            .iter()
            .any(|membership| membership.group == child)
        {
            ppc_q3_record_object_submission_children(
                memory,
                q3_objects,
                q3_group_memberships,
                q3_views,
                q3_submissions,
                q3_submission_transforms,
                q3_submission_materials,
                q3_submission_lights,
                q3_attributes,
                q3_styles,
                q3_shader_boundaries,
                q3_shader_uv_transforms,
                q3_texture_shaders,
                q3_mipmap_textures,
                q3_trimeshes,
                q3_lights,
                view,
                child,
                local_to_world,
                material,
                depth.saturating_add(1),
            );
            continue;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ppc_q3_record_trimesh_submission(
    q3_views: &[PpcQ3ViewStateRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_submissions: &mut Vec<PpcQ3SubmissionRecord>,
    q3_submission_transforms: &mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_submission_materials: &mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &mut Vec<PpcQ3SubmissionLightRecord>,
    q3_attributes: &[PpcQ3AttributeRecord],
    q3_shader_boundaries: &[PpcQ3ShaderBoundaryRecord],
    q3_shader_uv_transforms: &[PpcQ3ShaderUvTransformRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_lights: &[PpcQ3LightRecord],
    view: u32,
    trimesh: u32,
    local_to_world: [[f32; 4]; 4],
    mut material: PpcQ3ViewMaterialRecord,
) {
    ppc_q3_view_material_merge_trimesh_attributes(
        &mut material,
        q3_trimeshes,
        q3_attributes,
        trimesh,
    );
    let light_snapshot = ppc_q3_submission_light_snapshot(
        q3_views,
        q3_group_memberships,
        q3_lights,
        view,
        PpcQ3SubmissionKind::TriMesh,
        trimesh,
        0,
    );
    ppc_q3_record_submission(
        q3_submissions,
        q3_submission_transforms,
        q3_submission_materials,
        q3_submission_lights,
        view,
        PpcQ3SubmissionKind::TriMesh,
        trimesh,
        0,
        local_to_world,
        material,
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        light_snapshot,
    );
}

pub fn ppc_q3_object_submission_children(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    group: u32,
) -> Vec<u32> {
    let mut children: Vec<(usize, u32)> = q3_group_memberships
        .iter()
        .enumerate()
        .filter(|(_, membership)| membership.group == group)
        .map(|(index, membership)| (index, membership.object))
        .collect();
    if ppc_q3_object_is_io_proxy_display_group(q3_objects, group) {
        return children
            .into_iter()
            .find(|(_, child)| ppc_q3_io_proxy_display_group_child_supported(q3_objects, *child))
            .map(|(_, child)| vec![child])
            .unwrap_or_default();
    }
    if ppc_q3_object_is_ordered_display_group(q3_objects, group) {
        children.sort_by_key(|(index, child)| {
            (
                ppc_q3_ordered_display_group_child_sort_key(q3_objects, *child),
                *index,
            )
        });
    }
    children.into_iter().map(|(_, child)| child).collect()
}

pub fn ppc_q3_object_is_ordered_display_group(q3_objects: &[PpcQ3ObjectRecord], object: u32) -> bool {
    ppc_q3_object_type_for_handle(q3_objects, object) == PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY
}

pub fn ppc_q3_object_is_io_proxy_display_group(q3_objects: &[PpcQ3ObjectRecord], object: u32) -> bool {
    ppc_q3_object_type_for_handle(q3_objects, object) == PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY
}

pub fn ppc_q3_io_proxy_display_group_child_supported(
    q3_objects: &[PpcQ3ObjectRecord],
    object: u32,
) -> bool {
    let object_type = ppc_q3_object_type_for_handle(q3_objects, object);
    ppc_q3_object_type_is_geometry(object_type) || ppc_q3_object_type_is_display_group(object_type)
}

pub fn ppc_q3_ordered_display_group_child_sort_key(
    q3_objects: &[PpcQ3ObjectRecord],
    object: u32,
) -> u8 {
    let object_type = ppc_q3_object_type_for_handle(q3_objects, object);
    if ppc_q3_object_type_is_transform(object_type) {
        0
    } else if ppc_q3_object_type_is_style(object_type) {
        1
    } else if ppc_q3_object_type_is_set(object_type) {
        2
    } else if ppc_q3_object_type_is_shader(object_type) {
        3
    } else if ppc_q3_object_type_is_geometry(object_type) {
        4
    } else if ppc_q3_object_type_is_display_group(object_type) {
        5
    } else {
        6
    }
}

pub fn ppc_q3_object_type_for_handle(q3_objects: &[PpcQ3ObjectRecord], object: u32) -> u32 {
    ppc_q3_known_object_type_for_handle(q3_objects, object).unwrap_or(PPC_Q3_TYPE_NONE)
}

pub fn ppc_q3_known_object_type_for_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    object: u32,
) -> Option<u32> {
    q3_objects
        .iter()
        .find(|record| record.object == object)
        .map(|record| record.object_type)
}

pub fn ppc_q3_submission_light_snapshot(
    q3_views: &[PpcQ3ViewStateRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_lights: &[PpcQ3LightRecord],
    view: u32,
    kind: PpcQ3SubmissionKind,
    primary: u32,
    secondary: u32,
) -> PpcQ3SubmissionLightRecord {
    let light_group = q3_views
        .iter()
        .find(|record| record.view == view)
        .map(|record| record.light_group)
        .unwrap_or(0);
    let lights = if light_group == 0 {
        Vec::new()
    } else {
        q3_group_memberships
            .iter()
            .filter(|membership| membership.group == light_group)
            .filter_map(|membership| {
                q3_lights
                    .iter()
                    .find(|light| light.light == membership.object)
                    .copied()
            })
            .collect()
    };
    PpcQ3SubmissionLightRecord {
        view,
        kind,
        primary,
        secondary,
        light_group,
        lights,
    }
}

pub fn ppc_q3_view_material_mut(
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    view: u32,
) -> &mut PpcQ3ViewMaterialRecord {
    if let Some(index) = q3_view_materials
        .iter()
        .position(|record| record.view == view)
    {
        return &mut q3_view_materials[index];
    }
    q3_view_materials.push(PpcQ3ViewMaterialRecord::new(view));
    q3_view_materials
        .last_mut()
        .expect("just pushed Q3 view material record")
}

pub fn ppc_q3_view_material_for_submission(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    q3_styles: &[PpcQ3StyleRecord],
    view: u32,
    kind: PpcQ3SubmissionKind,
    primary: u32,
    fog_style: Option<PpcQ3FogStyleData>,
) -> PpcQ3ViewMaterialRecord {
    let record = ppc_q3_view_material_mut(q3_view_materials, view);
    match kind {
        PpcQ3SubmissionKind::Shader => {
            if let Some(illumination_type) = ppc_q3_illumination_shader_type(q3_objects, primary) {
                record.illumination_type = illumination_type;
            } else {
                record.shader = primary;
            }
        }
        PpcQ3SubmissionKind::Style => {
            if let Some(style) = q3_styles
                .iter()
                .find(|style| style.style == primary)
                .copied()
            {
                ppc_q3_view_material_set_style(record, style);
            }
        }
        PpcQ3SubmissionKind::FogStyle => {
            record.fog_style = fog_style;
        }
        PpcQ3SubmissionKind::TriMesh
        | PpcQ3SubmissionKind::MatrixTransform
        | PpcQ3SubmissionKind::ResetTransform
        | PpcQ3SubmissionKind::Push
        | PpcQ3SubmissionKind::Pop
        | PpcQ3SubmissionKind::Object => {}
    }
    record.clone()
}

pub fn ppc_q3_view_material_set_style(material: &mut PpcQ3ViewMaterialRecord, style: PpcQ3StyleRecord) {
    if let Some(record) = material
        .styles
        .iter_mut()
        .find(|record| record.kind == style.kind)
    {
        *record = style;
    } else {
        material.styles.push(style);
        material.styles.sort_by_key(|record| match record.kind {
            PpcQ3StyleKind::Backfacing => 0,
            PpcQ3StyleKind::Interpolation => 1,
            PpcQ3StyleKind::Fill => 2,
            PpcQ3StyleKind::Orientation => 3,
        });
    }
}

pub fn ppc_q3_view_material_set_attributes(
    material: &mut PpcQ3ViewMaterialRecord,
    attributes: Vec<PpcQ3AttributeRecord>,
) {
    if let Some(shader) = ppc_q3_attributes_surface_shader(&attributes) {
        material.shader = shader;
    }
    material.attributes = attributes;
}

pub fn ppc_q3_view_material_merge_trimesh_attributes(
    material: &mut PpcQ3ViewMaterialRecord,
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_attributes: &[PpcQ3AttributeRecord],
    trimesh: u32,
) -> bool {
    let Some(record) = q3_trimeshes.iter().find(|record| record.trimesh == trimesh) else {
        return false;
    };
    let mesh_attribute_set =
        ppc_q3_trimesh_header_u32(&record.data, PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET)
            .filter(|attribute_set| *attribute_set != 0);
    for attribute_set in mesh_attribute_set
        .iter()
        .chain(record.triangle_attribute_sets.iter())
    {
        let attributes = ppc_q3_attributes_for_set(q3_attributes, *attribute_set);
        ppc_q3_view_material_merge_attributes(material, attributes);
    }
    true
}

pub fn ppc_q3_view_material_merge_attributes(
    material: &mut PpcQ3ViewMaterialRecord,
    attributes: Vec<PpcQ3AttributeRecord>,
) {
    if let Some(shader) = ppc_q3_attributes_surface_shader(&attributes) {
        material.shader = shader;
    }
    for attribute in attributes {
        if let Some(existing) = material
            .attributes
            .iter_mut()
            .find(|record| record.attribute_type == attribute.attribute_type)
        {
            *existing = attribute;
        } else {
            material.attributes.push(attribute);
        }
    }
}

pub fn ppc_q3_illumination_shader_type(q3_objects: &[PpcQ3ObjectRecord], shader: u32) -> Option<u32> {
    q3_objects
        .iter()
        .find(|record| record.object == shader)
        .map(|record| record.object_type)
        .filter(|object_type| ppc_q3_object_type_is_illumination_shader(*object_type))
}

pub fn ppc_q3_attributes_for_set(
    q3_attributes: &[PpcQ3AttributeRecord],
    attribute_set: u32,
) -> Vec<PpcQ3AttributeRecord> {
    q3_attributes
        .iter()
        .filter(|record| record.attribute_set == attribute_set)
        .cloned()
        .collect()
}

pub fn ppc_q3_attributes_surface_shader(attributes: &[PpcQ3AttributeRecord]) -> Option<u32> {
    attributes
        .iter()
        .find(|record| record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER)
        .and_then(ppc_q3_attribute_surface_shader_handle)
}

pub fn ppc_q3_attribute_surface_shader_handle(record: &PpcQ3AttributeRecord) -> Option<u32> {
    ppc_q3_attribute_surface_shader_handle_for_data(record.attribute_type, &record.data)
}

pub fn ppc_q3_attribute_surface_shader_handle_for_data(
    attribute_type: u32,
    data: &[u8],
) -> Option<u32> {
    if attribute_type != PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER {
        return None;
    }
    Some(u32::from_be_bytes([
        *data.first()?,
        *data.get(1)?,
        *data.get(2)?,
        *data.get(3)?,
    ]))
}

pub fn ppc_q3_attribute_surface_shader_accepts_object(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    surface_shader: u32,
) -> bool {
    let Some(object_type) = q3_objects
        .iter()
        .find(|record| record.object == surface_shader)
        .map(|record| record.object_type)
    else {
        return true;
    };
    if ppc_q3_object_type_is_surface_shader(object_type) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_view_transform_mut(
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    view: u32,
) -> &mut PpcQ3ViewTransformRecord {
    if let Some(index) = q3_view_transforms
        .iter()
        .position(|record| record.view == view)
    {
        return &mut q3_view_transforms[index];
    }
    q3_view_transforms.push(PpcQ3ViewTransformRecord::new(view));
    q3_view_transforms
        .last_mut()
        .expect("just pushed Q3 view transform record")
}

pub fn ppc_q3_view_transform_current_value(
    q3_view_transforms: &[PpcQ3ViewTransformRecord],
    view: u32,
) -> [[f32; 4]; 4] {
    q3_view_transforms
        .iter()
        .find(|record| record.view == view)
        .map(|record| record.local_to_world)
        .unwrap_or_else(ppc_q3_matrix4x4_identity)
}

pub fn ppc_q3_view_transform_current(
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    view: u32,
) -> [[f32; 4]; 4] {
    ppc_q3_view_transform_mut(q3_view_transforms, view).local_to_world
}

pub fn ppc_q3_view_transform_push_matrix(
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    view: u32,
    matrix: [[f32; 4]; 4],
) -> [[f32; 4]; 4] {
    let record = ppc_q3_view_transform_mut(q3_view_transforms, view);
    record.stack.push(matrix);
    record.local_to_world = ppc_q3_matrix4x4_multiply_values(record.local_to_world, matrix);
    record.local_to_world
}

pub fn ppc_q3_view_transform_reset(
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    view: u32,
) -> [[f32; 4]; 4] {
    let record = ppc_q3_view_transform_mut(q3_view_transforms, view);
    record.stack.clear();
    record.local_to_world = ppc_q3_matrix4x4_identity();
    record.local_to_world
}

pub fn ppc_q3_view_state_push(
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_view_transforms: &[PpcQ3ViewTransformRecord],
    q3_view_materials: &[PpcQ3ViewMaterialRecord],
    view: u32,
) -> [[f32; 4]; 4] {
    let transform = q3_view_transforms
        .iter()
        .find(|record| record.view == view)
        .cloned();
    let material = q3_view_materials
        .iter()
        .find(|record| record.view == view)
        .cloned();
    let local_to_world = transform
        .as_ref()
        .map(|record| record.local_to_world)
        .unwrap_or_else(ppc_q3_matrix4x4_identity);
    q3_view_state_stack.push(PpcQ3ViewStateSnapshotRecord {
        view,
        transform,
        material,
    });
    local_to_world
}

pub fn ppc_q3_view_state_pop(
    q3_view_state_stack: &mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    view: u32,
) -> Option<[[f32; 4]; 4]> {
    let index = q3_view_state_stack
        .iter()
        .rposition(|record| record.view == view)?;
    let snapshot = q3_view_state_stack.remove(index);
    ppc_q3_restore_view_transform(q3_view_transforms, view, snapshot.transform);
    ppc_q3_restore_view_material(q3_view_materials, view, snapshot.material);
    Some(ppc_q3_view_transform_current_value(
        q3_view_transforms,
        view,
    ))
}

pub fn ppc_q3_restore_view_transform(
    q3_view_transforms: &mut Vec<PpcQ3ViewTransformRecord>,
    view: u32,
    transform: Option<PpcQ3ViewTransformRecord>,
) {
    if let Some(transform) = transform {
        if let Some(index) = q3_view_transforms
            .iter()
            .position(|record| record.view == view)
        {
            q3_view_transforms[index] = transform;
        } else {
            q3_view_transforms.push(transform);
        }
    } else {
        q3_view_transforms.retain(|record| record.view != view);
    }
}

pub fn ppc_q3_restore_view_material(
    q3_view_materials: &mut Vec<PpcQ3ViewMaterialRecord>,
    view: u32,
    material: Option<PpcQ3ViewMaterialRecord>,
) {
    if let Some(material) = material {
        if let Some(index) = q3_view_materials
            .iter()
            .position(|record| record.view == view)
        {
            q3_view_materials[index] = material;
        } else {
            q3_view_materials.push(material);
        }
    } else {
        q3_view_materials.retain(|record| record.view != view);
    }
}

pub fn ppc_read_q3_fog_style_data(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
) -> Option<PpcQ3FogStyleData> {
    if data_ptr == 0 {
        return None;
    }
    let color_b_ptr = data_ptr.checked_add(PPC_Q3_FOG_STYLE_DATA_SIZE.checked_sub(4)?)?;
    Some(PpcQ3FogStyleData {
        state: memory.read_u32_be(data_ptr)?,
        mode: memory.read_u32_be(data_ptr.checked_add(4)?)?,
        fog_start: ppc_read_f32_be(memory, data_ptr.checked_add(8)?)?,
        fog_end: ppc_read_f32_be(memory, data_ptr.checked_add(12)?)?,
        density: ppc_read_f32_be(memory, data_ptr.checked_add(16)?)?,
        color: (
            ppc_read_f32_be(memory, data_ptr.checked_add(20)?)?,
            ppc_read_f32_be(memory, data_ptr.checked_add(24)?)?,
            ppc_read_f32_be(memory, data_ptr.checked_add(28)?)?,
            ppc_read_f32_be(memory, color_b_ptr)?,
        ),
    })
}

pub fn ppc_q3_fog_style_data_valid(data: PpcQ3FogStyleData) -> bool {
    data.state <= 1
        && (PPC_Q3_FOG_MODE_LINEAR..=PPC_Q3_FOG_MODE_ALPHA).contains(&data.mode)
        && data.fog_start.is_finite()
        && data.fog_end.is_finite()
        && data.density.is_finite()
        && data.color.0.is_finite()
        && data.color.1.is_finite()
        && data.color.2.is_finite()
        && data.color.3.is_finite()
}

pub fn ppc_q3_vector3d_normalize(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vector_ptr: u32,
    result_ptr: u32,
) -> u32 {
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, PPC_Q3_VECTOR3D_SIZE) {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Normalize invalid-out lr=${:08X} sp=${:08X} in=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], vector_ptr, result_ptr
            );
        }
        return 0;
    }
    let Some((x, y, z)) = ppc_read_q3_vector3d(memory, vector_ptr) else {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Normalize invalid-in lr=${:08X} sp=${:08X} in=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], vector_ptr, result_ptr
            );
        }
        return 0;
    };
    let length = (x.mul_add(x, y.mul_add(y, z * z))).sqrt();
    let result = if length > 0.0 {
        (x / length, y / length, z / length)
    } else {
        (0.0, 0.0, 0.0)
    };
    if ppc_write_q3_vector3d(memory, result_ptr, result).is_some() {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Normalize lr=${:08X} sp=${:08X} in=${:08X} out=${:08X} value=({:.6},{:.6},{:.6}) length={:.6} result=({:.6},{:.6},{:.6})",
                cpu.lr,
                cpu.gpr[1],
                vector_ptr,
                result_ptr,
                x,
                y,
                z,
                length,
                result.0,
                result.1,
                result.2
            );
        }
        result_ptr
    } else {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Normalize write-failed lr=${:08X} sp=${:08X} in=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], vector_ptr, result_ptr
            );
        }
        0
    }
}

pub fn ppc_q3_vector2d_normalize(memory: &mut PpcSectionMem, vector_ptr: u32, result_ptr: u32) -> u32 {
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, PPC_Q3_VECTOR2D_SIZE) {
        return 0;
    }
    let Some((x, y)) = ppc_read_q3_vector2d(memory, vector_ptr) else {
        return 0;
    };
    let length = (x.mul_add(x, y * y)).sqrt();
    let result = if length > 0.0 {
        (x / length, y / length)
    } else {
        (0.0, 0.0)
    };
    if ppc_write_q3_vector2d(memory, result_ptr, result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_vector3d_cross(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    left_ptr: u32,
    right_ptr: u32,
    result_ptr: u32,
) -> u32 {
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, PPC_Q3_VECTOR3D_SIZE) {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Cross invalid-out lr=${:08X} sp=${:08X} left=${:08X} right=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], left_ptr, right_ptr, result_ptr
            );
        }
        return 0;
    }
    let Some(left) = ppc_read_q3_vector3d(memory, left_ptr) else {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Cross invalid-left lr=${:08X} sp=${:08X} left=${:08X} right=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], left_ptr, right_ptr, result_ptr
            );
        }
        return 0;
    };
    let Some(right) = ppc_read_q3_vector3d(memory, right_ptr) else {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Cross invalid-right lr=${:08X} sp=${:08X} left=${:08X} right=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], left_ptr, right_ptr, result_ptr
            );
        }
        return 0;
    };
    let result = ppc_q3_vector3d_cross_values(left, right);
    if ppc_write_q3_vector3d(memory, result_ptr, result).is_some() {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Cross lr=${:08X} sp=${:08X} left=${:08X} right=${:08X} out=${:08X} left=({:.6},{:.6},{:.6}) right=({:.6},{:.6},{:.6}) result=({:.6},{:.6},{:.6})",
                cpu.lr,
                cpu.gpr[1],
                left_ptr,
                right_ptr,
                result_ptr,
                left.0,
                left.1,
                left.2,
                right.0,
                right.1,
                right.2,
                result.0,
                result.1,
                result.2
            );
        }
        result_ptr
    } else {
        if qd3d_collision_trace_enabled() {
            println!(
                "[QD3D-COLLISION] Vector3D_Cross write-failed lr=${:08X} sp=${:08X} left=${:08X} right=${:08X} out=${:08X}",
                cpu.lr, cpu.gpr[1], left_ptr, right_ptr, result_ptr
            );
        }
        0
    }
}

pub fn ppc_q3_point2d_distance(memory: &mut PpcSectionMem, first_ptr: u32, second_ptr: u32) -> f32 {
    let Some((x1, y1)) = ppc_read_q3_vector2d(memory, first_ptr) else {
        return 0.0;
    };
    let Some((x2, y2)) = ppc_read_q3_vector2d(memory, second_ptr) else {
        return 0.0;
    };
    let dx = x1 - x2;
    let dy = y1 - y2;
    (dx.mul_add(dx, dy * dy)).sqrt()
}

pub fn ppc_q3_point3d_distance(memory: &mut PpcSectionMem, first_ptr: u32, second_ptr: u32) -> f32 {
    let Some((x1, y1, z1)) = ppc_read_q3_vector3d(memory, first_ptr) else {
        return 0.0;
    };
    let Some((x2, y2, z2)) = ppc_read_q3_vector3d(memory, second_ptr) else {
        return 0.0;
    };
    let dx = x1 - x2;
    let dy = y1 - y2;
    let dz = z1 - z2;
    (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt()
}

pub fn ppc_q3_vector3d_length(memory: &mut PpcSectionMem, vector_ptr: u32) -> f32 {
    let Some((x, y, z)) = ppc_read_q3_vector3d(memory, vector_ptr) else {
        return 0.0;
    };
    (x.mul_add(x, y.mul_add(y, z * z))).sqrt()
}

pub fn ppc_q3_bounding_box_set_from_points3d(
    memory: &mut PpcSectionMem,
    result_ptr: u32,
    points_ptr: u32,
    count: u32,
    stride: u32,
) -> u32 {
    if result_ptr == 0
        || !ppc_memory_can_write_bytes(memory, result_ptr, PPC_Q3_BOUNDING_BOX_SIZE)
        || count > PPC_Q3_SOFTWARE_RENDER_MAX_POINTS
        || (count > 0 && (points_ptr == 0 || stride < PPC_Q3_POINT3D_SIZE))
    {
        return 0;
    }
    let mut min = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for index in 0..count {
        let Some(point_ptr) = index
            .checked_mul(stride)
            .and_then(|offset| points_ptr.checked_add(offset))
        else {
            return 0;
        };
        let Some(point) = ppc_read_q3_vector3d(memory, point_ptr) else {
            return 0;
        };
        if !ppc_q3_point_finite(point) {
            return 0;
        }
        min.0 = min.0.min(point.0);
        min.1 = min.1.min(point.1);
        min.2 = min.2.min(point.2);
        max.0 = max.0.max(point.0);
        max.1 = max.1.max(point.1);
        max.2 = max.2.max(point.2);
    }
    if count == 0 {
        min = (0.0, 0.0, 0.0);
        max = min;
    }
    let ok = ppc_write_q3_vector3d(memory, result_ptr, min).is_some()
        && ppc_write_q3_vector3d(memory, result_ptr + PPC_Q3_BOUNDING_BOX_MAX_OFFSET, max)
            .is_some()
        && memory
            .write_u32_be(result_ptr + PPC_Q3_BOUNDING_BOX_IS_EMPTY_OFFSET, u32::from(count == 0))
            .is_some();
    if ok { result_ptr } else { 0 }
}

pub fn ppc_q3_point3d_cross_product_tri(
    memory: &mut PpcSectionMem,
    first_ptr: u32,
    second_ptr: u32,
    third_ptr: u32,
    result_ptr: u32,
) -> u32 {
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, PPC_Q3_VECTOR3D_SIZE) {
        return 0;
    }
    let Some(first) = ppc_read_q3_vector3d(memory, first_ptr) else {
        return 0;
    };
    let Some(second) = ppc_read_q3_vector3d(memory, second_ptr) else {
        return 0;
    };
    let Some(third) = ppc_read_q3_vector3d(memory, third_ptr) else {
        return 0;
    };
    let first_edge = (second.0 - first.0, second.1 - first.1, second.2 - first.2);
    let second_edge = (third.0 - first.0, third.1 - first.1, third.2 - first.2);
    let result = ppc_q3_vector3d_cross_values(first_edge, second_edge);
    if ppc_write_q3_vector3d(memory, result_ptr, result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix3x3_set_translate(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    x: f32,
    y: f32,
) -> u32 {
    let mut matrix = [[0.0; 3]; 3];
    matrix[0][0] = 1.0;
    matrix[1][1] = 1.0;
    matrix[2][0] = x;
    matrix[2][1] = y;
    matrix[2][2] = 1.0;
    if ppc_write_q3_matrix3x3(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_identity(memory: &mut PpcSectionMem, matrix_ptr: u32) -> u32 {
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &ppc_q3_matrix4x4_identity()).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_translate(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    x: f32,
    y: f32,
    z: f32,
) -> u32 {
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = x;
    matrix[3][1] = y;
    matrix[3][2] = z;
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_scale(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    x: f32,
    y: f32,
    z: f32,
) -> u32 {
    let mut matrix = [[0.0; 4]; 4];
    matrix[0][0] = x;
    matrix[1][1] = y;
    matrix[2][2] = z;
    matrix[3][3] = 1.0;
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_rotate_x(memory: &mut PpcSectionMem, matrix_ptr: u32, angle: f32) -> u32 {
    let matrix = ppc_q3_matrix4x4_rotate_x(angle);
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_rotate_y(memory: &mut PpcSectionMem, matrix_ptr: u32, angle: f32) -> u32 {
    let matrix = ppc_q3_matrix4x4_rotate_y(angle);
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_rotate_z(memory: &mut PpcSectionMem, matrix_ptr: u32, angle: f32) -> u32 {
    let matrix = ppc_q3_matrix4x4_rotate_z(angle);
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_set_rotate_xyz(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    x_angle: f32,
    y_angle: f32,
    z_angle: f32,
) -> u32 {
    let matrix = ppc_q3_matrix4x4_multiply_values(
        ppc_q3_matrix4x4_multiply_values(
            ppc_q3_matrix4x4_rotate_x(x_angle),
            ppc_q3_matrix4x4_rotate_y(y_angle),
        ),
        ppc_q3_matrix4x4_rotate_z(z_angle),
    );
    if ppc_write_q3_matrix4x4(memory, matrix_ptr, &matrix).is_some() {
        matrix_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_multiply(
    memory: &mut PpcSectionMem,
    left_ptr: u32,
    right_ptr: u32,
    result_ptr: u32,
) -> u32 {
    let Some(left) = ppc_read_q3_matrix4x4(memory, left_ptr) else {
        return 0;
    };
    let Some(right) = ppc_read_q3_matrix4x4(memory, right_ptr) else {
        return 0;
    };
    let result = ppc_q3_matrix4x4_multiply_values(left, right);
    if ppc_write_q3_matrix4x4(memory, result_ptr, &result).is_some() {
        result_ptr
    } else {
        0
    }
}


// --- QuickDraw 3D Math Transformations ---

pub fn ppc_q3_matrix4x4_transpose(memory: &mut PpcSectionMem, matrix_ptr: u32, result_ptr: u32) -> u32 {
    let Some(matrix) = ppc_read_q3_matrix4x4(memory, matrix_ptr) else {
        return 0;
    };
    let result = ppc_q3_matrix4x4_transpose_values(matrix);
    if ppc_write_q3_matrix4x4(memory, result_ptr, &result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix4x4_invert(memory: &mut PpcSectionMem, matrix_ptr: u32, result_ptr: u32) -> u32 {
    let Some(matrix) = ppc_read_q3_matrix4x4(memory, matrix_ptr) else {
        return 0;
    };
    let Some(result) = ppc_q3_matrix4x4_invert_values(matrix) else {
        return 0;
    };
    if ppc_write_q3_matrix4x4(memory, result_ptr, &result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_point3d_transform(
    memory: &mut PpcSectionMem,
    point_ptr: u32,
    matrix_ptr: u32,
    result_ptr: u32,
) -> u32 {
    let Some(point) = ppc_read_q3_vector3d(memory, point_ptr) else {
        return 0;
    };
    let Some(matrix) = ppc_read_q3_matrix4x4(memory, matrix_ptr) else {
        return 0;
    };
    let result = ppc_q3_point3d_transform_values(point, matrix);
    if ppc_write_q3_vector3d(memory, result_ptr, result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_point3d_transform_array(
    memory: &mut PpcSectionMem,
    in_vertex_ptr: u32,
    matrix_ptr: u32,
    out_vertex_ptr: u32,
    num_vertices: u32,
    in_struct_size: u32,
    out_struct_size: u32,
    output_4d: bool,
) -> bool {
    let count = num_vertices as i32;
    if count < 0 {
        return false;
    }
    if count == 0 {
        return true;
    }
    let output_size = if output_4d { 16 } else { 12 };
    if in_vertex_ptr == 0
        || matrix_ptr == 0
        || out_vertex_ptr == 0
        || in_struct_size < 12
        || out_struct_size < output_size
    {
        return false;
    }
    let Some(matrix) = ppc_read_q3_matrix4x4(memory, matrix_ptr) else {
        return false;
    };

    let mut points = Vec::with_capacity(count as usize);
    for index in 0..(count as u32) {
        let Some(offset) = index.checked_mul(in_struct_size) else {
            return false;
        };
        let Some(point_ptr) = in_vertex_ptr.checked_add(offset) else {
            return false;
        };
        let Some(point) = ppc_read_q3_vector3d(memory, point_ptr) else {
            return false;
        };
        points.push(point);
    }
    for (index, point) in points.into_iter().enumerate() {
        let Some(offset) = (index as u32).checked_mul(out_struct_size) else {
            return false;
        };
        let Some(point_ptr) = out_vertex_ptr.checked_add(offset) else {
            return false;
        };
        if output_4d {
            let transformed = ppc_q3_point3d_transform_4d_values(point, matrix);
            if ppc_write_q3_rational_point4d(memory, point_ptr, transformed).is_none() {
                return false;
            }
        } else if ppc_write_q3_vector3d(
            memory,
            point_ptr,
            ppc_q3_point3d_transform_values(point, matrix),
        )
        .is_none()
        {
            return false;
        }
    }
    true
}

pub fn ppc_q3_vector3d_transform(
    memory: &mut PpcSectionMem,
    vector_ptr: u32,
    matrix_ptr: u32,
    result_ptr: u32,
) -> u32 {
    let Some(vector) = ppc_read_q3_vector3d(memory, vector_ptr) else {
        return 0;
    };
    let Some(matrix) = ppc_read_q3_matrix4x4(memory, matrix_ptr) else {
        return 0;
    };
    let result = ppc_q3_vector3d_transform_values(vector, matrix);
    if ppc_write_q3_vector3d(memory, result_ptr, result).is_some() {
        result_ptr
    } else {
        0
    }
}

pub fn ppc_q3_matrix_transform_new(
    cpu: &PpcCpu,
    q3_objects: &mut Vec<PpcQ3ObjectRecord>,
    next_q3_object: &mut u32,
) -> u32 {
    let matrix_ptr = cpu.gpr[3];
    if matrix_ptr == 0 {
        return 0;
    }
    ppc_q3_alloc_object(
        q3_objects,
        next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TRANSFORM_TYPE_MATRIX,
        matrix_ptr,
        64,
    )
}

pub fn ppc_q3_matrix_transform_set(
    cpu: &PpcCpu,
    q3_objects: &mut [PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let transform = cpu.gpr[3];
    let matrix_ptr = cpu.gpr[4];
    if transform == 0 || matrix_ptr == 0 {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    let Some(record) = q3_objects
        .iter_mut()
        .find(|record| record.object == transform)
    else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    };
    if record.object_type != PPC_Q3_TRANSFORM_TYPE_MATRIX {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        return false;
    }
    record.data_ptr = matrix_ptr;
    record.data_size = 64;
    true
}

pub fn ppc_q3_validate_transform_handle(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
    transform: u32,
) -> bool {
    if ppc_q3_object_type_is_transform(ppc_q3_object_type_for_handle(q3_objects, transform)) {
        true
    } else {
        q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
        false
    }
}

pub fn ppc_q3_matrix_transform_matrix_ptr(record: &PpcQ3ObjectRecord) -> Option<u32> {
    if record.object_type == PPC_Q3_TRANSFORM_TYPE_MATRIX && record.data_size >= 72 {
        record.data_ptr.checked_add(8)
    } else if record.data_size >= 64 {
        Some(record.data_ptr)
    } else {
        None
    }
}

pub fn ppc_q3_transform_get_matrix(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    q3_objects: &[PpcQ3ObjectRecord],
    q3_error_state: &mut PpcQ3ErrorState,
) -> bool {
    let transform = cpu.gpr[3];
    let matrix_out_ptr = cpu.gpr[4];
    if matrix_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, matrix_out_ptr, PPC_Q3_MATRIX4X4_SIZE)
    {
        return false;
    }
    if !ppc_q3_validate_transform_handle(q3_objects, q3_error_state, transform) {
        return false;
    }
    let Some(record) = q3_objects.iter().find(|record| record.object == transform) else {
        return false;
    };
    let Some(matrix_ptr) = ppc_q3_matrix_transform_matrix_ptr(record) else {
        return false;
    };
    matrix_ptr != 0 && ppc_q3_copy_or_zero(memory, matrix_ptr, matrix_out_ptr, 64)
}

pub fn ppc_q3_vector3d_cross_values(left: (f32, f32, f32), right: (f32, f32, f32)) -> (f32, f32, f32) {
    (
        left.1.mul_add(right.2, -(left.2 * right.1)),
        left.2.mul_add(right.0, -(left.0 * right.2)),
        left.0.mul_add(right.1, -(left.1 * right.0)),
    )
}

pub fn ppc_q3_vector3d_sub(left: (f32, f32, f32), right: (f32, f32, f32)) -> (f32, f32, f32) {
    (left.0 - right.0, left.1 - right.1, left.2 - right.2)
}

pub fn ppc_q3_vector3d_dot(left: (f32, f32, f32), right: (f32, f32, f32)) -> f32 {
    left.0
        .mul_add(right.0, left.1.mul_add(right.1, left.2 * right.2))
}

pub fn ppc_q3_vector3d_normalized_value(vector: (f32, f32, f32)) -> Option<(f32, f32, f32)> {
    let length = ppc_q3_vector3d_dot(vector, vector).sqrt();
    if !length.is_finite() || length == 0.0 {
        return None;
    }
    Some((vector.0 / length, vector.1 / length, vector.2 / length))
}

pub fn ppc_read_q3_vector2d(memory: &mut PpcSectionMem, vector_ptr: u32) -> Option<(f32, f32)> {
    Some((
        ppc_read_f32_be(memory, vector_ptr)?,
        ppc_read_f32_be(memory, vector_ptr + 4)?,
    ))
}

pub fn ppc_read_q3_vector3d(memory: &mut PpcSectionMem, vector_ptr: u32) -> Option<(f32, f32, f32)> {
    Some((
        ppc_read_f32_be(memory, vector_ptr)?,
        ppc_read_f32_be(memory, vector_ptr + 4)?,
        ppc_read_f32_be(memory, vector_ptr + 8)?,
    ))
}

pub fn ppc_read_q3_color_rgb(memory: &mut PpcSectionMem, color_ptr: u32) -> Option<(f32, f32, f32)> {
    ppc_read_q3_vector3d(memory, color_ptr)
}

pub fn ppc_write_q3_vector2d(
    memory: &mut PpcSectionMem,
    vector_ptr: u32,
    (x, y): (f32, f32),
) -> Option<()> {
    ppc_write_f32_be(memory, vector_ptr, x)?;
    ppc_write_f32_be(memory, vector_ptr + 4, y)?;
    Some(())
}

pub fn ppc_write_q3_vector3d(
    memory: &mut PpcSectionMem,
    vector_ptr: u32,
    (x, y, z): (f32, f32, f32),
) -> Option<()> {
    ppc_write_f32_be(memory, vector_ptr, x)?;
    ppc_write_f32_be(memory, vector_ptr + 4, y)?;
    ppc_write_f32_be(memory, vector_ptr + 8, z)?;
    Some(())
}

pub fn ppc_write_q3_area(
    memory: &mut PpcSectionMem,
    area_ptr: u32,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Option<()> {
    ppc_write_q3_vector2d(memory, area_ptr, (min_x, min_y))?;
    ppc_write_q3_vector2d(memory, area_ptr.checked_add(8)?, (max_x, max_y))?;
    Some(())
}

pub fn ppc_write_q3_color_rgb(
    memory: &mut PpcSectionMem,
    color_ptr: u32,
    color: (f32, f32, f32),
) -> Option<()> {
    ppc_write_q3_vector3d(memory, color_ptr, color)
}

pub fn ppc_write_q3_rational_point4d(
    memory: &mut PpcSectionMem,
    point_ptr: u32,
    (x, y, z, w): (f32, f32, f32, f32),
) -> Option<()> {
    ppc_write_f32_be(memory, point_ptr, x)?;
    ppc_write_f32_be(memory, point_ptr + 4, y)?;
    ppc_write_f32_be(memory, point_ptr + 8, z)?;
    ppc_write_f32_be(memory, point_ptr + 12, w)?;
    Some(())
}

pub fn ppc_read_q3_matrix4x4(memory: &mut PpcSectionMem, matrix_ptr: u32) -> Option<[[f32; 4]; 4]> {
    let mut matrix = [[0.0; 4]; 4];
    for (row, values) in matrix.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            let offset = ((row * 4 + column) * 4) as u32;
            *value = ppc_read_f32_be(memory, matrix_ptr.checked_add(offset)?)?;
        }
    }
    Some(matrix)
}

pub fn ppc_read_q3_matrix3x3(memory: &mut PpcSectionMem, matrix_ptr: u32) -> Option<[[f32; 3]; 3]> {
    let mut matrix = [[0.0; 3]; 3];
    for (row, values) in matrix.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            let offset = ((row * 3 + column) * 4) as u32;
            *value = ppc_read_f32_be(memory, matrix_ptr.checked_add(offset)?)?;
        }
    }
    Some(matrix)
}

pub fn ppc_write_q3_matrix3x3(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    matrix: &[[f32; 3]; 3],
) -> Option<()> {
    for (row, values) in matrix.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let offset = ((row * 3 + column) * 4) as u32;
            ppc_write_f32_be(memory, matrix_ptr.checked_add(offset)?, *value)?;
        }
    }
    Some(())
}

pub fn ppc_write_q3_matrix4x4(
    memory: &mut PpcSectionMem,
    matrix_ptr: u32,
    matrix: &[[f32; 4]; 4],
) -> Option<()> {
    for (row, values) in matrix.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let offset = ((row * 4 + column) * 4) as u32;
            ppc_write_f32_be(memory, matrix_ptr.checked_add(offset)?, *value)?;
        }
    }
    Some(())
}



pub fn ppc_q3_matrix3x3_identity() -> [[f32; 3]; 3] {
    let mut matrix = [[0.0; 3]; 3];
    for (index, row) in matrix.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    matrix
}

pub fn ppc_q3_matrix4x4_rotate_x(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[1][1] = cos;
    matrix[1][2] = sin;
    matrix[2][1] = -sin;
    matrix[2][2] = cos;
    matrix
}

pub fn ppc_q3_matrix4x4_rotate_y(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[0][0] = cos;
    matrix[0][2] = -sin;
    matrix[2][0] = sin;
    matrix[2][2] = cos;
    matrix
}

pub fn ppc_q3_matrix4x4_rotate_z(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[0][0] = cos;
    matrix[0][1] = sin;
    matrix[1][0] = -sin;
    matrix[1][1] = cos;
    matrix
}

pub fn ppc_q3_matrix4x4_multiply_values(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] = left[row][0].mul_add(
                right[0][column],
                left[row][1].mul_add(
                    right[1][column],
                    left[row][2].mul_add(right[2][column], left[row][3] * right[3][column]),
                ),
            );
        }
    }
    result
}

pub fn ppc_q3_matrix4x4_transpose_values(matrix: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] = matrix[column][row];
        }
    }
    result
}

pub fn ppc_q3_point3d_transform_values(
    (x, y, z): (f32, f32, f32),
    matrix: [[f32; 4]; 4],
) -> (f32, f32, f32) {
    let tx = x.mul_add(
        matrix[0][0],
        y.mul_add(matrix[1][0], z.mul_add(matrix[2][0], matrix[3][0])),
    );
    let ty = x.mul_add(
        matrix[0][1],
        y.mul_add(matrix[1][1], z.mul_add(matrix[2][1], matrix[3][1])),
    );
    let tz = x.mul_add(
        matrix[0][2],
        y.mul_add(matrix[1][2], z.mul_add(matrix[2][2], matrix[3][2])),
    );
    let tw = x.mul_add(
        matrix[0][3],
        y.mul_add(matrix[1][3], z.mul_add(matrix[2][3], matrix[3][3])),
    );
    if tw != 0.0 && tw != 1.0 {
        (tx / tw, ty / tw, tz / tw)
    } else {
        (tx, ty, tz)
    }
}

pub fn ppc_q3_point3d_transform_4d_values(
    (x, y, z): (f32, f32, f32),
    matrix: [[f32; 4]; 4],
) -> (f32, f32, f32, f32) {
    (
        x.mul_add(
            matrix[0][0],
            y.mul_add(matrix[1][0], z.mul_add(matrix[2][0], matrix[3][0])),
        ),
        x.mul_add(
            matrix[0][1],
            y.mul_add(matrix[1][1], z.mul_add(matrix[2][1], matrix[3][1])),
        ),
        x.mul_add(
            matrix[0][2],
            y.mul_add(matrix[1][2], z.mul_add(matrix[2][2], matrix[3][2])),
        ),
        x.mul_add(
            matrix[0][3],
            y.mul_add(matrix[1][3], z.mul_add(matrix[2][3], matrix[3][3])),
        ),
    )
}

pub fn ppc_q3_vector3d_transform_values(
    (x, y, z): (f32, f32, f32),
    matrix: [[f32; 4]; 4],
) -> (f32, f32, f32) {
    (
        x.mul_add(matrix[0][0], y.mul_add(matrix[1][0], z * matrix[2][0])),
        x.mul_add(matrix[0][1], y.mul_add(matrix[1][1], z * matrix[2][1])),
        x.mul_add(matrix[0][2], y.mul_add(matrix[1][2], z * matrix[2][2])),
    )
}

pub fn ppc_q3_matrix4x4_invert_values(matrix: [[f32; 4]; 4]) -> Option<[[f32; 4]; 4]> {
    let mut augmented = [[0.0f64; 8]; 4];
    for row in 0..4 {
        for column in 0..4 {
            augmented[row][column] = f64::from(matrix[row][column]);
        }
        augmented[row][4 + row] = 1.0;
    }
    for pivot_column in 0..4 {
        let mut pivot_row = pivot_column;
        for candidate in (pivot_column + 1)..4 {
            if augmented[candidate][pivot_column].abs() > augmented[pivot_row][pivot_column].abs() {
                pivot_row = candidate;
            }
        }
        let pivot = augmented[pivot_row][pivot_column];
        if pivot.abs() <= f64::EPSILON {
            return None;
        }
        if pivot_row != pivot_column {
            augmented.swap(pivot_row, pivot_column);
        }
        let pivot = augmented[pivot_column][pivot_column];
        for column in 0..8 {
            augmented[pivot_column][column] /= pivot;
        }
        for row in 0..4 {
            if row == pivot_column {
                continue;
            }
            let factor = augmented[row][pivot_column];
            if factor == 0.0 {
                continue;
            }
            for column in 0..8 {
                augmented[row][column] -= factor * augmented[pivot_column][column];
            }
        }
    }
    let mut inverse = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            inverse[row][column] = augmented[row][4 + column] as f32;
        }
    }
    Some(inverse)
}

pub fn ppc_fpr_as_f32(cpu: &PpcCpu, index: usize) -> f32 {
    f64::from_bits(cpu.fpr[index]) as f32
}

pub fn ppc_read_f32_be(memory: &mut PpcSectionMem, addr: u32) -> Option<f32> {
    Some(f32::from_bits(memory.read_u32_be(addr)?))
}

pub fn ppc_write_f32_be(memory: &mut PpcSectionMem, addr: u32, value: f32) -> Option<()> {
    memory.write_u32_be(addr, value.to_bits())
}

pub(crate) fn ppc_qa_engine_gestalt(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let engine = cpu.gpr[3];
    let selector = cpu.gpr[4];
    let response = cpu.gpr[5];
    if engine != PPC_QA_ENGINE || response == 0 {
        return ppc_i16_result(PPC_PARAM_ERR);
    }

    let response_size = if selector == 6 {
        PPC_QA_ENGINE_NAME.len() as u32 + 1
    } else {
        4
    };
    if !ppc_memory_can_write_bytes(memory, response, response_size) {
        return ppc_i16_result(PPC_PARAM_ERR);
    }

    let write_result = match selector {
        0 => memory.write_u32_be(response, ppc_qa_optional_features()),
        1 => memory.write_u32_be(response, ppc_qa_fast_features()),
        2 => memory.write_u32_be(response, PPC_QA_VENDOR_APPLE),
        3 => memory.write_u32_be(response, PPC_QA_ENGINE_APPLE_SW),
        4 => memory.write_u32_be(response, 1),
        5 => memory.write_u32_be(response, PPC_QA_ENGINE_NAME.len() as u32),
        6 => ppc_write_c_string(memory, response, PPC_QA_ENGINE_NAME),
        7 => memory.write_u32_be(response, PPC_QA_AVAILABLE_TEXTURE_MEMORY),
        _ => memory.write_u32_be(response, 0),
    };

    if write_result.is_some() {
        0
    } else {
        ppc_i16_result(PPC_PARAM_ERR)
    }
}

pub(crate) fn ppc_qa_optional_features() -> u32 {
    PPC_QA_OPTIONAL_TEXTURE | PPC_QA_OPTIONAL_TEXTURE_COLOR | PPC_QA_OPTIONAL_PERSPECTIVE_Z
}

pub(crate) fn ppc_qa_fast_features() -> u32 {
    PPC_QA_FAST_LINE | PPC_QA_FAST_GOURAUD | PPC_QA_FAST_TEXTURE
}

fn ppc_write_c_string(memory: &mut PpcSectionMem, ptr: u32, bytes: &[u8]) -> Option<()> {
    for (offset, byte) in bytes.iter().copied().enumerate() {
        memory.write_u8(ptr + offset as u32, byte)?;
    }
    memory.write_u8(ptr + bytes.len() as u32, 0)?;
    Some(())
}

