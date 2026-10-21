//! WebGPU presentation. Guest execution retains ownership of framebuffer memory.
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

fn error(value: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&value.to_string()).into()
}

#[wasm_bindgen]
pub struct WebGpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    texture: Option<wgpu::Texture>,
    binding: Option<wgpu::BindGroup>,
    failure: Arc<Mutex<Option<String>>>,
}

#[wasm_bindgen]
impl WebGpuPresenter {
    pub async fn create(canvas: web_sys::OffscreenCanvas) -> Result<WebGpuPresenter, JsValue> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
            .map_err(error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(error)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(error)?;
        let failure = Arc::new(Mutex::new(None));
        let lost = failure.clone();
        device.set_device_lost_callback(move |reason, message| {
            *lost.lock().unwrap() = Some(format!("WebGPU device lost: {reason:?}: {message}"));
        });
        let invalid = failure.clone();
        device.on_uncaptured_error(Arc::new(move |cause| {
            *invalid.lock().unwrap() = Some(format!("WebGPU validation: {cause}"));
        }));
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or_else(|| error("No WebGPU surface configuration"))?;
        // Guest bytes already contain display colors: do not apply sRGB encoding twice.
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|format| !format.is_srgb())
            .ok_or_else(|| error("No linear WebGPU surface format"))?;
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Systemless browser presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_presenter.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Systemless browser presentation"),
            layout: None,
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
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            texture: None,
            binding: None,
            failure,
        })
    }

    pub fn check_health(&self) -> Result<(), JsValue> {
        match self.failure.lock().unwrap().as_ref() {
            Some(message) => Err(error(message)),
            None => Ok(()),
        }
    }

    pub fn paint(&mut self, width: u32, height: u32, pixels: &[u8]) -> Result<(), JsValue> {
        self.check_health()?;
        let limit = self.device.limits().max_texture_dimension_2d;
        if width == 0
            || height == 0
            || width > limit
            || height > limit
            || u64::from(width) * u64::from(height) > 16 * 1024 * 1024
            || pixels.len() as u64 != u64::from(width) * u64::from(height) * 4
        {
            return Err(error("Invalid WebGPU RGBA frame"));
        }
        if self.texture.is_none() || self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("guest framebuffer"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.binding = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("guest framebuffer"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                }],
            }));
            self.texture = Some(texture);
        }
        self.queue.write_texture(
            self.texture.as_ref().unwrap().as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => return Err(error(format!("WebGPU surface unavailable: {other:?}"))),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, self.binding.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }

    // Own the completion state independently: disposal may occur while awaiting it.
    pub fn completed(&self) -> js_sys::Promise {
        let (sender, receiver) = futures_channel::oneshot::channel();
        self.queue.on_submitted_work_done(move || {
            let _ = sender.send(());
        });
        let failure = self.failure.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            receiver.await.map_err(error)?;
            if let Some(message) = failure.lock().unwrap().as_ref() {
                return Err(error(message));
            }
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn dispose(&self) {
        self.device.destroy();
    }
}
