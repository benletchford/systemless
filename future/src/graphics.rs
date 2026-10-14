use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Transfer {
    Copy = 0,
    Or = 1,
    Xor = 2,
    Bic = 3,
}
#[derive(Debug, Clone, Copy)]
pub enum Draw {
    Present,
    Fill {
        rect: Rect,
        clip: Rect,
        pattern: [u8; 8],
        foreground: u8,
        background: u8,
        mode: Transfer,
    },
    Copy {
        rect: Rect,
        clip: Rect,
        source_x: i32,
        source_y: i32,
        mode: Transfer,
    },
}
pub struct Graphics {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub base: GuestPtr,
    pub palette: [[u8; 4]; 256],
    pipeline: wgpu::ComputePipeline,
    error: Arc<Mutex<Option<String>>>,
}
fn words_bytes(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}
impl Graphics {
    pub async fn new(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        width: u32,
        height: u32,
        base: GuestPtr,
    ) -> Result<Self> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(Error::Graphics("invalid dimensions".into()));
        }
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .map_err(|e| Error::Graphics(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| Error::Graphics(e.to_string()))?;
        let error = Arc::new(Mutex::new(None));
        let errors = error.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            *errors.lock().unwrap() = Some(e.to_string());
        }));
        let errors = error.clone();
        device.set_device_lost_callback(move |reason, message| {
            *errors.lock().unwrap() = Some(format!("device lost: {reason:?}: {message}"));
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("indexed QuickDraw operations"),
            source: wgpu::ShaderSource::Wgsl(include_str!("graphics.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("exact indexed graphics"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("guest display"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let palette = std::array::from_fn(|i| [i as u8, i as u8, i as u8, 255]);
        Ok(Self {
            device,
            queue,
            adapter,
            texture,
            width,
            height,
            base,
            palette,
            pipeline,
            error,
        })
    }
    pub fn check_error(&self) -> Result<()> {
        if let Some(e) = self.error.lock().unwrap().as_ref() {
            return Err(Error::Graphics(e.clone()));
        }
        Ok(())
    }
    pub fn execute(&self, memory: &mut Memory, draw: Draw) -> Result<()> {
        self.check_error()?;
        let mut p = vec![0i32; 24];
        p[0] = self.width as i32;
        p[1] = self.height as i32;
        let (rect, clip) = match draw {
            Draw::Present => (Rect::new(0, 0, 0, 0), Rect::new(0, 0, 0, 0)),
            Draw::Fill {
                rect,
                clip,
                pattern,
                foreground,
                background,
                mode,
            } => {
                p[2] = 1;
                p[11] = foreground.into();
                p[12] = background.into();
                p[13] = mode as i32;
                for (i, b) in pattern.iter().enumerate() {
                    p[16 + i] = i32::from(*b);
                }
                (rect, clip)
            }
            Draw::Copy {
                rect,
                clip,
                source_x,
                source_y,
                mode,
            } => {
                p[2] = 2;
                p[13] = mode as i32;
                p[14] = source_x;
                p[15] = source_y;
                (rect, clip)
            }
        };
        p[3..11].copy_from_slice(&[
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            clip.left,
            clip.top,
            clip.right,
            clip.bottom,
        ]);
        // Bounds avoid signed overflow in shader copy-coordinate arithmetic.
        if p[3..11]
            .iter()
            .chain(p[14..16].iter())
            .any(|v| !(-32768..=32767).contains(v))
        {
            return Err(Error::Graphics(
                "coordinates outside QuickDraw i16 range".into(),
            ));
        }
        let indices = words_bytes(
            memory
                .read(self.base, (self.width * self.height) as usize)?
                .iter()
                .map(|b| u32::from(*b)),
        );
        let buffer = |label, bytes: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage,
                })
        };
        let pixels = buffer(
            "indices",
            &indices,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let source = buffer(
            "immutable copy source",
            &indices,
            wgpu::BufferUsages::STORAGE,
        );
        let colors = buffer(
            "palette",
            &self.palette.iter().flatten().copied().collect::<Vec<_>>(),
            wgpu::BufferUsages::STORAGE,
        );
        let params = buffer(
            "draw parameters",
            &words_bytes(p.into_iter().map(|v| v as u32)),
            wgpu::BufferUsages::STORAGE,
        );
        let view = self.texture.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: pixels.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: colors.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("synchronous guest indices"),
            size: indices.len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&pixels, 0, &readback, 0, indices.len() as u64);
        self.queue.submit([encoder.finish()]);
        let bytes = self.map_read(&readback)?;
        self.check_error()?;
        let guest: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v) as u8)
            .collect();
        memory.write(self.base, &guest)
    }
    fn map_read(&self, buffer: &wgpu::Buffer) -> Result<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Graphics(e.to_string()))?;
        rx.recv()
            .map_err(|e| Error::Graphics(e.to_string()))?
            .map_err(|e| Error::Graphics(e.to_string()))?;
        let bytes = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| Error::Graphics(e.to_string()))?
            .to_vec();
        buffer.unmap();
        Ok(bytes)
    }
    pub fn read_rgba(&self) -> Result<Vec<u8>> {
        self.check_error()?;
        let stride = (self.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(stride * self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let padded = self.map_read(&buffer)?;
        Ok(padded
            .chunks_exact(stride as usize)
            .flat_map(|row| row[..self.width as usize * 4].iter().copied())
            .collect())
    }
}
