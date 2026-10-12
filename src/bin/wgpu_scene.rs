//! Small custom control renderer. Every rectangle and glyph advance comes from guest snapshots.
use super::{renderer::Quad, Update};
use crate::{
    frames::{self, Rect},
    text::ClassicLine,
};
use systemless::runner::DialogItemKind;
const INK: [f32; 4] = [0.10, 0.11, 0.13, 1.];
const PAPER: [f32; 4] = [0.96, 0.96, 0.95, 1.];

fn rect(out: &mut Vec<Quad>, r: Rect, clip: Rect, color: [f32; 4]) {
    if let Some(r) = r.intersection(clip) {
        out.push(Quad {
            rect: [r.left as f32, r.top as f32, r.right as f32, r.bottom as f32],
            color,
        });
    }
}
fn border(out: &mut Vec<Quad>, r: Rect, clip: Rect, color: [f32; 4]) {
    rect(
        out,
        Rect {
            bottom: r.top + 1,
            ..r
        },
        clip,
        color,
    );
    rect(
        out,
        Rect {
            top: r.bottom - 1,
            ..r
        },
        clip,
        color,
    );
    rect(
        out,
        Rect {
            right: r.left + 1,
            ..r
        },
        clip,
        color,
    );
    rect(
        out,
        Rect {
            left: r.right - 1,
            ..r
        },
        clip,
        color,
    );
}
fn label(
    raster: u32,
    out: &mut Vec<Quad>,
    line: &ClassicLine,
    x: i32,
    baseline: i32,
    clip: Rect,
    color: [f32; 4],
) {
    if let Some(spans) = crate::text::comparison_outline_spans(line, raster) {
        for (left, top, right, bottom, alpha) in spans {
            let r = [
                (x as f32 + left).max(clip.left as f32),
                (baseline as f32 + top).max(clip.top as f32),
                (x as f32 + right).min(clip.right as f32),
                (baseline as f32 + bottom).min(clip.bottom as f32),
            ];
            if r[0] < r[2] && r[1] < r[3] {
                let mut ink = color;
                ink[3] *= alpha;
                out.push(Quad {
                    rect: r,
                    color: ink,
                });
            }
        }
        return;
    }
    for &(left, top, width) in &line.ink {
        rect(
            out,
            Rect {
                left: x + left,
                top: baseline + top,
                right: x + left + width,
                bottom: baseline + top + 1,
            },
            clip,
            color,
        );
    }
}
pub(super) fn controls(update: &Update, width: u32, height: u32, raster: u32) -> Vec<Quad> {
    let viewport = Rect {
        top: 0,
        left: 0,
        bottom: height as i32,
        right: width as i32,
    };
    let mut out = Vec::new();
    // Active retained guest menus must cover controls exactly as in the guest.
    if update.guest_menu_tracking || update.guest_popup.is_some() {
        return out;
    }
    for piece in frames::dialog_item_pieces(&update.dialogs, &update.windows, viewport) {
        let dialog = &update.dialogs[piece.dialog];
        let item = &dialog.items[piece.item];
        let r = Rect::from(item.bounds);
        let clip = piece.clip;
        let color = if item.enabled {
            INK
        } else {
            [0.5, 0.5, 0.5, 1.]
        };
        match item.kind {
            DialogItemKind::Button => {
                rect(&mut out, piece.source, clip, PAPER);
                let fill = if item.pressed {
                    [0.70, 0.76, 0.84, 1.]
                } else {
                    [1., 1., 1., 1.]
                };
                rect(&mut out, r, clip, fill);
                border(&mut out, r, clip, color);
                if dialog.default_item == Some(item.number) {
                    border(
                        &mut out,
                        Rect {
                            top: r.top - 3,
                            left: r.left - 3,
                            bottom: r.bottom + 3,
                            right: r.right + 3,
                        },
                        clip,
                        color,
                    );
                }
                let line = ClassicLine::unicode(&item.text, 0, 12);
                let m = systemless::quickdraw::text::get_font_metrics(0, 12);
                let x = r.left + (r.width() - line.positions.last().copied().unwrap_or(0)) / 2;
                let baseline = r.top
                    + (r.height() - i32::from(m.ascent) - i32::from(m.descent)) / 2
                    + i32::from(m.ascent);
                label(raster, &mut out, &line, x, baseline, clip, color);
            }
            DialogItemKind::Checkbox => {
                rect(&mut out, piece.source, clip, PAPER);
                let top = r.top + (r.height() - 12) / 2;
                let box_rect = Rect {
                    top,
                    left: r.left,
                    bottom: top + 12,
                    right: r.left + 12,
                };
                rect(&mut out, box_rect, clip, [1.; 4]);
                border(&mut out, box_rect, clip, color);
                if item.value.is_some_and(|v| v != 0) {
                    for i in 2..10 {
                        rect(
                            &mut out,
                            Rect {
                                top: top + i,
                                left: r.left + i,
                                bottom: top + i + 1,
                                right: r.left + i + 1,
                            },
                            clip,
                            color,
                        );
                        rect(
                            &mut out,
                            Rect {
                                top: top + i,
                                left: r.left + 11 - i,
                                bottom: top + i + 1,
                                right: r.left + 12 - i,
                            },
                            clip,
                            color,
                        );
                    }
                }
                let line = ClassicLine::unicode(&item.text, 0, 12);
                let m = systemless::quickdraw::text::get_font_metrics(0, 12);
                label(
                    raster,
                    &mut out,
                    &line,
                    r.left + 16,
                    r.top
                        + (r.height() - i32::from(m.ascent) - i32::from(m.descent)) / 2
                        + i32::from(m.ascent),
                    clip,
                    color,
                );
            }
            DialogItemKind::EditText => {
                let Some(layout) = &item.edit_text_layout else {
                    continue;
                };
                if layout.wrap {
                    continue;
                }
                let Some((start, end)) = item.selection else {
                    continue;
                };
                if start < 0 || end < start {
                    continue;
                }
                let line = ClassicLine::unicode(&item.text, layout.font.0, layout.font.1);
                let visible = if layout.text_edit_geometry {
                    item.text
                        .trim_end_matches([' ', '\r', '\n'])
                        .chars()
                        .count()
                } else {
                    line.positions.len() - 1
                };
                let geometry = crate::text::dialog_field_geometry(
                    &line,
                    visible,
                    layout,
                    (start as usize, end as usize),
                    dialog.active,
                    item.caret_visible.unwrap_or(false),
                    r.width(),
                    r.height(),
                );
                rect(&mut out, piece.source, clip, PAPER);
                rect(&mut out, r, clip, [1.; 4]);
                border(
                    &mut out,
                    Rect {
                        top: r.top - 1,
                        left: r.left - 1,
                        bottom: r.bottom + 1,
                        right: r.right + 1,
                    },
                    clip,
                    color,
                );
                let local = |q: (i32, i32, i32, i32)| Rect {
                    top: r.top + q.0,
                    left: r.left + q.1,
                    bottom: r.top + q.2,
                    right: r.left + q.3,
                };
                if let Some(selection) = geometry.selection {
                    rect(&mut out, local(selection), clip, [0.69, 0.80, 0.96, 1.]);
                }
                label(
                    raster,
                    &mut out,
                    &line,
                    r.left + 1,
                    r.top + i32::from(layout.baseline),
                    clip.intersection(r).unwrap_or(r),
                    color,
                );
                if let Some(caret) = geometry.caret {
                    rect(&mut out, local(caret), clip, color);
                }
            }
            // Resource art, unknown controls, and unqualified text retain the framebuffer.
            _ => {}
        }
    }
    out
}

