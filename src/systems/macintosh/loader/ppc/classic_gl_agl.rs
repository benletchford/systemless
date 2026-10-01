//! Classic AGL pixel-format requests decoded from guest `GLint` arrays.
//!
//! Attribute values and the boolean/value distinction come from Apple's
//! AGL/agl.h (Mac OS X 10.2.8 SDK). Capability selection happens separately.
//! https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.2.8.sdk/System/Library/Frameworks/AGL.framework/Versions/A/Headers/agl.h

use super::classic_gl_framebuffer::{ClassicGlClear, ClassicGlColorBuffer, ClassicGlFramebuffer};
use super::classic_gl_raster::{draw_triangle, ClassicGlRasterState, ClassicGlVertex};
use super::classic_gl_texture::{ClassicGlTextureImage, ClassicGlTextures};
use super::classic_gl_transform::ClassicGlTransform;
use super::{ppc_memory_can_write_bytes, PpcFrontBuffer, PpcSectionMem};
use ppc::PpcMemory;

const MAX_ATTRIBUTE_WORDS: u32 = 64;
const FIRST_AGL_OBJECT: u32 = 0x0500_0000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PpcAglPixelFormatRequest {
    pub rgba: bool,
    pub double_buffered: bool,
    pub stereo: bool,
    pub fullscreen: bool,
    pub offscreen: bool,
    pub accelerated: bool,
    pub no_recovery: bool,
    pub backing_store: bool,
    pub depth_bits: Option<i32>,
    pub stencil_bits: Option<i32>,
    pub alpha_bits: Option<i32>,
    pub red_bits: Option<i32>,
    pub green_bits: Option<i32>,
    pub blue_bits: Option<i32>,
    pub pixel_bits: Option<i32>,
    pub aux_buffers: Option<i32>,
    pub other_values: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcAglAttributeError {
    BadPointer,
    UnsupportedAttribute(i32),
    Unterminated,
}

#[derive(Debug, Clone)]
pub struct PpcAglPixelFormat {
    pub handle: u32,
    pub request: PpcAglPixelFormatRequest,
}

#[derive(Debug, Clone)]
pub struct PpcAglState {
    next_handle: u32,
    pixel_formats: Vec<PpcAglPixelFormat>,
    contexts: Vec<PpcAglContext>,
    current_context: u32,
}

#[derive(Debug, Clone)]
pub struct PpcAglContext {
    pub handle: u32,
    pub format: PpcAglPixelFormatRequest,
    pub drawable: u32,
    pub framebuffer: Option<ClassicGlFramebuffer>,
    clear_color: [f64; 4],
    clear_depth: f64,
    clear_stencil: i32,
    color_mask: [bool; 4],
    depth_mask: bool,
    stencil_mask: u32,
    scissor_enabled: bool,
    scissor: (i32, i32, u32, u32),
    scissor_explicit: bool,
    read_buffer: ClassicGlColorBuffer,
    draw_front: bool,
    draw_back: bool,
    pack: PpcGlPixelPack,
    transform: ClassicGlTransform,
    viewport: (i32, i32, u32, u32),
    viewport_explicit: bool,
    depth_range: (f64, f64),
    depth_test: bool,
    depth_func: u32,
    alpha_test: bool,
    alpha_func: u32,
    alpha_ref: u8,
    blend_enabled: bool,
    blend_src: u32,
    blend_dst: u32,
    current_color: [f64; 4],
    current_texcoord: [f64; 4],
    texture_2d_enabled: bool,
    primitive_mode: Option<u32>,
    vertices: Vec<ClassicGlVertex>,
    vertex_array: PpcGlArrayPointer,
    color_array: PpcGlArrayPointer,
    texcoord_array: PpcGlArrayPointer,
    textures: ClassicGlTextures,
}

#[derive(Debug, Clone, Copy)]
struct PpcGlArrayPointer {
    enabled: bool,
    size: u32,
    component_type: u32,
    stride: u32,
    pointer: u32,
}

impl Default for PpcGlArrayPointer {
    fn default() -> Self {
        Self {
            enabled: false,
            size: 4,
            component_type: 0x1406, // GL_FLOAT
            stride: 0,
            pointer: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PpcGlPixelPack {
    alignment: u32,
    row_length: u32,
    skip_rows: u32,
    skip_pixels: u32,
}

impl Default for PpcGlPixelPack {
    fn default() -> Self {
        Self {
            alignment: 4,
            row_length: 0,
            skip_rows: 0,
            skip_pixels: 0,
        }
    }
}

impl Default for PpcAglState {
    fn default() -> Self {
        Self {
            next_handle: FIRST_AGL_OBJECT,
            pixel_formats: Vec::new(),
            contexts: Vec::new(),
            current_context: 0,
        }
    }
}

impl PpcAglState {
    pub fn choose_pixel_format(&mut self, request: PpcAglPixelFormatRequest) -> u32 {
        if !ppc_agl_software_format_matches(&request) {
            return 0;
        }
        let handle = self.next_handle;
        let Some(next) = handle.checked_add(4) else {
            return 0;
        };
        self.next_handle = next;
        self.pixel_formats
            .push(PpcAglPixelFormat { handle, request });
        handle
    }

    pub fn pixel_format(&self, handle: u32) -> Option<&PpcAglPixelFormat> {
        self.pixel_formats
            .iter()
            .find(|format| format.handle == handle)
    }

    pub fn describe_pixel_format(&self, handle: u32, attribute: i32) -> Option<i32> {
        let request = &self.pixel_format(handle)?.request;
        Some(match attribute {
            2 | 3 | 7 | 14..=17 => 0,
            4 => 1, // AGL_RGBA
            5 => i32::from(request.double_buffered),
            6 | 53 | 73 => 0,  // stereo, offscreen, accelerated
            8..=11 => 8,       // RGBA8 color storage
            12 => 24,          // depth storage
            13 => 8,           // stencil storage
            50 => 32,          // AGL_PIXEL_SIZE
            54 => 1,           // fullscreen capable
            70 => 0x0002_0200, // AGL_RENDERER_GENERIC_ID
            76 => 0,           // backing store not selected by the software surface
            80 => 1,           // AGL_WINDOW
            _ => return None,
        })
    }

    pub fn destroy_pixel_format(&mut self, handle: u32) {
        self.pixel_formats.retain(|format| format.handle != handle);
    }

    pub fn create_context(&mut self, format: u32, share: u32) -> u32 {
        // Shared display lists and textures need shared GL object state.
        if share != 0 {
            return 0;
        }
        let Some(format) = self
            .pixel_format(format)
            .map(|format| format.request.clone())
        else {
            return 0;
        };
        let handle = self.next_handle;
        let Some(next) = handle.checked_add(4) else {
            return 0;
        };
        self.next_handle = next;
        let read_buffer = if format.double_buffered {
            ClassicGlColorBuffer::Back
        } else {
            ClassicGlColorBuffer::Front
        };
        let double_buffered = format.double_buffered;
        self.contexts.push(PpcAglContext {
            handle,
            format,
            drawable: 0,
            framebuffer: None,
            clear_color: [0.0; 4],
            clear_depth: 1.0,
            clear_stencil: 0,
            color_mask: [true; 4],
            depth_mask: true,
            stencil_mask: u32::MAX,
            scissor_enabled: false,
            scissor: (0, 0, 0, 0),
            scissor_explicit: false,
            read_buffer,
            draw_front: !double_buffered,
            draw_back: double_buffered,
            pack: PpcGlPixelPack::default(),
            transform: ClassicGlTransform::default(),
            viewport: (0, 0, 0, 0),
            viewport_explicit: false,
            depth_range: (0.0, 1.0),
            depth_test: false,
            depth_func: 0x0201, // GL_LESS
            alpha_test: false,
            alpha_func: 0x0207, // GL_ALWAYS
            alpha_ref: 0,
            blend_enabled: false,
            blend_src: 1, // GL_ONE
            blend_dst: 0, // GL_ZERO
            current_color: [1.0; 4],
            current_texcoord: [0.0, 0.0, 0.0, 1.0],
            texture_2d_enabled: false,
            primitive_mode: None,
            vertices: Vec::new(),
            vertex_array: PpcGlArrayPointer::default(),
            color_array: PpcGlArrayPointer::default(),
            texcoord_array: PpcGlArrayPointer::default(),
            textures: ClassicGlTextures::default(),
        });
        handle
    }

    pub fn context(&self, handle: u32) -> Option<&PpcAglContext> {
        self.contexts
            .iter()
            .find(|context| context.handle == handle)
    }

    pub fn context_mut(&mut self, handle: u32) -> Option<&mut PpcAglContext> {
        self.contexts
            .iter_mut()
            .find(|context| context.handle == handle)
    }

    pub fn destroy_context(&mut self, handle: u32) -> bool {
        let Some(index) = self
            .contexts
            .iter()
            .position(|context| context.handle == handle)
        else {
            return false;
        };
        self.contexts.swap_remove(index);
        if self.current_context == handle {
            self.current_context = 0;
        }
        true
    }

    pub fn set_current_context(&mut self, handle: u32) -> bool {
        if handle != 0 && self.context(handle).is_none() {
            return false;
        }
        self.current_context = handle;
        true
    }

    pub fn current_context(&self) -> u32 {
        self.current_context
    }

    pub fn current_transform_mut(&mut self) -> Option<&mut ClassicGlTransform> {
        Some(&mut self.context_mut(self.current_context)?.transform)
    }

    pub fn gl_viewport(&mut self, x: i32, y: i32, width: i32, height: i32) -> bool {
        if width < 0 || height < 0 {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.viewport = (x, y, width as u32, height as u32);
        context.viewport_explicit = true;
        true
    }

    pub fn gl_depth_range(&mut self, near: f64, far: f64) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.depth_range = (near.clamp(0.0, 1.0), far.clamp(0.0, 1.0));
        true
    }

    pub fn gl_depth_test(&mut self, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.depth_test = enabled;
        true
    }

    pub fn gl_depth_func(&mut self, function: u32) -> bool {
        if !(0x0200..=0x0207).contains(&function) {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.depth_func = function;
        true
    }

    pub fn gl_alpha_test(&mut self, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.alpha_test = enabled;
        true
    }

    pub fn gl_alpha_func(&mut self, function: u32, reference: f64) -> bool {
        if !(0x0200..=0x0207).contains(&function) || !reference.is_finite() {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.alpha_func = function;
        context.alpha_ref = (reference.clamp(0.0, 1.0) * 255.0).round() as u8;
        true
    }

    pub fn gl_blend(&mut self, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.blend_enabled = enabled;
        true
    }

    pub fn gl_blend_func(&mut self, source: u32, destination: u32) -> bool {
        let valid_source = matches!(source, 0 | 1 | 0x0302..=0x0308);
        let valid_destination = matches!(destination, 0 | 1 | 0x0300..=0x0305);
        if !valid_source || !valid_destination {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.blend_src = source;
        context.blend_dst = destination;
        true
    }

    pub fn gl_color(&mut self, color: [f64; 4]) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.current_color = color;
        true
    }

    pub fn gl_tex_coord(&mut self, coordinates: [f64; 4]) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.current_texcoord = coordinates;
        true
    }

    pub fn gl_texture_2d(&mut self, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.texture_2d_enabled = enabled;
        true
    }

    pub fn gl_begin(&mut self, mode: u32) -> bool {
        if !matches!(mode, 0x0004..=0x0007) {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.vertices.clear();
        context.primitive_mode = Some(mode);
        true
    }

    pub fn gl_vertex(&mut self, point: [f64; 4]) -> bool {
        const MAX_IMMEDIATE_VERTICES: usize = 1_000_000;
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_none() || context.vertices.len() >= MAX_IMMEDIATE_VERTICES {
            return false;
        }
        context.vertices.push(ClassicGlVertex {
            clip: context.transform.modelview_projection().transform(point),
            color: context.current_color,
            texcoord: context.current_texcoord,
        });
        true
    }

    pub fn gl_end(&mut self) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        let Some(mode) = context.primitive_mode.take() else {
            return false;
        };
        let vertices = std::mem::take(&mut context.vertices);
        let Some(framebuffer) = context.framebuffer.as_mut() else {
            return false;
        };
        let state = ClassicGlRasterState {
            viewport: context.viewport,
            depth_range: context.depth_range,
            scissor: context.scissor_enabled.then_some(context.scissor),
            draw_front: context.draw_front,
            draw_back: context.draw_back,
            color_mask: context.color_mask,
            depth_test: context.depth_test,
            depth_func: context.depth_func,
            depth_mask: context.depth_mask,
            alpha_test: context
                .alpha_test
                .then_some((context.alpha_func, context.alpha_ref)),
            blend: context
                .blend_enabled
                .then_some((context.blend_src, context.blend_dst)),
        };
        let texture = context
            .texture_2d_enabled
            .then(|| context.textures.bound())
            .flatten();
        let mut draw = |a: usize, b: usize, c: usize| {
            draw_triangle(
                framebuffer,
                [vertices[a], vertices[b], vertices[c]],
                state,
                texture,
            )
        };
        match mode {
            0x0004 => {
                for start in (0..vertices.len().saturating_sub(2)).step_by(3) {
                    if !draw(start, start + 1, start + 2) {
                        return false;
                    }
                }
            }
            0x0005 => {
                for start in 0..vertices.len().saturating_sub(2) {
                    let (a, b) = if start % 2 == 0 {
                        (start, start + 1)
                    } else {
                        (start + 1, start)
                    };
                    if !draw(a, b, start + 2) {
                        return false;
                    }
                }
            }
            0x0006 => {
                for index in 1..vertices.len().saturating_sub(1) {
                    if !draw(0, index, index + 1) {
                        return false;
                    }
                }
            }
            0x0007 => {
                for start in (0..vertices.len().saturating_sub(3)).step_by(4) {
                    if !draw(start, start + 1, start + 2) || !draw(start, start + 2, start + 3) {
                        return false;
                    }
                }
            }
            _ => unreachable!(),
        }
        true
    }

    pub fn gl_vertex_pointer(
        &mut self,
        size: i32,
        component_type: u32,
        stride: i32,
        pointer: u32,
    ) -> bool {
        if !(2..=4).contains(&size)
            || !matches!(component_type, 0x1402 | 0x1404 | 0x1406 | 0x140a)
            || stride < 0
        {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.vertex_array = PpcGlArrayPointer {
            enabled: context.vertex_array.enabled,
            size: size as u32,
            component_type,
            stride: stride as u32,
            pointer,
        };
        true
    }

    pub fn gl_color_pointer(
        &mut self,
        size: i32,
        component_type: u32,
        stride: i32,
        pointer: u32,
    ) -> bool {
        if !(3..=4).contains(&size)
            || !matches!(component_type, 0x1400..=0x1406 | 0x140a)
            || stride < 0
        {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.color_array = PpcGlArrayPointer {
            enabled: context.color_array.enabled,
            size: size as u32,
            component_type,
            stride: stride as u32,
            pointer,
        };
        true
    }

    pub fn gl_tex_coord_pointer(
        &mut self,
        size: i32,
        component_type: u32,
        stride: i32,
        pointer: u32,
    ) -> bool {
        if !(1..=4).contains(&size)
            || !matches!(component_type, 0x1402 | 0x1404 | 0x1406 | 0x140a)
            || stride < 0
        {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.texcoord_array = PpcGlArrayPointer {
            enabled: context.texcoord_array.enabled,
            size: size as u32,
            component_type,
            stride: stride as u32,
            pointer,
        };
        true
    }

    pub fn gl_client_state(&mut self, array: u32, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        match array {
            0x8074 => context.vertex_array.enabled = enabled, // GL_VERTEX_ARRAY
            0x8076 => context.color_array.enabled = enabled,  // GL_COLOR_ARRAY
            0x8078 => context.texcoord_array.enabled = enabled, // GL_TEXTURE_COORD_ARRAY
            _ => return false,
        }
        true
    }

    pub fn gl_draw_arrays(
        &mut self,
        memory: &mut PpcSectionMem,
        mode: u32,
        first: i32,
        count: i32,
    ) -> bool {
        if first < 0 || count < 0 || count > 1_000_000 {
            return false;
        }
        let Some(end) = (first as u32).checked_add(count as u32) else {
            return false;
        };
        self.gl_draw_indices(memory, mode, first as u32..end)
    }

    pub fn gl_draw_elements(
        &mut self,
        memory: &mut PpcSectionMem,
        mode: u32,
        count: i32,
        index_type: u32,
        indices: u32,
    ) -> bool {
        if count < 0 || count > 1_000_000 || !matches!(index_type, 0x1401 | 0x1403 | 0x1405) {
            return false;
        }
        let width = match index_type {
            0x1401 => 1,
            0x1403 => 2,
            _ => 4,
        };
        let mut values = Vec::with_capacity(count as usize);
        for index in 0..count as u32 {
            let Some(address) = index
                .checked_mul(width)
                .and_then(|offset| indices.checked_add(offset))
            else {
                return false;
            };
            let value = match index_type {
                0x1401 => memory.read_u8(address).map(u32::from),
                0x1403 => memory.read_u16_be(address).map(u32::from),
                _ => memory.read_u32_be(address),
            };
            let Some(value) = value else {
                return false;
            };
            values.push(value);
        }
        self.gl_draw_indices(memory, mode, values)
    }

    fn gl_draw_indices<I: IntoIterator<Item = u32>>(
        &mut self,
        memory: &mut PpcSectionMem,
        mode: u32,
        indices: I,
    ) -> bool {
        if !matches!(mode, 0x0004..=0x0007) {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() || !context.vertex_array.enabled {
            return false;
        }
        let matrix = context.transform.modelview_projection();
        let mut vertices = Vec::new();
        for index in indices {
            let Some(position) =
                ppc_gl_read_array_components(memory, context.vertex_array, index, false)
            else {
                return false;
            };
            let color = if context.color_array.enabled {
                let Some(color) =
                    ppc_gl_read_array_components(memory, context.color_array, index, true)
                else {
                    return false;
                };
                color
            } else {
                context.current_color
            };
            vertices.push(ClassicGlVertex {
                clip: matrix.transform(position),
                color,
                texcoord: if context.texcoord_array.enabled {
                    let Some(value) =
                        ppc_gl_read_array_components(memory, context.texcoord_array, index, false)
                    else {
                        return false;
                    };
                    value
                } else {
                    context.current_texcoord
                },
            });
        }
        context.vertices = vertices;
        context.primitive_mode = Some(mode);
        self.gl_end()
    }

    pub fn gl_clear_color(&mut self, components: [f64; 4]) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.clear_color = components.map(|component| component.clamp(0.0, 1.0));
        true
    }

    pub fn gl_clear_depth(&mut self, depth: f64) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.clear_depth = depth.clamp(0.0, 1.0);
        true
    }

    pub fn gl_clear_stencil(&mut self, stencil: i32) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.clear_stencil = stencil;
        true
    }

    pub fn gl_color_mask(&mut self, mask: [bool; 4]) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.color_mask = mask;
        true
    }

    pub fn gl_depth_mask(&mut self, mask: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.depth_mask = mask;
        true
    }

    pub fn gl_stencil_mask(&mut self, mask: u32) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.stencil_mask = mask;
        true
    }

    pub fn gl_scissor(&mut self, x: i32, y: i32, width: i32, height: i32) -> bool {
        if width < 0 || height < 0 {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.scissor = (x, y, width as u32, height as u32);
        context.scissor_explicit = true;
        true
    }

    pub fn gl_scissor_test(&mut self, enabled: bool) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.scissor_enabled = enabled;
        true
    }

    pub fn gl_read_buffer(&mut self, source: u32) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        context.read_buffer = match source {
            0x0400 | 0x0404 => ClassicGlColorBuffer::Front, // FRONT_LEFT, FRONT
            0x0402 | 0x0405 if context.format.double_buffered => ClassicGlColorBuffer::Back,
            _ => return false,
        };
        true
    }

    pub fn gl_draw_buffer(&mut self, target: u32) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        let (front, back) = match target {
            0 => (false, false),              // GL_NONE
            0x0400 | 0x0404 => (true, false), // FRONT_LEFT, FRONT
            0x0402 | 0x0405 if context.format.double_buffered => (false, true),
            0x0406 | 0x0408 => (true, context.format.double_buffered), // LEFT, FRONT_AND_BACK
            _ => return false,
        };
        context.draw_front = front;
        context.draw_back = back;
        true
    }

    pub fn gl_pixel_store_i(&mut self, name: u32, value: i32) -> bool {
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        if matches!(name, 0x0cf2..=0x0cf5) {
            return context.textures.unpack.set(name, value);
        }
        if value < 0 {
            return false;
        }
        let value = value as u32;
        match name {
            0x0d02 => context.pack.row_length = value, // PACK_ROW_LENGTH
            0x0d03 => context.pack.skip_rows = value,  // PACK_SKIP_ROWS
            0x0d04 => context.pack.skip_pixels = value, // PACK_SKIP_PIXELS
            0x0d05 if matches!(value, 1 | 2 | 4 | 8) => context.pack.alignment = value,
            _ => return false,
        }
        true
    }

    pub fn gl_gen_textures(
        &mut self,
        memory: &mut PpcSectionMem,
        count: i32,
        destination: u32,
    ) -> bool {
        let Ok(count) = u32::try_from(count) else {
            return false;
        };
        let Some(bytes) = count.checked_mul(4) else {
            return false;
        };
        if count > 0
            && (destination == 0 || !ppc_memory_can_write_bytes(memory, destination, bytes))
        {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        let Some(names) = context.textures.reserve_names(count) else {
            return false;
        };
        for (index, name) in names.into_iter().enumerate() {
            let Some(address) = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_mul(4))
                .and_then(|offset| destination.checked_add(offset))
            else {
                return false;
            };
            if memory.write_u32_be(address, name).is_none() {
                return false;
            }
        }
        true
    }

    pub fn gl_delete_textures(
        &mut self,
        memory: &mut PpcSectionMem,
        count: i32,
        source: u32,
    ) -> bool {
        let Ok(count) = u32::try_from(count) else {
            return false;
        };
        if count > 1_000_000 || (count > 0 && source == 0) {
            return false;
        }
        let mut names = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Some(address) = index
                .checked_mul(4)
                .and_then(|offset| source.checked_add(offset))
            else {
                return false;
            };
            let Some(name) = memory.read_u32_be(address) else {
                return false;
            };
            names.push(name);
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.textures.delete(&names);
        true
    }

    pub fn gl_bind_texture(&mut self, target: u32, name: u32) -> bool {
        if target != 0x0de1 {
            return false;
        } // GL_TEXTURE_2D
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context.textures.bind_2d(name);
        true
    }

    pub fn gl_tex_parameter_i(&mut self, target: u32, name: u32, value: i32) -> bool {
        if target != 0x0de1 {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        context
            .textures
            .bound_mut()
            .is_some_and(|texture| texture.set_parameter(name, value as u32))
    }

    pub fn gl_tex_image_2d(
        &mut self,
        memory: &mut PpcSectionMem,
        target: u32,
        level: i32,
        internal_format: i32,
        width: i32,
        height: i32,
        border: i32,
        format: u32,
        pixel_type: u32,
        pixels: u32,
    ) -> bool {
        if target != 0x0de1
            || !(0..=12).contains(&level)
            || border != 0
            || !matches!(
                internal_format,
                1..=4 | 0x1906 | 0x1907 | 0x1908 | 0x1909 | 0x190a
            )
            || !matches!(
                format,
                0x1906 | 0x1907 | 0x1908 | 0x1909 | 0x190a | 0x80e0 | 0x80e1
            )
            || pixel_type != 0x1401
        {
            return false;
        }
        let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
            return false;
        };
        let Some(mut image) = ClassicGlTextureImage::new(width, height, internal_format as u32)
        else {
            return false;
        };
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        if pixels != 0
            && !image.upload_sub_image(
                memory,
                (0, 0),
                (width, height),
                format,
                pixel_type,
                pixels,
                context.textures.unpack,
            )
        {
            return false;
        }
        let Some(texture) = context.textures.bound_mut() else {
            return false;
        };
        texture.images[level as usize] = Some(image);
        true
    }

    pub fn gl_tex_sub_image_2d(
        &mut self,
        memory: &mut PpcSectionMem,
        target: u32,
        level: i32,
        x_offset: i32,
        y_offset: i32,
        width: i32,
        height: i32,
        format: u32,
        pixel_type: u32,
        pixels: u32,
    ) -> bool {
        if target != 0x0de1 || !(0..=12).contains(&level) || pixels == 0 {
            return false;
        }
        let (Ok(x_offset), Ok(y_offset), Ok(width), Ok(height)) = (
            u32::try_from(x_offset),
            u32::try_from(y_offset),
            u32::try_from(width),
            u32::try_from(height),
        ) else {
            return false;
        };
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        if context.primitive_mode.is_some() {
            return false;
        }
        let unpack = context.textures.unpack;
        let Some(image) = context
            .textures
            .bound_mut()
            .and_then(|texture| texture.images[level as usize].as_mut())
        else {
            return false;
        };
        image.upload_sub_image(
            memory,
            (x_offset, y_offset),
            (width, height),
            format,
            pixel_type,
            pixels,
            unpack,
        )
    }

    pub fn gl_read_pixels(
        &self,
        memory: &mut PpcSectionMem,
        rect: (i32, i32, i32, i32),
        format: u32,
        pixel_type: u32,
        destination: u32,
    ) -> bool {
        const GL_UNSIGNED_BYTE: u32 = 0x1401;
        let components: usize = match format {
            0x1907 => 3, // GL_RGB
            0x1908 => 4, // GL_RGBA
            _ => return false,
        };
        if pixel_type != GL_UNSIGNED_BYTE || destination == 0 {
            return false;
        }
        let (x, y, width, height) = rect;
        let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
            u32::try_from(x),
            u32::try_from(y),
            u32::try_from(width),
            u32::try_from(height),
        ) else {
            return false;
        };
        let Some(context) = self.context(self.current_context) else {
            return false;
        };
        let Some(framebuffer) = context.framebuffer.as_ref() else {
            return false;
        };
        let Some(pixels) = framebuffer.read_rgba(context.read_buffer, x, y, width, height) else {
            return false;
        };
        let Some(stride) = (if context.pack.row_length == 0 {
            width
        } else {
            context.pack.row_length
        })
        .checked_mul(components as u32)
        .and_then(|length| length.checked_add(context.pack.alignment - 1))
        .map(|length| length & !(context.pack.alignment - 1)) else {
            return false;
        };
        let Some(start) = context
            .pack
            .skip_rows
            .checked_mul(stride)
            .and_then(|offset| {
                context
                    .pack
                    .skip_pixels
                    .checked_mul(components as u32)
                    .and_then(|pixels| offset.checked_add(pixels))
            })
            .and_then(|offset| destination.checked_add(offset))
        else {
            return false;
        };
        let Ok(row_len) = usize::try_from(width).map(|width| width * components) else {
            return false;
        };
        let mut row = vec![0; row_len];
        for row_index in 0..height {
            let Some(address) = row_index
                .checked_mul(stride)
                .and_then(|offset| start.checked_add(offset))
            else {
                return false;
            };
            let source = row_index as usize * width as usize * 4;
            for column in 0..width as usize {
                let source = source + column * 4;
                let target = column * components;
                row[target..target + components]
                    .copy_from_slice(&pixels[source..source + components]);
            }
            if memory.write_bytes(address, &row).is_none() {
                return false;
            }
        }
        true
    }

    pub fn gl_clear(&mut self, mask: u32) -> bool {
        const COLOR: u32 = 0x0000_4000;
        const DEPTH: u32 = 0x0000_0100;
        const STENCIL: u32 = 0x0000_0400;
        if mask & !(COLOR | DEPTH | STENCIL) != 0 {
            return false;
        }
        let Some(context) = self.context_mut(self.current_context) else {
            return false;
        };
        let Some(framebuffer) = context.framebuffer.as_mut() else {
            return false;
        };
        let clear_color = context
            .clear_color
            .map(|component| (component * 255.0).round() as u8);
        let color_requested = mask & COLOR != 0;
        let clear = ClassicGlClear {
            color: (color_requested && context.draw_front)
                .then_some((ClassicGlColorBuffer::Front, clear_color))
                .or_else(|| {
                    (color_requested && context.draw_back)
                        .then_some((ClassicGlColorBuffer::Back, clear_color))
                }),
            color_mask: context.color_mask,
            depth: (mask & DEPTH != 0).then_some(context.clear_depth as f32),
            depth_mask: context.depth_mask,
            stencil: (mask & STENCIL != 0).then_some(context.clear_stencil as u8),
            stencil_mask: context.stencil_mask as u8,
            scissor: context.scissor_enabled.then_some(context.scissor),
        };
        if !framebuffer.clear(clear) {
            return false;
        }
        if color_requested && context.draw_front && context.draw_back {
            framebuffer.clear(ClassicGlClear {
                color: Some((ClassicGlColorBuffer::Back, clear_color)),
                depth: None,
                stencil: None,
                ..clear
            })
        } else {
            true
        }
    }

    pub fn set_drawable(
        &mut self,
        handle: u32,
        drawable: u32,
        surface: Option<PpcFrontBuffer>,
    ) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if drawable == 0 {
            context.drawable = 0;
            context.framebuffer = None;
            return true;
        }
        let Some(surface) = surface.filter(|surface| surface.depth == 16) else {
            return false;
        };
        let Some(framebuffer) = ClassicGlFramebuffer::new(
            surface.width,
            surface.height,
            context.format.double_buffered,
        ) else {
            return false;
        };
        context.drawable = drawable;
        context.framebuffer = Some(framebuffer);
        if !context.scissor_explicit {
            context.scissor = (0, 0, surface.width, surface.height);
        }
        if !context.viewport_explicit {
            context.viewport = (0, 0, surface.width, surface.height);
        }
        true
    }

    pub fn update_context(&mut self, handle: u32, surface: Option<PpcFrontBuffer>) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if context.drawable == 0 {
            return true;
        }
        let Some(surface) = surface.filter(|surface| surface.depth == 16) else {
            return false;
        };
        let resize = context.framebuffer.as_ref().is_none_or(|framebuffer| {
            framebuffer.width() != surface.width || framebuffer.height() != surface.height
        });
        if resize {
            let Some(framebuffer) = ClassicGlFramebuffer::new(
                surface.width,
                surface.height,
                context.format.double_buffered,
            ) else {
                return false;
            };
            context.framebuffer = Some(framebuffer);
            if !context.scissor_explicit {
                context.scissor = (0, 0, surface.width, surface.height);
            }
            if !context.viewport_explicit {
                context.viewport = (0, 0, surface.width, surface.height);
            }
        }
        true
    }

    pub fn swap_buffers(
        &mut self,
        handle: u32,
        memory: &mut PpcSectionMem,
        surface: PpcFrontBuffer,
    ) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        let Some(framebuffer) = context.framebuffer.as_mut() else {
            return false;
        };
        let buffer = if context.format.double_buffered {
            ClassicGlColorBuffer::Back
        } else {
            ClassicGlColorBuffer::Front
        };
        if !framebuffer.present_rgb555(buffer, memory, surface) {
            return false;
        }
        if context.format.double_buffered {
            framebuffer.swap()
        } else {
            true
        }
    }
}

