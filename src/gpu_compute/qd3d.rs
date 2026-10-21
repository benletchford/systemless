//! QD3D rasterization with RGB555 readback for guest memory coherence.
use super::routing::{qd3d_triangle_area, MAX_RASTER_WORK, TILE_SIZE};
use super::{read_mapped, BindingRole, ComputeHealth, Scopes, ScratchBuffers};
use crate::loader::ppc::{
    ppc_q3_software_interpolate_projected_point, ppc_q3_software_perspective_weights,
    ppc_q3_vector3d_normalized_value, PpcQ3LightKind, PpcQ3RasterFrame, PpcQ3RasterPrimitive,
    PpcQ3RasterVertex, PpcQ3SoftwareProjectedPoint, PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
    PPC_Q3_ILLUMINATION_TYPE_PHONG,
};
use std::sync::Arc;

// Three 128-byte vertices and five 16-byte state vectors.
#[doc(hidden)]
pub const TRIANGLE_BYTES: usize = 464;
const LIGHT_BYTES: usize = 64;

/// The QD3D rasterizer on one wgpu device.
pub struct Qd3dCompute {
    health: Arc<dyn ComputeHealth>,
    pub buffers: ScratchBuffers,
    pub device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    palette: std::cell::RefCell<Option<([[u16; 3]; 256], wgpu::Buffer)>>,
}

