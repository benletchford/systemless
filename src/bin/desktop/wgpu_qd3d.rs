//! GPU rasterization with synchronous RGB555 readback for guest memory coherence.
#[cfg(feature = "test-support")]
use systemless::gpu_compute::qd3d_triangle_area as triangle_area;
#[cfg(test)]
use systemless::gpu_compute::{raster_tiles, TRIANGLE_BYTES};
use systemless::gpu_compute::{Qd3dCompute, Qd3dWorkEstimate};
#[cfg(test)]
use systemless::loader::ppc::{
    ppc_q3_vector3d_normalized_value, PpcQ3LightKind, PpcQ3RasterVertex,
    PpcQ3SoftwareProjectedPoint, PPC_Q3_ILLUMINATION_TYPE_LAMBERT, PPC_Q3_ILLUMINATION_TYPE_PHONG,
};
use systemless::loader::ppc::{
    PpcQ3GpuFrame, PpcQ3GpuRenderer, PpcQ3RasterFrame, PpcQ3RasterPrimitive,
};

#[cfg(feature = "test-support")]
#[path = "wgpu_qd3d/routing_trace.rs"]
mod routing_trace;

pub(super) struct Qd3dRenderer {
    state: Option<State>,
    initialization_error: Option<String>,
    last_fallback: Option<String>,
    #[cfg(feature = "test-support")]
    routing_trace: Option<routing_trace::RoutingTrace>,
}
/// Desktop compute device: the shared kernels with a blocking, bounded wait.
struct State {
    health: std::sync::Arc<super::renderer_recovery::Health>,
    core: Qd3dCompute,
}
impl std::ops::Deref for State {
    type Target = Qd3dCompute;
    fn deref(&self) -> &Qd3dCompute {
        &self.core
    }
}

impl Drop for Qd3dRenderer {
    fn drop(&mut self) {
        self.retire_failed_state();
    }
}

impl Qd3dRenderer {
    #[cfg(all(test, feature = "test-support"))]
    pub(super) fn qualification_snapshot(&self) -> Result<serde_json::Value, String> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| "compute device not initialized".to_owned())?;
        state.health.check().map_err(|e| e.to_string())?;
        let mut report = super::gpu_resources::snapshot_for_role(&state.device, "QD3D compute");
        report["scratch"] = state.buffers.resource_snapshot();
        Ok(report)
    }

    fn retire_failed_state(&mut self) {
        if let Some(error) = self
            .state
            .as_ref()
            .and_then(|state| state.health.check().err())
        {
            self.initialization_error = Some(error.to_string());
            if let Some(state) = self.state.take() {
                super::renderer_recovery::retire_compute(state);
            }
        }
    }

    /// Adapter/device creation is deferred until a supported 3D frame arrives.
    pub fn new() -> Self {
        eprintln!("[GPU] QD3D adaptive dispatch enabled");
        Self {
            state: None,
            initialization_error: None,
            last_fallback: None,
            #[cfg(feature = "test-support")]
            routing_trace: routing_trace::RoutingTrace::from_environment(),
        }
    }

    fn render_frame(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<Vec<u16>, String> {
        #[cfg(feature = "test-support")]
        let trace_started = self
            .routing_trace
            .as_ref()
            .map(|_| std::time::Instant::now());
        self.retire_failed_state();
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render_with_palette(frame, background, clut),
            None => Err(self.initialization_error.as_ref().unwrap().clone()),
        };
        self.retire_failed_state();
        if let Err(error) = &result {
            if self.last_fallback.as_ref() != Some(error) {
                eprintln!("[GPU] QD3D software fallback: {error}");
                self.last_fallback = Some(error.clone());
            }
        } else {
            self.last_fallback = None;
        }
        #[cfg(feature = "test-support")]
        if let (Some(trace), Some(started)) = (&self.routing_trace, trace_started) {
            let elapsed = started.elapsed();
            trace.render_result(&result, elapsed, clut.is_some());
        }
        result
    }
}

impl State {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .map_err(|e| e.to_string())?;
        super::renderer_recovery::report_adapter(&adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;
        let health = super::renderer_recovery::Health::install(&device, None);
        let core = pollster::block_on(Qd3dCompute::new(device, queue, health.clone()))?;
        Ok(Self { health, core })
    }

    #[cfg(test)]
    fn render(&self, frame: &PpcQ3RasterFrame, background: &[u16]) -> Result<Vec<u16>, String> {
        self.render_with_palette(frame, background, None)
    }

    fn render_with_palette(
        &self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<Vec<u16>, String> {
        pollster::block_on(self.core.render_with_palette(frame, background, clut))
    }
}
impl Qd3dRenderer {
    fn prefers_native_frame(&self, frame: &PpcQ3RasterFrame) -> bool {
        if self.initialization_error.is_some() {
            return false;
        }
        let Qd3dWorkEstimate {
            work,
            gpu_work,
            threshold,
        } = Qd3dWorkEstimate::of(frame);
        #[cfg(feature = "test-support")]
        if let Some(trace) = &self.routing_trace {
            trace.work_estimate(work, gpu_work, threshold);
        }
        Qd3dWorkEstimate {
            work,
            gpu_work,
            threshold,
        }
        .prefers_gpu()
    }
}

impl PpcQ3GpuRenderer for Qd3dRenderer {
    fn prefers_native(&self, frame: &PpcQ3RasterFrame) -> bool {
        let selected = self.prefers_native_frame(frame);
        #[cfg(feature = "test-support")]
        if let Some(trace) = &self.routing_trace {
            trace.selection(frame, selected);
        }
        selected
    }

    fn render_indexed(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: &[[u16; 3]; 256],
    ) -> Result<Vec<u16>, String> {
        self.render_frame(frame, background, Some(clut))
    }

    fn render(&mut self, frame: &PpcQ3GpuFrame, background: &[u16]) -> Result<Vec<u16>, String> {
        self.render_native(&PpcQ3RasterFrame::from(frame), background)
    }

    fn render_native(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
    ) -> Result<Vec<u16>, String> {
        self.render_frame(frame, background, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::PpcMemory;
    use systemless::loader::ppc::*;
    use systemless::memory::GuestAddressSpace;

    #[test]
    fn gpu_failed_compute_device_retires_to_cpu_fallback() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        state
            .health
            .record(super::super::renderer_recovery::Failure::lost(
                "injected compute loss",
            ));
        let mut renderer = Qd3dRenderer::new();
        renderer.state = Some(state);
        let frame = PpcQ3RasterFrame {
            width: 1,
            height: 1,
            viewport: [0, 0, 0, 0],
            clear_color: None,
            textures: vec![],
            draws: vec![],
        };
        for _ in 0..2 {
            assert_eq!(
                renderer.render_frame(&frame, &[0], None).unwrap_err(),
                "injected compute loss"
            );
            assert!(renderer.state.is_none());
        }
    }

    #[test]
    fn gpu_readback_timeout_retires_compute_without_a_device_loss_callback() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let error = state
            .health
            .finish_readback(Err(wgpu::PollError::Timeout))
            .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(!state.health.check().unwrap_err().recoverable);
        let mut renderer = Qd3dRenderer::new();
        renderer.state = Some(state);
        let frame = PpcQ3RasterFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: Some([0., 0., 0., 1.]),
            textures: vec![],
            draws: vec![],
        };
        // Positive control: this workload normally selects the GPU.
        assert!(Qd3dRenderer::new().prefers_native(&frame));
        for _ in 0..2 {
            assert_eq!(
                renderer.render_frame(&frame, &[0], None).unwrap_err(),
                error
            );
            assert!(renderer.state.is_none());
            assert!(!renderer.prefers_native(&frame));
        }
    }

