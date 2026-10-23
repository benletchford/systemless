//! Vertex processing, primitive assembly and clipping (OpenGL 1.2.1
//! specification, sections 2.6-2.13).

use super::consts::*;
use super::math::{dot3, normalize3, Mat4, Vec4};
use super::raster::{self, RasterTarget, WindowVertex};
use super::state::{ContextState, Material, TEXTURE_UNITS};

/// A vertex as specified by the application, in object coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: Vec4,
    pub color: [f32; 4],
    pub normal: [f32; 3],
    pub tex: [[f32; 4]; TEXTURE_UNITS],
    pub edge: bool,
}

/// A vertex after transformation and lighting, before clipping.
#[derive(Debug, Clone, Copy)]
pub struct ClipVertex {
    pub clip: Vec4,
    pub eye: Vec4,
    pub front: [f32; 4],
    pub back: [f32; 4],
    pub front_specular: [f32; 3],
    pub back_specular: [f32; 3],
    pub tex: [[f32; 4]; TEXTURE_UNITS],
    pub fog: f32,
    pub point_size: f32,
    pub edge: bool,
}

impl ClipVertex {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let l4 = |a: [f32; 4], b: [f32; 4]| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ]
        };
        let l3 = |a: [f32; 3], b: [f32; 3]| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        Self {
            clip: l4(self.clip, other.clip),
            eye: l4(self.eye, other.eye),
            front: l4(self.front, other.front),
            back: l4(self.back, other.back),
            front_specular: l3(self.front_specular, other.front_specular),
            back_specular: l3(self.back_specular, other.back_specular),
            tex: std::array::from_fn(|unit| l4(self.tex[unit], other.tex[unit])),
            fog: self.fog + (other.fog - self.fog) * t,
            point_size: self.point_size + (other.point_size - self.point_size) * t,
            edge: self.edge,
        }
    }
}

/// Per-draw values derived from context state once rather than per vertex.
pub struct VertexTransform {
    modelview: Mat4,
    mvp: Mat4,
    normal_matrix: Mat4,
    texture: [Option<Mat4>; TEXTURE_UNITS],
    rescale: f32,
}

impl VertexTransform {
    pub fn new(state: &ContextState) -> Self {
        let modelview = *state.modelview_top();
        let mvp = state.projection_top().mul(&modelview);
        let normal_matrix = modelview.inverse().transpose();
        let texture = std::array::from_fn(|unit| {
            let matrix = *state.texture_top(unit);
            (matrix != Mat4::IDENTITY).then_some(matrix)
        });
        let rescale = if state.raster.enables.rescale_normal {
            let m = &normal_matrix;
            let column = [m.at(0, 2), m.at(1, 2), m.at(2, 2)];
            let length = dot3(column, column).sqrt();
            if length == 0.0 {
                1.0
            } else {
                1.0 / length
            }
        } else {
            1.0
        };
        Self {
            modelview,
            mvp,
            normal_matrix,
            texture,
            rescale,
        }
    }
}

fn material_for(state: &ContextState, face: usize, current_color: [f32; 4]) -> Material {
    let mut material = state.raster.materials[face];
    let raster = &state.raster;
    if raster.enables.color_material {
        let applies = match raster.color_material_face {
            FRONT => face == 0,
            BACK => face == 1,
            _ => true,
        };
        if applies {
            match raster.color_material_mode {
                AMBIENT => material.ambient = current_color,
                DIFFUSE => material.diffuse = current_color,
                SPECULAR => material.specular = current_color,
                EMISSION => material.emission = current_color,
                _ => {
                    material.ambient = current_color;
                    material.diffuse = current_color;
                }
            }
        }
    }
    material
}

