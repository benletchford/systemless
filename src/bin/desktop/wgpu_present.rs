//! Opt-in portable desktop presentation. Guest memory remains authoritative.
use std::rc::Rc;
use winit::window::Window;

pub struct WgpuPresenter {
    // Drop the surface before its owning window; raw handles never escape this owner.
    surface: wgpu::Surface<'static>,
    _window: Rc<Window>,
    display: winit::event_loop::OwnedDisplayHandle,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    geometry: wgpu::Buffer,
    image: Option<(wgpu::Texture, wgpu::BindGroup)>,
    bytes: Vec<u8>,
    compact: super::wgpu_compact::CompactRenderer,
}

fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    pipeline_with_shader(device, format, include_str!("wgpu_present.wgsl"))
}

pub(super) fn pipeline_with_shader(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    source: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("desktop presentation"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("desktop presentation"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl WgpuPresenter {
    pub fn new(
        window: Rc<Window>,
        display: winit::event_loop::OwnedDisplayHandle,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display.clone()),
        ));
        // SAFETY: the Rc below owns the window until after surface destruction.
        // Creation, use, and destruction all occur on the window's event-loop thread.
        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_window(window.as_ref())
                .map_err(|e| e.to_string())?;
            instance
                .create_surface_unsafe(target)
                .map_err(|e| e.to_string())?
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| e.to_string())?;
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or("surface has no supported configuration")?;
        // Guest RGB values already include Macintosh gamma; avoid a second sRGB encoding.
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
            .ok_or("surface has no unorm format")?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.width = 0;
        config.height = 0;
        let pipeline = pipeline(&device, config.format);
        let geometry = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("desktop geometry"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        eprintln!(
            "[GPU] wgpu {:?}: {}",
            adapter.get_info().backend,
            adapter.get_info().name
        );
        let compact = super::wgpu_compact::CompactRenderer::new(&device, config.format);
        Ok(Self {
            compact,
            surface,
            _window: window,
            display,
            device,
            queue,
            config,
            pipeline,
            geometry,
            image: None,
            bytes: Vec::new(),
        })
    }

    fn acquire(&mut self, output: (u32, u32)) -> Result<Option<wgpu::SurfaceTexture>, String> {
        if output.0 == 0 || output.1 == 0 {
            return Ok(None);
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if output.0 > limit || output.1 > limit {
            return Err(format!(
                "GPU drawable exceeds the adapter texture limit of {limit}"
            ));
        }
        if self.config.width != output.0 || self.config.height != output.1 {
            self.config.width = output.0;
            self.config.height = output.1;
            self.surface.configure(&self.device, &self.config);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                *self = Self::new(self._window.clone(), self.display.clone())?;
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("wgpu surface validation failed".into())
            }
        };
        Ok(Some(frame))
    }

    pub fn present_compact(
        &mut self,
        image: &std::sync::Arc<systemless::memory::CompactPresentation>,
        crop: (u32, u32, u32, u32),
        output: (u32, u32),
    ) -> Result<Option<bool>, String> {
        let viewport = super::aspect_fit_dimensions(crop.2, crop.3, output.0, output.1);
        if !super::wgpu_compact::CompactRenderer::supports(
            image,
            crop,
            viewport,
            &self.device.limits(),
        ) {
            return Ok(None);
        }
        let Some(frame) = self.acquire(output)? else {
            return Ok(Some(false));
        };
        self.compact.encode(
            &self.device,
            &self.queue,
            image,
            crop,
            viewport,
            &frame.texture.create_view(&Default::default()),
        );
        self.queue.present(frame);
        Ok(Some(true))
    }

    pub fn present(
        &mut self,
        pixels: &[u32],
        size: (u32, u32),
        crop: (u32, u32, u32, u32),
        output: (u32, u32),
    ) -> Result<bool, String> {
        if output.0 == 0 || output.1 == 0 {
            return Ok(false);
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if size.0 > limit || size.1 > limit || output.0 > limit || output.1 > limit {
            return Err(format!(
                "GPU frame exceeds the adapter texture limit of {limit}"
            ));
        }
        if size.0 == 0
            || size.1 == 0
            || pixels.len() != size.0 as usize * size.1 as usize
            || crop.2 == 0
            || crop.3 == 0
            || crop.0.checked_add(crop.2).is_none_or(|x| x > size.0)
            || crop.1.checked_add(crop.3).is_none_or(|y| y > size.1)
        {
            return Err("invalid GPU frame geometry".into());
        }
        let Some(frame) = self.acquire(output)? else {
            return Ok(false);
        };
        if self
            .image
            .as_ref()
            .is_none_or(|(image, _)| image.width() != size.0 || image.height() != size.1)
        {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("desktop RGB"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("desktop image"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.geometry.as_entire_binding(),
                    },
                ],
            });
            self.image = Some((texture, group));
        }
        self.bytes.clear();
        self.bytes.extend(
            pixels
                .iter()
                .flat_map(|p| [(*p >> 16) as u8, (*p >> 8) as u8, *p as u8, 255]),
        );
        let (image, group) = self.image.as_ref().unwrap();
        self.queue.write_texture(
            image.as_image_copy(),
            &self.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.0 * 4),
                rows_per_image: Some(size.1),
            },
            image.size(),
        );
        let rect = super::aspect_fit_dimensions(crop.2, crop.3, output.0, output.1);
        let words = [
            crop.0, crop.1, crop.2, crop.3, rect.0, rect.1, rect.2, rect.3,
        ];
        let uniforms: Vec<u8> = words.into_iter().flat_map(u32::to_ne_bytes).collect();
        self.queue.write_buffer(&self.geometry, 0, &uniforms);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("desktop"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_viewport(
                rect.0 as f32,
                rect.1 as f32,
                rect.2 as f32,
                rect.3 as f32,
                0.,
                1.,
            );
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::util::DeviceExt;

    #[test]
    fn gpu_readback_preserves_rgb_crop_orientation_and_letterboxing() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                eprintln!("SKIP GPU readback: {error}");
                return;
            }
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let pipeline = pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let source = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 3,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        // Crop the first column; all four remaining colors must retain their position.
        queue.write_texture(
            source.as_image_copy(),
            &[
                99, 99, 99, 255, 18, 52, 86, 255, 255, 0, 0, 255, 99, 99, 99, 255, 0, 255, 0, 255,
                0, 0, 255, 255,
            ],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(12),
                rows_per_image: Some(2),
            },
            source.size(),
        );
        let words = [1u32, 0, 2, 2, 1, 0, 2, 2];
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_ne_bytes).collect();
        let geometry = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &source.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: geometry.as_entire_binding(),
                },
            ],
        });
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 512,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let view = target.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_viewport(1., 0., 2., 2., 0., 1.);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(2),
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
        let data = readback.slice(..).get_mapped_range().unwrap();
        assert_eq!(
            &data[..16],
            &[0, 0, 0, 255, 18, 52, 86, 255, 255, 0, 0, 255, 0, 0, 0, 255]
        );
        assert_eq!(
            &data[256..272],
            &[0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]
        );
    }
}
