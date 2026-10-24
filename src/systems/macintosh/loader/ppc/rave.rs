//! QuickDraw 3D RAVE (Renderer Acceleration Virtual Engine) 1.6 for PowerPC
//! applications that link `QuickDraw™ 3D Accelerator` directly.
//!
//! A draw context is a guest `TQADrawContext` whose method pointers are
//! transition vectors into reserved import trap slots, so the application's
//! `QA<Method>(context, ...)` macros land in [`ppc_rave_method`]. Triangles,
//! lines and points arrive in window coordinates and are rasterised by the
//! classic OpenGL pipeline's rasteriser into a per-context colour and Z
//! buffer, which `QARenderEnd` presents to the context's GDevice rectangle or
//! memory device. Structure layouts and constants follow Apple's RAVE.h
//! (QuickDraw 3D 1.6).

use super::classic_gl::consts as gl;
use super::classic_gl::raster::{self, RasterTarget, WindowVertex};
use super::classic_gl::state::ContextState;
use super::classic_gl::texture::{BaseFormat, Texture};
use super::classic_gl_framebuffer::{ClassicGlColorBuffer, ClassicGlFramebuffer};
use super::*;
use std::collections::BTreeMap;

/// Number of method pointers in a RAVE 1.6 `TQADrawContext`.
pub(crate) const PPC_QA_METHOD_COUNT: u32 = 35;
/// Size of a `TQADrawContext`: drawPrivate, version, then the methods.
const DRAW_CONTEXT_SIZE: u32 = 8 + PPC_QA_METHOD_COUNT * 4;
const QA_VERSION_1_6: u32 = 4;

// TQAError.
const QA_NO_ERR: u32 = 0;
const QA_ERROR: u32 = 1;
const QA_OUT_OF_MEMORY: u32 = 2;
const QA_NOT_SUPPORTED: u32 = 3;
const QA_PARAM_ERR: u32 = 5;

// Method slots, in TQADrawContext order.
const M_SET_FLOAT: u32 = 0;
const M_SET_INT: u32 = 1;
const M_SET_PTR: u32 = 2;
const M_GET_FLOAT: u32 = 3;
const M_GET_INT: u32 = 4;
const M_GET_PTR: u32 = 5;
const M_DRAW_POINT: u32 = 6;
const M_DRAW_LINE: u32 = 7;
const M_DRAW_TRI_GOURAUD: u32 = 8;
const M_DRAW_TRI_TEXTURE: u32 = 9;
const M_DRAW_V_GOURAUD: u32 = 10;
const M_DRAW_V_TEXTURE: u32 = 11;
const M_DRAW_BITMAP: u32 = 12;
const M_RENDER_START: u32 = 13;
const M_RENDER_END: u32 = 14;
const M_RENDER_ABORT: u32 = 15;
const M_FLUSH: u32 = 16;
const M_SYNC: u32 = 17;
const M_SUBMIT_VERTICES_GOURAUD: u32 = 18;
const M_SUBMIT_VERTICES_TEXTURE: u32 = 19;
const M_DRAW_TRI_MESH_GOURAUD: u32 = 20;
const M_DRAW_TRI_MESH_TEXTURE: u32 = 21;
const M_SET_NOTICE_METHOD: u32 = 22;
const M_GET_NOTICE_METHOD: u32 = 23;
const M_SUBMIT_MULTI_TEXTURE_PARAMS: u32 = 24;
const M_ACCESS_DRAW_BUFFER: u32 = 25;
const M_ACCESS_DRAW_BUFFER_END: u32 = 26;
const M_ACCESS_Z_BUFFER: u32 = 27;
const M_ACCESS_Z_BUFFER_END: u32 = 28;
const M_CLEAR_DRAW_BUFFER: u32 = 29;
const M_CLEAR_Z_BUFFER: u32 = 30;
const M_TEXTURE_FROM_CONTEXT: u32 = 31;
const M_BITMAP_FROM_CONTEXT: u32 = 32;
const M_BUSY: u32 = 33;
const M_SWAP_BUFFERS: u32 = 34;

// TQATagInt / TQATagPtr / TQATagFloat.
const TAG_Z_FUNCTION: u32 = 0;
const TAG_COLOR_BG_A: u32 = 1;
const TAG_COLOR_BG_B: u32 = 4;
const TAG_WIDTH: u32 = 5;
const TAG_Z_MIN_OFFSET: u32 = 6;
const TAG_Z_MIN_SCALE: u32 = 7;
const TAG_BLEND: u32 = 9;
const TAG_PERSPECTIVE_Z: u32 = 10;
const TAG_TEXTURE_FILTER: u32 = 11;
const TAG_TEXTURE_OP: u32 = 12;
const TAG_TEXTURE: u32 = 13;
const TAG_FOG_MODE: u32 = 17;
const TAG_FOG_COLOR_A: u32 = 18;
const TAG_FOG_COLOR_B: u32 = 21;
const TAG_FOG_START: u32 = 22;
const TAG_FOG_END: u32 = 23;
const TAG_FOG_DENSITY: u32 = 24;
const TAG_CHANNEL_MASK: u32 = 27;
const TAG_Z_BUFFER_MASK: u32 = 28;
const TAG_ALPHA_TEST_FUNC: u32 = 31;
const TAG_DONT_SWAP: u32 = 32;
const TAG_ALPHA_TEST_REF: u32 = 46;
const TAG_BITMAP_SCALE_X: u32 = 52;
const TAG_BITMAP_SCALE_Y: u32 = 53;
const TAG_GL_TEXTURE_WRAP_U: u32 = 101;
const TAG_GL_TEXTURE_WRAP_V: u32 = 102;
const TAG_GL_TEXTURE_MAG_FILTER: u32 = 103;
const TAG_GL_TEXTURE_MIN_FILTER: u32 = 104;
const TAG_GL_SCISSOR_X_MIN: u32 = 105;
const TAG_GL_SCISSOR_Y_MAX: u32 = 108;
const TAG_GL_BLEND_SRC: u32 = 109;
const TAG_GL_BLEND_DST: u32 = 110;
const TAG_GL_DEPTH_BG: u32 = 112;

// kQATextureOp_*.
const TEXTURE_OP_MODULATE: u32 = 1 << 0;
const TEXTURE_OP_HIGHLIGHT: u32 = 1 << 1;
const TEXTURE_OP_DECAL: u32 = 1 << 2;
const TEXTURE_OP_SHRINK: u32 = 1 << 3;
const TEXTURE_OP_BLEND: u32 = 1 << 4;

// TQAImagePixelType.
const PIXEL_ALPHA1: u32 = 0;
const PIXEL_RGB16: u32 = 1;
const PIXEL_ARGB16: u32 = 2;
const PIXEL_RGB32: u32 = 3;
const PIXEL_ARGB32: u32 = 4;
const PIXEL_CL4: u32 = 5;
const PIXEL_CL8: u32 = 6;
const PIXEL_RGB8_332: u32 = 9;
const PIXEL_ARGB16_4444: u32 = 10;
const PIXEL_ACL16_88: u32 = 11;
const PIXEL_I8: u32 = 12;
const PIXEL_AI16_88: u32 = 13;

const CONTEXT_NO_Z_BUFFER: u32 = 1 << 0;
const TEXTURE_MIPMAP: u32 = 1 << 1;
const TEXTURE_FLIP_ORIGIN: u32 = 1 << 6;

