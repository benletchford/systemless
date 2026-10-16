//! Offscreen library compatibility probe, not a production text backend.
use glyphon::{
    Buffer, Cache, Color, ContentType, CustomGlyph, FontSystem, Metrics, RasterizedCustomGlyph,
    Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use std::error::Error;
use systemless_future::font::Strike;
const WIDTH: u32 = 16;
const HEIGHT: u32 = 12;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Probe {
    device: wgpu::Device,
    queue: wgpu::Queue,
    font_system: FontSystem,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    target: wgpu::Texture,
}
impl Probe {
    async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance.request_adapter(&Default::default()).await?;
        println!("adapter={:?}", adapter.get_info());
        let (device, queue) = adapter.request_device(&Default::default()).await?;
        let cache = Cache::new(&device);
        let mut viewport = Viewport::new(&device, &cache);
        viewport.update(
            &queue,
            Resolution {
                width: WIDTH,
                height: HEIGHT,
            },
        );
        let mut atlas = TextAtlas::new(&device, &queue, &cache, wgpu::TextureFormat::Rgba8Unorm);
        let renderer = TextRenderer::new(&mut atlas, &device, Default::default(), None);
        let font_system =
            FontSystem::new_with_locale_and_db("en-US".into(), glyphon::fontdb::Database::new());
        // Empty text: guest FONT masks supply every glyph, without modern shaping.
        let buffer = Buffer::new_empty(Metrics::new(12.0, 12.0));
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("classic bitmap glyph probe"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Ok(Self {
            device,
            queue,
            font_system,
            swash: SwashCache::new(),
            viewport,
            atlas,
            renderer,
            buffer,
            target,
        })
    }
    fn frame(&mut self, request: Draw<'_>) -> Result<(Vec<u8>, usize)> {
        let Draw {
            id,
            mask,
            width,
            height,
            origin,
            bounds,
            passes,
        } = request;
        let glyphs = [CustomGlyph {
            id,
            left: origin.0,
            top: origin.1,
            width: f32::from(width),
            height: f32::from(height),
            color: Some(Color::rgb(255, 255, 255)),
            snap_to_physical_pixel: true,
            ..Default::default()
        }];
        let mut uploads = 0;
        self.renderer.prepare_with_custom(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            [TextArea {
                buffer: &self.buffer,
                left: 0.0,
                top: 0.0,
                scale: 1.0,
                bounds,
                default_color: Color::rgb(255, 255, 255),
                custom_glyphs: &glyphs,
            }],
            &mut self.swash,
            |input| {
                assert_eq!((input.width, input.height), (width, height));
                uploads += 1;
                Some(RasterizedCustomGlyph {
                    data: mask.iter().map(|bit| bit * 255).collect(),
                    content_type: ContentType::Mask,
                })
            },
        )?;
        let view = self.target.create_view(&Default::default());
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph probe readback"),
            size: u64::from(256 * HEIGHT),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for pass in 0..passes {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if pass == 0 {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glyphon custom bitmap glyph"),
                color_attachments: &attachments,
                ..Default::default()
            });
            self.renderer
                .render(&self.atlas, &self.viewport, &mut render)?;
        }
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        receive.recv()??;
        let padded = readback.slice(..).get_mapped_range()?;
        let bytes = padded
            .as_chunks::<256>()
            .0
            .iter()
            .flat_map(|row| row[..WIDTH as usize * 4].iter().copied())
            .collect();
        drop(padded);
        readback.unmap();
        Ok((bytes, uploads))
    }
}
#[derive(Clone, Copy)]
struct Draw<'a> {
    id: u16,
    mask: &'a [u8],
    width: u16,
    height: u16,
    origin: (f32, f32),
    bounds: TextBounds,
    passes: usize,
}
fn expected(draw: Draw<'_>) -> Vec<u8> {
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    for y in 0..i32::from(draw.height) {
        for x in 0..i32::from(draw.width) {
            let px = draw.origin.0.round() as i32 + x;
            let py = draw.origin.1.round() as i32 + y;
            if (0..WIDTH as i32).contains(&px)
                && (0..HEIGHT as i32).contains(&py)
                && (draw.bounds.left..draw.bounds.right).contains(&px)
                && (draw.bounds.top..draw.bounds.bottom).contains(&py)
                && draw.mask[y as usize * usize::from(draw.width) + x as usize] != 0
            {
                let offset = ((py as u32 * WIDTH + px as u32) * 4) as usize;
                pixels[offset..offset + 3].fill(255);
            }
        }
    }
    pixels
}
fn check(label: &str, actual: &[u8], reference: &[u8]) {
    assert_eq!(actual.len(), reference.len());
    if let Some((offset, (a, b))) = actual
        .iter()
        .zip(reference)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("{label}: byte {offset} was {a}, expected {b}");
    }
    println!("PASS {label}: {} exact RGBA pixels", actual.len() / 4);
}
fn main() -> Result<()> {
    let words = [
        0x9000u16, 65, 66, 4, 0xfffe, 0xffff, 4, 3, 12, 2, 1, 0xffff, 1, 0xe400, 0xba00, 0x4600, 0,
        3, 5, 7, 0x0104, 0xffff, 0x0203, 0xffff,
    ];
    let font_bytes: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let font = Strike::parse(&font_bytes)?;
    let glyph = font.glyph(b'A');
    let mask = font.mask(b'A')?;
    let mut probe = pollster::block_on(Probe::new())?;
    let draw = Draw {
        id: 1,
        mask: &mask,
        width: glyph.width,
        height: glyph.height,
        origin: (2.25, 2.25),
        bounds: TextBounds {
            left: 0,
            top: 0,
            right: WIDTH as i32,
            bottom: HEIGHT as i32,
        },
        passes: 1,
    };
    let (first, uploads) = probe.frame(draw)?;
    check(
        "guest bitmap mask and pixel snapping",
        &first,
        &expected(draw),
    );
    assert_eq!(uploads, 1);
    let (again, uploads) = probe.frame(draw)?;
    check("cached glyph reuse", &again, &first);
    assert_eq!(uploads, 0);
    let clipped = Draw {
        bounds: TextBounds {
            left: 3,
            top: 3,
            right: 5,
            bottom: 5,
        },
        ..draw
    };
    let (pixels, _) = probe.frame(clipped)?;
    check("partial glyph clipping", &pixels, &expected(clipped));
    let mut edited_font = font_bytes.clone();
    edited_font[26] ^= 0x80;
    let edited = Strike::parse(&edited_font)?.mask(b'A')?;
    let stale = Draw {
        mask: &edited,
        ..draw
    };
    let (pixels, uploads) = probe.frame(stale)?;
    check(
        "unchanged library cache key retains old source",
        &pixels,
        &first,
    );
    assert_eq!(uploads, 0);
    assert_ne!(pixels, expected(stale));
    let fresh = Draw { id: 2, ..stale };
    let (pixels, uploads) = probe.frame(fresh)?;
    check(
        "new source generation refreshes glyph",
        &pixels,
        &expected(fresh),
    );
    assert_eq!(uploads, 1);
    let (twice, _) = probe.frame(Draw { passes: 2, ..draw })?;
    check("stock composition repeats without XOR", &twice, &first);
    let black: Vec<_> = (0..WIDTH * HEIGHT).flat_map(|_| [0, 0, 0, 255]).collect();
    assert_ne!(
        twice, black,
        "stock renderer unexpectedly implemented QuickDraw XOR"
    );
    println!("RESULT bitmap adapter viable; explicit source generations and QuickDraw GPU transfer adapter required");
    Ok(())
}
