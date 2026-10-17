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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boolean {
    Intersection,
    Union,
    Difference,
    Xor,
}
impl Boolean {
    fn selected(self, state: u8) -> bool {
        match self {
            Self::Intersection => state == 3,
            Self::Union => state != 0,
            Self::Difference => state == 1,
            Self::Xor => state == 1 || state == 2,
        }
    }
}
/// Combine decoded geometry by vertical bands, then encode inversion changes.
pub fn combine(a: &[u8], b: &[u8], op: Boolean) -> Result<Vec<u8>> {
    use std::collections::BTreeMap;
    let a = decode(a)?;
    let b = decode(b)?;
    let ys: BTreeSet<_> = a.iter().chain(&b).flat_map(|r| [r.top, r.bottom]).collect();
    if ys.len().saturating_mul(a.len() + b.len()) > 2_000_000 {
        return Err(invalid());
    }
    let ys: Vec<_> = ys.into_iter().collect();
    let mut output = Vec::new();
    for band in ys.windows(2) {
        let mut edges: BTreeMap<i32, u8> = BTreeMap::new();
        for (rects, bit) in [(&a, 1), (&b, 2)] {
            for r in rects {
                if r.top <= band[0] && r.bottom > band[0] {
                    *edges.entry(r.left).or_default() ^= bit;
                    *edges.entry(r.right).or_default() ^= bit;
                }
            }
        }
        let mut state = 0;
        let mut left = None;
        for (x, toggle) in edges {
            let before = op.selected(state);
            state ^= toggle;
            let after = op.selected(state);
            if !before && after {
                left = Some(x);
            }
            if before && !after {
                if output.len() >= 65536 {
                    return Err(invalid());
                }
                output.push(Rect::new(
                    left.take().ok_or_else(invalid)?,
                    band[0],
                    x,
                    band[1],
                ));
            }
        }
        if state != 0 {
            return Err(invalid());
        }
    }
    encode(&output)
}
pub(crate) fn encode(rectangles: &[Rect]) -> Result<Vec<u8>> {
    use std::collections::BTreeMap;
    if rectangles
        .iter()
        .flat_map(|r| [r.left, r.top, r.right, r.bottom])
        .any(|v| i16::try_from(v).is_err())
    {
        return Err(invalid());
    }

    let Some(first) = rectangles.first().copied() else {
        return Ok(vec![0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    };
    let bounds = rectangles.iter().fold(first, |r, s| {
        Rect::new(
            r.left.min(s.left),
            r.top.min(s.top),
            r.right.max(s.right),
            r.bottom.max(s.bottom),
        )
    });
    let mut words = vec![
        0,
        bounds.top as i16,
        bounds.left as i16,
        bounds.bottom as i16,
        bounds.right as i16,
    ];
    let area = |r: Rect| i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
    if rectangles.iter().map(|r| area(*r)).sum::<i64>() != area(bounds) {
        let mut rows: BTreeMap<i32, BTreeSet<i32>> = BTreeMap::new();
        for r in rectangles {
            for y in [r.top, r.bottom] {
                for x in [r.left, r.right] {
                    if x == 32767 || y == 32767 {
                        return Err(invalid());
                    }
                    let row = rows.entry(y).or_default();
                    if !row.remove(&x) {
                        row.insert(x);
                    }
                }
            }
        }
        for (y, row) in rows {
            if row.is_empty() {
                continue;
            }
            words.push(y as i16);
            words.extend(row.into_iter().map(|x| x as i16));
            words.push(32767);
            if words.len() > 16382 {
                return Err(invalid());
            }
        }
        words.push(32767);
    }
    words[0] = (words.len() * 2) as i16;
    Ok(words.into_iter().flat_map(i16::to_be_bytes).collect())
}

/// Translate an owned region without changing its inversion storage size. Reject
/// wrapping geometry or a translated coordinate colliding with the row sentinel.
pub(crate) fn translated(bytes: &[u8], dh: i32, dv: i32) -> Result<Vec<u8>> {
    decode(bytes)?;
    let mut output = bytes.to_vec();
    let mut shift = |at: usize, delta: i32, sentinel: bool| -> Result<()> {
        let old = i16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
        let new = i32::from(old) + delta;
        if !(-32768..=32767).contains(&new) || sentinel && new == 32767 {
            return Err(invalid());
        }
        output[at..at + 2].copy_from_slice(&(new as i16).to_be_bytes());
        Ok(())
    };
    for (at, delta) in [(2, dv), (4, dh), (6, dv), (8, dh)] {
        shift(at, delta, false)?;
    }
    let mut at = 10;
    while at < bytes.len() {
        if bytes[at..at + 2] == [0x7f, 0xff] {
            break;
        }
        shift(at, dv, true)?;
        at += 2;
        while bytes[at..at + 2] != [0x7f, 0xff] {
            shift(at, dh, true)?;
            at += 2;
        }
        at += 2;
    }
    decode(&output)?;
    Ok(output)
}