impl Qd3dCompute {
    pub async fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        health: Arc<dyn ComputeHealth>,
    ) -> Result<Self, String> {
        let scopes = Scopes::push(&device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QD3D rasterizer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("qd3d.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QD3D RGB555"),
            layout: None,
            module: &shader,
            entry_point: Some("raster"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = scopes.pop_unrecorded().await {
            return Err(error);
        }
        eprintln!("[GPU] QD3D rasterization: {}", device.adapter_info().name);
        health.check()?;
        Ok(Self {
            health,
            buffers: Default::default(),
            device,
            queue,
            pipeline,
            palette: Default::default(),
        })
    }

    /// Render one completed frame. Indexed targets pass their live palette;
    /// painted cells then hold palette indices, with bit 15 marking untouched
    /// background cells.
    pub async fn render_with_palette(
        &self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<Vec<u16>, String> {
        let scopes = Scopes::push(&self.device);
        let result = self.render_inner(frame, background, clut).await;
        match scopes.pop(&*self.health).await {
            Some(error) => Err(error),
            None => result,
        }
    }

    async fn render_inner(
        &self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<Vec<u16>, String> {
        self.health.check()?;
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
        let mut triangles = Vec::new();
        let mut deferred = Vec::new();
        for draw in &frame.draws {
            if draw.vertices.len() % 3 != 0 {
                return Err("incomplete GPU primitive triple".into());
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
                if draw.primitive != PpcQ3RasterPrimitive::Triangles
                    && triangle
                        .iter()
                        .any(|v| v.screen_x.fract() != 0. || v.screen_y.fract() != 0.)
                {
                    return Err("GPU point and line coordinates must be integral".into());
                }
                let area = (triangle[2].screen_x - triangle[0].screen_x)
                    * (triangle[1].screen_y - triangle[0].screen_y)
                    - (triangle[2].screen_y - triangle[0].screen_y)
                        * (triangle[1].screen_x - triangle[0].screen_x);
                // Preserve the software rasterizer's double-precision classification.
                let precise_area = qd3d_triangle_area(triangle);
                let degenerate =
                    draw.primitive == PpcQ3RasterPrimitive::Triangles && precise_area == 0.;
                if draw.primitive == PpcQ3RasterPrimitive::Triangles
                    && (!area.is_finite() || (!degenerate && area == 0.))
                {
                    return Err("GPU triangle area precision requires software rendering".into());
                }
                let sort_depth = match draw.primitive {
                    PpcQ3RasterPrimitive::Triangles => {
                        (triangle[0].depth + triangle[1].depth + triangle[2].depth) / 3.
                    }
                    PpcQ3RasterPrimitive::Lines => (triangle[0].depth + triangle[1].depth) * 0.5,
                    PpcQ3RasterPrimitive::Points => triangle[0].depth,
                };
                let vertices: [PpcQ3RasterVertex; 3] = triangle.try_into().unwrap();
                let edges = degenerate.then(|| degenerate_edges(vertices));
                let packets = edges
                    .as_ref()
                    .map_or(std::slice::from_ref(&vertices), |edges| edges.as_slice());
                let primitive = if degenerate {
                    PpcQ3RasterPrimitive::Lines
                } else {
                    draw.primitive
                };
                for triangle in packets {
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
                    for value in [
                        offset,
                        width,
                        height,
                        wrap | u32::from(draw.write_depth && !degenerate)
                            | (u32::from(!draw.test_depth || degenerate) << 3)
                            | ((primitive as u32) << 4),
                    ] {
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
                        deferred.push((sort_depth, bytes));
                    } else {
                        triangles.extend(bytes);
                    }
                }
            }
        }
        deferred.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, bytes) in deferred {
            triangles.extend(bytes);
        }
        let triangle_count = triangles.len() / TRIANGLE_BYTES;
        if triangles.len() as u64 > limits.max_storage_buffer_binding_size.into() {
            return Err("QD3D GPU primitive storage exceeds limits".into());
        }
        // Build lists after transparency sorting: each tile retains the exact
        // primitive order, including repeated alpha blends at shared edges.
        let tiles = raster_tiles(
            frame.width,
            frame.height,
            &triangles,
            limits.max_storage_buffer_binding_size as usize,
        )?;
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
        let buffer = |role, bytes: &[u8], usage| {
            self.buffers
                .upload(&self.device, &self.queue, role, bytes, usage)
        };
        let triangle_buffer = buffer("3d-triangles", &triangles, wgpu::BufferUsages::STORAGE);
        let texture_buffer = buffer("3d-textures", &texture_bytes, wgpu::BufferUsages::STORAGE);
        let tile_buffer = buffer("3d-tiles", &tiles, wgpu::BufferUsages::STORAGE);
        let light_buffer = buffer("3d-lights", &light_bytes, wgpu::BufferUsages::STORAGE);
        // Live guest palettes can change between frames. Cache by all components,
        // avoiding 32K nearest-color searches on each indexed animation frame.
        // The cache borrow ends here, before any readback await.
        let empty_palette = buffer("3d-empty-palette", &[0; 4], wgpu::BufferUsages::STORAGE);
        let palette_buffer = match clut {
            Some(clut) => {
                let mut palette_cache = self.palette.borrow_mut();
                if palette_cache
                    .as_ref()
                    .is_none_or(|(cached, _)| cached != clut)
                {
                    let bytes: Vec<u8> = crate::loader::ppc::ppc_q3_gpu_palette_lookup(clut)
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect();
                    *palette_cache = Some((
                        *clut,
                        buffer("3d-palette", &bytes, wgpu::BufferUsages::STORAGE),
                    ));
                }
                palette_cache.as_ref().unwrap().1.clone()
            }
            None => empty_palette,
        };
        let bytes: Vec<u8> = background
            .iter()
            .flat_map(|p| u32::from(*p).to_le_bytes())
            .collect();
        let pixels = buffer(
            "3d-pixels",
            &bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let clear = frame
            .clear_color
            .map(|c| u32::from(crate::loader::ppc::ppc_q3_rgb555((c[0], c[1], c[2]))))
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
            u32::from(clut.is_some()),
            0,
        ];
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let geometry = buffer("3d-geometry", &bytes, wgpu::BufferUsages::UNIFORM);
        let group = self.buffers.bind_group(
            &self.device,
            if clut.is_some() {
                BindingRole::Qd3dIndexed
            } else {
                BindingRole::Qd3d
            },
            &self.pipeline,
            &[
                (0, &triangle_buffer),
                (1, &texture_buffer),
                (2, &pixels),
                (3, &geometry),
                (4, &light_buffer),
                (5, &palette_buffer),
                (6, &tile_buffer),
            ],
        );
        let readback = self.buffers.get(
            &self.device,
            "3d-readback",
            pixel_bytes,
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(frame.width.div_ceil(8), frame.height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&pixels, 0, &readback, 0, pixel_bytes);
        self.queue.submit([encoder.finish()]);
        read_mapped(
            &*self.health,
            &self.device,
            &readback,
            pixel_bytes,
            |bytes| {
                bytes
                    .chunks_exact(4)
                    .map(|p| u32::from_le_bytes(p.try_into().unwrap()) as u16)
                    .collect()
            },
        )
        .await
    }
}

/// Reuse the tested line kernel. Keeping edge expansion on the host avoids
/// adding divergent coverage/blend loops to every regular triangle invocation.
fn degenerate_edges(vertices: [PpcQ3RasterVertex; 3]) -> [[PpcQ3RasterVertex; 3]; 3] {
    let points = vertices.map(|v| PpcQ3SoftwareProjectedPoint {
        x: v.screen_x.round() as i32,
        y: v.screen_y.round() as i32,
        raster_position: Some((v.screen_x, v.screen_y)),
        z: v.depth,
        reciprocal_w: v.reciprocal_w,
        world: (v.shading.world[0], v.shading.world[1], v.shading.world[2]),
        view_direction: v.shading.view_direction.map(|v| (v[0], v[1], v[2])),
        fog_depth: v.fog_depth,
        uv: Some((v.uv[0], v.uv[1])),
        diffuse: Some((v.color[0], v.color[1], v.color[2])),
        ambient_coefficient: Some(v.ambient_coefficient),
        normal: v.shading.normal.map(|v| (v[0], v[1], v[2])),
        specular_color: Some((
            v.shading.specular_color[0],
            v.shading.specular_color[1],
            v.shading.specular_color[2],
        )),
        specular_control: Some(v.shading.specular_control),
        highlight_state: Some(v.shading.highlight),
        vertex_alpha: v.vertex_alpha,
    });
    let point = ppc_q3_software_interpolate_projected_point(&points, [1. / 3.; 3], 0, 0);
    let weights = ppc_q3_software_perspective_weights(&points, [1. / 3.; 3]);
    let mut sample = vertices[0];
    sample.depth = point.z;
    sample.reciprocal_w = 1.;
    let uv = point.uv.unwrap();
    sample.uv = [uv.0, uv.1];
    let color = point.diffuse.unwrap();
    sample.color = [
        color.0,
        color.1,
        color.2,
        weights[0].mul_add(
            vertices[0].color[3],
            weights[1].mul_add(vertices[1].color[3], weights[2] * vertices[2].color[3]),
        ),
    ];
    sample.ambient_coefficient = point.ambient_coefficient.unwrap();
    sample.fog_depth = point.fog_depth;
    sample.vertex_alpha = point.vertex_alpha;
    sample.shading.normal = point.normal.map(|v| [v.0, v.1, v.2]);
    sample.shading.view_direction = point.view_direction.map(|v| [v.0, v.1, v.2]);
    sample.shading.world = [point.world.0, point.world.1, point.world.2];
    let specular = point.specular_color.unwrap();
    sample.shading.specular_color = [specular.0, specular.1, specular.2];
    sample.shading.specular_control = point.specular_control.unwrap();
    sample.shading.highlight = point.highlight_state.unwrap();
    [(0, 1), (1, 2), (2, 0)].map(|(a, b)| {
        let mut packet = [sample; 3];
        packet[0].screen_x = vertices[a].screen_x.round();
        packet[0].screen_y = vertices[a].screen_y.round();
        packet[1].screen_x = vertices[b].screen_x.round();
        packet[1].screen_y = vertices[b].screen_y.round();
        packet[2].screen_x = packet[0].screen_x;
        packet[2].screen_y = packet[0].screen_y;
        packet
    })
}

/// Conservative screen bounds; explicit lines use rounded integer coverage.
fn primitive_bounds(bytes: &[u8], width: u32, height: u32) -> [u32; 4] {
    let coordinate = |vertex: usize, axis: usize| {
        f32::from_le_bytes(
            bytes[vertex * 128 + axis * 4..vertex * 128 + axis * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    let mut bounds = [0; 4];
    for axis in 0..2 {
        let limit = if axis == 0 { width } else { height };
        let low = (0..3)
            .map(|v| coordinate(v, axis))
            .fold(f32::INFINITY, f32::min);
        let high = (0..3)
            .map(|v| coordinate(v, axis))
            .fold(f32::NEG_INFINITY, f32::max);
        bounds[axis] = low.floor().max(0.).min(limit as f32) as u32;
        bounds[axis + 2] = (high.ceil() + 1.).max(0.).min(limit as f32) as u32;
    }
    bounds
}

#[doc(hidden)]
pub fn raster_tiles(
    width: u32,
    height: u32,
    primitives: &[u8],
    limit: usize,
) -> Result<Vec<u8>, String> {
    let columns = width.div_ceil(TILE_SIZE);
    let rows = height.div_ceil(TILE_SIZE);
    let count = u64::from(columns) * u64::from(rows);
    if count.saturating_mul(8) > limit as u64 {
        return Err("QD3D GPU tile storage exceeds limits".into());
    }
    let mut lists = vec![Vec::<u32>::new(); count as usize];
    let mut entries = 0usize;
    let mut work = 0u64;
    for (index, primitive) in primitives.chunks_exact(TRIANGLE_BYTES).enumerate() {
        let lighting = 1 + u64::from(u32::from_le_bytes(primitive[452..456].try_into().unwrap()));
        let [left, top, right, bottom] = primitive_bounds(primitive, width, height);
        if left >= right || top >= bottom {
            continue;
        }
        for y in top / TILE_SIZE..bottom.div_ceil(TILE_SIZE) {
            for x in left / TILE_SIZE..right.div_ceil(TILE_SIZE) {
                entries += 1;
                if (count as usize * 2 + entries).saturating_mul(4) > limit {
                    return Err("QD3D GPU tile storage exceeds limits".into());
                }
                work = work.saturating_add(
                    u64::from((width - x * TILE_SIZE).min(TILE_SIZE))
                        * u64::from((height - y * TILE_SIZE).min(TILE_SIZE))
                        * lighting,
                );
                if work > MAX_RASTER_WORK {
                    return Err("QD3D GPU frame exceeds raster work limits".into());
                }
                lists[(y * columns + x) as usize].push(index as u32);
            }
        }
    }
    let mut words = vec![0u32; count as usize * 2];
    for (tile, list) in lists.into_iter().enumerate() {
        words[tile * 2] = words.len() as u32;
        words[tile * 2 + 1] = list.len() as u32;
        words.extend(list);
    }
    Ok(words.into_iter().flat_map(u32::to_le_bytes).collect())
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