/// Lighting equation of OpenGL 1.2.1 section 2.13.1, returning the primary
/// and (with separate specular) secondary colors.
fn light_vertex(
    state: &ContextState,
    eye: Vec4,
    normal: [f32; 3],
    material: &Material,
) -> ([f32; 4], [f32; 3]) {
    let raster = &state.raster;
    let mut color = [
        material.emission[0] + material.ambient[0] * raster.light_model_ambient[0],
        material.emission[1] + material.ambient[1] * raster.light_model_ambient[1],
        material.emission[2] + material.ambient[2] * raster.light_model_ambient[2],
    ];
    let mut specular_sum = [0.0f32; 3];
    let vertex = if eye[3] != 0.0 {
        [eye[0] / eye[3], eye[1] / eye[3], eye[2] / eye[3]]
    } else {
        [eye[0], eye[1], eye[2]]
    };
    for light in raster.lights.iter().filter(|light| light.enabled) {
        let (to_light, distance) = if light.position[3] == 0.0 {
            (
                normalize3([light.position[0], light.position[1], light.position[2]]),
                None,
            )
        } else {
            let p = [
                light.position[0] / light.position[3] - vertex[0],
                light.position[1] / light.position[3] - vertex[1],
                light.position[2] / light.position[3] - vertex[2],
            ];
            let d = dot3(p, p).sqrt();
            (normalize3(p), Some(d))
        };
        let attenuation = match distance {
            Some(d) => {
                let denominator =
                    light.attenuation[0] + light.attenuation[1] * d + light.attenuation[2] * d * d;
                if denominator > 0.0 {
                    1.0 / denominator
                } else {
                    1.0
                }
            }
            None => 1.0,
        };
        let spot = if light.spot_cutoff != 180.0 {
            let direction = normalize3(light.spot_direction);
            let cos = dot3([-to_light[0], -to_light[1], -to_light[2]], direction);
            if cos < light.spot_cutoff.to_radians().cos() {
                0.0
            } else {
                cos.max(0.0).powf(light.spot_exponent)
            }
        } else {
            1.0
        };
        let factor = attenuation * spot;
        if factor == 0.0 {
            continue;
        }
        let n_dot_l = dot3(normal, to_light);
        for c in 0..3 {
            color[c] += factor * material.ambient[c] * light.ambient[c];
        }
        if n_dot_l > 0.0 {
            for c in 0..3 {
                color[c] += factor * n_dot_l * material.diffuse[c] * light.diffuse[c];
            }
            let eye_vector = if raster.light_model_local_viewer {
                normalize3([-vertex[0], -vertex[1], -vertex[2]])
            } else {
                [0.0, 0.0, 1.0]
            };
            let half = normalize3([
                to_light[0] + eye_vector[0],
                to_light[1] + eye_vector[1],
                to_light[2] + eye_vector[2],
            ]);
            let n_dot_h = dot3(normal, half).max(0.0);
            let shine = if material.shininess == 0.0 {
                1.0
            } else {
                n_dot_h.powf(material.shininess)
            };
            for c in 0..3 {
                specular_sum[c] += factor * shine * material.specular[c] * light.specular[c];
            }
        }
    }
    let alpha = material.diffuse[3];
    if raster.separate_specular {
        (
            [
                color[0].clamp(0.0, 1.0),
                color[1].clamp(0.0, 1.0),
                color[2].clamp(0.0, 1.0),
                alpha,
            ],
            [
                specular_sum[0].clamp(0.0, 1.0),
                specular_sum[1].clamp(0.0, 1.0),
                specular_sum[2].clamp(0.0, 1.0),
            ],
        )
    } else {
        (
            [
                (color[0] + specular_sum[0]).clamp(0.0, 1.0),
                (color[1] + specular_sum[1]).clamp(0.0, 1.0),
                (color[2] + specular_sum[2]).clamp(0.0, 1.0),
                alpha.clamp(0.0, 1.0),
            ],
            [0.0; 3],
        )
    }
}

fn generate_texcoords(
    state: &ContextState,
    unit: usize,
    vertex: &Vertex,
    eye: Vec4,
    eye_normal: [f32; 3],
) -> [f32; 4] {
    let mut out = vertex.tex[unit];
    let gens = &state.raster.units[unit].gen;
    if !gens.iter().any(|gen| gen.enabled) {
        return out;
    }
    let sphere = || {
        let u = normalize3([eye[0], eye[1], eye[2]]);
        let n = eye_normal;
        let d = 2.0 * dot3(n, u);
        let r = [u[0] - n[0] * d, u[1] - n[1] * d, u[2] - n[2] * d + 1.0];
        let m = 2.0 * dot3(r, r).sqrt();
        if m == 0.0 {
            [0.5, 0.5]
        } else {
            [(u[0] - n[0] * d) / m + 0.5, (u[1] - n[1] * d) / m + 0.5]
        }
    };
    for (index, gen) in gens.iter().enumerate() {
        if !gen.enabled {
            continue;
        }
        out[index] = match gen.mode {
            OBJECT_LINEAR => {
                let p = vertex.position;
                gen.object_plane[0] * p[0]
                    + gen.object_plane[1] * p[1]
                    + gen.object_plane[2] * p[2]
                    + gen.object_plane[3] * p[3]
            }
            SPHERE_MAP if index < 2 => sphere()[index],
            _ => {
                gen.eye_plane[0] * eye[0]
                    + gen.eye_plane[1] * eye[1]
                    + gen.eye_plane[2] * eye[2]
                    + gen.eye_plane[3] * eye[3]
            }
        };
    }
    out
}

