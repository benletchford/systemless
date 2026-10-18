//! Resolve sparse retained glyph/QuickDraw coverage directly in the fragment shader.
use std::sync::Arc;
use systemless::memory::CompactPresentation;

pub(super) struct CompactRenderer {
    pipelines: [Option<wgpu::RenderPipeline>; 8],
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    #[cfg(test)]
    timestamps: Option<(wgpu::QuerySet, u32)>,
    #[cfg(target_os = "macos")]
    shared: Option<super::wgpu_shared::Pool>,
    prepared: Option<Prepared>,
    epoch: u64,
    layout: wgpu::BindGroupLayout,
    geometry: wgpu::Buffer,
    resources: Option<Resources>,
    source: Option<Arc<CompactPresentation>>,
    previous: Option<Arc<CompactPresentation>>,
    geometry_words: Option<[u32; 12]>,
    staging: wgpu::util::StagingBelt,
    primary: Option<wgpu::util::StagingBelt>,
}

struct Prepared {
    image: Arc<CompactPresentation>,
    crop: (u32, u32, u32, u32),
    viewport: (u32, u32, u32, u32),
    epoch: u64,
    cells: std::ops::Range<usize>,
    detail: std::ops::Range<usize>,
}

struct Resources {
    cells: wgpu::Buffer,
    detail: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl CompactRenderer {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let entries: Vec<_> = (0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 2 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("retained coverage"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("retained coverage"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("retained coverage"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_compact.wgsl").into()),
        });
        Self {
            geometry: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("retained geometry"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            staging: wgpu::util::StagingBelt::new(device.clone(), 64 * 1024),
            primary: device
                .features()
                .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS)
                .then(|| {
                    wgpu::util::StagingBelt::new_with_buffer_usages(
                        device.clone(),
                        64 * 1024,
                        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::UNIFORM,
                    )
                }),
            #[cfg(test)]
            timestamps: None,
            #[cfg(target_os = "macos")]
            shared: super::wgpu_shared::Pool::new(device, _queue),
            prepared: None,
            epoch: 0,
            resources: None,
            source: None,
            previous: None,
            geometry_words: None,
            pipelines: std::array::from_fn(|_| None),
            shader,
            pipeline_layout,
            format,
            layout,
        }
    }

    /// Integer coverage is exact within the same accumulator limits as Metal/D3D.
    /// Invalid or out-of-range packets retain the existing CPU resolve fallback.
    fn supports_layout(
        image: &CompactPresentation,
        crop: (u32, u32, u32, u32),
        viewport: (u32, u32, u32, u32),
        limits: &wgpu::Limits,
    ) -> bool {
        let (x, y, w, h) = crop;
        let (_, _, dw, dh) = viewport;
        if image.width == 0
            || image.height == 0
            || !(1..=4).contains(&image.scale)
            || w == 0
            || h == 0
            || dw == 0
            || dh == 0
            || x.checked_add(w).is_none_or(|end| end > image.width)
            || y.checked_add(h).is_none_or(|end| end > image.height)
            || image.cells.len() as u64 != u64::from(image.width) * u64::from(image.height)
        {
            return false;
        }
        let sw = u64::from(w) * u64::from(image.scale);
        let sh = u64::from(h) * u64::from(image.scale);
        let total = if sw > dw.into() { sw } else { 1 } * if sh > dh.into() { sh } else { 1 };
        if total > u64::from(u32::MAX) / 256
            || (2 * u64::from(dw) + 1) * sw > u32::MAX.into()
            || (2 * u64::from(dh) + 1) * sh > u32::MAX.into()
        {
            return false;
        }
        [image.cells.len(), image.detail.len().max(1)]
            .into_iter()
            .all(|n| (n as u64) * 4 <= u64::from(limits.max_storage_buffer_binding_size))
    }

    #[cfg(test)]
    fn supports(
        image: &CompactPresentation,
        crop: (u32, u32, u32, u32),
        viewport: (u32, u32, u32, u32),
        limits: &wgpu::Limits,
    ) -> bool {
        Self::supports_layout(image, crop, viewport, limits) && valid_cells(image, &image.cells)
    }

    pub fn prepare(
        &mut self,
        image: &Arc<CompactPresentation>,
        crop: (u32, u32, u32, u32),
        viewport: (u32, u32, u32, u32),
        limits: &wgpu::Limits,
    ) -> bool {
        if self.prepared.as_ref().is_some_and(|p| {
            p.epoch == self.epoch
                && Arc::ptr_eq(&p.image, image)
                && p.crop == crop
                && p.viewport == viewport
        }) {
            return true;
        }
        self.prepared = None;
        if !Self::supports_layout(image, crop, viewport, limits) {
            return false;
        }
        #[cfg(target_os = "macos")]
        let shared = self
            .shared
            .as_ref()
            .is_some_and(|pool| pool.supports(image, limits));
        #[cfg(not(target_os = "macos"))]
        let shared = false;
        let same_image = self
            .previous
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, image));
        let cells = if same_image {
            0..0
        } else if shared {
            0..image.cells.len()
        } else {
            changed_words(
                self.previous.as_ref().map(|p| p.cells.as_slice()),
                &image.cells,
            )
        };
        let detail = if same_image {
            0..0
        } else if shared {
            0..image.detail.len()
        } else {
            changed_words(
                self.previous.as_ref().map(|p| p.detail.as_slice()),
                &image.detail,
            )
        };
        // Previously accepted cells remain valid only if their tile size and
        // accessible detail range are unchanged. RGB detail words are data,
        // not further indices, so changing those words needs no index scan.
        if !valid_changed_cells(self.previous.as_deref(), image, cells.clone()) {
            return false;
        }
        self.prepared = Some(Prepared {
            image: image.clone(),
            crop,
            viewport,
            epoch: self.epoch,
            cells,
            detail,
        });
        true
    }