/// Opaque handles for engine-owned objects that applications never
/// dereference (textures, bitmaps and colour tables).
const FIRST_RAVE_HANDLE: u32 = 0x6100_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcRaveFunction {
    DrawContextNew,
    DrawContextDelete,
    ColorTableNew,
    ColorTableDelete,
    TextureNew,
    TextureDetach,
    TextureDelete,
    TextureBindColorTable,
    BitmapNew,
    BitmapDetach,
    BitmapDelete,
    BitmapBindColorTable,
    EngineCheckDevice,
    EngineEnable,
    EngineDisable,
    AccessTexture,
    AccessTextureEnd,
    AccessBitmap,
    AccessBitmapEnd,
    RegisterDrawNotificationProc,
    UnregisterDrawNotificationProc,
}

impl PpcRaveFunction {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "QADrawContextNew" => Self::DrawContextNew,
            "QADrawContextDelete" => Self::DrawContextDelete,
            "QAColorTableNew" => Self::ColorTableNew,
            "QAColorTableDelete" => Self::ColorTableDelete,
            "QATextureNew" => Self::TextureNew,
            "QATextureDetach" => Self::TextureDetach,
            "QATextureDelete" => Self::TextureDelete,
            "QATextureBindColorTable" => Self::TextureBindColorTable,
            "QABitmapNew" => Self::BitmapNew,
            "QABitmapDetach" => Self::BitmapDetach,
            "QABitmapDelete" => Self::BitmapDelete,
            "QABitmapBindColorTable" => Self::BitmapBindColorTable,
            "QAEngineCheckDevice" => Self::EngineCheckDevice,
            "QAEngineEnable" => Self::EngineEnable,
            "QAEngineDisable" => Self::EngineDisable,
            "QAAccessTexture" => Self::AccessTexture,
            "QAAccessTextureEnd" => Self::AccessTextureEnd,
            "QAAccessBitmap" => Self::AccessBitmap,
            "QAAccessBitmapEnd" => Self::AccessBitmapEnd,
            "QARegisterDrawNotificationProc" => Self::RegisterDrawNotificationProc,
            "QAUnregisterDrawNotificationProc" => Self::UnregisterDrawNotificationProc,
            _ => return None,
        })
    }
}

/// The target a RAVE draw context presents to.
#[derive(Debug, Clone, Copy)]
enum RaveDevice {
    Memory {
        row_bytes: u32,
        pixel_type: u32,
        base: u32,
    },
    GDevice(u32),
}

/// A texture's source image kept for `kQATexture_NoCopy`/`QAAccessTexture`.
#[derive(Debug, Clone)]
struct RaveImageSource {
    pixel_type: u32,
    levels: Vec<(u32, u32, u32, u32)>,
    flip: bool,
    color_table: Option<u32>,
}

#[derive(Debug, Clone)]
struct RaveBitmap {
    width: u32,
    height: u32,
    texels: Vec<[f32; 4]>,
    /// kQAPixel_Alpha1 bitmaps take their colour from the drawing vertex.
    coverage_only: bool,
}

#[derive(Debug, Clone)]
struct RaveContext {
    device: RaveDevice,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    flags: u32,
    framebuffer: ClassicGlFramebuffer,
    ints: BTreeMap<u32, u32>,
    floats: BTreeMap<u32, f32>,
    texture: u32,
    gouraud: Vec<WindowVertex>,
    textured: Vec<WindowVertex>,
    notices: BTreeMap<u32, (u32, u32)>,
}

/// Engine state for every RAVE object a process owns.
#[derive(Debug, Clone)]
pub(crate) struct PpcRaveState {
    contexts: BTreeMap<u32, RaveContext>,
    /// Rasteriser state; RAVE textures live in its texture map.
    gl: ContextState,
    texture_sources: BTreeMap<u32, RaveImageSource>,
    bitmaps: BTreeMap<u32, RaveBitmap>,
    color_tables: BTreeMap<u32, Vec<[f32; 4]>>,
    next_handle: u32,
    next_notification: u32,
}

impl Default for PpcRaveState {
    fn default() -> Self {
        Self {
            contexts: BTreeMap::new(),
            gl: ContextState::default(),
            texture_sources: BTreeMap::new(),
            bitmaps: BTreeMap::new(),
            color_tables: BTreeMap::new(),
            next_handle: FIRST_RAVE_HANDLE,
            next_notification: 1,
        }
    }
}

impl PpcRaveState {
    fn new_handle(&mut self) -> u32 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(0x10);
        handle
    }
}

/// The transition vector for draw-context method `method`.
pub(crate) fn ppc_rave_method_tvector(method: u32) -> u32 {
    PPC_IMPORT_TVECTOR_BASE + (PPC_QA_METHOD_IMPORT_BASE + method) * 8
}

/// Whether an import trap slot index is a RAVE draw-context method.
pub(crate) fn ppc_rave_method_for_import_index(index: u32) -> Option<u32> {
    index
        .checked_sub(PPC_QA_METHOD_IMPORT_BASE)
        .filter(|method| *method < PPC_QA_METHOD_COUNT)
}

fn read_f32(memory: &mut PpcSectionMem, address: u32) -> f32 {
    f32::from_bits(memory.read_u32_be(address).unwrap_or(0))
}

fn read_floats<const N: usize>(memory: &mut PpcSectionMem, address: u32) -> [f32; N] {
    std::array::from_fn(|index| read_f32(memory, address.wrapping_add(index as u32 * 4)))
}

impl RaveContext {
    fn int(&self, tag: u32) -> u32 {
        self.ints
            .get(&tag)
            .copied()
            .unwrap_or_else(|| default_int(tag, self.flags))
    }

    fn float(&self, tag: u32) -> f32 {
        self.floats
            .get(&tag)
            .copied()
            .unwrap_or_else(|| default_float(tag))
    }

    /// Window vertex for a TQAVGouraud. RAVE y grows downwards; the
    /// rasteriser uses OpenGL's bottom-up rows.
    fn gouraud_vertex(&self, v: [f32; 8]) -> WindowVertex {
        let [x, y, z, inv_w, r, g, b, a] = v;
        self.window_vertex(x, y, z, inv_w, [r, g, b, a], [0.0; 3], [0.0, 0.0])
    }

