//! QuickDraw 3D types, records, scene representation, and GPU frame generation.

use super::{
    graphics::{PpcFrontBuffer, PpcGWorldRecord},
    ppc_front_buffer_for_gworld, ppc_live_quickdraw_surface, ppc_q3_camera_view_to_frustum_matrix,
    ppc_q3_camera_world_to_view_matrix, ppc_q3_matrix4x4_invert_values,
    ppc_q3_matrix4x4_transpose_values, ppc_q3_object_type_is_illumination_shader,
    ppc_q3_point3d_transform_4d_values, ppc_q3_point3d_transform_values,
    ppc_q3_vector3d_cross_values, ppc_q3_vector3d_dot, ppc_q3_vector3d_normalized_value,
    ppc_q3_vector3d_sub, ppc_q3_vector3d_transform_values, ppc_quickdraw_read_pixel,
    ppc_quickdraw_write_raw_pixel, ppc_rgb555_to_clut_index, qd3d_trimesh_trace_enabled, PpcCpu,
    PpcSectionMem,
};
use crate::memory::GuestWritableSpan;
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
