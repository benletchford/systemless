//! Scanline spans for ovals and round rects.
//!
//! Both generators return one `(left, right)` pair per row of the shape's
//! bounding box, with `right` exclusive: oval spans are relative to the box's
//! left edge, round-rect spans are in the rect's own coordinates.

use crate::trap::types::Rect;

#[cfg(test)]
mod tests;

pub(crate) fn compute_oval_spans(width: i16, height: i16) -> Vec<(i16, i16)> {
    if width <= 0 || height <= 0 {
        return vec![];
    }

    // Bill Atkinson's QuickDraw Oval Algorithm (from references/QuickDraw/DrawArc.a)
    // This is a 100% bit-accurate recreation using a 64-bit fixed-point difference engine.
    let mut spans = vec![(0, 0); height as usize];

    // InitOval logic (DrawArc.a:898)
    let mut oval_y = 1 - height;
    let mut rsq_ysq = 2 * (height as i32) - 1;
    let mut square = 0i64; // 32.32 FIXED

    let width_f = (width as i32) << 16;
    let half_width = width_f >> 1;

    let mut left_edge = half_width;
    let mut right_edge = (width_f - half_width) + 0x8000; // 0.5 bias for rounding

    // ODDNUM = (H/W)^2 as 32.32 Fixed.
    // Uses _FixRatio (16.16) then _LongMul (32.32).
    let ratio = ((height as i64) << 16) / (width as i64);
    let mut odd_num = ratio * ratio; // 16.16 * 16.16 -> 32.32
    let odd_bump = odd_num * 2;

    let half_f = 0x8000i32;

    for y_idx in 0..height {
        // BumpOval logic (DrawArc.a:1003) - Bumps BEFORE finalizing each scanline.
        // PutOval.a (line 143) shows that vertical N uses edges after N+1 bumps.

        // WHILE SQUARE < RSQYSQ DO MAKE OVAL BIGGER
        while (square >> 32) < (rsq_ysq as i64) {
            right_edge += half_f;
            left_edge -= half_f;
            square += odd_num;
            odd_num += odd_bump;
        }
        // WHILE SQUARE > RSQYSQ DO MAKE OVAL SMALLER
        while (square >> 32) > (rsq_ysq as i64) {
            right_edge -= half_f;
            left_edge += half_f;
            odd_num -= odd_bump;
            square -= odd_num;
        }

        let l = (left_edge >> 16) as i16;
        let r = (right_edge >> 16) as i16;
        spans[y_idx as usize] = (l.max(0), r.min(width));

        // Update RSQYSQ for next scanline: RSQYSQ := RSQYSQ - 4 * (OVALY + 1)
        rsq_ysq -= 4 * (oval_y as i32 + 1);
        oval_y += 2;
    }

    spans
}

pub(crate) fn compute_rrect_spans(r: &Rect, ow: i16, oh: i16) -> Vec<(i16, i16)> {
    let width = r.right - r.left;
    let height = r.bottom - r.top;
    if width <= 0 || height <= 0 {
        return vec![];
    }

    let ow = ow.min(width).max(0);
    let oh = oh.min(height).max(0);

    if oh < 1 || ow < 1 {
        return vec![(r.left, r.right); height as usize];
    }

    let corner_spans = compute_oval_spans(ow, oh);
    let mut spans = Vec::new();

    let mid_y = oh / 2;
    let insert_y = height - oh;
    let insert_x = width - ow;

    // Top curves
    for y in 0..mid_y {
        if (y as usize) < corner_spans.len() {
            let (l_rel, r_rel) = corner_spans[y as usize];
            spans.push((r.left + l_rel, r.left + r_rel + insert_x));
        }
    }

    // Stretched middle
    for _ in 0..insert_y {
        if (mid_y as usize) < corner_spans.len() {
            let (l_rel, r_rel) = corner_spans[mid_y as usize];
            spans.push((r.left + l_rel, r.left + r_rel + insert_x));
        } else {
            spans.push((r.left, r.right));
        }
    }

    // Bottom curves
    for y in mid_y..oh {
        if (y as usize) < corner_spans.len() {
            let (l_rel, r_rel) = corner_spans[y as usize];
            spans.push((r.left + l_rel, r.left + r_rel + insert_x));
        }
    }

    spans
}