    /// Window vertex for a TQAVTexture, applying kQATag_TextureOp: the
    /// vertex colour carries kd for Modulate (or the decal colour), and the
    /// highlight ks is added as a secondary colour.
    fn texture_vertex(&self, v: [f32; 16]) -> WindowVertex {
        let [x, y, z, inv_w, r, g, b, a, u_over_w, v_over_w, kd_r, kd_g, kd_b, ks_r, ks_g, ks_b] =
            v;
        let op = self.int(TAG_TEXTURE_OP);
        let color = if op & TEXTURE_OP_DECAL != 0 {
            [r, g, b, a]
        } else if op & TEXTURE_OP_MODULATE != 0 {
            [kd_r, kd_g, kd_b, a]
        } else {
            [1.0, 1.0, 1.0, a]
        };
        let specular = if op & TEXTURE_OP_HIGHLIGHT != 0 {
            [ks_r, ks_g, ks_b]
        } else {
            [0.0; 3]
        };
        let w = if inv_w != 0.0 { 1.0 / inv_w } else { 1.0 };
        self.window_vertex(
            x,
            y,
            z,
            inv_w,
            color,
            specular,
            [u_over_w * w, v_over_w * w],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn window_vertex(
        &self,
        x: f32,
        y: f32,
        z: f32,
        inv_w: f32,
        color: [f32; 4],
        specular: [f32; 3],
        uv: [f32; 2],
    ) -> WindowVertex {
        // kQAPerspectiveZ_On orders surfaces by 1/w (nearer is larger); map it
        // monotonically into [0, 1) so the depth comparisons keep their sense.
        let depth = if self.int(TAG_PERSPECTIVE_Z) != 0 && inv_w > 0.0 {
            1.0 / (1.0 + inv_w)
        } else {
            z
        };
        let inv_w = if inv_w > 0.0 { inv_w } else { 1.0 };
        WindowVertex {
            x,
            y: self.height as f32 - y,
            z: depth,
            inv_w,
            color,
            specular,
            tex: [[uv[0], uv[1], 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]],
            fog: 1.0 / inv_w,
            point_size: self.float(TAG_WIDTH),
            edge: true,
        }
    }
}

fn default_int(tag: u32, flags: u32) -> u32 {
    match tag {
        TAG_Z_FUNCTION if flags & CONTEXT_NO_Z_BUFFER != 0 => 0,
        TAG_Z_FUNCTION => 1,     // kQAZFunction_LT
        TAG_CHANNEL_MASK => 0xF, // all channels
        TAG_Z_BUFFER_MASK => 1,  // kQAZBufferMask_Enable
        TAG_GL_SCISSOR_X_MIN..=TAG_GL_SCISSOR_Y_MAX => 0,
        _ => 0,
    }
}

fn default_float(tag: u32) -> f32 {
    match tag {
        TAG_WIDTH | TAG_BITMAP_SCALE_X | TAG_BITMAP_SCALE_Y => 1.0,
        TAG_FOG_END | TAG_FOG_DENSITY | TAG_GL_DEPTH_BG => 1.0,
        // A 24-bit Z buffer resolves differences of one part in 2^24.
        TAG_Z_MIN_OFFSET | TAG_Z_MIN_SCALE => 1.0 / 16_777_216.0,
        _ => 0.0,
    }
}

/// Configure the shared rasteriser state from a context's tags.
fn configure(gl_state: &mut ContextState, context: &RaveContext, textured: bool) {
    let r = &mut gl_state.raster;
    let z_function = context.int(TAG_Z_FUNCTION);
    r.enables.depth_test = z_function != 0;
    r.depth_func = match z_function {
        1 => gl::LESS,
        2 => gl::EQUAL,
        3 => gl::LEQUAL,
        4 => gl::GREATER,
        5 => gl::NOTEQUAL,
        6 => gl::GEQUAL,
        8 => gl::NEVER,
        _ => gl::ALWAYS,
    };
    r.depth_mask = z_function != 0 && context.int(TAG_Z_BUFFER_MASK) != 0;
    // RAVE alpha always means transparency (RAVE.h, TQAVGouraud).
    r.enables.blend = true;
    let (src, dst) = match context.int(TAG_BLEND) {
        1 => (gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA),
        2 => (context.int(TAG_GL_BLEND_SRC), context.int(TAG_GL_BLEND_DST)),
        _ => (gl::ONE, gl::ONE_MINUS_SRC_ALPHA),
    };
    r.blend_src = if src == 0 && context.int(TAG_BLEND) == 2 {
        gl::ONE
    } else {
        src
    };
    r.blend_dst = dst;
    let mask = context.int(TAG_CHANNEL_MASK);
    r.color_mask = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0, mask & 8 != 0];
    let alpha_test = context.int(TAG_ALPHA_TEST_FUNC);
    r.enables.alpha_test = alpha_test != 0;
    r.alpha_func = match alpha_test {
        1 => gl::LESS,
        2 => gl::EQUAL,
        3 => gl::LEQUAL,
        4 => gl::GREATER,
        5 => gl::NOTEQUAL,
        6 => gl::GEQUAL,
        _ => gl::ALWAYS,
    };
    r.alpha_ref = context.float(TAG_ALPHA_TEST_REF);
    let fog_mode = context.int(TAG_FOG_MODE);
    r.enables.fog = matches!(fog_mode, 2..=4);
    r.fog_mode = match fog_mode {
        2 => gl::LINEAR,
        3 => gl::EXP,
        _ => gl::EXP2,
    };
    r.fog_start = context.float(TAG_FOG_START);
    r.fog_end = context.float(TAG_FOG_END);
    r.fog_density = context.float(TAG_FOG_DENSITY);
    r.fog_color = [
        context.float(TAG_FOG_COLOR_A + 1),
        context.float(TAG_FOG_COLOR_A + 2),
        context.float(TAG_FOG_COLOR_B),
        context.float(TAG_FOG_COLOR_A),
    ];
    let (x_min, y_min, x_max, y_max) = (
        context.int(TAG_GL_SCISSOR_X_MIN) as i32,
        context.int(TAG_GL_SCISSOR_X_MIN + 1) as i32,
        context.int(TAG_GL_SCISSOR_X_MIN + 2) as i32,
        context.int(TAG_GL_SCISSOR_Y_MAX) as i32,
    );
    r.enables.scissor_test = x_max > x_min && y_max > y_min;
    r.scissor = [
        x_min,
        context.height as i32 - y_max,
        x_max - x_min,
        y_max - y_min,
    ];
    // Highlight colours are added after texturing, like separate specular.
    r.separate_specular = true;
    r.enables.lighting = true;
    let unit = &mut r.units[0];
    unit.enabled_2d = textured && context.texture != 0;
    unit.binding_2d = context.texture;
    let op = context.int(TAG_TEXTURE_OP);
    unit.env.mode = if op & TEXTURE_OP_DECAL != 0 {
        gl::DECAL
    } else if op & TEXTURE_OP_BLEND != 0 {
        gl::BLEND
    } else {
        gl::MODULATE
    };
    if let Some(texture) = gl_state.textures.get_mut(&context.texture) {
        let filter = context.int(TAG_TEXTURE_FILTER);
        let mipmapped = texture.levels.len() > 1;
        texture.mag_filter = if filter == 0 { gl::NEAREST } else { gl::LINEAR };
        texture.min_filter = match (filter, mipmapped) {
            (0, false) => gl::NEAREST,
            (_, false) => gl::LINEAR,
            (0, true) => gl::NEAREST_MIPMAP_NEAREST,
            (1, true) => gl::LINEAR_MIPMAP_NEAREST,
            _ => gl::LINEAR_MIPMAP_LINEAR,
        };
        let shrink = op & TEXTURE_OP_SHRINK != 0;
        let wrap = |tag| {
            if shrink || context.int(tag) == 1 {
                gl::CLAMP_TO_EDGE
            } else {
                gl::REPEAT
            }
        };
        texture.wrap_s = wrap(TAG_GL_TEXTURE_WRAP_U);
        texture.wrap_t = wrap(TAG_GL_TEXTURE_WRAP_V);
        let _ = (TAG_GL_TEXTURE_MAG_FILTER, TAG_GL_TEXTURE_MIN_FILTER);
    }
}

fn with_target(
    gl_state: &ContextState,
    framebuffer: &mut ClassicGlFramebuffer,
    draw: impl FnOnce(&ContextState, &mut RasterTarget),
) {
    if let Some(mut target) = framebuffer.raster_target(ClassicGlColorBuffer::Back) {
        draw(gl_state, &mut target);
    }
}

/// Decode one RAVE image pixel to RGBA in [0, 1].
fn decode_pixel(
    memory: &mut PpcSectionMem,
    pixel_type: u32,
    address: u32,
    column: u32,
    table: Option<&Vec<[f32; 4]>>,
) -> [f32; 4] {
    let c8 = |v: u32| v as f32 / 255.0;
    let c5 = |v: u32| v as f32 / 31.0;
    let c4 = |v: u32| v as f32 / 15.0;
    match pixel_type {
        PIXEL_RGB16 | PIXEL_ARGB16 => {
            let v = u32::from(memory.read_u16_be(address + column * 2).unwrap_or(0));
            let a = if pixel_type == PIXEL_ARGB16 {
                (v >> 15) as f32
            } else {
                1.0
            };
            [c5((v >> 10) & 31), c5((v >> 5) & 31), c5(v & 31), a]
        }
        PIXEL_RGB32 | PIXEL_ARGB32 => {
            let v = memory.read_u32_be(address + column * 4).unwrap_or(0);
            let a = if pixel_type == PIXEL_ARGB32 {
                c8(v >> 24)
            } else {
                1.0
            };
            [c8((v >> 16) & 255), c8((v >> 8) & 255), c8(v & 255), a]
        }
        PIXEL_ARGB16_4444 => {
            let v = u32::from(memory.read_u16_be(address + column * 2).unwrap_or(0));
            [
                c4((v >> 8) & 15),
                c4((v >> 4) & 15),
                c4(v & 15),
                c4(v >> 12),
            ]
        }
        PIXEL_RGB8_332 => {
            let v = u32::from(memory.read_u8(address + column).unwrap_or(0));
            [
                ((v >> 5) & 7) as f32 / 7.0,
                ((v >> 2) & 7) as f32 / 7.0,
                (v & 3) as f32 / 3.0,
                1.0,
            ]
        }
        PIXEL_I8 => {
            let i = c8(u32::from(memory.read_u8(address + column).unwrap_or(0)));
            [i, i, i, 1.0]
        }
        PIXEL_AI16_88 => {
            let v = u32::from(memory.read_u16_be(address + column * 2).unwrap_or(0));
            let i = c8(v & 255);
            [i, i, i, c8(v >> 8)]
        }
        PIXEL_CL8 | PIXEL_ACL16_88 | PIXEL_CL4 => {
            let (index, alpha) = match pixel_type {
                PIXEL_CL8 => (
                    u32::from(memory.read_u8(address + column).unwrap_or(0)),
                    None,
                ),
                PIXEL_ACL16_88 => {
                    let v = u32::from(memory.read_u16_be(address + column * 2).unwrap_or(0));
                    (v & 255, Some(c8(v >> 8)))
                }
                _ => {
                    let byte = u32::from(memory.read_u8(address + column / 2).unwrap_or(0));
                    (
                        if column % 2 == 0 {
                            byte >> 4
                        } else {
                            byte & 15
                        },
                        None,
                    )
                }
            };
            let mut color = table
                .and_then(|table| table.get(index as usize).copied())
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            if let Some(alpha) = alpha {
                color[3] = alpha;
            }
            color
        }
        PIXEL_ALPHA1 => {
            // One bit per pixel, most significant bit leftmost; the colour
            // comes from the vertex the bitmap is drawn with.
            let byte = memory.read_u8(address + column / 8).unwrap_or(0);
            let alpha = f32::from((byte >> (7 - column % 8)) & 1);
            [1.0, 1.0, 1.0, alpha]
        }
        _ => {
            // YUV video formats are not used for textures or bitmaps here.
            [1.0; 4]
        }
    }
}

/// Read a TQAImage (width, height, rowBytes, pixmap) as RGBA rows from the
/// bottom row up, the order the rasteriser's texture images use.
fn read_image(
    memory: &mut PpcSectionMem,
    pixel_type: u32,
    width: u32,
    height: u32,
    row_bytes: u32,
    pixmap: u32,
    flip: bool,
    table: Option<&Vec<[f32; 4]>>,
) -> Vec<[f32; 4]> {
    let mut texels = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        // RAVE images are top-down unless kQATexture_FlipOrigin is set.
        let source_row = if flip { row } else { height - 1 - row };
        let address = pixmap.wrapping_add(source_row.wrapping_mul(row_bytes));
        for column in 0..width {
            texels.push(decode_pixel(memory, pixel_type, address, column, table));
        }
    }
    texels
}

