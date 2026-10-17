//! Optional host rasterization with synchronous guest-memory writeback.
use crate::memory::{MacMemoryBus, MemoryBus};

/// A clipped rectangle draw in port coordinates. Bounds use
/// `[left, top, right, bottom]`, with exclusive far edges. Border dimensions
/// are `[horizontal, vertical]`; None fills the rectangle. Transfer entries
/// are `[destination_pass_mask, xor_mask]` for white and black pattern bits.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuCommand {
    /// Optional sorted interval endpoints for each row of `area`.
    pub clip_rows: Option<Vec<Vec<i32>>>,
    pub area: [i32; 4],
    pub rect: [i32; 4],
    pub border: Option<[i32; 2]>,
    pub pattern: [u8; 8],
    pub transfer: [[u32; 2]; 2],
}
impl QuickDrawGpuCommand {
    pub fn valid_clip_rows(&self) -> bool {
        let Some(rows) = &self.clip_rows else {
            return true;
        };
        let Some(height) = self.area[3]
            .checked_sub(self.area[1])
            .and_then(|v| usize::try_from(v).ok())
        else {
            return false;
        };
        let Some(width) = self.area[2]
            .checked_sub(self.area[0])
            .and_then(|v| usize::try_from(v).ok())
        else {
            return false;
        };
        let mut total = 0usize;
        rows.len() == height
            && rows.iter().all(|row| {
                total = total.saturating_add(row.len());
                total <= 1_048_576
                    && (total / 2).saturating_mul(width) <= 64_000_000
                    && row.len() % 2 == 0
                    && row.chunks_exact(2).all(|pair| pair[0] < pair[1])
                    && row
                        .chunks_exact(2)
                        .zip(row.chunks_exact(2).skip(1))
                        .all(|(a, b)| a[1] <= b[0])
            })
    }
    pub fn pixel_count(&self) -> Option<usize> {
        let width = u32::try_from(self.area[2].checked_sub(self.area[0])?).ok()?;
        let height = u32::try_from(self.area[3].checked_sub(self.area[1])?).ok()?;
        if width == 0 || height == 0 {
            return None;
        }
        (width as usize).checked_mul(height as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickDrawGpuPixel {
    pub value: u32,
    /// Uncovered cells cause no guest store, preserving retained glyph detail.
    pub covered: bool,
}

/// An error or malformed result leaves guest pixels untouched for CPU fallback.
pub trait QuickDrawGpuRenderer {
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String>;
}

pub(crate) struct RasterTarget {
    pub base: u32,
    pub row_bytes: u32,
    pub depth: u16,
    pub origin: [i32; 2],
}

/// Only ordinary writable RAM is accelerated. Stores go through the original
/// bus routing, including same-value paint notifications and packed neighbors.
pub(crate) fn try_rectangle(
    bus: &mut MacMemoryBus,
    target: RasterTarget,
    command: QuickDrawGpuCommand,
) -> bool {
    if bus.quickdraw_gpu_renderer.is_none() || !bus.presented_bytes_gates_open() {
        return false;
    }
    let Some(count) = command.pixel_count().filter(|count| *count <= 4_194_304) else {
        return false;
    };
    if !command.valid_clip_rows() || !matches!(target.depth, 1 | 2 | 4 | 8 | 16 | 32) {
        return false;
    }
    let width = (command.area[2] - command.area[0]) as usize;
    let height = (command.area[3] - command.area[1]) as usize;
    let Some(x) = command.area[0]
        .checked_sub(target.origin[0])
        .and_then(|v| u32::try_from(v).ok())
    else {
        return false;
    };
    let Some(y) = command.area[1]
        .checked_sub(target.origin[1])
        .and_then(|v| u32::try_from(v).ok())
    else {
        return false;
    };
    let Some(first_bit) = x.checked_mul(u32::from(target.depth)) else {
        return false;
    };
    let byte_start = first_bit / 8;
    let bit_offset = (first_bit % 8) as usize;
    let Some(bits) = width
        .checked_mul(target.depth as usize)
        .and_then(|n| n.checked_add(bit_offset))
    else {
        return false;
    };
    let row_len = bits.div_ceil(8);
    if u64::from(byte_start) + row_len as u64 > u64::from(target.row_bytes) {
        return false;
    }
    let mut addresses = Vec::with_capacity(height);
    for row in 0..height {
        let Some(address) = y
            .checked_add(row as u32)
            .and_then(|v| v.checked_mul(target.row_bytes))
            .and_then(|v| target.base.checked_add(v))
            .and_then(|v| v.checked_add(byte_start))
        else {
            return false;
        };
        if !bus.presented_bytes_writable(address, row_len) {
            return false;
        }
        addresses.push(address);
    }
    let mask = match target.depth {
        32 => 0x00ff_ffff,
        16 => 0x7fff,
        depth => (1u32 << depth) - 1,
    };
    let mut rows = Vec::with_capacity(height);
    let mut background = Vec::with_capacity(count);
    for &address in &addresses {
        let bytes = bus.read_bytes(address, row_len);
        for column in 0..width {
            let value = if target.depth < 8 {
                let bit = bit_offset + column * target.depth as usize;
                u32::from(bytes[bit / 8] >> (8 - target.depth as usize - bit % 8)) & mask
            } else {
                let lanes = target.depth as usize / 8;
                bytes[column * lanes..(column + 1) * lanes]
                    .iter()
                    .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
                    & mask
            };
            background.push(value);
        }
        rows.push(bytes);
    }
    let result = bus
        .quickdraw_gpu_renderer
        .as_mut()
        .unwrap()
        .render(&command, &background);
    let Ok(pixels) = result else {
        return false;
    };
    if pixels.len() != count {
        return false;
    }
    for (row, (address, mut bytes)) in addresses.into_iter().zip(rows).enumerate() {
        let mut touched = vec![false; row_len];
        for column in 0..width {
            let pixel = pixels[row * width + column];
            if !pixel.covered {
                continue;
            }
            let value = pixel.value & mask;
            if target.depth < 8 {
                let bit = bit_offset + column * target.depth as usize;
                let shift = 8 - target.depth as usize - bit % 8;
                let field = (mask as u8) << shift;
                bytes[bit / 8] = (bytes[bit / 8] & !field) | ((value as u8) << shift);
                touched[bit / 8] = true;
            } else {
                let lanes = target.depth as usize / 8;
                let at = column * lanes;
                bytes[at..at + lanes].copy_from_slice(&value.to_be_bytes()[4 - lanes..]);
                touched[at..at + lanes].fill(true);
            }
        }
        let mut at = 0;
        while at < row_len {
            if !touched[at] {
                at += 1;
                continue;
            }
            let start = at;
            while at < row_len && touched[at] {
                at += 1;
            }
            bus.write_bytes(address + start as u32, &bytes[start..at]);
        }
    }
    true
}

#[cfg(test)]
pub(crate) struct ReferenceRenderer {
    pub calls: std::rc::Rc<std::cell::Cell<usize>>,
    pub failure: u8,
}
#[cfg(test)]
impl QuickDrawGpuRenderer for ReferenceRenderer {
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.calls.set(self.calls.get() + 1);
        if self.failure == 1 {
            return Err("test rejection".into());
        }
        if self.failure == 2 {
            return Ok(Vec::new());
        }
        let width = (command.area[2] - command.area[0]) as usize;
        Ok(background
            .iter()
            .enumerate()
            .map(|(i, &old)| {
                let x = command.area[0] + (i % width) as i32;
                let y = command.area[1] + (i / width) as i32;
                let r = command.rect;
                let in_clip = command.clip_rows.as_ref().is_none_or(|rows| {
                    rows[i / width]
                        .chunks_exact(2)
                        .any(|p| x >= p[0] && x < p[1])
                });
                let covered = in_clip
                    && x >= r[0]
                    && x < r[2]
                    && y >= r[1]
                    && y < r[3]
                    && command.border.is_none_or(|b| {
                        x < r[0] + b[0] || x >= r[2] - b[0] || y < r[1] + b[1] || y >= r[3] - b[1]
                    });
                let black =
                    command.pattern[y.rem_euclid(8) as usize] & (0x80 >> x.rem_euclid(8)) != 0;
                let [pass, xor] = command.transfer[usize::from(black)];
                QuickDrawGpuPixel {
                    value: if covered { (old & pass) ^ xor } else { old },
                    covered,
                }
            })
            .collect())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_rectangle_writeback_preserves_packed_neighbors_padding_and_frame_holes() {
        for depth in [1, 2, 4, 8, 16, 32] {
            let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            bus.quickdraw_gpu_renderer = Some(Box::new(ReferenceRenderer {
                calls: calls.clone(),
                failure: 0,
            }));
            let row_bytes = (13usize * depth as usize).div_ceil(8) + 3;
            let original = vec![0xa5; row_bytes * 6];
            let mut expected = original.clone();
            bus.write_bytes(0x1000, &original);
            let mask = match depth {
                32 => 0x00ff_ffff,
                16 => 0x7fff,
                _ => (1u32 << depth) - 1,
            };
            let command = QuickDrawGpuCommand {
                clip_rows: None,
                area: [-3, 2, 8, 6],
                rect: [-3, 2, 8, 6],
                border: Some([1, 1]),
                pattern: [0x96, 0x69, 0x55, 0xaa, 0xcc, 0x33, 0xf0, 0x0f],
                transfer: [[0, 0], [0, mask]],
            };
            for y in 0..4 {
                for x in 0..11 {
                    if y != 0 && y != 3 && x != 0 && x != 10 {
                        continue;
                    }
                    let world_x = x as i32 - 3;
                    let world_y = y + 2;
                    let black = command.pattern[world_y % 8] & (0x80 >> world_x.rem_euclid(8)) != 0;
                    let value = if black { mask } else { 0 };
                    let bit = ((y + 1) * row_bytes * 8) + (x + 2) * depth as usize;
                    for lane in 0..depth as usize {
                        let target = bit + lane;
                        let field = 0x80 >> (target % 8);
                        if (value >> (depth as usize - 1 - lane)) & 1 != 0 {
                            expected[target / 8] |= field;
                        } else {
                            expected[target / 8] &= !field;
                        }
                    }
                }
            }
            assert!(try_rectangle(
                &mut bus,
                RasterTarget {
                    base: 0x1000,
                    row_bytes: row_bytes as u32,
                    depth,
                    origin: [-5, 1]
                },
                command
            ));
            assert_eq!(
                bus.read_bytes(0x1000, original.len()),
                expected,
                "depth={depth}"
            );
            assert_eq!(calls.get(), 1);
        }
    }
    #[test]
    fn gpu_rectangle_errors_and_missing_backend_leave_guest_bytes_unchanged() {
        for failure in [0, 1, 2, 3] {
            let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
            bus.write_bytes(0x1000, &[0xa5; 16]);
            if failure != 0 {
                bus.quickdraw_gpu_renderer = Some(Box::new(ReferenceRenderer {
                    calls: Default::default(),
                    failure,
                }));
            }
            let mut command = QuickDrawGpuCommand {
                clip_rows: None,
                area: [0, 0, 4, 4],
                rect: [0, 0, 4, 4],
                border: None,
                pattern: [0xff; 8],
                transfer: [[0, 0], [0, 255]],
            };
            if failure == 3 {
                command.clip_rows = Some(vec![]);
            }
            assert!(!try_rectangle(
                &mut bus,
                RasterTarget {
                    base: 0x1000,
                    row_bytes: 4,
                    depth: 8,
                    origin: [0, 0]
                },
                command
            ));
            assert_eq!(bus.read_bytes(0x1000, 16), vec![0xa5; 16]);
        }
    }
}
