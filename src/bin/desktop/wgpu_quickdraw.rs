//! Guest-visible QuickDraw rasterization, enabled only by --gpu.
use super::wgpu_buffers::ScratchBuffers;
use systemless::quickdraw::gpu::{
    QuickDrawGpuBlit, QuickDrawGpuCommand, QuickDrawGpuPackedCopy, QuickDrawGpuPixel,
    QuickDrawGpuRenderer,
};

pub(super) struct QuickDrawRenderer {
    state: Option<State>,
    initialization_error: Option<String>,
    last_error: Option<String>,
}
impl QuickDrawRenderer {
    pub fn new() -> Self {
        eprintln!("[GPU] QuickDraw adaptive dispatch enabled");
        Self {
            state: None,
            initialization_error: None,
            last_error: None,
        }
    }
}
impl QuickDrawGpuRenderer for QuickDrawRenderer {
    fn prefers_precomputed_raster(&self) -> bool {
        false
    }
    fn prefers_raster(&self, command: &QuickDrawGpuCommand) -> bool {
        // Rectangles, pre-tessellated spans and glyph atlases already have
        // their coverage on the CPU. A GPU round trip cannot amortize that
        // work. Keep GPU evaluation for substantial polygon edge work.
        command.polygon_edges.as_ref().is_some_and(|edges| {
            command
                .pixel_count()
                .unwrap_or(0)
                .saturating_mul(edges.len())
                >= 131_072
        })
    }

    fn prefers_blit(&self, destination_pixels: usize, expensive_color: bool) -> bool {
        expensive_color && destination_pixels >= 16_384
    }

