//! Bounded decoding of classic inversion-point region geometry.
use crate::{graphics::Rect, Error, Result};
use std::collections::BTreeSet;
fn invalid() -> Error {
    Error::Graphics("invalid or excessive region geometry".into())
}
/// Nonoverlapping half-open rectangles, split at each inversion scanline.
pub fn decode(bytes: &[u8]) -> Result<Vec<Rect>> {
    if bytes.len() < 10 || bytes.len() > 32766 || !bytes.len().is_multiple_of(2) {
        return Err(invalid());
    }
    let word = |i| i16::from_be_bytes([bytes[i], bytes[i + 1]]) as i32;
    if word(0) as usize != bytes.len() {
        return Err(invalid());
    }
    let bounds = Rect::new(word(4), word(2), word(8), word(6));
    if bytes.len() == 10 {
        return Ok(
            if bounds.left < bounds.right && bounds.top < bounds.bottom {
                vec![bounds]
            } else {
                vec![]
            },
        );
    }
    let mut active = BTreeSet::new();
    let mut previous_y = None;
    let mut rectangles = Vec::new();
    let mut offset = 10;
    loop {
        if offset + 2 > bytes.len() {
            return Err(invalid());
        }
        let y = word(offset);
        offset += 2;
        if y == 32767 {
            if offset != bytes.len() || !active.is_empty() {
                return Err(invalid());
            }
            break;
        }
        if y < bounds.top || y > bounds.bottom || previous_y.is_some_and(|old| y <= old) {
            return Err(invalid());
        }
        if let Some(top) = previous_y {
            let mut xs = active.iter().copied();
            while let Some(left) = xs.next() {
                let right = xs.next().ok_or_else(invalid)?;
                if rectangles.len() >= 65536 {
                    return Err(invalid());
                }
                rectangles.push(Rect::new(left, top, right, y));
            }
        }
        let mut previous_x = None;
        let mut count = 0;
        loop {
            if offset + 2 > bytes.len() {
                return Err(invalid());
            }
            let x = word(offset);
            offset += 2;
            if x == 32767 {
                break;
            }
            if x < bounds.left || x > bounds.right || previous_x.is_some_and(|old| x <= old) {
                return Err(invalid());
            }
            if !active.remove(&x) {
                active.insert(x);
            }
            previous_x = Some(x);
            count += 1;
        }
        if count % 2 != 0 {
            return Err(invalid());
        }
        previous_y = Some(y);
    }
    let Some(first) = rectangles.first().copied() else {
        return Err(invalid());
    };
    let actual = rectangles.iter().fold(first, |r, s| {
        Rect::new(
            r.left.min(s.left),
            r.top.min(s.top),
            r.right.max(s.right),
            r.bottom.max(s.bottom),
        )
    });
    if actual != bounds {
        return Err(invalid());
    }
    Ok(rectangles)
}
