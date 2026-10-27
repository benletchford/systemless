//! Rasterization and per-fragment operations (OpenGL 1.2.1 specification,
//! sections 3.3-3.10 and chapter 4).

use super::consts::*;
use super::state::{ContextState, TexEnv, TEXTURE_UNITS};
use super::texture::{BaseFormat, Texture};

/// Window-space vertex. `x`/`y` use OpenGL's lower-left origin.
#[derive(Debug, Clone, Copy)]
pub struct WindowVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub inv_w: f32,
    pub color: [f32; 4],
    pub specular: [f32; 3],
    pub tex: [[f32; 4]; TEXTURE_UNITS],
    pub fog: f32,
    pub point_size: f32,
    pub edge: bool,
}

/// The buffers fragments are written to.
pub struct RasterTarget<'a> {
    pub width: usize,
    pub height: usize,
    pub color: &'a mut [[u8; 4]],
    pub depth: &'a mut [f32],
    pub stencil: &'a mut [u8],
}

struct ActiveUnit<'a> {
    texture: &'a Texture,
    min_filter: u32,
    env: TexEnv,
    width: f32,
    height: f32,
    /// Whether sampling depends on the level of detail. Without mipmaps and
    /// with the same nearest/linear choice for minification and
    /// magnification, every lambda samples the base level identically.
    needs_lod: bool,
}

fn active_units(state: &ContextState) -> Vec<(usize, ActiveUnit<'_>)> {
    (0..TEXTURE_UNITS)
        .filter(|&unit| state.raster.units[unit].enabled_2d)
        .filter_map(|unit| {
            let texture = state.bound_texture(unit);
            let Some(min_filter) = texture.effective_min_filter() else {
                if super::gl_trace_enabled() {
                    eprintln!(
                        "[GL] unit {unit} texture {} has no base image; texturing disabled",
                        state.raster.units[unit].binding_2d
                    );
                }
                return None;
            };
            let (width, height) = texture.base_size();
            let needs_lod = !matches!(min_filter, NEAREST | LINEAR)
                || (min_filter == LINEAR) != (texture.mag_filter == LINEAR);
            Some((
                unit,
                ActiveUnit {
                    texture,
                    min_filter,
                    env: state.raster.units[unit].env,
                    width: width as f32,
                    height: height as f32,
                    needs_lod,
                },
            ))
        })
        .collect()
}

/// Polygon offset (OpenGL 1.2.1, section 3.5.5) for a 24-bit depth buffer.
pub fn polygon_offset(state: &ContextState, vertices: &[WindowVertex], enabled: bool) -> f32 {
    if !enabled || vertices.len() < 3 {
        return 0.0;
    }
    let [factor, units] = state.raster.polygon_offset;
    let (a, b, c) = (&vertices[0], &vertices[1], &vertices[2]);
    let det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
    let slope = if det == 0.0 {
        0.0
    } else {
        let dzdx = ((b.z - a.z) * (c.y - a.y) - (c.z - a.z) * (b.y - a.y)) / det;
        let dzdy = ((c.z - a.z) * (b.x - a.x) - (b.z - a.z) * (c.x - a.x)) / det;
        dzdx.abs().max(dzdy.abs())
    };
    factor * slope + units * (1.0 / 16_777_216.0)
}

fn compare(func: u32, incoming: f32, stored: f32) -> bool {
    match func {
        NEVER => false,
        LESS => incoming < stored,
        EQUAL => incoming == stored,
        LEQUAL => incoming <= stored,
        GREATER => incoming > stored,
        NOTEQUAL => incoming != stored,
        GEQUAL => incoming >= stored,
        _ => true,
    }
}

fn stencil_op(op: u32, value: u8, reference: u8) -> u8 {
    match op {
        ZERO => 0,
        REPLACE => reference,
        INCR => value.saturating_add(1),
        DECR => value.saturating_sub(1),
        INVERT => !value,
        _ => value,
    }
}