    fn encode_shared(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Arc<CompactPresentation>,
        crop: (u32, u32, u32, u32),
        viewport: (u32, u32, u32, u32),
        target: &wgpu::TextureView,
    ) -> bool {
        let limits = device.limits();
        let alignment = u64::from(
            limits
                .min_storage_buffer_offset_alignment
                .max(limits.min_uniform_buffer_offset_alignment),
        );
        let align = |offset: u64| offset.div_ceil(alignment) * alignment;
        let cells_size = wgpu::BufferSize::new(image.cells.len().max(1) as u64 * 4).unwrap();
        let detail_size = wgpu::BufferSize::new(image.detail.len().max(1) as u64 * 4).unwrap();
        let detail_offset = align(cells_size.get());
        let geometry_offset = align(detail_offset + detail_size.get());
        let total = geometry_offset + 48;
        if total > limits.max_buffer_size {
            return false;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut encoder = device.create_command_encoder(&Default::default());
        let belt = self.primary.as_mut().unwrap();
        // One mapped allocation and completion callback for the whole packet.
        // Each binding still exposes only its own validated, aligned range.
        let slice = belt.allocate(
            wgpu::BufferSize::new(total).unwrap(),
            wgpu::BufferSize::new(alignment).unwrap(),
        );
        let buffer = slice.buffer().clone();
        let base = slice.offset();
        let upload = |offset: u64, size: wgpu::BufferSize, words: &[u32]| {
            let offset = base + offset;
            let mut bytes = buffer
                .slice(offset..offset + size.get())
                .get_mapped_range_mut()
                .unwrap();
            if words.is_empty() {
                bytes.copy_from_slice(&[0; 4]);
            } else {
                bytes.copy_from_slice(super::wgpu_buffers::word_bytes(words));
            }
            (buffer.clone(), offset, size)
        };
        let cells = upload(0, cells_size, &image.cells);
        let detail = upload(detail_offset, detail_size, &image.detail);
        let geometry = upload(
            geometry_offset,
            wgpu::BufferSize::new(48).unwrap(),
            &[
                image.width,
                image.height,
                image.scale,
                0,
                crop.0,
                crop.1,
                crop.2,
                crop.3,
                viewport.0,
                viewport.1,
                viewport.2,
                viewport.3,
            ],
        );
        let entries: Vec<_> = [&cells, &detail, &geometry]
            .into_iter()
            .enumerate()
            .map(|(binding, (buffer, offset, size))| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: *offset,
                    size: Some(*size),
                }),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shared retained coverage"),
            layout: &self.layout,
            entries: &entries,
        });
        draw(
            &mut encoder,
            self.pipelines
                [(image.scale - 1) as usize + if image.detail.is_empty() { 0 } else { 4 }]
            .as_ref()
            .unwrap(),
            &group,
            target,
            viewport,
            {
                #[cfg(test)]
                {
                    timestamp_writes(&self.timestamps)
                }
                #[cfg(not(test))]
                {
                    None
                }
            },
        );
        belt.finish_and_recall_on_submit(&encoder);
        queue.submit([encoder.finish()]);
        true
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Arc<CompactPresentation>,
        crop: (u32, u32, u32, u32),
        viewport: (u32, u32, u32, u32),
        target: &wgpu::TextureView,
    ) {
        assert!(
            self.prepare(image, crop, viewport, &device.limits()),
            "invalid retained GPU packet"
        );
        let index = (image.scale - 1) as usize + if image.detail.is_empty() { 0 } else { 4 };
        if self.pipelines[index].is_none() {
            self.pipelines[index] = Some(super::wgpu_present::pipeline_with_module(
                device,
                self.format,
                &self.shader,
                Some(&self.pipeline_layout),
                &[
                    ("sample_scale", f64::from(image.scale)),
                    (
                        "uniform_fast_path",
                        if image.detail.is_empty() { 1. } else { 0. },
                    ),
                ],
            ));
        }
        #[cfg(target_os = "macos")]
        if let Some(pool) = &mut self.shared {
            let geometry = [
                image.width,
                image.height,
                image.scale,
                0,
                crop.0,
                crop.1,
                crop.2,
                crop.3,
                viewport.0,
                viewport.1,
                viewport.2,
                viewport.3,
            ];
            if pool.render(
                &self.layout,
                image,
                &geometry,
                self.pipelines[index].as_ref().unwrap(),
                target,
                viewport,
                {
                    #[cfg(test)]
                    {
                        timestamp_writes(&self.timestamps)
                    }
                    #[cfg(not(test))]
                    {
                        None
                    }
                },
            ) {
                self.previous = Some(image.clone());
                self.epoch = self.epoch.wrapping_add(1);
                return;
            }
        }
        let prepared = self.prepared.as_ref().unwrap();
        let cells_changed = prepared.cells.clone();
        let detail_changed = prepared.detail.clone();
        let full_upload = cells_changed.len() + detail_changed.len()
            > (image.cells.len() + image.detail.len()) / 2;
        if self.primary.is_some()
            && full_upload
            && self.encode_shared(device, queue, image, crop, viewport, target)
        {
            self.previous = Some(image.clone());
            self.epoch = self.epoch.wrapping_add(1);
            return;
        }
        if self.resources.as_ref().is_none_or(|resources| {
            resources.cells.size() < image.cells.len() as u64 * 4
                || resources.detail.size() < image.detail.len().max(1) as u64 * 4
        }) {
            let storage = |count: usize| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("retained coverage"),
                    size: (count.max(1) as u64 * 4).next_power_of_two(),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let cells = storage(image.cells.len());
            let detail = storage(image.detail.len());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("retained coverage"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: cells.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: detail.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.geometry.as_entire_binding(),
                    },
                ],
            });
            self.resources = Some(Resources {
                cells,
                detail,
                group,
            });
            self.source = None;
        }
        // Return completed transfer buffers to the belt without waiting on GPU
        // work. Queue::write_buffer otherwise allocates fresh staging per upload.
        let _ = device.poll(wgpu::PollType::Poll);
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut upload = |target: &wgpu::Buffer, words: &[u32], changed: std::ops::Range<usize>| {
            if let Some(size) = wgpu::BufferSize::new(changed.len() as u64 * 4) {
                self.staging
                    .write_buffer(&mut encoder, target, changed.start as u64 * 4, size)
                    .copy_from_slice(super::wgpu_buffers::word_bytes(&words[changed]));
            }
        };
        let resources = self.resources.as_ref().unwrap();
        if self
            .source
            .as_ref()
            .is_none_or(|source| !Arc::ptr_eq(source, image))
        {
            let same_previous = self
                .source
                .as_ref()
                .zip(self.previous.as_ref())
                .is_some_and(|(source, previous)| Arc::ptr_eq(source, previous));
            let cells = if same_previous {
                cells_changed
            } else {
                changed_words(
                    self.source.as_ref().map(|p| p.cells.as_slice()),
                    &image.cells,
                )
            };
            let detail = if same_previous {
                detail_changed
            } else {
                changed_words(
                    self.source.as_ref().map(|p| p.detail.as_slice()),
                    &image.detail,
                )
            };
            upload(&resources.cells, &image.cells, cells);
            upload(&resources.detail, &image.detail, detail);
            self.source = Some(image.clone());
        }
        let geometry = [
            image.width,
            image.height,
            image.scale,
            0,
            crop.0,
            crop.1,
            crop.2,
            crop.3,
            viewport.0,
            viewport.1,
            viewport.2,
            viewport.3,
        ];
        if self.geometry_words != Some(geometry) {
            upload(&self.geometry, &geometry, 0..geometry.len());
            self.geometry_words = Some(geometry);
        }
        draw(
            &mut encoder,
            self.pipelines
                [(image.scale - 1) as usize + if image.detail.is_empty() { 0 } else { 4 }]
            .as_ref()
            .unwrap(),
            &resources.group,
            target,
            viewport,
            {
                #[cfg(test)]
                {
                    timestamp_writes(&self.timestamps)
                }
                #[cfg(not(test))]
                {
                    None
                }
            },
        );
        self.staging.finish_and_recall_on_submit(&encoder);
        queue.submit([encoder.finish()]);
        self.previous = Some(image.clone());
        self.epoch = self.epoch.wrapping_add(1);
    }
}

