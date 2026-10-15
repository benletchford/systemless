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
pub struct PenLine {
    pub origin_x: i32,
    pub origin_y: i32,
    pub minor_delta: i32,
    pub steps: u32,
    pub vertical: bool,
    pub direction: i32,
    pub width: u16,
    pub height: u16,
}
#[derive(Debug, Clone, Copy)]
pub struct BlitMask<'a> {
    pub rect: Rect,
    pub width: u32,
    pub height: u32,
    pub bits: &'a [u8],
}
#[derive(Debug, Clone, Copy)]
pub enum Draw<'a> {
    Polygon {
        rect: Rect,
        clip: Rect,
        vertices: &'a [(i16, i16)],
        origin_x: i32,
        origin_y: i32,
        frame: Option<(i16, i16)>,
        pattern: [u8; 8],
        foreground: u8,
        background: u8,
        mode: Transfer,
    },
    Present,
    Cursor {
        x: i32,
        y: i32,
        data: [u16; 16],
        mask: [u16; 16],
    },
    Line {
        rect: Rect,
        clip: Rect,
        line: PenLine,
        pattern: [u8; 8],
        foreground: u8,
        background: u8,
        mode: Transfer,
    },
    Fill {
        rect: Rect,
        clip: Rect,
        pattern: [u8; 8],
        foreground: u8,
        background: u8,
        mode: Transfer,
    },
    Blit {
        mask: Option<BlitMask<'a>>,
        rect: Rect,
        clip: Rect,
        source: Rect,
        width: u32,
        height: u32,
        foreground: u8,
        background: u8,
        invert: bool,
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
pub(crate) struct IndexedTarget<'a> {
    pub depth: u16,
    pub rgb: &'a [[u16; 3]; 256],
    pub defined: &'a [bool; 256],
    pub direct_source: bool,
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
            let mut first = errors.lock().unwrap();
            if first.is_none() {
                *first = Some(e.to_string());
            }
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
    /// A scratch drawing target on the same device. Guest memory remains the
    /// authority; callers need not retain this texture after synchronized drawing.
    pub fn target(&self, width: u32, height: u32) -> Result<Self> {
        self.check_error()?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(Error::Graphics("invalid dimensions".into()));
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("QuickDraw bitmap target"),
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
        Ok(Self {
            device: self.device.clone(),
            queue: self.queue.clone(),
            adapter: self.adapter.clone(),
            texture,
            width,
            height,
            base: self.base,
            palette: self.palette,
            pipeline: self.pipeline.clone(),
            error: self.error.clone(),
        })
    }
    pub fn check_error(&self) -> Result<()> {
        if let Some(e) = self.error.lock().unwrap().as_ref() {
            return Err(Error::Graphics(e.clone()));
        }
        Ok(())
    }
    pub fn execute(&self, memory: &mut Memory, draw: Draw) -> Result<()> {
        self.execute_clipped(memory, draw, None)
    }
    pub fn execute_clipped(
        &self,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
    ) -> Result<()> {
        self.execute_source(memory, draw, clips, None)
    }
    /// Immutable source indices support retained window copies and scaled bitmaps.
    pub(crate) fn execute_source(
        &self,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        copy_source: Option<&[u8]>,
    ) -> Result<()> {
        self.execute_source_palette(memory, draw, clips, copy_source, None)
    }
    pub(crate) fn execute_source_palette(
        &self,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        copy_source: Option<&[u8]>,
        source_palette: Option<&[[u16; 3]; 256]>,
    ) -> Result<()> {
        self.execute_indexed(memory, draw, clips, copy_source, source_palette, None)
    }
    pub(crate) fn execute_indexed(
        &self,
        memory: &mut Memory,
        draw: Draw,
        clips: Option<&[Rect]>,
        copy_source: Option<&[u8]>,
        source_palette: Option<&[[u16; 3]; 256]>,
        target: Option<IndexedTarget<'_>>,
    ) -> Result<()> {
        if source_palette.is_some() && !matches!(draw, Draw::Blit { .. }) {
            return Err(Error::Graphics(
                "indexed source requires a bitmap blit".into(),
            ));
        }
        if copy_source.is_some() && !matches!(draw, Draw::Copy { .. } | Draw::Blit { .. }) {
            return Err(Error::Graphics(
                "external source requires a copy command".into(),
            ));
        }
        let source_extent = if let Draw::Blit {
            width,
            height,
            source,
            ..
        } = draw
        {
            if width == 0
                || height == 0
                || width > 4096
                || height > 4096
                || [source.left, source.top, source.right, source.bottom]
                    .iter()
                    .any(|v| !(-32768..=32767).contains(v))
                || source.right <= source.left
                || source.bottom <= source.top
                || source.right - source.left > 4096
                || source.bottom - source.top > 4096
                || copy_source.is_none()
            {
                return Err(Error::Graphics("invalid scaled bitmap source".into()));
            }
            width * height
        } else {
            self.width * self.height
        };
        if copy_source.is_some_and(|s| s.len() != source_extent as usize) {
            return Err(Error::Graphics("copy source extent mismatch".into()));
        }
        if let Draw::Blit {
            mask: Some(mask),
            source,
            ..
        } = draw
        {
            if mask.width == 0
                || mask.height == 0
                || mask.width > 4096
                || mask.height > 4096
                || mask.bits.len() != (mask.width * mask.height) as usize
                || mask.bits.iter().any(|&v| v > 1)
                || [
                    mask.rect.left,
                    mask.rect.top,
                    mask.rect.right,
                    mask.rect.bottom,
                ]
                .iter()
                .any(|v| !(-32768..=32767).contains(v))
                || mask.rect.right - mask.rect.left != source.right - source.left
                || mask.rect.bottom - mask.rect.top != source.bottom - source.top
            {
                return Err(Error::Graphics("invalid one-bit blit mask".into()));
            }
        }
        self.check_error()?;
        let mut p = vec![0i32; 24];
        p[0] = self.width as i32;
        p[1] = self.height as i32;
        let (rect, clip) = match draw {
            Draw::Cursor { .. } => {
                p[2] = 3;
                (Rect::new(0, 0, 0, 0), Rect::new(0, 0, 0, 0))
            }
            Draw::Present => (Rect::new(0, 0, 0, 0), Rect::new(0, 0, 0, 0)),
            Draw::Polygon {
                rect,
                clip,
                vertices,
                origin_x,
                origin_y,
                frame,
                pattern,
                foreground,
                background,
                mode,
            } => {
                if !(-65535..=65535).contains(&origin_x)
                    || !(-65535..=65535).contains(&origin_y)
                    || vertices.len() > 16381
                    || frame.is_some_and(|(w, h)| w < 0 || h < 0)
                {
                    return Err(Error::Graphics("invalid polygon geometry".into()));
                }
                if !vertices.is_empty() {
                    let min_x = vertices.iter().map(|p| i32::from(p.0)).min().unwrap();
                    let max_x = vertices.iter().map(|p| i32::from(p.0)).max().unwrap();
                    let min_y = vertices.iter().map(|p| i32::from(p.1)).min().unwrap();
                    let max_y = vertices.iter().map(|p| i32::from(p.1)).max().unwrap();
                    if frame.is_none()
                        && (rect.left < min_x + origin_x
                            || rect.right > max_x + origin_x
                            || rect.top < min_y + origin_y
                            || rect.bottom > max_y + origin_y)
                    {
                        return Err(Error::Graphics(
                            "polygon fill bounds exceed vertex extent".into(),
                        ));
                    }
                    if max_x - min_x > 32767 || max_y - min_y > 32767 {
                        return Err(Error::Graphics(
                            "polygon extent exceeds signed-word raster limit".into(),
                        ));
                    }
                }
                let width = (rect.right.min(self.width as i32) - rect.left.max(0)).max(0) as u64;
                let height = (rect.bottom.min(self.height as i32) - rect.top.max(0)).max(0) as u64;
                if width * height * vertices.len() as u64 > 32_000_000 {
                    return Err(Error::Graphics("GPU polygon work limit exceeded".into()));
                }
                p[2] = 6;
                p[11] = foreground.into();
                p[12] = background.into();
                p[13] = mode as i32;
                p[14] = origin_x;
                p[15] = origin_y;
                for (i, b) in pattern.iter().enumerate() {
                    p[16 + i] = i32::from(*b);
                }
                (rect, clip)
            }
            Draw::Line {
                rect,
                clip,
                line,
                pattern,
                foreground,
                background,
                mode,
            } => {
                if !(-65535..=65535).contains(&line.origin_x)
                    || !(-65535..=65535).contains(&line.origin_y)
                    || line.minor_delta.unsigned_abs() > line.steps
                    || line.steps > 65535
                    || ![-1, 1].contains(&line.direction)
                    || line.width > 32767
                    || line.height > 32767
                {
                    return Err(Error::Graphics("invalid line geometry".into()));
                }
                p[2] = 5;
                p[11] = foreground.into();
                p[12] = background.into();
                p[13] = mode as i32;
                p[14] = line.origin_x;
                p[15] = line.origin_y;
                for (i, b) in pattern.iter().enumerate() {
                    p[16 + i] = i32::from(*b);
                }
                (rect, clip)
            }
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
            Draw::Blit {
                mask: _,
                rect,
                clip,
                source,
                width,
                height,
                foreground,
                background,
                invert,
                mode,
            } => {
                p[2] = 4;
                p[11] = foreground.into();
                p[12] = background.into();
                p[13] = mode as i32;
                p[14] = source.left;
                p[15] = source.top;
                p[16] = width as i32;
                p[17] = height as i32;
                p[18] = source.right;
                p[19] = source.bottom;
                p[20] = i32::from(invert);
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
            .chain(
                p[14..16]
                    .iter()
                    .filter(|_| !matches!(draw, Draw::Line { .. } | Draw::Polygon { .. })),
            )
            .any(|v| !(-32768..=32767).contains(v))
        {
            return Err(Error::Graphics(
                "coordinates outside QuickDraw i16 range".into(),
            ));
        }
        p.push(clips.map_or(-1, |r| r.len() as i32));
        if let Some(clips) = clips {
            if clips.len() > 65536
                || u64::from(self.width) * u64::from(self.height) * clips.len() as u64 > 32_000_000
            {
                return Err(Error::Graphics(
                    "GPU region clipping work limit exceeded".into(),
                ));
            }
            for r in clips {
                let coordinates = [r.left, r.top, r.right, r.bottom];
                if coordinates.iter().any(|v| !(-32768..=32767).contains(v)) {
                    return Err(Error::Graphics(
                        "region coordinates outside GPU range".into(),
                    ));
                }
                p.extend(coordinates);
            }
        }
        if let Some(palette) = source_palette {
            p[21] = 1;
            p.extend(palette.iter().flatten().map(|&v| i32::from(v)));
        }
        if let Draw::Blit {
            mask: Some(mask), ..
        } = draw
        {
            p[22] = p.len() as i32;
            p.extend([
                mask.width as i32,
                mask.height as i32,
                mask.rect.left,
                mask.rect.top,
            ]);
            p.extend(mask.bits.iter().map(|&v| i32::from(v)));
        }
        if let Draw::Polygon {
            vertices, frame, ..
        } = draw
        {
            let (w, h) = frame.unwrap_or((-1, -1));
            p.extend([vertices.len() as i32, i32::from(w), i32::from(h)]);
            for &(x, y) in vertices {
                p.extend([i32::from(x), i32::from(y)]);
            }
        }
        if let Draw::Line { line, .. } = draw {
            p.extend([
                line.minor_delta,
                line.steps as i32,
                i32::from(line.vertical),
                line.direction,
                i32::from(line.width),
                i32::from(line.height),
            ]);
        }
        if let Draw::Cursor { x, y, data, mask } = draw {
            if clips.is_some() {
                return Err(Error::Graphics(
                    "cursor presentation cannot use drawing clips".into(),
                ));
            }
            if !(-65535..=65535).contains(&x) || !(-65535..=65535).contains(&y) {
                return Err(Error::Graphics(
                    "cursor position exceeds guest coordinate range".into(),
                ));
            }
            p.extend([x, y]);
            p.extend(data.map(i32::from));
            p.extend(mask.map(i32::from));
            if memory
                .read(self.base, (self.width * self.height) as usize)?
                .iter()
                .any(|v| *v > 1)
            {
                return Err(Error::Graphics(
                    "cursor presentation requires monochrome indices".into(),
                ));
            }
        }
        if let Some(target) = &target {
            let expensive = match draw {
                Draw::Fill { mode, .. } | Draw::Line { mode, .. } | Draw::Polygon { mode, .. } => {
                    matches!(mode, Transfer::Xor)
                }
                Draw::Copy { mode, .. } => !matches!(mode, Transfer::Copy),
                Draw::Blit {
                    mode,
                    invert,
                    foreground,
                    background,
                    ..
                } => {
                    let identity = target.direct_source
                        && !invert
                        && target.rgb[usize::from(foreground)] == [0; 3]
                        && target.rgb[usize::from(background)] == [65535; 3]
                        && matches!(mode, Transfer::Copy);
                    !identity
                        && (matches!(mode, Transfer::Xor)
                            || source_palette.is_some_and(|p| {
                                p.iter().any(|rgb| *rgb != [0; 3] && *rgb != [65535; 3])
                            }))
                }
                _ => false,
            };
            let width = (rect.right.min(clip.right).min(self.width as i32)
                - rect.left.max(clip.left).max(0))
            .max(0) as u64;
            let height = (rect.bottom.min(clip.bottom).min(self.height as i32)
                - rect.top.max(clip.top).max(0))
            .max(0) as u64;
            if expensive && width * height * (1u64 << target.depth) > 128_000_000 {
                return Err(Error::Graphics(
                    "indexed color matching work limit exceeded".into(),
                ));
            }
            p[2] |= 256;
            p.extend([i32::from(target.depth), i32::from(target.direct_source)]);
            for i in 0..256 {
                p.extend(target.rgb[i].map(i32::from));
                p.push(i32::from(target.defined[i]));
            }
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
        let source_indices = copy_source.map(|s| words_bytes(s.iter().map(|&b| u32::from(b))));
        let source = buffer(
            "immutable copy source",
            source_indices.as_deref().unwrap_or(&indices),
            wgpu::BufferUsages::STORAGE,
        );
        let colors = buffer(
            "palette",
            &target
                .as_ref()
                .map_or(self.palette, |t| {
                    std::array::from_fn(|i| {
                        [
                            (t.rgb[i][0] >> 8) as u8,
                            (t.rgb[i][1] >> 8) as u8,
                            (t.rgb[i][2] >> 8) as u8,
                            255,
                        ]
                    })
                })
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
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
