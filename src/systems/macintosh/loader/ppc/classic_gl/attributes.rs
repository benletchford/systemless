//! Attribute groups from OpenGL 1.2.1 section 6.1 and its state tables.

use super::consts::*;
use super::state::{ClientState, ContextState, Current, Raster};
use super::texture::Texture;

#[derive(Debug, Clone)]
pub struct ServerSnapshot {
    mask: u32,
    raster: Raster,
    current: Current,
    textures: Vec<(u32, TextureParameters)>,
}

/// Texture images are not attribute state. Save only the bound objects' parameters.
#[derive(Debug, Clone)]
struct TextureParameters {
    min_filter: u32,
    mag_filter: u32,
    wrap_s: u32,
    wrap_t: u32,
    border_color: [f32; 4],
    priority: f32,
    min_lod: f32,
    max_lod: f32,
    base_level: u32,
    max_level: u32,
}

impl TextureParameters {
    fn capture(texture: &Texture) -> Self {
        Self {
            min_filter: texture.min_filter,
            mag_filter: texture.mag_filter,
            wrap_s: texture.wrap_s,
            wrap_t: texture.wrap_t,
            border_color: texture.border_color,
            priority: texture.priority,
            min_lod: texture.min_lod,
            max_lod: texture.max_lod,
            base_level: texture.base_level,
            max_level: texture.max_level,
        }
    }

    fn restore(self, texture: &mut Texture) {
        texture.min_filter = self.min_filter;
        texture.mag_filter = self.mag_filter;
        texture.wrap_s = self.wrap_s;
        texture.wrap_t = self.wrap_t;
        texture.border_color = self.border_color;
        texture.priority = self.priority;
        texture.min_lod = self.min_lod;
        texture.max_lod = self.max_lod;
        texture.base_level = self.base_level;
        texture.max_level = self.max_level;
    }
}

impl ServerSnapshot {
    fn capture(state: &ContextState, mask: u32) -> Self {
        let mut textures = Vec::new();
        if mask & TEXTURE_BIT != 0 {
            for unit in &state.raster.units {
                let name = unit.binding_2d;
                if textures.iter().any(|(saved, _)| *saved == name) {
                    continue;
                }
                let texture = if name == 0 {
                    Some(&state.default_texture)
                } else {
                    state.textures.get(&name)
                };
                if let Some(texture) = texture {
                    textures.push((name, TextureParameters::capture(texture)));
                }
            }
        }
        Self {
            mask,
            raster: state.raster.clone(),
            current: state.current,
            textures,
        }
    }

