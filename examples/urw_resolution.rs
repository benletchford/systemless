//! Isolated high-resolution text proof using Toolbox Showcase strings and sizes.
//! This is a font specimen, not an emulator framebuffer capture or a display backend.
use image::{imageops, Rgb, RgbImage};
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, HintingInstance, OutlinePen, SmoothMode, Target},
    FontRef, MetadataProvider,
};
use std::path::Path;
use systemless::quickdraw::fonts::{
    get_font_face_or_default, FONT_CHICAGO, FONT_GENEVA, FONT_MONACO,
};

const WIDTH: u32 = 620;
const HEIGHT: u32 = 300;
const SANS: &[u8] = include_bytes!("../src/quickdraw/fonts/urw/NimbusSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../src/quickdraw/fonts/urw/NimbusSans-Bold.ttf");
const MONO: &[u8] = include_bytes!("../src/quickdraw/fonts/urw/NimbusMonoPS-Regular.ttf");

#[derive(Default)]
struct Pen(Vec<zeno::Command>);
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::MoveTo((x, y).into()));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::LineTo((x, y).into()));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0
            .push(zeno::Command::QuadTo((cx, cy).into(), (x, y).into()));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(zeno::Command::CurveTo(
            (cx0, cy0).into(),
            (cx1, cy1).into(),
            (x, y).into(),
        ));
    }
    fn close(&mut self) {
        self.0.push(zeno::Command::Close);
    }
}

fn text(
    image: &mut RgbImage,
    scale: u32,
    native: bool,
    family: i16,
    size: i16,
    origin: (i32, i32),
    label: &str,
    color: [u8; 3],
) {
    // Keep every pen position, baseline, and advance in the guest's logical
    // coordinate system. Only outline rasterization receives more pixels.
    let face = get_font_face_or_default(family, size);
    let bytes = match family {
        FONT_CHICAGO => BOLD,
        FONT_MONACO => MONO,
        _ => SANS,
    };
    let font = FontRef::new(bytes).unwrap();
    let outlines = font.outline_glyphs();
    let hinter = HintingInstance::new(
        &outlines,
        Size::new(f32::from(size) * scale as f32),
        LocationRef::default(),
        Target::from(SmoothMode::Normal),
    )
    .unwrap();
    let mut pen_x = origin.0;
    for ch in label.chars() {
        assert!(
            ch.is_ascii(),
            "specimen strings must use the shared ASCII layout"
        );
        let logical = &face.glyphs[ch as usize - 32];
        let (pixels, width, height, left, top) = if native {
            let mut path = Pen::default();
            outlines
                .get(font.charmap().map(ch).unwrap())
                .unwrap()
                .draw(DrawSettings::hinted(&hinter, false), &mut path)
                .unwrap();
            let mut pixels = Vec::new();
            let place = zeno::Mask::new(path.0.as_slice())
                .origin(zeno::Origin::BottomLeft)
                .inspect(|format, w, h| pixels.resize(format.buffer_size(w, h), 0))
                .render_into(&mut pixels, None);
            (pixels, place.width, place.height, place.left, -place.top)
        } else {
            let mut pixels = Vec::new();
            for y in 0..u32::from(logical.height) * scale {
                for x in 0..u32::from(logical.width) * scale {
                    pixels.push(
                        face.data[logical.data_offset
                            + (y / scale) as usize * usize::from(logical.width)
                            + (x / scale) as usize],
                    );
                }
            }
            (
                pixels,
                u32::from(logical.width) * scale,
                u32::from(logical.height) * scale,
                i32::from(logical.origin_x) * scale as i32,
                i32::from(logical.origin_y) * scale as i32,
            )
        };
        for y in 0..height {
            for x in 0..width {
                let px = pen_x * scale as i32 + left + x as i32;
                let py = origin.1 * scale as i32 + top + y as i32;
                if px < 0 || py < 0 || px >= image.width() as i32 || py >= image.height() as i32 {
                    continue;
                }
                let alpha = u32::from(pixels[(y * width + x) as usize]);
                let background = image.get_pixel_mut(px as u32, py as u32);
                for channel in 0..3 {
                    background[channel] = ((u32::from(color[channel]) * alpha
                        + u32::from(background[channel]) * (255 - alpha)
                        + 127)
                        / 255) as u8;
                }
            }
        }
        pen_x += i32::from(logical.advance);
    }
}