#[cfg(test)]
fn timestamp_writes(
    timestamps: &Option<(wgpu::QuerySet, u32)>,
) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
    timestamps
        .as_ref()
        .map(|(query_set, index)| wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: Some(*index),
            end_of_pass_write_index: Some(*index + 1),
        })
}

fn valid_changed_cells(
    previous: Option<&CompactPresentation>,
    image: &CompactPresentation,
    changed: std::ops::Range<usize>,
) -> bool {
    let unchanged_bounds = previous.is_some_and(|previous| {
        previous.scale == image.scale && previous.detail.len() == image.detail.len()
    });
    valid_cells(
        image,
        if unchanged_bounds {
            &image.cells[changed]
        } else {
            &image.cells
        },
    )
}

fn valid_cells(image: &CompactPresentation, cells: &[u32]) -> bool {
    let tile = (image.scale * image.scale) as usize; // scale was validated as 1..=4
    // Uniform colors sort below every flagged detail reference. Therefore a
    // single vectorizable unsigned maximum checks all exclusive tile ends.
    // If even one tile cannot fit, only unflagged cells are allowed.
    let limit = image
        .detail
        .len()
        .checked_sub(tile)
        .map_or(0x7fffffff, |offset| {
            0x80000000 | offset.min(0x7fffffff) as u32
        });
    cells.iter().copied().max().unwrap_or(0) <= limit
}