pub fn process_vertex(
    state: &ContextState,
    transform: &VertexTransform,
    vertex: &Vertex,
) -> ClipVertex {
    let raster = &state.raster;
    let eye = transform.modelview.transform(vertex.position);
    let clip = transform.mvp.transform(vertex.position);
    let needs_normal = raster.enables.lighting
        || raster.units.iter().any(|unit| {
            unit.gen
                .iter()
                .any(|gen| gen.enabled && gen.mode == SPHERE_MAP)
        });
    let mut eye_normal = [0.0; 3];
    if needs_normal {
        eye_normal = transform.normal_matrix.transform_direction(vertex.normal);
        if transform.rescale != 1.0 {
            eye_normal = [
                eye_normal[0] * transform.rescale,
                eye_normal[1] * transform.rescale,
                eye_normal[2] * transform.rescale,
            ];
        }
        if raster.enables.normalize {
            eye_normal = normalize3(eye_normal);
        }
    }
    let (front, back, front_specular, back_specular) = if raster.enables.lighting {
        let front_material = material_for(state, 0, vertex.color);
        let (front, front_specular) = light_vertex(state, eye, eye_normal, &front_material);
        if raster.light_model_two_side {
            let back_material = material_for(state, 1, vertex.color);
            let flipped = [-eye_normal[0], -eye_normal[1], -eye_normal[2]];
            let (back, back_specular) = light_vertex(state, eye, flipped, &back_material);
            (front, back, front_specular, back_specular)
        } else {
            (front, front, front_specular, front_specular)
        }
    } else {
        let color = vertex.color.map(|c| c.clamp(0.0, 1.0));
        (color, color, [0.0; 3], [0.0; 3])
    };
    let tex = std::array::from_fn(|unit| {
        let generated = generate_texcoords(state, unit, vertex, eye, eye_normal);
        match transform.texture[unit] {
            Some(matrix) => matrix.transform(generated),
            None => generated,
        }
    });
    let distance = (eye[0] * eye[0] + eye[1] * eye[1] + eye[2] * eye[2]).sqrt();
    let point_size = if raster.point_attenuation != [1.0, 0.0, 0.0] {
        let a = raster.point_attenuation;
        let denominator = a[0] + a[1] * distance + a[2] * distance * distance;
        let scale = if denominator > 0.0 {
            (1.0 / denominator).sqrt()
        } else {
            1.0
        };
        (raster.point_size * scale).clamp(raster.point_size_min, raster.point_size_max)
    } else {
        raster.point_size
    };
    ClipVertex {
        clip,
        eye,
        front,
        back,
        front_specular,
        back_specular,
        tex,
        fog: eye[2].abs(),
        point_size,
        edge: vertex.edge,
    }
}

/// Clip planes in clip coordinates (the view volume) plus enabled user
/// planes, which test eye coordinates.
fn inside(state: &ContextState, plane: usize, v: &ClipVertex) -> f32 {
    let c = v.clip;
    match plane {
        0 => c[3] + c[0],
        1 => c[3] - c[0],
        2 => c[3] + c[1],
        3 => c[3] - c[1],
        4 => c[3] + c[2],
        5 => c[3] - c[2],
        _ => {
            let p = state.raster.clip_planes[plane - 6];
            p[0] * v.eye[0] + p[1] * v.eye[1] + p[2] * v.eye[2] + p[3] * v.eye[3]
        }
    }
}

fn active_planes(state: &ContextState) -> impl Iterator<Item = usize> + '_ {
    (0..6).chain(
        state
            .raster
            .enables
            .clip_planes
            .iter()
            .enumerate()
            .filter(|(_, enabled)| **enabled)
            .map(|(index, _)| index + 6),
    )
}

