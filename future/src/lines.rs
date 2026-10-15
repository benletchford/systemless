//! Bounded integer line geometry; coverage and pixel transfer execute on the GPU.
use crate::{
    graphics::{PenLine, Rect},
    Error, Result,
};
pub fn geometry(from: (i16, i16), to: (i16, i16), width: i16, height: i16) -> Result<PenLine> {
    if width < 0 || height < 0 {
        return Err(Error::Graphics("negative QuickDraw pen size".into()));
    }
    let ((sx, sy), (ex, ey)) = if from.1 <= to.1 {
        (from, to)
    } else {
        (to, from)
    };
    let (sx, sy, ex, ey) = (i32::from(sx), i32::from(sy), i32::from(ex), i32::from(ey));
    let dh = (ex - sx).abs();
    let dv = ey - sy;
    let vertical = dv >= dh;
    Ok(PenLine {
        origin_x: sx,
        origin_y: sy,
        minor_delta: if vertical { ex - sx } else { dv },
        steps: if vertical { dv as u32 } else { dh as u32 },
        vertical,
        direction: if ex >= sx { 1 } else { -1 },
        width: width as u16,
        height: height as u16,
    })
}
pub fn bounds(line: PenLine) -> Rect {
    let (x1, y1) = if line.vertical {
        (
            line.origin_x + line.minor_delta,
            line.origin_y + line.steps as i32,
        )
    } else {
        (
            line.origin_x + line.direction * line.steps as i32,
            line.origin_y + line.minor_delta,
        )
    };
    Rect::new(
        line.origin_x.min(x1),
        line.origin_y.min(y1),
        line.origin_x.max(x1) + i32::from(line.width),
        line.origin_y.max(y1) + i32::from(line.height),
    )
}
