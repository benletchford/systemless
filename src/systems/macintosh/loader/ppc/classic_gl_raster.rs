//! OpenGL 1.2.1 homogeneous clipping and filled triangle rasterization.
//! https://registry.khronos.org/OpenGL/specs/gl/glspec121.pdf

use super::classic_gl_framebuffer::ClassicGlFramebuffer;
use super::classic_gl_texture::ClassicGlTexture2D;

#[derive(Debug, Clone, Copy)]
pub struct ClassicGlVertex {
    pub clip: [f64; 4],
    pub color: [f64; 4],
    pub texcoord: [f64; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct ClassicGlRasterState {
    pub viewport: (i32, i32, u32, u32),
    pub depth_range: (f64, f64),
    pub scissor: Option<(i32, i32, u32, u32)>,
    pub draw_front: bool,
    pub draw_back: bool,
    pub color_mask: [bool; 4],
    pub depth_test: bool,
    pub depth_mask: bool,
}

#[derive(Clone, Copy)]
struct WindowVertex {
    x: f64,
    y: f64,
    depth: f64,
    reciprocal_w: f64,
    color: [f64; 4],
    texcoord: [f64; 4],
}

fn plane_distance(vertex: ClassicGlVertex, plane: usize) -> f64 {
    let [x, y, z, w] = vertex.clip;
    match plane {
        0 => w + x,
        1 => w - x,
        2 => w + y,
        3 => w - y,
        4 => w + z,
        _ => w - z,
    }
}

fn interpolate(a: ClassicGlVertex, b: ClassicGlVertex, fraction: f64) -> ClassicGlVertex {
    ClassicGlVertex {
        clip: std::array::from_fn(|index| {
            a.clip[index] + fraction * (b.clip[index] - a.clip[index])
        }),
        color: std::array::from_fn(|index| {
            a.color[index] + fraction * (b.color[index] - a.color[index])
        }),
        texcoord: std::array::from_fn(|index| {
            a.texcoord[index] + fraction * (b.texcoord[index] - a.texcoord[index])
        }),
    }
}

fn clipped_polygon(vertices: [ClassicGlVertex; 3]) -> Vec<ClassicGlVertex> {
    let mut polygon = vertices.to_vec();
    for plane in 0..6 {
        if polygon.is_empty() {
            break;
        }
        let mut clipped = Vec::with_capacity(polygon.len() + 1);
        let mut previous = *polygon.last().unwrap();
        let mut previous_distance = plane_distance(previous, plane);
        for &current in &polygon {
            let current_distance = plane_distance(current, plane);
            let previous_inside = previous_distance >= 0.0;
            let current_inside = current_distance >= 0.0;
            if previous_inside != current_inside {
                let fraction = previous_distance / (previous_distance - current_distance);
                clipped.push(interpolate(previous, current, fraction));
            }
            if current_inside {
                clipped.push(current);
            }
            previous = current;
            previous_distance = current_distance;
        }
        polygon = clipped;
    }
    polygon
}

fn window_vertex(vertex: ClassicGlVertex, state: ClassicGlRasterState) -> Option<WindowVertex> {
    let [x, y, z, w] = vertex.clip;
    if !vertex.clip.into_iter().all(f64::is_finite) || w <= 0.0 {
        return None;
    }
    let reciprocal_w = 1.0 / w;
    let (left, bottom, width, height) = state.viewport;
    let (near, far) = state.depth_range;
    Some(WindowVertex {
        x: f64::from(left) + (x * reciprocal_w + 1.0) * f64::from(width) / 2.0,
        y: f64::from(bottom) + (y * reciprocal_w + 1.0) * f64::from(height) / 2.0,
        depth: (far - near) * (z * reciprocal_w + 1.0) / 2.0 + near,
        reciprocal_w,
        color: vertex.color,
        texcoord: vertex.texcoord,
    })
}

fn edge(a: WindowVertex, b: WindowVertex, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

fn inclusive_edge(a: WindowVertex, b: WindowVertex) -> bool {
    b.y > a.y || (b.y == a.y && b.x < a.x)
}

fn raster_triangle(
    framebuffer: &mut ClassicGlFramebuffer,
    vertices: [WindowVertex; 3],
    state: ClassicGlRasterState,
    texture: Option<&ClassicGlTexture2D>,
) -> bool {
    let mut vertices = vertices;
    let mut area = edge(vertices[0], vertices[1], vertices[2].x, vertices[2].y);
    if area == 0.0 {
        return true;
    }
    if area < 0.0 {
        vertices.swap(1, 2);
        area = -area;
    }
    let min_x = vertices.iter().map(|v| v.x).fold(f64::INFINITY, f64::min);
    let max_x = vertices
        .iter()
        .map(|v| v.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = vertices.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
    let max_y = vertices
        .iter()
        .map(|v| v.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let x0 = min_x.floor().max(0.0) as u32;
    let y0 = min_y.floor().max(0.0) as u32;
    let x1 = max_x.ceil().min(f64::from(framebuffer.width())) as u32;
    let y1 = max_y.ceil().min(f64::from(framebuffer.height())) as u32;
    let [a, b, c] = vertices;
    for y in y0..y1 {
        for x in x0..x1 {
            let (sample_x, sample_y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let edges = [
                edge(b, c, sample_x, sample_y),
                edge(c, a, sample_x, sample_y),
                edge(a, b, sample_x, sample_y),
            ];
            let inclusive = [
                inclusive_edge(b, c),
                inclusive_edge(c, a),
                inclusive_edge(a, b),
            ];
            if edges
                .iter()
                .zip(inclusive)
                .any(|(&edge, inclusive)| edge < 0.0 || (edge == 0.0 && !inclusive))
            {
                continue;
            }
            let weights = edges.map(|edge| edge / area);
            let denominator: f64 = weights
                .iter()
                .zip(vertices)
                .map(|(&weight, vertex)| weight * vertex.reciprocal_w)
                .sum();
            if denominator <= 0.0 || !denominator.is_finite() {
                return false;
            }
            let mut color: [f64; 4] = std::array::from_fn(|component| {
                let value: f64 = weights
                    .iter()
                    .zip(vertices)
                    .map(|(&weight, vertex)| weight * vertex.reciprocal_w * vertex.color[component])
                    .sum::<f64>()
                    / denominator;
                value.clamp(0.0, 1.0)
            });
            if let Some(texture) = texture {
                let texcoord = std::array::from_fn(|component| {
                    weights
                        .iter()
                        .zip(vertices)
                        .map(|(&weight, vertex)| {
                            weight * vertex.reciprocal_w * vertex.texcoord[component]
                        })
                        .sum::<f64>()
                        / denominator
                });
                if let Some(image) = texture.images[0].as_ref() {
                    // OpenGL 1.2.1, section 3.8.5, equation 3.15: the footprint uses
                    // window derivatives of projected texture coordinates in texels.
                    let weight_dx = [
                        -(c.y - b.y) / area,
                        -(a.y - c.y) / area,
                        -(b.y - a.y) / area,
                    ];
                    let weight_dy = [(c.x - b.x) / area, (a.x - c.x) / area, (b.x - a.x) / area];
                    let weighted = |component: usize, weights: [f64; 3]| -> f64 {
                        weights
                            .iter()
                            .zip(vertices)
                            .map(|(&weight, vertex)| {
                                weight * vertex.reciprocal_w * vertex.texcoord[component]
                            })
                            .sum()
                    };
                    let s = weighted(0, weights);
                    let t = weighted(1, weights);
                    let q = weighted(3, weights);
                    if q != 0.0 {
                        let derivative = |component: usize, direction: [f64; 3], numerator: f64| {
                            (weighted(component, direction) * q
                                - numerator * weighted(3, direction))
                                / (q * q)
                        };
                        let (w, h) = (f64::from(image.width), f64::from(image.height));
                        let rho_x = (w * derivative(0, weight_dx, s))
                            .hypot(h * derivative(1, weight_dx, t));
                        let rho_y = (w * derivative(0, weight_dy, s))
                            .hypot(h * derivative(1, weight_dy, t));
                        let rho = rho_x.max(rho_y);
                        let lod = if rho > 0.0 { rho.log2() } else { -1000.0 };
                        if let Some(modulated) = texture.modulate_with_lod(color, texcoord, lod) {
                            color = modulated;
                        }
                    }
                }
            }
            let color = color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
            let depth = weights
                .iter()
                .zip(vertices)
                .map(|(&weight, vertex)| weight * vertex.depth)
                .sum::<f64>() as f32;
            if !framebuffer.write_fragment(x, y, depth, color, state) {
                return false;
            }
        }
    }
    true
}

pub fn draw_triangle(
    framebuffer: &mut ClassicGlFramebuffer,
    vertices: [ClassicGlVertex; 3],
    state: ClassicGlRasterState,
    texture: Option<&ClassicGlTexture2D>,
) -> bool {
    if vertices
        .iter()
        .any(|vertex| !vertex.clip.into_iter().all(f64::is_finite))
    {
        return false;
    }
    let polygon = clipped_polygon(vertices);
    if polygon.len() < 3 {
        return true;
    }
    let Some(window) = polygon
        .into_iter()
        .map(|vertex| window_vertex(vertex, state))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    for index in 1..window.len() - 1 {
        if !raster_triangle(
            framebuffer,
            [window[0], window[index], window[index + 1]],
            state,
            texture,
        ) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::classic_gl_framebuffer::ClassicGlColorBuffer;
    use super::*;

    fn state() -> ClassicGlRasterState {
        ClassicGlRasterState {
            viewport: (0, 0, 4, 4),
            depth_range: (0.0, 1.0),
            scissor: None,
            draw_front: true,
            draw_back: false,
            color_mask: [true; 4],
            depth_test: false,
            depth_mask: true,
        }
    }

    fn vertex(x: f64, y: f64, z: f64, w: f64, color: [f64; 4]) -> ClassicGlVertex {
        ClassicGlVertex {
            clip: [x, y, z, w],
            color,
            texcoord: [0.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn filled_triangle_writes_visible_pixels_with_lower_left_origin() {
        let mut framebuffer = ClassicGlFramebuffer::new(4, 4, false).unwrap();
        let red = [1.0, 0.0, 0.0, 1.0];
        assert!(draw_triangle(
            &mut framebuffer,
            [
                vertex(-1.0, -1.0, 0.0, 1.0, red),
                vertex(1.0, -1.0, 0.0, 1.0, red),
                vertex(-1.0, 1.0, 0.0, 1.0, red),
            ],
            state(),
            None
        ));
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 3, 3),
            Some([0; 4])
        );
    }

    #[test]
    fn triangle_clips_to_view_volume_and_respects_scissor_depth() {
        let mut framebuffer = ClassicGlFramebuffer::new(4, 4, false).unwrap();
        let mut state = state();
        state.scissor = Some((1, 1, 2, 2));
        state.depth_test = true;
        let green = [0.0, 1.0, 0.0, 1.0];
        assert!(draw_triangle(
            &mut framebuffer,
            [
                vertex(-2.0, -2.0, 0.0, 1.0, green),
                vertex(2.0, -2.0, 0.0, 1.0, green),
                vertex(0.0, 2.0, 0.0, 1.0, green),
            ],
            state,
            None
        ));
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 1, 1),
            Some([0, 255, 0, 255])
        );
        assert_eq!(framebuffer.depth_at(1, 1), Some(0.5));
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 0, 0),
            Some([0; 4])
        );
        let blue = [0.0, 0.0, 1.0, 1.0];
        assert!(draw_triangle(
            &mut framebuffer,
            [
                vertex(-2.0, -2.0, 0.5, 1.0, blue),
                vertex(2.0, -2.0, 0.5, 1.0, blue),
                vertex(0.0, 2.0, 0.5, 1.0, blue),
            ],
            state,
            None
        ));
        assert_eq!(
            framebuffer.pixel(ClassicGlColorBuffer::Front, 1, 1),
            Some([0, 255, 0, 255])
        );
    }
}