fn ppc_gl_read_array_components(
    memory: &mut PpcSectionMem,
    array: PpcGlArrayPointer,
    index: u32,
    normalized: bool,
) -> Option<[f64; 4]> {
    let component_width: u32 = match array.component_type {
        0x1400 | 0x1401 => 1,
        0x1402 | 0x1403 => 2,
        0x1404..=0x1406 => 4,
        0x140a => 8,
        _ => return None,
    };
    let stride = if array.stride == 0 {
        array.size.checked_mul(component_width)?
    } else {
        array.stride
    };
    let base = array.pointer.checked_add(index.checked_mul(stride)?)?;
    let mut values = [0.0, 0.0, 0.0, 1.0];
    for component in 0..array.size {
        let address = base.checked_add(component.checked_mul(component_width)?)?;
        values[component as usize] = match array.component_type {
            0x1400 => {
                let value = memory.read_u8(address)? as i8;
                if normalized {
                    (f64::from(value) / 127.0).max(-1.0)
                } else {
                    f64::from(value)
                }
            }
            0x1401 => {
                let value = memory.read_u8(address)?;
                if normalized {
                    f64::from(value) / 255.0
                } else {
                    f64::from(value)
                }
            }
            0x1402 => {
                let value = memory.read_u16_be(address)? as i16;
                if normalized {
                    (f64::from(value) / 32767.0).max(-1.0)
                } else {
                    f64::from(value)
                }
            }
            0x1403 => {
                let value = memory.read_u16_be(address)?;
                if normalized {
                    f64::from(value) / 65535.0
                } else {
                    f64::from(value)
                }
            }
            0x1404 => {
                let value = memory.read_u32_be(address)? as i32;
                if normalized {
                    (f64::from(value) / 2147483647.0).max(-1.0)
                } else {
                    f64::from(value)
                }
            }
            0x1405 => {
                let value = memory.read_u32_be(address)?;
                if normalized {
                    f64::from(value) / 4294967295.0
                } else {
                    f64::from(value)
                }
            }
            0x1406 => f64::from(f32::from_bits(memory.read_u32_be(address)?)),
            0x140a => f64::from_bits(memory.read_u64_be(address)?),
            _ => return None,
        };
    }
    Some(values)
}