    fn render_blit_cells(
        &mut self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        if let Some(error) = self
            .state
            .as_ref()
            .and_then(|state| state.health.check().err())
        {
            self.initialization_error = Some(error.to_string());
            self.state = None;
        }
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render_blit_cells(command),
            None => Err(self.initialization_error.clone().unwrap()),
        };
        if let Err(error) = &result {
            if self.last_error.as_ref() != Some(error) {
                eprintln!("[GPU] QuickDraw clipped bitmap CPU fallback: {error}");
                self.last_error = Some(error.clone());
            }
        }
        result
    }
    fn render_packed_blit(&mut self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        if let Some(error) = self
            .state
            .as_ref()
            .and_then(|state| state.health.check().err())
        {
            self.initialization_error = Some(error.to_string());
            self.state = None;
        }
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
        if let Some(error) = self
            .state
            .as_ref()
            .and_then(|state| state.health.check().err())
        {
            self.initialization_error = Some(error.to_string());
            self.state = None;
        }
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
        if let Some(error) = self
            .state
            .as_ref()
            .and_then(|state| state.health.check().err())
        {
            self.initialization_error = Some(error.to_string());
            self.state = None;
        }
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
    health: std::sync::Arc<super::renderer_recovery::Health>,
    buffers: ScratchBuffers,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    polygon_products: wgpu::ComputePipeline,
    blit_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
    cells_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
    arithmetic_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
    deep_mask_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
    packed_pipeline: std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>,
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
        health.check().map_err(|e| e.to_string())?;
        Ok(Self {
            health,
            buffers: Default::default(),
            device,
            queue,
            pipeline,
            polygon_products,
            blit_pipeline: Default::default(),
            cells_pipeline: Default::default(),
            arithmetic_pipeline: Default::default(),
            deep_mask_pipeline: Default::default(),
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
                self.health
                    .record(super::renderer_recovery::Failure::from(e.to_string()));
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
        if command.clip.is_some() || command.arithmetic.is_some() || command.deep_mask.is_some() {
            return Err("clipped bitmap requires coverage readback".into());
        }
        self.render_blit_words(command, false)
    }
    fn render_blit_cells(
        &self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let words = self.render_blit_words(command, true)?;
        if words.len() % 2 != 0 || words.chunks_exact(2).any(|p| p[1] > 1) {
            return Err("invalid bitmap coverage readback".into());
        }
        Ok(words
            .chunks_exact(2)
            .map(|p| QuickDrawGpuPixel {
                value: p[0],
                covered: p[1] != 0,
            })
            .collect())
    }
    fn render_blit_words(
        &self,
        command: &QuickDrawGpuBlit,
        cells: bool,
    ) -> Result<Vec<u32>, String> {
        self.health.check().map_err(|e| e.to_string())?;
        if !command.valid() {
            return Err("invalid QuickDraw bitmap blit".into());
        }
        let mut cached = if cells && command.deep_mask.is_some() {
            self.deep_mask_pipeline.borrow_mut()
        } else if cells && command.arithmetic.is_some() {
            self.arithmetic_pipeline.borrow_mut()
        } else if cells {
            self.cells_pipeline.borrow_mut()
        } else {
            self.blit_pipeline.borrow_mut()
        };
        if cached.is_none() {
            *cached = Some(
                self.create_blit_pipeline(if cells && command.deep_mask.is_some() {
                    "blit_deep_mask"
                } else if cells && command.arithmetic.is_some() {
                    "blit_arithmetic"
                } else if cells {
                    "blit_cells"
                } else {
                    "blit"
                }),
            );
        }
        let pipeline = cached.as_ref().unwrap().as_ref().map_err(Clone::clone)?;
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.render_blit_inner(command, pipeline, cells);
        let mut error = None;
        for scope in [validation, memory, internal] {
            if let Some(e) = pollster::block_on(scope.pop()) {
                self.health
                    .record(super::renderer_recovery::Failure::from(e.to_string()));
                error = Some(e.to_string());
            }
        }
        match error {
            Some(error) => Err(error),
            None => result,
        }
    }
    fn render_packed_blit(&self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        self.health.check().map_err(|e| e.to_string())?;
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
                1,
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
                self.health
                    .record(super::renderer_recovery::Failure::from(e.to_string()));
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
        cells: bool,
    ) -> Result<Vec<u32>, String> {
        self.health.check().map_err(|e| e.to_string())?;
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
        let mut coverage = vec![0, 0, u32::MAX];
        if let Some(clip) = &command.clip {
            if let Some([key, mask]) = clip.transparent {
                coverage[1] = key;
                coverage[2] = mask;
                coverage[0] |= 2;
            }
            if let Some(mask) = &clip.coverage {
                coverage[0] |= 1;
                coverage.extend(mask.iter().map(|v| u32::from(*v)));
            }
        }
        let mut inputs: Vec<(u32, &[u32], wgpu::BufferUsages)> = vec![
            (0, &command.pixels, wgpu::BufferUsages::STORAGE),
            (1, &palette, wgpu::BufferUsages::STORAGE),
            (2, &columns, wgpu::BufferUsages::STORAGE),
            (3, &rows, wgpu::BufferUsages::STORAGE),
            (5, &geometry, wgpu::BufferUsages::UNIFORM),
        ];
        if cells {
            inputs.push((6, &coverage, wgpu::BufferUsages::STORAGE));
        }
        let mut arithmetic = Vec::new();
        if let Some(a) = &command.arithmetic {
            arithmetic.extend([
                a.depth,
                a.mode,
                a.source_clut.len() as u32,
                u32::from(a.op[0]),
                u32::from(a.op[1]),
                u32::from(a.op[2]),
            ]);
            arithmetic.extend(
                a.source_clut
                    .iter()
                    .chain(&a.destination_clut)
                    .flatten()
                    .map(|v| u32::from(*v)),
            );
            arithmetic.extend(&a.background);
            inputs.retain(|(binding, _, _)| *binding != 1);
            inputs.push((7, &arithmetic, wgpu::BufferUsages::STORAGE));
        }
        let mut deep = Vec::new();
        if let Some(d) = &command.deep_mask {
            deep.extend([
                d.source_depth,
                d.destination_depth,
                d.mask_depth,
                d.mode,
                d.source_clut.len() as u32,
                d.destination_clut.len() as u32,
                d.mask_clut.len() as u32,
                d.mask_width,
                d.mask_height,
                width,
                height,
            ]);
            deep.extend(
                d.source_clut
                    .iter()
                    .chain(&d.destination_clut)
                    .chain(&d.mask_clut)
                    .flatten()
                    .map(|v| u32::from(*v)),
            );
            deep.extend(&d.mask_pixels);
            deep.extend(&d.mask_columns);
            deep.extend(&d.mask_rows);
            deep.extend(&d.background);
            inputs.retain(|(binding, _, _)| *binding != 1);
            inputs.push((8, &deep, wgpu::BufferUsages::STORAGE));
        }
        self.dispatch_blit(pipeline, width, height, if cells { 2 } else { 1 }, &inputs)
    }
    fn dispatch_blit(
        &self,
        pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        output_words: u32,
        inputs: &[(u32, &[u32], wgpu::BufferUsages)],
    ) -> Result<Vec<u32>, String> {
        let output_bytes = u64::from(width) * u64::from(height) * 4 * u64::from(output_words);
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
        const INPUT_ROLES: [&str; 10] = [
            "blit-source",
            "blit-palette",
            "blit-columns",
            "blit-rows",
            "unused",
            "blit-geometry",
            "blit-coverage",
            "blit-arithmetic",
            "blit-mask",
            "blit-extra",
        ];
        let mut buffers: Vec<(u32, wgpu::Buffer)> = inputs
            .iter()
            .map(|(binding, words, usage)| {
                (
                    *binding,
                    self.buffers.upload(
                        &self.device,
                        &self.queue,
                        INPUT_ROLES[*binding as usize],
                        &super::wgpu_buffers::little_endian_word_bytes(words),
                        *usage,
                    ),
                )
            })
            .collect();
        let output = self.buffers.get(
            &self.device,
            "blit-output",
            output_bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
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
        let readback = self.buffers.get(
            &self.device,
            "QuickDraw bitmap readback",
            output_bytes,
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
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
            .slice(..output_bytes)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        super::wgpu_buffers::wait_for_readback(&self.device).map_err(|e| e.to_string())?;
        self.health.check().map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = readback
            .slice(..output_bytes)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let result = bytes
            .chunks_exact(4)
            .map(|p| u32::from_le_bytes(p.try_into().unwrap()))
            .collect();
        drop(bytes);
        readback.unmap();
        Ok(result)
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
                self.health
                    .record(super::renderer_recovery::Failure::from(e.to_string()));
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
        self.health.check().map_err(|e| e.to_string())?;
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
        let clip_buffer = self.buffers.upload(
            &self.device,
            &self.queue,
            "QuickDraw clip rows",
            &clip_bytes,
            wgpu::BufferUsages::STORAGE,
        );
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
        let edge_buffer = self.buffers.upload(
            &self.device,
            &self.queue,
            "QuickDraw polygon edges",
            &edge_bytes,
            wgpu::BufferUsages::STORAGE,
        );
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
        let products = self.buffers.get(
            &self.device,
            "QuickDraw rounded polygon products",
            product_bytes,
            wgpu::BufferUsages::STORAGE,
        );
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
        let mask_buffer = self.buffers.upload(
            &self.device,
            &self.queue,
            "QuickDraw glyph coverage",
            &mask_bytes,
            wgpu::BufferUsages::STORAGE,
        );
        let input = super::wgpu_buffers::little_endian_word_bytes(background);
        let input = self.buffers.upload(
            &self.device,
            &self.queue,
            "QuickDraw background",
            &input,
            wgpu::BufferUsages::STORAGE,
        );
        let output = self.buffers.get(
            &self.device,
            "QuickDraw result",
            output_bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
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
        let uniform = self.buffers.upload(
            &self.device,
            &self.queue,
            "QuickDraw geometry",
            &uniform,
            wgpu::BufferUsages::UNIFORM,
        );
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
        let readback = self.buffers.get(
            &self.device,
            "QuickDraw readback",
            output_bytes,
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
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
            .slice(..output_bytes)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        super::wgpu_buffers::wait_for_readback(&self.device).map_err(|e| e.to_string())?;
        self.health.check().map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = readback
            .slice(..output_bytes)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let result = bytes
            .chunks_exact(8)
            .map(|p| QuickDrawGpuPixel {
                value: u32::from_le_bytes(p[..4].try_into().unwrap()),
                covered: u32::from_le_bytes(p[4..].try_into().unwrap()) != 0,
            })
            .collect();
        drop(bytes);
        readback.unmap();
        Ok(result)
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
                            assert_eq!(
                                state.render_packed_blit(&command).unwrap(),
                                expected,
                                "depth={depth},sb={source_bit},db={destination_bit},width={width},mapped={mapped}"
                            );
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
            clip: None,
            arithmetic: None,
            deep_mask: None,
        };
        assert_eq!(state.render_blit(&raw).unwrap(), vec![u32::MAX]);
    }

    #[test]
    fn gpu_deep_mask_blends_match_cpu_all_depths_boolean_modes_and_coverage() {
        use systemless::quickdraw::gpu::{QuickDrawGpuBlitClip, QuickDrawGpuDeepMask};
        fn rgb(depth: u32, pixel: u32, clut: &[[u16; 3]]) -> [u16; 3] {
            match depth {
                1 => {
                    if pixel == 0 {
                        [65535; 3]
                    } else {
                        [0; 3]
                    }
                }
                2 | 4 | 8 => clut[pixel as usize],
                16 => [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31]
                    .map(|v| (f64::from(v) * 65535.0 / 31.0).round() as u16),
                32 => [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255].map(|v| v as u16 * 257),
                _ => unreachable!(),
            }
        }
        fn encode(depth: u32, rgb: [u16; 3], clut: &[[u16; 3]]) -> u32 {
            match depth {
                1 => {
                    let black: u64 = rgb.iter().map(|v| u64::from(*v).pow(2)).sum();
                    let white: u64 = rgb.iter().map(|v| u64::from(65535 - *v).pow(2)).sum();
                    u32::from(black <= white)
                }
                16 => {
                    (u32::from(rgb[0] / 2048) << 10)
                        | (u32::from(rgb[1] / 2048) << 5)
                        | u32::from(rgb[2] / 2048)
                }
                32 => {
                    (u32::from(rgb[0] / 256) << 16)
                        | (u32::from(rgb[1] / 256) << 8)
                        | u32::from(rgb[2] / 256)
                }
                _ => {
                    clut.iter()
                        .enumerate()
                        .min_by_key(|(_, c)| {
                            (0..3)
                                .map(|j| (i32::from(c[j] / 2048) - i32::from(rgb[j] / 2048)).pow(2))
                                .sum::<i32>()
                        })
                        .unwrap()
                        .0 as u32
                }
            }
        }
        fn boolean(s: u32, d: u32, depth: u32, mode: u32) -> u32 {
            let bits = if depth == 32 {
                u32::MAX
            } else {
                (1 << depth) - 1
            };
            match mode {
                1 => s | d,
                2 => s ^ d,
                3 => !s & d & bits,
                4 => !s & bits,
                5 => (!s & bits) | d,
                6 => (!s & bits) ^ d,
                7 => s & d,
                _ => s,
            }
        }
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
        let depths = [1u32, 2, 4, 8, 16, 32];
        let mut cases = Vec::new();
        for s in depths {
            for d in depths {
                for m in depths {
                    cases.push((s, d, m, 0));
                }
            }
        }
        for mode in [0, 1, 2, 3, 4, 5, 6, 7, 36] {
            for (s, d, m) in [
                (1, 32, 1),
                (32, 1, 8),
                (16, 8, 16),
                (8, 16, 32),
                (8, 4, 8),
                (32, 32, 1),
                (32, 32, 8),
                (4, 2, 4),
            ] {
                cases.push((s, d, m, mode));
            }
        }
        for (source_depth, destination_depth, mask_depth, mode) in cases {
            let limit = |d| if d == 32 { u32::MAX } else { (1u32 << d) - 1 };
            let clut = |d| -> Vec<[u16; 3]> {
                if matches!(d, 2 | 4 | 8) {
                    (0..1usize << d)
                        .map(|i| {
                            if i == 0 {
                                [65535; 3]
                            } else if i == 1 {
                                [0; 3]
                            } else {
                                [
                                    ((i / 2 * 7 % 32) * 2048 + i % 2) as u16,
                                    ((i / 2 * 11 % 32) * 2048 + i % 2) as u16,
                                    ((i / 2 * 17 % 32) * 2048 + i % 2) as u16,
                                ]
                            }
                        })
                        .collect()
                } else {
                    vec![]
                }
            };
            let pixels: Vec<u32> = (0..12u32)
                .map(|i| i.wrapping_mul(2654435761) & limit(source_depth))
                .collect();
            let background = (0..12u32)
                .map(|i| i.wrapping_mul(123456789).rotate_left(13) & limit(destination_depth))
                .collect();
            let mask_pixels = (0..6)
                .map(|i| match mask_depth {
                    1 => u32::from(i % 3 != 0),
                    2 | 4 | 8 => i % 3,
                    16 => [65535, 0, 0x3d2a][i as usize % 3],
                    32 => [0xa5ffffff, 0x6b000000, 0x8c7f10e3][i as usize % 3],
                    _ => unreachable!(),
                })
                .collect();
            let d = QuickDrawGpuDeepMask {
                source_depth,
                destination_depth,
                mask_depth,
                mode,
                source_clut: clut(source_depth)
                    .into_iter()
                    .map(|c| [c[1], c[2], c[0]])
                    .collect(),
                destination_clut: clut(destination_depth).into_iter().rev().collect(),
                mask_clut: clut(mask_depth),
                mask_width: 3,
                mask_height: 2,
                mask_pixels,
                mask_columns: vec![2, 0, 1, 2],
                mask_rows: vec![1, 0, 1],
                background,
            };
            let columns = vec![[0, 1], [3, 4], [1, 2], [2, 3]];
            let rows = vec![[2, 3], [0, 1], [1, 2]];
            let coverage: Vec<u8> = (0..12).map(|i| u8::from(i % 5 != 0)).collect();
            let expected: Vec<QuickDrawGpuPixel> = (0..12)
                .map(|i| {
                    let raw = pixels[(rows[i / 4][0] * 4 + columns[i % 4][0]) as usize];
                    let dst = d.background[i];
                    let mask =
                        d.mask_pixels[(d.mask_rows[i / 4] * 3 + d.mask_columns[i % 4]) as usize];
                    let s = rgb(source_depth, raw, &d.source_clut);
                    let target = rgb(destination_depth, dst, &d.destination_clut);
                    let value = if mask_depth == 1 {
                        boolean(
                            encode(destination_depth, s, &d.destination_clut),
                            dst,
                            destination_depth,
                            mode,
                        )
                    } else {
                        let s = if (1..=7).contains(&mode) {
                            rgb(
                                destination_depth,
                                boolean(
                                    encode(destination_depth, s, &d.destination_clut),
                                    dst,
                                    destination_depth,
                                    mode,
                                ),
                                &d.destination_clut,
                            )
                        } else {
                            s
                        };
                        let mask = rgb(mask_depth, mask, &d.mask_clut);
                        let color = std::array::from_fn(|c| {
                            ((f64::from(s[c]) * f64::from(65535 - mask[c])
                                + f64::from(target[c]) * f64::from(mask[c]))
                                / 65535.0)
                                .round() as u16
                        });
                        encode(destination_depth, color, &d.destination_clut)
                    };
                    QuickDrawGpuPixel {
                        value,
                        covered: coverage[i] != 0 && (mask_depth != 1 || mask != 0),
                    }
                })
                .collect();
            let command = QuickDrawGpuBlit {
                width: 4,
                height: 3,
                pixels,
                columns,
                rows,
                palette: None,
                direct: None,
                clip: Some(QuickDrawGpuBlitClip {
                    coverage: Some(coverage),
                    transparent: None,
                }),
                arithmetic: None,
                deep_mask: Some(d),
            };
            assert_eq!(
                state.render_blit_cells(&command).unwrap(),
                expected,
                "source={source_depth},destination={destination_depth},mask={mask_depth},mode={mode}"
            );
            assert!(state.render_blit(&command).is_err());
            let mut invalid = command;
            invalid.deep_mask.as_mut().unwrap().mask_rows.pop();
            assert!(state.render_blit_cells(&invalid).is_err());
        }
    }

    #[test]
    fn gpu_arithmetic_bitmap_modes_match_full_precision_cpu_rules() {
        use systemless::quickdraw::gpu::{QuickDrawGpuArithmetic, QuickDrawGpuBlitClip};
        fn color(depth: u32, pixel: u32, clut: &[[u16; 3]]) -> [u16; 3] {
            match depth {
                2 | 4 | 8 => clut[pixel as usize],
                16 => [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31]
                    .map(|v| ((f64::from(v) * 65535.0 / 31.0).round()) as u16),
                32 => [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255].map(|v| v as u16 * 257),
                _ => unreachable!(),
            }
        }
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
        for depth in [1u32, 2, 4, 8, 16, 32] {
            for mode in [32u32, 33, 34, 35, 37, 38, 39] {
                for op in [[0u16; 3], [65535; 3], [1, 32768, 60000]] {
                    let limit = if depth == 32 {
                        u32::MAX
                    } else {
                        (1 << depth) - 1
                    };
                    let count = if matches!(depth, 2 | 4 | 8) {
                        1usize << depth
                    } else {
                        0
                    };
                    let source_clut = (0..count)
                        .map(|i| {
                            [
                                ((i * 1327 + 511) % 65536) as u16,
                                ((i * 8191 + 65535) % 65536) as u16,
                                ((i * 37 + 29) % 65536) as u16,
                            ]
                        })
                        .collect();
                    let destination_clut = (0..count)
                        .map(|i| {
                            [
                                ((i / 2 * 7 % 32) * 2048 + i % 2) as u16,
                                ((i / 2 * 11 % 32) * 2048 + i % 2) as u16,
                                ((i / 2 * 17 % 32) * 2048 + i % 2) as u16,
                            ]
                        })
                        .collect();
                    let pixels: Vec<u32> = (0..279u32)
                        .map(|i| {
                            if i % 7 == 0 {
                                limit
                            } else {
                                i.wrapping_mul(2654435761) & limit
                            }
                        })
                        .collect();
                    let background: Vec<u32> = (0..279u32)
                        .map(|i| {
                            if i % 11 == 0 {
                                0
                            } else {
                                i.wrapping_mul(123456789).rotate_left(13) & limit
                            }
                        })
                        .collect();
                    let a = QuickDrawGpuArithmetic {
                        depth,
                        mode,
                        op,
                        source_clut,
                        destination_clut,
                        background,
                    };
                    let mask: Vec<u8> = (0..279).map(|i| u8::from(i % 5 != 0)).collect();
                    let expected: Vec<QuickDrawGpuPixel> = pixels
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let d = a.background[i];
                            let value = if depth == 1 {
                                match mode {
                                    32 => *s,
                                    33 | 37 => d & !*s,
                                    34 | 38 => d ^ s,
                                    _ => d | s,
                                }
                            } else {
                                let src = color(depth, *s, &a.source_clut);
                                let dst = color(depth, d, &a.destination_clut);
                                let rgb: [u16; 3] = std::array::from_fn(|c| match mode {
                                    32 => {
                                        ((u64::from(src[c]) * u64::from(op[c])
                                            + u64::from(dst[c]) * (65536 - u64::from(op[c])))
                                            >> 16) as u16
                                    }
                                    33 => src[c].saturating_add(dst[c]).min(op[c]),
                                    34 => src[c].wrapping_add(dst[c]),
                                    35 => src[c].saturating_sub(dst[c]).max(op[c]),
                                    37 => src[c].max(dst[c]),
                                    38 => src[c].wrapping_sub(dst[c]),
                                    _ => src[c].min(dst[c]),
                                });
                                match depth {
                                    16 => {
                                        (u32::from(rgb[0] / 2048) << 10)
                                            | (u32::from(rgb[1] / 2048) << 5)
                                            | u32::from(rgb[2] / 2048)
                                    }
                                    32 => {
                                        (u32::from(rgb[0] / 256) << 16)
                                            | (u32::from(rgb[1] / 256) << 8)
                                            | u32::from(rgb[2] / 256)
                                    }
                                    _ => {
                                        a.destination_clut
                                            .iter()
                                            .enumerate()
                                            .min_by_key(|(_, c)| {
                                                (0..3)
                                                    .map(|j| {
                                                        (i32::from(c[j] / 2048)
                                                            - i32::from(rgb[j] / 2048))
                                                        .pow(2)
                                                    })
                                                    .sum::<i32>()
                                            })
                                            .unwrap()
                                            .0 as u32
                                    }
                                }
                            };
                            QuickDrawGpuPixel {
                                value,
                                covered: mask[i] != 0,
                            }
                        })
                        .collect();
                    let command = QuickDrawGpuBlit {
                        width: 31,
                        height: 9,
                        pixels,
                        columns: (0..31).map(|x| [x, x + 1]).collect(),
                        rows: (0..9).map(|y| [y, y + 1]).collect(),
                        palette: None,
                        direct: None,
                        clip: Some(QuickDrawGpuBlitClip {
                            coverage: Some(mask),
                            transparent: None,
                        }),
                        arithmetic: Some(a),
                        deep_mask: None,
                    };
                    assert_eq!(
                        state.render_blit_cells(&command).unwrap(),
                        expected,
                        "depth={depth},mode={mode},op={op:?}"
                    );
                    assert!(state.render_blit(&command).is_err());
                    let mut invalid = command.clone();
                    invalid.arithmetic.as_mut().unwrap().background.pop();
                    assert!(state.render_blit_cells(&invalid).is_err());
                    let mut invalid = command;
                    invalid.arithmetic.as_mut().unwrap().mode = 36;
                    assert!(state.render_blit_cells(&invalid).is_err());
                }
            }
        }
    }

    #[test]
    fn gpu_masked_bitmap_cells_apply_source_keys_before_color_conversion() {
        use systemless::quickdraw::gpu::{QuickDrawGpuBlitClip, QuickDrawGpuDirectColor};
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
        for direct in [false, true] {
            for masked in [false, true] {
                for transparent in [false, true] {
                    let pixels = if direct {
                        vec![
                            0x00ff_ffff,
                            0xaaff_ffff,
                            0xff00_0000,
                            0x0001_0203,
                            0x00ff_ffff,
                            0xa501_0203,
                        ]
                    } else {
                        vec![1, 2, 1, 3, 1, 4]
                    };
                    let columns = vec![[0, 1], [2, 3], [1, 2], [0, 1]];
                    let rows = vec![[0, 1], [1, 2], [0, 1]];
                    let mask: Vec<u8> = (0..12).map(|i| if i % 3 == 0 { 0 } else { 2 }).collect();
                    let key = if direct { 0x00ff_ffff } else { 1 };
                    let source_mask = if direct { 0x00ff_ffff } else { u32::MAX };
                    let palette =
                        (!direct).then(|| std::array::from_fn(|i| if i == 1 { 200 } else { 1 }));
                    let command = QuickDrawGpuBlit {
                        width: 3,
                        height: 2,
                        pixels,
                        columns,
                        rows,
                        palette,
                        direct: direct.then_some(QuickDrawGpuDirectColor {
                            source_depth: 32,
                            destination_depth: 16,
                            palette: vec![],
                        }),
                        clip: Some(QuickDrawGpuBlitClip {
                            coverage: masked.then_some(mask.clone()),
                            transparent: transparent.then_some([key, source_mask]),
                        }),
                        arithmetic: None,
                        deep_mask: None,
                    };
                    let actual = state.render_blit_cells(&command).unwrap();
                    for (i, cell) in actual.iter().enumerate() {
                        let raw = command.pixels
                            [(command.rows[i / 4][0] * 3 + command.columns[i % 4][0]) as usize];
                        let covered =
                            (!masked || mask[i] != 0) && (!transparent || raw & source_mask != key);
                        let value = if direct {
                            ((raw >> 19 & 31) << 10) | ((raw >> 11 & 31) << 5) | (raw >> 3 & 31)
                        } else {
                            command.palette.as_ref().unwrap()[raw as usize]
                        };
                        assert_eq!(
                            *cell,
                            QuickDrawGpuPixel { value, covered },
                            "direct={direct},masked={masked},transparent={transparent},cell={i}"
                        );
                    }
                    assert!(state.render_blit(&command).is_err());
                    let mut invalid = command.clone();
                    invalid.clip.as_mut().unwrap().coverage = Some(vec![1]);
                    assert!(state.render_blit_cells(&invalid).is_err());
                    let mut invalid = command;
                    invalid.clip.as_mut().unwrap().transparent = Some([1, 0]);
                    assert!(state.render_blit_cells(&invalid).is_err());
                }
            }
        }
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
                    clip: None,
                    arithmetic: None,
                    deep_mask: None,
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
                                let black: u64 =
                                    rgb.iter().map(|c| u64::from(c * 257).pow(2)).sum();
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
            let palette = mapped
                .then(|| std::array::from_fn(|i| 0xff00_0000 | ((255 - i as u32) * 0x010101)));
            let command = QuickDrawGpuBlit {
                width: 23,
                height: 17,
                pixels,
                columns: vec![[0, 1], [1, 5], [3, 4], [22, 23], [22, 23]],
                rows: vec![[0, 1], [1, 7], [9, 17], [16, 17]],
                palette,
                direct: None,
                clip: None,
                arithmetic: None,
                deep_mask: None,
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
