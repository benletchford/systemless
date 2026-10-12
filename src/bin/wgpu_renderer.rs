//! GPU presentation; the guest coordinate transform is shared by painting and hit testing.
use std::sync::Arc;
use winit::window::Window;
#[path = "wgpu_gpu_probe.rs"]
mod gpu_probe;

pub(super) struct Quad {
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

pub(super) struct Renderer {
    instance: wgpu::Instance,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    quad_buffer: wgpu::Buffer,
    texture: Option<(wgpu::Texture, wgpu::BindGroup)>,
    window: Option<Arc<Window>>,
    offscreen: Option<wgpu::Texture>,
    suspended: bool,
    gpu: Option<gpu_probe::GpuProbe>,
    sequence: u64,
}
impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        Self::create(Some(window), size.width.max(1), size.height.max(1)).await
    }
    pub async fn offscreen(width: u32, height: u32) -> Self {
        Self::create(None, width, height).await
    }
    async fn create(window: Option<Arc<Window>>, width: u32, height: u32) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = window
            .as_ref()
            .map(|window| instance.create_surface(window.clone()).unwrap());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surface.as_ref(),
                ..Default::default()
            })
            .await
            .unwrap();
        eprintln!("[wgpu-adapter] {:?}", adapter.get_info());
        let timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: if timestamps {
                    wgpu::Features::TIMESTAMP_QUERY
                } else {
                    wgpu::Features::empty()
                },
                ..Default::default()
            })
            .await
            .unwrap();
        let gpu = timestamps.then(|| gpu_probe::GpuProbe::new(&device, &queue));
        let mut config = if let Some(surface) = &surface {
            surface.get_default_config(&adapter, width, height).unwrap()
        } else {
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            }
        };
        config.present_mode = wgpu::PresentMode::Fifo;
        if let Some(surface) = &surface {
            config.format = surface
                .get_capabilities(&adapter)
                .formats
                .into_iter()
                .find(|f| !f.is_srgb())
                .unwrap_or(config.format);
            surface.configure(&device, &config);
        }
        let offscreen = surface
            .is_none()
            .then(|| Self::capture_texture(&device, &config));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("guest framebuffer"), source: wgpu::ShaderSource::Wgsl(r#"
@group(0) @binding(0) var image: texture_2d<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) index: u32) -> Vertex {
    let xy = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    var out: Vertex;
    out.position = vec4(xy[index], 0., 1.);
    out.uv = vec2((xy[index].x + 1.) * 0.5, (1. - xy[index].y) * 0.5);
    return out;
}
struct QuadVertex { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn quad_vs(@builtin(vertex_index) index: u32, @location(0) rect: vec4<f32>, @location(1) color: vec4<f32>) -> QuadVertex {
    let corners = array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));
    let p = mix(rect.xy,rect.zw,corners[index]) / vec2<f32>(textureDimensions(image));
    var out: QuadVertex;
    out.position=vec4(p.x*2.-1.,1.-p.y*2.,0.,1.); out.color=color; return out;
}
@fragment fn quad_fs(in: QuadVertex) -> @location(0) vec4<f32> { return in.color; }
@fragment fn fs(in: Vertex) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(image));
    let pixel = clamp(vec2<i32>(in.uv * vec2<f32>(size)), vec2(0), size - vec2(1));
    return textureLoad(image, pixel, 0);
}
"#.into()) });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("guest controls"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("quad_vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("quad_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let quad_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("control instances"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("guest presentation"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            instance,
            surface,
            device,
            queue,
            config,
            pipeline,
            quad_pipeline,
            quad_buffer,
            texture: None,
            window,
            offscreen,
            suspended: false,
            gpu,
            sequence: 0,
        }
    }
    fn capture_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen comparison"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
    pub fn save_capture(&self, path: &std::path::Path) {
        let texture = self
            .offscreen
            .as_ref()
            .expect("capture requires offscreen mode");
        let stride = (self.config.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture readback"),
            size: u64::from(stride) * u64::from(self.config.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(self.config.height),
                },
            },
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let mut rgba = Vec::new();
        for row in data.chunks_exact(stride as usize) {
            for pixel in row[..self.config.width as usize * 4].chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }
        image::save_buffer(
            path,
            &rgba,
            self.config.width,
            self.config.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    pub fn raster(&self, width: u32, height: u32) -> u32 {
        let (_, _, w, _) = self.viewport(width, height);
        (w / width as f32).ceil().clamp(1., 8.) as u32
    }
    pub fn gpu_samples(&self) -> Option<&[(u64, f64)]> {
        self.gpu.as_ref().map(|g| g.samples.as_slice())
    }
    pub fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.suspended = width == 0 || height == 0;
        if self.suspended {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        } else {
            self.offscreen = Some(Self::capture_texture(&self.device, &self.config));
        }
    }
    pub fn viewport(&self, width: u32, height: u32) -> (f32, f32, f32, f32) {
        let scale = (self.config.width as f32 / width as f32)
            .min(self.config.height as f32 / height as f32);
        let w = width as f32 * scale;
        let h = height as f32 * scale;
        (
            (self.config.width as f32 - w) * 0.5,
            (self.config.height as f32 - h) * 0.5,
            w,
            h,
        )
    }
    pub fn guest_point(&self, x: f64, y: f64, width: u32, height: u32) -> (i16, i16) {
        let (left, top, w, h) = self.viewport(width, height);
        (
            ((y - top as f64) * height as f64 / h as f64)
                .floor()
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16,
            ((x - left as f64) * width as f64 / w as f64)
                .floor()
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16,
        )
    }
    pub fn draw(&mut self, width: u32, height: u32, bgra: &[u8], quads: &[Quad]) -> bool {
        if self.suspended || width == 0 || height == 0 {
            return false;
        }
        assert_eq!(bgra.len(), width as usize * height as usize * 4);
        if self
            .texture
            .as_ref()
            .is_none_or(|(t, _)| t.width() != width || t.height() != height)
        {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("guest BGRA"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.texture = Some((texture, group));
        }
        let (texture, group) = self.texture.as_ref().unwrap();
        self.queue.write_texture(
            texture.as_image_copy(),
            bgra,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            texture.size(),
        );
        let quad_bytes: Vec<u8> = quads
            .iter()
            .flat_map(|quad| {
                quad.rect
                    .into_iter()
                    .chain(quad.color)
                    .flat_map(f32::to_ne_bytes)
            })
            .collect();
        if quad_bytes.len() as u64 > self.quad_buffer.size() {
            self.quad_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("control instances"),
                size: (quad_bytes.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.quad_buffer, 0, &quad_bytes);
        let output = if let Some(surface) = &self.surface {
            Some(match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                wgpu::CurrentSurfaceTexture::Outdated => {
                    surface.configure(&self.device, &self.config);
                    self.window.as_ref().unwrap().request_redraw();
                    return false;
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return false;
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    let surface = self
                        .instance
                        .create_surface(self.window.as_ref().unwrap().clone())
                        .unwrap();
                    surface.configure(&self.device, &self.config);
                    self.surface = Some(surface);
                    self.window.as_ref().unwrap().request_redraw();
                    return false;
                }
                wgpu::CurrentSurfaceTexture::Validation => panic!("surface validation failed"),
            })
        } else {
            None
        };
        self.sequence += 1;
        let _ = self.device.poll(wgpu::PollType::Poll);
        if let Some(gpu) = &mut self.gpu {
            gpu.collect();
        }
        let sample =
            self.sequence % 30 == 0 && self.gpu.as_ref().is_some_and(|gpu| gpu.available());
        let target = output
            .as_ref()
            .map(|f| &f.texture)
            .or(self.offscreen.as_ref())
            .unwrap();
        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("guest scene"),
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
                timestamp_writes: if sample {
                    self.gpu
                        .as_ref()
                        .map(|gpu| wgpu::RenderPassTimestampWrites {
                            query_set: &gpu.queries,
                            beginning_of_pass_write_index: Some(0),
                            end_of_pass_write_index: Some(1),
                        })
                } else {
                    None
                },
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let (x, y, w, h) = self.viewport(width, height);
            pass.set_viewport(x, y, w, h, 0., 1.);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
            if !quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
                pass.draw(0..6, 0..quads.len() as u32);
            }
        }
        if sample {
            self.gpu.as_ref().unwrap().resolve(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        if sample {
            self.gpu.as_mut().unwrap().submitted(self.sequence);
        }
        if let Some(output) = output {
            self.window.as_ref().unwrap().pre_present_notify();
            output.present();
        }
        true
    }
}
