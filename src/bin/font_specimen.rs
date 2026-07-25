//! Font specimen generator — renders every registered systemless face
//! (all families, every size) as a labelled specimen row on both a white
//! and a black background, so the hand-authored bitmap faces can be
//! audited directly with the eye instead of by hunting through game
//! screenshots.
//!
//! Run:  cargo run --bin font_specimen
//! Out:  target/font_specimens/specimen_white.png
//!       target/font_specimens/specimen_black.png
//!       target/font_specimens/specimen_white@1x.png   (true pixels)
//!       target/font_specimens/specimen_black@1x.png
//!
//! Every glyph is blitted through the same `get_glyph` path the QuickDraw
//! text traps use (advance / origin_x / origin_y / data_offset), so the
//! specimen is pixel-accurate to what apps actually draw.

use systemless::quickdraw::fonts::{font_name_for_id, FontMetrics, FONT_TABLE};
use systemless::quickdraw::text::get_glyph;

const LABEL_FONT: i16 = 0; // Chicago
const LABEL_SIZE: i16 = 12;
const SCALE: u32 = 4;
const PAD: i32 = 6; // left/right margin, px (unscaled)
const ROW_GAP: i32 = 4; // vertical gap between rows, px (unscaled)
const LABEL_W: i32 = 78; // fixed column for the label, px (unscaled)

// Exercises the reported trouble spots (kerning of name/with, the mangled
// w, i/l spacing, digits) plus a full pangram and common UI words.
const SPECIMEN: &str =
    "The quick brown fox jumps over the LAZY dog. name with new  Cancel OK  0123456789 !?(),.";

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<u8>, // 0 = ink-absent, 1 = ink-present
}

impl Canvas {
    fn new(w: i32, h: i32) -> Self {
        Canvas {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }
    fn set(&mut self, x: i32, y: i32) {
        if x >= 0 && x < self.w && y >= 0 && y < self.h {
            self.px[(y * self.w + x) as usize] = 1;
        }
    }
}

/// Blit `text` in face (font_id,size) with its baseline at `baseline_y`,
/// pen starting at `x`. Returns the pen x after the last glyph.
fn draw_text(
    canvas: &mut Canvas,
    x: i32,
    baseline_y: i32,
    font_id: i16,
    size: i16,
    text: &str,
) -> i32 {
    let mut pen = x;
    for ch in text.chars() {
        if let Some((glyph, data)) = get_glyph(font_id, size, ch) {
            let left = pen + glyph.origin_x as i32;
            for gy in 0..glyph.height as i32 {
                let sy = baseline_y + glyph.origin_y as i32 + gy;
                for gx in 0..glyph.width as i32 {
                    let idx = glyph.data_offset + (gy * glyph.width as i32 + gx) as usize;
                    if idx < data.len() && data[idx] >= 128 {
                        canvas.set(left + gx, sy);
                    }
                }
            }
            pen += glyph.advance as i32;
        } else {
            pen += 6;
        }
    }
    pen
}

fn text_width(font_id: i16, size: i16, text: &str) -> i32 {
    text.chars()
        .map(|ch| {
            get_glyph(font_id, size, ch)
                .map(|(g, _)| g.advance as i32)
                .unwrap_or(6)
        })
        .sum()
}

fn row_height(m: &FontMetrics) -> i32 {
    (m.ascent + m.descent + m.leading) as i32
}

fn main() {
    // Curate the face list: FONT_TABLE order, deduped by (id,size), so
    // every registered family/size appears exactly once — aliases
    // (Application, Helvetica, London, Venice, Cairo) included, since
    // seeing them render as their substitute is the point.
    let faces: Vec<&_> = FONT_TABLE.iter().collect();

    // Pass 1: measure.
    let mut total_h = PAD;
    let mut max_w = 0i32;
    for f in &faces {
        let rh = row_height(&f.metrics).max(row_height(&label_metrics()));
        let specimen_w = text_width(f.font_id, f.size, SPECIMEN);
        max_w = max_w.max(PAD + LABEL_W + specimen_w + PAD);
        total_h += rh + ROW_GAP;
    }
    total_h += PAD;

    let mut canvas = Canvas::new(max_w, total_h);

    // Pass 2: draw.
    let mut y = PAD;
    for f in &faces {
        let rh = row_height(&f.metrics).max(row_height(&label_metrics()));
        let baseline = y + label_metrics().ascent.max(f.metrics.ascent) as i32;
        let name = font_name_for_id(f.font_id).unwrap_or("?");
        let label = format!("{} {}", name, f.size);
        draw_text(&mut canvas, PAD, baseline, LABEL_FONT, LABEL_SIZE, &label);
        draw_text(
            &mut canvas,
            PAD + LABEL_W,
            baseline,
            f.font_id,
            f.size,
            SPECIMEN,
        );
        y += rh + ROW_GAP;
    }

    std::fs::create_dir_all("target/font_specimens").expect("mkdir");
    save(
        &canvas,
        false,
        "target/font_specimens/specimen_white@1x.png",
        1,
    );
    save(
        &canvas,
        true,
        "target/font_specimens/specimen_black@1x.png",
        1,
    );
    save(
        &canvas,
        false,
        "target/font_specimens/specimen_white.png",
        SCALE,
    );
    save(
        &canvas,
        true,
        "target/font_specimens/specimen_black.png",
        SCALE,
    );
    println!(
        "wrote {} faces to target/font_specimens/ ({}x{} @1x, {}x scaled)",
        faces.len(),
        canvas.w,
        canvas.h,
        SCALE
    );
}

fn label_metrics() -> FontMetrics {
    systemless::quickdraw::fonts::get_font_face_or_default(LABEL_FONT, LABEL_SIZE).metrics
}

fn save(canvas: &Canvas, black_bg: bool, path: &str, scale: u32) {
    let (bg, ink): (u8, u8) = if black_bg { (0, 255) } else { (255, 0) };
    let w = canvas.w as u32 * scale;
    let h = canvas.h as u32 * scale;
    let mut buf = vec![bg; (w * h) as usize];
    for cy in 0..canvas.h {
        for cx in 0..canvas.w {
            if canvas.px[(cy * canvas.w + cx) as usize] == 1 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = cx as u32 * scale + sx;
                        let py = cy as u32 * scale + sy;
                        buf[(py * w + px) as usize] = ink;
                    }
                }
            }
        }
    }
    let img: image::GrayImage = image::GrayImage::from_raw(w, h, buf).expect("image");
    img.save(path).expect("save png");
}
