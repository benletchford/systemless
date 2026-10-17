//! Glyphon prepares binary guest glyph masks; QuickDraw owns destination semantics.
use crate::{
    graphics::{Graphics, Rect},
    memory::Memory,
    quickdraw::Ports,
    text::GlyphCell,
    Error, Result,
};
use glyphon::{
    Buffer, Cache, Color, ContentType, CustomGlyph, FontSystem, Metrics, RasterizedCustomGlyph,
    Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
mod identities;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MAX_BYTES: usize = 1_000_000;
pub(crate) struct Renderer {
    cache: Cache,
    masks: identities::Masks,
    fonts: FontSystem,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    target: Option<wgpu::Texture>,
}
fn error(e: impl std::fmt::Display) -> Error {
    Error::Graphics(format!("Glyphon: {e}"))
}
pub(crate) fn validate(cells: &[GlyphCell]) -> Result<()> {
    for cell in cells {
        let width = cell.bounds.right - cell.bounds.left;
        let height = cell.bounds.bottom - cell.bounds.top;
        if !(0..=4096).contains(&width)
            || !(0..=4096).contains(&height)
            || cell.mask.len() != width as usize * height as usize
            || cell.mask.len() > MAX_BYTES
            || cell.mask.iter().any(|b| *b > 1)
        {
            return Err(error("invalid or oversized binary guest glyph"));
        }
    }
    Ok(())
}
impl Renderer {
    fn new(gpu: &Graphics) -> Self {
        let cache = Cache::new(&gpu.device);
        let viewport = Viewport::new(&gpu.device, &cache);
        let mut atlas = TextAtlas::new(&gpu.device, &gpu.queue, &cache, FORMAT);
        let renderer = TextRenderer::new(&mut atlas, &gpu.device, Default::default(), None);
        Self {
            cache,
            masks: identities::Masks::new(256, MAX_BYTES),
            fonts: FontSystem::new_with_locale_and_db(
                "en-US".into(),
                glyphon::fontdb::Database::new(),
            ),
            swash: SwashCache::new(),
            viewport,
            atlas,
            renderer,
            buffer: Buffer::new_empty(Metrics::new(12.0, 12.0)),
            target: None,
        }
    }
    fn prepare(&mut self, gpu: &Graphics, cell: &GlyphCell) -> Result<()> {
        let width = (cell.bounds.right - cell.bounds.left) as u16;
        let height = (cell.bounds.bottom - cell.bounds.top) as u16;
        let (id, reset) = self
            .masks
            .resolve(width, height, &cell.mask)
            .map_err(error)?;
        if reset {
            self.atlas = TextAtlas::new(&gpu.device, &gpu.queue, &self.cache, FORMAT);
            self.renderer =
                TextRenderer::new(&mut self.atlas, &gpu.device, Default::default(), None);
        }
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.width() < u32::from(width) || t.height() < u32::from(height))
        {
            let mut extent = (
                u32::from(width).next_power_of_two(),
                u32::from(height).next_power_of_two(),
            );
            if u64::from(extent.0) * u64::from(extent.1) > MAX_BYTES as u64 {
                extent = (u32::from(width), u32::from(height));
            }
            self.target = Some(gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shared guest text mask"),
                size: wgpu::Extent3d {
                    width: extent.0,
                    height: extent.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
            self.viewport.update(
                &gpu.queue,
                Resolution {
                    width: extent.0,
                    height: extent.1,
                },
            );
        }
        let glyphs = [CustomGlyph {
            id,
            left: 0.0,
            top: 0.0,
            width: f32::from(width),
            height: f32::from(height),
            color: Some(Color::rgb(255, 255, 255)),
            snap_to_physical_pixel: true,
            ..Default::default()
        }];
        self.renderer
            .prepare_with_custom(
                &gpu.device,
                &gpu.queue,
                &mut self.fonts,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.buffer,
                    left: 0.0,
                    top: 0.0,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: width.into(),
                        bottom: height.into(),
                    },
                    default_color: Color::rgb(255, 255, 255),
                    custom_glyphs: &glyphs,
                }],
                &mut self.swash,
                |_| {
                    Some(RasterizedCustomGlyph {
                        data: cell.mask.iter().map(|b| b * 255).collect(),
                        content_type: ContentType::Mask,
                    })
                },
            )
            .map_err(error)?;
        let view = self
            .target
            .as_ref()
            .unwrap()
            .create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shared Glyphon guest text"),
                color_attachments: &attachments,
                ..Default::default()
            });
            self.renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(error)?;
        }
        gpu.queue.submit([encoder.finish()]);
        Ok(())
    }
}
pub(crate) fn draw(
    gpu: &Graphics,
    memory: &mut Memory,
    ports: Ports,
    cell: &GlyphCell,
    mode: i16,
) -> Result<()> {
    if cell.bounds.left == cell.bounds.right || cell.bounds.top == cell.bounds.bottom {
        return Ok(());
    }
    let mut state = gpu
        .text
        .lock()
        .map_err(|_| error("text cache lock poisoned"))?;
    let renderer = state.get_or_insert_with(|| Renderer::new(gpu));
    renderer.prepare(gpu, cell)?;
    ports.copy_gpu_mask_region(
        memory,
        gpu,
        renderer.target.as_ref().unwrap(),
        (
            Rect::new(
                0,
                0,
                cell.bounds.right - cell.bounds.left,
                cell.bounds.bottom - cell.bounds.top,
            ),
            cell.bounds,
        ),
        mode,
    )
}
