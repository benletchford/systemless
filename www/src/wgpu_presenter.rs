//! WebGPU presentation. Guest execution retains ownership of framebuffer memory.
#[cfg(target_arch = "wasm32")]
use std::num::NonZeroU64;
#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, Mutex};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(any(target_arch = "wasm32", test))]
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
#[cfg(any(target_arch = "wasm32", test))]
const MAX_CURSOR_PIXELS: u64 = 4096;
#[cfg(any(target_arch = "wasm32", test))]
const MODE_RGBA: u32 = 0;
#[cfg(any(target_arch = "wasm32", test))]
const MODE_INDEXED8: u32 = 1;
#[cfg(any(target_arch = "wasm32", test))]
const MODE_COMPACT: u32 = 2;

#[cfg(any(target_arch = "wasm32", test))]
fn validate_surface(width: u32, height: u32, max_texture_dim: u32) -> Result<(), &'static str> {
    if width == 0
        || height == 0
        || width > max_texture_dim
        || height > max_texture_dim
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("Invalid WebGPU frame dimensions");
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
#[allow(clippy::too_many_arguments)]
fn validate_cursor(
    width: u32,
    height: u32,
    max_texture_dim: u32,
    x: u32,
    y: u32,
    cursor_w: u32,
    cursor_h: u32,
    cursor_pixels: &[u8],
) -> Result<bool, &'static str> {
    if cursor_w == 0 && cursor_h == 0 && x == 0 && y == 0 && cursor_pixels.is_empty() {
        return Ok(false);
    }
    if cursor_w == 0
        || cursor_h == 0
        || cursor_w > max_texture_dim
        || cursor_h > max_texture_dim
        || u64::from(cursor_w) * u64::from(cursor_h) > MAX_CURSOR_PIXELS
        || x.checked_add(cursor_w).is_none_or(|end| end > width)
        || y.checked_add(cursor_h).is_none_or(|end| end > height)
        || cursor_pixels.len() as u64 != u64::from(cursor_w) * u64::from(cursor_h) * 4
    {
        return Err("Invalid owned cursor patch");
    }
    Ok(true)
}

#[cfg(any(target_arch = "wasm32", test))]
fn validate_rgba(
    width: u32,
    height: u32,
    max_texture_dim: u32,
    pixels: &[u8],
) -> Result<(), &'static str> {
    validate_surface(width, height, max_texture_dim)?;
    if pixels.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return Err("Invalid WebGPU RGBA frame");
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
fn validate_indexed8(
    width: u32,
    height: u32,
    stride: u32,
    max_texture_dim: u32,
    pixels: &[u8],
    palette: &[u8],
) -> Result<(), &'static str> {
    validate_surface(width, height, max_texture_dim)?;
    if stride < width
        || stride > max_texture_dim
        || u64::from(stride) * u64::from(height) > MAX_PIXELS
        || pixels.len() as u64 != u64::from(stride) * u64::from(height)
        || palette.len() != 256 * 4
    {
        return Err("Invalid indexed image layout or palette");
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
#[allow(clippy::too_many_arguments)]
fn validate_compact(
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    scale: u32,
    max_texture_dim: u32,
    max_storage_bytes: u64,
    cells_bytes: &[u8],
    detail_bytes: &[u8],
) -> Result<u32, &'static str> {
    validate_surface(width, height, max_texture_dim)?;
    if source_width == 0
        || source_height == 0
        || !(1..=4).contains(&scale)
        || u64::from(source_width) * u64::from(source_height) > MAX_PIXELS
        || cells_bytes.len() as u64 != u64::from(source_width) * u64::from(source_height) * 4
        || !detail_bytes.len().is_multiple_of(4)
    {
        return Err("Invalid complete CompactPresentation");
    }
    let detail_words = (detail_bytes.len() / 4) as u64;
    if detail_words > MAX_PIXELS {
        return Err("Invalid complete CompactPresentation");
    }
    if width % source_width != 0 {
        return Err("Unsupported compact output scale");
    }
    let output_scale = width / source_width;
    if !(1..=4).contains(&output_scale)
        || u64::from(height) != u64::from(source_height) * u64::from(output_scale)
    {
        return Err("Unsupported compact output scale");
    }
    if (cells_bytes.len() as u64) > max_storage_bytes
        || (detail_bytes.len().max(4) as u64) > max_storage_bytes
    {
        return Err("Compact presentation exceeds storage buffer capacity");
    }
    let tile = u64::from(scale) * u64::from(scale);
    for chunk in cells_bytes.chunks_exact(4) {
        let cell = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if (cell >> 31) != 0 && u64::from(cell & 0x7fff_ffff) + tile > detail_words {
            return Err("Invalid compact detail reference");
        }
    }
    Ok(output_scale)
}

