//! Classic Mac OS OpenGL 1.2: a fixed-function renderer for PowerPC
//! applications that link Apple's `OpenGLLibrary`.
//!
//! Entry points are decoded from the PowerPC calling convention into
//! [`GlCall`] values. Calls are either executed against the AGL context's
//! framebuffer or recorded into display lists. Rendering follows the
//! OpenGL 1.2.1 specification: per-vertex transform and lighting, clipping,
//! rasterization with perspective-correct interpolation, texturing with
//! mipmaps and the ARB multitexture, env_add and env_combine extensions,
//! fog, and the per-fragment scissor, alpha, stencil, depth and blend tests.
//! <https://registry.khronos.org/OpenGL/specs/gl/glspec121.pdf>

pub mod args;
pub mod consts;
pub mod functions;
pub mod glu;
pub mod math;
mod pipeline;
mod raster;
pub mod state;
pub mod texture;

use self::args::PpcCallArgs;
use self::consts::*;
use self::functions::{AttribType, GlFunction, GlNamed};
use self::math::Mat4;
use self::pipeline::{ClipVertex, Vertex, VertexTransform};
use self::state::{
    ClientArray, ContextState, MAX_CLIP_PLANES_COUNT, MAX_LIGHTS_COUNT, MAX_PROJECTION_STACK,
    MAX_STACK_DEPTH, MAX_TEXTURE_STACK, TEXTURE_UNITS,
};
use self::texture::{BaseFormat, PixelLayout, MAX_LEVELS, MAX_TEXTURE_DIMENSION};
use super::classic_gl_framebuffer::{ClassicGlColorBuffer, ClassicGlFramebuffer};
use super::PpcSectionMem;
use ppc::{PpcCpu, PpcMemory};
use std::collections::HashMap;
use std::rc::Rc;

pub use self::raster::RasterTarget;

/// Strings returned by glGetString live in the import-data page, after the
/// StdCLib `_iob` records.
pub const GL_STRING_AREA: u32 = super::PPC_IMPORT_DATA_BASE + 0xB00;
const GL_STRING_AREA_SIZE: u32 = 0x500;

pub const GL_VENDOR: &str = "Systemless";
pub const GL_RENDERER: &str = "Systemless OpenGL Renderer";
pub const GL_VERSION: &str = "1.2 Systemless";
pub const GL_EXTENSIONS: &str = "GL_ARB_multitexture GL_ARB_texture_env_add \
GL_ARB_texture_env_combine GL_EXT_texture_env_add GL_EXT_compiled_vertex_array \
GL_EXT_blend_color GL_EXT_bgra GL_APPLE_packed_pixels GL_EXT_packed_pixels \
GL_SGIS_texture_edge_clamp GL_EXT_rescale_normal GL_EXT_separate_specular_color \
GL_ARB_point_parameters GL_EXT_point_parameters";

const MAX_LIST_NESTING_DEPTH: u32 = 64;
const MAX_ARRAY_VERTICES: usize = 1 << 20;

/// A decoded OpenGL command that may be executed or compiled into a list.
#[derive(Debug, Clone)]
pub enum GlCall {
    Begin(u32),
    End,
    Vertex([f32; 4]),
    Color([f32; 4]),
    Normal([f32; 3]),
    TexCoord(usize, [f32; 4]),
    EdgeFlag(bool),
    Rect([f32; 4]),
    ArrayVertex(Box<Vertex>, [bool; 3]),
    DrawVertices(u32, Rc<Vec<Vertex>>),
    Enable(u32, bool),
    MatrixMode(u32),
    LoadMatrix(Mat4),
    MultMatrix(Mat4),
    PushMatrix,
    PopMatrix,
    Viewport([i32; 4]),
    DepthRange(f32, f32),
    ClearColor([f32; 4]),
    ClearDepth(f32),
    ClearStencil(i32),
    Clear(u32),
    ColorMask([bool; 4]),
    DepthMask(bool),
    StencilMask(u32),
    DepthFunc(u32),
    AlphaFunc(u32, f32),
    BlendFunc(u32, u32),
    BlendColor([f32; 4]),
    StencilFunc(u32, i32, u32),
    StencilOp(u32, u32, u32),
    CullFace(u32),
    FrontFace(u32),
    ShadeModel(u32),
    PolygonMode(u32, u32),
    PolygonOffset(f32, f32),
    LineWidth(f32),
    LineStipple(i32, u16),
    PointSize(f32),
    PointParameter(u32, [f32; 3]),
    Scissor([i32; 4]),
    Hint(u32, u32),
    DrawBuffer(u32),
    ReadBuffer(u32),
    Fog(u32, [f32; 4]),
    Light(u32, u32, [f32; 4]),
    LightModel(u32, [f32; 4]),
    Material(u32, u32, [f32; 4]),
    ColorMaterial(u32, u32),
    TexParameter(u32, u32, [f32; 4]),
    TexEnv(u32, u32, [f32; 4]),
    TexGen(u32, u32, [f32; 4]),
    BindTexture(u32, u32),
    TexImage2D {
        level: usize,
        internal: u32,
        width: u32,
        height: u32,
        pixels: Option<Rc<Vec<[f32; 4]>>>,
    },
    TexSubImage2D {
        level: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        pixels: Rc<Vec<[f32; 4]>>,
    },
    CopyTexImage2D {
        level: usize,
        internal: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    CopyTexSubImage2D {
        level: usize,
        xoffset: i32,
        yoffset: i32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    ActiveTexture(usize),
    ClipPlane(u32, [f32; 4]),
    CallList(u32),
    CallLists(Vec<u32>),
    ListBase(u32),
    PushAttrib,
    PopAttrib,
    PixelTransfer(u32, f32),
    Error(u32),
}

/// The GL half of an AGL context: server state, display lists and the
/// primitive currently being specified.
#[derive(Debug, Clone)]
pub struct GlContext {
    pub state: ContextState,
    lists: HashMap<u32, Rc<Vec<GlCall>>>,
    compiling: Option<(u32, bool, Vec<GlCall>)>,
    begin: Option<(u32, Vec<ClipVertex>)>,
    transform: Option<Rc<VertexTransform>>,
    list_depth: u32,
    double_buffered: bool,
}

impl std::fmt::Debug for VertexTransform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VertexTransform")
    }
}

/// Result of an OpenGL entry point.
pub enum GlResult {
    Void,
    Value(u32),
}

impl GlContext {
    pub fn new(double_buffered: bool) -> Self {
        let mut state = ContextState::default();
        let buffer = if double_buffered { BACK } else { FRONT };
        state.raster.draw_buffer = buffer;
        state.raster.read_buffer = buffer;
        Self {
            state,
            lists: HashMap::new(),
            compiling: None,
            begin: None,
            transform: None,
            list_depth: 0,
            double_buffered,
        }
    }

    /// AGL initialises the viewport and scissor box to the drawable's size the
    /// first time a context is attached (OpenGL 1.2.1, section 2.10.1).
    pub fn attach_drawable(&mut self, width: u32, height: u32) {
        if !self.state.viewport_initialised {
            self.state.viewport_initialised = true;
            let rect = [0, 0, width as i32, height as i32];
            self.state.raster.viewport = rect;
            self.state.raster.scissor = rect;
        }
    }

    fn color_buffer(&self, selector: u32) -> ClassicGlColorBuffer {
        match selector {
            FRONT | FRONT_LEFT | FRONT_RIGHT | LEFT | RIGHT => ClassicGlColorBuffer::Front,
            _ if self.double_buffered => ClassicGlColorBuffer::Back,
            _ => ClassicGlColorBuffer::Front,
        }
    }

    pub fn inside_begin(&self) -> bool {
        self.begin.is_some()
    }

    fn submit(&mut self, call: GlCall, framebuffer: &mut Option<&mut ClassicGlFramebuffer>) {
        if let Some((_, execute, list)) = self.compiling.as_mut() {
            let execute = *execute;
            list.push(call.clone());
            if !execute {
                return;
            }
        }
        self.execute(call, framebuffer);
    }

    fn current_vertex(&self, position: [f32; 4]) -> Vertex {
        let current = &self.state.current;
        Vertex {
            position,
            color: current.color,
            normal: current.normal,
            tex: current.tex,
            edge: current.edge_flag,
        }
    }

    fn emit_vertex(&mut self, vertex: Vertex) {
        let Some(transform) = self.transform.clone() else {
            return;
        };
        if let Some((_, vertices)) = self.begin.as_mut() {
            vertices.push(pipeline::process_vertex(&self.state, &transform, &vertex));
        }
    }

    fn with_target(
        &self,
        framebuffer: &mut Option<&mut ClassicGlFramebuffer>,
        draw: impl FnOnce(&ContextState, &mut RasterTarget),
    ) {
        let Some(framebuffer) = framebuffer.as_deref_mut() else {
            return;
        };
        let buffer = self.color_buffer(self.state.raster.draw_buffer);
        if self.state.raster.draw_buffer == NONE {
            return;
        }
        if let Some(mut target) = framebuffer.raster_target(buffer) {
            draw(&self.state, &mut target);
        }
    }