fn load_texture(memory: &mut PpcSectionMem, rave: &mut PpcRaveState, handle: u32) {
    let Some(source) = rave.texture_sources.get(&handle).cloned() else {
        return;
    };
    let table = source
        .color_table
        .and_then(|table| rave.color_tables.get(&table))
        .cloned();
    let has_alpha = matches!(
        source.pixel_type,
        PIXEL_ARGB16
            | PIXEL_ARGB32
            | PIXEL_ARGB16_4444
            | PIXEL_AI16_88
            | PIXEL_ACL16_88
            | PIXEL_CL8
            | PIXEL_CL4
    );
    let base = if has_alpha {
        BaseFormat::Rgba
    } else {
        BaseFormat::Rgb
    };
    let mut texture = Texture::default();
    for (level, (width, height, row_bytes, pixmap)) in source.levels.iter().copied().enumerate() {
        let texels = read_image(
            memory,
            source.pixel_type,
            width,
            height,
            row_bytes,
            pixmap,
            source.flip,
            table.as_ref(),
        );
        texture.specify_level(level, gl::RGBA, base, width, height, Some(texels));
    }
    rave.gl.textures.insert(handle, texture);
}

/// Present the context's back buffer to its device.
fn present(memory: &mut PpcSectionMem, context: &RaveContext) {
    let (base, row_bytes, depth, x0, y0) = match context.device {
        RaveDevice::Memory {
            row_bytes,
            pixel_type,
            base,
        } => {
            let depth = match pixel_type {
                PIXEL_RGB16 | PIXEL_ARGB16 => 16,
                PIXEL_RGB32 | PIXEL_ARGB32 => 32,
                _ => return,
            };
            (base, row_bytes, depth, 0, 0)
        }
        RaveDevice::GDevice(gdevice) => {
            // GDevice gdPMap (offset 22) is a PixMapHandle for the screen,
            // whose bounds are in global coordinates.
            let Some(pixmap) = memory
                .read_u32_be(gdevice.wrapping_add(22))
                .and_then(|handle| memory.read_u32_be(handle))
            else {
                return;
            };
            let Some(bits) = quickdraw::ppc_read_pixmap_bits(memory, pixmap) else {
                return;
            };
            (
                bits.base_addr,
                bits.row_bytes,
                bits.depth,
                context.left - i32::from(bits.left),
                context.top - i32::from(bits.top),
            )
        }
    };
    if !matches!(depth, 16 | 32) {
        return;
    }
    let Some(pixels) = context.framebuffer.read_rgba(
        ClassicGlColorBuffer::Back,
        0,
        0,
        context.width,
        context.height,
    ) else {
        return;
    };
    let bytes_per_pixel = (depth / 8) as usize;
    let mut row = vec![0u8; context.width as usize * bytes_per_pixel];
    for y in 0..context.height {
        let screen_y = y0 + y as i32;
        if screen_y < 0 {
            continue;
        }
        let source = &pixels[((context.height - 1 - y) * context.width * 4) as usize..]
            [..(context.width * 4) as usize];
        for x in 0..context.width as usize {
            let [r, g, b] = [source[x * 4], source[x * 4 + 1], source[x * 4 + 2]];
            if depth == 16 {
                let v = (u16::from(r >> 3) << 10) | (u16::from(g >> 3) << 5) | u16::from(b >> 3);
                row[x * 2..x * 2 + 2].copy_from_slice(&v.to_be_bytes());
            } else {
                row[x * 4..x * 4 + 4].copy_from_slice(&[0, r, g, b]);
            }
        }
        let skip = (-x0).max(0) as usize;
        if skip >= context.width as usize {
            continue;
        }
        let address = base
            .wrapping_add((screen_y as u32).wrapping_mul(row_bytes))
            .wrapping_add((x0.max(0) as u32) * (depth / 8));
        let _ = memory.write_bytes(address, &row[skip * bytes_per_pixel..]);
    }
}