#[cfg(any(target_arch = "wasm32", test))]
fn encode_params(words: [u32; 8]) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 4..(index + 1) * 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(any(target_arch = "wasm32", test))]
fn grown_buffer_capacity(required: u64, max_storage_bytes: u64) -> u64 {
    let floor = required.max(256).min(max_storage_bytes & !3);
    floor
        .checked_next_power_of_two()
        .map_or(max_storage_bytes & !3, |pow| pow.min(max_storage_bytes & !3))
        .max((required + 3) & !3)
}

#[cfg(target_arch = "wasm32")]
fn error(value: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&value.to_string()).into()
}

#[cfg(target_arch = "wasm32")]
fn create_texture_2d(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

#[cfg(target_arch = "wasm32")]
fn create_storage_buffer(device: &wgpu::Device, label: &'static str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WebGpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface_configured: bool,
    bind_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    last_params: Option<[u32; 8]>,
    rgba_texture: wgpu::Texture,
    rgba_size: (u32, u32),
    indexed_texture: wgpu::Texture,
    indexed_size: (u32, u32),
    palette_texture: wgpu::Texture,
    last_palette: Option<Box<[u8; 1024]>>,
    cursor_texture: wgpu::Texture,
    cursor_size: (u32, u32),
    cells_buffer: wgpu::Buffer,
    cells_capacity: u64,
    detail_buffer: wgpu::Buffer,
    detail_capacity: u64,
    binding: wgpu::BindGroup,
    failure: Arc<Mutex<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
impl WebGpuPresenter {
    fn rebuild_bind_group(&mut self) {
        let rgba_view = self.rgba_texture.create_view(&Default::default());
        let indexed_view = self.indexed_texture.create_view(&Default::default());
        let palette_view = self.palette_texture.create_view(&Default::default());
        let cursor_view = self.cursor_texture.create_view(&Default::default());
        self.binding = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("guest presentation"),
            layout: &self.bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&rgba_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&indexed_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&palette_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&cursor_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.cells_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self.detail_buffer.as_entire_binding(),
                },
            ],
        });
    }

    fn ensure_surface(&mut self, width: u32, height: u32) {
        if !self.surface_configured || self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.surface_configured = true;
        }
    }

    fn upload_cursor(
        &mut self,
        has_cursor: bool,
        cursor_x: u32,
        cursor_y: u32,
        cursor_w: u32,
        cursor_h: u32,
        cursor_pixels: &[u8],
    ) -> ([u32; 4], bool) {
        if !has_cursor {
            return ([0, 0, 0, 0], false);
        }
        let mut rebind = false;
        if cursor_w > self.cursor_size.0 || cursor_h > self.cursor_size.1 {
            let next_w = self.cursor_size.0.max(cursor_w);
            let next_h = self.cursor_size.1.max(cursor_h);
            self.cursor_texture = create_texture_2d(
                &self.device,
                "cursor patch",
                next_w,
                next_h,
                wgpu::TextureFormat::Rgba8Unorm,
            );
            self.cursor_size = (next_w, next_h);
            rebind = true;
        }
        self.queue.write_texture(
            self.cursor_texture.as_image_copy(),
            cursor_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cursor_w * 4),
                rows_per_image: Some(cursor_h),
            },
            wgpu::Extent3d {
                width: cursor_w,
                height: cursor_h,
                depth_or_array_layers: 1,
            },
        );
        ([cursor_x, cursor_y, cursor_w, cursor_h], rebind)
    }

    fn submit_frame(&mut self, width: u32, height: u32, params: [u32; 8]) -> Result<(), JsValue> {
        self.ensure_surface(width, height);
        if self.last_params != Some(params) {
            self.queue
                .write_buffer(&self.params_buffer, 0, &encode_params(params));
            self.last_params = Some(params);
        }
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
            pass.set_bind_group(0, &self.binding, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
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
        let tex_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let storage_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(4),
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Systemless browser presentation"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(32),
                    },
                    count: None,
                },
                tex_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                tex_entry(2, wgpu::TextureSampleType::Uint),
                tex_entry(3, wgpu::TextureSampleType::Float { filterable: false }),
                tex_entry(4, wgpu::TextureSampleType::Float { filterable: false }),
                storage_entry(5),
                storage_entry(6),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Systemless browser presentation"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Systemless browser presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_presenter.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Systemless browser presentation"),
            layout: Some(&pipeline_layout),
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
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("presentation params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rgba_texture = create_texture_2d(
            &device,
            "guest rgba framebuffer",
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let indexed_texture = create_texture_2d(
            &device,
            "guest indexed framebuffer",
            1,
            1,
            wgpu::TextureFormat::R8Uint,
        );
        let palette_texture = create_texture_2d(
            &device,
            "guest palette",
            256,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let cursor_texture = create_texture_2d(
            &device,
            "cursor patch",
            64,
            64,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let cells_buffer = create_storage_buffer(&device, "compact cells", 256);
        let detail_buffer = create_storage_buffer(&device, "compact detail", 256);
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("guest presentation"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &rgba_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &indexed_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &palette_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &cursor_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: cells_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: detail_buffer.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            surface_configured: false,
            bind_layout,
            pipeline,
            params_buffer,
            last_params: None,
            rgba_texture,
            rgba_size: (1, 1),
            indexed_texture,
            indexed_size: (1, 1),
            palette_texture,
            last_palette: None,
            cursor_texture,
            cursor_size: (64, 64),
            cells_buffer,
            cells_capacity: 256,
            detail_buffer,
            detail_capacity: 256,
            binding,
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
        self.paint_rgba_cursor(width, height, pixels, 0, 0, 0, 0, &[])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_rgba_cursor(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
        cursor_x: u32,
        cursor_y: u32,
        cursor_width: u32,
        cursor_height: u32,
        cursor_pixels: &[u8],
    ) -> Result<(), JsValue> {
        self.check_health()?;
        let limit = self.device.limits().max_texture_dimension_2d;
        validate_rgba(width, height, limit, pixels).map_err(error)?;
        let has_cursor = validate_cursor(
            width,
            height,
            limit,
            cursor_x,
            cursor_y,
            cursor_width,
            cursor_height,
            cursor_pixels,
        )
        .map_err(error)?;
        let mut rebind = false;
        if self.rgba_size != (width, height) {
            self.rgba_texture = create_texture_2d(
                &self.device,
                "guest rgba framebuffer",
                width,
                height,
                wgpu::TextureFormat::Rgba8Unorm,
            );
            self.rgba_size = (width, height);
            rebind = true;
        }
        self.queue.write_texture(
            self.rgba_texture.as_image_copy(),
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
        let (rect, cursor_rebind) = self.upload_cursor(
            has_cursor,
            cursor_x,
            cursor_y,
            cursor_width,
            cursor_height,
            cursor_pixels,
        );
        if rebind || cursor_rebind {
            self.rebuild_bind_group();
        }
        self.submit_frame(
            width,
            height,
            [MODE_RGBA, 0, 0, 0, rect[0], rect[1], rect[2], rect[3]],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_indexed8(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        pixels: &[u8],
        palette: &[u8],
        cursor_x: u32,
        cursor_y: u32,
        cursor_width: u32,
        cursor_height: u32,
        cursor_pixels: &[u8],
    ) -> Result<(), JsValue> {
        self.check_health()?;
        let limit = self.device.limits().max_texture_dimension_2d;
        validate_indexed8(width, height, stride, limit, pixels, palette).map_err(error)?;
        let has_cursor = validate_cursor(
            width,
            height,
            limit,
            cursor_x,
            cursor_y,
            cursor_width,
            cursor_height,
            cursor_pixels,
        )
        .map_err(error)?;
        let mut rebind = false;
        if self.indexed_size != (stride, height) {
            self.indexed_texture = create_texture_2d(
                &self.device,
                "guest indexed framebuffer",
                stride,
                height,
                wgpu::TextureFormat::R8Uint,
            );
            self.indexed_size = (stride, height);
            rebind = true;
        }
        self.queue.write_texture(
            self.indexed_texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width: stride,
                height,
                depth_or_array_layers: 1,
            },
        );
        if self
            .last_palette
            .as_ref()
            .is_none_or(|cached| &cached[..] != palette)
        {
            self.queue.write_texture(
                self.palette_texture.as_image_copy(),
                palette,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256 * 4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 256,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            let mut next = Box::new([0u8; 1024]);
            next.copy_from_slice(palette);
            self.last_palette = Some(next);
        }
        let (rect, cursor_rebind) = self.upload_cursor(
            has_cursor,
            cursor_x,
            cursor_y,
            cursor_width,
            cursor_height,
            cursor_pixels,
        );
        if rebind || cursor_rebind {
            self.rebuild_bind_group();
        }
        self.submit_frame(
            width,
            height,
            [MODE_INDEXED8, 0, 0, 0, rect[0], rect[1], rect[2], rect[3]],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_compact(
        &mut self,
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
        scale: u32,
        cells_bytes: &[u8],
        detail_bytes: &[u8],
    ) -> Result<(), JsValue> {
        self.check_health()?;
        let limits = self.device.limits();
        let max_storage = u64::from(limits.max_storage_buffer_binding_size);
        let output_scale = validate_compact(
            width,
            height,
            source_width,
            source_height,
            scale,
            limits.max_texture_dimension_2d,
            max_storage,
            cells_bytes,
            detail_bytes,
        )
        .map_err(error)?;
        let mut rebind = false;
        let cells_len = cells_bytes.len() as u64;
        if cells_len > self.cells_capacity {
            let cap = grown_buffer_capacity(cells_len, max_storage);
            self.cells_buffer = create_storage_buffer(&self.device, "compact cells", cap);
            self.cells_capacity = cap;
            rebind = true;
        }
        let detail_len = detail_bytes.len() as u64;
        if detail_len > self.detail_capacity {
            let cap = grown_buffer_capacity(detail_len, max_storage);
            self.detail_buffer = create_storage_buffer(&self.device, "compact detail", cap);
            self.detail_capacity = cap;
            rebind = true;
        }
        self.queue
            .write_buffer(&self.cells_buffer, 0, cells_bytes);
        if !detail_bytes.is_empty() {
            self.queue
                .write_buffer(&self.detail_buffer, 0, detail_bytes);
        }
        if rebind {
            self.rebuild_bind_group();
        }
        self.submit_frame(
            width,
            height,
            [
                MODE_COMPACT,
                source_width,
                scale,
                output_scale,
                0,
                0,
                0,
                0,
            ],
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn words_le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn eval_wgsl_compact_pixel(
        source_width: u32,
        scale: u32,
        output_scale: u32,
        cells: &[u32],
        detail: &[u32],
        px: u32,
        py: u32,
    ) -> u32 {
        let unpack = |word: u32| [(word >> 16) & 255, (word >> 8) & 255, word & 255];
        let cx = px / output_scale;
        let cy = py / output_scale;
        let cell = cells[(cy * source_width + cx) as usize];
        let rgb = if (cell & 0x8000_0000) == 0 {
            unpack(cell)
        } else {
            let offset = cell & 0x7fff_ffff;
            let wx = px - cx * output_scale;
            let wy = py - cy * output_scale;
            if scale <= output_scale {
                let sx = (((wx * 2 + 1) * scale) / (output_scale * 2)).min(scale - 1);
                let sy = (((wy * 2 + 1) * scale) / (output_scale * 2)).min(scale - 1);
                unpack(detail[(offset + sy * scale + sx) as usize])
            } else {
                let mut sum = [0u32; 3];
                for y in 0..scale {
                    let low_y = (wy * scale).max(y * output_scale);
                    let high_y = ((wy + 1) * scale).min((y + 1) * output_scale);
                    if high_y > low_y {
                        let wy_w = high_y - low_y;
                        for x in 0..scale {
                            let low_x = (wx * scale).max(x * output_scale);
                            let high_x = ((wx + 1) * scale).min((x + 1) * output_scale);
                            if high_x > low_x {
                                let wx_w = high_x - low_x;
                                let s = unpack(detail[(offset + y * scale + x) as usize]);
                                let weight = wx_w * wy_w;
                                sum[0] += s[0] * weight;
                                sum[1] += s[1] * weight;
                                sum[2] += s[2] * weight;
                            }
                        }
                    }
                }
                let total = scale * scale;
                [
                    (sum[0] + total / 2) / total,
                    (sum[1] + total / 2) / total,
                    (sum[2] + total / 2) / total,
                ]
            }
        };
        0xff00_0000 | (rgb[0] << 16) | (rgb[1] << 8) | rgb[2]
    }

    #[test]
    fn presenter_validators_enforce_rgba_indexed_cursor_and_compact_contracts() {
        assert_eq!(MODE_RGBA, 0);
        assert_eq!(MODE_INDEXED8, 1);
        assert_eq!(MODE_COMPACT, 2);

        // RGBA and surface bounds.
        assert!(validate_rgba(4, 3, 8192, &[0u8; 48]).is_ok());
        assert!(validate_rgba(0, 3, 8192, &[]).is_err());
        assert!(validate_rgba(8193, 1, 8192, &[0u8; 8193 * 4]).is_err());
        assert!(validate_rgba(4, 3, 8192, &[0u8; 47]).is_err());

        // Cursor bounds.
        assert_eq!(validate_cursor(10, 10, 8192, 0, 0, 0, 0, &[]), Ok(false));
        assert_eq!(
            validate_cursor(10, 10, 8192, 8, 7, 2, 3, &[255u8; 24]),
            Ok(true)
        );
        assert!(validate_cursor(10, 10, 8192, 9, 7, 2, 3, &[255u8; 24]).is_err());
        assert!(validate_cursor(10, 10, 8192, 8, 8, 2, 3, &[255u8; 24]).is_err());
        assert!(validate_cursor(100, 100, 8192, 0, 0, 65, 64, &[255u8; 65 * 64 * 4]).is_err());
        assert!(validate_cursor(10, 10, 8192, 0, 0, 2, 2, &[255u8; 12]).is_err());
        assert!(validate_cursor(10, 10, 8192, 1, 0, 0, 0, &[]).is_err());

        // Indexed8 stride and palette bounds.
        assert!(validate_indexed8(5, 3, 8, 8192, &[0u8; 24], &[0u8; 1024]).is_ok());
        assert!(validate_indexed8(5, 3, 4, 8192, &[0u8; 12], &[0u8; 1024]).is_err());
        assert!(validate_indexed8(5, 3, 8, 8192, &[0u8; 23], &[0u8; 1024]).is_err());
        assert!(validate_indexed8(5, 3, 8, 8192, &[0u8; 24], &[0u8; 1020]).is_err());

        // Compact bounds, scale, outputScale, storage limit, and detail references.
        let max_storage = 134_217_728;
        let ok_cells = words_le(&[0x0011_2233, 0x8000_0000]);
        let ok_detail = words_le(&[1, 2, 3, 4]);
        assert_eq!(
            validate_compact(4, 2, 2, 1, 2, 8192, max_storage, &ok_cells, &ok_detail),
            Ok(2)
        );
        // Out-of-range detail reference rejected.
        assert!(validate_compact(
            4,
            2,
            2,
            1,
            2,
            8192,
            max_storage,
            &words_le(&[0x8000_0001, 0]),
            &ok_detail,
        )
        .is_err());
        // Empty detail allowed only when no cell sets bit 31.
        assert_eq!(
            validate_compact(
                2,
                1,
                2,
                1,
                4,
                8192,
                max_storage,
                &words_le(&[0x123456, 0x654321]),
                &[]
            ),
            Ok(1)
        );
        assert!(validate_compact(
            2,
            1,
            2,
            1,
            1,
            8192,
            max_storage,
            &words_le(&[0x8000_0000, 0]),
            &[]
        )
        .is_err());
        // Unsupported outputScale (0, 5, non-integer, height mismatch) and storage cap.
        assert!(validate_compact(10, 5, 2, 1, 2, 8192, max_storage, &ok_cells, &ok_detail).is_err());
        assert!(validate_compact(3, 1, 2, 1, 2, 8192, max_storage, &ok_cells, &ok_detail).is_err());
        assert!(validate_compact(4, 3, 2, 1, 2, 8192, max_storage, &ok_cells, &ok_detail).is_err());
        assert!(validate_compact(4, 2, 2, 1, 2, 8192, 4, &ok_cells, &ok_detail).is_err());

        // Uniform encoding and storage capacity growth.
        let encoded = encode_params([MODE_COMPACT, 640, 2, 2, 0, 0, 0, 0]);
        assert_eq!(u32::from_le_bytes(encoded[0..4].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(encoded[4..8].try_into().unwrap()), 640);
        assert_eq!(grown_buffer_capacity(12, max_storage), 256);
        assert_eq!(grown_buffer_capacity(1_228_800, max_storage), 2_097_152);
    }

    #[test]
    fn wgsl_compact_integer_kernel_matches_native_reference_vectors() {
        #[derive(serde::Deserialize)]
        struct Output {
            scale: u32,
            argb: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Vector {
            width: u32,
            height: u32,
            scale: u32,
            cells: Vec<u32>,
            detail: Vec<u32>,
            outputs: Vec<Output>,
        }

        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../tests/fixtures/compact-native.json")).unwrap();
        assert!(!vectors.is_empty());
        for v in &vectors {
            let cells_bytes = words_le(&v.cells);
            let detail_bytes = words_le(&v.detail);
            for out in &v.outputs {
                let width = v.width * out.scale;
                let height = v.height * out.scale;
                let oscale = validate_compact(
                    width,
                    height,
                    v.width,
                    v.height,
                    v.scale,
                    8192,
                    134_217_728,
                    &cells_bytes,
                    &detail_bytes,
                )
                .unwrap();
                assert_eq!(oscale, out.scale);
                for py in 0..height {
                    for px in 0..width {
                        let got = eval_wgsl_compact_pixel(
                            v.width,
                            v.scale,
                            oscale,
                            &v.cells,
                            &v.detail,
                            px,
                            py,
                        );
                        assert_eq!(got, out.argb[(py * width + px) as usize]);
                    }
                }
            }
        }
    }
}