    fn execute(&mut self, call: GlCall, framebuffer: &mut Option<&mut ClassicGlFramebuffer>) {
        if self.begin.is_some()
            && !matches!(
                call,
                GlCall::Vertex(_)
                    | GlCall::Color(_)
                    | GlCall::Normal(_)
                    | GlCall::TexCoord(..)
                    | GlCall::EdgeFlag(_)
                    | GlCall::ArrayVertex(..)
                    | GlCall::Material(..)
                    | GlCall::CallList(_)
                    | GlCall::CallLists(_)
                    | GlCall::End
            )
        {
            self.state.set_error(INVALID_OPERATION);
            return;
        }
        let state = &mut self.state;
        let raster = &mut state.raster;
        match call {
            GlCall::Begin(mode) => {
                if mode > POLYGON {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                self.transform = Some(Rc::new(VertexTransform::new(state)));
                self.begin = Some((mode, Vec::new()));
            }
            GlCall::End => {
                let Some((mode, vertices)) = self.begin.take() else {
                    state.set_error(INVALID_OPERATION);
                    return;
                };
                self.with_target(framebuffer, |state, target| {
                    pipeline::draw_primitives(state, target, mode, &vertices);
                });
            }
            GlCall::Vertex(position) => {
                let vertex = self.current_vertex(position);
                self.emit_vertex(vertex);
            }
            GlCall::Color(color) => state.current.color = color,
            GlCall::Normal(normal) => state.current.normal = normal,
            GlCall::TexCoord(unit, coords) => {
                if unit < TEXTURE_UNITS {
                    state.current.tex[unit] = coords;
                }
            }
            GlCall::EdgeFlag(flag) => state.current.edge_flag = flag,
            GlCall::ArrayVertex(vertex, present) => {
                if present[0] {
                    state.current.color = vertex.color;
                }
                if present[1] {
                    state.current.normal = vertex.normal;
                }
                if present[2] {
                    state.current.tex = vertex.tex;
                }
                if self.begin.is_some() {
                    self.emit_vertex(*vertex);
                }
            }
            GlCall::Rect([x1, y1, x2, y2]) => {
                let transform = Rc::new(VertexTransform::new(state));
                let corners = [[x1, y1], [x2, y1], [x2, y2], [x1, y2]];
                let vertices: Vec<ClipVertex> = corners
                    .iter()
                    .map(|&[x, y]| {
                        let vertex = self.current_vertex([x, y, 0.0, 1.0]);
                        pipeline::process_vertex(&self.state, &transform, &vertex)
                    })
                    .collect();
                self.with_target(framebuffer, |state, target| {
                    pipeline::draw_primitives(state, target, POLYGON, &vertices);
                });
            }
            GlCall::DrawVertices(mode, vertices) => {
                let transform = VertexTransform::new(state);
                let processed: Vec<ClipVertex> = vertices
                    .iter()
                    .map(|vertex| pipeline::process_vertex(&self.state, &transform, vertex))
                    .collect();
                self.with_target(framebuffer, |state, target| {
                    pipeline::draw_primitives(state, target, mode, &processed);
                });
            }
            GlCall::Enable(cap, enabled) => {
                if !set_capability(state, cap, enabled) {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::MatrixMode(mode) => {
                if matches!(mode, MODELVIEW | PROJECTION | TEXTURE) {
                    raster.matrix_mode = mode;
                } else {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::LoadMatrix(matrix) => {
                *state.current_stack_mut().last_mut().expect("stack") = matrix;
            }
            GlCall::MultMatrix(matrix) => {
                let top = state.current_stack_mut().last_mut().expect("stack");
                *top = top.mul(&matrix);
            }
            GlCall::PushMatrix => {
                let limit = state.current_stack_limit();
                let stack = state.current_stack_mut();
                if stack.len() >= limit {
                    state.set_error(STACK_OVERFLOW);
                } else {
                    let top = *stack.last().expect("stack");
                    stack.push(top);
                }
            }
            GlCall::PopMatrix => {
                let stack = state.current_stack_mut();
                if stack.len() <= 1 {
                    state.set_error(STACK_UNDERFLOW);
                } else {
                    stack.pop();
                }
            }
            GlCall::Viewport([x, y, w, h]) => {
                if w < 0 || h < 0 {
                    state.set_error(INVALID_VALUE);
                } else {
                    raster.viewport = [x, y, w.min(4096), h.min(4096)];
                    state.viewport_initialised = true;
                }
            }
            GlCall::DepthRange(near, far) => {
                raster.depth_range = [near.clamp(0.0, 1.0), far.clamp(0.0, 1.0)];
            }
            GlCall::ClearColor(color) => raster.clear_color = color.map(|c| c.clamp(0.0, 1.0)),
            GlCall::ClearDepth(depth) => raster.depth_clear = depth.clamp(0.0, 1.0),
            GlCall::ClearStencil(value) => raster.stencil_clear = value,
            GlCall::Clear(mask) => {
                if mask
                    & !(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT | STENCIL_BUFFER_BIT | ACCUM_BUFFER_BIT)
                    != 0
                {
                    state.set_error(INVALID_VALUE);
                    return;
                }
                let Some(framebuffer) = framebuffer.as_deref_mut() else {
                    return;
                };
                let raster = &self.state.raster;
                let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let buffer = self.color_buffer(raster.draw_buffer);
                let clear = super::classic_gl_framebuffer::ClassicGlClear {
                    color: (mask & COLOR_BUFFER_BIT != 0 && raster.draw_buffer != NONE)
                        .then(|| (buffer, raster.clear_color.map(to_u8))),
                    color_mask: raster.color_mask,
                    depth: (mask & DEPTH_BUFFER_BIT != 0).then_some(raster.depth_clear),
                    depth_mask: raster.depth_mask,
                    stencil: (mask & STENCIL_BUFFER_BIT != 0).then_some(raster.stencil_clear as u8),
                    stencil_mask: raster.stencil_write_mask as u8,
                    scissor: raster.enables.scissor_test.then(|| {
                        let [x, y, w, h] = raster.scissor;
                        (x, y, w.max(0) as u32, h.max(0) as u32)
                    }),
                };
                framebuffer.clear(clear);
            }
            GlCall::ColorMask(mask) => raster.color_mask = mask,
            GlCall::DepthMask(mask) => raster.depth_mask = mask,
            GlCall::StencilMask(mask) => raster.stencil_write_mask = mask,
            GlCall::DepthFunc(func) => {
                if (NEVER..=ALWAYS).contains(&func) {
                    raster.depth_func = func;
                } else {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::AlphaFunc(func, reference) => {
                if (NEVER..=ALWAYS).contains(&func) {
                    raster.alpha_func = func;
                    raster.alpha_ref = reference.clamp(0.0, 1.0);
                } else {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::BlendFunc(src, dst) => {
                raster.blend_src = src;
                raster.blend_dst = dst;
            }
            GlCall::BlendColor(color) => raster.blend_color = color.map(|c| c.clamp(0.0, 1.0)),
            GlCall::StencilFunc(func, reference, mask) => {
                raster.stencil_func = func;
                raster.stencil_ref = reference;
                raster.stencil_value_mask = mask;
            }
            GlCall::StencilOp(fail, zfail, zpass) => {
                raster.stencil_fail = fail;
                raster.stencil_zfail = zfail;
                raster.stencil_zpass = zpass;
            }
            GlCall::CullFace(mode) => raster.cull_face_mode = mode,
            GlCall::FrontFace(mode) => raster.front_face = mode,
            GlCall::ShadeModel(mode) => {
                if matches!(mode, FLAT | SMOOTH) {
                    raster.shade_model = mode;
                } else {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::PolygonMode(face, mode) => {
                if !matches!(mode, POINT | LINE | FILL) {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                match face {
                    FRONT => raster.polygon_mode[0] = mode,
                    BACK => raster.polygon_mode[1] = mode,
                    FRONT_AND_BACK => raster.polygon_mode = [mode, mode],
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::PolygonOffset(factor, units) => raster.polygon_offset = [factor, units],
            GlCall::LineWidth(width) => {
                if width <= 0.0 {
                    state.set_error(INVALID_VALUE);
                } else {
                    raster.line_width = width;
                }
            }
            GlCall::LineStipple(factor, pattern) => {
                raster.line_stipple = (factor.clamp(1, 256), pattern);
            }
            GlCall::PointSize(size) => {
                if size <= 0.0 {
                    state.set_error(INVALID_VALUE);
                } else {
                    raster.point_size = size;
                }
            }
            GlCall::PointParameter(pname, values) => match pname {
                POINT_SIZE_MIN => raster.point_size_min = values[0].max(0.0),
                POINT_SIZE_MAX => raster.point_size_max = values[0].max(0.0),
                POINT_FADE_THRESHOLD_SIZE => {}
                POINT_DISTANCE_ATTENUATION => raster.point_attenuation = values,
                _ => state.set_error(INVALID_ENUM),
            },
            GlCall::Scissor([x, y, w, h]) => {
                if w < 0 || h < 0 {
                    state.set_error(INVALID_VALUE);
                } else {
                    raster.scissor = [x, y, w, h];
                }
            }
            GlCall::Hint(target, mode) => {
                let index = match target {
                    PERSPECTIVE_CORRECTION_HINT => 0,
                    POINT_SMOOTH_HINT => 1,
                    LINE_SMOOTH_HINT => 2,
                    POLYGON_SMOOTH_HINT => 3,
                    FOG_HINT => 4,
                    _ => {
                        state.set_error(INVALID_ENUM);
                        return;
                    }
                };
                raster.hints[index] = mode;
            }
            GlCall::DrawBuffer(buffer) => raster.draw_buffer = buffer,
            GlCall::ReadBuffer(buffer) => raster.read_buffer = buffer,
            GlCall::Fog(pname, values) => match pname {
                FOG_MODE => raster.fog_mode = values[0] as u32,
                FOG_DENSITY => raster.fog_density = values[0],
                FOG_START => raster.fog_start = values[0],
                FOG_END => raster.fog_end = values[0],
                FOG_COLOR => raster.fog_color = values.map(|c| c.clamp(0.0, 1.0)),
                FOG_INDEX | FOG_HINT => {}
                _ => state.set_error(INVALID_ENUM),
            },
            GlCall::Light(light, pname, values) => {
                let index = light.wrapping_sub(LIGHT0) as usize;
                if index >= MAX_LIGHTS_COUNT {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                let modelview = *state.modelview_top();
                let light = &mut state.raster.lights[index];
                match pname {
                    AMBIENT => light.ambient = values,
                    DIFFUSE => light.diffuse = values,
                    SPECULAR => light.specular = values,
                    POSITION => light.position = modelview.transform(values),
                    SPOT_DIRECTION => {
                        light.spot_direction =
                            modelview.transform_direction([values[0], values[1], values[2]]);
                    }
                    SPOT_EXPONENT => light.spot_exponent = values[0],
                    SPOT_CUTOFF => light.spot_cutoff = values[0],
                    CONSTANT_ATTENUATION => light.attenuation[0] = values[0],
                    LINEAR_ATTENUATION => light.attenuation[1] = values[0],
                    QUADRATIC_ATTENUATION => light.attenuation[2] = values[0],
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::LightModel(pname, values) => match pname {
                LIGHT_MODEL_AMBIENT => raster.light_model_ambient = values,
                LIGHT_MODEL_LOCAL_VIEWER => raster.light_model_local_viewer = values[0] != 0.0,
                LIGHT_MODEL_TWO_SIDE => raster.light_model_two_side = values[0] != 0.0,
                LIGHT_MODEL_COLOR_CONTROL => {
                    raster.separate_specular = values[0] as u32 == SEPARATE_SPECULAR_COLOR;
                }
                _ => state.set_error(INVALID_ENUM),
            },
            GlCall::Material(face, pname, values) => {
                let faces: &[usize] = match face {
                    FRONT => &[0],
                    BACK => &[1],
                    FRONT_AND_BACK => &[0, 1],
                    _ => {
                        state.set_error(INVALID_ENUM);
                        return;
                    }
                };
                for &index in faces {
                    let material = &mut raster.materials[index];
                    match pname {
                        AMBIENT => material.ambient = values,
                        DIFFUSE => material.diffuse = values,
                        SPECULAR => material.specular = values,
                        EMISSION => material.emission = values,
                        SHININESS => material.shininess = values[0].clamp(0.0, 128.0),
                        AMBIENT_AND_DIFFUSE => {
                            material.ambient = values;
                            material.diffuse = values;
                        }
                        _ => {}
                    }
                }
            }
            GlCall::ColorMaterial(face, mode) => {
                raster.color_material_face = face;
                raster.color_material_mode = mode;
            }
            GlCall::TexParameter(target, pname, values) => {
                if target != TEXTURE_2D && target != TEXTURE_1D {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                let unit = raster.active_unit;
                let texture = state.bound_texture_mut(unit);
                let value = values[0] as u32;
                match pname {
                    TEXTURE_MIN_FILTER => texture.min_filter = value,
                    TEXTURE_MAG_FILTER => texture.mag_filter = value,
                    TEXTURE_WRAP_S => texture.wrap_s = value,
                    TEXTURE_WRAP_T => texture.wrap_t = value,
                    TEXTURE_WRAP_R | TEXTURE_MAX_ANISOTROPY_EXT => {}
                    TEXTURE_BORDER_COLOR => texture.border_color = values,
                    TEXTURE_PRIORITY => texture.priority = values[0].clamp(0.0, 1.0),
                    TEXTURE_MIN_LOD => texture.min_lod = values[0],
                    TEXTURE_MAX_LOD => texture.max_lod = values[0],
                    TEXTURE_BASE_LEVEL => texture.base_level = value,
                    TEXTURE_MAX_LEVEL => texture.max_level = value,
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::TexEnv(target, pname, values) => {
                if target != TEXTURE_ENV {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                let unit = raster.active_unit;
                let env = &mut raster.units[unit].env;
                let value = values[0] as u32;
                match pname {
                    TEXTURE_ENV_MODE => env.mode = value,
                    TEXTURE_ENV_COLOR => env.color = values.map(|c| c.clamp(0.0, 1.0)),
                    COMBINE_RGB => env.combine_rgb = value,
                    COMBINE_ALPHA => env.combine_alpha = value,
                    SOURCE0_RGB..=SOURCE2_RGB => {
                        env.source_rgb[(pname - SOURCE0_RGB) as usize] = value
                    }
                    SOURCE0_ALPHA..=SOURCE2_ALPHA => {
                        env.source_alpha[(pname - SOURCE0_ALPHA) as usize] = value;
                    }
                    OPERAND0_RGB..=OPERAND2_RGB => {
                        env.operand_rgb[(pname - OPERAND0_RGB) as usize] = value;
                    }
                    OPERAND0_ALPHA..=OPERAND2_ALPHA => {
                        env.operand_alpha[(pname - OPERAND0_ALPHA) as usize] = value;
                    }
                    RGB_SCALE => env.rgb_scale = values[0],
                    ALPHA_SCALE => env.alpha_scale = values[0],
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::TexGen(coord, pname, values) => {
                let index = coord.wrapping_sub(S) as usize;
                if index >= 4 {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                let modelview_inverse = state.modelview_top().inverse();
                let unit = state.raster.active_unit;
                let gen = &mut state.raster.units[unit].gen[index];
                match pname {
                    TEXTURE_GEN_MODE => gen.mode = values[0] as u32,
                    OBJECT_PLANE => gen.object_plane = values,
                    EYE_PLANE => {
                        // The eye plane is transformed by the inverse
                        // modelview matrix when specified (section 2.10.4).
                        gen.eye_plane = modelview_inverse.transpose().transform(values);
                    }
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::BindTexture(target, name) => {
                if target != TEXTURE_2D && target != TEXTURE_1D {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                if name != 0 {
                    state.textures.entry(name).or_default();
                    state.next_texture_name = state.next_texture_name.max(name.saturating_add(1));
                }
                let unit = state.raster.active_unit;
                state.raster.units[unit].binding_2d = name;
            }
            GlCall::TexImage2D {
                level,
                internal,
                width,
                height,
                pixels,
            } => {
                let Some(base) = BaseFormat::from_internal(internal) else {
                    state.set_error(INVALID_VALUE);
                    return;
                };
                let unit = state.raster.active_unit;
                let texture = state.bound_texture_mut(unit);
                texture.specify_level(
                    level,
                    internal,
                    base,
                    width,
                    height,
                    pixels.map(|pixels| (*pixels).clone()),
                );
            }
            GlCall::TexSubImage2D {
                level,
                x,
                y,
                width,
                height,
                pixels,
            } => {
                let unit = state.raster.active_unit;
                let texture = state.bound_texture_mut(unit);
                if x < 0
                    || y < 0
                    || !texture.update_level(level, x as u32, y as u32, width, height, &pixels)
                {
                    state.set_error(INVALID_VALUE);
                }
            }
            GlCall::CopyTexImage2D {
                level,
                internal,
                x,
                y,
                width,
                height,
            } => {
                let Some(base) = BaseFormat::from_internal(internal) else {
                    state.set_error(INVALID_VALUE);
                    return;
                };
                let pixels = self.read_color_rect(framebuffer, x, y, width, height);
                let unit = self.state.raster.active_unit;
                let texture = self.state.bound_texture_mut(unit);
                texture.specify_level(level, internal, base, width, height, pixels);
            }
            GlCall::CopyTexSubImage2D {
                level,
                xoffset,
                yoffset,
                x,
                y,
                width,
                height,
            } => {
                let Some(pixels) = self.read_color_rect(framebuffer, x, y, width, height) else {
                    return;
                };
                let unit = self.state.raster.active_unit;
                let texture = self.state.bound_texture_mut(unit);
                if xoffset < 0
                    || yoffset < 0
                    || !texture.update_level(
                        level,
                        xoffset as u32,
                        yoffset as u32,
                        width,
                        height,
                        &pixels,
                    )
                {
                    self.state.set_error(INVALID_VALUE);
                }
            }
            GlCall::ActiveTexture(unit) => {
                if unit < TEXTURE_UNITS {
                    raster.active_unit = unit;
                } else {
                    state.set_error(INVALID_ENUM);
                }
            }
            GlCall::ClipPlane(plane, equation) => {
                let index = plane.wrapping_sub(CLIP_PLANE0) as usize;
                if index >= MAX_CLIP_PLANES_COUNT {
                    state.set_error(INVALID_ENUM);
                    return;
                }
                let inverse = state.modelview_top().inverse();
                state.raster.clip_planes[index] = inverse.transpose().transform(equation);
            }
            GlCall::CallList(list) => self.call_list(list, framebuffer),
            GlCall::CallLists(lists) => {
                let base = self.state.raster.list_base;
                for list in lists {
                    self.call_list(base.wrapping_add(list), framebuffer);
                }
            }
            GlCall::ListBase(base) => raster.list_base = base,
            GlCall::PushAttrib => {
                if state.attrib_stack.len() >= 16 {
                    state.set_error(STACK_OVERFLOW);
                } else {
                    let saved = state.raster.clone();
                    state.attrib_stack.push(saved);
                }
            }
            GlCall::PopAttrib => match state.attrib_stack.pop() {
                Some(saved) => state.raster = saved,
                None => state.set_error(STACK_UNDERFLOW),
            },
            GlCall::PixelTransfer(pname, value) => {
                let transfer = &mut raster.transfer;
                match pname {
                    RED_SCALE => transfer.scale[0] = value,
                    GREEN_SCALE => transfer.scale[1] = value,
                    BLUE_SCALE => transfer.scale[2] = value,
                    ALPHA_SCALE => transfer.scale[3] = value,
                    RED_BIAS => transfer.bias[0] = value,
                    GREEN_BIAS => transfer.bias[1] = value,
                    BLUE_BIAS => transfer.bias[2] = value,
                    ALPHA_BIAS => transfer.bias[3] = value,
                    DEPTH_SCALE => transfer.depth_scale = value,
                    DEPTH_BIAS => transfer.depth_bias = value,
                    MAP_COLOR | MAP_STENCIL | INDEX_SHIFT | INDEX_OFFSET => {}
                    _ => state.set_error(INVALID_ENUM),
                }
            }
            GlCall::Error(error) => state.set_error(error),
        }
    }

    fn call_list(&mut self, list: u32, framebuffer: &mut Option<&mut ClassicGlFramebuffer>) {
        if self.list_depth >= MAX_LIST_NESTING_DEPTH {
            return;
        }
        let Some(calls) = self.lists.get(&list).cloned() else {
            return;
        };
        self.list_depth += 1;
        for call in calls.iter() {
            self.execute(call.clone(), framebuffer);
        }
        self.list_depth -= 1;
    }

    fn read_color_rect(
        &self,
        framebuffer: &mut Option<&mut ClassicGlFramebuffer>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Option<Vec<[f32; 4]>> {
        let framebuffer = framebuffer.as_deref()?;
        let buffer = self.color_buffer(self.state.raster.read_buffer);
        let mut out = Vec::with_capacity(width as usize * height as usize);
        for row in 0..height as i32 {
            for column in 0..width as i32 {
                let (px, py) = (x + column, y + row);
                let pixel = if px >= 0 && py >= 0 {
                    framebuffer.pixel(buffer, px as u32, py as u32)
                } else {
                    None
                }
                .unwrap_or([0, 0, 0, 0]);
                out.push(pixel.map(|c| f32::from(c) / 255.0));
            }
        }
        Some(out)
    }
}

fn set_capability(state: &mut ContextState, cap: u32, enabled: bool) -> bool {
    let unit = state.raster.active_unit;
    let e = &mut state.raster.enables;
    match cap {
        ALPHA_TEST => e.alpha_test = enabled,
        BLEND => e.blend = enabled,
        COLOR_MATERIAL => e.color_material = enabled,
        CULL_FACE => e.cull_face = enabled,
        DEPTH_TEST => e.depth_test = enabled,
        DITHER => e.dither = enabled,
        FOG => e.fog = enabled,
        LIGHTING => e.lighting = enabled,
        LINE_SMOOTH => e.line_smooth = enabled,
        LINE_STIPPLE => e.line_stipple = enabled,
        NORMALIZE => e.normalize = enabled,
        POINT_SMOOTH => e.point_smooth = enabled,
        POLYGON_OFFSET_FILL => e.polygon_offset_fill = enabled,
        POLYGON_OFFSET_LINE => e.polygon_offset_line = enabled,
        POLYGON_OFFSET_POINT => e.polygon_offset_point = enabled,
        POLYGON_SMOOTH => e.polygon_smooth = enabled,
        POLYGON_STIPPLE => e.polygon_stipple = enabled,
        RESCALE_NORMAL => e.rescale_normal = enabled,
        SCISSOR_TEST => e.scissor_test = enabled,
        STENCIL_TEST => e.stencil_test = enabled,
        COLOR_LOGIC_OP | INDEX_LOGIC_OP => e.color_logic_op = enabled,
        AUTO_NORMAL => e.auto_normal = enabled,
        TEXTURE_2D => state.raster.units[unit].enabled_2d = enabled,
        TEXTURE_1D => state.raster.units[unit].enabled_1d = enabled,
        TEXTURE_GEN_S..=TEXTURE_GEN_Q => {
            state.raster.units[unit].gen[(cap - TEXTURE_GEN_S) as usize].enabled = enabled;
        }
        _ if (CLIP_PLANE0..CLIP_PLANE0 + MAX_CLIP_PLANES_COUNT as u32).contains(&cap) => {
            e.clip_planes[(cap - CLIP_PLANE0) as usize] = enabled;
        }
        _ if (LIGHT0..LIGHT0 + MAX_LIGHTS_COUNT as u32).contains(&cap) => {
            state.raster.lights[(cap - LIGHT0) as usize].enabled = enabled;
        }
        _ => return false,
    }
    true
}

fn is_enabled(state: &ContextState, cap: u32) -> Option<bool> {
    let unit = state.raster.active_unit;
    let e = &state.raster.enables;
    let client = &state.client;
    Some(match cap {
        ALPHA_TEST => e.alpha_test,
        BLEND => e.blend,
        COLOR_MATERIAL => e.color_material,
        CULL_FACE => e.cull_face,
        DEPTH_TEST => e.depth_test,
        DITHER => e.dither,
        FOG => e.fog,
        LIGHTING => e.lighting,
        LINE_SMOOTH => e.line_smooth,
        LINE_STIPPLE => e.line_stipple,
        NORMALIZE => e.normalize,
        POINT_SMOOTH => e.point_smooth,
        POLYGON_OFFSET_FILL => e.polygon_offset_fill,
        POLYGON_OFFSET_LINE => e.polygon_offset_line,
        POLYGON_OFFSET_POINT => e.polygon_offset_point,
        POLYGON_SMOOTH => e.polygon_smooth,
        POLYGON_STIPPLE => e.polygon_stipple,
        RESCALE_NORMAL => e.rescale_normal,
        SCISSOR_TEST => e.scissor_test,
        STENCIL_TEST => e.stencil_test,
        COLOR_LOGIC_OP | INDEX_LOGIC_OP => e.color_logic_op,
        AUTO_NORMAL => e.auto_normal,
        TEXTURE_2D => state.raster.units[unit].enabled_2d,
        TEXTURE_1D => state.raster.units[unit].enabled_1d,
        TEXTURE_GEN_S..=TEXTURE_GEN_Q => {
            state.raster.units[unit].gen[(cap - TEXTURE_GEN_S) as usize].enabled
        }
        VERTEX_ARRAY => client.vertex.enabled,
        NORMAL_ARRAY => client.normal.enabled,
        COLOR_ARRAY => client.color.enabled,
        TEXTURE_COORD_ARRAY => client.tex[client.active_unit].enabled,
        INDEX_ARRAY | EDGE_FLAG_ARRAY => false,
        _ if (CLIP_PLANE0..CLIP_PLANE0 + MAX_CLIP_PLANES_COUNT as u32).contains(&cap) => {
            e.clip_planes[(cap - CLIP_PLANE0) as usize]
        }
        _ if (LIGHT0..LIGHT0 + MAX_LIGHTS_COUNT as u32).contains(&cap) => {
            state.raster.lights[(cap - LIGHT0) as usize].enabled
        }
        _ => return None,
    })
}

/// Convert an attribute component read from a vector or scalar argument.
fn normalized(kind: AttribType, raw: f64) -> f32 {
    (match kind {
        AttribType::Byte => (2.0 * raw + 1.0) / 255.0,
        AttribType::UnsignedByte => raw / 255.0,
        AttribType::Short => (2.0 * raw + 1.0) / 65535.0,
        AttribType::UnsignedShort => raw / 65535.0,
        AttribType::Int => (2.0 * raw + 1.0) / 4_294_967_295.0,
        AttribType::UnsignedInt => raw / 4_294_967_295.0,
        AttribType::Float | AttribType::Double => raw,
    }) as f32
}

fn read_component(memory: &mut PpcSectionMem, address: u32, kind: AttribType) -> f64 {
    match kind {
        AttribType::Byte => f64::from(memory.read_u8(address).unwrap_or(0) as i8),
        AttribType::UnsignedByte => f64::from(memory.read_u8(address).unwrap_or(0)),
        AttribType::Short => f64::from(memory.read_u16_be(address).unwrap_or(0) as i16),
        AttribType::UnsignedShort => f64::from(memory.read_u16_be(address).unwrap_or(0)),
        AttribType::Int => f64::from(memory.read_u32_be(address).unwrap_or(0) as i32),
        AttribType::UnsignedInt => f64::from(memory.read_u32_be(address).unwrap_or(0)),
        AttribType::Float => f64::from(f32::from_bits(memory.read_u32_be(address).unwrap_or(0))),
        AttribType::Double => {
            let high = memory.read_u32_be(address).unwrap_or(0);
            let low = memory.read_u32_be(address.wrapping_add(4)).unwrap_or(0);
            f64::from_bits((u64::from(high) << 32) | u64::from(low))
        }
    }
}

/// Read `count` components of an attribute call (scalar arguments or a
/// vector pointer).
fn read_attribute(
    args: &mut PpcCallArgs,
    kind: AttribType,
    vector: bool,
    count: usize,
) -> Vec<f64> {
    if vector {
        let pointer = args.u32();
        let memory = args.memory();
        (0..count)
            .map(|index| {
                read_component(
                    memory,
                    pointer.wrapping_add(index as u32 * kind.bytes()),
                    kind,
                )
            })
            .collect()
    } else {
        (0..count)
            .map(|_| match kind {
                AttribType::Float => f64::from(args.f32()),
                AttribType::Double => args.f64(),
                AttribType::Byte => f64::from(args.u32() as i8),
                AttribType::UnsignedByte => f64::from(args.u32() as u8),
                AttribType::Short => f64::from(args.u32() as i16),
                AttribType::UnsignedShort => f64::from(args.u32() as u16),
                AttribType::Int => f64::from(args.i32()),
                AttribType::UnsignedInt => f64::from(args.u32()),
            })
            .collect()
    }
}

fn array_type(kind: u32) -> Option<AttribType> {
    Some(match kind {
        BYTE => AttribType::Byte,
        UNSIGNED_BYTE => AttribType::UnsignedByte,
        SHORT => AttribType::Short,
        UNSIGNED_SHORT => AttribType::UnsignedShort,
        INT => AttribType::Int,
        UNSIGNED_INT => AttribType::UnsignedInt,
        FLOAT => AttribType::Float,
        DOUBLE => AttribType::Double,
        _ => return None,
    })
}

fn read_array_element(
    memory: &mut PpcSectionMem,
    array: &ClientArray,
    index: u32,
    normalize: bool,
) -> Option<Vec<f32>> {
    let kind = array_type(array.kind)?;
    let stride = if array.stride == 0 {
        array.size * kind.bytes()
    } else {
        array.stride
    };
    let base = array.pointer.wrapping_add(index.wrapping_mul(stride));
    Some(
        (0..array.size)
            .map(|component| {
                let raw = read_component(memory, base.wrapping_add(component * kind.bytes()), kind);
                if normalize {
                    normalized(kind, raw)
                } else {
                    raw as f32
                }
            })
            .collect(),
    )
}

impl GlContext {
    /// Build a vertex from the enabled client arrays, returning which of
    /// color, normal and texture attributes came from arrays.
    fn array_vertex(&self, memory: &mut PpcSectionMem, index: u32) -> Option<(Vertex, [bool; 3])> {
        let client = &self.state.client;
        if !client.vertex.enabled {
            return None;
        }
        let mut vertex = self.current_vertex([0.0, 0.0, 0.0, 1.0]);
        let position = read_array_element(memory, &client.vertex, index, false)?;
        for (slot, value) in vertex.position.iter_mut().zip(position) {
            *slot = value;
        }
        if client.vertex.size < 3 {
            vertex.position[2] = 0.0;
        }
        let mut present = [false; 3];
        if client.color.enabled {
            let color = read_array_element(memory, &client.color, index, true)?;
            vertex.color = [
                color[0],
                color[1],
                color[2],
                color.get(3).copied().unwrap_or(1.0),
            ];
            present[0] = true;
        }
        if client.normal.enabled {
            let normal = read_array_element(memory, &client.normal, index, true)?;
            vertex.normal = [normal[0], normal[1], normal[2]];
            present[1] = true;
        }
        for unit in 0..TEXTURE_UNITS {
            if client.tex[unit].enabled {
                let coords = read_array_element(memory, &client.tex[unit], index, false)?;
                let mut tex = [0.0, 0.0, 0.0, 1.0];
                for (slot, value) in tex.iter_mut().zip(coords) {
                    *slot = value;
                }
                vertex.tex[unit] = tex;
                present[2] = true;
            }
        }
        Some((vertex, present))
    }

    fn array_vertices(
        &self,
        memory: &mut PpcSectionMem,
        indices: impl Iterator<Item = u32>,
    ) -> Option<Vec<Vertex>> {
        let mut vertices = Vec::new();
        for index in indices {
            if vertices.len() >= MAX_ARRAY_VERTICES {
                break;
            }
            vertices.push(self.array_vertex(memory, index)?.0);
        }
        Some(vertices)
    }
}

fn four(values: &[f64], defaults: [f32; 4]) -> [f32; 4] {
    let mut out = defaults;
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = *value as f32;
    }
    out
}

fn read_f32s(memory: &mut PpcSectionMem, pointer: u32, count: usize) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (index, slot) in out.iter_mut().enumerate().take(count) {
        *slot = f32::from_bits(
            memory
                .read_u32_be(pointer.wrapping_add(index as u32 * 4))
                .unwrap_or(0),
        );
    }
    out
}

fn read_i32s_as_f32(memory: &mut PpcSectionMem, pointer: u32, count: usize) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (index, slot) in out.iter_mut().enumerate().take(count) {
        *slot = memory
            .read_u32_be(pointer.wrapping_add(index as u32 * 4))
            .unwrap_or(0) as i32 as f32;
    }
    out
}

fn read_matrix_f32(memory: &mut PpcSectionMem, pointer: u32) -> Mat4 {
    Mat4(std::array::from_fn(|index| {
        f32::from_bits(
            memory
                .read_u32_be(pointer.wrapping_add(index as u32 * 4))
                .unwrap_or(0),
        )
    }))
}

fn read_matrix_f64(memory: &mut PpcSectionMem, pointer: u32) -> Mat4 {
    Mat4(std::array::from_fn(|index| {
        read_component(
            memory,
            pointer.wrapping_add(index as u32 * 8),
            AttribType::Double,
        ) as f32
    }))
}

/// Number of values for the vector forms of glLight/glMaterial/glFog/etc.
fn parameter_count(pname: u32) -> usize {
    match pname {
        AMBIENT | DIFFUSE | SPECULAR | POSITION | EMISSION | AMBIENT_AND_DIFFUSE
        | LIGHT_MODEL_AMBIENT | FOG_COLOR | TEXTURE_ENV_COLOR | TEXTURE_BORDER_COLOR
        | OBJECT_PLANE | EYE_PLANE => 4,
        SPOT_DIRECTION | POINT_DISTANCE_ATTENUATION => 3,
        _ => 1,
    }
}

/// Pixel data read from guest memory and unpacked with the current client
/// unpack state.
fn unpack_image(
    ctx: &GlContext,
    memory: &mut PpcSectionMem,
    width: u32,
    height: u32,
    format: u32,
    kind: u32,
    pointer: u32,
) -> Result<Option<Vec<[f32; 4]>>, u32> {
    let layout = PixelLayout::new(format, kind).ok_or(INVALID_ENUM)?;
    if pointer == 0 {
        return Ok(None);
    }
    let store = ctx.state.client.unpack;
    let bytes_needed = layout.span_bytes(width as usize, height as usize, &store);
    let bytes = super::memory::ppc_memory_read_bytes(memory, pointer, bytes_needed as u32)
        .ok_or(INVALID_VALUE)?;
    texture::unpack_rgba(
        &bytes,
        width as usize,
        height as usize,
        format,
        kind,
        &store,
        &ctx.state.raster.transfer,
    )
    .map(Some)
    .ok_or(INVALID_VALUE)
}

fn valid_texture_size(width: u32, height: u32, level: usize) -> bool {
    level < MAX_LEVELS
        && width <= MAX_TEXTURE_DIMENSION
        && height <= MAX_TEXTURE_DIMENSION
        && (width == 0 || width.is_power_of_two())
        && (height == 0 || height.is_power_of_two())
}

/// Values of a glGet query and whether they are colors (which integer
/// queries scale to the full integer range).
fn get_values(ctx: &GlContext, pname: u32) -> Option<(Vec<f64>, bool)> {
    let state = &ctx.state;
    let r = &state.raster;
    let f = |v: f32| f64::from(v);
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    let matrix = |m: &Mat4| m.0.iter().map(|&v| f64::from(v)).collect::<Vec<_>>();
    let unit = r.active_unit;
    let colors = |c: [f32; 4]| (c.iter().map(|&v| f64::from(v)).collect::<Vec<_>>(), true);
    let scalar = |v: f64| (vec![v], false);
    if let Some(enabled) = is_enabled(state, pname) {
        return Some(scalar(b(enabled)));
    }
    Some(match pname {
        CURRENT_COLOR => colors(state.current.color),
        CURRENT_NORMAL => (
            state.current.normal.iter().map(|&v| f64::from(v)).collect(),
            false,
        ),
        CURRENT_TEXTURE_COORDS => (
            state.current.tex[unit]
                .iter()
                .map(|&v| f64::from(v))
                .collect(),
            false,
        ),
        POINT_SIZE => scalar(f(r.point_size)),
        LINE_WIDTH => scalar(f(r.line_width)),
        LINE_STIPPLE_PATTERN => scalar(f64::from(r.line_stipple.1)),
        LINE_STIPPLE_REPEAT => scalar(f64::from(r.line_stipple.0)),
        POLYGON_MODE => (
            vec![f64::from(r.polygon_mode[0]), f64::from(r.polygon_mode[1])],
            false,
        ),
        CULL_FACE_MODE => scalar(f64::from(r.cull_face_mode)),
        FRONT_FACE => scalar(f64::from(r.front_face)),
        SHADE_MODEL => scalar(f64::from(r.shade_model)),
        DEPTH_RANGE => (vec![f(r.depth_range[0]), f(r.depth_range[1])], false),
        DEPTH_WRITEMASK => scalar(b(r.depth_mask)),
        DEPTH_CLEAR_VALUE => scalar(f(r.depth_clear)),
        DEPTH_FUNC => scalar(f64::from(r.depth_func)),
        STENCIL_CLEAR_VALUE => scalar(f64::from(r.stencil_clear)),
        STENCIL_FUNC => scalar(f64::from(r.stencil_func)),
        STENCIL_VALUE_MASK => scalar(f64::from(r.stencil_value_mask & 0xFF)),
        STENCIL_FAIL => scalar(f64::from(r.stencil_fail)),
        STENCIL_PASS_DEPTH_FAIL => scalar(f64::from(r.stencil_zfail)),
        STENCIL_PASS_DEPTH_PASS => scalar(f64::from(r.stencil_zpass)),
        STENCIL_REF => scalar(f64::from(r.stencil_ref)),
        STENCIL_WRITEMASK => scalar(f64::from(r.stencil_write_mask & 0xFF)),
        MATRIX_MODE => scalar(f64::from(r.matrix_mode)),
        VIEWPORT => (r.viewport.iter().map(|&v| f64::from(v)).collect(), false),
        SCISSOR_BOX => (r.scissor.iter().map(|&v| f64::from(v)).collect(), false),
        MODELVIEW_STACK_DEPTH => scalar(state.modelview.len() as f64),
        PROJECTION_STACK_DEPTH => scalar(state.projection.len() as f64),
        TEXTURE_STACK_DEPTH => scalar(state.texture_matrix[unit].len() as f64),
        MODELVIEW_MATRIX => (matrix(state.modelview_top()), false),
        PROJECTION_MATRIX => (matrix(state.projection_top()), false),
        TEXTURE_MATRIX => (matrix(state.texture_top(unit)), false),
        ALPHA_TEST_FUNC => scalar(f64::from(r.alpha_func)),
        ALPHA_TEST_REF => (vec![f(r.alpha_ref)], true),
        BLEND_SRC => scalar(f64::from(r.blend_src)),
        BLEND_DST => scalar(f64::from(r.blend_dst)),
        BLEND_COLOR => colors(r.blend_color),
        DRAW_BUFFER => scalar(f64::from(r.draw_buffer)),
        READ_BUFFER => scalar(f64::from(r.read_buffer)),
        COLOR_CLEAR_VALUE => colors(r.clear_color),
        COLOR_WRITEMASK => (r.color_mask.iter().map(|&m| b(m)).collect(), false),
        DOUBLEBUFFER => scalar(b(ctx.double_buffered)),
        STEREO => scalar(0.0),
        MAX_LIGHTS => scalar(MAX_LIGHTS_COUNT as f64),
        MAX_CLIP_PLANES => scalar(MAX_CLIP_PLANES_COUNT as f64),
        MAX_TEXTURE_SIZE => scalar(f64::from(MAX_TEXTURE_DIMENSION)),
        MAX_PIXEL_MAP_TABLE => scalar(256.0),
        MAX_ATTRIB_STACK_DEPTH => scalar(16.0),
        MAX_MODELVIEW_STACK_DEPTH => scalar(MAX_STACK_DEPTH as f64),
        MAX_NAME_STACK_DEPTH => scalar(64.0),
        MAX_PROJECTION_STACK_DEPTH => scalar(MAX_PROJECTION_STACK as f64),
        MAX_TEXTURE_STACK_DEPTH => scalar(MAX_TEXTURE_STACK as f64),
        MAX_VIEWPORT_DIMS => (vec![4096.0, 4096.0], false),
        MAX_LIST_NESTING => scalar(f64::from(MAX_LIST_NESTING_DEPTH)),
        MAX_ELEMENTS_VERTICES | MAX_ELEMENTS_INDICES => scalar(65536.0),
        MAX_TEXTURE_UNITS => scalar(TEXTURE_UNITS as f64),
        ACTIVE_TEXTURE => scalar(f64::from(TEXTURE0) + unit as f64),
        CLIENT_ACTIVE_TEXTURE => scalar(f64::from(TEXTURE0) + state.client.active_unit as f64),
        TEXTURE_BINDING_2D => scalar(f64::from(r.units[unit].binding_2d)),
        SUBPIXEL_BITS => scalar(4.0),
        RED_BITS | GREEN_BITS | BLUE_BITS | ALPHA_BITS => scalar(8.0),
        DEPTH_BITS => scalar(24.0),
        STENCIL_BITS => scalar(8.0),
        LIST_BASE => scalar(f64::from(r.list_base)),
        LIST_INDEX => scalar(f64::from(
            ctx.compiling.as_ref().map_or(0, |(name, _, _)| *name),
        )),
        LIST_MODE => scalar(f64::from(ctx.compiling.as_ref().map_or(
            0,
            |(_, execute, _)| {
                if *execute {
                    COMPILE_AND_EXECUTE
                } else {
                    COMPILE
                }
            },
        ))),
        POLYGON_OFFSET_FACTOR => scalar(f(r.polygon_offset[0])),
        POLYGON_OFFSET_UNITS => scalar(f(r.polygon_offset[1])),
        FOG_MODE => scalar(f64::from(r.fog_mode)),
        FOG_DENSITY => scalar(f(r.fog_density)),
        FOG_START => scalar(f(r.fog_start)),
        FOG_END => scalar(f(r.fog_end)),
        FOG_COLOR => colors(r.fog_color),
        LIGHT_MODEL_AMBIENT => colors(r.light_model_ambient),
        LIGHT_MODEL_LOCAL_VIEWER => scalar(b(r.light_model_local_viewer)),
        LIGHT_MODEL_TWO_SIDE => scalar(b(r.light_model_two_side)),
        UNPACK_ALIGNMENT => scalar(f64::from(state.client.unpack.alignment)),
        PACK_ALIGNMENT => scalar(f64::from(state.client.pack.alignment)),
        UNPACK_ROW_LENGTH => scalar(f64::from(state.client.unpack.row_length)),
        PACK_ROW_LENGTH => scalar(f64::from(state.client.pack.row_length)),
        PERSPECTIVE_CORRECTION_HINT => scalar(f64::from(r.hints[0])),
        FOG_HINT => scalar(f64::from(r.hints[4])),
        VERTEX_ARRAY_SIZE => scalar(f64::from(state.client.vertex.size)),
        POINT_SIZE_MIN => scalar(f(r.point_size_min)),
        POINT_SIZE_MAX => scalar(f(r.point_size_max)),
        POINT_DISTANCE_ATTENUATION => (
            r.point_attenuation.iter().map(|&v| f64::from(v)).collect(),
            false,
        ),
        ARRAY_ELEMENT_LOCK_FIRST_EXT => scalar(f64::from(state.client.lock.map_or(0, |l| l.0))),
        ARRAY_ELEMENT_LOCK_COUNT_EXT => scalar(f64::from(state.client.lock.map_or(0, |l| l.1))),
        _ => return None,
    })
}

/// Write `text` with a NUL terminator into the GL string area.
fn write_gl_string(memory: &mut PpcSectionMem, offset: u32, text: &str) -> u32 {
    let address = GL_STRING_AREA + offset;
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    if offset + bytes.len() as u32 > GL_STRING_AREA_SIZE
        || memory.write_bytes(address, &bytes).is_none()
    {
        return 0;
    }
    address
}

fn gl_string(memory: &mut PpcSectionMem, name: u32) -> u32 {
    match name {
        VENDOR => write_gl_string(memory, 0, GL_VENDOR),
        RENDERER => write_gl_string(memory, 0x20, GL_RENDERER),
        VERSION => write_gl_string(memory, 0x60, GL_VERSION),
        EXTENSIONS => write_gl_string(memory, 0x80, GL_EXTENSIONS),
        _ => 0,
    }
}

/// Pack `rgba` (bottom-up rows) into guest memory for glReadPixels.
fn pack_pixels(
    memory: &mut PpcSectionMem,
    pointer: u32,
    width: usize,
    height: usize,
    format: u32,
    kind: u32,
    store: &texture::PixelStore,
    values: &[[f32; 4]],
) -> bool {
    let Some(layout) = PixelLayout::new(format, kind) else {
        return false;
    };
    let (offset, row_bytes) = layout.row_geometry(width, store);
    let group = layout.group_bytes();
    let mut buffer = vec![0u8; layout.span_bytes(width, height, store)];
    if buffer.is_empty() {
        return true;
    }
    if let Some(existing) =
        super::memory::ppc_memory_read_bytes(memory, pointer, buffer.len() as u32)
    {
        buffer.copy_from_slice(&existing);
    }
    let pick = |rgba: [f32; 4]| -> [f32; 4] {
        match format {
            RED => [rgba[0], 0.0, 0.0, 0.0],
            GREEN => [rgba[1], 0.0, 0.0, 0.0],
            BLUE => [rgba[2], 0.0, 0.0, 0.0],
            ALPHA => [rgba[3], 0.0, 0.0, 0.0],
            LUMINANCE => [(rgba[0] + rgba[1] + rgba[2]).min(1.0), 0.0, 0.0, 0.0],
            LUMINANCE_ALPHA => [(rgba[0] + rgba[1] + rgba[2]).min(1.0), rgba[3], 0.0, 0.0],
            BGR => [rgba[2], rgba[1], rgba[0], 0.0],
            BGRA => [rgba[2], rgba[1], rgba[0], rgba[3]],
            _ => rgba,
        }
    };
    for row in 0..height {
        for column in 0..width {
            let c = pick(values[row * width + column]);
            let start = offset + row * row_bytes + column * group;
            let out = &mut buffer[start..start + group];
            let q = |v: f32, bits: u32| {
                ((v.clamp(0.0, 1.0) * ((1u32 << bits) - 1) as f32) + 0.5) as u32
            };
            if layout.packed {
                let packed = match kind {
                    UNSIGNED_SHORT_5_6_5 => (q(c[0], 5) << 11) | (q(c[1], 6) << 5) | q(c[2], 5),
                    UNSIGNED_SHORT_5_6_5_REV => (q(c[2], 5) << 11) | (q(c[1], 6) << 5) | q(c[0], 5),
                    UNSIGNED_SHORT_4_4_4_4 => {
                        (q(c[0], 4) << 12) | (q(c[1], 4) << 8) | (q(c[2], 4) << 4) | q(c[3], 4)
                    }
                    UNSIGNED_SHORT_4_4_4_4_REV => {
                        (q(c[3], 4) << 12) | (q(c[2], 4) << 8) | (q(c[1], 4) << 4) | q(c[0], 4)
                    }
                    UNSIGNED_SHORT_5_5_5_1 => {
                        (q(c[0], 5) << 11) | (q(c[1], 5) << 6) | (q(c[2], 5) << 1) | q(c[3], 1)
                    }
                    UNSIGNED_SHORT_1_5_5_5_REV => {
                        (q(c[3], 1) << 15) | (q(c[2], 5) << 10) | (q(c[1], 5) << 5) | q(c[0], 5)
                    }
                    UNSIGNED_INT_8_8_8_8 => {
                        (q(c[0], 8) << 24) | (q(c[1], 8) << 16) | (q(c[2], 8) << 8) | q(c[3], 8)
                    }
                    UNSIGNED_INT_8_8_8_8_REV => {
                        (q(c[3], 8) << 24) | (q(c[2], 8) << 16) | (q(c[1], 8) << 8) | q(c[0], 8)
                    }
                    UNSIGNED_BYTE_3_3_2 => (q(c[0], 3) << 5) | (q(c[1], 3) << 2) | q(c[2], 2),
                    _ => 0,
                };
                match group {
                    1 => out[0] = packed as u8,
                    2 => out.copy_from_slice(&(packed as u16).to_be_bytes()),
                    _ => out.copy_from_slice(&packed.to_be_bytes()),
                }
            } else {
                for component in 0..layout.components {
                    let v = c[component];
                    let element = &mut out
                        [component * layout.element_bytes..(component + 1) * layout.element_bytes];
                    match kind {
                        UNSIGNED_BYTE => element[0] = q(v, 8) as u8,
                        BYTE => element[0] = ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8,
                        UNSIGNED_SHORT => element.copy_from_slice(&(q(v, 16) as u16).to_be_bytes()),
                        SHORT => element.copy_from_slice(
                            &((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_be_bytes(),
                        ),
                        UNSIGNED_INT => element.copy_from_slice(
                            &((f64::from(v.clamp(0.0, 1.0)) * 4_294_967_295.0) as u32)
                                .to_be_bytes(),
                        ),
                        INT => element.copy_from_slice(
                            &((f64::from(v.clamp(-1.0, 1.0)) * 2_147_483_647.0) as i32)
                                .to_be_bytes(),
                        ),
                        _ => element.copy_from_slice(&v.to_bits().to_be_bytes()),
                    }
                }
            }
        }
    }
    memory.write_bytes(pointer, &buffer).is_some()
}

pub(crate) fn gl_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_GL_TRACE").is_some())
}

/// Execute one `OpenGLLibrary` entry point for the current context.
pub fn call(
    ctx: &mut GlContext,
    framebuffer: Option<&mut ClassicGlFramebuffer>,
    function: GlFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> GlResult {
    if !gl_trace_enabled() {
        return call_untraced(ctx, framebuffer, function, cpu, memory);
    }
    let error_before = ctx.state.error;
    let result = call_untraced(ctx, framebuffer, function, cpu, memory);
    if error_before == NO_ERROR && ctx.state.error != NO_ERROR {
        eprintln!(
            "[GL] {function:?} raised 0x{:04X} r3=${:08X} r4=${:08X} r5=${:08X} r6=${:08X}",
            ctx.state.error, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]
        );
    }
    result
}

fn call_untraced(
    ctx: &mut GlContext,
    mut framebuffer: Option<&mut ClassicGlFramebuffer>,
    function: GlFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> GlResult {
    let mut args = PpcCallArgs::new(cpu, memory);
    let call = match function {
        GlFunction::Vertex { size, kind, vector } => {
            let v = read_attribute(&mut args, kind, vector, size as usize);
            Some(GlCall::Vertex(four(&v, [0.0, 0.0, 0.0, 1.0])))
        }
        GlFunction::Color { size, kind, vector } => {
            let v = read_attribute(&mut args, kind, vector, size as usize);
            let mut color = [1.0; 4];
            for (slot, value) in color.iter_mut().zip(&v) {
                *slot = normalized(kind, *value);
            }
            Some(GlCall::Color(color))
        }
        GlFunction::TexCoord { size, kind, vector } => {
            let v = read_attribute(&mut args, kind, vector, size as usize);
            Some(GlCall::TexCoord(0, four(&v, [0.0, 0.0, 0.0, 1.0])))
        }
        GlFunction::MultiTexCoord { size, kind, vector } => {
            let target = args.u32();
            let v = read_attribute(&mut args, kind, vector, size as usize);
            let unit = target.wrapping_sub(TEXTURE0) as usize;
            if unit >= TEXTURE_UNITS {
                Some(GlCall::Error(INVALID_ENUM))
            } else {
                Some(GlCall::TexCoord(unit, four(&v, [0.0, 0.0, 0.0, 1.0])))
            }
        }
        GlFunction::Normal { kind, vector } => {
            let v = read_attribute(&mut args, kind, vector, 3);
            Some(GlCall::Normal([
                normalized(kind, v[0]),
                normalized(kind, v[1]),
                normalized(kind, v[2]),
            ]))
        }
        GlFunction::Rect { kind, vector } => {
            let corners = if vector {
                let first = args.u32();
                let second = args.u32();
                let memory = args.memory();
                let read = |memory: &mut PpcSectionMem, p: u32, i: u32| {
                    read_component(memory, p.wrapping_add(i * kind.bytes()), kind) as f32
                };
                [
                    read(memory, first, 0),
                    read(memory, first, 1),
                    read(memory, second, 0),
                    read(memory, second, 1),
                ]
            } else {
                let v = read_attribute(&mut args, kind, false, 4);
                [v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32]
            };
            Some(GlCall::Rect(corners))
        }
        GlFunction::Named(named) => match decode_named(ctx, &mut framebuffer, named, &mut args) {
            Decoded::Call(call) => Some(call),
            Decoded::Done(result) => return result,
        },
    };
    if let Some(call) = call {
        ctx.submit(call, &mut framebuffer);
    }
    GlResult::Void
}

enum Decoded {
    Call(GlCall),
    Done(GlResult),
}

fn done() -> Decoded {
    Decoded::Done(GlResult::Void)
}

fn value(value: u32) -> Decoded {
    Decoded::Done(GlResult::Value(value))
}

fn decode_named(
    ctx: &mut GlContext,
    framebuffer: &mut Option<&mut ClassicGlFramebuffer>,
    named: GlNamed,
    args: &mut PpcCallArgs,
) -> Decoded {
    use Decoded::Call;
    match named {
        GlNamed::Begin => Call(GlCall::Begin(args.u32())),
        GlNamed::End => Call(GlCall::End),
        GlNamed::EdgeFlag => Call(GlCall::EdgeFlag(args.u32() as u8 != 0)),
        GlNamed::EdgeFlagv => {
            let pointer = args.u32();
            let flag = args.memory().read_u8(pointer).unwrap_or(1) != 0;
            Call(GlCall::EdgeFlag(flag))
        }
        GlNamed::Enable => Call(GlCall::Enable(args.u32(), true)),
        GlNamed::Disable => Call(GlCall::Enable(args.u32(), false)),
        GlNamed::IsEnabled => {
            let cap = args.u32();
            match is_enabled(&ctx.state, cap) {
                Some(enabled) => value(u32::from(enabled)),
                None => {
                    ctx.state.set_error(INVALID_ENUM);
                    value(0)
                }
            }
        }
        GlNamed::EnableClientState | GlNamed::DisableClientState => {
            let array = args.u32();
            let enabled = named == GlNamed::EnableClientState;
            let client = &mut ctx.state.client;
            match array {
                VERTEX_ARRAY => client.vertex.enabled = enabled,
                NORMAL_ARRAY => client.normal.enabled = enabled,
                COLOR_ARRAY => client.color.enabled = enabled,
                TEXTURE_COORD_ARRAY => {
                    let unit = client.active_unit;
                    client.tex[unit].enabled = enabled;
                }
                INDEX_ARRAY | EDGE_FLAG_ARRAY => {}
                _ => ctx.state.set_error(INVALID_ENUM),
            }
            done()
        }
        GlNamed::VertexPointer | GlNamed::ColorPointer | GlNamed::TexCoordPointer => {
            let size = args.u32();
            let kind = args.u32();
            let stride = args.u32();
            let pointer = args.u32();
            let client = &mut ctx.state.client;
            let unit = client.active_unit;
            let array = match named {
                GlNamed::VertexPointer => &mut client.vertex,
                GlNamed::ColorPointer => &mut client.color,
                _ => &mut client.tex[unit],
            };
            if !(1..=4).contains(&size) || array_type(kind).is_none() {
                ctx.state.set_error(INVALID_VALUE);
            } else {
                array.size = size;
                array.kind = kind;
                array.stride = stride;
                array.pointer = pointer;
            }
            done()
        }
        GlNamed::NormalPointer => {
            let kind = args.u32();
            let stride = args.u32();
            let pointer = args.u32();
            let normal = &mut ctx.state.client.normal;
            normal.size = 3;
            normal.kind = kind;
            normal.stride = stride;
            normal.pointer = pointer;
            done()
        }
        GlNamed::ClientActiveTextureARB => {
            let unit = args.u32().wrapping_sub(TEXTURE0) as usize;
            if unit < TEXTURE_UNITS {
                ctx.state.client.active_unit = unit;
            } else {
                ctx.state.set_error(INVALID_ENUM);
            }
            done()
        }
        GlNamed::LockArraysEXT => {
            let first = args.i32();
            let count = args.i32();
            ctx.state.client.lock = Some((first, count));
            done()
        }
        GlNamed::UnlockArraysEXT => {
            ctx.state.client.lock = None;
            done()
        }
        GlNamed::ArrayElement => {
            let index = args.u32();
            let memory = args.memory();
            match ctx.array_vertex(memory, index) {
                Some((vertex, present)) => Call(GlCall::ArrayVertex(Box::new(vertex), present)),
                None => done(),
            }
        }
        GlNamed::DrawArrays => {
            let mode = args.u32();
            let first = args.i32();
            let count = args.i32();
            if first < 0 || count < 0 {
                return Call(GlCall::Error(INVALID_VALUE));
            }
            let memory = args.memory();
            match ctx.array_vertices(memory, (first as u32)..(first as u32 + count as u32)) {
                Some(vertices) => Call(GlCall::DrawVertices(mode, Rc::new(vertices))),
                None => done(),
            }
        }
        GlNamed::DrawElements | GlNamed::DrawRangeElements => {
            let mode = args.u32();
            if named == GlNamed::DrawRangeElements {
                let _start = args.u32();
                let _end = args.u32();
            }
            let count = args.i32();
            let kind = args.u32();
            let pointer = args.u32();
            if count < 0 {
                return Call(GlCall::Error(INVALID_VALUE));
            }
            let size = match kind {
                UNSIGNED_BYTE => 1,
                UNSIGNED_SHORT => 2,
                UNSIGNED_INT => 4,
                _ => return Call(GlCall::Error(INVALID_ENUM)),
            };
            let memory = args.memory();
            let bytes = super::memory::ppc_memory_read_bytes(memory, pointer, count as u32 * size)
                .unwrap_or_default();
            let indices = bytes.chunks_exact(size as usize).map(|chunk| match size {
                1 => u32::from(chunk[0]),
                2 => u32::from(u16::from_be_bytes([chunk[0], chunk[1]])),
                _ => u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            });
            let indices: Vec<u32> = indices.collect();
            match ctx.array_vertices(memory, indices.into_iter()) {
                Some(vertices) => Call(GlCall::DrawVertices(mode, Rc::new(vertices))),
                None => done(),
            }
        }
        GlNamed::MatrixMode => Call(GlCall::MatrixMode(args.u32())),
        GlNamed::LoadIdentity => Call(GlCall::LoadMatrix(Mat4::IDENTITY)),
        GlNamed::LoadMatrixf => {
            let pointer = args.u32();
            Call(GlCall::LoadMatrix(read_matrix_f32(args.memory(), pointer)))
        }
        GlNamed::LoadMatrixd => {
            let pointer = args.u32();
            Call(GlCall::LoadMatrix(read_matrix_f64(args.memory(), pointer)))
        }
        GlNamed::MultMatrixf => {
            let pointer = args.u32();
            Call(GlCall::MultMatrix(read_matrix_f32(args.memory(), pointer)))
        }
        GlNamed::MultMatrixd => {
            let pointer = args.u32();
            Call(GlCall::MultMatrix(read_matrix_f64(args.memory(), pointer)))
        }
        GlNamed::PushMatrix => Call(GlCall::PushMatrix),
        GlNamed::PopMatrix => Call(GlCall::PopMatrix),
        GlNamed::Rotatef => {
            let (a, x, y, z) = (args.f32(), args.f32(), args.f32(), args.f32());
            Call(GlCall::MultMatrix(Mat4::rotate(a, x, y, z)))
        }
        GlNamed::Rotated => {
            let (a, x, y, z) = (args.f64(), args.f64(), args.f64(), args.f64());
            Call(GlCall::MultMatrix(Mat4::rotate(
                a as f32, x as f32, y as f32, z as f32,
            )))
        }
        GlNamed::Translatef => {
            let (x, y, z) = (args.f32(), args.f32(), args.f32());
            Call(GlCall::MultMatrix(Mat4::translate(x, y, z)))
        }
        GlNamed::Translated => {
            let (x, y, z) = (args.f64(), args.f64(), args.f64());
            Call(GlCall::MultMatrix(Mat4::translate(
                x as f32, y as f32, z as f32,
            )))
        }
        GlNamed::Scalef => {
            let (x, y, z) = (args.f32(), args.f32(), args.f32());
            Call(GlCall::MultMatrix(Mat4::scale(x, y, z)))
        }
        GlNamed::Scaled => {
            let (x, y, z) = (args.f64(), args.f64(), args.f64());
            Call(GlCall::MultMatrix(Mat4::scale(
                x as f32, y as f32, z as f32,
            )))
        }
        GlNamed::Ortho | GlNamed::Frustum => {
            let v: [f64; 6] = std::array::from_fn(|_| args.f64());
            if v[0] == v[1]
                || v[2] == v[3]
                || v[4] == v[5]
                || (named == GlNamed::Frustum && (v[4] <= 0.0 || v[5] <= 0.0))
            {
                return Call(GlCall::Error(INVALID_VALUE));
            }
            Call(GlCall::MultMatrix(if named == GlNamed::Ortho {
                Mat4::ortho(v[0], v[1], v[2], v[3], v[4], v[5])
            } else {
                Mat4::frustum(v[0], v[1], v[2], v[3], v[4], v[5])
            }))
        }
        GlNamed::Viewport => Call(GlCall::Viewport([
            args.i32(),
            args.i32(),
            args.i32(),
            args.i32(),
        ])),
        GlNamed::DepthRange => {
            let near = args.f64();
            let far = args.f64();
            Call(GlCall::DepthRange(near as f32, far as f32))
        }
        GlNamed::ClearColor => Call(GlCall::ClearColor([
            args.f32(),
            args.f32(),
            args.f32(),
            args.f32(),
        ])),
        GlNamed::ClearDepth => Call(GlCall::ClearDepth(args.f64() as f32)),
        GlNamed::ClearStencil => Call(GlCall::ClearStencil(args.i32())),
        GlNamed::ClearIndex | GlNamed::ClearAccum | GlNamed::Accum => done(),
        GlNamed::Clear => Call(GlCall::Clear(args.u32())),
        GlNamed::ColorMask => {
            let mask = [args.u32(), args.u32(), args.u32(), args.u32()].map(|v| v as u8 != 0);
            Call(GlCall::ColorMask(mask))
        }
        GlNamed::DepthMask => Call(GlCall::DepthMask(args.u32() as u8 != 0)),
        GlNamed::StencilMask => Call(GlCall::StencilMask(args.u32())),
        GlNamed::DepthFunc => Call(GlCall::DepthFunc(args.u32())),
        GlNamed::AlphaFunc => {
            let func = args.u32();
            let reference = args.f32();
            Call(GlCall::AlphaFunc(func, reference))
        }
        GlNamed::BlendFunc => Call(GlCall::BlendFunc(args.u32(), args.u32())),
        GlNamed::BlendColorEXT => Call(GlCall::BlendColor([
            args.f32(),
            args.f32(),
            args.f32(),
            args.f32(),
        ])),
        GlNamed::StencilFunc => Call(GlCall::StencilFunc(args.u32(), args.i32(), args.u32())),
        GlNamed::StencilOp => Call(GlCall::StencilOp(args.u32(), args.u32(), args.u32())),
        GlNamed::CullFace => Call(GlCall::CullFace(args.u32())),
        GlNamed::FrontFace => Call(GlCall::FrontFace(args.u32())),
        GlNamed::ShadeModel => Call(GlCall::ShadeModel(args.u32())),
        GlNamed::PolygonMode => Call(GlCall::PolygonMode(args.u32(), args.u32())),
        GlNamed::PolygonOffset => Call(GlCall::PolygonOffset(args.f32(), args.f32())),
        GlNamed::LineWidth => Call(GlCall::LineWidth(args.f32())),
        GlNamed::LineStipple => Call(GlCall::LineStipple(args.i32(), args.u32() as u16)),
        GlNamed::PointSize => Call(GlCall::PointSize(args.f32())),
        GlNamed::PointParameterfARB | GlNamed::PointParameterfEXT => {
            let pname = args.u32();
            let v = args.f32();
            Call(GlCall::PointParameter(pname, [v, 0.0, 0.0]))
        }
        GlNamed::PointParameterfvARB | GlNamed::PointParameterfvEXT => {
            let pname = args.u32();
            let pointer = args.u32();
            let v = read_f32s(args.memory(), pointer, parameter_count(pname));
            Call(GlCall::PointParameter(pname, [v[0], v[1], v[2]]))
        }
        GlNamed::Scissor => Call(GlCall::Scissor([
            args.i32(),
            args.i32(),
            args.i32(),
            args.i32(),
        ])),
        GlNamed::Hint => Call(GlCall::Hint(args.u32(), args.u32())),
        GlNamed::DrawBuffer => Call(GlCall::DrawBuffer(args.u32())),
        GlNamed::ReadBuffer => Call(GlCall::ReadBuffer(args.u32())),
        GlNamed::Fogf => {
            let pname = args.u32();
            Call(GlCall::Fog(pname, [args.f32(), 0.0, 0.0, 0.0]))
        }
        GlNamed::Fogi => {
            let pname = args.u32();
            Call(GlCall::Fog(pname, [args.i32() as f32, 0.0, 0.0, 0.0]))
        }
        GlNamed::Fogfv | GlNamed::Fogiv => {
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let mut v = if named == GlNamed::Fogfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            if named == GlNamed::Fogiv && pname == FOG_COLOR {
                v = v.map(|c| (f64::from(c) / 2_147_483_647.0) as f32);
            }
            Call(GlCall::Fog(pname, v))
        }
        GlNamed::Lightf | GlNamed::Lighti => {
            let light = args.u32();
            let pname = args.u32();
            let v = if named == GlNamed::Lightf {
                args.f32()
            } else {
                args.i32() as f32
            };
            Call(GlCall::Light(light, pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::Lightfv | GlNamed::Lightiv => {
            let light = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let v = if named == GlNamed::Lightfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            Call(GlCall::Light(light, pname, v))
        }
        GlNamed::LightModelf | GlNamed::LightModeli => {
            let pname = args.u32();
            let v = if named == GlNamed::LightModelf {
                args.f32()
            } else {
                args.i32() as f32
            };
            Call(GlCall::LightModel(pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::LightModelfv | GlNamed::LightModeliv => {
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let v = if named == GlNamed::LightModelfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            Call(GlCall::LightModel(pname, v))
        }
        GlNamed::Materialf | GlNamed::Materiali => {
            let face = args.u32();
            let pname = args.u32();
            let v = if named == GlNamed::Materialf {
                args.f32()
            } else {
                args.i32() as f32
            };
            Call(GlCall::Material(face, pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::Materialfv | GlNamed::Materialiv => {
            let face = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let v = if named == GlNamed::Materialfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            Call(GlCall::Material(face, pname, v))
        }
        GlNamed::ColorMaterial => Call(GlCall::ColorMaterial(args.u32(), args.u32())),
        GlNamed::TexParameterf | GlNamed::TexParameteri => {
            let target = args.u32();
            let pname = args.u32();
            let v = if named == GlNamed::TexParameterf {
                args.f32()
            } else {
                args.i32() as f32
            };
            Call(GlCall::TexParameter(target, pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::TexParameterfv | GlNamed::TexParameteriv => {
            let target = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let v = if named == GlNamed::TexParameterfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            Call(GlCall::TexParameter(target, pname, v))
        }
        GlNamed::TexEnvf | GlNamed::TexEnvi => {
            let target = args.u32();
            let pname = args.u32();
            let v = if named == GlNamed::TexEnvf {
                args.f32()
            } else {
                args.i32() as f32
            };
            Call(GlCall::TexEnv(target, pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::TexEnvfv | GlNamed::TexEnviv => {
            let target = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let mut v = if named == GlNamed::TexEnvfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            if named == GlNamed::TexEnviv && pname == TEXTURE_ENV_COLOR {
                v = v.map(|c| (f64::from(c) / 2_147_483_647.0) as f32);
            }
            Call(GlCall::TexEnv(target, pname, v))
        }
        GlNamed::TexGenf | GlNamed::TexGeni | GlNamed::TexGend => {
            let coord = args.u32();
            let pname = args.u32();
            let v = match named {
                GlNamed::TexGenf => args.f32(),
                GlNamed::TexGend => args.f64() as f32,
                _ => args.i32() as f32,
            };
            Call(GlCall::TexGen(coord, pname, [v, 0.0, 0.0, 0.0]))
        }
        GlNamed::TexGenfv | GlNamed::TexGeniv => {
            let coord = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let count = parameter_count(pname);
            let v = if named == GlNamed::TexGenfv {
                read_f32s(args.memory(), pointer, count)
            } else {
                read_i32s_as_f32(args.memory(), pointer, count)
            };
            Call(GlCall::TexGen(coord, pname, v))
        }
        GlNamed::GenTextures => {
            let count = args.i32();
            let pointer = args.u32();
            if count < 0 {
                ctx.state.set_error(INVALID_VALUE);
                return done();
            }
            let memory = args.memory();
            for index in 0..count as u32 {
                let name = ctx.state.next_texture_name;
                ctx.state.next_texture_name += 1;
                ctx.state.textures.insert(name, Default::default());
                let _ = memory.write_u32_be(pointer.wrapping_add(index * 4), name);
            }
            done()
        }
        GlNamed::DeleteTextures => {
            let count = args.i32().max(0) as u32;
            let pointer = args.u32();
            let memory = args.memory();
            for index in 0..count {
                let name = memory
                    .read_u32_be(pointer.wrapping_add(index * 4))
                    .unwrap_or(0);
                if name == 0 {
                    continue;
                }
                ctx.state.textures.remove(&name);
                for unit in &mut ctx.state.raster.units {
                    if unit.binding_2d == name {
                        unit.binding_2d = 0;
                    }
                }
            }
            done()
        }
        GlNamed::IsTexture => {
            let name = args.u32();
            value(u32::from(
                name != 0 && ctx.state.textures.contains_key(&name),
            ))
        }
        GlNamed::AreTexturesResident => {
            let count = args.i32().max(0) as u32;
            let _textures = args.u32();
            let residences = args.u32();
            let memory = args.memory();
            for index in 0..count {
                let _ = memory.write_u8(residences.wrapping_add(index), 1);
            }
            value(1)
        }
        GlNamed::PrioritizeTextures => done(),
        GlNamed::BindTexture => Call(GlCall::BindTexture(args.u32(), args.u32())),
        GlNamed::ActiveTextureARB => {
            let unit = args.u32().wrapping_sub(TEXTURE0) as usize;
            Call(GlCall::ActiveTexture(unit))
        }
        GlNamed::TexImage1D => {
            let _ = (
                args.u32(),
                args.i32(),
                args.i32(),
                args.i32(),
                args.i32(),
                args.u32(),
                args.u32(),
                args.u32(),
            );
            done()
        }
        GlNamed::TexImage2D => {
            let target = args.u32();
            let level = args.i32();
            let internal = args.u32();
            let width = args.i32();
            let height = args.i32();
            let border = args.i32();
            let format = args.u32();
            let kind = args.u32();
            let pointer = args.u32();
            if target != TEXTURE_2D {
                return Call(GlCall::Error(INVALID_ENUM));
            }
            if level < 0
                || width < 0
                || height < 0
                || border != 0
                || !valid_texture_size(width as u32, height as u32, level as usize)
            {
                return Call(GlCall::Error(INVALID_VALUE));
            }
            if gl_trace_enabled() {
                eprintln!(
                    "[GL] glTexImage2D level={level} internal=0x{internal:04X} {width}x{height} format=0x{format:04X} type=0x{kind:04X} pixels=${pointer:08X} texture={}",
                    ctx.state.raster.units[ctx.state.raster.active_unit].binding_2d
                );
            }
            match unpack_image(
                ctx,
                args.memory(),
                width as u32,
                height as u32,
                format,
                kind,
                pointer,
            ) {
                Ok(pixels) => Call(GlCall::TexImage2D {
                    level: level as usize,
                    internal,
                    width: width as u32,
                    height: height as u32,
                    pixels: pixels.map(Rc::new),
                }),
                Err(error) => Call(GlCall::Error(error)),
            }
        }
        GlNamed::TexSubImage2D => {
            let target = args.u32();
            let level = args.i32();
            let x = args.i32();
            let y = args.i32();
            let width = args.i32();
            let height = args.i32();
            let format = args.u32();
            let kind = args.u32();
            let pointer = args.u32();
            if target != TEXTURE_2D {
                return Call(GlCall::Error(INVALID_ENUM));
            }
            if level < 0 || width < 0 || height < 0 {
                return Call(GlCall::Error(INVALID_VALUE));
            }
            match unpack_image(
                ctx,
                args.memory(),
                width as u32,
                height as u32,
                format,
                kind,
                pointer,
            ) {
                Ok(Some(pixels)) => Call(GlCall::TexSubImage2D {
                    level: level as usize,
                    x,
                    y,
                    width: width as u32,
                    height: height as u32,
                    pixels: Rc::new(pixels),
                }),
                Ok(None) => Call(GlCall::Error(INVALID_VALUE)),
                Err(error) => Call(GlCall::Error(error)),
            }
        }
        GlNamed::CopyTexImage2D => {
            let _target = args.u32();
            let level = args.i32().max(0) as usize;
            let internal = args.u32();
            let x = args.i32();
            let y = args.i32();
            let width = args.i32().max(0) as u32;
            let height = args.i32().max(0) as u32;
            let _border = args.i32();
            Call(GlCall::CopyTexImage2D {
                level,
                internal,
                x,
                y,
                width,
                height,
            })
        }
        GlNamed::CopyTexSubImage2D => {
            let _target = args.u32();
            let level = args.i32().max(0) as usize;
            let xoffset = args.i32();
            let yoffset = args.i32();
            let x = args.i32();
            let y = args.i32();
            let width = args.i32().max(0) as u32;
            let height = args.i32().max(0) as u32;
            Call(GlCall::CopyTexSubImage2D {
                level,
                xoffset,
                yoffset,
                x,
                y,
                width,
                height,
            })
        }
        GlNamed::PixelStorei | GlNamed::PixelStoref => {
            let pname = args.u32();
            let v = if named == GlNamed::PixelStoref {
                args.f32() as i32
            } else {
                args.i32()
            };
            let client = &mut ctx.state.client;
            let (store, field) = if pname >= PACK_SWAP_BYTES {
                (&mut client.pack, pname - PACK_SWAP_BYTES)
            } else {
                (&mut client.unpack, pname.wrapping_sub(UNPACK_SWAP_BYTES))
            };
            match field {
                0 => store.swap_bytes = v != 0,
                1 => store.lsb_first = v != 0,
                2 => store.row_length = v,
                3 => store.skip_rows = v,
                4 => store.skip_pixels = v,
                5 if matches!(v, 1 | 2 | 4 | 8) => store.alignment = v,
                _ => ctx.state.set_error(INVALID_ENUM),
            }
            done()
        }
        GlNamed::PixelTransferf => {
            let pname = args.u32();
            Call(GlCall::PixelTransfer(pname, args.f32()))
        }
        GlNamed::PixelTransferi => {
            let pname = args.u32();
            Call(GlCall::PixelTransfer(pname, args.i32() as f32))
        }
        GlNamed::PixelMapfv | GlNamed::PixelMapuiv | GlNamed::PixelMapusv => done(),
        GlNamed::ClipPlane => {
            let plane = args.u32();
            let pointer = args.u32();
            let memory = args.memory();
            let equation = std::array::from_fn(|index| {
                read_component(
                    memory,
                    pointer.wrapping_add(index as u32 * 8),
                    AttribType::Double,
                ) as f32
            });
            Call(GlCall::ClipPlane(plane, equation))
        }
        GlNamed::GetClipPlane => {
            let plane = args.u32().wrapping_sub(CLIP_PLANE0) as usize;
            let pointer = args.u32();
            if plane < MAX_CLIP_PLANES_COUNT {
                let modelview = *ctx.state.modelview_top();
                // Stored planes are in eye space; report them as specified.
                let equation = modelview
                    .transpose()
                    .transform(ctx.state.raster.clip_planes[plane]);
                let memory = args.memory();
                for (index, component) in equation.iter().enumerate() {
                    let bits = f64::from(*component).to_bits();
                    let _ = memory
                        .write_u32_be(pointer.wrapping_add(index as u32 * 8), (bits >> 32) as u32);
                    let _ = memory
                        .write_u32_be(pointer.wrapping_add(index as u32 * 8 + 4), bits as u32);
                }
            }
            done()
        }
        GlNamed::NewList => {
            let list = args.u32();
            let mode = args.u32();
            if list == 0 {
                ctx.state.set_error(INVALID_VALUE);
            } else if !matches!(mode, COMPILE | COMPILE_AND_EXECUTE) {
                ctx.state.set_error(INVALID_ENUM);
            } else if ctx.compiling.is_some() {
                ctx.state.set_error(INVALID_OPERATION);
            } else {
                ctx.compiling = Some((list, mode == COMPILE_AND_EXECUTE, Vec::new()));
            }
            done()
        }
        GlNamed::EndList => {
            match ctx.compiling.take() {
                Some((list, _, calls)) => {
                    ctx.lists.insert(list, Rc::new(calls));
                }
                None => ctx.state.set_error(INVALID_OPERATION),
            }
            done()
        }
        GlNamed::GenLists => {
            let range = args.i32();
            if range <= 0 {
                return value(0);
            }
            let mut first = 1u32;
            while (first..first + range as u32).any(|list| ctx.lists.contains_key(&list)) {
                first += 1;
            }
            for list in first..first + range as u32 {
                ctx.lists.insert(list, Rc::new(Vec::new()));
            }
            value(first)
        }
        GlNamed::DeleteLists => {
            let first = args.u32();
            let range = args.i32().max(0) as u32;
            for list in first..first.saturating_add(range) {
                ctx.lists.remove(&list);
            }
            done()
        }
        GlNamed::IsList => {
            let list = args.u32();
            value(u32::from(ctx.lists.contains_key(&list)))
        }
        GlNamed::CallList => Call(GlCall::CallList(args.u32())),
        GlNamed::CallLists => {
            let count = args.i32().max(0) as u32;
            let kind = args.u32();
            let pointer = args.u32();
            let memory = args.memory();
            let lists = (0..count)
                .map(|index| match kind {
                    UNSIGNED_BYTE => {
                        u32::from(memory.read_u8(pointer.wrapping_add(index)).unwrap_or(0))
                    }
                    BYTE => memory.read_u8(pointer.wrapping_add(index)).unwrap_or(0) as i8 as u32,
                    UNSIGNED_SHORT => u32::from(
                        memory
                            .read_u16_be(pointer.wrapping_add(index * 2))
                            .unwrap_or(0),
                    ),
                    SHORT => memory
                        .read_u16_be(pointer.wrapping_add(index * 2))
                        .unwrap_or(0) as i16 as u32,
                    FLOAT => f32::from_bits(
                        memory
                            .read_u32_be(pointer.wrapping_add(index * 4))
                            .unwrap_or(0),
                    ) as u32,
                    _ => memory
                        .read_u32_be(pointer.wrapping_add(index * 4))
                        .unwrap_or(0),
                })
                .collect();
            Call(GlCall::CallLists(lists))
        }
        GlNamed::ListBase => Call(GlCall::ListBase(args.u32())),
        GlNamed::PushAttrib => {
            let _mask = args.u32();
            Call(GlCall::PushAttrib)
        }
        GlNamed::PopAttrib => Call(GlCall::PopAttrib),
        GlNamed::PushClientAttrib | GlNamed::PopClientAttrib => done(),
        GlNamed::Finish | GlNamed::Flush => done(),
        GlNamed::GetError => value(ctx.state.take_error()),
        GlNamed::GetString => {
            let name = args.u32();
            let pointer = gl_string(args.memory(), name);
            if pointer == 0 {
                ctx.state.set_error(INVALID_ENUM);
            }
            value(pointer)
        }
        GlNamed::GetIntegerv | GlNamed::GetFloatv | GlNamed::GetBooleanv | GlNamed::GetDoublev => {
            let pname = args.u32();
            let pointer = args.u32();
            let Some((values, is_color)) = get_values(ctx, pname) else {
                ctx.state.set_error(INVALID_ENUM);
                return done();
            };
            let memory = args.memory();
            for (index, v) in values.iter().enumerate() {
                let index = index as u32;
                let _ = match named {
                    GlNamed::GetFloatv => {
                        memory.write_u32_be(pointer.wrapping_add(index * 4), (*v as f32).to_bits())
                    }
                    GlNamed::GetBooleanv => {
                        memory.write_u8(pointer.wrapping_add(index), u8::from(*v != 0.0))
                    }
                    GlNamed::GetDoublev => {
                        let bits = v.to_bits();
                        memory
                            .write_u32_be(pointer.wrapping_add(index * 8), (bits >> 32) as u32)
                            .and_then(|_| {
                                memory
                                    .write_u32_be(pointer.wrapping_add(index * 8 + 4), bits as u32)
                            })
                    }
                    _ => {
                        let integer = if is_color {
                            (v.clamp(-1.0, 1.0) * 2_147_483_647.0).round() as i32
                        } else {
                            v.round() as i32
                        };
                        memory.write_u32_be(pointer.wrapping_add(index * 4), integer as u32)
                    }
                };
            }
            done()
        }
        GlNamed::GetPointerv => {
            let pname = args.u32();
            let pointer = args.u32();
            let client = &ctx.state.client;
            let address = match pname {
                0x808E => client.vertex.pointer,
                0x808F => client.normal.pointer,
                0x8090 => client.color.pointer,
                0x8092 => client.tex[client.active_unit].pointer,
                _ => 0,
            };
            let _ = args.memory().write_u32_be(pointer, address);
            done()
        }
        GlNamed::GetLightfv
        | GlNamed::GetMaterialfv
        | GlNamed::GetTexEnvfv
        | GlNamed::GetTexEnviv
        | GlNamed::GetTexParameterfv
        | GlNamed::GetTexParameteriv => {
            let target = args.u32();
            let pname = args.u32();
            let pointer = args.u32();
            let values: Option<Vec<f32>> = match named {
                GlNamed::GetLightfv => {
                    let index = target.wrapping_sub(LIGHT0) as usize;
                    ctx.state.raster.lights.get(index).map(|light| match pname {
                        AMBIENT => light.ambient.to_vec(),
                        DIFFUSE => light.diffuse.to_vec(),
                        SPECULAR => light.specular.to_vec(),
                        POSITION => light.position.to_vec(),
                        SPOT_DIRECTION => light.spot_direction.to_vec(),
                        SPOT_EXPONENT => vec![light.spot_exponent],
                        SPOT_CUTOFF => vec![light.spot_cutoff],
                        CONSTANT_ATTENUATION => vec![light.attenuation[0]],
                        LINEAR_ATTENUATION => vec![light.attenuation[1]],
                        _ => vec![light.attenuation[2]],
                    })
                }
                GlNamed::GetMaterialfv => {
                    let material = ctx.state.raster.materials[usize::from(target == BACK)];
                    Some(match pname {
                        AMBIENT => material.ambient.to_vec(),
                        DIFFUSE => material.diffuse.to_vec(),
                        SPECULAR => material.specular.to_vec(),
                        EMISSION => material.emission.to_vec(),
                        _ => vec![material.shininess],
                    })
                }
                GlNamed::GetTexEnvfv | GlNamed::GetTexEnviv => {
                    let env = ctx.state.raster.units[ctx.state.raster.active_unit].env;
                    Some(match pname {
                        TEXTURE_ENV_COLOR => env.color.to_vec(),
                        _ => vec![env.mode as f32],
                    })
                }
                _ => {
                    let texture = ctx.state.bound_texture(ctx.state.raster.active_unit);
                    Some(match pname {
                        TEXTURE_MIN_FILTER => vec![texture.min_filter as f32],
                        TEXTURE_MAG_FILTER => vec![texture.mag_filter as f32],
                        TEXTURE_WRAP_S => vec![texture.wrap_s as f32],
                        TEXTURE_WRAP_T => vec![texture.wrap_t as f32],
                        TEXTURE_BORDER_COLOR => texture.border_color.to_vec(),
                        TEXTURE_PRIORITY => vec![texture.priority],
                        _ => vec![0.0],
                    })
                }
            };
            let as_int = matches!(named, GlNamed::GetTexEnviv | GlNamed::GetTexParameteriv);
            if let Some(values) = values {
                let memory = args.memory();
                for (index, v) in values.iter().enumerate() {
                    let word = if as_int {
                        *v as i32 as u32
                    } else {
                        v.to_bits()
                    };
                    let _ = memory.write_u32_be(pointer.wrapping_add(index as u32 * 4), word);
                }
            } else {
                ctx.state.set_error(INVALID_ENUM);
            }
            done()
        }
        GlNamed::GetTexLevelParameteriv => {
            let _target = args.u32();
            let level = args.i32().max(0) as usize;
            let pname = args.u32();
            let pointer = args.u32();
            let texture = ctx.state.bound_texture(ctx.state.raster.active_unit);
            let image = texture.levels.get(level).and_then(Option::as_ref);
            let result = match pname {
                0x1000 => image.map_or(0, |image| image.width as i32),
                0x1001 => image.map_or(0, |image| image.height as i32),
                0x1003 => texture.internal_format as i32,
                0x805C..=0x805F | 0x8060..=0x8061 => 8,
                _ => 0,
            };
            let _ = args.memory().write_u32_be(pointer, result as u32);
            done()
        }
        GlNamed::ReadPixels => {
            let x = args.i32();
            let y = args.i32();
            let width = args.i32().max(0) as u32;
            let height = args.i32().max(0) as u32;
            let format = args.u32();
            let kind = args.u32();
            let pointer = args.u32();
            if PixelLayout::new(format, kind).is_none() {
                ctx.state.set_error(INVALID_ENUM);
                return done();
            }
            let values: Vec<[f32; 4]> = match format {
                DEPTH_COMPONENT => {
                    let Some(framebuffer) = framebuffer.as_deref() else {
                        return done();
                    };
                    (0..height as i32)
                        .flat_map(|row| (0..width as i32).map(move |column| (x + column, y + row)))
                        .map(|(px, py)| {
                            let depth = if px >= 0 && py >= 0 {
                                framebuffer.depth_at(px as u32, py as u32).unwrap_or(1.0)
                            } else {
                                1.0
                            };
                            [depth, depth, depth, depth]
                        })
                        .collect()
                }
                STENCIL_INDEX => {
                    let Some(framebuffer) = framebuffer.as_deref() else {
                        return done();
                    };
                    (0..height as i32)
                        .flat_map(|row| (0..width as i32).map(move |column| (x + column, y + row)))
                        .map(|(px, py)| {
                            let stencil = if px >= 0 && py >= 0 {
                                framebuffer.stencil_at(px as u32, py as u32).unwrap_or(0)
                            } else {
                                0
                            };
                            let v = f32::from(stencil) / 255.0;
                            [v, v, v, v]
                        })
                        .collect()
                }
                _ => match ctx.read_color_rect(framebuffer, x, y, width, height) {
                    Some(values) => values,
                    None => return done(),
                },
            };
            let store = ctx.state.client.pack;
            if !pack_pixels(
                args.memory(),
                pointer,
                width as usize,
                height as usize,
                format,
                kind,
                &store,
                &values,
            ) {
                ctx.state.set_error(INVALID_VALUE);
            }
            done()
        }
    }
}

#[cfg(test)]
mod tests;
