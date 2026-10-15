//! Bounded scanline geometry; pixel transfer remains in the wgpu renderer.
//! Uses the fixed-point ellipse recurrence compared against legacy Systemless.
use crate::{graphics::Rect, Error, Result};

fn spans(r: Rect, oval_width: i32, oval_height: i32) -> Vec<(i32, i32)> {
    let width = r.right - r.left;
    let height = r.bottom - r.top;
    if width <= 0 || height <= 0 {
        return vec![];
    }
    let w = oval_width.clamp(0, width);
    let h = oval_height.clamp(0, height);
    if w == 0 || h == 0 {
        return vec![(r.left, r.right); height as usize];
    }
    // Track the squared horizontal radius in 32.32 units. Wide arithmetic
    // avoids the legacy implementation's overflow at extreme aspect ratios.
    let ratio = (i128::from(h) << 16) / i128::from(w);
    let step = ratio * ratio;
    let mut difference = step;
    let mut square = 0i128;
    let mut target = i128::from(2 * h - 1);
    let mut vertical = 1 - h;
    let mut left = i64::from(w) << 15;
    let mut right = (i64::from(w) << 16) - left + 0x8000;
    let mut corners = Vec::with_capacity(h as usize);
    for _ in 0..h {
        while square >> 32 < target {
            left -= 0x8000;
            right += 0x8000;
            square += difference;
            difference += 2 * step;
        }
        while square >> 32 > target {
            left += 0x8000;
            right -= 0x8000;
            difference -= 2 * step;
            square -= difference;
        }
        corners.push((
            (left >> 16).max(0) as i32,
            (right >> 16).min(i64::from(w)) as i32,
        ));
        target -= i128::from(4 * (vertical + 1));
        vertical += 2;
    }
    (0..height)
        .map(|row| {
            let index = if row < h / 2 {
                row
            } else if row < h / 2 + height - h {
                h / 2
            } else {
                row - (height - h)
            };
            let (left, right) = corners[index as usize];
            (r.left + left, r.left + right + width - w)
        })
        .collect()
}

/// Return nonoverlapping coverage rectangles. A frame subtracts the inner
/// rounded rectangle after insetting by the rectangular pen dimensions.
pub fn coverage(
    r: Rect,
    oval_width: i16,
    oval_height: i16,
    pen: Option<(i16, i16)>,
) -> Result<Vec<Rect>> {
    if [r.left, r.top, r.right, r.bottom]
        .iter()
        .any(|v| i16::try_from(*v).is_err())
    {
        return Err(Error::Graphics(
            "rounded rectangle outside guest coordinate range".into(),
        ));
    }
    let (pw, ph) = pen
        .map(|(w, h)| (i32::from(w), i32::from(h)))
        .unwrap_or((0, 0));
    if pw < 0 || ph < 0 {
        return Err(Error::Graphics("negative QuickDraw pen size".into()));
    }
    if pen.is_some() && (pw == 0 || ph == 0) {
        return Ok(vec![]);
    }
    let outer = spans(r, i32::from(oval_width), i32::from(oval_height));
    let inner_rect = Rect::new(r.left + pw, r.top + ph, r.right - pw, r.bottom - ph);
    let inner = if pen.is_some() {
        spans(
            inner_rect,
            i32::from(oval_width) - 2 * pw,
            i32::from(oval_height) - 2 * ph,
        )
    } else {
        vec![]
    };
    let mut result: Vec<Rect> = Vec::new();
    for (index, (left, right)) in outer.into_iter().enumerate() {
        let y = r.top + index as i32;
        let inside = if y >= inner_rect.top {
            inner.get((y - inner_rect.top) as usize).copied()
        } else {
            None
        };
        let pieces = match inside {
            Some((il, ir)) => [
                (left, il.clamp(left, right)),
                (ir.clamp(left, right), right),
            ],
            None => [(left, right), (right, right)],
        };
        for (left, right) in pieces {
            if left >= right {
                continue;
            }
            // At most two spans per row; coalesce vertical runs for the GPU.
            if let Some(previous) = result
                .iter_mut()
                .rev()
                .take(2)
                .find(|old| old.left == left && old.right == right && old.bottom == y)
            {
                previous.bottom = y + 1;
            } else {
                result.push(Rect::new(left, y, right, y + 1));
            }
        }
    }
    Ok(result)
}