pub fn clip_polygon(state: &ContextState, mut polygon: Vec<ClipVertex>) -> Vec<ClipVertex> {
    for plane in active_planes(state) {
        if polygon.is_empty() {
            break;
        }
        let all_inside = polygon.iter().all(|v| inside(state, plane, v) >= 0.0);
        if all_inside {
            continue;
        }
        let mut output = Vec::with_capacity(polygon.len() + 2);
        for index in 0..polygon.len() {
            let current = &polygon[index];
            let next = &polygon[(index + 1) % polygon.len()];
            let dc = inside(state, plane, current);
            let dn = inside(state, plane, next);
            if dc >= 0.0 {
                output.push(*current);
            }
            if (dc >= 0.0) != (dn >= 0.0) {
                let t = dc / (dc - dn);
                let mut vertex = current.lerp(next, t);
                // An edge created by clipping is a boundary edge only where
                // it follows an original boundary edge.
                vertex.edge = if dc >= 0.0 { current.edge } else { true };
                output.push(vertex);
            }
        }
        polygon = output;
    }
    polygon
}

fn clip_line(
    state: &ContextState,
    a: ClipVertex,
    b: ClipVertex,
) -> Option<(ClipVertex, ClipVertex)> {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for plane in active_planes(state) {
        let da = inside(state, plane, &a);
        let db = inside(state, plane, &b);
        if da < 0.0 && db < 0.0 {
            return None;
        }
        if da < 0.0 {
            t0 = t0.max(da / (da - db));
        } else if db < 0.0 {
            t1 = t1.min(da / (da - db));
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((a.lerp(&b, t0), a.lerp(&b, t1)))
}

fn to_window(state: &ContextState, v: &ClipVertex, back_facing: bool) -> WindowVertex {
    let w = if v.clip[3] == 0.0 {
        f32::MIN_POSITIVE
    } else {
        v.clip[3]
    };
    let inv_w = 1.0 / w;
    let viewport = state.raster.viewport;
    let range = state.raster.depth_range;
    let x = (v.clip[0] * inv_w + 1.0) * 0.5 * viewport[2] as f32 + viewport[0] as f32;
    let y = (v.clip[1] * inv_w + 1.0) * 0.5 * viewport[3] as f32 + viewport[1] as f32;
    let z = (v.clip[2] * inv_w) * 0.5 * (range[1] - range[0]) + 0.5 * (range[1] + range[0]);
    let use_back =
        back_facing && state.raster.light_model_two_side && state.raster.enables.lighting;
    WindowVertex {
        x,
        y,
        z,
        inv_w,
        color: if use_back { v.back } else { v.front },
        specular: if use_back {
            v.back_specular
        } else {
            v.front_specular
        },
        tex: v.tex,
        fog: v.fog,
        point_size: v.point_size,
        edge: v.edge,
    }
}

fn signed_area(vertices: &[WindowVertex]) -> f32 {
    let mut area = 0.0;
    for index in 0..vertices.len() {
        let a = &vertices[index];
        let b = &vertices[(index + 1) % vertices.len()];
        area += a.x * b.y - b.x * a.y;
    }
    area * 0.5
}

/// Draw one polygon (already flat-shaded if required).
fn draw_polygon(state: &ContextState, target: &mut RasterTarget, polygon: Vec<ClipVertex>) {
    let clipped = clip_polygon(state, polygon);
    if clipped.len() < 3 {
        return;
    }
    let provisional: Vec<WindowVertex> =
        clipped.iter().map(|v| to_window(state, v, false)).collect();
    let area = signed_area(&provisional);
    if area == 0.0 {
        return;
    }
    let ccw = area > 0.0;
    let front_facing = if state.raster.front_face == CCW {
        ccw
    } else {
        !ccw
    };
    if state.raster.enables.cull_face {
        let culled = match state.raster.cull_face_mode {
            FRONT => front_facing,
            BACK => !front_facing,
            _ => true,
        };
        if culled {
            return;
        }
    }
    let vertices: Vec<WindowVertex> = if front_facing {
        provisional
    } else {
        clipped.iter().map(|v| to_window(state, v, true)).collect()
    };
    let mode = state.raster.polygon_mode[usize::from(!front_facing)];
    match mode {
        POINT => {
            for vertex in vertices.iter().filter(|v| v.edge) {
                raster::draw_point(
                    state,
                    target,
                    vertex,
                    state.raster.enables.polygon_offset_point,
                );
            }
        }
        LINE => {
            for index in 0..vertices.len() {
                if vertices[index].edge {
                    let next = &vertices[(index + 1) % vertices.len()];
                    raster::draw_line(
                        state,
                        target,
                        &vertices[index],
                        next,
                        state.raster.enables.polygon_offset_line,
                        0,
                    );
                }
            }
        }
        _ => {
            let offset =
                raster::polygon_offset(state, &vertices, state.raster.enables.polygon_offset_fill);
            for index in 1..vertices.len() - 1 {
                raster::draw_triangle(
                    state,
                    target,
                    &vertices[0],
                    &vertices[index],
                    &vertices[index + 1],
                    offset,
                );
            }
        }
    }
}

fn flatten(vertices: &mut [ClipVertex], provoking: usize) {
    let source = vertices[provoking];
    for vertex in vertices.iter_mut() {
        vertex.front = source.front;
        vertex.back = source.back;
        vertex.front_specular = source.front_specular;
        vertex.back_specular = source.back_specular;
    }
}

/// Assemble and rasterize the primitives of a glBegin/glEnd block (or an
/// equivalent array draw) from processed vertices.
pub fn draw_primitives(
    state: &ContextState,
    target: &mut RasterTarget,
    mode: u32,
    vertices: &[ClipVertex],
) {
    let flat = state.raster.shade_model == FLAT;
    let polygon = |mut points: Vec<ClipVertex>, provoking: usize, target: &mut RasterTarget| {
        if flat {
            flatten(&mut points, provoking);
        }
        draw_polygon(state, target, points);
    };
    match mode {
        POINTS => {
            for vertex in vertices {
                let clip = vertex.clip;
                let w = clip[3];
                if clip[0].abs() > w || clip[1].abs() > w || clip[2].abs() > w || w <= 0.0 {
                    continue;
                }
                if (0..6).any(|plane| {
                    state.raster.enables.clip_planes[plane]
                        && inside(state, plane + 6, vertex) < 0.0
                }) {
                    continue;
                }
                let window = to_window(state, vertex, false);
                raster::draw_point(state, target, &window, false);
            }
        }
        LINES | LINE_STRIP | LINE_LOOP => {
            let segments: Vec<(usize, usize)> = match mode {
                LINES => (0..vertices.len() / 2)
                    .map(|i| (2 * i, 2 * i + 1))
                    .collect(),
                LINE_STRIP => (1..vertices.len()).map(|i| (i - 1, i)).collect(),
                _ => {
                    let mut s: Vec<(usize, usize)> =
                        (1..vertices.len()).map(|i| (i - 1, i)).collect();
                    if vertices.len() > 1 {
                        s.push((vertices.len() - 1, 0));
                    }
                    s
                }
            };
            let mut stipple_counter = 0u32;
            for (a, b) in segments {
                let (mut va, vb) = (vertices[a], vertices[b]);
                if flat {
                    va.front = vb.front;
                    va.back = vb.back;
                    va.front_specular = vb.front_specular;
                    va.back_specular = vb.back_specular;
                }
                if mode == LINES {
                    stipple_counter = 0;
                }
                if let Some((ca, cb)) = clip_line(state, va, vb) {
                    let wa = to_window(state, &ca, false);
                    let wb = to_window(state, &cb, false);
                    stipple_counter =
                        raster::draw_line(state, target, &wa, &wb, false, stipple_counter);
                }
            }
        }
        TRIANGLES => {
            for triangle in vertices.chunks_exact(3) {
                polygon(triangle.to_vec(), 2, target);
            }
        }
        TRIANGLE_STRIP => {
            for i in 2..vertices.len() {
                let (a, b) = if i % 2 == 0 {
                    (i - 2, i - 1)
                } else {
                    (i - 1, i - 2)
                };
                let mut points = vec![vertices[a], vertices[b], vertices[i]];
                for p in &mut points {
                    p.edge = true;
                }
                polygon(points, 2, target);
            }
        }
        TRIANGLE_FAN => {
            for i in 2..vertices.len() {
                let mut points = vec![vertices[0], vertices[i - 1], vertices[i]];
                for p in &mut points {
                    p.edge = true;
                }
                polygon(points, 2, target);
            }
        }
        QUADS => {
            for quad in vertices.chunks_exact(4) {
                polygon(quad.to_vec(), 3, target);
            }
        }
        QUAD_STRIP => {
            let mut i = 3;
            while i < vertices.len() {
                let mut points = vec![
                    vertices[i - 3],
                    vertices[i - 2],
                    vertices[i],
                    vertices[i - 1],
                ];
                for p in &mut points {
                    p.edge = true;
                }
                polygon(points, 2, target);
                i += 2;
            }
        }
        POLYGON => {
            if vertices.len() >= 3 {
                polygon(vertices.to_vec(), 0, target);
            }
        }
        _ => {}
    }
}