    fn restore(self, state: &mut ContextState) {
        let mask = self.mask;
        let saved = self.raster;
        let current = &mut state.raster;
        macro_rules! fields {
            ($bit:ident; $($field:ident),+ $(,)?) => {
                if mask & $bit != 0 { $(current.$field = saved.$field;)+ }
            };
        }
        macro_rules! enables {
            ($bit:ident; $($field:ident),+ $(,)?) => {
                if mask & $bit != 0 { $(current.enables.$field = saved.enables.$field;)+ }
            };
        }
        if mask & CURRENT_BIT != 0 {
            state.current = self.current;
            current.position = saved.position;
        }
        fields!(POINT_BIT; point_size, point_size_min, point_size_max, point_attenuation);
        enables!(POINT_BIT; point_smooth);
        fields!(LINE_BIT; line_width, line_stipple);
        enables!(LINE_BIT; line_smooth, line_stipple);
        fields!(POLYGON_BIT; cull_face_mode, front_face, polygon_mode, polygon_offset);
        enables!(POLYGON_BIT; cull_face, polygon_smooth, polygon_stipple, polygon_offset_fill, polygon_offset_line, polygon_offset_point);
        fields!(PIXEL_MODE_BIT; transfer, read_buffer);
        fields!(LIGHTING_BIT; shade_model, lights, light_model_ambient, light_model_local_viewer, light_model_two_side, separate_specular, materials, color_material_face, color_material_mode);
        enables!(LIGHTING_BIT; lighting, color_material);
        fields!(FOG_BIT; fog_mode, fog_density, fog_start, fog_end, fog_color);
        enables!(FOG_BIT; fog);
        fields!(DEPTH_BUFFER_BIT; depth_func, depth_mask, depth_clear);
        enables!(DEPTH_BUFFER_BIT; depth_test);
        fields!(STENCIL_BUFFER_BIT; stencil_func, stencil_ref, stencil_value_mask, stencil_write_mask, stencil_fail, stencil_zfail, stencil_zpass, stencil_clear);
        enables!(STENCIL_BUFFER_BIT; stencil_test);
        fields!(VIEWPORT_BIT; viewport, depth_range);
        fields!(TRANSFORM_BIT; matrix_mode, clip_planes);
        enables!(TRANSFORM_BIT; normalize, rescale_normal, clip_planes);
        fields!(COLOR_BUFFER_BIT; alpha_func, alpha_ref, blend_src, blend_dst, blend_color, color_mask, clear_color, draw_buffer);
        enables!(COLOR_BUFFER_BIT; alpha_test, blend, dither, color_logic_op);
        fields!(HINT_BIT; hints);
        enables!(EVAL_BIT; auto_normal);
        fields!(LIST_BIT; list_base);
        fields!(TEXTURE_BIT; units, active_unit);
        fields!(SCISSOR_BIT; scissor);
        enables!(SCISSOR_BIT; scissor_test);
        // Enable state also belongs to the individual groups above. ENABLE_BIT
        // additionally covers lights and texture generators stored outside Enables.
        if mask & ENABLE_BIT != 0 {
            current.enables = saved.enables;
            for (light, saved) in current.lights.iter_mut().zip(saved.lights) {
                light.enabled = saved.enabled;
            }
            for (unit, saved) in current.units.iter_mut().zip(saved.units) {
                unit.enabled_1d = saved.enabled_1d;
                unit.enabled_2d = saved.enabled_2d;
                for (gen, saved) in unit.gen.iter_mut().zip(saved.gen) {
                    gen.enabled = saved.enabled;
                }
            }
        }
        for (name, parameters) in self.textures {
            let texture = if name == 0 {
                &mut state.default_texture
            } else {
                state.textures.entry(name).or_default()
            };
            parameters.restore(texture);
        }
    }
}

impl ContextState {
    pub fn push_attributes(&mut self, mask: u32) {
        if mask & !ALL_ATTRIB_BITS != 0 {
            self.set_error(INVALID_VALUE);
        } else if self.attrib_stack.len() >= 16 {
            self.set_error(STACK_OVERFLOW);
        } else {
            self.attrib_stack.push(ServerSnapshot::capture(self, mask));
        }
    }

    pub fn pop_attributes(&mut self) {
        match self.attrib_stack.pop() {
            Some(saved) => saved.restore(self),
            None => self.set_error(STACK_UNDERFLOW),
        }
    }

    pub fn push_client_attributes(&mut self, mask: u32) {
        if mask != CLIENT_ALL_ATTRIB_BITS
            && mask & !(CLIENT_PIXEL_STORE_BIT | CLIENT_VERTEX_ARRAY_BIT) != 0
        {
            self.set_error(INVALID_VALUE);
        } else if self.client_attrib_stack.len() >= 16 {
            self.set_error(STACK_OVERFLOW);
        } else {
            self.client_attrib_stack.push((mask, self.client.clone()));
        }
    }

    pub fn pop_client_attributes(&mut self) {
        let Some((mask, saved)) = self.client_attrib_stack.pop() else {
            self.set_error(STACK_UNDERFLOW);
            return;
        };
        if mask & CLIENT_PIXEL_STORE_BIT != 0 {
            self.client.pack = saved.pack;
            self.client.unpack = saved.unpack;
        }
        if mask & CLIENT_VERTEX_ARRAY_BIT != 0 {
            let pack = self.client.pack;
            let unpack = self.client.unpack;
            self.client = ClientState {
                pack,
                unpack,
                ..saved
            };
        }
    }
}
