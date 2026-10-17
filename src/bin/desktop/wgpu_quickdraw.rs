//! Guest-visible QuickDraw rasterization, enabled only by --gpu.
use systemless::quickdraw::gpu::{
    QuickDrawGpuBlit, QuickDrawGpuCommand, QuickDrawGpuPackedCopy, QuickDrawGpuPixel,
    QuickDrawGpuRenderer,
};
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
    fn render_packed_blit(&mut self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render_packed_blit(command),
            None => Err(self.initialization_error.clone().unwrap()),
        };
        if let Err(error) = &result {
            if self.last_error.as_ref() != Some(error) {
                eprintln!("[GPU] QuickDraw packed bitmap CPU fallback: {error}");
                self.last_error = Some(error.clone());
            }
        }
        result
    }
    fn render_blit(&mut self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render_blit(command),
            None => Err(self.initialization_error.clone().unwrap()),
        };
        if let Err(error) = &result {
            if self.last_error.as_ref() != Some(error) {
                eprintln!("[GPU] QuickDraw bitmap CPU fallback: {error}");
                self.last_error = Some(error.clone());
            }
        }
        result
    }
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
    blit_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
    packed_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
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
            blit_pipeline: Default::default(),
            packed_pipeline: Default::default(),
        })
    }
    fn create_blit_pipeline(&self, entry: &str) -> Result<wgpu::ComputePipeline, String> {
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("QuickDraw bitmap samples"),
                source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_blit.wgsl").into()),
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("QuickDraw bitmap blit"),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
        let mut error = None;
        for scope in [validation, memory, internal] {
            if let Some(e) = pollster::block_on(scope.pop()) {
                error = Some(e.to_string());
            }
        }
        match error {
            Some(error) => Err(error),
            None => {
                eprintln!("[GPU] QuickDraw {entry} bitmap sampling enabled");
                Ok(pipeline)
            }
        }
    }
    fn render_blit(&self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        if !command.valid() {
            return Err("invalid QuickDraw bitmap blit".into());
        }
        let mut cached = self.blit_pipeline.borrow_mut();
        if cached.is_none() {
            *cached = Some(self.create_blit_pipeline("blit"));
        }
        let pipeline = cached.as_ref().unwrap().as_ref().map_err(Clone::clone)?;
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.render_blit_inner(command, pipeline);
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
    fn render_packed_blit(&self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        if !command.valid() {
            return Err("invalid packed bitmap blit".into());
        }
        let mut cached = self.packed_pipeline.borrow_mut();
        if cached.is_none() {
            *cached = Some(self.create_blit_pipeline("packed_copy"));
        }
        let pipeline = cached.as_ref().unwrap().as_ref().map_err(Clone::clone)?;
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let source: Vec<u32> = command.source.iter().map(|v| u32::from(*v)).collect();
        let palette: Vec<u32> = command.palette.as_ref().map_or_else(
            || (0..256).collect(),
            |p| p.iter().map(|v| u32::from(*v)).collect(),
        );
        let background: Vec<u32> = command
            .background
            .iter()
            .flat_map(|v| [u32::from(*v), 0])
            .collect();
        let geometry = [
            command.source_stride,
            command.destination_stride,
            command.height,
            command.depth,
            command.source_bit,
            command.destination_bit,
            command.width,
            u32::from(command.palette.is_some()),
        ];
        let result = self
            .dispatch_blit(
                pipeline,
                command.destination_stride,
                command.height,
                &[
                    (0, &source, wgpu::BufferUsages::STORAGE),
                    (1, &palette, wgpu::BufferUsages::STORAGE),
                    (2, &background, wgpu::BufferUsages::STORAGE),
                    (5, &geometry, wgpu::BufferUsages::UNIFORM),
                ],
            )
            .and_then(|pixels| {
                if pixels.iter().any(|p| *p > 255) {
                    return Err("packed bitmap returned invalid bytes".into());
                }
                Ok(pixels.into_iter().map(|p| p as u8).collect())
            });
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
    fn render_blit_inner(
        &self,
        command: &QuickDrawGpuBlit,
        pipeline: &wgpu::ComputePipeline,
    ) -> Result<Vec<u32>, String> {
        let width = command.columns.len() as u32;
        let height = command.rows.len() as u32;
        let palette: Vec<u32> = command
            .direct
            .as_ref()
            .filter(|d| !d.palette.is_empty())
            .map_or_else(
                || {
                    command
                        .palette
                        .as_ref()
                        .map_or_else(|| (0..256).collect(), |p| p.to_vec())
                },
                |d| d.palette.clone(),
            );
        let columns: Vec<u32> = command.columns.iter().flatten().copied().collect();
        let rows: Vec<u32> = command.rows.iter().flatten().copied().collect();
        let geometry = [
            command.width,
            width,
            height,
            u32::from(command.palette.is_some()),
            command.direct.as_ref().map_or(0, |d| d.source_depth),
            command.direct.as_ref().map_or(0, |d| d.destination_depth),
            command
                .direct
                .as_ref()
                .map_or(0, |d| d.palette.len() as u32),
            0,
        ];
        self.dispatch_blit(
            pipeline,
            width,
            height,
            &[
                (0, &command.pixels, wgpu::BufferUsages::STORAGE),
                (1, &palette, wgpu::BufferUsages::STORAGE),
                (2, &columns, wgpu::BufferUsages::STORAGE),
                (3, &rows, wgpu::BufferUsages::STORAGE),
                (5, &geometry, wgpu::BufferUsages::UNIFORM),
            ],
        )
    }
    fn dispatch_blit(
        &self,
        pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        inputs: &[(u32, &[u32], wgpu::BufferUsages)],
    ) -> Result<Vec<u32>, String> {
        let output_bytes = u64::from(width) * u64::from(height) * 4;
        let limits = self.device.limits();
        if output_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || output_bytes > limits.max_buffer_size
            || inputs.iter().any(|(_, v, _)| {
                v.len() as u64 * 4 > u64::from(limits.max_storage_buffer_binding_size)
            })
            || width.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || height.div_ceil(8) > limits.max_compute_workgroups_per_dimension
        {
            return Err("QuickDraw bitmap exceeds GPU limits".into());
        }
        let mut buffers: Vec<(u32, wgpu::Buffer)> = inputs
            .iter()
            .map(|(binding, words, usage)| {
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                (
                    *binding,
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("QuickDraw bitmap data"),
                            contents: &bytes,
                            usage: *usage,
                        }),
                )
            })
            .collect();
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw bitmap result"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        buffers.push((4, output));
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("QuickDraw bitmap"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw bitmap readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&buffers.last().unwrap().1, 0, &readback, 0, output_bytes);
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
            .map(|p| u32::from_le_bytes(p.try_into().unwrap()))
            .collect())
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
        if !command.valid_mask()
            || !command.valid_clip_rows()
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
        let mask_bytes: Vec<u8> = command.mask.as_ref().map_or_else(
            || vec![0; 4],
            |mask| {
                mask.coverage
                    .iter()
                    .flat_map(|&v| u32::from(v).to_le_bytes())
                    .collect()
            },
        );
        if mask_bytes.len() as u64 > u64::from(limits.max_storage_buffer_binding_size) {
            return Err("QuickDraw mask exceeds GPU limits".into());
        }
        let mask_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw glyph coverage"),
                contents: &mask_bytes,
                usage: wgpu::BufferUsages::STORAGE,
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
        let mut words = Vec::with_capacity(32);
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
            command
                .mask
                .as_ref()
                .map_or(0, |mask| u32::from(mask.threshold)),
            u32::from(
                command
                    .mask
                    .as_ref()
                    .is_some_and(|mask| mask.repeat_transfer),
            ),
        ]);
        let mask_geometry = command.mask.as_ref().map_or([0; 4], |mask| {
            [
                mask.origin[0],
                mask.origin[1],
                mask.size[0] as i32,
                mask.size[1] as i32,
            ]
        });
        words.extend(mask_geometry.map(|v| v as u32));
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
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: mask_buffer.as_entire_binding(),
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
    fn gpu_packed_bitmap_bytes_match_pixel_oracle_at_each_alignment() {
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
        for depth in [1, 2, 4u32] {
            let offsets: Vec<_> = if depth == 1 {
                vec![1, 7]
            } else {
                (0..8).step_by(depth as usize).collect()
            };
            for source_bit in &offsets {
                for destination_bit in &offsets {
                    for width in [1, 17] {
                        for mapped in [false, true] {
                            let source_stride = (source_bit + width * depth).div_ceil(8) + 2;
                            let destination_stride =
                                (destination_bit + width * depth).div_ceil(8) + 3;
                            let height = 3;
                            let source = (0..source_stride * height)
                                .map(|i| (i * 73 + 19) as u8)
                                .collect();
                            let background = (0..destination_stride * height)
                                .map(|i| (i * 37 + 165) as u8)
                                .collect();
                            let mask = ((1 << depth) - 1) as u8;
                            let palette = mapped.then(|| {
                                std::array::from_fn(|i| {
                                    if i <= usize::from(mask) {
                                        mask - i as u8
                                    } else {
                                        255
                                    }
                                })
                            });
                            let command = QuickDrawGpuPackedCopy {
                                depth,
                                width,
                                height,
                                source_stride,
                                source_bit: *source_bit,
                                source,
                                destination_stride,
                                destination_bit: *destination_bit,
                                background,
                                palette,
                            };
                            let mut expected = command.background.clone();
                            for y in 0..height {
                                for x in 0..width {
                                    let bit = source_bit + x * depth;
                                    let value = (command.source
                                        [(y * source_stride + bit / 8) as usize]
                                        >> (8 - depth - bit % 8))
                                        & mask;
                                    let value = command
                                        .palette
                                        .as_ref()
                                        .map_or(value, |p| p[value as usize]);
                                    let bit = destination_bit + x * depth;
                                    let shift = 8 - depth - bit % 8;
                                    let byte =
                                        &mut expected[(y * destination_stride + bit / 8) as usize];
                                    *byte = (*byte & !(mask << shift)) | (value << shift);
                                }
                            }
                            assert_eq!(state.render_packed_blit(&command).unwrap(),expected,"depth={depth},sb={source_bit},db={destination_bit},width={width},mapped={mapped}");
                            let mut invalid = command.clone();
                            invalid.source.pop();
                            assert!(state.render_packed_blit(&invalid).is_err());
                        }
                    }
                }
            }
        }
        let raw = QuickDrawGpuBlit {
            width: 1,
            height: 1,
            pixels: vec![u32::MAX],
            columns: vec![[0, 1]],
            rows: vec![[0, 1]],
            palette: None,
            direct: None,
        };
        assert_eq!(state.render_blit(&raw).unwrap(), vec![u32::MAX]);
    }

    #[test]
    fn gpu_direct_bitmap_conversions_match_cpu_colors_and_palette_ties() {
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
        for source_depth in [16, 32] {
            for destination_depth in [1, 2, 4, 8, 16, 32] {
                if source_depth == destination_depth {
                    continue;
                }
                let pixels = (0..65536u32)
                    .map(|i| {
                        if source_depth == 16 {
                            i
                        } else {
                            i.wrapping_mul(2654435761)
                        }
                    })
                    .collect();
                let count = if destination_depth <= 8 {
                    1usize << destination_depth
                } else {
                    0
                };
                let palette = (0..count)
                    .map(|i| {
                        // Repeated colors exercise the lowest-index tie rule.
                        ((i / 2) as u32 * 1357) & 32767
                    })
                    .collect();
                let command = QuickDrawGpuBlit {
                    width: 256,
                    height: 256,
                    pixels,
                    columns: (0..256).map(|x| [x, x + 1]).collect(),
                    rows: (0..256).map(|y| [y, y + 1]).collect(),
                    palette: None,
                    direct: Some(systemless::quickdraw::gpu::QuickDrawGpuDirectColor {
                        source_depth,
                        destination_depth,
                        palette,
                    }),
                };
                let colors = &command.direct.as_ref().unwrap().palette;
                let expected: Vec<u32> = command
                    .pixels
                    .iter()
                    .map(|pixel| {
                        let rgb = if source_depth == 16 {
                            [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31]
                                .map(|v| ((f64::from(v) * 65535.0 / 31.0).round() as u32) >> 8)
                        } else {
                            [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255]
                        };
                        match destination_depth {
                            32 => (rgb[0] << 16) | (rgb[1] << 8) | rgb[2],
                            16 => ((rgb[0] / 8) << 10) | ((rgb[1] / 8) << 5) | (rgb[2] / 8),
                            1 if source_depth == 32 => {
                                let black: u64 = rgb.iter().map(|c| u64::from(c * 257).pow(2)).sum();
                                let white: u64 =
                                    rgb.iter().map(|c| u64::from(65535 - c * 257).pow(2)).sum();
                                u32::from(black <= white)
                            }
                            _ => {
                                colors
                                    .iter()
                                    .enumerate()
                                    .min_by_key(|(_, p)| {
                                        let c = [
                                            (*p >> 10 & 31) as i32,
                                            (*p >> 5 & 31) as i32,
                                            (*p & 31) as i32,
                                        ];
                                        (0..3)
                                            .map(|i| (c[i] - (rgb[i] / 8) as i32).pow(2))
                                            .sum::<i32>()
                                    })
                                    .unwrap()
                                    .0 as u32
                            }
                        }
                    })
                    .collect();
                assert_eq!(
                    state.render_blit(&command).unwrap(),
                    expected,
                    "{source_depth}->{destination_depth}"
                );
                let mut invalid = command.clone();
                invalid.direct.as_mut().unwrap().source_depth = 15;
                assert!(state.render_blit(&invalid).is_err());
                let mut invalid = command.clone();
                invalid.columns[0] = [0, 2];
                assert!(state.render_blit(&invalid).is_err());
                if count > 0 {
                    let mut invalid = command;
                    invalid.direct.as_mut().unwrap().palette.pop();
                    assert!(state.render_blit(&invalid).is_err());
                }
            }
        }
    }

    #[test]
    fn gpu_bitmap_samples_reductions_palettes_and_raw_words_match_cpu() {
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
        for mapped in [false, true] {
            let pixels: Vec<u32> = (0..23 * 17u32)
                .map(|i| {
                    if mapped {
                        (i * 73) & 255
                    } else {
                        i.wrapping_mul(2654435761)
                    }
                })
                .collect();
            let palette = mapped.then(|| {
                std::array::from_fn(|i| 0xff00_0000 | ((255 - i as u32) * 0x010101))
            });
            let command = QuickDrawGpuBlit {
                width: 23,
                height: 17,
                pixels,
                columns: vec![[0, 1], [1, 5], [3, 4], [22, 23], [22, 23]],
                rows: vec![[0, 1], [1, 7], [9, 17], [16, 17]],
                palette,
                direct: None,
            };
            let mut expected = Vec::new();
            for row in &command.rows {
                for col in &command.columns {
                    let mut value = 0;
                    for y in row[0]..row[1] {
                        for x in col[0]..col[1] {
                            value = value.max(command.pixels[(y * 23 + x) as usize]);
                        }
                    }
                    expected.push(
                        command
                            .palette
                            .as_ref()
                            .map_or(value, |p| p[value as usize]),
                    );
                }
            }
            assert_eq!(state.render_blit(&command).unwrap(), expected);
            let mut invalid = command.clone();
            invalid.columns[0] = [4, 3];
            assert!(state.render_blit(&invalid).is_err());
            invalid = command.clone();
            invalid.pixels.pop();
            assert!(state.render_blit(&invalid).is_err());
            if mapped {
                invalid = command.clone();
                invalid.pixels[0] = 256;
                assert!(state.render_blit(&invalid).is_err());
            }
        }
    }

    #[test]
    fn gpu_repeated_mask_transfers_preserve_even_xor_paint_coverage() {
        use systemless::quickdraw::gpu::QuickDrawGpuMask;
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
        let coverage = vec![0, 1, 2, 3, 4, 254, 255];
        for transfer in [[0xffff, 0x73a5], [0, 0x1234], [0x8527, 0], [0x8527, 0x73a5]] {
            let command = QuickDrawGpuCommand {
                mask: Some(QuickDrawGpuMask {
                    origin: [0, 0],
                    size: [7, 1],
                    coverage: coverage.clone(),
                    threshold: 1,
                    repeat_transfer: true,
                }),
                area: [0, 0, 7, 1],
                rect: [0, 0, 7, 1],
                border: None,
                clip_rows: None,
                polygon_edges: None,
                pattern: [255; 8],
                transfer: [transfer; 2],
            };
            let pixels = state.render(&command, &[0xc7a5; 7]).unwrap();
            for (pixel, count) in pixels.iter().zip(&coverage) {
                let mut expected = 0xc7a5;
                for _ in 0..*count {
                    expected = (expected & transfer[0]) ^ transfer[1];
                }
                assert_eq!(
                    *pixel,
                    QuickDrawGpuPixel {
                        value: expected,
                        covered: *count != 0
                    }
                );
            }
        }
    }

    #[test]
    fn gpu_glyph_masks_match_thresholds_clips_and_negative_origins() {
        use systemless::quickdraw::gpu::QuickDrawGpuMask;
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
        let coverage = (0..63)
            .map(|i| [0, 1, 127, 128, 129, 254, 255][i % 7])
            .collect::<Vec<_>>();
        for origin in [[-3, -2], [4, 6]] {
            for threshold in [1, 128, 255] {
                let command = QuickDrawGpuCommand {
                    mask: Some(QuickDrawGpuMask {
                        repeat_transfer: false,
                        origin,
                        size: [9, 7],
                        coverage: coverage.clone(),
                        threshold,
                    }),
                    area: [-5, -4, 16, 15],
                    rect: [-5, -4, 16, 15],
                    clip_rows: Some(vec![vec![-5, 1, 3, 16]; 19]),
                    polygon_edges: None,
                    border: None,
                    pattern: [0xff; 8],
                    transfer: [[0xffff, 0xa5]; 2],
                };
                let pixels = state.render(&command, &vec![0x8001; 21 * 19]).unwrap();
                for y in -4..15 {
                    for x in -5..16 {
                        let tx = x - origin[0];
                        let ty = y - origin[1];
                        let covered = (0..9).contains(&tx)
                            && (0..7).contains(&ty)
                            && !(1..3).contains(&x)
                            && coverage[(ty * 9 + tx) as usize] >= threshold;
                        assert_eq!(
                            pixels[((y + 4) * 21 + x + 5) as usize],
                            QuickDrawGpuPixel {
                                value: if covered { 0x80a4 } else { 0x8001 },
                                covered
                            },
                            "origin={origin:?},threshold={threshold},point={x},{y}"
                        );
                    }
                }
            }
        }
        for area in [
            [-32768, -32768, -32766, -32766],
            [32766, 32766, 32767, 32767],
        ] {
            let origin = if area[0] < 0 {
                [-32770, -32770]
            } else {
                [32766, 32766]
            };
            let command = QuickDrawGpuCommand {
                mask: Some(QuickDrawGpuMask {
                    repeat_transfer: false,
                    origin,
                    size: [4, 4],
                    coverage: vec![255; 16],
                    threshold: 128,
                }),
                area,
                rect: area,
                clip_rows: None,
                polygon_edges: None,
                border: None,
                pattern: [0xff; 8],
                transfer: [[0xffff, 0xa5]; 2],
            };
            let pixels = state
                .render(&command, &vec![0x8001; command.pixel_count().unwrap()])
                .unwrap();
            assert!(pixels.iter().all(|p| *p
                == QuickDrawGpuPixel {
                    value: 0x80a4,
                    covered: true
                }));
        }
        let mut command = QuickDrawGpuCommand {
            mask: None,
            area: [0, 0, 1, 1],
            rect: [0, 0, 1, 1],
            clip_rows: None,
            polygon_edges: None,
            border: None,
            pattern: [0; 8],
            transfer: [[0; 2]; 2],
        };
        for (size, coverage, threshold) in [
            ([0, 1], vec![], 128),
            ([2, 2], vec![255; 3], 128),
            ([1, 1], vec![255], 0),
        ] {
            command.mask = Some(QuickDrawGpuMask {
                repeat_transfer: false,
                origin: [0, 0],
                size,
                coverage,
                threshold,
            });
            assert!(state.render(&command, &[0]).is_err());
        }
    }

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
                mask: None,
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
            mask: None,
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
                    mask: None,
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
            mask: None,
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
                        mask: None,
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
            mask: None,
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
