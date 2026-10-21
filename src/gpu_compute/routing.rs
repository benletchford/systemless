//! Dispatch heuristics shared by every host. They estimate whether a
//! synchronous GPU round trip can beat the CPU before any guest pixels are
//! staged; they do not depend on a device or backend.
use crate::loader::ppc::{PpcQ3RasterFrame, PpcQ3RasterPrimitive, PpcQ3RasterVertex};
use crate::quickdraw::gpu::QuickDrawGpuCommand;

pub(crate) const MAX_RASTER_WORK: u64 = 512_000_000;
pub(crate) const TILE_SIZE: u32 = 16;
const MIN_NATIVE_WORK: u64 = 200_000;
const MAX_TILED_TO_CPU_WORK_RATIO: u64 = 4;

/// Rectangles, pre-tessellated spans and glyph atlases already have their
/// coverage on the CPU. A GPU round trip cannot amortize that work, so keep
/// GPU evaluation for substantial polygon edge work.
pub fn quickdraw_prefers_raster(command: &QuickDrawGpuCommand) -> bool {
    command.polygon_edges.as_ref().is_some_and(|edges| {
        command
            .pixel_count()
            .unwrap_or(0)
            .saturating_mul(edges.len())
            >= 131_072
    })
}

/// Only large color-converting transfers amortize a synchronous readback.
pub fn quickdraw_prefers_blit(destination_pixels: usize, expensive_color: bool) -> bool {
    expensive_color && destination_pixels >= 16_384
}

/// Signed doubled screen area; zero marks a degenerate triangle.
pub fn qd3d_triangle_area(vertices: &[PpcQ3RasterVertex]) -> f64 {
    let [a, b, c] = [vertices[0], vertices[1], vertices[2]];
    (f64::from(c.screen_x) - f64::from(a.screen_x))
        * (f64::from(b.screen_y) - f64::from(a.screen_y))
        - (f64::from(c.screen_y) - f64::from(a.screen_y))
            * (f64::from(b.screen_x) - f64::from(a.screen_x))
}

/// CPU and conservative tiled GPU work for one completed frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qd3dWorkEstimate {
    pub work: u64,
    pub gpu_work: u64,
    pub threshold: u64,
}

impl Qd3dWorkEstimate {
    /// Conservative tiled GPU work above which a frame stays on the CPU.
    pub const MAX_GPU_WORK: u64 = MAX_RASTER_WORK;
    /// Largest accepted ratio of tiled GPU work to estimated CPU work.
    pub const MAX_GPU_TO_CPU_RATIO: u64 = MAX_TILED_TO_CPU_WORK_RATIO;

    pub fn of(frame: &PpcQ3RasterFrame) -> Self {
        // Estimate CPU raster work, rather than using framebuffer size: a tiny
        // triangle in a large window still loses to GPU submission/readback.
        let threshold = (u64::from(frame.width) * u64::from(frame.height) / 4).max(MIN_NATIVE_WORK);
        let mut gpu_work = 0u64;
        let left = (frame.viewport[0].max(0) as u32).min(frame.width);
        let top = (frame.viewport[1].max(0) as u32).min(frame.height);
        let right = (frame.viewport[2].saturating_add(1).max(0) as u32).min(frame.width);
        let bottom = (frame.viewport[3].saturating_add(1).max(0) as u32).min(frame.height);
        let mut work = if frame.clear_color.is_some() {
            u64::from(right.saturating_sub(left)) * u64::from(bottom.saturating_sub(top))
        } else {
            0
        };
        for draw in &frame.draws {
            let (x0, y0, x1, y1) = if draw.primitive == PpcQ3RasterPrimitive::Triangles {
                (left, top, right, bottom)
            } else {
                (0, 0, frame.width, frame.height)
            };
            let cost = 1 + draw
                .lighting
                .as_ref()
                .map_or(0, |lighting| lighting.lights.len()) as u64;
            for vertices in draw.vertices.chunks_exact(3) {
                let degenerate = draw.primitive == PpcQ3RasterPrimitive::Triangles
                    && qd3d_triangle_area(vertices) == 0.;
                let edge_count = if degenerate { 3 } else { 1 };
                let (x0, y0, x1, y1) = if degenerate {
                    (0, 0, frame.width, frame.height)
                } else {
                    (x0, y0, x1, y1)
                };
                let min_x = vertices
                    .iter()
                    .map(|v| v.screen_x)
                    .fold(f32::INFINITY, f32::min)
                    .max(x0 as f32);
                let max_x = vertices
                    .iter()
                    .map(|v| v.screen_x)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .min(x1 as f32);
                let min_y = vertices
                    .iter()
                    .map(|v| v.screen_y)
                    .fold(f32::INFINITY, f32::min)
                    .max(y0 as f32);
                let max_y = vertices
                    .iter()
                    .map(|v| v.screen_y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .min(y1 as f32);
                // Match conservative tile coverage, including inclusive edges.
                let tile_left = min_x.floor().max(0.) as u32 / TILE_SIZE;
                let tile_top = min_y.floor().max(0.) as u32 / TILE_SIZE;
                let tile_right = ((max_x.ceil() + 1.).max(0.).min(frame.width as f32) as u32)
                    .div_ceil(TILE_SIZE);
                let tile_bottom = ((max_y.ceil() + 1.).max(0.).min(frame.height as f32) as u32)
                    .div_ceil(TILE_SIZE);
                gpu_work = gpu_work.saturating_add(
                    u64::from(tile_right.saturating_sub(tile_left))
                        .saturating_mul(u64::from(tile_bottom.saturating_sub(tile_top)))
                        .saturating_mul(u64::from(TILE_SIZE * TILE_SIZE))
                        .saturating_mul(cost)
                        .saturating_mul(edge_count),
                );
                let width = (max_x - min_x).max(0.).ceil() as u64;
                let height = (max_y - min_y).max(0.).ceil() as u64;
                let samples = if degenerate {
                    3 * (width.max(height) + 1)
                } else {
                    match draw.primitive {
                        PpcQ3RasterPrimitive::Triangles => width.saturating_mul(height),
                        PpcQ3RasterPrimitive::Lines => width.max(height) + 1,
                        PpcQ3RasterPrimitive::Points => 1,
                    }
                };
                work = work.saturating_add(samples.saturating_mul(cost));
            }
        }
        Self {
            work,
            gpu_work,
            threshold,
        }
    }

    /// Whether a synchronous GPU frame should replace CPU rasterization.
    pub fn prefers_gpu(&self) -> bool {
        self.work >= self.threshold
            && self.gpu_work <= MAX_RASTER_WORK
            && self.gpu_work <= self.work.saturating_mul(MAX_TILED_TO_CPU_WORK_RATIO)
    }
}
