//! Custom chrome uses guest-owned rectangles and Font Manager strikes.
use crate::renderer::Quad;
use systemless::{
    quickdraw::{fonts, text},
    runner::ComparisonDialog,
};
type Rect = (i16, i16, i16, i16);
const BLACK: [f32; 4] = [0., 0., 0., 1.];
const WHITE: [f32; 4] = [1., 1., 1., 1.];
fn rect(out: &mut Vec<Quad>, r: Rect, clip: Rect, c: [f32; 4]) {
    let r = (
        r.0.max(clip.0),
        r.1.max(clip.1),
        r.2.min(clip.2),
        r.3.min(clip.3),
    );
    if r.0 < r.2 && r.1 < r.3 {
        out.push(Quad {
            rect: [r.1 as f32, r.0 as f32, r.3 as f32, r.2 as f32],
            color: c,
        });
    }
}
fn border(out: &mut Vec<Quad>, r: Rect, clip: Rect, c: [f32; 4]) {
    for line in [
        (r.0, r.1, r.0 + 1, r.3),
        (r.2 - 1, r.1, r.2, r.3),
        (r.0, r.1, r.2, r.1 + 1),
        (r.0, r.3 - 1, r.2, r.3),
    ] {
        rect(out, line, clip, c);
    }
}
fn positions(s: &str, font: (i16, i16)) -> Vec<i16> {
    let mut p = vec![0i16];
    let (_, scale) = fonts::get_font_face_scaled(font.0, font.1);
    for ch in s.chars() {
        let advance = text::get_unicode_glyph(font.0, font.1, ch)
            .map_or(0, |(g, _)| i16::from(g.advance) * i16::from(scale));
        p.push(p.last().unwrap().saturating_add(advance));
    }
    p
}
fn label(
    out: &mut Vec<Quad>,
    s: &str,
    font: (i16, i16),
    x: i16,
    baseline: i16,
    clip: Rect,
    selection: Option<Rect>,
    ink: [f32; 4],
) {
    let (_, scale) = fonts::get_font_face_scaled(font.0, font.1);
    let scale = i16::from(scale);
    let mut pen = x;
    for ch in s.chars() {
        if let Some((g, data)) = text::get_unicode_glyph(font.0, font.1, ch) {
            for y in 0..g.height {
                for x in 0..g.width {
                    if data[g.data_offset + y as usize * g.width as usize + x as usize]
                        >= fonts::MONO_COVERAGE_THRESHOLD
                    {
                        let l = pen + (i16::from(g.origin_x) + i16::from(x)) * scale;
                        let t = baseline + (i16::from(g.origin_y) + i16::from(y)) * scale;
                        for dy in 0..scale {
                            for dx in 0..scale {
                                let x = l + dx;
                                let y = t + dy;
                                let selected = selection
                                    .is_some_and(|r| y >= r.0 && y < r.2 && x >= r.1 && x < r.3);
                                rect(
                                    out,
                                    (y, x, y + 1, x + 1),
                                    clip,
                                    if selected { WHITE } else { ink },
                                );
                            }
                        }
                    }
                }
            }
            pen += i16::from(g.advance) * scale;
        }
    }
}
pub fn controls(dialog: Option<&ComparisonDialog>) -> Vec<Quad> {
    let mut out = Vec::new();
    let Some(d) = dialog else {
        return out;
    };
    for i in &d.items {
        let r = i.bounds;
        let clip = d.bounds;
        let ink = if i.enabled {
            BLACK
        } else {
            [0.5, 0.5, 0.5, 1.]
        };
        match i.kind {
            4 => {
                if i.number == d.default_item { rect(&mut out,(r.0-4,r.1-4,r.2+4,r.3+4),clip,WHITE); }
                rect(&mut out, r, clip, if i.pressed { BLACK } else { WHITE });
                border(&mut out, r, clip, ink);
                if i.number == d.default_item {
                    border(&mut out, (r.0 - 3, r.1 - 3, r.2 + 3, r.3 + 3), clip, ink);
                }
                let font = (0, 12);
                let p = positions(&i.text, font);
                let m = text::get_font_metrics(0, 12);
                label(
                    &mut out,
                    &i.text,
                    font,
                    r.1 + (r.3 - r.1 - p.last().unwrap()) / 2,
                    r.0 + (r.2 - r.0 - m.ascent - m.descent) / 2 + m.ascent,
                    r,
                    None,
                    if i.pressed { WHITE } else { ink },
                );
            }
            5 => {
                rect(&mut out, r, clip, WHITE);
                let t = r.0 + (r.2 - r.0 - 12) / 2;
                let box_rect = (t, r.1, t + 12, r.1 + 12);
                border(&mut out, box_rect, clip, ink);
                if i.value != 0 {
                    for n in 2..10 {
                        rect(
                            &mut out,
                            (t + n, r.1 + n, t + n + 1, r.1 + n + 1),
                            clip,
                            ink,
                        );
                        rect(
                            &mut out,
                            (t + n, r.1 + 11 - n, t + n + 1, r.1 + 12 - n),
                            clip,
                            ink,
                        );
                    }
                }
                let m = text::get_font_metrics(0, 12);
                label(
                    &mut out,
                    &i.text,
                    (0, 12),
                    r.1 + 17,
                    r.0 + (r.2 - r.0 - m.ascent - m.descent) / 2 + m.ascent,
                    r,
                    None,
                    ink,
                );
            }
            16 => {
                let frame = (r.0 - 3, r.1 - 3, r.2 + 3, r.3 + 3);
                rect(&mut out, frame, clip, WHITE);
                border(&mut out, frame, clip, ink);
                let p = positions(&i.text, d.font);
                let m = text::get_font_metrics(d.font.0, d.font.1);
                let selected = i.selection.and_then(|(a, b)| {
                    let a = (a.max(0) as usize).min(p.len() - 1);
                    let b = (b.max(0) as usize).min(p.len() - 1);
                    (a < b).then(|| {
                        (
                            r.0,
                            if a == 0 { r.1 } else { r.1 + 1 + p[a] },
                            (r.0 + m.ascent + m.descent + m.leading).min(r.2),
                            if b == p.len() - 1 {
                                r.3
                            } else {
                                r.1 + 1 + p[b]
                            },
                        )
                    })
                });
                if let Some(sel) = selected {
                    rect(&mut out, sel, r, BLACK);
                }
                label(
                    &mut out,
                    &i.text,
                    d.font,
                    r.1 + 1,
                    r.0 + m.ascent,
                    r,
                    selected,
                    ink,
                );
                if i.caret && selected.is_none() {
                    let at = i
                        .selection
                        .map_or(0, |(_, b)| b.max(0) as usize)
                        .min(p.len() - 1);
                    let x = r.1 + 1 + p[at];
                    rect(&mut out, (r.0, x, r.2, x + 1), r, ink);
                }
            }
            _ => {}
        }
    }
    out
}
