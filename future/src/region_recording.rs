//! OpenRgn/CloseRgn, IM QuickDraw Drawing pp. 3-87–3-89.
//! rgnSave holds a private, bounded guest handle of mathematical edges, not pixels.
use crate::{
    graphics::Rect,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
const CAPACITY: usize = 32760;
const HEADER: usize = 16;
type Edge = ((i16, i16), (i16, i16));
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Open,
    Close(u32),
}
#[derive(Default)]
pub struct Manager {
    active: Option<(u32, GuestPtr)>,
}
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn buffer(memory: &Memory, port: GuestPtr) -> Result<Option<(GuestPtr, usize)>> {
    let handle = memory.u32(GuestPtr(port.0 + 96))?;
    if handle == 0 {
        return Ok(None);
    }
    for offset in [92, 100, 104] {
        if memory.u32(GuestPtr(port.0 + offset))? != 0 {
            return Err(bad(
                "combined recording or custom procedures are unsupported",
            ));
        }
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    let header = memory.read(data, HEADER)?;
    let count = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
    if &header[..4] != b"RGN1"
        || header[4..8] != port.0.to_be_bytes()
        || header[12..16] != (CAPACITY as u32).to_be_bytes()
        || count > (CAPACITY - HEADER) / 8
    {
        return Err(bad("foreign or corrupt region recording state"));
    }
    memory.read(data, CAPACITY)?;
    Ok(Some((data, count)))
}
pub(crate) fn active(memory: &Memory, port: GuestPtr) -> Result<bool> {
    Ok(buffer(memory, port)?.is_some())
}
fn append(memory: &mut Memory, port: GuestPtr, edges: &[Edge]) -> Result<bool> {
    let Some((data, count)) = buffer(memory, port)? else {
        return Ok(false);
    };
    let mut bytes = Vec::new();
    for &(a, b) in edges {
        if a.1 == b.1 {
            continue;
        }
        for coordinate in [a.0, a.1, b.0, b.1] {
            bytes.extend(coordinate.to_be_bytes());
        }
    }
    let next = count + bytes.len() / 8;
    if next > (CAPACITY - HEADER) / 8 {
        return Err(bad("region recording edge limit exceeded"));
    }
    let destination = GuestPtr(data.0 + HEADER as u32 + count as u32 * 8);
    memory.validate_write(destination, bytes.len())?;
    memory.validate_write(GuestPtr(data.0 + 8), 4)?;
    memory.write(destination, &bytes)?;
    memory.write(GuestPtr(data.0 + 8), &(next as u32).to_be_bytes())?;
    Ok(true)
}
pub(crate) fn line(
    memory: &mut Memory,
    port: GuestPtr,
    from: (i16, i16),
    to: (i16, i16),
) -> Result<bool> {
    append(memory, port, &[(from, to)])
}
pub(crate) fn polygon(memory: &mut Memory, port: GuestPtr, points: &[(i16, i16)]) -> Result<bool> {
    let edges: Vec<_> = points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
        .collect();
    append(memory, port, &edges)
}
pub(crate) fn shape(memory: &mut Memory, port: GuestPtr, spans: &[Rect]) -> Result<bool> {
    let mut edges = Vec::new();
    for r in spans
        .iter()
        .filter(|r| r.left < r.right && r.top < r.bottom)
    {
        let word =
            |value| i16::try_from(value).map_err(|_| bad("region recording coordinate range"));
        let (l, t, r, b) = (word(r.left)?, word(r.top)?, word(r.right)?, word(r.bottom)?);
        edges.extend([((l, t), (l, b)), ((r, t), (r, b))]);
    }
    append(memory, port, &edges)
}
fn geometry(edges: &[Edge]) -> Result<Vec<u8>> {
    if edges.is_empty() {
        return crate::region_geometry::encode(&[]);
    }
    let top = edges.iter().map(|e| e.0 .1.min(e.1 .1)).min().unwrap();
    let bottom = edges.iter().map(|e| e.0 .1.max(e.1 .1)).max().unwrap();
    if i64::from(i32::from(bottom) - i32::from(top)) * edges.len() as i64 > 16_000_000 {
        return Err(bad("region recording scan conversion work limit exceeded"));
    }
    let mut rectangles: Vec<Rect> = Vec::new();
    let mut previous: std::collections::BTreeMap<(i32, i32), usize> =
        std::collections::BTreeMap::new();
    for y in i32::from(top)..i32::from(bottom) {
        let mut crossings = Vec::new();
        for &(mut a, mut b) in edges {
            if a.1 > b.1 {
                std::mem::swap(&mut a, &mut b);
            }
            if y < i32::from(a.1) || y >= i32::from(b.1) {
                continue;
            }
            let dy = i64::from(b.1) - i64::from(a.1);
            let numerator = i64::from(a.0) * dy
                + (i64::from(y) - i64::from(a.1)) * (i64::from(b.0) - i64::from(a.0));
            crossings.push((-(-numerator).div_euclid(dy)) as i32);
        }
        crossings.sort_unstable();
        if !crossings.len().is_multiple_of(2) {
            return Err(bad("region outline is not closed"));
        }
        let mut current = std::collections::BTreeMap::new();
        for pair in crossings.chunks_exact(2) {
            if pair[0] < pair[1] {
                let key = (pair[0], pair[1]);
                let index = if let Some(&index) = previous.get(&key) {
                    rectangles[index].bottom = y + 1;
                    index
                } else {
                    if rectangles.len() >= 65536 {
                        return Err(bad("region recording span limit exceeded"));
                    }
                    rectangles.push(Rect::new(pair[0], y, pair[1], y + 1));
                    rectangles.len() - 1
                };
                current.insert(key, index);
            }
        }
        previous = current;
    }
    crate::region_geometry::encode(&rectangles)
}
impl Manager {
    pub(crate) fn is_open(&self) -> bool {
        self.active.is_some()
    }
    pub fn call(&mut self, memory: &mut Memory, process: &mut Process, op: Op) -> Result<()> {
        let ports = process
            .ports
            .ok_or_else(|| bad("region recording requires InitGraf"))?;
        let port = ports.current(memory)?;
        memory.validate_write(GuestPtr(port.0 + 66), 2)?;
        memory.validate_write(GuestPtr(port.0 + 96), 4)?;
        match op {
            Op::Open => {
                if self.active.is_some() || process.polygons.is_open() {
                    return Err(bad("nested region/polygon recording is unsupported"));
                }
                for at in [92, 96, 100, 104] {
                    if memory.u32(GuestPtr(port.0 + at))? != 0 {
                        return Err(bad(
                            "combined recording or custom procedures are unsupported",
                        ));
                    }
                }
                let visibility = (memory.u16(GuestPtr(port.0 + 66))? as i16).wrapping_sub(1);
                let handle = process.new_handle(memory, CAPACITY as u32, false)?;
                if handle == 0 {
                    return Ok(());
                }
                let data = GuestPtr(memory.u32(GuestPtr(handle))?);
                let mut header = b"RGN1".to_vec();
                header.extend(port.0.to_be_bytes());
                header.extend(0u32.to_be_bytes());
                header.extend((CAPACITY as u32).to_be_bytes());
                memory.write(data, &header)?;
                memory.write(GuestPtr(port.0 + 96), &handle.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 66), &visibility.to_be_bytes())?;
                self.active = Some((handle, port));
            }
            Op::Close(destination) => {
                let (handle, original) =
                    self.active.ok_or_else(|| bad("CloseRgn without OpenRgn"))?;
                if port != original
                    || memory.u32(GuestPtr(port.0 + 96))? != handle
                    || destination == handle
                {
                    return Err(bad("region recording port or handle changed"));
                }
                if process.handle_size(memory, handle)? != CAPACITY as u32 {
                    return Err(bad("region recording handle resized"));
                }
                crate::regions::read(memory, process, destination)?;
                let (data, count) =
                    buffer(memory, port)?.ok_or_else(|| bad("missing region recording"))?;
                let bytes = memory.read(GuestPtr(data.0 + HEADER as u32), count * 8)?;
                let edges: Vec<Edge> = bytes
                    .chunks_exact(8)
                    .map(|edge| {
                        let word = |i| i16::from_be_bytes(edge[i..i + 2].try_into().unwrap());
                        ((word(0), word(2)), (word(4), word(6)))
                    })
                    .collect();
                let result = geometry(&edges)?;
                let visibility = (memory.u16(GuestPtr(port.0 + 66))? as i16).wrapping_add(1);
                process.replace_handles(memory, vec![(destination, result)])?;
                if process.mem_error != 0 {
                    return Ok(());
                }
                process.dispose_handle(memory, handle)?;
                memory.write(GuestPtr(port.0 + 96), &0u32.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 66), &visibility.to_be_bytes())?;
                self.active = None;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn contains(bytes: &[u8], x: i32, y: i32) -> bool {
        crate::region_geometry::decode(bytes)
            .unwrap()
            .iter()
            .any(|r| r.left <= x && x < r.right && r.top <= y && y < r.bottom)
    }
    #[test]
    fn exact_edges_preserve_negative_coordinates_diagonals_holes_and_disjoint_loops() {
        let edges = [((-4, -3), (6, -3)), ((6, -3), (-4, 7)), ((-4, 7), (-4, -3))];
        let bytes = geometry(&edges).unwrap();
        for y in -5..10 {
            for x in -6..9 {
                assert_eq!(
                    contains(&bytes, x, y),
                    x >= -4 && y >= -3 && x + y < 3,
                    "{x},{y}"
                );
            }
        }
        let mut edges = vec![
            ((0, 0), (0, 10)),
            ((10, 0), (10, 10)),
            ((2, 2), (2, 8)),
            ((8, 2), (8, 8)),
            ((12, 1), (12, 4)),
            ((15, 1), (15, 4)),
        ];
        let bytes = geometry(&edges).unwrap();
        assert!(contains(&bytes, 1, 1));
        assert!(!contains(&bytes, 3, 3));
        assert!(contains(&bytes, 13, 2));
        assert!(!contains(&bytes, 11, 2));
        edges.extend([((0, 0), (0, 10)), ((10, 0), (10, 10))]);
        let bytes = geometry(&edges).unwrap();
        assert!(!contains(&bytes, 1, 1));
        assert!(contains(&bytes, 3, 3));
        assert!(geometry(&[((0, 0), (0, 5))]).is_err());
        let tall = geometry(&[((-1, -32768), (-1, 32767)), ((1, -32768), (1, 32767))]).unwrap();
        assert_eq!(tall, [0, 10, 128, 0, 255, 255, 127, 255, 0, 1]);
    }
}