fn blend_factor(factor: u32, src: [f32; 4], dst: [f32; 4], constant: [f32; 4]) -> [f32; 4] {
    match factor {
        ZERO => [0.0; 4],
        ONE => [1.0; 4],
        SRC_COLOR => src,
        ONE_MINUS_SRC_COLOR => src.map(|c| 1.0 - c),
        DST_COLOR => dst,
        ONE_MINUS_DST_COLOR => dst.map(|c| 1.0 - c),
        SRC_ALPHA => [src[3]; 4],
        ONE_MINUS_SRC_ALPHA => [1.0 - src[3]; 4],
        DST_ALPHA => [dst[3]; 4],
        ONE_MINUS_DST_ALPHA => [1.0 - dst[3]; 4],
        SRC_ALPHA_SATURATE => {
            let f = src[3].min(1.0 - dst[3]);
            [f, f, f, 1.0]
        }
        CONSTANT_COLOR => constant,
        ONE_MINUS_CONSTANT_COLOR => constant.map(|c| 1.0 - c),
        CONSTANT_ALPHA => [constant[3]; 4],
        ONE_MINUS_CONSTANT_ALPHA => [1.0 - constant[3]; 4],
        _ => [1.0; 4],
    }
}

fn combine_operand(operand: u32, value: [f32; 4], rgb: bool) -> [f32; 3] {
    match operand {
        ONE_MINUS_SRC_COLOR if rgb => [1.0 - value[0], 1.0 - value[1], 1.0 - value[2]],
        SRC_ALPHA => [value[3]; 3],
        ONE_MINUS_SRC_ALPHA => [1.0 - value[3]; 3],
        _ if rgb => [value[0], value[1], value[2]],
        _ => [value[3]; 3],
    }
}

fn combine_function(function: u32, a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| match function {
        REPLACE => a[i],
        ADD => a[i] + b[i],
        ADD_SIGNED => a[i] + b[i] - 0.5,
        INTERPOLATE => a[i] * c[i] + b[i] * (1.0 - c[i]),
        SUBTRACT => a[i] - b[i],
        _ => a[i] * b[i],
    })
}

/// Texture environment (OpenGL 1.2.1 table 3.22, ARB_texture_env_add and
/// ARB_texture_env_combine).
fn apply_env(
    env: &TexEnv,
    base: BaseFormat,
    previous: [f32; 4],
    texel: [f32; 4],
    primary: [f32; 4],
) -> [f32; 4] {
    let [cf0, cf1, cf2, af] = previous;
    let cf = [cf0, cf1, cf2];
    let ct = [texel[0], texel[1], texel[2]];
    let at = texel[3];
    let cc = [env.color[0], env.color[1], env.color[2]];
    let ac = env.color[3];
    let rgb = |f: &dyn Fn(usize) -> f32| [f(0), f(1), f(2)];
    let (out_rgb, out_a) = match env.mode {
        REPLACE => match base {
            BaseFormat::Alpha => (cf, at),
            BaseFormat::Luminance | BaseFormat::Rgb => (ct, af),
            _ => (ct, at),
        },
        DECAL => match base {
            BaseFormat::Rgb => (ct, af),
            BaseFormat::Rgba => (rgb(&|i| cf[i] * (1.0 - at) + ct[i] * at), af),
            _ => (cf, af),
        },
        BLEND => match base {
            BaseFormat::Alpha => (cf, af * at),
            BaseFormat::Intensity => (
                rgb(&|i| cf[i] * (1.0 - ct[i]) + cc[i] * ct[i]),
                af * (1.0 - at) + ac * at,
            ),
            BaseFormat::Luminance | BaseFormat::Rgb => {
                (rgb(&|i| cf[i] * (1.0 - ct[i]) + cc[i] * ct[i]), af)
            }
            _ => (rgb(&|i| cf[i] * (1.0 - ct[i]) + cc[i] * ct[i]), af * at),
        },
        ADD => match base {
            BaseFormat::Alpha => (cf, af * at),
            BaseFormat::Intensity => (rgb(&|i| cf[i] + ct[i]), af + at),
            BaseFormat::Luminance | BaseFormat::Rgb => (rgb(&|i| cf[i] + ct[i]), af),
            _ => (rgb(&|i| cf[i] + ct[i]), af * at),
        },
        COMBINE => {
            let source = |selector: u32| match selector {
                TEXTURE => texel,
                CONSTANT => env.color,
                PRIMARY_COLOR => primary,
                _ => previous,
            };
            let rgb_args: [[f32; 3]; 3] = std::array::from_fn(|n| {
                combine_operand(env.operand_rgb[n], source(env.source_rgb[n]), true)
            });
            let alpha_args: [[f32; 3]; 3] = std::array::from_fn(|n| {
                combine_operand(env.operand_alpha[n], source(env.source_alpha[n]), false)
            });
            let out_rgb = combine_function(env.combine_rgb, rgb_args[0], rgb_args[1], rgb_args[2]);
            let out_alpha = combine_function(
                env.combine_alpha,
                alpha_args[0],
                alpha_args[1],
                alpha_args[2],
            );
            (
                out_rgb.map(|c| c * env.rgb_scale),
                out_alpha[0] * env.alpha_scale,
            )
        }
        _ => match base {
            // MODULATE
            BaseFormat::Alpha => (cf, af * at),
            BaseFormat::Luminance | BaseFormat::Rgb => (rgb(&|i| cf[i] * ct[i]), af),
            _ => (rgb(&|i| cf[i] * ct[i]), af * at),
        },
    };
    [
        out_rgb[0].clamp(0.0, 1.0),
        out_rgb[1].clamp(0.0, 1.0),
        out_rgb[2].clamp(0.0, 1.0),
        out_a.clamp(0.0, 1.0),
    ]
}