fn ppc_agl_software_format_matches(request: &PpcAglPixelFormatRequest) -> bool {
    // AGL_ACCELERATED asks for hardware rendering. The guest-backed software
    // surface must never be advertised as a hardware renderer.
    if !request.rgba
        || request.accelerated
        || request.stereo
        || request.offscreen
        || request.backing_store
    {
        return false;
    }
    for (requested, available) in [
        (request.red_bits, 8),
        (request.green_bits, 8),
        (request.blue_bits, 8),
        (request.alpha_bits, 8),
        (request.depth_bits, 24),
        (request.stencil_bits, 8),
        (request.pixel_bits, 32),
        (request.aux_buffers, 0),
    ] {
        if requested.is_some_and(|bits| bits < 0 || bits > available) {
            return false;
        }
    }
    request
        .other_values
        .iter()
        .all(|&(attribute, value)| match attribute {
            3 | 14..=17 | 55..=57 => value == 0,
            70 => value == 0x0002_0200, // AGL_RENDERER_GENERIC_ID
            2 => value == 0,            // AGL_BUFFER_SIZE is for color-index formats
            _ => false,
        })
}

/// Reads a terminated AGL attribute list without crossing an unmapped guest
/// region or allowing an unbounded walk through guest memory.
pub fn ppc_agl_read_pixel_format_request(
    memory: &mut PpcSectionMem,
    address: u32,
) -> Result<PpcAglPixelFormatRequest, PpcAglAttributeError> {
    if address == 0 {
        return Err(PpcAglAttributeError::BadPointer);
    }
    let mut request = PpcAglPixelFormatRequest::default();
    let mut offset = 0;
    while offset < MAX_ATTRIBUTE_WORDS {
        let word_address = address
            .checked_add(
                offset
                    .checked_mul(4)
                    .ok_or(PpcAglAttributeError::BadPointer)?,
            )
            .ok_or(PpcAglAttributeError::BadPointer)?;
        let attribute = memory
            .read_u32_be(word_address)
            .ok_or(PpcAglAttributeError::BadPointer)? as i32;
        offset += 1;
        match attribute {
            0 => return Ok(request),              // AGL_NONE
            1 | 51 | 52 | 71 | 74 | 75 | 78 => {} // supported selection policies
            4 => request.rgba = true,
            5 => request.double_buffered = true,
            6 => request.stereo = true,
            53 => request.offscreen = true,
            54 => request.fullscreen = true,
            72 => request.no_recovery = true,
            73 => request.accelerated = true,
            76 => request.backing_store = true,
            2 | 3 | 7..=17 | 50 | 55..=57 | 70 => {
                if offset >= MAX_ATTRIBUTE_WORDS {
                    return Err(PpcAglAttributeError::Unterminated);
                }
                let value_address = address
                    .checked_add(
                        offset
                            .checked_mul(4)
                            .ok_or(PpcAglAttributeError::BadPointer)?,
                    )
                    .ok_or(PpcAglAttributeError::BadPointer)?;
                let value = memory
                    .read_u32_be(value_address)
                    .ok_or(PpcAglAttributeError::BadPointer)? as i32;
                offset += 1;
                match attribute {
                    7 => request.aux_buffers = Some(value),
                    8 => request.red_bits = Some(value),
                    9 => request.green_bits = Some(value),
                    10 => request.blue_bits = Some(value),
                    11 => request.alpha_bits = Some(value),
                    12 => request.depth_bits = Some(value),
                    13 => request.stencil_bits = Some(value),
                    50 => request.pixel_bits = Some(value),
                    _ => request.other_values.push((attribute, value)),
                }
            }
            _ => return Err(PpcAglAttributeError::UnsupportedAttribute(attribute)),
        }
    }
    Err(PpcAglAttributeError::Unterminated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::PpcMemory;

    #[test]
    fn immediate_triangle_reaches_guest_window_pixels_after_swap() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            width: 4,
            height: 4,
            depth: 16,
            row_bytes: 8,
        };
        assert!(agl.set_drawable(context, 0x2000, Some(surface)));
        assert!(!agl.gl_viewport(0, 0, -1, 4));
        assert!(agl.gl_color([1.0, 0.0, 0.0, 1.0]));
        assert!(agl.gl_begin(0x0004)); // GL_TRIANGLES
        assert!(agl.gl_vertex([-1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([-1.0, 1.0, 0.0, 1.0]));
        assert!(agl.gl_end());
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 32]);
        assert!(agl.swap_buffers(context, &mut memory, surface));
        assert_eq!(memory.read_u16_be(0x1000 + 3 * 8), Some(0x7c00));
        assert_eq!(memory.read_u16_be(0x1000 + 3 * 2), Some(0));
    }

    #[test]
    fn immediate_vertices_use_modelview_projection_before_rasterization() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x1000,
                width: 4,
                height: 4,
                depth: 16,
                row_bytes: 8,
            })
        ));
        agl.current_transform_mut()
            .unwrap()
            .translate(1.0, 0.0, 0.0);
        assert!(agl.gl_begin(0x0004));
        assert!(agl.gl_vertex([-1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([-1.0, 1.0, 0.0, 1.0]));
        assert!(agl.gl_end());
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0; 4])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 2, 0),
            Some([255; 4])
        );
    }

    #[test]
    fn guest_client_arrays_and_indices_draw_into_the_same_framebuffer() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x3000,
                width: 4,
                height: 4,
                depth: 16,
                row_bytes: 8,
            })
        ));
        let mut memory = PpcSectionMem::new();
        let vertices: [f32; 12] = [
            -1.0, -1.0, 0.0, 99.0, 1.0, -1.0, 0.0, 99.0, -1.0, 1.0, 0.0, 99.0,
        ];
        memory.add_region(
            0x1000,
            vertices
                .iter()
                .flat_map(|value| value.to_bits().to_be_bytes())
                .collect(),
        );
        memory.add_region(0x1100, vec![255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255]);
        memory.add_region(0x1200, vec![0, 0, 0, 1, 0, 2]);
        assert!(agl.gl_vertex_pointer(3, 0x1406, 16, 0x1000));
        assert!(agl.gl_color_pointer(4, 0x1401, 0, 0x1100));
        assert!(agl.gl_client_state(0x8074, true));
        assert!(agl.gl_client_state(0x8076, true));
        assert!(agl.gl_begin(0x0004));
        assert!(!agl.gl_vertex_pointer(3, 0x1406, 16, 0x1000));
        assert!(!agl.gl_client_state(0x8074, false));
        assert!(agl.gl_end());
        assert!(agl.gl_draw_arrays(&mut memory, 0x0004, 0, 3));
        assert_eq!(
            agl.context(context)
                .unwrap()
                .framebuffer
                .as_ref()
                .unwrap()
                .pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 0, 0, 255])
        );
        assert!(agl
            .context_mut(context)
            .unwrap()
            .framebuffer
            .as_mut()
            .unwrap()
            .clear_color(ClassicGlColorBuffer::Front, [0; 4]));
        assert!(agl.gl_draw_elements(&mut memory, 0x0004, 3, 0x1403, 0x1200));
        assert_eq!(
            agl.context(context)
                .unwrap()
                .framebuffer
                .as_ref()
                .unwrap()
                .pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 0, 0, 255])
        );
        assert!(!agl.gl_draw_elements(&mut memory, 0x0004, 3, 0x1403, 0x1300));
    }

    #[test]
    fn texture_uploads_use_guest_unpack_state_and_named_context_objects() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 4]);
        memory.add_region(
            0x2000,
            vec![255, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0],
        );
        memory.add_region(0x3000, vec![4, 5, 6, 7]);
        assert!(agl.gl_gen_textures(&mut memory, 1, 0x1000));
        assert_eq!(memory.read_u32_be(0x1000), Some(1));
        assert!(agl.gl_bind_texture(0x0de1, 1));
        assert!(agl.gl_tex_parameter_i(0x0de1, 0x2801, 0x2600)); // NEAREST minification
        assert!(agl.gl_tex_image_2d(
            &mut memory,
            0x0de1,
            0,
            0x1907,
            2,
            2,
            0,
            0x1907,
            0x1401,
            0x2000,
        ));
        let image = agl
            .context(context)
            .unwrap()
            .textures
            .bound()
            .unwrap()
            .images[0]
            .as_ref()
            .unwrap();
        assert_eq!(image.texel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(image.texel(1, 1), Some([255, 255, 255, 255]));
        assert!(!agl.gl_tex_sub_image_2d(
            &mut memory,
            0x0de1,
            0,
            1,
            1,
            1,
            1,
            0x1908,
            0x1401,
            0x4000
        ));
        assert!(agl.gl_tex_sub_image_2d(
            &mut memory,
            0x0de1,
            0,
            1,
            1,
            1,
            1,
            0x1908,
            0x1401,
            0x3000
        ));
        assert_eq!(
            agl.context(context)
                .unwrap()
                .textures
                .bound()
                .unwrap()
                .images[0]
                .as_ref()
                .unwrap()
                .texel(1, 1),
            Some([4, 5, 6, 7])
        );
        assert!(agl.gl_delete_textures(&mut memory, 1, 0x1000));
        assert_eq!(agl.context(context).unwrap().textures.bound_2d, 0);
    }

    #[test]
    fn enabled_texture_modulates_immediate_triangle_pixels() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x3000,
                width: 4,
                height: 4,
                depth: 16,
                row_bytes: 8,
            })
        ));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![255, 0, 0, 255]);
        assert!(agl.gl_tex_parameter_i(0x0de1, 0x2801, 0x2600));
        assert!(agl.gl_tex_parameter_i(0x0de1, 0x2800, 0x2600));
        assert!(agl.gl_tex_image_2d(
            &mut memory,
            0x0de1,
            0,
            0x1908,
            1,
            1,
            0,
            0x1908,
            0x1401,
            0x1000
        ));
        assert!(agl.gl_texture_2d(true));
        assert!(agl.gl_tex_coord([0.5, 0.5, 0.0, 1.0]));
        assert!(agl.gl_begin(0x0004));
        assert!(agl.gl_vertex([-1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([-1.0, 1.0, 0.0, 1.0]));
        assert!(agl.gl_end());
        assert_eq!(
            agl.context(context)
                .unwrap()
                .framebuffer
                .as_ref()
                .unwrap()
                .pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 0, 0, 255])
        );
    }

    #[test]
    fn minified_triangle_samples_mipmap_level() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x3000,
                width: 4,
                height: 4,
                depth: 16,
                row_bytes: 8,
            })
        ));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, [255, 0, 0, 255].repeat(4));
        memory.add_region(0x1100, vec![0, 255, 0, 255]);
        assert!(agl.gl_tex_image_2d(
            &mut memory,
            0x0de1,
            0,
            0x1908,
            2,
            2,
            0,
            0x1908,
            0x1401,
            0x1000
        ));
        assert!(agl.gl_tex_image_2d(
            &mut memory,
            0x0de1,
            1,
            0x1908,
            1,
            1,
            0,
            0x1908,
            0x1401,
            0x1100
        ));
        assert!(agl.gl_texture_2d(true));
        assert!(agl.gl_begin(0x0004));
        assert!(agl.gl_tex_coord([0.0, 0.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([-1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_tex_coord([8.0, 0.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([1.0, -1.0, 0.0, 1.0]));
        assert!(agl.gl_tex_coord([0.0, 8.0, 0.0, 1.0]));
        assert!(agl.gl_vertex([-1.0, 1.0, 0.0, 1.0]));
        assert!(agl.gl_end());
        assert_eq!(
            agl.context(context)
                .unwrap()
                .framebuffer
                .as_ref()
                .unwrap()
                .pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0, 255, 0, 255])
        );
    }

    #[test]
    fn clear_uses_current_context_back_buffer_and_default_depth_stencil() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(!agl.gl_clear(0x4000));
        assert!(agl.set_current_context(context));
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            width: 1,
            height: 1,
            depth: 16,
            row_bytes: 2,
        };
        assert!(agl.set_drawable(context, 0x2000, Some(surface)));
        assert!(agl.gl_clear_color([1.0, 0.5, -1.0, 2.0]));
        assert!(agl.gl_clear(0x4000 | 0x0100 | 0x0400));
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Back, 0, 0),
            Some([255, 128, 0, 255])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0; 4])
        );
        assert_eq!(framebuffer.depth_at(0, 0), Some(1.0));
        assert_eq!(framebuffer.stencil_at(0, 0), Some(0));
        assert!(!agl.gl_clear(0x8000_0000));
    }

    #[test]
    fn clear_honors_scissor_and_write_masks() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x1000,
                width: 2,
                height: 2,
                depth: 16,
                row_bytes: 4,
            })
        ));
        assert!(agl.gl_clear_color([0.25, 0.5, 0.75, 1.0]));
        assert!(agl.gl_clear_depth(0.25));
        assert!(agl.gl_clear_stencil(0xab));
        assert!(agl.gl_color_mask([true, false, true, false]));
        assert!(agl.gl_depth_mask(false));
        assert!(agl.gl_stencil_mask(0x0f));
        assert!(!agl.gl_scissor(0, 0, -1, 1));
        assert!(agl.gl_scissor(1, 0, 1, 1));
        assert!(agl.gl_scissor_test(true));
        assert!(agl.gl_clear(0x4000 | 0x0100 | 0x0400));
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 1, 0),
            Some([64, 0, 191, 0])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0; 4])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 1, 1),
            Some([0; 4])
        );
        assert_eq!(framebuffer.depth_at(1, 0), Some(1.0));
        assert_eq!(framebuffer.stencil_at(1, 0), Some(0x0b));
        assert!(agl.gl_scissor_test(false));
        assert!(agl.gl_depth_mask(true));
        assert!(agl.gl_clear(0x0100));
        assert_eq!(
            agl.context(context)
                .unwrap()
                .framebuffer
                .as_ref()
                .unwrap()
                .depth_at(0, 0),
            Some(0.25)
        );
    }

    #[test]
    fn read_pixels_packs_bottom_up_rgb_rows_into_guest_memory() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x2000,
                width: 2,
                height: 2,
                depth: 16,
                row_bytes: 4,
            })
        ));
        let framebuffer = agl
            .context_mut(context)
            .unwrap()
            .framebuffer
            .as_mut()
            .unwrap();
        assert!(framebuffer.set_pixel(ClassicGlColorBuffer::Back, 0, 0, [255, 0, 0, 255]));
        assert!(framebuffer.set_pixel(ClassicGlColorBuffer::Back, 1, 0, [0, 255, 0, 255]));
        assert!(framebuffer.set_pixel(ClassicGlColorBuffer::Back, 0, 1, [0, 0, 255, 255]));
        assert!(framebuffer.set_pixel(ClassicGlColorBuffer::Back, 1, 1, [255, 255, 255, 255]));
        assert!(agl.gl_pixel_store_i(0x0d02, 3)); // PACK_ROW_LENGTH
        assert!(agl.gl_pixel_store_i(0x0d03, 1)); // PACK_SKIP_ROWS
        assert!(agl.gl_pixel_store_i(0x0d04, 1)); // PACK_SKIP_PIXELS
        assert!(!agl.gl_pixel_store_i(0x0d05, 3)); // Invalid alignment
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0xee; 64]);
        assert!(agl.gl_read_pixels(&mut memory, (0, 0, 2, 2), 0x1907, 0x1401, 0x1000));
        let bytes = (0..64)
            .map(|offset| memory.read_u8(0x1000 + offset).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(&bytes[15..21], &[255, 0, 0, 0, 255, 0]);
        assert_eq!(&bytes[27..33], &[0, 0, 255, 255, 255, 255]);
        assert_eq!(bytes[21], 0xee);
        assert_eq!(bytes[26], 0xee);
        assert!(!agl.gl_read_pixels(&mut memory, (0, 0, 2, 2), 0x1907, 0x1403, 0x1000));
        assert!(agl.gl_read_buffer(0x0404)); // FRONT
        assert!(!agl.gl_read_buffer(0x0408)); // FRONT_AND_BACK is invalid for reads
    }

    #[test]
    fn draw_buffer_routes_clear_to_front_back_both_or_neither() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert!(agl.set_current_context(context));
        assert!(agl.set_drawable(
            context,
            0x2000,
            Some(PpcFrontBuffer {
                base_addr: 0x1000,
                width: 1,
                height: 1,
                depth: 16,
                row_bytes: 2,
            })
        ));
        assert!(agl.gl_clear_color([1.0, 0.0, 0.0, 1.0]));
        assert!(agl.gl_draw_buffer(0x0404)); // FRONT
        assert!(agl.gl_clear(0x4000));
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Back, 0, 0),
            Some([0; 4])
        );
        assert!(agl.gl_draw_buffer(0x0408)); // FRONT_AND_BACK
        assert!(agl.gl_clear_color([0.0, 1.0, 0.0, 1.0]));
        assert!(agl.gl_clear(0x4000));
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0, 255, 0, 255])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Back, 0, 0),
            Some([0, 255, 0, 255])
        );
        assert!(agl.gl_draw_buffer(0)); // NONE
        assert!(agl.gl_clear_color([0.0, 0.0, 1.0, 1.0]));
        assert!(agl.gl_clear(0x4000));
        let framebuffer = agl.context(context).unwrap().framebuffer.as_ref().unwrap();
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0, 255, 0, 255])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Back, 0, 0),
            Some([0, 255, 0, 255])
        );
        assert!(!agl.gl_draw_buffer(0x0401)); // FRONT_RIGHT needs stereo
    }

    #[test]
    fn reads_boolean_and_value_attributes_from_guest_memory() {
        let mut memory = PpcSectionMem::new();
        let words: [u32; 8] = [4, 5, 12, 24, 13, 8, 73, 0];
        memory.add_region(
            0x1000,
            words.iter().flat_map(|word| word.to_be_bytes()).collect(),
        );
        let request = ppc_agl_read_pixel_format_request(&mut memory, 0x1000).unwrap();
        assert!(request.rgba && request.double_buffered && request.accelerated);
        assert_eq!(request.depth_bits, Some(24));
        assert_eq!(request.stencil_bits, Some(8));
        assert_eq!(request.alpha_bits, None);
    }

    #[test]
    fn rejects_unmapped_and_unterminated_attribute_lists() {
        let mut memory = PpcSectionMem::new();
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0),
            Err(PpcAglAttributeError::BadPointer)
        );
        memory.add_region(0x1000, 12u32.to_be_bytes().to_vec());
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x1000),
            Err(PpcAglAttributeError::BadPointer)
        );
        memory.add_region(0x2000, vec![0, 0, 0, 4].repeat(64));
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x2000),
            Err(PpcAglAttributeError::Unterminated)
        );
    }

    #[test]
    fn rejects_unknown_attributes_instead_of_misreading_following_words() {
        let mut memory = PpcSectionMem::new();
        memory.add_region(
            0x1000,
            [99u32, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x1000),
            Err(PpcAglAttributeError::UnsupportedAttribute(99))
        );
    }

    #[test]
    fn software_format_selection_tracks_opaque_lifetime_and_declines_acceleration() {
        let mut state = PpcAglState::default();
        let request = PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            depth_bits: Some(24),
            stencil_bits: Some(8),
            ..Default::default()
        };
        let handle = state.choose_pixel_format(request.clone());
        assert_ne!(handle, 0);
        assert_eq!(state.pixel_format(handle).unwrap().request, request);
        state.destroy_pixel_format(handle);
        assert!(state.pixel_format(handle).is_none());
        let accelerated = PpcAglPixelFormatRequest {
            accelerated: true,
            ..request
        };
        assert_eq!(state.choose_pixel_format(accelerated), 0);
    }

    #[test]
    fn description_reports_actual_software_format_properties() {
        let mut state = PpcAglState::default();
        let handle = state.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        assert_eq!(state.describe_pixel_format(handle, 5), Some(1));
        assert_eq!(state.describe_pixel_format(handle, 12), Some(24));
        assert_eq!(state.describe_pixel_format(handle, 73), Some(0));
        assert_eq!(state.describe_pixel_format(handle, 999), None);
        state.destroy_pixel_format(handle);
        assert_eq!(state.describe_pixel_format(handle, 5), None);
    }

    #[test]
    fn context_keeps_format_after_disposal_and_swaps_to_guest_pixels() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert_ne!(context, 0);
        agl.destroy_pixel_format(format);
        assert!(agl.set_current_context(context));
        assert_eq!(agl.current_context(), context);
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 2,
            width: 1,
            height: 1,
            depth: 16,
        };
        assert!(agl.set_drawable(context, 0x2000, Some(surface)));
        assert!(agl
            .context_mut(context)
            .unwrap()
            .framebuffer
            .as_mut()
            .unwrap()
            .clear_color(ClassicGlColorBuffer::Back, [255, 0, 0, 255]));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 2]);
        assert!(agl.swap_buffers(context, &mut memory, surface));
        assert_eq!(memory.read_u16_be(0x1000), Some(0x7c00));
        assert!(agl.destroy_context(context));
        assert_eq!(agl.current_context(), 0);
    }
}
