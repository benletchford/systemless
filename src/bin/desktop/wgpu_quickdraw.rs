//! Guest-visible QuickDraw rasterization, enabled only by --gpu.
use systemless::quickdraw::gpu::{QuickDrawGpuCommand, QuickDrawGpuPixel, QuickDrawGpuRenderer};
use wgpu::util::DeviceExt;

pub(super) struct QuickDrawRenderer {
    state: Option<State>,
    initialization_error: Option<String>,
    last_error: Option<String>,
}
impl QuickDrawRenderer {
    pub fn new() -> Self {
        Self {
            state: None,
            initialization_error: None,
            last_error: None,
        }
    }
}
impl QuickDrawGpuRenderer for QuickDrawRenderer {
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render(command, background),
            None => Err(self.initialization_error.clone().unwrap()),
        };
        if let Err(error) = &result {
            if self.last_error.as_ref() != Some(error) {
                eprintln!("[GPU] QuickDraw CPU fallback: {error}");
                self.last_error = Some(error.clone());
            }
        }
        result
    }
}
struct State {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    polygon_products: wgpu::ComputePipeline,
}
impl State {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .map_err(|e| e.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QuickDraw shapes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_quickdraw.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QuickDraw rasterizer"),
            layout: None,
            module: &shader,
            entry_point: Some("raster"),
            compilation_options: Default::default(),
            cache: None,
        });
        let polygon_products = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QuickDraw rounded polygon products"),
            layout: None,
            module: &shader,
            entry_point: Some("polygon_products"),
            compilation_options: Default::default(),
            cache: None,
        });
        eprintln!("[GPU] QuickDraw rasterization: {}", adapter.get_info().name);
        Ok(Self {
            device,
            queue,
            pipeline,
            polygon_products,
        })
    }
    fn render(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.render_inner(command, background);
        let mut error = None;
        for scope in [validation, memory, internal] {
            if let Some(e) = pollster::block_on(scope.pop()) {
                error = Some(e.to_string());
            }
        }
        match error {
            Some(error) => Err(error),
            None => result,
        }
    }
    fn render_inner(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let count = command.pixel_count().ok_or("invalid QuickDraw area")?;
        let width = (command.area[2] - command.area[0]) as u32;
        let height = (command.area[3] - command.area[1]) as u32;
        let limits = self.device.limits();
        let output_bytes = (count as u64)
            .checked_mul(8)
            .ok_or("QuickDraw size overflow")?;
        if !command.valid_clip_rows()
            || !command.valid_polygon_edges()
            || background.len() != count
            || output_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || output_bytes > limits.max_buffer_size
            || width.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || height.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || command
                .area
                .iter()
                .chain(&command.rect)
                .any(|v| !(-32768..=32767).contains(v))
            || command
                .border
                .is_some_and(|b| b.iter().any(|v| !(0..=65535).contains(v)))
        {
            return Err("unsupported QuickDraw GPU request".into());
        }
        let mut clips = vec![0i32; command.clip_rows.as_ref().map_or(1, |rows| rows.len() * 2)];
        if let Some(rows) = &command.clip_rows {
            for (row, endpoints) in rows.iter().enumerate() {
                clips[row * 2] = clips.len() as i32;
                clips[row * 2 + 1] = endpoints.len() as i32;
                clips.extend_from_slice(endpoints);
            }
        }
        let clip_bytes: Vec<u8> = clips.into_iter().flat_map(i32::to_le_bytes).collect();
        if clip_bytes.len() as u64 > u64::from(limits.max_storage_buffer_binding_size) {
            return Err("QuickDraw clip storage exceeds GPU limits".into());
        }
        let clip_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw clip rows"),
                contents: &clip_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let edges = command.polygon_edges.as_deref().unwrap_or(&[]);
        let edge_bytes: Vec<u8> = if edges.is_empty() {
            vec![0; 16]
        } else {
            edges
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect()
        };
        let edge_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw polygon edges"),
                contents: &edge_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let product_bytes = (height as u64)
            .checked_mul(edges.len() as u64)
            .and_then(|n| n.checked_mul(4))
            .ok_or("QuickDraw polygon product size overflow")?
            .max(4);
        if product_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || product_bytes > limits.max_buffer_size
        {
            return Err("QuickDraw polygon products exceed GPU limits".into());
        }
        let products = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw rounded polygon products"),
            size: product_bytes,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let input: Vec<u8> = background.iter().flat_map(|p| p.to_le_bytes()).collect();
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw background"),
                contents: &input,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw result"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let border = command.border.unwrap_or([0, 0]);
        let mut words = Vec::with_capacity(28);
        words.extend([
            command.area[0] as u32,
            command.area[1] as u32,
            width,
            height,
        ]);
        words.extend(command.rect.map(|v| v as u32));
        words.extend([
            border[0] as u32,
            border[1] as u32,
            u32::from(command.border.is_some()),
            u32::from(command.clip_rows.is_some()),
        ]);
        words.extend(command.transfer.into_iter().flatten());
        words.extend(command.pattern.map(u32::from));
        words.extend([
            u32::from(command.polygon_edges.is_some()),
            edges.len() as u32,
            0,
            0,
        ]);
        let uniform: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw geometry"),
                contents: &uniform,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: clip_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: edge_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: products.as_entire_binding(),
                },
            ],
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if !edges.is_empty() {
            let product_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("QuickDraw polygon products"),
                layout: &self.polygon_products.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: edge_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: products.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.polygon_products);
            pass.set_bind_group(0, &product_group, &[]);
            pass.dispatch_workgroups((edges.len() as u32).div_ceil(64), height, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_bytes);
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
            .chunks_exact(8)
            .map(|p| QuickDrawGpuPixel {
                value: u32::from_le_bytes(p[..4].try_into().unwrap()),
                covered: u32::from_le_bytes(p[4..].try_into().unwrap()) != 0,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_quickdraw_polygons_match_even_odd_crossings() {
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
        let mut polygons = vec![
            vec![[-9, -7], [12, -7], [12, 11], [-9, 11]],
            vec![[-8, -6], [14, 13], [-8, 13], [14, -6]],
            vec![[-5, -6], [11, -6], [11, 1], [2, 1], [2, 12], [-5, 12]],
            vec![[0, -8], [0, 13], [0, -8]],
            vec![],
        ];
        let mut seed = 17u32;
        for _ in 0..80 {
            let mut vertices = Vec::new();
            for _ in 0..7 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let x = (seed % 29) as i32 - 12;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                vertices.push([x, (seed % 25) as i32 - 10]);
            }
            polygons.push(vertices);
        }
        for vertices in polygons {
            let mut edges = Vec::new();
            for i in 0..vertices.len() {
                let [x0, y0] = vertices[i];
                let [x1, y1] = vertices[(i + 1) % vertices.len()];
                if y0 == y1 {
                    continue;
                }
                let (top, bottom, left) = if y0 < y1 { (y0, y1, x0) } else { (y1, y0, x1) };
                edges.push([
                    top as f32,
                    bottom as f32,
                    left as f32,
                    (x1 - x0) as f32 / (y1 - y0) as f32,
                ]);
            }
            let command = QuickDrawGpuCommand {
                polygon_edges: Some(edges.clone()),
                clip_rows: Some(
                    (-10..16)
                        .map(|y| {
                            if y % 4 == 0 {
                                vec![-9, 1, 4, 15]
                            } else {
                                vec![-12, 18]
                            }
                        })
                        .collect(),
                ),
                area: [-12, -10, 18, 16],
                rect: [-12, -10, 18, 16],
                border: None,
                pattern: [0xff; 8],
                transfer: [[0, 3], [0, 7]],
            };
            let pixels = state.render(&command, &vec![0x1234; 30 * 26]).unwrap();
            for y in -10..16 {
                for x in -12..18 {
                    let crossings = edges
                        .iter()
                        .filter(|e| {
                            y as f32 >= e[0]
                                && (y as f32) < e[1]
                                && e[2] + (y as f32 - e[0]) * e[3] <= x as f32
                        })
                        .count();
                    let covered = crossings % 2 != 0
                        && (y % 4 != 0 || (-9..1).contains(&x) || (4..15).contains(&x));
                    assert_eq!(
                        pixels[((y + 10) * 30 + x + 12) as usize],
                        QuickDrawGpuPixel {
                            value: if covered { 7 } else { 0x1234 },
                            covered
                        },
                        "vertices={vertices:?},point={x},{y}"
                    );
                }
            }
        }
        let mut command = QuickDrawGpuCommand {
            polygon_edges: None,
            clip_rows: None,
            area: [0, 0, 1, 1],
            rect: [0, 0, 1, 1],
            border: None,
            pattern: [0; 8],
            transfer: [[0; 2]; 2],
        };
        for invalid in [
            [0., 1., f32::NAN, 0.],
            [2., 1., 0., 0.],
            [0., 1., 0., f32::INFINITY],
        ] {
            command.polygon_edges = Some(vec![invalid]);
            assert!(state.render(&command, &[0]).is_err());
        }
        command.area = [0, 0, 100, 100];
        command.polygon_edges = Some(vec![[0., 1., 0., 0.]; 6401]);
        assert!(state.render(&command, &vec![0; 10000]).is_err());
    }

    #[test]
    fn gpu_quickdraw_complex_clips_preserve_holes_and_empty_rows() {
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
        for origin in [[-11, -9], [-3, 2], [0, 0], [5, 7]] {
            for border in [None, Some([1, 2])] {
                let command = QuickDrawGpuCommand {
                    polygon_edges: None,
                    area: [origin[0], origin[1], origin[0] + 17, origin[1] + 13],
                    rect: [origin[0], origin[1], origin[0] + 17, origin[1] + 13],
                    border,
                    pattern: [0xff; 8],
                    transfer: [[0, 3], [0, 7]],
                    clip_rows: Some(
                        (0..13)
                            .map(|y| match y % 3 {
                                0 => vec![],
                                1 => vec![origin[0] + 2, origin[0] + 6],
                                _ => vec![
                                    origin[0] - 2,
                                    origin[0] + 4,
                                    origin[0] + 9,
                                    origin[0] + 20,
                                ],
                            })
                            .collect(),
                    ),
                };
                let background = vec![0x123456; 17 * 13];
                let pixels = state.render(&command, &background).unwrap();
                for y in 0..13 {
                    for x in 0..17 {
                        let clipped = match y % 3 {
                            0 => false,
                            1 => (2..6).contains(&x),
                            _ => x < 4 || x >= 9,
                        };
                        let covered =
                            clipped && border.is_none_or(|_| x == 0 || x == 16 || y < 2 || y >= 11);
                        assert_eq!(
                            pixels[y * 17 + x],
                            QuickDrawGpuPixel {
                                value: if covered { 7 } else { 0x123456 },
                                covered
                            },
                            "origin={origin:?},point={x},{y}"
                        );
                    }
                }
            }
        }
        let command = QuickDrawGpuCommand {
            polygon_edges: None,
            area: [0, 0, 1, 1],
            rect: [0, 0, 1, 1],
            border: None,
            pattern: [0; 8],
            transfer: [[0; 2]; 2],
            clip_rows: None,
        };
        for rows in [
            vec![],
            vec![vec![0]],
            vec![vec![2, 1]],
            vec![vec![0, 3, 2, 4]],
        ] {
            let mut invalid = command.clone();
            invalid.clip_rows = Some(rows);
            assert!(state.render(&invalid, &[0]).is_err());
        }
        let mut oversized = command;
        oversized.area[2] = 32767;
        oversized.clip_rows = Some(vec![(0..32768).collect()]);
        assert!(state.render(&oversized, &vec![0; 32767]).is_err());
    }

    #[test]
    fn gpu_quickdraw_rectangles_match_patterns_transfers_frames_and_negative_origins() {
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
        for origin in [[-11, -9], [-3, 2], [0, 0], [5, 7]] {
            for border in [None, Some([0, 0]), Some([1, 2]), Some([20, 1])] {
                for transfer in [
                    [[0, 0], [0, 255]],
                    [[0xff, 0], [0xff, 0xff]],
                    [[0x00ff_ffff, 0x0000_1234], [0x0000_ffff, 0x00ff_0000]],
                ] {
                    let command = QuickDrawGpuCommand {
                        polygon_edges: None,
                        clip_rows: None,
                        area: [origin[0], origin[1], origin[0] + 17, origin[1] + 13],
                        rect: [origin[0] + 1, origin[1] + 1, origin[0] + 16, origin[1] + 12],
                        border,
                        pattern: [0x96, 0x69, 0xaa, 0x55, 0xcc, 0x33, 0xf0, 0x0f],
                        transfer,
                    };
                    let background: Vec<u32> = (0..17 * 13)
                        .map(|i| (i as u32).wrapping_mul(0x10203) ^ 0xa5c6e7)
                        .collect();
                    let actual = state.render(&command, &background).unwrap();
                    for y in 0..13 {
                        for x in 0..17 {
                            let i = y * 17 + x;
                            let px = origin[0] + x as i32;
                            let py = origin[1] + y as i32;
                            let inside = (1..16).contains(&x) && (1..12).contains(&y);
                            let covered = inside
                                && border.is_none_or(|b| {
                                    x < 1 + b[0] as usize
                                        || x >= 16usize.saturating_sub(b[0] as usize)
                                        || y < 1 + b[1] as usize
                                        || y >= 12usize.saturating_sub(b[1] as usize)
                                });
                            let black = command.pattern[py.rem_euclid(8) as usize]
                                & (0x80 >> px.rem_euclid(8))
                                != 0;
                            let [pass, xor] = transfer[usize::from(black)];
                            let expected = QuickDrawGpuPixel {
                                covered,
                                value: if covered {
                                    (background[i] & pass) ^ xor
                                } else {
                                    background[i]
                                },
                            };
                            assert_eq!(
                                actual[i], expected,
                                "origin={origin:?},border={border:?},point={x},{y}"
                            );
                        }
                    }
                }
            }
        }
        let command = QuickDrawGpuCommand {
            polygon_edges: None,
            clip_rows: None,
            area: [0, 0, 1, 1],
            rect: [0, 0, 1, 1],
            border: None,
            pattern: [0; 8],
            transfer: [[0; 2]; 2],
        };
        assert!(state.render(&command, &[]).is_err());
    }
}