fn specimen(scale: u32, native: bool) -> RgbImage {
    let mut image = RgbImage::from_pixel(WIDTH * scale, HEIGHT * scale, Rgb([255; 3]));
    text(
        &mut image,
        scale,
        native,
        FONT_CHICAGO,
        12,
        (20, 26),
        "Toolbox text: resolution proof",
        [0; 3],
    );
    text(
        &mut image,
        scale,
        native,
        FONT_GENEVA,
        9,
        (20, 43),
        "Same URW fonts, logical sizes, baselines, and per-character spacing.",
        [70; 3],
    );
    let rows = [
        (FONT_GENEVA, 9, 72, "Geneva 9pt / Nimbus Sans", [0; 3]),
        (
            FONT_GENEVA,
            9,
            88,
            "TextEdit manages styled and plain text formatting, automatic word wrapping,",
            [0; 3],
        ),
        (
            FONT_GENEVA,
            9,
            102,
            "selection highlighting, and clipboard scrap operations.",
            [0; 3],
        ),
        (
            FONT_GENEVA,
            10,
            135,
            "Geneva 10pt / Plain text and colored text",
            [0; 3],
        ),
        (
            FONT_GENEVA,
            10,
            151,
            "TETextBox renders transient wrapped paragraphs with specified justification.",
            [0, 55, 170],
        ),
        (FONT_MONACO, 9, 184, "Monaco 9pt / Nimbus Mono PS", [0; 3]),
        (
            FONT_MONACO,
            9,
            200,
            "Click to move the insertion point or drag across characters to select text.",
            [0; 3],
        ),
        (
            FONT_GENEVA,
            12,
            235,
            "Geneva 12pt / Aa Bb Mm 0123456789",
            [0; 3],
        ),
    ];
    for (family, size, y, label, color) in rows {
        text(
            &mut image,
            scale,
            native,
            family,
            size,
            (20, y),
            label,
            color,
        );
    }
    for y in 255 * scale..HEIGHT * scale {
        for x in 0..WIDTH * scale {
            image.put_pixel(x, y, Rgb([30, 36, 46]));
        }
    }
    text(
        &mut image,
        scale,
        native,
        FONT_GENEVA,
        10,
        (20, 278),
        "Reversed text: open counters, fine strokes, and clear spacing.",
        [245; 3],
    );
    image
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .expect("usage: urw_resolution <output-directory>");
    let output = Path::new(&output);
    std::fs::create_dir_all(output).unwrap();
    specimen(1, false).save(output.join("text-1x.png")).unwrap();
    let antialiased_1x = specimen(1, true);
    antialiased_1x.save(output.join("text-1x-aa.png")).unwrap();
    for scale in [2, 3] {
        let nearest = specimen(scale, false);
        let native = specimen(scale, true);
        assert_eq!(nearest.dimensions(), native.dimensions());
        assert_ne!(
            nearest, native,
            "native outlines must not just enlarge the guest bitmap"
        );
        assert_ne!(
            imageops::resize(
                &antialiased_1x,
                WIDTH * scale,
                HEIGHT * scale,
                imageops::FilterType::Nearest
            ),
            native,
            "higher resolution must add detail beyond enlarging 1x antialiasing"
        );
        native
            .save(output.join(format!("text-{scale}x.png")))
            .unwrap();
        nearest
            .save(output.join(format!("enlarged-{scale}x.png")))
            .unwrap();
        let mut comparison =
            RgbImage::from_pixel(WIDTH * scale, (HEIGHT * 2 + 26) * scale, Rgb([235; 3]));
        imageops::replace(&mut comparison, &nearest, 0, 0);
        text(
            &mut comparison,
            scale,
            true,
            FONT_GENEVA,
            10,
            (20, HEIGHT as i32 + 17),
            &format!("ABOVE: enlarged 1x bitmap    BELOW: freshly rasterized {scale}x outlines"),
            [0; 3],
        );
        imageops::replace(
            &mut comparison,
            &native,
            0,
            i64::from((HEIGHT + 26) * scale),
        );
        comparison
            .save(output.join(format!("comparison-{scale}x.png")))
            .unwrap();
    }
    println!(
        "Wrote 1x/2x/3x specimens and equal-size comparisons to {}",
        output.display()
    );
}