pub(super) fn menu(
    update: &Update,
    state: &super::menu::MenuPresenter,
    width: u32,
    height: u32,
    raster: u32,
) -> Vec<Quad> {
    let mut out = Vec::new();
    let Some(id) = state.open else {
        return out;
    };
    let Some(p) = update
        .menu_presentations
        .iter()
        .find(|p| (p.pane.menu.guest_id, p.pane.menu.generation) == id)
    else {
        return out;
    };
    let clip = Rect {
        top: 0,
        left: 0,
        bottom: height as i32,
        right: width as i32,
    };
    let pane = &p.pane;
    let r = Rect::from(pane.bounds);
    rect(
        &mut out,
        Rect {
            top: r.top + 2,
            left: r.left + 2,
            bottom: r.bottom + 2,
            right: r.right + 2,
        },
        clip,
        INK,
    );
    rect(&mut out, r, clip, PAPER);
    border(&mut out, r, clip, INK);
    rect(
        &mut out,
        Rect::from(p.title_bounds),
        clip,
        [0.69, 0.80, 0.96, 1.],
    );
    label(
        raster,
        &mut out,
        &ClassicLine::unicode(&pane.menu.title, 0, 12),
        i32::from(p.title_origin.0),
        i32::from(p.title_origin.1),
        clip,
        INK,
    );
    let mut top = pane.content_top;
    for (item, height) in pane.menu.items.iter().zip(&pane.row_heights) {
        let row = Rect {
            top: i32::from(top),
            left: r.left + 1,
            bottom: i32::from(top + height),
            right: r.right - 1,
        };
        let row_clip = row.intersection(r).unwrap_or(r);
        if item.number == state.selected {
            rect(&mut out, row, clip, [0.69, 0.80, 0.96, 1.]);
        }
        if item.separator {
            rect(
                &mut out,
                Rect {
                    top: row.top + row.height() / 2,
                    left: row.left + 2,
                    bottom: row.top + row.height() / 2 + 1,
                    right: row.right - 2,
                },
                clip,
                [0.6, 0.6, 0.6, 1.],
            );
        } else {
            let (mark, text, command, baseline) = pane.text_anchors(top - r.top as i16, *height);
            let bytes: Vec<_> = item
                .text
                .chars()
                .filter_map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
                .collect();
            let line =
                ClassicLine::styled(&bytes, pane.font.family, pane.font.point_size(), item.style);
            let ink = if item.enabled {
                INK
            } else {
                [0.5, 0.5, 0.5, 1.]
            };
            label(
                raster,
                &mut out,
                &line,
                r.left + i32::from(text),
                r.top + i32::from(baseline),
                row_clip,
                ink,
            );
            if item.checked {
                label(
                    raster,
                    &mut out,
                    &ClassicLine::unicode("✓", 0, 12),
                    r.left + i32::from(mark),
                    r.top + i32::from(baseline),
                    row_clip,
                    ink,
                );
            }
            if let Some(key) = item.key_equivalent {
                label(
                    raster,
                    &mut out,
                    &ClassicLine::unicode(&format!("⌘{key}"), 0, 12),
                    r.left + i32::from(command),
                    r.top + i32::from(baseline),
                    row_clip,
                    ink,
                );
            }
        }
        top = top.saturating_add(*height);
    }
    out
}