fn clear_context(context: &mut RaveContext, color: bool, depth: bool) {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let background = [
        to_u8(context.float(TAG_COLOR_BG_A + 1)),
        to_u8(context.float(TAG_COLOR_BG_A + 2)),
        to_u8(context.float(TAG_COLOR_BG_B)),
        to_u8(context.float(TAG_COLOR_BG_A)),
    ];
    let depth_value = context.float(TAG_GL_DEPTH_BG);
    context
        .framebuffer
        .clear(super::classic_gl_framebuffer::ClassicGlClear {
            color: color.then_some((ClassicGlColorBuffer::Back, background)),
            color_mask: [true; 4],
            depth: depth.then_some(depth_value),
            depth_mask: true,
            stencil: None,
            stencil_mask: 0,
            scissor: None,
        });
}

/// A heap allocator for guest memory owned by the engine.
pub(crate) type PpcRaveAllocator<'a> = &'a mut dyn FnMut(&mut PpcSectionMem, u32) -> u32;

/// `QA*` entry points exported by the accelerator library.
pub(crate) fn ppc_rave_function(
    function: PpcRaveFunction,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    rave: &mut PpcRaveState,
    allocate: PpcRaveAllocator<'_>,
) -> PpcImportAction {
    let arguments = [
        cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8],
    ];
    let action = ppc_rave_function_untraced(function, cpu, memory, rave, allocate);
    if ppc_hle_trace_enabled() {
        eprintln!("[PPC-TRACE] RAVE {function:?} r3-r8={arguments:08X?} -> {action:?}");
    }
    action
}

fn ppc_rave_function_untraced(
    function: PpcRaveFunction,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    rave: &mut PpcRaveState,
    allocate: PpcRaveAllocator<'_>,
) -> PpcImportAction {
    let result = |error: u32| PpcImportAction::Return(error);
    let engine_ok = |engine: u32| engine == PPC_QA_ENGINE || engine == PPC_QA_SOFTWARE_ENGINE;
    match function {
        PpcRaveFunction::DrawContextNew => {
            // TQAError QADrawContextNew(const TQADevice *device, const TQARect
            //     *rect, const TQAClip *clip, const TQAEngine *engine,
            //     unsigned long flags, TQADrawContext **newDrawContext);
            let (device, rect, _clip, engine, flags, out) = (
                cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8],
            );
            if device == 0 || rect == 0 || out == 0 || !engine_ok(engine) {
                return result(QA_PARAM_ERR);
            }
            let device_type = memory.read_u32_be(device).unwrap_or(u32::MAX);
            let rave_device = match device_type {
                0 => RaveDevice::Memory {
                    row_bytes: memory.read_u32_be(device + 4).unwrap_or(0),
                    pixel_type: memory.read_u32_be(device + 8).unwrap_or(0),
                    base: memory.read_u32_be(device + 20).unwrap_or(0),
                },
                1 => RaveDevice::GDevice(memory.read_u32_be(device + 4).unwrap_or(0)),
                _ => return result(QA_NOT_SUPPORTED),
            };
            let [left, right, top, bottom] = std::array::from_fn(|index| {
                memory.read_u32_be(rect + index as u32 * 4).unwrap_or(0) as i32
            });
            let (width, height) = ((right - left).max(0) as u32, (bottom - top).max(0) as u32);
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] RAVE context device={rave_device:?} rect=({left},{top})-({right},{bottom}) flags=0x{flags:X}"
                );
            }
            let Some(framebuffer) = ClassicGlFramebuffer::new(width, height, true) else {
                return result(QA_PARAM_ERR);
            };
            let pointer = allocate(memory, DRAW_CONTEXT_SIZE);
            if pointer == 0 {
                return result(QA_OUT_OF_MEMORY);
            }
            let _ = memory.write_u32_be(pointer, pointer);
            let _ = memory.write_u32_be(pointer + 4, QA_VERSION_1_6);
            for method in 0..PPC_QA_METHOD_COUNT {
                let _ =
                    memory.write_u32_be(pointer + 8 + method * 4, ppc_rave_method_tvector(method));
            }
            let mut context = RaveContext {
                device: rave_device,
                left,
                top,
                width,
                height,
                flags,
                framebuffer,
                ints: BTreeMap::new(),
                floats: BTreeMap::new(),
                texture: 0,
                gouraud: Vec::new(),
                textured: Vec::new(),
                notices: BTreeMap::new(),
            };
            clear_context(&mut context, true, true);
            rave.contexts.insert(pointer, context);
            let _ = memory.write_u32_be(out, pointer);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::DrawContextDelete => {
            rave.contexts.remove(&cpu.gpr[3]);
            PpcImportAction::ReturnPreserve
        }
        PpcRaveFunction::ColorTableNew => {
            // QAColorTableNew(engine, tableType, pixelData, transparentIndexFlag, newTable)
            let (engine, table_type, data, transparent, out) =
                (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]);
            if !engine_ok(engine) || data == 0 || out == 0 {
                return result(QA_PARAM_ERR);
            }
            let count = if table_type == 1 { 16 } else { 256 };
            let mut entries: Vec<[f32; 4]> = (0..count)
                .map(|index| {
                    let v = memory.read_u32_be(data + index * 4).unwrap_or(0);
                    [
                        ((v >> 16) & 255) as f32 / 255.0,
                        ((v >> 8) & 255) as f32 / 255.0,
                        (v & 255) as f32 / 255.0,
                        1.0,
                    ]
                })
                .collect();
            if transparent != 0 {
                entries[0][3] = 0.0;
            }
            let handle = rave.new_handle();
            rave.color_tables.insert(handle, entries);
            let _ = memory.write_u32_be(out, handle);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::ColorTableDelete => {
            rave.color_tables.remove(&cpu.gpr[4]);
            PpcImportAction::ReturnPreserve
        }
        PpcRaveFunction::TextureNew => {
            // QATextureNew(engine, flags, pixelType, const TQAImage images[], newTexture)
            let (engine, flags, pixel_type, images, out) =
                (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]);
            if !engine_ok(engine) || images == 0 || out == 0 {
                return result(QA_PARAM_ERR);
            }
            let read_image_record = |memory: &mut PpcSectionMem, index: u32| {
                let base = images + index * 16;
                (
                    memory.read_u32_be(base).unwrap_or(0),
                    memory.read_u32_be(base + 4).unwrap_or(0),
                    memory.read_u32_be(base + 8).unwrap_or(0),
                    memory.read_u32_be(base + 12).unwrap_or(0),
                )
            };
            let first = read_image_record(memory, 0);
            if first.0 == 0 || first.1 == 0 || first.0 > 2048 || first.1 > 2048 || first.3 == 0 {
                return result(QA_PARAM_ERR);
            }
            let mut levels = vec![first];
            if flags & TEXTURE_MIPMAP != 0 {
                let (mut w, mut h) = (first.0, first.1);
                let mut index = 1;
                while w > 1 || h > 1 {
                    w = (w / 2).max(1);
                    h = (h / 2).max(1);
                    let level = read_image_record(memory, index);
                    if level.3 == 0 {
                        break;
                    }
                    levels.push((w, h, level.2, level.3));
                    index += 1;
                }
            }
            let handle = rave.new_handle();
            rave.texture_sources.insert(
                handle,
                RaveImageSource {
                    pixel_type,
                    levels,
                    flip: flags & TEXTURE_FLIP_ORIGIN != 0,
                    color_table: None,
                },
            );
            load_texture(memory, rave, handle);
            let _ = memory.write_u32_be(out, handle);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::TextureDetach => {
            // The engine has copied the image; the application may reuse it.
            load_texture(memory, rave, cpu.gpr[4]);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::TextureDelete => {
            let handle = cpu.gpr[4];
            rave.texture_sources.remove(&handle);
            rave.gl.textures.remove(&handle);
            PpcImportAction::ReturnPreserve
        }
        PpcRaveFunction::TextureBindColorTable => {
            let (texture, table) = (cpu.gpr[4], cpu.gpr[5]);
            let Some(source) = rave.texture_sources.get_mut(&texture) else {
                return result(QA_PARAM_ERR);
            };
            source.color_table = Some(table);
            load_texture(memory, rave, texture);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::BitmapNew => {
            // QABitmapNew(engine, flags, pixelType, const TQAImage *image, newBitmap)
            let (engine, flags, pixel_type, image, out) =
                (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]);
            if !engine_ok(engine) || image == 0 || out == 0 {
                return result(QA_PARAM_ERR);
            }
            let width = memory.read_u32_be(image).unwrap_or(0);
            let height = memory.read_u32_be(image + 4).unwrap_or(0);
            let row_bytes = memory.read_u32_be(image + 8).unwrap_or(0);
            let pixmap = memory.read_u32_be(image + 12).unwrap_or(0);
            if width == 0 || height == 0 || width > 4096 || height > 4096 || pixmap == 0 {
                return result(QA_PARAM_ERR);
            }
            let texels = read_image(
                memory,
                pixel_type,
                width,
                height,
                row_bytes,
                pixmap,
                flags & TEXTURE_FLIP_ORIGIN != 0,
                None,
            );
            let handle = rave.new_handle();
            rave.bitmaps.insert(
                handle,
                RaveBitmap {
                    width,
                    height,
                    texels,
                    coverage_only: pixel_type == PIXEL_ALPHA1,
                },
            );
            let _ = memory.write_u32_be(out, handle);
            result(QA_NO_ERR)
        }
        PpcRaveFunction::BitmapDetach => result(QA_NO_ERR),
        PpcRaveFunction::BitmapDelete => {
            rave.bitmaps.remove(&cpu.gpr[4]);
            PpcImportAction::ReturnPreserve
        }
        PpcRaveFunction::BitmapBindColorTable => result(QA_NO_ERR),
        PpcRaveFunction::EngineCheckDevice => {
            // The engine renders to memory devices and 16/32-bit GDevices.
            let (engine, device) = (cpu.gpr[3], cpu.gpr[4]);
            let ok = engine_ok(engine)
                && device != 0
                && memory.read_u32_be(device).is_some_and(|kind| kind <= 1);
            result(if ok { QA_NO_ERR } else { QA_NOT_SUPPORTED })
        }
        PpcRaveFunction::EngineEnable | PpcRaveFunction::EngineDisable => result(QA_NO_ERR),
        PpcRaveFunction::AccessTexture | PpcRaveFunction::AccessBitmap => result(QA_NOT_SUPPORTED),
        PpcRaveFunction::AccessTextureEnd | PpcRaveFunction::AccessBitmapEnd => result(QA_NO_ERR),
        PpcRaveFunction::RegisterDrawNotificationProc => {
            if cpu.gpr[6] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[6], rave.next_notification);
                rave.next_notification += 1;
            }
            result(QA_NO_ERR)
        }
        PpcRaveFunction::UnregisterDrawNotificationProc => result(QA_NO_ERR),
    }
}

