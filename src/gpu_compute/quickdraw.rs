//! Guest-visible QuickDraw rasterization and bitmap transfer kernels.
use super::{
    little_endian_word_bytes, read_mapped, BindingRole, ComputeHealth, Scopes, ScratchBuffers,
};
use crate::quickdraw::gpu::{
    QuickDrawGpuBlit, QuickDrawGpuCommand, QuickDrawGpuPackedCopy, QuickDrawGpuPixel,
};
use std::sync::Arc;

type PipelineCache = std::cell::RefCell<Option<Result<wgpu::ComputePipeline, String>>>;

/// QuickDraw rasterization and bitmap transfer kernels on one wgpu device.
pub struct QuickDrawCompute {
    health: Arc<dyn ComputeHealth>,
    pub buffers: ScratchBuffers,
    pub device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    polygon_products: wgpu::ComputePipeline,
    blit_pipeline: PipelineCache,
    cells_pipeline: PipelineCache,
    arithmetic_pipeline: PipelineCache,
    deep_mask_pipeline: PipelineCache,
    packed_pipeline: PipelineCache,
}
impl QuickDrawCompute {
    /// Bitmap pipelines compile on first use; shape pipelines compile here.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        health: Arc<dyn ComputeHealth>,
    ) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QuickDraw shapes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quickdraw.wgsl").into()),
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
        eprintln!(
            "[GPU] QuickDraw rasterization: {}",
            device.adapter_info().name
        );
        health.check()?;
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
    async fn create_blit_pipeline(&self, entry: &str) -> Result<wgpu::ComputePipeline, String> {
        let scopes = Scopes::push(&self.device);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("QuickDraw bitmap samples"),
                source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
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
        match scopes.pop(&*self.health).await {
            Some(error) => Err(error),
            None => {
                eprintln!("[GPU] QuickDraw {entry} bitmap sampling enabled");
                Ok(pipeline)
            }
        }
    }
    /// Compile once per entry point. No cache borrow is held across the await.
    async fn cached_pipeline(
        &self,
        cached: &PipelineCache,
        entry: &str,
    ) -> Result<wgpu::ComputePipeline, String> {
        if cached.borrow().is_none() {
            let created = self.create_blit_pipeline(entry).await;
            cached.borrow_mut().get_or_insert(created);
        }
        cached.borrow().as_ref().unwrap().clone()
    }
    pub async fn render_blit(&self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        if command.clip.is_some() || command.arithmetic.is_some() || command.deep_mask.is_some() {
            return Err("clipped bitmap requires coverage readback".into());
        }
        self.render_blit_words(command, false).await
    }
    pub async fn render_blit_cells(
        &self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let words = self.render_blit_words(command, true).await?;
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
    async fn render_blit_words(
        &self,
        command: &QuickDrawGpuBlit,
        cells: bool,
    ) -> Result<Vec<u32>, String> {
        self.health.check()?;
        if !command.valid() {
            return Err("invalid QuickDraw bitmap blit".into());
        }
        let (cached, entry) = if cells && command.deep_mask.is_some() {
            (&self.deep_mask_pipeline, "blit_deep_mask")
        } else if cells && command.arithmetic.is_some() {
            (&self.arithmetic_pipeline, "blit_arithmetic")
        } else if cells {
            (&self.cells_pipeline, "blit_cells")
        } else {
            (&self.blit_pipeline, "blit")
        };
        let pipeline = self.cached_pipeline(cached, entry).await?;
        let scopes = Scopes::push(&self.device);
        let result = self.render_blit_inner(command, &pipeline, cells).await;
        match scopes.pop(&*self.health).await {
            Some(error) => Err(error),
            None => result,
        }
    }
    pub async fn render_packed_blit(
        &self,
        command: &QuickDrawGpuPackedCopy,
    ) -> Result<Vec<u8>, String> {
        self.health.check()?;
        if !command.valid() {
            return Err("invalid packed bitmap blit".into());
        }
        let pipeline = self
            .cached_pipeline(&self.packed_pipeline, "packed_copy")
            .await?;
        let scopes = Scopes::push(&self.device);
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
                &pipeline,
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
            .await
            .and_then(|pixels| {
                if pixels.iter().any(|p| *p > 255) {
                    return Err("packed bitmap returned invalid bytes".into());
                }
                Ok(pixels.into_iter().map(|p| p as u8).collect())
            });
        match scopes.pop(&*self.health).await {
            Some(error) => Err(error),
            None => result,
        }
    }
    async fn render_blit_inner(
        &self,
        command: &QuickDrawGpuBlit,
        pipeline: &wgpu::ComputePipeline,
        cells: bool,
    ) -> Result<Vec<u32>, String> {
        self.health.check()?;
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
                d.mode | (u32::from(d.classic_colors) << 8),
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
            .await
    }
    async fn dispatch_blit(
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
                        &little_endian_word_bytes(words),
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
        let bindings: Vec<_> = buffers
            .iter()
            .map(|(slot, buffer)| (*slot, buffer))
            .collect();
        let group = self
            .buffers
            .bind_group(&self.device, BindingRole::Bitmap, pipeline, &bindings);
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
        read_mapped(
            &*self.health,
            &self.device,
            &readback,
            output_bytes,
            |bytes| {
                bytes
                    .chunks_exact(4)
                    .map(|p| u32::from_le_bytes(p.try_into().unwrap()))
                    .collect()
            },
        )
        .await
    }
    pub async fn render(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let scopes = Scopes::push(&self.device);
        let result = self.render_inner(command, background).await;
        match scopes.pop(&*self.health).await {
            Some(error) => Err(error),
            None => result,
        }
    }
    async fn render_inner(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.health.check()?;
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
        let input = little_endian_word_bytes(background);
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
        let group = self.buffers.bind_group(
            &self.device,
            BindingRole::Raster,
            &self.pipeline,
            &[
                (0, &input),
                (1, &output),
                (2, &uniform),
                (3, &clip_buffer),
                (4, &edge_buffer),
                (5, &products),
                (6, &mask_buffer),
            ],
        );
        let readback = self.buffers.get(
            &self.device,
            "QuickDraw readback",
            output_bytes,
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if !edges.is_empty() {
            let product_group = self.buffers.bind_group(
                &self.device,
                BindingRole::PolygonProducts,
                &self.polygon_products,
                &[(2, &uniform), (4, &edge_buffer), (5, &products)],
            );
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
        read_mapped(
            &*self.health,
            &self.device,
            &readback,
            output_bytes,
            |bytes| {
                bytes
                    .chunks_exact(8)
                    .map(|p| QuickDrawGpuPixel {
                        value: u32::from_le_bytes(p[..4].try_into().unwrap()),
                        covered: u32::from_le_bytes(p[4..].try_into().unwrap()) != 0,
                    })
                    .collect()
            },
        )
        .await
    }
}
