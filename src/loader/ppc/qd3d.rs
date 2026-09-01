//! QuickDraw 3D types, records, scene representation, and GPU frame generation.

use super::graphics::PpcFrontBuffer;
use serde::{Deserialize, Serialize};

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