fn draw_vertices(
    rave: &mut PpcRaveState,
    context_ptr: u32,
    mode: u32,
    vertices: &[WindowVertex],
    textured: bool,
) {
    let PpcRaveState {
        contexts,
        gl: gl_state,
        ..
    } = rave;
    let Some(context) = contexts.get_mut(&context_ptr) else {
        return;
    };
    configure(gl_state, context, textured);
    with_target(
        gl_state,
        &mut context.framebuffer,
        |state, target| match mode {
            0 => {
                for v in vertices {
                    raster::draw_point(state, target, v, false);
                }
            }
            1 => {
                for pair in vertices.chunks_exact(2) {
                    raster::draw_line(state, target, &pair[0], &pair[1], false, 0);
                }
            }
            2 => {
                for pair in vertices.windows(2) {
                    raster::draw_line(state, target, &pair[0], &pair[1], false, 0);
                }
            }
            3 => {
                for tri in vertices.chunks_exact(3) {
                    raster::draw_triangle(state, target, &tri[0], &tri[1], &tri[2], 0.0);
                }
            }
            4 => {
                for i in 2..vertices.len() {
                    raster::draw_triangle(
                        state,
                        target,
                        &vertices[i - 2],
                        &vertices[i - 1],
                        &vertices[i],
                        0.0,
                    );
                }
            }
            5 => {
                for i in 2..vertices.len() {
                    raster::draw_triangle(
                        state,
                        target,
                        &vertices[0],
                        &vertices[i - 1],
                        &vertices[i],
                        0.0,
                    );
                }
            }
            _ => {}
        },
    );
}

fn read_gouraud(memory: &mut PpcSectionMem, context: &RaveContext, address: u32) -> WindowVertex {
    context.gouraud_vertex(read_floats::<8>(memory, address))
}

fn read_texture_vertex(
    memory: &mut PpcSectionMem,
    context: &RaveContext,
    address: u32,
) -> WindowVertex {
    context.texture_vertex(read_floats::<16>(memory, address))
}

/// A method called through a draw context's method table.
pub(crate) fn ppc_rave_method(
    method: u32,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    rave: &mut PpcRaveState,
    allocate: PpcRaveAllocator<'_>,
) -> PpcImportAction {
    let trace = ppc_hle_trace_enabled();
    let arguments = [cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]];
    let caller = cpu.lr;
    let action = ppc_rave_method_untraced(method, cpu, memory, rave, allocate);
    if trace {
        eprintln!(
            "[PPC-TRACE] RAVE method {method} lr=${caller:08X} r3-r6={arguments:08X?} f1={} -> {action:?}",
            f64::from_bits(cpu.fpr[1])
        );
    }
    action
}