/// Find the smallest upload containing all changed words. Binary slice
/// comparisons use vectorized equality over large spans, avoiding one branch
/// and bounds check per guest pixel (or thousands of tiny memcmp calls).
fn changed_words(previous: Option<&[u32]>, words: &[u32]) -> std::ops::Range<usize> {
    let Some(previous) = previous.filter(|previous| previous.len() == words.len()) else {
        return 0..words.len();
    };
    if previous == words {
        return 0..0;
    }
    let mut low = 0;
    let mut high = words.len();
    if previous[0] == words[0] {
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if previous[low..middle] == words[low..middle] {
                low = middle;
            } else {
                high = middle;
            }
        }
    }
    let start = low;
    high = words.len();
    if previous[high - 1] != words[high - 1] {
        return start..high;
    }
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if previous[middle..high] == words[middle..high] {
            high = middle;
        } else {
            low = middle;
        }
    }
    start..high
}

pub(super) fn draw(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
    viewport: (u32, u32, u32, u32),
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
) {
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("retained coverage"),
            timestamp_writes,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_viewport(
            viewport.0 as f32,
            viewport.1 as f32,
            viewport.2 as f32,
            viewport.3 as f32,
            0.,
            1.,
        );
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn changed_uploads_cover_block_boundaries_and_resized_packets() {
        let original: Vec<u32> = (0..10_000).collect();
        assert_eq!(changed_words(Some(&original), &original), 0..0);
        assert_eq!(changed_words(None, &original), 0..original.len());
        assert_eq!(
            changed_words(Some(&original[..100]), &original),
            0..original.len()
        );
        for position in [0, 63, 64, 65, 5000, 9998, 9999] {
            let mut changed = original.clone();
            changed[position] ^= 1;
            assert_eq!(
                changed_words(Some(&original), &changed),
                position..position + 1
            );
            changed[42] ^= 1;
            assert_eq!(
                changed_words(Some(&original), &changed),
                position.min(42)..position.max(42) + 1
            );
        }
    }

    pub(crate) fn benchmark_wgpu_presentation(text: bool) {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        eprintln!("[RENDER-BENCH] adapter={:?}", adapter.get_info());
        let measure_gpu = std::env::var_os("SYSTEMLESS_BENCH_GPU_TIMESTAMPS").is_some()
            && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let shared = matches!(
            adapter.get_info().device_type,
            wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu
        );
        let features = if shared {
            adapter.features() & wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
        } else {
            wgpu::Features::empty()
        } | if measure_gpu {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        }))
        .unwrap();
        let query_set = measure_gpu.then(|| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("presentation timing"),
                ty: wgpu::QueryType::Timestamp,
                count: 840,
            })
        });
        let mut renderer = CompactRenderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("presentation benchmark"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        {
            let image = Arc::new(crate::renderer_bench::presentation_fixture(text));
            // Alternate owned packets so each sample includes the upload cost
            // of a newly exported frame, like the default Metal measurement.
            let mut alternate = (*image).clone();
            alternate.cells[0] ^= 0x010101;
            let images = [image.clone(), Arc::new(alternate)];
            let mut samples = Vec::new();
            let mut submitted = Vec::new();
            let mut prepared = Vec::new();
            let mut encoded = Vec::new();
            for iteration in 0..420 {
                renderer.timestamps = query_set
                    .as_ref()
                    .map(|query| (query.clone(), iteration as u32 * 2));
                let image = &images[iteration % 2];
                let start = std::time::Instant::now();
                assert!(renderer.prepare(
                    image,
                    (0, 0, 640, 480),
                    (0, 0, 800, 600),
                    &device.limits()
                ));
                let prepare_elapsed = start.elapsed();
                renderer.encode(
                    &device,
                    &queue,
                    image,
                    (0, 0, 640, 480),
                    (0, 0, 800, 600),
                    &view,
                );
                let submit_elapsed = start.elapsed();
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                if iteration >= 120 {
                    samples.push(start.elapsed());
                    submitted.push(submit_elapsed);
                    prepared.push(prepare_elapsed);
                    encoded.push(submit_elapsed - prepare_elapsed);
                }
            }
            if let Some(query_set) = &query_set {
                let size = 840 * 8;
                let resolved = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.resolve_query_set(query_set, 0..840, &resolved, 0);
                encoder.copy_buffer_to_buffer(&resolved, 0, &readback, 0, size);
                queue.submit([encoder.finish()]);
                readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, |result| result.unwrap());
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                let bytes = readback.slice(..).get_mapped_range().unwrap();
                let mut times = Vec::new();
                for pair in bytes.chunks_exact(16).skip(120) {
                    let start = u64::from_le_bytes(pair[..8].try_into().unwrap());
                    let end = u64::from_le_bytes(pair[8..].try_into().unwrap());
                    times.push(std::time::Duration::from_secs_f64(
                        end.saturating_sub(start) as f64 * f64::from(queue.get_timestamp_period())
                            / 1e9,
                    ));
                }
                crate::renderer_bench::report(
                    if text {
                        "present-text-gpu-time"
                    } else {
                        "present-ui-gpu-time"
                    },
                    "gpu",
                    &mut times,
                );
            }
            crate::renderer_bench::report(
                if text { "present-text" } else { "present-ui" },
                "gpu",
                &mut samples,
            );
            crate::renderer_bench::report(
                if text {
                    "present-text-submit"
                } else {
                    "present-ui-submit"
                },
                "gpu",
                &mut submitted,
            );
            crate::renderer_bench::report(
                if text {
                    "present-text-prepare"
                } else {
                    "present-ui-prepare"
                },
                "gpu",
                &mut prepared,
            );
            crate::renderer_bench::report(
                if text {
                    "present-text-encode"
                } else {
                    "present-ui-encode"
                },
                "gpu",
                &mut encoded,
            );
        }
    }
    #[test]
    fn gpu_retained_coverage_matches_cpu_at_fractional_scales_and_crops() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                eprintln!("SKIP retained GPU readback: {error}");
                return;
            }
        };
        for shared in [false, true] {
            if shared
                && !adapter
                    .features()
                    .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS)
            {
                continue;
            }
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_features: if shared {
                        wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
                    } else {
                        wgpu::Features::empty()
                    },
                    ..Default::default()
                }))
                .unwrap();
            check_retained_coverage(device, queue);
        }
    }

    fn check_retained_coverage(device: wgpu::Device, queue: wgpu::Queue) {
        let mut renderer = CompactRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        for scale in 1..=4 {
            for retained in [false, true] {
                let mut image = CompactPresentation {
                    width: 7,
                    height: 5,
                    scale,
                    cells: Vec::new(),
                    detail: Vec::new(),
                };
                for i in 0..35u32 {
                    if retained && i % 3 == 0 {
                        image.cells.push(0x80000000 | image.detail.len() as u32);
                        image.detail.extend(
                            (0..scale * scale)
                                .map(|sample| (i * 819121 + sample * 123799) & 0xffffff),
                        );
                    } else {
                        image.cells.push((i * 987131) & 0xffffff);
                    }
                }
                let image = Arc::new(image);
                let mut changed = (*image).clone();
                *changed.cells.last_mut().unwrap() ^= 1;
                if let Some(word) = changed.detail.last_mut() {
                    *word ^= 0xabcdef;
                }
                for image in [image, Arc::new(changed)] {
                    for crop in [(0, 0, 7, 5), (1, 1, 5, 3)] {
                        for size in [(1, 1), (3, 2), (13, 9), (28, 21), (3, 17), (19, 2)] {
                            let viewport = (2, 3, size.0, size.1);
                            assert!(CompactRenderer::supports(
                                &image,
                                crop,
                                viewport,
                                &device.limits()
                            ));
                            let target = device.create_texture(&wgpu::TextureDescriptor {
                                label: None,
                                size: wgpu::Extent3d {
                                    width: size.0 + 4,
                                    height: size.1 + 6,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Rgba8Unorm,
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                    | wgpu::TextureUsages::COPY_SRC,
                                view_formats: &[],
                            });
                            renderer.encode(
                                &device,
                                &queue,
                                &image,
                                crop,
                                viewport,
                                &target.create_view(&Default::default()),
                            );
                            let row_bytes = 256u32;
                            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                                label: None,
                                size: u64::from(row_bytes) * u64::from(target.height()),
                                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            });
                            let mut encoder = device.create_command_encoder(&Default::default());
                            encoder.copy_texture_to_buffer(
                                target.as_image_copy(),
                                wgpu::TexelCopyBufferInfo {
                                    buffer: &readback,
                                    layout: wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(row_bytes),
                                        rows_per_image: Some(target.height()),
                                    },
                                },
                                target.size(),
                            );
                            queue.submit([encoder.finish()]);
                            let (tx, rx) = std::sync::mpsc::channel();
                            readback
                                .slice(..)
                                .map_async(wgpu::MapMode::Read, move |result| {
                                    tx.send(result).unwrap()
                                });
                            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                            rx.recv().unwrap().unwrap();
                            let bytes = readback.slice(..).get_mapped_range().unwrap();
                            let mut cropped = image.as_ref().clone();
                            cropped.width = crop.2;
                            cropped.height = crop.3;
                            cropped.cells.clear();
                            for y in crop.1..crop.1 + crop.3 {
                                let start = (y * image.width + crop.0) as usize;
                                cropped.cells.extend_from_slice(
                                    &image.cells[start..start + crop.2 as usize],
                                );
                            }
                            let mut expected = Vec::new();
                            assert!(cropped.render_argb_resized(size, &mut expected));
                            for y in 0..target.height() {
                                for x in 0..target.width() {
                                    let offset = (y * row_bytes + x * 4) as usize;
                                    let rgba = &bytes[offset..offset + 4];
                                    let actual = 0xff000000
                                        | (u32::from(rgba[0]) << 16)
                                        | (u32::from(rgba[1]) << 8)
                                        | u32::from(rgba[2]);
                                    let expected =
                                        if x >= 2 && x < 2 + size.0 && y >= 3 && y < 3 + size.1 {
                                            expected[((y - 3) * size.0 + x - 2) as usize]
                                        } else {
                                            0xff000000
                                        };
                                    assert_eq!(
                                        actual, expected,
                                        "scale={scale} crop={crop:?} size={size:?} pixel=({x},{y})"
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
    fn incremental_validation_rechecks_changed_indices_and_bounds() {
        let previous = CompactPresentation {
            width: 2,
            height: 1,
            scale: 2,
            cells: vec![0x80000000, 0],
            detail: vec![0; 4],
        };
        let mut image = previous.clone();
        image.cells[1] = 0x80000001;
        assert!(!valid_changed_cells(Some(&previous), &image, 1..2));
        image = previous.clone();
        image.detail.pop();
        assert!(!valid_changed_cells(Some(&previous), &image, 0..0));
        image = previous.clone();
        image.scale = 3;
        assert!(!valid_changed_cells(Some(&previous), &image, 0..0));
        image = previous.clone();
        image.detail.fill(0x123456);
        assert!(valid_changed_cells(Some(&previous), &image, 0..0));
        assert!(valid_changed_cells(None, &image, 0..0));
    }

    fn presentation_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => Some(super::super::wgpu_present::request_device(&adapter).unwrap()),
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                None
            }
        }
    }

    #[test]
    fn gpu_queued_retained_packets_preserve_every_frame() {
        let Some((device, queue)) = presentation_device() else {
            return;
        };
        let mut renderer = CompactRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("queued retained frames"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut frames = Vec::new();
        for frame in 0..24u32 {
            let color = (frame * 717137) & 0xffffff;
            let image = Arc::new(CompactPresentation {
                width: 8,
                height: 8,
                scale: 4,
                cells: (0..64)
                    .map(|cell| if cell == 63 { color } else { 0x80000000 })
                    .collect(),
                detail: (0..16)
                    .map(|sample| ((frame / 2) * 717137 + sample * 8137) & 0xffffff)
                    .collect(),
            });
            let mut expected = Vec::new();
            assert!(image.render_argb_resized((16, 16), &mut expected));
            // Detail matches the last presented packet in each pair, but
            // differs from the previous portable upload. Keep the resident
            // buffer's history separate from the shared presentation history.
            #[cfg(target_os = "macos")]
            let pool = if frame % 2 == 1 {
                renderer.shared.take()
            } else {
                None
            };
            renderer.encode(&device, &queue, &image, (0, 0, 8, 8), (0, 0, 16, 16), &view);
            #[cfg(target_os = "macos")]
            if pool.is_some() {
                renderer.shared = pool;
            }
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256 * 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(16),
                    },
                },
                target.size(),
            );
            queue.submit([encoder.finish()]);
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, |result| result.unwrap());
            frames.push((readback, expected));
        }
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        for (frame, (readback, expected)) in frames.iter().enumerate() {
            let bytes = readback.slice(..).get_mapped_range().unwrap();
            for y in 0..16 {
                for x in 0..16 {
                    let rgba = &bytes[y * 256 + x * 4..y * 256 + x * 4 + 4];
                    let actual = 0xff000000
                        | (u32::from(rgba[0]) << 16)
                        | (u32::from(rgba[1]) << 8)
                        | u32::from(rgba[2]);
                    assert_eq!(
                        actual,
                        expected[y * 16 + x],
                        "queued frame={frame} pixel=({x},{y})"
                    );
                }
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(pool) = &renderer.shared {
            let (allocated, ready) = pool.status();
            assert!((1..=3).contains(&allocated));
            assert_eq!(allocated, ready);
        }
    }

    #[test]
    fn gpu_dense_retained_packet_matches_cpu_at_presentation_size() {
        let Some((device, queue)) = presentation_device() else {
            return;
        };
        let mut renderer = CompactRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dense retained oracle"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let image = Arc::new(crate::renderer_bench::presentation_fixture(true));
        let mut changed = (*image).clone();
        changed.cells[0] ^= 0x010101;
        changed.detail[17] ^= 0x234567;
        *changed.detail.last_mut().unwrap() ^= 0xabcdef;
        for image in [image, Arc::new(changed)] {
            let mut expected = Vec::new();
            assert!(image.render_argb_resized((800, 600), &mut expected));
            renderer.encode(
                &device,
                &queue,
                &image,
                (0, 0, 640, 480),
                (0, 0, 800, 600),
                &view,
            );
            let row_bytes = 3200u32.div_ceil(256) * 256;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(row_bytes) * 600,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_bytes),
                        rows_per_image: Some(600),
                    },
                },
                target.size(),
            );
            queue.submit([encoder.finish()]);
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, |result| result.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let bytes = readback.slice(..).get_mapped_range().unwrap();
            for y in 0..600usize {
                for x in 0..800usize {
                    let offset = y * row_bytes as usize + x * 4;
                    let rgba = &bytes[offset..offset + 4];
                    let actual = 0xff000000
                        | (u32::from(rgba[0]) << 16)
                        | (u32::from(rgba[1]) << 8)
                        | u32::from(rgba[2]);
                    assert_eq!(actual, expected[y * 800 + x], "dense pixel=({x},{y})");
                }
            }
        }
    }

    #[test]
    fn maximum_reference_validation_matches_scalar_at_boundaries_and_tails() {
        for scale in 1..=4 {
            for detail in [0, 1, 15, 16, 63, 64, 65] {
                for count in 0..80usize {
                    let tile = scale * scale;
                    let mut image = CompactPresentation {
                        width: count as u32,
                        height: 1,
                        scale,
                        cells: (0..count)
                            .map(|i| {
                                if detail >= tile && i % 3 == 0 {
                                    0x80000000 | (detail - tile)
                                } else {
                                    0x7fffffff
                                }
                            })
                            .collect(),
                        detail: vec![0; detail as usize],
                    };
                    let check = |image: &CompactPresentation| {
                        let expected = image.cells.iter().all(|&cell| {
                            cell >> 31 == 0
                                || u64::from(cell & 0x7fffffff) + u64::from(tile)
                                    <= image.detail.len() as u64
                        });
                        assert_eq!(
                            valid_cells(image, &image.cells),
                            expected,
                            "scale={scale} detail={detail} count={count}"
                        );
                    };
                    check(&image);
                    for index in 0..count {
                        let old = image.cells[index];
                        for invalid in [0xffffffff, 0x80000000 | detail.saturating_sub(tile) + 1] {
                            image.cells[index] = invalid;
                            check(&image);
                        }
                        image.cells[index] = old;
                    }
                }
            }
        }
    }

    #[test]
    fn malformed_retained_packets_require_cpu_fallback() {
        let mut image = CompactPresentation {
            width: 1,
            height: 1,
            scale: 4,
            cells: vec![0x80000000],
            detail: vec![0; 15],
        };
        let limits = wgpu::Limits::default();
        assert!(!CompactRenderer::supports(
            &image,
            (0, 0, 1, 1),
            (0, 0, 2, 2),
            &limits
        ));
        image.detail.push(0xffffff);
        assert!(CompactRenderer::supports(
            &image,
            (0, 0, 1, 1),
            (0, 0, 2, 2),
            &limits
        ));
        assert!(!CompactRenderer::supports(
            &image,
            (1, 0, 1, 1),
            (0, 0, 2, 2),
            &limits
        ));
        image.scale = 0;
        assert!(!CompactRenderer::supports(
            &image,
            (0, 0, 1, 1),
            (0, 0, 2, 2),
            &limits
        ));
    }
}