/// Inputs to the fragment pipeline after interpolation.
struct Fragment {
    x: usize,
    y: usize,
    z: f32,
    color: [f32; 4],
    specular: [f32; 3],
    texcoords: [[f32; 2]; TEXTURE_UNITS],
    lambda: [f32; TEXTURE_UNITS],
    fog: f32,
}

fn fog_factor(state: &ContextState, distance: f32) -> f32 {
    let raster = &state.raster;
    let f = match raster.fog_mode {
        LINEAR => {
            let range = raster.fog_end - raster.fog_start;
            if range == 0.0 {
                1.0
            } else {
                (raster.fog_end - distance) / range
            }
        }
        EXP2 => {
            let d = raster.fog_density * distance;
            (-(d * d)).exp()
        }
        _ => (-(raster.fog_density * distance)).exp(),
    };
    f.clamp(0.0, 1.0)
}

fn process_fragment(
    state: &ContextState,
    target: &mut RasterTarget,
    units: &[(usize, ActiveUnit)],
    fragment: Fragment,
) {
    let raster = &state.raster;
    if fragment.x >= target.width || fragment.y >= target.height {
        return;
    }
    if raster.enables.scissor_test {
        let [sx, sy, sw, sh] = raster.scissor;
        let (x, y) = (fragment.x as i32, fragment.y as i32);
        if x < sx || y < sy || x >= sx + sw || y >= sy + sh {
            return;
        }
    }
    let primary = fragment.color;
    let mut color = primary;
    for (unit, active) in units {
        let [s, t] = fragment.texcoords[*unit];
        let texel = active
            .texture
            .sample(s, t, fragment.lambda[*unit], active.min_filter);
        color = apply_env(
            &active.env,
            active.texture.base_format,
            color,
            texel,
            primary,
        );
    }
    if raster.separate_specular && raster.enables.lighting {
        for c in 0..3 {
            color[c] = (color[c] + fragment.specular[c]).min(1.0);
        }
    }
    if raster.enables.fog {
        let f = fog_factor(state, fragment.fog);
        for c in 0..3 {
            color[c] = f * color[c] + (1.0 - f) * raster.fog_color[c];
        }
    }
    if raster.enables.alpha_test && !compare(raster.alpha_func, color[3], raster.alpha_ref) {
        return;
    }
    let index = fragment.y * target.width + fragment.x;
    if raster.enables.stencil_test {
        let reference = (raster.stencil_ref.clamp(0, 255) as u32 & raster.stencil_value_mask) as u8;
        let stored = target.stencil[index];
        let masked = (u32::from(stored) & raster.stencil_value_mask) as u8;
        let write_mask = raster.stencil_write_mask as u8;
        let update = |op: u32, stencil: &mut u8| {
            let next = stencil_op(op, *stencil, raster.stencil_ref.clamp(0, 255) as u8);
            *stencil = (*stencil & !write_mask) | (next & write_mask);
        };
        if !compare(raster.stencil_func, f32::from(reference), f32::from(masked)) {
            update(raster.stencil_fail, &mut target.stencil[index]);
            return;
        }
        let depth_pass = !raster.enables.depth_test
            || compare(raster.depth_func, fragment.z, target.depth[index]);
        let op = if depth_pass {
            raster.stencil_zpass
        } else {
            raster.stencil_zfail
        };
        update(op, &mut target.stencil[index]);
        if !depth_pass {
            return;
        }
    } else if raster.enables.depth_test
        && !compare(raster.depth_func, fragment.z, target.depth[index])
    {
        return;
    }
    if raster.enables.depth_test && raster.depth_mask {
        target.depth[index] = fragment.z;
    }
    let destination = target.color[index];
    let dst = [
        f32::from(destination[0]) / 255.0,
        f32::from(destination[1]) / 255.0,
        f32::from(destination[2]) / 255.0,
        f32::from(destination[3]) / 255.0,
    ];
    if raster.enables.blend {
        let sf = blend_factor(raster.blend_src, color, dst, raster.blend_color);
        let df = blend_factor(raster.blend_dst, color, dst, raster.blend_color);
        for c in 0..4 {
            color[c] = (color[c] * sf[c] + dst[c] * df[c]).clamp(0.0, 1.0);
        }
    }
    let pixel = &mut target.color[index];
    for c in 0..4 {
        if raster.color_mask[c] {
            pixel[c] = (color[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[inline]
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Fill convention for counter-clockwise triangles in OpenGL's y-up window
/// space: a pixel centre exactly on an edge belongs to the triangle only for
/// edges heading down (left edges) or horizontally left (top edges), so
/// triangles sharing an edge never both write its pixels (OpenGL 1.2.1,
/// section 3.5.1).
#[inline]
fn owns_edge(ax: f32, ay: f32, bx: f32, by: f32) -> bool {
    by < ay || (by == ay && bx < ax)
}

fn bounds(
    state: &ContextState,
    target: &RasterTarget,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
) -> Option<(usize, usize, usize, usize)> {
    let mut x0 = min_x.floor().max(0.0) as i64;
    let mut y0 = min_y.floor().max(0.0) as i64;
    let mut x1 = (max_x.ceil() as i64).min(target.width as i64 - 1);
    let mut y1 = (max_y.ceil() as i64).min(target.height as i64 - 1);
    if state.raster.enables.scissor_test {
        let [sx, sy, sw, sh] = state.raster.scissor;
        x0 = x0.max(i64::from(sx));
        y0 = y0.max(i64::from(sy));
        x1 = x1.min(i64::from(sx) + i64::from(sw) - 1);
        y1 = y1.min(i64::from(sy) + i64::from(sh) - 1);
    }
    (x0 <= x1 && y0 <= y1).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

pub fn draw_triangle(
    state: &ContextState,
    target: &mut RasterTarget,
    v0: &WindowVertex,
    v1: &WindowVertex,
    v2: &WindowVertex,
    depth_offset: f32,
) {
    // Orient counter-clockwise so inside points have positive edge values.
    let area = edge(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if area == 0.0 || !area.is_finite() {
        return;
    }
    let (a, b, c) = if area > 0.0 {
        (v0, v1, v2)
    } else {
        (v0, v2, v1)
    };
    let area = area.abs();
    let Some((x0, y0, x1, y1)) = bounds(
        state,
        target,
        a.x.min(b.x).min(c.x),
        a.x.max(b.x).max(c.x),
        a.y.min(b.y).min(c.y),
        a.y.max(b.y).max(c.y),
    ) else {
        return;
    };
    let units = active_units(state);
    let own0 = owns_edge(b.x, b.y, c.x, c.y);
    let own1 = owns_edge(c.x, c.y, a.x, a.y);
    let own2 = owns_edge(a.x, a.y, b.x, b.y);
    let covers = |e: f32, owned: bool| e > 0.0 || (e == 0.0 && owned);
    let early_depth = state.raster.enables.depth_test && !state.raster.enables.stencil_test;
    let inv_area = 1.0 / area;
    // Perspective-correct attribute interpolation: interpolate attr/w and
    // 1/w linearly in window space.
    let interp = |l: [f32; 3], fa: f32, fb: f32, fc: f32| l[0] * fa + l[1] * fb + l[2] * fc;
    let tex_at = |l: [f32; 3], unit: usize| -> [f32; 2] {
        let w = interp(l, a.inv_w, b.inv_w, c.inv_w);
        let q = interp(
            l,
            a.tex[unit][3] * a.inv_w,
            b.tex[unit][3] * b.inv_w,
            c.tex[unit][3] * c.inv_w,
        ) / w;
        let s = interp(
            l,
            a.tex[unit][0] * a.inv_w,
            b.tex[unit][0] * b.inv_w,
            c.tex[unit][0] * c.inv_w,
        ) / w;
        let t = interp(
            l,
            a.tex[unit][1] * a.inv_w,
            b.tex[unit][1] * b.inv_w,
            c.tex[unit][1] * c.inv_w,
        ) / w;
        if q != 0.0 && q != 1.0 {
            [s / q, t / q]
        } else {
            [s, t]
        }
    };
    let weights = |px: f32, py: f32| {
        [
            edge(b.x, b.y, c.x, c.y, px, py) * inv_area,
            edge(c.x, c.y, a.x, a.y, px, py) * inv_area,
            edge(a.x, a.y, b.x, b.y, px, py) * inv_area,
        ]
    };
    for y in y0..=y1 {
        let py = y as f32 + 0.5;
        for x in x0..=x1 {
            let px = x as f32 + 0.5;
            let e0 = edge(b.x, b.y, c.x, c.y, px, py);
            let e1 = edge(c.x, c.y, a.x, a.y, px, py);
            let e2 = edge(a.x, a.y, b.x, b.y, px, py);
            if !(covers(e0, own0) && covers(e1, own1) && covers(e2, own2)) {
                continue;
            }
            let l = [e0 * inv_area, e1 * inv_area, e2 * inv_area];
            let z = (interp(l, a.z, b.z, c.z) + depth_offset).clamp(0.0, 1.0);
            // Early depth rejection: without stencil, a fragment that fails
            // the depth test is discarded with no side effects, so skip its
            // colour, texture, and fog work. process_fragment still applies
            // the full per-fragment sequence to the survivors.
            if early_depth {
                let index = y * target.width + x;
                if x < target.width
                    && y < target.height
                    && !compare(state.raster.depth_func, z, target.depth[index])
                {
                    continue;
                }
            }
            let w = interp(l, a.inv_w, b.inv_w, c.inv_w);
            let perspective =
                |fa: f32, fb: f32, fc: f32| interp(l, fa * a.inv_w, fb * b.inv_w, fc * c.inv_w) / w;
            let color = std::array::from_fn(|k| perspective(a.color[k], b.color[k], c.color[k]));
            let specular =
                std::array::from_fn(|k| perspective(a.specular[k], b.specular[k], c.specular[k]));
            let mut texcoords = [[0.0; 2]; TEXTURE_UNITS];
            let mut lambda = [0.0; TEXTURE_UNITS];
            for (unit, active) in &units {
                let st = tex_at(l, *unit);
                texcoords[*unit] = st;
                if !active.needs_lod {
                    continue;
                }
                let dx = tex_at(weights(px + 1.0, py), *unit);
                let dy = tex_at(weights(px, py + 1.0), *unit);
                let dudx = (dx[0] - st[0]) * active.width;
                let dvdx = (dx[1] - st[1]) * active.height;
                let dudy = (dy[0] - st[0]) * active.width;
                let dvdy = (dy[1] - st[1]) * active.height;
                let rho = (dudx * dudx + dvdx * dvdx)
                    .max(dudy * dudy + dvdy * dvdy)
                    .sqrt();
                lambda[*unit] = if rho > 0.0 { rho.log2() } else { 0.0 };
            }
            let fog = perspective(a.fog, b.fog, c.fog);
            process_fragment(
                state,
                target,
                &units,
                Fragment {
                    x,
                    y,
                    z,
                    color,
                    specular,
                    texcoords,
                    lambda,
                    fog,
                },
            );
        }
    }
}

/// Draw a line segment; returns the updated stipple counter.
pub fn draw_line(
    state: &ContextState,
    target: &mut RasterTarget,
    a: &WindowVertex,
    b: &WindowVertex,
    offset: bool,
    mut stipple_counter: u32,
) -> u32 {
    let units = active_units(state);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as u32;
    let width = state.raster.line_width.round().max(1.0) as i32;
    let x_major = dx.abs() >= dy.abs();
    let depth_offset = if offset {
        state.raster.polygon_offset[1] / 16_777_216.0
    } else {
        0.0
    };
    let (factor, pattern) = state.raster.line_stipple;
    for step in 0..steps {
        let t = (step as f32 + 0.5) / steps as f32;
        if state.raster.enables.line_stipple {
            let bit = (stipple_counter / factor.max(1) as u32) % 16;
            stipple_counter = stipple_counter.wrapping_add(1);
            if pattern & (1 << bit) == 0 {
                continue;
            }
        }
        let inv_w = a.inv_w + (b.inv_w - a.inv_w) * t;
        let perspective =
            |fa: f32, fb: f32| ((fa * a.inv_w) + (fb * b.inv_w - fa * a.inv_w) * t) / inv_w;
        let color = std::array::from_fn(|k| perspective(a.color[k], b.color[k]));
        let specular = std::array::from_fn(|k| perspective(a.specular[k], b.specular[k]));
        let mut texcoords = [[0.0; 2]; TEXTURE_UNITS];
        for (unit, _) in &units {
            texcoords[*unit] = [
                perspective(a.tex[*unit][0], b.tex[*unit][0]),
                perspective(a.tex[*unit][1], b.tex[*unit][1]),
            ];
        }
        let x = a.x + dx * t;
        let y = a.y + dy * t;
        let z = (a.z + (b.z - a.z) * t + depth_offset).clamp(0.0, 1.0);
        let fog = perspective(a.fog, b.fog);
        for k in -(width - 1) / 2..=width / 2 {
            let (px, py) = if x_major {
                (x, y + k as f32)
            } else {
                (x + k as f32, y)
            };
            if px < 0.0 || py < 0.0 {
                continue;
            }
            process_fragment(
                state,
                target,
                &units,
                Fragment {
                    x: px as usize,
                    y: py as usize,
                    z,
                    color,
                    specular,
                    texcoords,
                    lambda: [0.0; TEXTURE_UNITS],
                    fog,
                },
            );
        }
    }
    stipple_counter
}

pub fn draw_point(state: &ContextState, target: &mut RasterTarget, v: &WindowVertex, offset: bool) {
    let units = active_units(state);
    let size = v.point_size.round().max(1.0) as i32;
    let depth_offset = if offset {
        state.raster.polygon_offset[1] / 16_777_216.0
    } else {
        0.0
    };
    let x0 = (v.x - size as f32 / 2.0 + 0.5).floor() as i32;
    let y0 = (v.y - size as f32 / 2.0 + 0.5).floor() as i32;
    let mut texcoords = [[0.0; 2]; TEXTURE_UNITS];
    for (unit, _) in &units {
        texcoords[*unit] = [v.tex[*unit][0], v.tex[*unit][1]];
    }
    for y in y0..y0 + size {
        for x in x0..x0 + size {
            if x < 0 || y < 0 {
                continue;
            }
            process_fragment(
                state,
                target,
                &units,
                Fragment {
                    x: x as usize,
                    y: y as usize,
                    z: (v.z + depth_offset).clamp(0.0, 1.0),
                    color: v.color,
                    specular: v.specular,
                    texcoords,
                    lambda: [0.0; TEXTURE_UNITS],
                    fog: v.fog,
                },
            );
        }
    }
}

/// Bitmap fragments use the latched raster attributes and ordinary fragment tests.
pub(super) fn draw_bitmap(
    state: &ContextState,
    target: &mut RasterTarget,
    bitmap: &super::bitmap::Bitmap,
) {
    let position = &state.raster.position;
    let x0 = (position.window[0] - bitmap.origin[0]).floor() as i64;
    let y0 = (position.window[1] - bitmap.origin[1]).floor() as i64;
    let units = active_units(state);
    for row in 0..bitmap.height {
        let y = y0.saturating_add(row as i64);
        if y < 0 || y >= target.height as i64 {
            continue;
        }
        for col in 0..bitmap.width {
            let x = x0.saturating_add(col as i64);
            if x < 0 || x >= target.width as i64 || bitmap.pixels[row * bitmap.width + col] == 0 {
                continue;
            }
            process_fragment(
                state,
                target,
                &units,
                Fragment {
                    x: x as usize,
                    y: y as usize,
                    z: position.window[2],
                    color: position.color,
                    specular: [0.0; 3],
                    texcoords: position.tex.map(|t| [t[0] / t[3], t[1] / t[3]]),
                    lambda: [0.0; TEXTURE_UNITS],
                    fog: position.distance,
                },
            );
        }
    }
}