    #[test]
    fn gpu_readback_reuses_buffers_without_stale_pixels_after_resizing() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        for (index, width) in [64, 16, 128, 8].into_iter().enumerate() {
            let height = width * 3 / 4;
            let frame = PpcQ3RasterFrame {
                width,
                height,
                viewport: [0, 0, width as i32 - 1, height as i32 - 1],
                clear_color: (index == 2).then_some([0., 1., 0., 1.]),
                textures: vec![],
                draws: if index % 2 == 0 {
                    vec![triangle(0.5, [1., 0., 0., 1.])]
                } else {
                    vec![]
                },
            };
            let background: Vec<_> = (0..width * height)
                .map(|i| 0x8000 | (i % 32768) as u16)
                .collect();
            assert_eq!(
                state.render(&frame, &background).unwrap(),
                cpu_render(&frame, &background)
            );
        }
    }

    #[test]
    fn adaptive_dispatch_avoids_tiny_3d_work_in_large_framebuffers() {
        let renderer = Qd3dRenderer::new();
        let mut frame = PpcQ3RasterFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: None,
            textures: vec![],
            draws: vec![triangle(0.5, [1., 0., 0., 1.])],
        };
        assert!(!renderer.prefers_native(&frame));
        for v in &mut frame.draws[0].vertices {
            v.screen_x *= 20.;
            v.screen_y *= 20.;
        }
        assert!(renderer.prefers_native(&frame));
        frame.draws.clear();
        frame.clear_color = Some([0., 0., 0., 1.]);
        assert!(renderer.prefers_native(&frame));
        frame.viewport = [1, 1, 30, 20];
        assert!(!renderer.prefers_native(&frame));
        frame.width = 32;
        frame.height = 24;
        frame.viewport = [0, 0, 31, 23];
        assert!(!renderer.prefers_native(&frame));
    }

    fn sparse_mesh() -> PpcQ3RasterFrame {
        let mut draw = triangle(0.5, [1., 0., 0., 1.]);
        draw.vertices.clear();
        for y in 0..8 {
            for x in 0..8 {
                draw.vertices.extend([
                    vertex(
                        (x * 80 + 1) as f32,
                        (y * 60 + 1) as f32,
                        0.5,
                        [1., 0., 0., 1.],
                    ),
                    vertex(
                        (x * 80 + 79) as f32,
                        (y * 60 + 2) as f32,
                        0.5,
                        [1., 0., 0., 1.],
                    ),
                    vertex(
                        (x * 80 + 4) as f32,
                        (y * 60 + 58) as f32,
                        0.5,
                        [1., 0., 0., 1.],
                    ),
                ]);
            }
        }
        PpcQ3RasterFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: None,
            textures: vec![],
            draws: vec![draw],
        }
    }

    #[test]
    fn adaptive_dispatch_accepts_tiled_meshes_but_bounds_excessive_overlap() {
        let renderer = Qd3dRenderer::new();
        let mut frame = sparse_mesh();
        assert!(renderer.prefers_native(&frame));
        let mut full = triangle(0.5, [1., 0., 0., 1.]);
        full.vertices[0].screen_x = 0.;
        full.vertices[0].screen_y = 0.;
        full.vertices[1].screen_x = 639.;
        full.vertices[1].screen_y = 0.;
        full.vertices[2].screen_x = 0.;
        full.vertices[2].screen_y = 479.;
        frame.draws[0].vertices = full.vertices.repeat(2048);
        assert!(!renderer.prefers_native(&frame));
        full.vertices[0].screen_x = 0.;
        full.vertices[0].screen_y = 0.;
        full.vertices[1].screen_x = 300.;
        full.vertices[1].screen_y = 200.;
        full.vertices[2].screen_x = 600.;
        full.vertices[2].screen_y = 400.;
        frame.draws[0].vertices = full.vertices.repeat(64);
        // Software walks three edges, not the entire triangle bounds.
        assert!(!renderer.prefers_native(&frame));
    }

    #[test]
    fn gpu_degenerate_triangles_match_centroid_shading_and_repeated_edge_blends() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x4210; 32 * 24];
        for (dx, dy) in [
            (0., 0.),
            (8., 0.),
            (0., 8.),
            (8., 4.),
            (4., 8.),
            (-8., 4.),
            (-4., -8.),
        ] {
            for alpha in [1., 0.4] {
                for ambient in [false, true] {
                    let mut draw = triangle(0.9, [1., 0., 0., alpha]);
                    draw.ambient_light = ambient.then_some([0.7, 0.8, 0.9]);
                    draw.ambient_per_vertex = ambient;
                    draw.lighting = ambient.then_some(PpcQ3RasterLighting {
                        illumination_type: PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                        interpolation_style: PPC_Q3_INTERPOLATION_STYLE_VERTEX,
                        lights: vec![PpcQ3LightRecord {
                            light: 0,
                            light_type: 0,
                            data: PpcQ3LightData {
                                is_on: 1,
                                brightness: 0.4,
                                color: (0.8, 0.5, 0.7),
                            },
                            kind: PpcQ3LightKind::Directional {
                                direction: (0., 0., -1.),
                                casts_shadows: 0,
                            },
                        }],
                    });
                    draw.fog = Some(PpcQ3FogStyleData {
                        state: 1,
                        mode: 0,
                        fog_start: 0.,
                        fog_end: 2.,
                        density: 0.2,
                        color: (1., 0.2, 0.4, 0.6),
                    });
                    for (i, v) in draw.vertices.iter_mut().enumerate() {
                        v.screen_x = 16. + dx * (i as f32 - 1.);
                        v.screen_y = 12. + dy * (i as f32 - 1.);
                        v.color = [
                            [1., 0., 0., alpha],
                            [0., 1., 0., alpha],
                            [0., 0., 1., alpha],
                        ][i];
                        v.reciprocal_w = [1., 0.5, 0.25][i];
                        v.ambient_coefficient = [0.2, 0.6, 1.][i];
                        v.fog_depth = [0.2, 0.8, 1.4][i];
                        v.shading.normal = Some([0., 0., 1.]);
                    }
                    let frame = PpcQ3RasterFrame {
                        width: 32,
                        height: 24,
                        viewport: [10, 10, 20, 16],
                        clear_color: None,
                        textures: vec![],
                        draws: vec![triangle(0.1, [0., 0., 1., 1.]), draw],
                    };
                    let actual = state.render(&frame, &background).unwrap();
                    let expected = cpu_render(&frame, &background);
                    for (index, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                        for shift in [0, 5, 10] {
                            assert!((((actual>>shift)&31) as i32-((expected>>shift)&31) as i32).abs()<=1,
                                "degenerate ({dx},{dy}), alpha={alpha}, ambient={ambient}, pixel {index}: {actual:04x} != {expected:04x}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn tile_lists_preserve_order_and_bound_storage() {
        let mut primitive = vec![0u8; TRIANGLE_BYTES];
        for (v, (x, y)) in [(0., 0.), (31., 0.), (0., 31.)].into_iter().enumerate() {
            primitive[v * 128..v * 128 + 4].copy_from_slice(&f32::to_le_bytes(x));
            primitive[v * 128 + 4..v * 128 + 8].copy_from_slice(&f32::to_le_bytes(y));
        }
        let primitives = primitive.repeat(3);
        let bytes = raster_tiles(32, 32, &primitives, 1024).unwrap();
        let words: Vec<_> = bytes
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
            .collect();
        for header in words[..8].chunks_exact(2) {
            assert_eq!(header[1], 3);
            assert_eq!(
                &words[header[0] as usize..header[0] as usize + 3],
                &[0, 1, 2]
            );
        }
        assert!(raster_tiles(32, 32, &primitives, bytes.len() - 1).is_err());
    }

    #[test]
    fn gpu_tiles_accept_large_local_meshes_and_preserve_order() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let mut frame = sparse_mesh();
        // 1,792 primitives: the previous 640*480*N scan exceeded 512M.
        let original = frame.draws[0].vertices.clone();
        for _ in 0..27 {
            frame.draws[0].vertices.extend_from_slice(&original);
        }
        assert!(Qd3dRenderer::new().prefers_native(&frame));
        let background = vec![0x4210; (frame.width * frame.height) as usize];
        assert_eq!(
            state.render(&frame, &background).unwrap(),
            cpu_render(&frame, &background)
        );
    }

    #[test]
    #[ignore = "optimized tiled mesh benchmark; run alone"]
    fn benchmark_qd3d_tiled_mesh() {
        let state = State::new().expect("benchmark requires GPU");
        let mut frame = sparse_mesh();
        let original = frame.draws[0].vertices.clone();
        for _ in 0..27 {
            frame.draws[0].vertices.extend_from_slice(&original);
        }
        let background = vec![0x4210; (frame.width * frame.height) as usize];
        assert_eq!(
            state.render(&frame, &background).unwrap(),
            cpu_render(&frame, &background)
        );
        let adaptive = Qd3dRenderer::new();
        assert!(adaptive.prefers_native(&frame));
        let mut samples: [Vec<std::time::Duration>; 2] = std::array::from_fn(|_| Vec::new());
        for iteration in 0..103 {
            for offset in 0..2 {
                let index = (iteration + offset) % 2;
                let start = std::time::Instant::now();
                std::hint::black_box(if index == 0 {
                    cpu_render(&frame, &background)
                } else if adaptive.prefers_native(&frame) {
                    state.render(&frame, &background).unwrap()
                } else {
                    panic!("tiled mesh must reach GPU");
                });
                if iteration >= 3 {
                    samples[index].push(start.elapsed());
                }
            }
        }
        for (backend, samples) in ["default", "gpu-adaptive"].into_iter().zip(&mut samples) {
            crate::renderer_bench::report("3d-tiled-1792", backend, samples);
        }
    }

    #[test]
    #[ignore = "optimized synchronous 3D benchmark; run alone"]
    fn benchmark_qd3d_kernels() {
        let state = State::new().expect("benchmark requires a GPU");
        let mut cases = Vec::new();
        for (width, height) in [(64, 48), (320, 240), (640, 480)] {
            let mut draw = triangle(0.5, [1., 0., 0., 1.]);
            for v in &mut draw.vertices {
                v.screen_x *= width as f32 / 32.;
                v.screen_y *= height as f32 / 24.;
            }
            let frame = PpcQ3RasterFrame {
                width,
                height,
                viewport: [0, 0, width as i32 - 1, height as i32 - 1],
                clear_color: None,
                textures: vec![],
                draws: vec![draw],
            };
            cases.push((format!("3d-kernel-{width}x{height}"), frame));
        }
        cases.push(("3d-sparse-64".into(), sparse_mesh()));
        let mut overdraw = PpcQ3RasterFrame {
            width: 640,
            height: 480,
            viewport: [0, 0, 639, 479],
            clear_color: None,
            textures: vec![],
            draws: vec![],
        };
        for layer in 0..8 {
            let mut draw = triangle(0.9 - layer as f32 * 0.1, [1., 0., 0., 1.]);
            for v in &mut draw.vertices {
                v.screen_x *= 20.;
                v.screen_y *= 20.;
            }
            overdraw.draws.push(draw);
        }
        cases.push(("3d-overdraw-8".into(), overdraw));
        for (name, frame) in cases {
            let (width, height) = (frame.width, frame.height);
            let background = vec![0; (width * height) as usize];
            let expected = cpu_render(&frame, &background);
            assert!(
                state.render(&frame, &background).unwrap() == expected,
                "3D benchmark {name} differs from CPU"
            );
            let adaptive = Qd3dRenderer::new();
            let backends = ["default", "gpu", "gpu-adaptive"];
            let mut samples: [Vec<std::time::Duration>; 3] = std::array::from_fn(|_| Vec::new());
            // Rotate order within each repetition: CPU samples must not all
            // precede or follow a long forced-GPU batch. Include selection in
            // adaptive cost, as the real guest path does.
            for iteration in 0..33 {
                for offset in 0..3 {
                    let index = (iteration + offset) % 3;
                    let backend = backends[index];
                    let start = std::time::Instant::now();
                    let gpu = backend == "gpu"
                        || (backend == "gpu-adaptive" && adaptive.prefers_native(&frame));
                    std::hint::black_box(if gpu {
                        state.render(&frame, &background).unwrap()
                    } else {
                        cpu_render(&frame, &background)
                    });
                    if iteration >= 3 {
                        samples[index].push(start.elapsed());
                    }
                }
            }
            for (backend, samples) in backends.into_iter().zip(&mut samples) {
                crate::renderer_bench::report(&name, backend, samples);
            }
        }
    }

    fn vertex(x: f32, y: f32, z: f32, color: [f32; 4]) -> PpcQ3RasterVertex {
        PpcQ3RasterVertex {
            shading: PpcQ3RasterShading::default(),
            ambient_coefficient: 1.,
            fog_depth: 0.,
            vertex_alpha: None,
            screen_x: x,
            screen_y: y,
            depth: z,
            reciprocal_w: 1.,
            uv: [0., 0.],
            color,
        }
    }
    fn triangle(z: f32, color: [f32; 4]) -> PpcQ3RasterDraw {
        PpcQ3RasterDraw {
            primitive: PpcQ3RasterPrimitive::Triangles,
            test_depth: true,
            lighting: None,
            ambient_light: None,
            ambient_per_vertex: false,
            fog: None,
            texture: None,
            vertices: vec![
                vertex(1., 1., z, color),
                vertex(30., 2., z, color),
                vertex(4., 22., z, color),
            ],
            blend: color[3] < 1.,
            write_depth: color[3] == 1.,
        }
    }
    fn cpu_render(frame: &PpcQ3RasterFrame, background: &[u16]) -> Vec<u16> {
        cpu_render_target(frame, background, None)
    }
    fn cpu_render_target(
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<[[u16; 3]; 256]>,
    ) -> Vec<u16> {
        let mut memory = GuestAddressSpace::new();
        let mut bytes = Vec::new();
        for row in background.chunks(frame.width as usize) {
            if clut.is_some() {
                bytes.extend(row.iter().map(|p| *p as u8));
            } else {
                bytes.extend(row.iter().flat_map(|p| p.to_be_bytes()));
            }
            bytes.extend([0x55; 6]); // Row padding must never affect the image.
        }
        memory.add_region(0x100000, bytes);
        let front = PpcFrontBuffer {
            base_addr: 0x100000,
            width: frame.width,
            height: frame.height,
            depth: if clut.is_some() { 8 } else { 16 },
            row_bytes: frame.width * if clut.is_some() { 1 } else { 2 } + 6,
        };
        let surface = PpcQ3SoftwareFrontBufferSurface::new(&mut memory, front, clut);
        let mut depth = PpcQ3SoftwareDepthBuffer::new(front);
        let viewport = PpcQ3ViewportRect {
            left: frame.viewport[0] as u32,
            top: frame.viewport[1] as u32,
            right: frame.viewport[2] as u32 + 1,
            bottom: frame.viewport[3] as u32 + 1,
        };
        if let Some(c) = frame.clear_color {
            for y in viewport.top..viewport.bottom {
                for x in viewport.left..viewport.right {
                    assert!(surface.write_pixel(
                        &mut memory,
                        (x as i32, y as i32),
                        ppc_q3_rgb555((c[0], c[1], c[2]))
                    ));
                }
            }
        }
        let mut lights = PpcQ3SubmissionLightRecord {
            view: 0,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: 0,
            secondary: 0,
            light_group: 0,
            lights: vec![],
        };
        for draw in &frame.draws {
            lights.lights = draw
                .ambient_light
                .map(|color| {
                    vec![PpcQ3LightRecord {
                        light: 0,
                        light_type: 0,
                        data: PpcQ3LightData {
                            is_on: 1,
                            brightness: 1.,
                            color: (color[0], color[1], color[2]),
                        },
                        kind: PpcQ3LightKind::Ambient,
                    }]
                })
                .unwrap_or_default();
            if let Some(lighting) = &draw.lighting {
                lights.lights.extend(lighting.lights.iter().copied());
            }
            let material = PpcQ3SoftwareMaterialSample {
                diffuse: (1., 1., 1.),
                ambient_coefficient: 1.,
                specular_color: (0., 0., 0.),
                specular_control: 0.,
                highlight_state: draw.lighting.is_some(),
                transparency: if ppc_q3_software_material_uses_alpha_fog(draw.fog) {
                    let alpha = draw.vertices[0].color[3];
                    (alpha, alpha, alpha)
                } else {
                    (1., 1., 1.)
                },
                illumination_type: if let Some(lighting) = &draw.lighting {
                    lighting.illumination_type
                } else if draw.ambient_light.is_some() {
                    PPC_Q3_ILLUMINATION_TYPE_LAMBERT
                } else {
                    PPC_Q3_ILLUMINATION_TYPE_NULL
                },
                texture: None,
                uv_transform: None,
                shader_boundary: None,
                fog_style: draw.fog,
            };
            for vertices in draw.vertices.chunks_exact(3) {
                let points = std::array::from_fn(|i| {
                    let v = vertices[i];
                    PpcQ3SoftwareProjectedPoint {
                        x: v.screen_x.round() as i32,
                        y: v.screen_y.round() as i32,
                        raster_position: Some((v.screen_x, v.screen_y)),
                        z: v.depth,
                        reciprocal_w: v.reciprocal_w,
                        world: (v.shading.world[0], v.shading.world[1], v.shading.world[2]),
                        view_direction: v.shading.view_direction.map(|v| (v[0], v[1], v[2])),
                        fog_depth: v.fog_depth,
                        uv: None,
                        diffuse: Some((v.color[0], v.color[1], v.color[2])),
                        ambient_coefficient: Some(v.ambient_coefficient),
                        normal: v.shading.normal.map(|n| (n[0], n[1], n[2])),
                        specular_color: Some((
                            v.shading.specular_color[0],
                            v.shading.specular_color[1],
                            v.shading.specular_color[2],
                        )),
                        specular_control: Some(v.shading.specular_control),
                        highlight_state: Some(v.shading.highlight),
                        vertex_alpha: if ppc_q3_software_material_uses_alpha_fog(draw.fog) {
                            v.vertex_alpha
                        } else {
                            Some(v.color[3])
                        },
                    }
                });
                match draw.primitive {
                    PpcQ3RasterPrimitive::Points => {
                        if draw.write_depth {
                            ppc_q3_write_software_depth_material_pixel(
                                &mut memory,
                                surface,
                                &mut depth,
                                points[0],
                                &material,
                                &lights,
                            );
                        } else {
                            ppc_q3_draw_deferred_software_point(
                                &mut memory,
                                surface,
                                &mut depth,
                                points[0],
                                &material,
                                &lights,
                            );
                        }
                    }
                    PpcQ3RasterPrimitive::Lines => {
                        if draw.test_depth {
                            ppc_q3_draw_deferred_software_material_line(
                                &mut memory,
                                surface,
                                &depth,
                                points[0],
                                points[1],
                                &material,
                                &lights,
                            );
                        } else {
                            ppc_q3_draw_software_material_line(
                                &mut memory,
                                surface,
                                points[0],
                                points[1],
                                &material,
                                &lights,
                            );
                        }
                    }
                    PpcQ3RasterPrimitive::Triangles => {
                        ppc_q3_draw_software_triangle(
                            &mut memory,
                            surface,
                            &mut depth,
                            points,
                            &material,
                            &lights,
                            if let Some(lighting) = &draw.lighting {
                                lighting.interpolation_style
                            } else if draw.ambient_per_vertex {
                                PPC_Q3_INTERPOLATION_STYLE_VERTEX
                            } else {
                                PPC_Q3_INTERPOLATION_STYLE_PIXEL
                            },
                            viewport,
                            draw.write_depth,
                        );
                    }
                }
            }
        }
        let mut result = Vec::new();
        for y in 0..front.height {
            for x in 0..front.width {
                result.push(if clut.is_some() {
                    u16::from(
                        memory
                            .read_u8(front.base_addr + y * front.row_bytes + x)
                            .unwrap(),
                    )
                } else {
                    surface
                        .read_pixel(&mut memory, (x as i32, y as i32))
                        .unwrap()
                });
            }
        }
        result
    }

    #[test]
    fn gpu_indexed_blending_quantizes_each_primitive_and_preserves_exact_indices() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let mut clut = [[u16::MAX; 3]; 256];
        clut[7] = [0; 3];
        let mut degenerate = triangle(0.4, [1., 1., 1., 0.4]);
        degenerate.vertices[0].screen_x = 2.;
        degenerate.vertices[0].screen_y = 4.;
        degenerate.vertices[1].screen_x = 16.;
        degenerate.vertices[1].screen_y = 4.;
        degenerate.vertices[2].screen_x = 30.;
        degenerate.vertices[2].screen_y = 4.;
        let degenerate_frame = PpcQ3RasterFrame {
            width: 32,
            height: 24,
            viewport: [0, 0, 31, 23],
            clear_color: None,
            textures: vec![],
            draws: vec![degenerate],
        };
        let expected = cpu_render_target(&degenerate_frame, &vec![7; 32 * 24], Some(clut));
        let actual = state
            .render_with_palette(&degenerate_frame, &vec![0x8000; 32 * 24], Some(&clut))
            .unwrap();
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(if actual & 0x8000 != 0 { 7 } else { *actual }, expected);
        }

        let frame = PpcQ3RasterFrame {
            width: 32,
            height: 24,
            viewport: [0, 0, 31, 23],
            clear_color: None,
            textures: vec![],
            draws: vec![
                triangle(0.6, [1., 1., 1., 0.4]),
                triangle(0.4, [1., 1., 1., 0.4]),
            ],
        };
        let background = vec![0x8000; 32 * 24];
        let indexed_background = vec![7; background.len()];
        let expected = cpu_render_target(&frame, &indexed_background, Some(clut));
        let old_rgb = state.render(&frame, &background).unwrap();
        assert!(
            old_rgb[4 * 32 + 4] & 31 > 15,
            "old path accumulates gray past the palette midpoint"
        );
        let actual = state
            .render_with_palette(&frame, &background, Some(&clut))
            .unwrap();
        assert_eq!(actual[4 * 32 + 4], 7);
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(if actual & 0x8000 != 0 { 7 } else { *actual }, *expected);
        }
        // Use arbitrary 16-bit CLUT components, duplicate colors, and clear paths.
        for i in 0..256 {
            clut[i] = [
                ((i * 251) % 65536) as u16,
                ((i * 701) % 65536) as u16,
                ((i * 997) % 65536) as u16,
            ];
        }
        clut[7] = [0; 3];
        clut[200] = clut[30];
        for clear in [None, Some([0.23, 0.67, 0.45, 1.])] {
            let mut frame = frame.clone();
            frame.clear_color = clear;
            frame.draws[0] = triangle(0.6, [0.35, 0.61, 0.82, 0.4]);
            frame.draws[1] = triangle(0.4, [0.73, 0.22, 0.45, 0.4]);
            let expected = cpu_render_target(&frame, &indexed_background, Some(clut));
            let actual = state
                .render_with_palette(&frame, &background, Some(&clut))
                .unwrap();
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(if actual & 0x8000 != 0 { 7 } else { *actual }, *expected);
            }
        }
    }

    #[test]
    fn gpu_mixed_opacity_batches_preserve_slanted_opaque_depth_occlusion() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let mut opaque = triangle(0.1, [1., 0., 0., 1.]);
        opaque.vertices[1].depth = 0.9;
        opaque.vertices[2].depth = 0.9;
        // Avoid RGB555 half-step rounding ties in this depth/order regression.
        let translucent = triangle(0.51, [0., 1., 0., 0.4]);
        let background_mesh = triangle(0.99, [0., 0., 1., 1.]);
        let frame = PpcQ3RasterFrame {
            width: 32,
            height: 24,
            viewport: [0, 0, 31, 23],
            clear_color: None,
            textures: vec![],
            draws: vec![opaque.clone(), translucent.clone(), background_mesh.clone()],
        };
        let background = vec![0x8000; 32 * 24];
        let actual = state.render(&frame, &background).unwrap();
        // Software renders opaque primitives first, then sorted translucent ones.
        let mut software_frame = frame;
        software_frame.draws = vec![opaque, background_mesh, translucent];
        let expected = cpu_render(&software_frame, &background);
        assert_eq!(
            expected[4 * 32 + 4],
            0x7c00,
            "the near part of the red face must occlude green"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn gpu_qd3d_lines_match_bresenham_in_every_octant() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x801f; 32 * 24];
        for alpha in [1., 0.4] {
            for dx in -8..=8 {
                for dy in -8..=8 {
                    let mut draw = triangle(0.3, [1., 0.2, 0.4, alpha]);
                    draw.primitive = PpcQ3RasterPrimitive::Lines;
                    draw.test_depth = alpha < 1.;
                    draw.write_depth = false;
                    draw.vertices = vec![
                        vertex(16., 12., 0.3, [1., 0.2, 0.4, alpha]),
                        vertex(
                            (16 + dx) as f32,
                            (12 + dy) as f32,
                            0.6,
                            [1., 0.2, 0.4, alpha],
                        ),
                        vertex(16., 12., 0.45, [1., 0.2, 0.4, alpha]),
                    ];
                    let frame = PpcQ3RasterFrame {
                        width: 32,
                        height: 24,
                        viewport: [4, 4, 20, 18],
                        clear_color: None,
                        textures: vec![],
                        draws: vec![draw],
                    };
                    let actual = state.render(&frame, &background).unwrap();
                    let expected = cpu_render(&frame, &background);
                    for (i, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                        assert_eq!(
                            actual & 0x7fff,
                            expected & 0x7fff,
                            "dx={dx},dy={dy},alpha={alpha},pixel={i}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn gpu_qd3d_points_and_edges_preserve_depth_rules() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x801f; 32 * 24];
        for alpha in [1., 0.4] {
            for z in [0.2, 0.8] {
                let mut edge = triangle(z, [1., 0., 0., alpha]);
                edge.primitive = PpcQ3RasterPrimitive::Lines;
                edge.test_depth = alpha < 1.;
                edge.write_depth = false;
                edge.vertices = vec![
                    vertex(0., 8., z, [1., 0., 0., alpha]),
                    vertex(31., 8., z, [1., 0., 0., alpha]),
                    vertex(16., 8., z, [1., 0., 0., alpha]),
                ];
                let mut point = triangle(z, [0., 0., 1., alpha]);
                point.primitive = PpcQ3RasterPrimitive::Points;
                point.vertices = vec![vertex(8., 8., z, [0., 0., 1., alpha]); 3];
                let mut outside = point.clone();
                outside.vertices = vec![vertex(1., 1., z, [0., 0., 1., alpha]); 3];
                let frame = PpcQ3RasterFrame {
                    width: 32,
                    height: 24,
                    viewport: [2, 3, 28, 20],
                    clear_color: Some([0.1, 0.2, 0.3, 1.]),
                    textures: vec![],
                    draws: vec![triangle(0.5, [0., 1., 0., 1.]), edge, point, outside],
                };
                let actual = state.render(&frame, &background).unwrap();
                let expected = cpu_render(&frame, &background);
                assert_eq!(
                    actual[33], 0x001f,
                    "same-color point writes still carry coverage"
                );
                for (i, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                    for shift in [0, 5, 10] {
                        assert!(
                            (((actual >> shift) & 31) as i32 - ((expected >> shift) & 31) as i32)
                                .abs()
                                <= 1,
                            "alpha={alpha},z={z},pixel={i}: {actual:04x}!={expected:04x}"
                        );
                    }
                }
            }
        }
        // Signed zero depths compare equal, so transparent submission order
        // must stay stable across primitive kinds and depth serialization.
        let mut first = triangle(-0., [0., 0., 1., 0.4]);
        first.primitive = PpcQ3RasterPrimitive::Points;
        first.vertices = vec![vertex(8., 8., -0., [0., 0., 1., 0.4]); 3];
        let mut second = first.clone();
        second.vertices = vec![vertex(8., 8., 0., [1., 0., 0., 0.4]); 3];
        let frame = PpcQ3RasterFrame {
            width: 32,
            height: 24,
            viewport: [0, 0, 31, 23],
            clear_color: None,
            textures: vec![],
            draws: vec![first, second],
        };
        let actual = state.render(&frame, &background).unwrap();
        let expected = cpu_render(&frame, &background);
        assert_eq!(actual[8 * 32 + 8], expected[8 * 32 + 8]);
    }

    #[test]
    fn gpu_qd3d_directional_lighting_matches_software() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x801f; 32 * 24];
        for mixed_highlights in [false, true] {
            for interpolation_style in [0, 1, 2] {
                for illumination_type in [
                    PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                    PPC_Q3_ILLUMINATION_TYPE_PHONG,
                ] {
                    for control in [0., 8., 40.] {
                        for missing in [false, true] {
                            let mut draw = triangle(0.3, [0.65, 0.3, 0.2, 1.]);
                            draw.ambient_light = Some([0.1, 0.15, 0.2]);
                            draw.lighting = Some(PpcQ3RasterLighting {
                                interpolation_style,
                                illumination_type,
                                lights: vec![
                                    PpcQ3LightRecord {
                                        light: 0,
                                        light_type: 0,
                                        data: PpcQ3LightData {
                                            is_on: 1,
                                            brightness: 0.7,
                                            color: (0.8, 0.5, 0.3),
                                        },
                                        kind: PpcQ3LightKind::Directional {
                                            casts_shadows: 0,
                                            direction: (0.3, -0.4, -1.),
                                        },
                                    },
                                    PpcQ3LightRecord {
                                        light: 0,
                                        light_type: 0,
                                        data: PpcQ3LightData {
                                            is_on: 1,
                                            brightness: 0.4,
                                            color: (0.2, 0.5, 0.8),
                                        },
                                        kind: PpcQ3LightKind::Directional {
                                            casts_shadows: 0,
                                            direction: (-0.6, 0.2, -1.),
                                        },
                                    },
                                ],
                            });
                            for (i, v) in draw.vertices.iter_mut().enumerate() {
                                let j = if interpolation_style == 0 { 0 } else { i };
                                let n = ppc_q3_vector3d_normalized_value(
                                    [(0., 0., 1.), (0.4, 0.2, 1.), (-0.3, 0.5, 1.)][j],
                                )
                                .unwrap();
                                v.shading.normal = (!missing).then_some([n.0, n.1, n.2]);
                                v.shading.view_direction = Some([0.1, 0.2, 1.]);
                                v.shading.specular_color = [0.4, 0.6, 0.9];
                                v.shading.specular_control = control;
                                v.shading.highlight = !(mixed_highlights && j == 1);
                                v.reciprocal_w = [1., 0.5, 0.25][i];
                                v.ambient_coefficient = [0.2, 0.8, 0.5][j];
                            }
                            let frame = PpcQ3RasterFrame {
                                width: 32,
                                height: 24,
                                viewport: [0, 0, 31, 23],
                                clear_color: None,
                                textures: vec![],
                                draws: vec![draw],
                            };
                            let actual = state.render(&frame, &background).unwrap();
                            let expected = cpu_render(&frame, &background);
                            for (index, (&actual, &expected)) in
                                actual.iter().zip(&expected).enumerate()
                            {
                                for shift in [0, 5, 10] {
                                    assert!(
                                        (((actual >> shift) & 31) as i32
                                            - ((expected >> shift) & 31) as i32)
                                            .abs()
                                            <= 1,
                                        "pixel {index}, style={interpolation_style}, control={control}, missing={missing}: {actual:04x} != {expected:04x}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn gpu_qd3d_point_and_spot_lights_match_software() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x801f; 32 * 24];
        for interpolation_style in [0, 1, 2] {
            for illumination_type in [
                PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                PPC_Q3_ILLUMINATION_TYPE_PHONG,
            ] {
                for attenuation in 0..=2 {
                    for kind in 0..=4 {
                        for missing_normal in [false, true] {
                            let mut draw = triangle(0.3, [0.6, 0.5, 0.4, 1.]);
                            draw.ambient_light = Some([0.08, 0.05, 0.03]);
                            let location = (0.2, -0.1, 2.);
                            let light_kind = if kind == 0 {
                                PpcQ3LightKind::Point {
                                    casts_shadows: 0,
                                    attenuation,
                                    location,
                                }
                            } else {
                                PpcQ3LightKind::Spot {
                                    casts_shadows: 0,
                                    attenuation,
                                    location,
                                    direction: (0., 0., -1.),
                                    hot_angle: 0.2,
                                    outer_angle: 0.55,
                                    fall_off: kind - 1,
                                }
                            };
                            draw.lighting = Some(PpcQ3RasterLighting {
                                interpolation_style,
                                illumination_type,
                                lights: vec![PpcQ3LightRecord {
                                    light: 0,
                                    light_type: 0,
                                    kind: light_kind,
                                    data: PpcQ3LightData {
                                        is_on: 1,
                                        brightness: 0.65,
                                        color: (0.7, 0.4, 0.9),
                                    },
                                }],
                            });
                            for (i, v) in draw.vertices.iter_mut().enumerate() {
                                let j = if interpolation_style == 0 { 0 } else { i };
                                v.shading.world =
                                    [[-0.8, -0.5, 0.], [1.8, -0.3, 0.2], [0.3, 1.1, -0.4]][j];
                                v.shading.normal = (!missing_normal).then_some([0., 0., 1.]);
                                v.shading.view_direction = Some([0.1, 0.2, 1.]);
                                v.shading.highlight = true;
                                v.shading.specular_color = [0.4, 0.5, 0.6];
                                v.shading.specular_control = 8.;
                                v.reciprocal_w = [1., 0.5, 0.25][i];
                            }
                            let mut back = draw.clone();
                            back.lighting.as_mut().unwrap().lights[0].data.color = (0.2, 0.7, 0.3);
                            for v in &mut back.vertices {
                                v.screen_x += 7.;
                                v.screen_y += 5.;
                                v.depth = 0.6;
                            }
                            let frame = PpcQ3RasterFrame {
                                width: 32,
                                height: 24,
                                viewport: [0, 0, 31, 23],
                                clear_color: None,
                                textures: vec![],
                                draws: vec![draw, back],
                            };
                            let actual = state.render(&frame, &background).unwrap();
                            let expected = cpu_render(&frame, &background);
                            for (index, (&actual, &expected)) in
                                actual.iter().zip(&expected).enumerate()
                            {
                                for shift in [0, 5, 10] {
                                    assert!(
                                        (((actual >> shift) & 31) as i32
                                            - ((expected >> shift) & 31) as i32)
                                            .abs()
                                            <= 1,
                                        "pixel={index}, style={interpolation_style}, attenuation={attenuation}, kind={kind}, missing_normal={missing_normal}: {actual:04x} != {expected:04x}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // Coincident/overflowing positions and a zero spot axis contribute no
        // light, matching the CPU's undefined-direction handling.
        for (world, location, kind) in [
            ([0.; 3], (0., 0., 0.), 0),
            ([f32::MAX; 3], (0., 0., 0.), 0),
            ([0.; 3], (0., 0., 2.), 1),
        ] {
            let mut draw = triangle(0.3, [0.6, 0.5, 0.4, 1.]);
            let light_kind = if kind == 0 {
                PpcQ3LightKind::Point {
                    casts_shadows: 0,
                    attenuation: 0,
                    location,
                }
            } else {
                PpcQ3LightKind::Spot {
                    casts_shadows: 0,
                    attenuation: 0,
                    location,
                    direction: (0., 0., 0.),
                    hot_angle: 0.2,
                    outer_angle: 0.55,
                    fall_off: 1,
                }
            };
            draw.lighting = Some(PpcQ3RasterLighting {
                interpolation_style: 2,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                lights: vec![PpcQ3LightRecord {
                    light: 0,
                    light_type: 0,
                    kind: light_kind,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 1.,
                        color: (1., 1., 1.),
                    },
                }],
            });
            for v in &mut draw.vertices {
                v.shading.world = world;
            }
            let frame = PpcQ3RasterFrame {
                width: 32,
                height: 24,
                viewport: [0, 0, 31, 23],
                clear_color: None,
                textures: vec![],
                draws: vec![draw],
            };
            let actual = state.render(&frame, &background).unwrap();
            let expected = cpu_render(&frame, &background);
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(actual & 0x7fff, expected & 0x7fff);
            }
        }
    }

    #[test]
    fn gpu_qd3d_specular_texture_fog_and_alpha_order() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        for primitive in [
            PpcQ3RasterPrimitive::Triangles,
            PpcQ3RasterPrimitive::Lines,
            PpcQ3RasterPrimitive::Points,
        ] {
            for interpolation_style in [0, 1, 2] {
                let mut draw = triangle(0.3, [0.8, 0.4, 0.2, 1.]);
                draw.primitive = primitive;
                if primitive != PpcQ3RasterPrimitive::Triangles {
                    let a = vertex(1., 8., 0.3, [0.8, 0.4, 0.2, 1.]);
                    let b = vertex(30., 8., 0.3, [0.8, 0.4, 0.2, 1.]);
                    let c = vertex(8., 8., 0.3, [0.8, 0.4, 0.2, 1.]);
                    draw.vertices = if primitive == PpcQ3RasterPrimitive::Points {
                        vec![c; 3]
                    } else {
                        vec![a, b, c]
                    };
                }
                draw.texture = Some(0);
                draw.blend = true;
                draw.write_depth = false;
                draw.fog = Some(PpcQ3FogStyleData {
                    state: 1,
                    mode: 0,
                    fog_start: 0.,
                    fog_end: 2.,
                    density: 0.,
                    color: (1., 0.2, 0.4, 0.6),
                });
                draw.lighting = Some(PpcQ3RasterLighting {
                    interpolation_style,
                    illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                    lights: vec![PpcQ3LightRecord {
                        light: 0,
                        light_type: 0,
                        data: PpcQ3LightData {
                            is_on: 1,
                            brightness: 1.,
                            color: (1., 1., 1.),
                        },
                        kind: PpcQ3LightKind::Directional {
                            casts_shadows: 0,
                            direction: (0., 0., -1.),
                        },
                    }],
                });
                for v in &mut draw.vertices {
                    v.fog_depth = 1.;
                    v.shading = PpcQ3RasterShading {
                        highlight: true,
                        normal: Some([0., 0., 1.]),
                        view_direction: Some([0., 0., 1.]),
                        specular_color: [0.2; 3],
                        specular_control: 8.,
                        ..Default::default()
                    };
                }
                let frame = PpcQ3RasterFrame {
                    width: 32,
                    height: 24,
                    viewport: [0, 0, 31, 23],
                    clear_color: None,
                    textures: vec![PpcQ3GpuTexture {
                        width: 1,
                        height: 1,
                        rgba: vec![128, 255, 64, 128],
                        wrap_u: false,
                        wrap_v: false,
                    }],
                    draws: vec![draw],
                };
                let texture = [128. / 255., 1., 64. / 255.];
                let diffuse = [0.8, 0.4, 0.2];
                let fog = [0.2, 0.4, 0.6];
                let alpha = 128. / 255.;
                let expected = std::array::from_fn::<_, 3, _>(|i| {
                    let lit = if interpolation_style == 2
                        || primitive != PpcQ3RasterPrimitive::Triangles
                    {
                        diffuse[i] * texture[i] + 0.2
                    } else {
                        (diffuse[i] + 0.2) * texture[i]
                    };
                    (lit * 0.5 + fog[i] * 0.5) * alpha + if i == 2 { 1. - alpha } else { 0. }
                });
                let pixels = state.render(&frame, &vec![0x801f; 32 * 24]).unwrap();
                assert_eq!(
                    pixels[8 * 32 + 8],
                    ppc_q3_rgb555((expected[0], expected[1], expected[2])),
                    "style={interpolation_style}"
                );
            }
        }
    }

    #[test]
    fn gpu_qd3d_ambient_lighting_matches_software_interpolation() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let background = vec![0x8000; 32 * 24];
        for fog_enabled in [false, true] {
            for ambient_per_vertex in [false, true] {
                for light in [[0., 0., 0.], [0.8, 0.35, 0.6], [2., 1.5, 0.5]] {
                    let mut draw = triangle(0.3, [1., 1., 1., 1.]);
                    draw.fog = fog_enabled.then_some(PpcQ3FogStyleData {
                        state: 1,
                        mode: 0,
                        fog_start: 0.,
                        fog_end: 1.5,
                        density: 0.2,
                        color: (1., 0.2, 0.4, 0.6),
                    });
                    draw.ambient_light = Some(light);
                    draw.ambient_per_vertex = ambient_per_vertex;
                    for (i, vertex) in draw.vertices.iter_mut().enumerate() {
                        vertex.fog_depth = [0.2, 0.6, 1.3][i];
                        vertex.ambient_coefficient = [0.1, 0.65, 1.][i];
                        vertex.color = [
                            [0.9, 0.2, 0.1, 1.],
                            [0.2, 0.8, 0.4, 1.],
                            [0.1, 0.3, 0.9, 1.],
                        ][i];
                        vertex.reciprocal_w = [1., 0.5, 0.25][i];
                    }
                    let frame = PpcQ3RasterFrame {
                        width: 32,
                        height: 24,
                        viewport: [0, 0, 31, 23],
                        clear_color: None,
                        textures: vec![],
                        draws: vec![draw],
                    };
                    let actual = state.render(&frame, &background).unwrap();
                    let expected = cpu_render(&frame, &background);
                    for (index, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                        for shift in [0, 5, 10] {
                            assert!(
                                (((actual >> shift) & 31) as i32
                                    - ((expected >> shift) & 31) as i32)
                                    .abs()
                                    <= 1,
                                "pixel {index}, vertex={ambient_per_vertex}: {actual:04x} != {expected:04x}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn gpu_qd3d_readback_matches_software_depth_alpha_clear_and_viewport() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                eprintln!("SKIP QD3D readback: {error}");
                return;
            }
        };
        let background = vec![0x801f; 32 * 24];
        for clear_color in [None, Some([0.25, 0.5, 0.75, 1.])] {
            let frame = PpcQ3RasterFrame {
                width: 32,
                height: 24,
                viewport: [2, 3, 28, 20],
                clear_color,
                textures: vec![],
                draws: vec![
                    triangle(0.8, [1., 0., 0., 1.]),
                    triangle(0.2, [0., 1., 0., 1.]),
                    triangle(0.6, [1., 1., 1., 1.]),
                    triangle(0.1, [1., 0., 0., 0.4]),
                ],
            };
            let actual = state.render(&frame, &background).unwrap();
            let expected = cpu_render(&frame, &background);
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "pixel {index}");
            }
        }
    }

    #[test]
    fn gpu_qd3d_perspective_colors_match_cpu_quantization() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        for perspective in [false, true] {
            for reverse in [false, true] {
                let mut draw = triangle(0.2, [1., 0., 0., 1.]);
                draw.vertices[1].color = [0., 1., 0., 1.];
                draw.vertices[2].color = [0., 0., 1., 1.];
                if perspective {
                    draw.vertices[1].reciprocal_w = 0.5;
                    draw.vertices[2].reciprocal_w = 0.25;
                }
                if reverse {
                    draw.vertices.reverse();
                }
                let frame = PpcQ3RasterFrame {
                    width: 32,
                    height: 24,
                    viewport: [0, 0, 31, 23],
                    clear_color: None,
                    textures: vec![],
                    draws: vec![draw],
                };
                let background = vec![0x801f; 32 * 24];
                let actual = state.render(&frame, &background).unwrap();
                let expected = cpu_render(&frame, &background);
                for (i, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                    assert_eq!(actual & 0x8000, expected & 0x8000, "coverage at {i}");
                    for shift in [0, 5, 10] {
                        // Different f32 operation ordering may straddle one RGB555
                        // rounding boundary; coverage and the other channels agree.
                        assert!(
                            ((actual >> shift) & 31).abs_diff((expected >> shift) & 31) <= 1,
                            "perspective={perspective} reverse={reverse} pixel={i}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn gpu_qd3d_fog_matches_software_modes_and_alpha_semantics() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        for mode in 0..=3 {
            for zero_range in [false, true] {
                for alpha_present in [false, true] {
                    let mut draw = triangle(0.1, [0.8, 0.2, 0.1, 0.6]);
                    draw.fog = Some(PpcQ3FogStyleData {
                        state: 1,
                        mode,
                        fog_start: 1.,
                        fog_end: if zero_range { 1. } else { 5. },
                        density: 0.3,
                        color: (1., 0.1, 0.3, 0.9),
                    });
                    for (i, v) in draw.vertices.iter_mut().enumerate() {
                        v.fog_depth = [0.5, 3., 7.][i];
                        v.reciprocal_w = [1., 0.5, 0.25][i];
                        v.vertex_alpha = alpha_present.then_some([0.1, 0.5, 0.9][i]);
                    }
                    let frame = PpcQ3RasterFrame {
                        width: 32,
                        height: 24,
                        viewport: [2, 3, 28, 20],
                        clear_color: None,
                        textures: vec![],
                        draws: vec![draw],
                    };
                    let background = vec![0x801f; 32 * 24];
                    let actual = state.render(&frame, &background).unwrap();
                    let expected = cpu_render(&frame, &background);
                    for (i, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                        assert_eq!(actual & 0x8000, expected & 0x8000, "fog coverage at {i}");
                        for shift in [0, 5, 10] {
                            assert!(
                                ((actual >> shift) & 31).abs_diff((expected >> shift) & 31) <= 1,
                                "mode={mode} zero_range={zero_range} alpha_present={alpha_present} pixel={i}: {actual:04x} vs {expected:04x}"
                            );
                        }
                    }
                }
            }
        }
        // Texture alpha changes opacity, while alpha fog changes the color
        // factor. Both distance and alpha fog must run after texture sampling.
        for mode in [PPC_Q3_FOG_MODE_LINEAR, PPC_Q3_FOG_MODE_ALPHA] {
            let mut draw = triangle(0., [1.; 4]);
            draw.texture = Some(0);
            draw.blend = true;
            draw.write_depth = false;
            draw.fog = Some(PpcQ3FogStyleData {
                state: 1,
                mode,
                fog_start: 1.,
                fog_end: 5.,
                density: 0.3,
                color: (1., 0., 0., 1.),
            });
            for vertex in &mut draw.vertices {
                vertex.fog_depth = 3.;
                vertex.vertex_alpha = Some(0.5);
            }
            let frame = PpcQ3RasterFrame {
                width: 32,
                height: 24,
                viewport: [0, 0, 31, 23],
                clear_color: None,
                textures: vec![PpcQ3GpuTexture {
                    width: 1,
                    height: 1,
                    rgba: vec![255, 0, 0, 128],
                    wrap_u: false,
                    wrap_v: false,
                }],
                draws: vec![draw],
            };
            let actual = state.render(&frame, &vec![0x801f; 32 * 24]).unwrap();
            assert_eq!(
                actual[8 * 32 + 8],
                0x2017,
                "fog/texture alpha ordering for mode {mode}"
            );
        }
    }

    #[test]
    fn gpu_qd3d_rejects_invalid_packets_before_dispatch() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let frame = PpcQ3RasterFrame {
            width: 32,
            height: 24,
            viewport: [0, 0, 31, 23],
            clear_color: None,
            textures: vec![],
            draws: vec![triangle(0., [1.; 4])],
        };
        let background = vec![0; 32 * 24];
        let mut invalid = frame.clone();
        invalid.draws[0].texture = Some(0);
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].vertices[0].depth = f32::NAN;
        assert!(state.render(&invalid, &background).is_err());
        let mut thin = frame.clone();
        for (vertex, (x, y)) in
            thin.draws[0]
                .vertices
                .iter_mut()
                .zip([(0., 0.), (10000., 10001.), (10001., 10002.)])
        {
            vertex.screen_x = x;
            vertex.screen_y = y;
        }
        assert!(state
            .render(&thin, &background)
            .unwrap_err()
            .contains("area precision"));
        let mut invalid = frame.clone();
        invalid.draws[0].vertices.pop();
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].fog = Some(PpcQ3FogStyleData {
            state: 1,
            mode: 99,
            fog_start: 0.,
            fog_end: 1.,
            density: 0.2,
            color: (1., 1., 1., 1.),
        });
        assert!(state.render(&invalid, &background).is_err());
        invalid.draws[0].fog.as_mut().unwrap().mode = 0;
        invalid.draws[0].fog.as_mut().unwrap().density = f32::NAN;
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].ambient_light = Some([f32::NAN, 0., 0.]);
        assert!(state.render(&invalid, &background).is_err());
        invalid.draws[0].ambient_light = Some([1.; 3]);
        invalid.draws[0].vertices[0].ambient_coefficient = f32::INFINITY;
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].vertices[0].shading.normal = Some([f32::NAN, 0., 1.]);
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].lighting = Some(PpcQ3RasterLighting {
            lights: vec![],
            interpolation_style: 99,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        });
        assert!(state.render(&invalid, &background).is_err());
        for kind in [
            PpcQ3LightKind::Point {
                casts_shadows: 0,
                attenuation: 3,
                location: (0., 0., 1.),
            },
            PpcQ3LightKind::Point {
                casts_shadows: 0,
                attenuation: 0,
                location: (f32::NAN, 0., 1.),
            },
            PpcQ3LightKind::Spot {
                casts_shadows: 0,
                attenuation: 0,
                location: (0., 0., 1.),
                direction: (0., 0., -1.),
                hot_angle: 0.8,
                outer_angle: 0.2,
                fall_off: 0,
            },
            PpcQ3LightKind::Spot {
                casts_shadows: 0,
                attenuation: 0,
                location: (0., 0., 1.),
                direction: (0., 0., -1.),
                hot_angle: 0.2,
                outer_angle: 0.8,
                fall_off: 4,
            },
        ] {
            let mut invalid = frame.clone();
            invalid.draws[0].lighting = Some(PpcQ3RasterLighting {
                interpolation_style: 2,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                lights: vec![PpcQ3LightRecord {
                    light: 0,
                    light_type: 0,
                    kind,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 1.,
                        color: (1., 1., 1.),
                    },
                }],
            });
            assert!(state.render(&invalid, &background).is_err());
        }
        let mut invalid = frame.clone();
        invalid.draws[0].primitive = PpcQ3RasterPrimitive::Lines;
        invalid.draws[0].vertices[0].screen_x = 0.5;
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame;
        invalid.width = u32::MAX;
        invalid.height = u32::MAX;
        assert!(state.render(&invalid, &[]).is_err());
    }

    #[test]
    fn gpu_qd3d_textures_preserve_wrap_clamp_and_bottom_origin() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                eprintln!("SKIP QD3D texture readback: {error}");
                return;
            }
        };
        for wrap in [false, true] {
            for (uv, expected) in [
                ([0.25, 0.25], 0x001f),
                ([0.25, 0.75], 0x7c00),
                ([1.25, 0.75], if wrap { 0x7c00 } else { 0x03e0 }),
            ] {
                let mut draw = triangle(0., [1.; 4]);
                draw.texture = Some(0);
                for v in &mut draw.vertices {
                    v.uv = uv;
                }
                let frame = PpcQ3RasterFrame {
                    width: 32,
                    height: 24,
                    viewport: [0, 0, 31, 23],
                    clear_color: None,
                    textures: vec![PpcQ3GpuTexture {
                        width: 2,
                        height: 2,
                        rgba: vec![
                            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                        ],
                        wrap_u: wrap,
                        wrap_v: wrap,
                    }],
                    draws: vec![draw],
                };
                let pixels = state.render(&frame, &vec![0; 32 * 24]).unwrap();
                assert_eq!(pixels[8 * 32 + 8], expected, "wrap={wrap},uv={uv:?}");
            }
        }
    }
}
