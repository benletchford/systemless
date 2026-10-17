//! GPU rasterization with synchronous RGB555 readback for guest memory coherence.
use systemless::loader::ppc::{
    ppc_q3_vector3d_normalized_value, PpcQ3GpuFrame, PpcQ3GpuRenderer, PpcQ3LightKind,
    PpcQ3RasterFrame, PpcQ3RasterVertex, PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
    PPC_Q3_ILLUMINATION_TYPE_PHONG,
};
use wgpu::util::DeviceExt;

// Three 128-byte vertices and five 16-byte state vectors.
const TRIANGLE_BYTES: usize = 464;
const LIGHT_BYTES: usize = 64;

pub(super) struct Qd3dRenderer {
    state: Option<State>,
    initialization_error: Option<String>,
    last_fallback: Option<String>,
}
struct State {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
}

impl Qd3dRenderer {
    /// Adapter/device creation is deferred until a supported 3D frame arrives.
    pub fn new() -> Self {
        Self {
            state: None,
            initialization_error: None,
            last_fallback: None,
        }
    }
}

impl State {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .map_err(|e| e.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QD3D rasterizer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_qd3d.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QD3D RGB555"),
            layout: None,
            module: &shader,
            entry_point: Some("raster"),
            compilation_options: Default::default(),
            cache: None,
        });
        eprintln!("[GPU] QD3D rasterization: {}", adapter.get_info().name);
        Ok(Self {
            device,
            queue,
            pipeline,
        })
    }

    fn render(&self, frame: &PpcQ3RasterFrame, background: &[u16]) -> Result<Vec<u16>, String> {
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.render_inner(frame, background);
        let mut error = None;
        for scope in [validation, memory, internal] {
            if let Some(captured) = pollster::block_on(scope.pop()) {
                error = Some(captured.to_string());
            }
        }
        match error {
            Some(error) => Err(error),
            None => result,
        }
    }

    fn render_inner(
        &self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
    ) -> Result<Vec<u16>, String> {
        let count = u64::from(frame.width) * u64::from(frame.height);
        let pixel_bytes = count
            .checked_mul(4)
            .ok_or("GPU framebuffer size overflow")?;
        let limits = self.device.limits();
        if count == 0
            || count != background.len() as u64
            || pixel_bytes > limits.max_storage_buffer_binding_size.into()
            || frame.width.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || frame.height.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || frame.viewport[0] < 0
            || frame.viewport[1] < 0
            || frame.viewport[2] < frame.viewport[0]
            || frame.viewport[3] < frame.viewport[1]
            || frame.viewport[2] as u32 >= frame.width
            || frame.viewport[3] as u32 >= frame.height
        {
            return Err("invalid QD3D GPU framebuffer".into());
        }
        let mut texture_bytes = Vec::new();
        let mut textures = Vec::new();
        for texture in &frame.textures {
            let count = (u64::from(texture.width) * u64::from(texture.height))
                .checked_mul(4)
                .ok_or("GPU texture size overflow")?;
            if count == 0
                || count != texture.rgba.len() as u64
                || texture_bytes.len() as u64 + count
                    > limits.max_storage_buffer_binding_size.into()
            {
                return Err("invalid QD3D GPU texture".into());
            }
            textures.push((texture_bytes.len() as u32 / 4, texture));
            texture_bytes.extend_from_slice(&texture.rgba);
        }
        let mut light_bytes = Vec::new();
        let mut lighting_work = 1u64;
        let mut triangles = Vec::new();
        let mut deferred = Vec::new();
        for draw in &frame.draws {
            if draw.vertices.len() % 3 != 0 {
                return Err("incomplete GPU triangle".into());
            }
            let (offset, width, height, wrap) = match draw.texture {
                Some(index) => {
                    let (offset, texture) =
                        textures.get(index).ok_or("invalid GPU texture reference")?;
                    (
                        *offset,
                        texture.width,
                        texture.height,
                        (u32::from(texture.wrap_u) << 1) | (u32::from(texture.wrap_v) << 2),
                    )
                }
                None => (0, 0, 0, 0),
            };
            if draw
                .ambient_light
                .is_some_and(|light| !light.into_iter().all(f32::is_finite))
            {
                return Err("invalid GPU ambient light".into());
            }
            let light_offset = u32::try_from(light_bytes.len() / LIGHT_BYTES)
                .map_err(|_| "GPU light offset overflow")?;
            let mut light_info = [light_offset, 0, 0, 0];
            if let Some(lighting) = &draw.lighting {
                if lighting.interpolation_style > 2
                    || ![
                        PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                        PPC_Q3_ILLUMINATION_TYPE_PHONG,
                    ]
                    .contains(&lighting.illumination_type)
                    || lighting.lights.len() > 64
                {
                    return Err("unsupported GPU lighting state".into());
                }
                light_info[1] = lighting.lights.len() as u32;
                light_info[2] = lighting.interpolation_style;
                light_info[3] =
                    u32::from(lighting.illumination_type == PPC_Q3_ILLUMINATION_TYPE_PHONG);
                lighting_work = lighting_work.max(1 + lighting.lights.len() as u64);
                for light in &lighting.lights {
                    let (direction, location, kind, parameters) = match light.kind {
                        PpcQ3LightKind::Directional { direction, .. } => {
                            (direction, (0., 0., 0.), 0., [0.; 4])
                        }
                        PpcQ3LightKind::Point {
                            attenuation,
                            location,
                            ..
                        } if attenuation <= 2 => {
                            ((0., 0., 0.), location, 1., [attenuation as f32, 0., 0., 0.])
                        }
                        PpcQ3LightKind::Spot {
                            attenuation,
                            location,
                            direction,
                            hot_angle,
                            outer_angle,
                            fall_off,
                            ..
                        } if attenuation <= 2
                            && fall_off <= 3
                            && hot_angle >= 0.
                            && outer_angle >= hot_angle =>
                        {
                            (
                                direction,
                                location,
                                2.,
                                [attenuation as f32, hot_angle, outer_angle, fall_off as f32],
                            )
                        }
                        _ => return Err("unsupported GPU light kind or parameters".into()),
                    };
                    let values = [
                        direction.0,
                        direction.1,
                        direction.2,
                        light.data.brightness,
                        light.data.color.0,
                        light.data.color.1,
                        light.data.color.2,
                    ];
                    if !values
                        .into_iter()
                        .chain([location.0, location.1, location.2])
                        .chain(parameters)
                        .all(f32::is_finite)
                        || light.data.is_on == 0
                    {
                        return Err("invalid GPU light".into());
                    }
                    let direction = ppc_q3_vector3d_normalized_value(direction);
                    let d = direction.unwrap_or((0., 0., 0.));
                    for value in [
                        d.0,
                        d.1,
                        d.2,
                        if direction.is_some() { 1. } else { 0. },
                        light.data.color.0,
                        light.data.color.1,
                        light.data.color.2,
                        light.data.brightness,
                        location.0,
                        location.1,
                        location.2,
                        kind,
                        parameters[0],
                        parameters[1],
                        parameters[2],
                        parameters[3],
                    ] {
                        light_bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
            let fog = draw.fog.filter(|fog| fog.state != 0);
            if let Some(fog) = fog {
                if fog.state != 1
                    || fog.mode > 3
                    || ![
                        fog.fog_start,
                        fog.fog_end,
                        fog.density,
                        fog.color.0,
                        fog.color.1,
                        fog.color.2,
                        fog.color.3,
                    ]
                    .into_iter()
                    .all(f32::is_finite)
                {
                    return Err("invalid GPU fog state".into());
                }
            }
            for triangle in draw.vertices.chunks_exact(3) {
                if triangle.iter().any(|v| !vertex_valid(v)) {
                    return Err("invalid projected GPU vertex".into());
                }
                let area = (triangle[2].screen_x - triangle[0].screen_x)
                    * (triangle[1].screen_y - triangle[0].screen_y)
                    - (triangle[2].screen_y - triangle[0].screen_y)
                        * (triangle[1].screen_x - triangle[0].screen_x);
                if area == 0. || !area.is_finite() {
                    return Err("degenerate GPU triangle requires software line rendering".into());
                }
                let mut bytes = Vec::with_capacity(TRIANGLE_BYTES);
                for v in triangle {
                    for value in [
                        v.screen_x,
                        v.screen_y,
                        v.depth,
                        v.reciprocal_w,
                        v.uv[0],
                        v.uv[1],
                        v.fog_depth,
                        v.vertex_alpha.unwrap_or(-1.0),
                        v.color[0],
                        v.color[1],
                        v.color[2],
                        v.color[3],
                        v.ambient_coefficient,
                        if v.shading.highlight { 1. } else { 0. },
                        0.,
                        0.,
                        v.shading.normal.unwrap_or([0.; 3])[0],
                        v.shading.normal.unwrap_or([0.; 3])[1],
                        v.shading.normal.unwrap_or([0.; 3])[2],
                        if v.shading.normal.is_some() { 1. } else { 0. },
                        v.shading.view_direction.unwrap_or([0.; 3])[0],
                        v.shading.view_direction.unwrap_or([0.; 3])[1],
                        v.shading.view_direction.unwrap_or([0.; 3])[2],
                        if v.shading.view_direction.is_some() {
                            1.
                        } else {
                            0.
                        },
                        v.shading.specular_color[0],
                        v.shading.specular_color[1],
                        v.shading.specular_color[2],
                        v.shading.specular_control,
                        v.shading.world[0],
                        v.shading.world[1],
                        v.shading.world[2],
                        0.,
                    ] {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
                for value in [offset, width, height, wrap | u32::from(draw.write_depth)] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                let fog_values = fog
                    .map(|fog| {
                        [
                            fog.color.1,
                            fog.color.2,
                            fog.color.3,
                            (fog.mode + 1) as f32,
                            fog.fog_start,
                            fog.fog_end,
                            fog.density,
                            0.0,
                        ]
                    })
                    .unwrap_or([0.0; 8]);
                for value in fog_values {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                let ambient = draw
                    .ambient_light
                    .map(|light| {
                        [
                            light[0],
                            light[1],
                            light[2],
                            if draw.ambient_per_vertex { 1. } else { 2. },
                        ]
                    })
                    .unwrap_or([0.; 4]);
                for value in ambient {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                for value in light_info {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                if draw.blend {
                    deferred.push((triangle.iter().map(|v| v.depth / 3.).sum::<f32>(), bytes));
                } else {
                    triangles.extend(bytes);
                }
            }
        }
        deferred.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, bytes) in deferred {
            triangles.extend(bytes);
        }
        let triangle_count = triangles.len() / TRIANGLE_BYTES;
        if triangles.len() as u64 > limits.max_storage_buffer_binding_size.into()
            || count
                .saturating_mul(triangle_count as u64)
                .saturating_mul(lighting_work)
                > 512_000_000
        {
            return Err("QD3D GPU frame exceeds raster work limits".into());
        }
        if light_bytes.len() as u64 > limits.max_storage_buffer_binding_size.into() {
            return Err("GPU light storage exceeds limits".into());
        }
        if light_bytes.is_empty() {
            light_bytes.resize(LIGHT_BYTES, 0);
        }
        if triangles.is_empty() {
            triangles.resize(TRIANGLE_BYTES, 0);
        }
        if texture_bytes.is_empty() {
            texture_bytes.resize(4, 255);
        }
        let buffer = |bytes: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("QD3D frame"),
                    contents: bytes,
                    usage,
                })
        };
        let triangle_buffer = buffer(&triangles, wgpu::BufferUsages::STORAGE);
        let texture_buffer = buffer(&texture_bytes, wgpu::BufferUsages::STORAGE);
        let light_buffer = buffer(&light_bytes, wgpu::BufferUsages::STORAGE);
        let bytes: Vec<u8> = background
            .iter()
            .flat_map(|p| u32::from(*p).to_le_bytes())
            .collect();
        let pixels = buffer(
            &bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let clear = frame
            .clear_color
            .map(|c| u32::from(systemless::loader::ppc::ppc_q3_rgb555((c[0], c[1], c[2]))))
            .unwrap_or(0);
        let words = [
            frame.width,
            frame.height,
            triangle_count as u32,
            0,
            frame.viewport[0] as u32,
            frame.viewport[1] as u32,
            frame.viewport[2] as u32,
            frame.viewport[3] as u32,
            u32::from(frame.clear_color.is_some()),
            clear,
            0,
            0,
        ];
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let geometry = buffer(&bytes, wgpu::BufferUsages::UNIFORM);
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("QD3D frame"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: triangle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: texture_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pixels.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: geometry.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QD3D guest readback"),
            size: pixel_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(frame.width.div_ceil(8), frame.height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&pixels, 0, &readback, 0, pixel_bytes);
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        Ok(bytes
            .chunks_exact(4)
            .map(|p| u32::from_le_bytes(p.try_into().unwrap()) as u16)
            .collect())
    }
}
fn vertex_valid(v: &PpcQ3RasterVertex) -> bool {
    [
        v.ambient_coefficient,
        v.screen_x,
        v.screen_y,
        v.depth,
        v.fog_depth,
        v.vertex_alpha.unwrap_or(1.0),
        v.reciprocal_w,
        v.uv[0],
        v.uv[1],
        v.color[0],
        v.color[1],
        v.color[2],
        v.color[3],
    ]
    .into_iter()
    .chain(v.shading.normal.unwrap_or([0.; 3]))
    .chain(v.shading.view_direction.unwrap_or([0.; 3]))
    .chain(v.shading.specular_color)
    .chain(v.shading.world)
    .chain([v.shading.specular_control])
    .all(f32::is_finite)
        && v.vertex_alpha
            .is_none_or(|alpha| (0.0..=1.0).contains(&alpha))
        && v.reciprocal_w > 0.
        && v.screen_x.abs() <= 32768.
        && v.screen_y.abs() <= 32768.
}
impl PpcQ3GpuRenderer for Qd3dRenderer {
    fn render(&mut self, frame: &PpcQ3GpuFrame, background: &[u16]) -> Result<Vec<u16>, String> {
        self.render_native(&PpcQ3RasterFrame::from(frame), background)
    }

    fn render_native(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
    ) -> Result<Vec<u16>, String> {
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render(frame, background),
            None => Err(self.initialization_error.as_ref().unwrap().clone()),
        };
        if let Err(error) = &result {
            if self.last_fallback.as_ref() != Some(error) {
                eprintln!("[GPU] QD3D software fallback: {error}");
                self.last_fallback = Some(error.clone());
            }
        } else {
            self.last_fallback = None;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::loader::ppc::*;
    use systemless::memory::GuestAddressSpace;

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
        let mut memory = GuestAddressSpace::new();
        let mut bytes = Vec::new();
        for row in background.chunks(frame.width as usize) {
            bytes.extend(row.iter().flat_map(|p| p.to_be_bytes()));
            bytes.extend([0x55; 6]); // Row padding must never affect the image.
        }
        memory.add_region(0x100000, bytes);
        let front = PpcFrontBuffer {
            base_addr: 0x100000,
            width: frame.width,
            height: frame.height,
            depth: 16,
            row_bytes: frame.width * 2 + 6,
        };
        let surface = PpcQ3SoftwareFrontBufferSurface::new(&mut memory, front, None);
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
        let mut result = Vec::new();
        for y in 0..front.height {
            for x in 0..front.width {
                result.push(
                    surface
                        .read_pixel(&mut memory, (x as i32, y as i32))
                        .unwrap(),
                );
            }
        }
        result
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
                                    assert!((((actual>>shift)&31) as i32-((expected>>shift)&31) as i32).abs()<=1,"pixel {index}, style={interpolation_style}, control={control}, missing={missing}: {actual:04x} != {expected:04x}");
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
                                    assert!((((actual>>shift)&31) as i32-((expected>>shift)&31) as i32).abs()<=1,"pixel={index}, style={interpolation_style}, attenuation={attenuation}, kind={kind}, missing_normal={missing_normal}: {actual:04x} != {expected:04x}");
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
        for interpolation_style in [0, 1, 2] {
            let mut draw = triangle(0.3, [0.8, 0.4, 0.2, 1.]);
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
                let lit = if interpolation_style == 2 {
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
                            assert!((((actual>>shift)&31) as i32 - ((expected>>shift)&31) as i32).abs() <= 1, "pixel {index}, vertex={ambient_per_vertex}: {actual:04x} != {expected:04x}");
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
                            assert!(((actual>>shift)&31).abs_diff((expected>>shift)&31)<=1,"mode={mode} zero_range={zero_range} alpha_present={alpha_present} pixel={i}: {actual:04x} vs {expected:04x}");
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
        let mut invalid = frame.clone();
        invalid.draws[0].vertices.pop();
        assert!(state.render(&invalid, &background).is_err());
        let mut invalid = frame.clone();
        invalid.draws[0].vertices.fill(vertex(1., 1., 0., [1.; 4]));
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