fn ppc_rave_method_untraced(
    method: u32,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    rave: &mut PpcRaveState,
    allocate: PpcRaveAllocator<'_>,
) -> PpcImportAction {
    let context_ptr = cpu.gpr[3];
    if !rave.contexts.contains_key(&context_ptr) {
        // Methods of an unknown context: return kQAParamErr where a result
        // is expected and otherwise do nothing.
        return PpcImportAction::Return(QA_PARAM_ERR);
    }
    let error = |e: u32| PpcImportAction::Return(e);
    match method {
        M_SET_FLOAT => {
            let tag = cpu.gpr[4];
            let value = f64::from_bits(cpu.fpr[1]) as f32;
            rave.contexts
                .get_mut(&context_ptr)
                .expect("checked")
                .floats
                .insert(tag, value);
            PpcImportAction::ReturnPreserve
        }
        M_SET_INT => {
            let (tag, value) = (cpu.gpr[4], cpu.gpr[5]);
            rave.contexts
                .get_mut(&context_ptr)
                .expect("checked")
                .ints
                .insert(tag, value);
            PpcImportAction::ReturnPreserve
        }
        M_SET_PTR => {
            let (tag, value) = (cpu.gpr[4], cpu.gpr[5]);
            if tag == TAG_TEXTURE {
                rave.contexts
                    .get_mut(&context_ptr)
                    .expect("checked")
                    .texture = value;
            }
            PpcImportAction::ReturnPreserve
        }
        M_GET_FLOAT => {
            let value = rave.contexts[&context_ptr].float(cpu.gpr[4]);
            cpu.fpr[1] = f64::from(value).to_bits();
            PpcImportAction::ReturnPreserve
        }
        M_GET_INT => PpcImportAction::Return(rave.contexts[&context_ptr].int(cpu.gpr[4])),
        M_GET_PTR => PpcImportAction::Return(if cpu.gpr[4] == TAG_TEXTURE {
            rave.contexts[&context_ptr].texture
        } else {
            0
        }),
        M_DRAW_POINT => {
            let v = read_gouraud(memory, &rave.contexts[&context_ptr], cpu.gpr[4]);
            draw_vertices(rave, context_ptr, 0, &[v], false);
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_LINE => {
            let context = &rave.contexts[&context_ptr];
            let v = [
                read_gouraud(memory, context, cpu.gpr[4]),
                read_gouraud(memory, context, cpu.gpr[5]),
            ];
            draw_vertices(rave, context_ptr, 1, &v, false);
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_TRI_GOURAUD => {
            let context = &rave.contexts[&context_ptr];
            let v = [cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]]
                .map(|address| read_gouraud(memory, context, address));
            draw_vertices(rave, context_ptr, 3, &v, false);
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_TRI_TEXTURE => {
            let context = &rave.contexts[&context_ptr];
            let v = [cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]]
                .map(|address| read_texture_vertex(memory, context, address));
            draw_vertices(rave, context_ptr, 3, &v, true);
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_V_GOURAUD | M_DRAW_V_TEXTURE => {
            let (count, mode, array) = (cpu.gpr[4].min(65536), cpu.gpr[5], cpu.gpr[6]);
            let textured = method == M_DRAW_V_TEXTURE;
            let stride = if textured { 64 } else { 32 };
            let context = &rave.contexts[&context_ptr];
            let vertices: Vec<WindowVertex> = (0..count)
                .map(|index| {
                    let address = array.wrapping_add(index * stride);
                    if textured {
                        read_texture_vertex(memory, context, address)
                    } else {
                        read_gouraud(memory, context, address)
                    }
                })
                .collect();
            draw_vertices(rave, context_ptr, mode, &vertices, textured);
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_BITMAP => {
            // QADrawBitmap(context, const TQAVGouraud *v, TQABitmap *bitmap):
            // v gives the bitmap's top-left corner and Z.
            let (vertex, bitmap) = (cpu.gpr[4], cpu.gpr[5]);
            let [x, y, z, _, r, g, b, a] = read_floats::<8>(memory, vertex);
            let Some(bitmap) = rave.bitmaps.get(&bitmap).cloned() else {
                return PpcImportAction::ReturnPreserve;
            };
            let PpcRaveState {
                contexts,
                gl: gl_state,
                ..
            } = rave;
            let context = contexts.get_mut(&context_ptr).expect("checked");
            configure(gl_state, context, false);
            let scale_x = context.float(TAG_BITMAP_SCALE_X);
            let scale_y = context.float(TAG_BITMAP_SCALE_Y);
            let height = context.height as f32;
            with_target(gl_state, &mut context.framebuffer, |state, target| {
                let out_w = (bitmap.width as f32 * scale_x).round() as u32;
                let out_h = (bitmap.height as f32 * scale_y).round() as u32;
                for row in 0..out_h {
                    for column in 0..out_w {
                        let source_row = ((row as f32 / scale_y) as u32).min(bitmap.height - 1);
                        let source_column =
                            ((column as f32 / scale_x) as u32).min(bitmap.width - 1);
                        // Bitmap texels are stored bottom-up.
                        let mut texel = bitmap.texels[((bitmap.height - 1 - source_row)
                            * bitmap.width
                            + source_column)
                            as usize];
                        if texel[3] == 0.0 {
                            continue;
                        }
                        if bitmap.coverage_only {
                            texel = [r, g, b, a * texel[3]];
                        }
                        let point = WindowVertex {
                            x: x + column as f32 + 0.5,
                            y: height - (y + row as f32) - 0.5,
                            z,
                            inv_w: 1.0,
                            color: texel,
                            specular: [0.0; 3],
                            tex: [[0.0, 0.0, 0.0, 1.0]; 2],
                            fog: 1.0,
                            point_size: 1.0,
                            edge: true,
                        };
                        raster::draw_point(state, target, &point, false);
                    }
                }
            });
            PpcImportAction::ReturnPreserve
        }
        M_RENDER_START => {
            // QARenderStart(context, dirtyRect, initialContext): initialise the
            // buffers with the background colour and Z (RAVE.h).
            let context = rave.contexts.get_mut(&context_ptr).expect("checked");
            let has_z = context.flags & CONTEXT_NO_Z_BUFFER == 0;
            clear_context(context, true, has_z);
            PpcImportAction::ReturnPreserve
        }
        M_RENDER_END | M_SWAP_BUFFERS => {
            let context = &rave.contexts[&context_ptr];
            if method == M_SWAP_BUFFERS || context.int(TAG_DONT_SWAP) == 0 {
                present(memory, context);
            }
            error(QA_NO_ERR)
        }
        M_RENDER_ABORT | M_FLUSH | M_SYNC => error(QA_NO_ERR),
        M_SUBMIT_VERTICES_GOURAUD | M_SUBMIT_VERTICES_TEXTURE => {
            let (count, array) = (cpu.gpr[4].min(65536), cpu.gpr[5]);
            let textured = method == M_SUBMIT_VERTICES_TEXTURE;
            let stride = if textured { 64 } else { 32 };
            let context = &rave.contexts[&context_ptr];
            let vertices: Vec<WindowVertex> = (0..count)
                .map(|index| {
                    let address = array.wrapping_add(index * stride);
                    if textured {
                        read_texture_vertex(memory, context, address)
                    } else {
                        read_gouraud(memory, context, address)
                    }
                })
                .collect();
            let context = rave.contexts.get_mut(&context_ptr).expect("checked");
            if textured {
                context.textured = vertices;
            } else {
                context.gouraud = vertices;
            }
            PpcImportAction::ReturnPreserve
        }
        M_DRAW_TRI_MESH_GOURAUD | M_DRAW_TRI_MESH_TEXTURE => {
            // TQAIndexedTriangle: triangleFlags then three vertex indices.
            let (count, triangles) = (cpu.gpr[4].min(65536), cpu.gpr[5]);
            let textured = method == M_DRAW_TRI_MESH_TEXTURE;
            let context = &rave.contexts[&context_ptr];
            let source = if textured {
                &context.textured
            } else {
                &context.gouraud
            };
            let mut vertices = Vec::with_capacity(count as usize * 3);
            for index in 0..count {
                let base = triangles.wrapping_add(index * 16);
                for corner in 0..3 {
                    let vertex = memory
                        .read_u32_be(base + 4 + corner * 4)
                        .unwrap_or(u32::MAX);
                    if let Some(v) = source.get(vertex as usize) {
                        vertices.push(*v);
                    }
                }
            }
            vertices.truncate(vertices.len() / 3 * 3);
            draw_vertices(rave, context_ptr, 3, &vertices, textured);
            PpcImportAction::ReturnPreserve
        }
        M_SET_NOTICE_METHOD => {
            let (selector, callback, refcon) = (cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]);
            rave.contexts
                .get_mut(&context_ptr)
                .expect("checked")
                .notices
                .insert(selector, (callback, refcon));
            error(QA_NO_ERR)
        }
        M_GET_NOTICE_METHOD => {
            let (selector, callback_out, refcon_out) = (cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]);
            let (callback, refcon) = rave.contexts[&context_ptr]
                .notices
                .get(&selector)
                .copied()
                .unwrap_or((0, 0));
            if callback_out != 0 {
                let _ = memory.write_u32_be(callback_out, callback);
            }
            if refcon_out != 0 {
                let _ = memory.write_u32_be(refcon_out, refcon);
            }
            error(QA_NO_ERR)
        }
        M_SUBMIT_MULTI_TEXTURE_PARAMS => PpcImportAction::ReturnPreserve,
        M_CLEAR_DRAW_BUFFER | M_CLEAR_Z_BUFFER => {
            let context = rave.contexts.get_mut(&context_ptr).expect("checked");
            clear_context(
                context,
                method == M_CLEAR_DRAW_BUFFER,
                method == M_CLEAR_Z_BUFFER,
            );
            error(QA_NO_ERR)
        }
        M_ACCESS_DRAW_BUFFER
        | M_ACCESS_Z_BUFFER
        | M_TEXTURE_FROM_CONTEXT
        | M_BITMAP_FROM_CONTEXT => {
            let _ = allocate;
            error(QA_NOT_SUPPORTED)
        }
        M_ACCESS_DRAW_BUFFER_END | M_ACCESS_Z_BUFFER_END => error(QA_NO_ERR),
        M_BUSY => PpcImportAction::Return(0),
        _ => error(QA_ERROR),
    }
}

/// Features the engine implements (kQAOptional_xxx).
pub(crate) fn ppc_rave_optional_features() -> u32 {
    (1 << 0)        // DeepZ: 24-bit Z buffer
        | (1 << 1)  // Texture
        | (1 << 2)  // TextureHQ: trilinear mipmapping
        | (1 << 3)  // TextureColor: modulation and highlight
        | (1 << 4)  // Blend
        | (1 << 5)  // BlendAlpha
        | (1 << 8)  // PerspectiveZ
        | (1 << 13) // CL4
        | (1 << 14) // CL8
        | (1 << 18) // FogDepth
        | (1 << 21) // ChannelMask
        | (1 << 22) // ZBufferMask
        | (1 << 23) // AlphaTest
        | (1 << 28) // ClearDrawBuffer
        | (1 << 29) // ClearZBuffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_tvectors_route_to_reserved_import_slots() {
        for method in 0..PPC_QA_METHOD_COUNT {
            let tvector = ppc_rave_method_tvector(method);
            let index = (tvector - PPC_IMPORT_TVECTOR_BASE) / 8;
            assert_eq!(ppc_rave_method_for_import_index(index), Some(method));
            assert!(index < PPC_IMPORT_SLOT_COUNT);
        }
        assert_eq!(
            ppc_rave_method_for_import_index(PPC_QA_METHOD_IMPORT_BASE - 1),
            None
        );
        assert_eq!(
            ppc_rave_method_for_import_index(PPC_IMPORT_SLOT_COUNT),
            None
        );
    }

    fn new_context(rave: &mut PpcRaveState, memory: &mut PpcSectionMem) -> u32 {
        // TQADevice { kQADeviceMemory, rowBytes, pixelType, width, height, baseAddr }
        memory.add_region(0x1000, vec![0; 0x100]);
        memory.add_region(0x8000, vec![0; 4 * 4 * 4]);
        memory.add_region(0x9000, vec![0; 0x400]);
        let device = [0u32, 16, PIXEL_RGB32, 4, 4, 0x8000];
        for (index, word) in device.iter().enumerate() {
            memory
                .write_u32_be(0x1000 + index as u32 * 4, *word)
                .unwrap();
        }
        // TQARect { left, right, top, bottom }
        for (index, word) in [0u32, 4, 0, 4].iter().enumerate() {
            memory
                .write_u32_be(0x1040 + index as u32 * 4, *word)
                .unwrap();
        }
        let mut cpu = PpcCpu::new();
        cpu.gpr[3] = 0x1000;
        cpu.gpr[4] = 0x1040;
        cpu.gpr[6] = PPC_QA_ENGINE;
        cpu.gpr[8] = 0x1080;
        let mut next = 0x9000;
        let mut allocate = |_: &mut PpcSectionMem, size: u32| {
            let pointer = next;
            next += size;
            pointer
        };
        assert!(matches!(
            ppc_rave_function(
                PpcRaveFunction::DrawContextNew,
                &mut cpu,
                memory,
                rave,
                &mut allocate
            ),
            PpcImportAction::Return(QA_NO_ERR)
        ));
        let context = memory.read_u32_be(0x1080).unwrap();
        assert_eq!(memory.read_u32_be(context + 4), Some(QA_VERSION_1_6));
        assert_eq!(
            memory.read_u32_be(context + 8 + M_RENDER_END * 4),
            Some(ppc_rave_method_tvector(M_RENDER_END))
        );
        context
    }

    #[test]
    fn gouraud_triangle_renders_and_presents_to_a_memory_device() {
        let mut rave = PpcRaveState::default();
        let mut memory = PpcSectionMem::new();
        let context = new_context(&mut rave, &mut memory);
        let mut allocate = |_: &mut PpcSectionMem, _: u32| 0;
        let mut cpu = PpcCpu::new();
        cpu.gpr[3] = context;
        // Background red via kQATag_ColorBG_r, then RenderStart.
        cpu.gpr[4] = TAG_COLOR_BG_A + 1;
        cpu.fpr[1] = 1.0f64.to_bits();
        ppc_rave_method(M_SET_FLOAT, &mut cpu, &mut memory, &mut rave, &mut allocate);
        cpu.gpr[4] = TAG_COLOR_BG_A;
        ppc_rave_method(M_SET_FLOAT, &mut cpu, &mut memory, &mut rave, &mut allocate);
        cpu.gpr[4] = 0;
        ppc_rave_method(
            M_RENDER_START,
            &mut cpu,
            &mut memory,
            &mut rave,
            &mut allocate,
        );
        // A green triangle covering the top-left half (x + y < 4).
        memory.add_region(0x2000, vec![0; 96]);
        let corners = [[0.0f32, 0.0], [4.0, 0.0], [0.0, 4.0]];
        for (index, [x, y]) in corners.iter().enumerate() {
            let vertex = [*x, *y, 0.5, 1.0, 0.0, 1.0, 0.0, 1.0];
            for (component, value) in vertex.iter().enumerate() {
                memory
                    .write_u32_be(
                        0x2000 + index as u32 * 32 + component as u32 * 4,
                        value.to_bits(),
                    )
                    .unwrap();
            }
        }
        cpu.gpr[4] = 0x2000;
        cpu.gpr[5] = 0x2020;
        cpu.gpr[6] = 0x2040;
        ppc_rave_method(
            M_DRAW_TRI_GOURAUD,
            &mut cpu,
            &mut memory,
            &mut rave,
            &mut allocate,
        );
        cpu.gpr[4] = 0;
        ppc_rave_method(
            M_RENDER_END,
            &mut cpu,
            &mut memory,
            &mut rave,
            &mut allocate,
        );
        // Row 0 (top) pixel 0 is green; the bottom-right pixel stays red.
        assert_eq!(memory.read_u32_be(0x8000), Some(0x0000_FF00));
        assert_eq!(
            memory.read_u32_be(0x8000 + 3 * 16 + 3 * 4),
            Some(0x00FF_0000)
        );
    }

    #[test]
    fn textures_convert_rave_pixel_types() {
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x3000, vec![0; 16]);
        memory.write_u16_be(0x3000, 0xFC00).unwrap(); // ARGB16: opaque red
        assert_eq!(
            decode_pixel(&mut memory, PIXEL_ARGB16, 0x3000, 0, None),
            [1.0, 0.0, 0.0, 1.0]
        );
        memory.write_u32_be(0x3004, 0x8000_FF00).unwrap();
        let green = decode_pixel(&mut memory, PIXEL_ARGB32, 0x3004, 0, None);
        assert!((green[3] - 128.0 / 255.0).abs() < 1e-6 && green[1] == 1.0);
    }
}
