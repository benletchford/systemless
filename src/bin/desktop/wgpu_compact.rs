//! Resolve sparse retained glyph/QuickDraw coverage directly in the fragment shader.
use std::sync::Arc;
use systemless::memory::CompactPresentation;

pub(super) struct CompactRenderer {
    pipeline: wgpu::RenderPipeline,
    geometry: wgpu::Buffer,
    resources: Option<Resources>,
    source: Option<Arc<CompactPresentation>>,
    bytes: Vec<u8>,
}

struct Resources {
    cells: wgpu::Buffer,
    detail: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl CompactRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            geometry: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("retained geometry"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            resources: None,
            source: None,
            bytes: Vec::new(),
            pipeline: super::wgpu_present::pipeline_with_shader(
                device,
                format,
                include_str!("wgpu_compact.wgsl"),
            ),
        }
    }

    /// Integer coverage is exact within the same accumulator limits as Metal/D3D.
    /// Invalid or out-of-range packets retain the existing CPU resolve fallback.
    pub fn supports(
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
        let tile = u64::from(image.scale * image.scale);
        if image.cells.iter().any(|cell| {
            cell >> 31 != 0 && u64::from(cell & 0x7fffffff) + tile > image.detail.len() as u64
        }) {
            return false;
        }
        [image.cells.len(), image.detail.len().max(1)]
            .into_iter()
            .all(|n| (n as u64) * 4 <= u64::from(limits.max_storage_buffer_binding_size))
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
                layout: &self.pipeline.get_bind_group_layout(0),
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
        let resources = self.resources.as_ref().unwrap();
        if self
            .source
            .as_ref()
            .is_none_or(|source| !Arc::ptr_eq(source, image))
        {
            self.bytes.clear();
            self.bytes
                .extend(image.cells.iter().flat_map(|word| word.to_ne_bytes()));
            queue.write_buffer(&resources.cells, 0, &self.bytes);
            self.bytes.clear();
            self.bytes
                .extend(image.detail.iter().flat_map(|word| word.to_ne_bytes()));
            if !self.bytes.is_empty() {
                queue.write_buffer(&resources.detail, 0, &self.bytes);
            }
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
        self.bytes.clear();
        self.bytes
            .extend(geometry.into_iter().flat_map(u32::to_ne_bytes));
        queue.write_buffer(&self.geometry, 0, &self.bytes);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained coverage"),
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &resources.group, &[]);
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
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = CompactRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        for scale in 1..=4 {
            let mut image = CompactPresentation {
                width: 7,
                height: 5,
                scale,
                cells: Vec::new(),
                detail: Vec::new(),
            };
            for i in 0..35u32 {
                if i % 3 == 0 {
                    image.cells.push(0x80000000 | image.detail.len() as u32);
                    image.detail.extend(
                        (0..scale * scale).map(|sample| (i * 819121 + sample * 123799) & 0xffffff),
                    );
                } else {
                    image.cells.push((i * 987131) & 0xffffff);
                }
            }
            let image = Arc::new(image);
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
                        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
                    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                    rx.recv().unwrap().unwrap();
                    let bytes = readback.slice(..).get_mapped_range().unwrap();
                    let mut cropped = image.as_ref().clone();
                    cropped.width = crop.2;
                    cropped.height = crop.3;
                    cropped.cells.clear();
                    for y in crop.1..crop.1 + crop.3 {
                        let start = (y * image.width + crop.0) as usize;
                        cropped
                            .cells
                            .extend_from_slice(&image.cells[start..start + crop.2 as usize]);
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
                            let expected = if x >= 2 && x < 2 + size.0 && y >= 3 && y < 3 + size.1 {
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
