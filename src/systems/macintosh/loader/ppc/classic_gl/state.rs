//! Fixed-function OpenGL 1.2 context state (OpenGL 1.2.1 specification,
//! chapter 6 state tables).

use super::consts::*;
use super::math::Mat4;
use super::texture::{PixelStore, PixelTransfer, Texture};
use std::collections::HashMap;

pub const TEXTURE_UNITS: usize = 2;
pub const MAX_LIGHTS_COUNT: usize = 8;
pub const MAX_CLIP_PLANES_COUNT: usize = 6;
pub const MAX_STACK_DEPTH: usize = 32;
pub const MAX_PROJECTION_STACK: usize = 4;
pub const MAX_TEXTURE_STACK: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct Light {
    pub enabled: bool,
    pub ambient: [f32; 4],
    pub diffuse: [f32; 4],
    pub specular: [f32; 4],
    /// Eye-space position (w = 0 for directional lights).
    pub position: [f32; 4],
    /// Eye-space spot direction.
    pub spot_direction: [f32; 3],
    pub spot_exponent: f32,
    pub spot_cutoff: f32,
    pub attenuation: [f32; 3],
}

impl Light {
    fn new(index: usize) -> Self {
        let primary = if index == 0 { 1.0 } else { 0.0 };
        Self {
            enabled: false,
            ambient: [0.0, 0.0, 0.0, 1.0],
            diffuse: [primary, primary, primary, 1.0],
            specular: [primary, primary, primary, 1.0],
            position: [0.0, 0.0, 1.0, 0.0],
            spot_direction: [0.0, 0.0, -1.0],
            spot_exponent: 0.0,
            spot_cutoff: 180.0,
            attenuation: [1.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub ambient: [f32; 4],
    pub diffuse: [f32; 4],
    pub specular: [f32; 4],
    pub emission: [f32; 4],
    pub shininess: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            ambient: [0.2, 0.2, 0.2, 1.0],
            diffuse: [0.8, 0.8, 0.8, 1.0],
            specular: [0.0, 0.0, 0.0, 1.0],
            emission: [0.0, 0.0, 0.0, 1.0],
            shininess: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TexEnv {
    pub mode: u32,
    pub color: [f32; 4],
    pub combine_rgb: u32,
    pub combine_alpha: u32,
    pub source_rgb: [u32; 3],
    pub source_alpha: [u32; 3],
    pub operand_rgb: [u32; 3],
    pub operand_alpha: [u32; 3],
    pub rgb_scale: f32,
    pub alpha_scale: f32,
}

impl Default for TexEnv {
    fn default() -> Self {
        Self {
            mode: MODULATE,
            color: [0.0; 4],
            combine_rgb: MODULATE,
            combine_alpha: MODULATE,
            source_rgb: [TEXTURE, PREVIOUS, CONSTANT],
            source_alpha: [TEXTURE, PREVIOUS, CONSTANT],
            operand_rgb: [SRC_COLOR, SRC_COLOR, SRC_ALPHA],
            operand_alpha: [SRC_ALPHA, SRC_ALPHA, SRC_ALPHA],
            rgb_scale: 1.0,
            alpha_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TexGen {
    pub enabled: bool,
    pub mode: u32,
    pub object_plane: [f32; 4],
    pub eye_plane: [f32; 4],
}

impl TexGen {
    fn new(coordinate: usize) -> Self {
        let mut plane = [0.0; 4];
        if coordinate < 2 {
            plane[coordinate] = 1.0;
        }
        Self {
            enabled: false,
            mode: EYE_LINEAR,
            object_plane: plane,
            eye_plane: plane,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextureUnit {
    pub enabled_2d: bool,
    pub enabled_1d: bool,
    pub binding_2d: u32,
    pub env: TexEnv,
    pub gen: [TexGen; 4],
}

impl Default for TextureUnit {
    fn default() -> Self {
        Self {
            enabled_2d: false,
            enabled_1d: false,
            binding_2d: 0,
            env: TexEnv::default(),
            gen: [
                TexGen::new(0),
                TexGen::new(1),
                TexGen::new(2),
                TexGen::new(3),
            ],
        }
    }
}

/// One client vertex array (OpenGL 1.2.1, section 2.8).
#[derive(Debug, Clone, Copy, Default)]
pub struct ClientArray {
    pub enabled: bool,
    pub size: u32,
    pub kind: u32,
    pub stride: u32,
    pub pointer: u32,
}

/// Server state saved by glPushAttrib. Only state that this renderer keeps is
/// recorded; the whole server block is restored regardless of `mask`, which
/// is a superset of the requested groups.
#[derive(Debug, Clone)]
pub struct Raster {
    pub position: super::bitmap::RasterPosition,
    pub enables: Enables,
    pub cull_face_mode: u32,
    pub front_face: u32,
    pub shade_model: u32,
    pub polygon_mode: [u32; 2],
    pub polygon_offset: [f32; 2],
    pub line_width: f32,
    pub point_size: f32,
    pub point_size_min: f32,
    pub point_size_max: f32,
    pub point_attenuation: [f32; 3],
    pub line_stipple: (i32, u16),
    pub depth_func: u32,
    pub depth_mask: bool,
    pub depth_clear: f32,
    pub alpha_func: u32,
    pub alpha_ref: f32,
    pub blend_src: u32,
    pub blend_dst: u32,
    pub blend_color: [f32; 4],
    pub stencil_func: u32,
    pub stencil_ref: i32,
    pub stencil_value_mask: u32,
    pub stencil_write_mask: u32,
    pub stencil_fail: u32,
    pub stencil_zfail: u32,
    pub stencil_zpass: u32,
    pub stencil_clear: i32,
    pub color_mask: [bool; 4],
    pub clear_color: [f32; 4],
    pub scissor: [i32; 4],
    pub viewport: [i32; 4],
    pub depth_range: [f32; 2],
    pub draw_buffer: u32,
    pub read_buffer: u32,
    pub fog_mode: u32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_color: [f32; 4],
    pub lights: [Light; MAX_LIGHTS_COUNT],
    pub light_model_ambient: [f32; 4],
    pub light_model_local_viewer: bool,
    pub light_model_two_side: bool,
    pub separate_specular: bool,
    pub materials: [Material; 2],
    pub color_material_face: u32,
    pub color_material_mode: u32,
    pub clip_planes: [[f32; 4]; MAX_CLIP_PLANES_COUNT],
    pub units: [TextureUnit; TEXTURE_UNITS],
    pub active_unit: usize,
    pub hints: [u32; 5],
    pub matrix_mode: u32,
    pub list_base: u32,
    pub transfer: PixelTransfer,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Enables {
    pub alpha_test: bool,
    pub blend: bool,
    pub color_material: bool,
    pub cull_face: bool,
    pub depth_test: bool,
    pub dither: bool,
    pub fog: bool,
    pub lighting: bool,
    pub line_smooth: bool,
    pub line_stipple: bool,
    pub normalize: bool,
    pub point_smooth: bool,
    pub polygon_offset_fill: bool,
    pub polygon_offset_line: bool,
    pub polygon_offset_point: bool,
    pub polygon_smooth: bool,
    pub polygon_stipple: bool,
    pub rescale_normal: bool,
    pub scissor_test: bool,
    pub stencil_test: bool,
    pub color_logic_op: bool,
    pub auto_normal: bool,
    pub clip_planes: [bool; MAX_CLIP_PLANES_COUNT],
}

impl Default for Raster {
    fn default() -> Self {
        Self {
            position: super::bitmap::RasterPosition::default(),
            enables: Enables {
                dither: true,
                ..Enables::default()
            },
            cull_face_mode: BACK,
            front_face: CCW,
            shade_model: SMOOTH,
            polygon_mode: [FILL, FILL],
            polygon_offset: [0.0, 0.0],
            line_width: 1.0,
            point_size: 1.0,
            point_size_min: 0.0,
            point_size_max: 64.0,
            point_attenuation: [1.0, 0.0, 0.0],
            line_stipple: (1, 0xFFFF),
            depth_func: LESS,
            depth_mask: true,
            depth_clear: 1.0,
            alpha_func: ALWAYS,
            alpha_ref: 0.0,
            blend_src: ONE,
            blend_dst: ZERO,
            blend_color: [0.0; 4],
            stencil_func: ALWAYS,
            stencil_ref: 0,
            stencil_value_mask: u32::MAX,
            stencil_write_mask: u32::MAX,
            stencil_fail: KEEP,
            stencil_zfail: KEEP,
            stencil_zpass: KEEP,
            stencil_clear: 0,
            color_mask: [true; 4],
            clear_color: [0.0; 4],
            scissor: [0, 0, 0, 0],
            viewport: [0, 0, 0, 0],
            depth_range: [0.0, 1.0],
            draw_buffer: BACK,
            read_buffer: BACK,
            fog_mode: EXP,
            fog_density: 1.0,
            fog_start: 0.0,
            fog_end: 1.0,
            fog_color: [0.0; 4],
            lights: std::array::from_fn(Light::new),
            light_model_ambient: [0.2, 0.2, 0.2, 1.0],
            light_model_local_viewer: false,
            light_model_two_side: false,
            separate_specular: false,
            materials: [Material::default(); 2],
            color_material_face: FRONT_AND_BACK,
            color_material_mode: AMBIENT_AND_DIFFUSE,
            clip_planes: [[0.0; 4]; MAX_CLIP_PLANES_COUNT],
            units: [TextureUnit::default(); TEXTURE_UNITS],
            active_unit: 0,
            hints: [DONT_CARE; 5],
            matrix_mode: MODELVIEW,
            list_base: 0,
            transfer: PixelTransfer::default(),
        }
    }
}

/// Current vertex attributes (OpenGL 1.2.1, section 2.7).
#[derive(Debug, Clone, Copy)]
pub struct Current {
    pub color: [f32; 4],
    pub normal: [f32; 3],
    pub tex: [[f32; 4]; TEXTURE_UNITS],
    pub edge_flag: bool,
}

impl Default for Current {
    fn default() -> Self {
        Self {
            color: [1.0; 4],
            normal: [0.0, 0.0, 1.0],
            tex: [[0.0, 0.0, 0.0, 1.0]; TEXTURE_UNITS],
            edge_flag: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientState {
    pub vertex: ClientArray,
    pub normal: ClientArray,
    pub color: ClientArray,
    pub tex: [ClientArray; TEXTURE_UNITS],
    pub active_unit: usize,
    pub unpack: PixelStore,
    pub pack: PixelStore,
    pub lock: Option<(i32, i32)>,
}

impl Default for ClientState {
    fn default() -> Self {
        let array = |size| ClientArray {
            enabled: false,
            size,
            kind: FLOAT,
            stride: 0,
            pointer: 0,
        };
        Self {
            vertex: array(4),
            normal: array(3),
            color: array(4),
            tex: [array(4); TEXTURE_UNITS],
            active_unit: 0,
            unpack: PixelStore::default(),
            pack: PixelStore::default(),
            lock: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextState {
    pub raster: Raster,
    pub current: Current,
    pub client: ClientState,
    pub modelview: Vec<Mat4>,
    pub projection: Vec<Mat4>,
    pub texture_matrix: [Vec<Mat4>; TEXTURE_UNITS],
    pub textures: HashMap<u32, Texture>,
    /// Name 0 is each unit's default texture object.
    pub default_texture: Texture,
    pub next_texture_name: u32,
    pub attrib_stack: Vec<Raster>,
    pub error: u32,
    pub viewport_initialised: bool,
}

impl Default for ContextState {
    fn default() -> Self {
        Self {
            raster: Raster::default(),
            current: Current::default(),
            client: ClientState::default(),
            modelview: vec![Mat4::IDENTITY],
            projection: vec![Mat4::IDENTITY],
            texture_matrix: std::array::from_fn(|_| vec![Mat4::IDENTITY]),
            textures: HashMap::new(),
            default_texture: Texture::default(),
            next_texture_name: 1,
            attrib_stack: Vec::new(),
            error: NO_ERROR,
            viewport_initialised: false,
        }
    }
}

impl ContextState {
    pub fn set_error(&mut self, error: u32) {
        if self.error == NO_ERROR {
            self.error = error;
        }
    }

    pub fn take_error(&mut self) -> u32 {
        std::mem::replace(&mut self.error, NO_ERROR)
    }

    pub fn modelview_top(&self) -> &Mat4 {
        self.modelview
            .last()
            .expect("modelview stack is never empty")
    }

    pub fn projection_top(&self) -> &Mat4 {
        self.projection
            .last()
            .expect("projection stack is never empty")
    }

    pub fn texture_top(&self, unit: usize) -> &Mat4 {
        self.texture_matrix[unit]
            .last()
            .expect("texture stack is never empty")
    }

    pub fn bound_texture(&self, unit: usize) -> &Texture {
        let name = self.raster.units[unit].binding_2d;
        if name == 0 {
            &self.default_texture
        } else {
            self.textures.get(&name).unwrap_or(&self.default_texture)
        }
    }

    pub fn bound_texture_mut(&mut self, unit: usize) -> &mut Texture {
        let name = self.raster.units[unit].binding_2d;
        if name == 0 {
            &mut self.default_texture
        } else {
            self.textures.entry(name).or_default()
        }
    }

    /// The matrix stack selected by glMatrixMode.
    pub fn current_stack_mut(&mut self) -> &mut Vec<Mat4> {
        match self.raster.matrix_mode {
            PROJECTION => &mut self.projection,
            TEXTURE => {
                let unit = self.raster.active_unit;
                &mut self.texture_matrix[unit]
            }
            _ => &mut self.modelview,
        }
    }

    pub fn current_stack_limit(&self) -> usize {
        match self.raster.matrix_mode {
            PROJECTION => MAX_PROJECTION_STACK,
            TEXTURE => MAX_TEXTURE_STACK,
            _ => MAX_STACK_DEPTH,
        }
    }
}
