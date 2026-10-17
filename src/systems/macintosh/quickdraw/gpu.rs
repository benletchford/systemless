//! Optional host rasterization with synchronous guest-memory writeback.
use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
mod arithmetic;
pub use arithmetic::QuickDrawGpuArithmetic;
mod blit;
pub use blit::{
    QuickDrawGpuBlit, QuickDrawGpuBlitClip, QuickDrawGpuDirectColor, QuickDrawGpuPackedCopy,
};

/// Glyph/bitmap coverage sampled in the same coordinates as the draw.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuMask {
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub coverage: Vec<u8>,
    pub threshold: u8,
    /// Coverage stores nonzero repetition counts; even counts compose the
    /// transfer twice, preserving painted cells whose XOR writes cancel.
    pub repeat_transfer: bool,
}

/// A clipped shape draw in port coordinates. Bounds use
/// `[left, top, right, bottom]`, with exclusive far edges. Border dimensions
/// are `[horizontal, vertical]`; None fills the rectangle. Transfer entries
/// are `[destination_pass_mask, xor_mask]` for white and black pattern bits.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuCommand {
    pub mask: Option<QuickDrawGpuMask>,
    /// Optional sorted interval endpoints for each row of `area`.
    pub clip_rows: Option<Vec<Vec<i32>>>,
    /// Polygon edges [minimum_y, maximum_y, x_at_minimum_y, inverse_slope].
    /// Horizontal edges are omitted; coverage uses the even-odd crossing rule.
    pub polygon_edges: Option<Vec<[f32; 4]>>,
    pub area: [i32; 4],
    pub rect: [i32; 4],
    pub border: Option<[i32; 2]>,
    pub pattern: [u8; 8],
    pub transfer: [[u32; 2]; 2],
}
impl QuickDrawGpuCommand {
    pub fn valid_mask(&self) -> bool {
        self.mask.as_ref().is_none_or(|mask| {
            mask.threshold != 0
                && (!mask.repeat_transfer || mask.threshold == 1)
                && mask.size.iter().all(|v| (1..=32767).contains(v))
                && mask.size[0]
                    .checked_mul(mask.size[1])
                    .is_some_and(|n| n <= 4_194_304 && n as usize == mask.coverage.len())
                // An atlas may straddle the port's i16 clipping bounds.
                && mask.origin.iter().all(|v| (-65535..=65535).contains(v))
        })
    }
    pub fn valid_polygon_edges(&self) -> bool {
        let Some(edges) = &self.polygon_edges else {
            return true;
        };
        self.pixel_count()
            .is_some_and(|count| count.saturating_mul(edges.len()) <= 64_000_000)
            && edges.len() <= 16_384
            && edges.iter().all(|edge| {
                edge.iter().all(|v| v.is_finite())
                    && edge[0] < edge[1]
                    && edge[..3].iter().all(|v| (-32768.0..=32767.0).contains(v))
                    && (-65535.0..=65535.0).contains(&edge[3])
            })
    }
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
    fn render_blit_cells(
        &mut self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        if command.clip.is_some() || command.arithmetic.is_some() {
            return Err("QuickDraw backend does not support clipped bitmap blits".into());
        }
        self.render_blit(command).map(|pixels| {
            pixels
                .into_iter()
                .map(|value| QuickDrawGpuPixel {
                    value,
                    covered: true,
                })
                .collect()
        })
    }
    fn render_packed_blit(&mut self, _command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        Err("QuickDraw backend does not support packed bitmap blits".into())
    }

    fn render_blit(&mut self, _command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        Err("QuickDraw backend does not support bitmap blits".into())
    }

    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String>;
}

/// Shared by serialized CPU adapters; detached memory snapshots have no backend.
#[derive(Clone, Default)]
pub(crate) struct QuickDrawGpuSlot(
    Option<std::rc::Rc<std::cell::RefCell<Box<dyn QuickDrawGpuRenderer>>>>,
);
impl std::fmt::Debug for QuickDrawGpuSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickDrawGpuSlot")
            .field("installed", &self.is_some())
            .finish()
    }
}
impl QuickDrawGpuSlot {
    pub(crate) fn new(renderer: Box<dyn QuickDrawGpuRenderer>) -> Self {
        Self(Some(std::rc::Rc::new(std::cell::RefCell::new(renderer))))
    }
    pub(crate) fn is_some(&self) -> bool {
        self.0.is_some()
    }
    pub(crate) fn is_none(&self) -> bool {
        self.0.is_none()
    }
    pub(crate) fn render_packed_blit(
        &self,
        command: &QuickDrawGpuPackedCopy,
    ) -> Result<Vec<u8>, String> {
        self.0
            .as_ref()
            .ok_or("QuickDraw GPU backend missing")?
            .try_borrow_mut()
            .map_err(|_| "QuickDraw GPU backend is busy")?
            .render_packed_blit(command)
    }
    pub(crate) fn render_blit_cells(
        &self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.0
            .as_ref()
            .ok_or("QuickDraw GPU backend missing")?
            .try_borrow_mut()
            .map_err(|_| "QuickDraw GPU backend is busy")?
            .render_blit_cells(command)
    }
    pub(crate) fn render_blit(&self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        self.0
            .as_ref()
            .ok_or("QuickDraw GPU backend missing")?
            .try_borrow_mut()
            .map_err(|_| "QuickDraw GPU backend is busy")?
            .render_blit(command)
    }
    fn render(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.0
            .as_ref()
            .ok_or("QuickDraw GPU backend missing")?
            .try_borrow_mut()
            .map_err(|_| "QuickDraw GPU backend is busy")?
            .render(command, background)
    }
}

trait RasterMemory {
    fn writable(&mut self, address: u32, len: usize) -> bool;
    fn read(&mut self, address: u32, len: usize) -> Option<Vec<u8>>;
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String>;
    fn store(&mut self, address: u32, bytes: &[u8]);
}
impl RasterMemory for MacMemoryBus {
    fn writable(&mut self, address: u32, len: usize) -> bool {
        self.presented_bytes_writable(address, len)
    }
    fn read(&mut self, address: u32, len: usize) -> Option<Vec<u8>> {
        Some(self.read_bytes(address, len))
    }
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.quickdraw_gpu_renderer.render(command, background)
    }
    fn store(&mut self, address: u32, bytes: &[u8]) {
        self.write_bytes(address, bytes);
    }
}
impl RasterMemory for GuestAddressSpace {
    fn writable(&mut self, address: u32, len: usize) -> bool {
        u32::try_from(len).is_ok_and(|len| self.is_writable_range(address, len))
    }
    fn read(&mut self, address: u32, len: usize) -> Option<Vec<u8>> {
        let mut bytes = vec![0; len];
        self.read_bytes_into(address, &mut bytes)?;
        Some(bytes)
    }
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.quickdraw_gpu_renderer().render(command, background)
    }
    fn store(&mut self, address: u32, bytes: &[u8]) {
        let result = self.write_bytes(address, bytes);
        debug_assert!(result.is_some(), "preflighted GPU store failed");
    }
}

pub(crate) fn try_native_rectangle(
    memory: &mut GuestAddressSpace,
    target: RasterTarget,
    command: QuickDrawGpuCommand,
) -> bool {
    if memory.quickdraw_gpu_renderer().is_none() {
        return false;
    }
    try_raster(memory, target, command, |_| {})
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
    try_rectangle_with_prepare(bus, target, command, |_| {})
}

/// Register presentation metadata after successful rasterization and before
/// stores. The callback must not alter guest bytes or the memory mapping.
pub(crate) fn try_rectangle_with_prepare(
    bus: &mut MacMemoryBus,
    target: RasterTarget,
    command: QuickDrawGpuCommand,
    prepare: impl FnOnce(&mut MacMemoryBus),
) -> bool {
    if bus.quickdraw_gpu_renderer.is_none() || !bus.presented_bytes_gates_open() {
        return false;
    }
    try_raster(bus, target, command, prepare)
}

fn try_raster<M: RasterMemory>(
    bus: &mut M,
    target: RasterTarget,
    command: QuickDrawGpuCommand,
    prepare: impl FnOnce(&mut M),
) -> bool {
    let Some(count) = command.pixel_count().filter(|count| *count <= 4_194_304) else {
        return false;
    };
    if !command.valid_mask()
        || !command.valid_clip_rows()
        || !command.valid_polygon_edges()
        || !matches!(target.depth, 1 | 2 | 4 | 8 | 16 | 32)
    {
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
        if !bus.writable(address, row_len) {
            return false;
        }
        addresses.push(address);
    }
    // Boolean text modes operate on the complete stored field: native RGB555
    // XOR/BIC must retain the destination's unused high bit. Color-only draws
    // already clear unused bits through their explicit transfer masks.
    let mask = match target.depth {
        32 => 0x00ff_ffff,
        depth => (1u32 << depth) - 1,
    };
    let mut rows = Vec::with_capacity(height);
    let mut background = Vec::with_capacity(count);
    for &address in &addresses {
        let Some(bytes) = bus.read(address, row_len) else {
            return false;
        };
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
    let result = bus.render(&command, &background);
    let Ok(pixels) = result else {
        return false;
    };
    if pixels.len() != count {
        return false;
    }
    prepare(bus);
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
            bus.store(address + start as u32, &bytes[start..at]);
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
    fn render_blit_cells(
        &mut self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.calls.set(self.calls.get() + 1);
        match self.failure {
            1 => Err("test clipped blit failure".into()),
            2 => Ok(vec![]),
            _ => command
                .reference_cells()
                .ok_or("invalid clipped blit".into()),
        }
    }
    fn render_packed_blit(&mut self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        self.calls.set(self.calls.get() + 1);
        match self.failure {
            1 => Err("test packed blit failure".into()),
            2 => Ok(vec![]),
            _ => command.reference().ok_or("invalid packed blit".into()),
        }
    }
    fn render_blit(&mut self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        self.calls.set(self.calls.get() + 1);
        match self.failure {
            1 => Err("test blit failure".into()),
            2 => Ok(vec![]),
            _ => command.reference().ok_or("invalid blit".into()),
        }
    }
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
                let in_polygon = command.polygon_edges.as_ref().is_none_or(|edges| {
                    edges
                        .iter()
                        .filter(|e| {
                            (y as f32) >= e[0]
                                && (y as f32) < e[1]
                                && e[2] + (y as f32 - e[0]) * e[3] <= x as f32
                        })
                        .count()
                        % 2
                        != 0
                });
                let in_mask = command.mask.as_ref().is_none_or(|mask| {
                    let tx = x - mask.origin[0];
                    let ty = y - mask.origin[1];
                    tx >= 0
                        && ty >= 0
                        && (tx as u32) < mask.size[0]
                        && (ty as u32) < mask.size[1]
                        && mask.coverage[ty as usize * mask.size[0] as usize + tx as usize]
                            >= mask.threshold
                });
                let covered = in_mask
                    && in_polygon
                    && in_clip
                    && x >= r[0]
                    && x < r[2]
                    && y >= r[1]
                    && y < r[3]
                    && command.border.is_none_or(|b| {
                        x < r[0] + b[0] || x >= r[2] - b[0] || y < r[1] + b[1] || y >= r[3] - b[1]
                    });
                let black =
                    command.pattern[y.rem_euclid(8) as usize] & (0x80 >> x.rem_euclid(8)) != 0;
                let [pass, mut xor] = command.transfer[usize::from(black)];
                if covered
                    && command.mask.as_ref().is_some_and(|mask| {
                        mask.repeat_transfer
                            && mask.coverage[(y - mask.origin[1]) as usize * mask.size[0] as usize
                                + (x - mask.origin[0]) as usize]
                                % 2
                                == 0
                    })
                {
                    xor ^= xor & pass;
                }
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
    use ppc::PpcMemory;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn gpu_native_slot_shares_between_adapters_but_not_detached_snapshots() {
        let calls = Rc::new(Cell::new(0));
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1000, vec![0xa5; 16]);
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        bus.set_quickdraw_gpu_renderer(QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
            calls: calls.clone(),
            failure: 0,
        })));
        bus.attach_guest_address_space(memory.shared_view());
        assert!(memory.quickdraw_gpu_renderer().is_some());
        assert!(memory.clone().quickdraw_gpu_renderer().is_none());
        let command = QuickDrawGpuCommand {
            mask: None,
            clip_rows: Some(vec![vec![], vec![1, 3]]),
            polygon_edges: None,
            area: [0, 0, 4, 2],
            rect: [0, 0, 4, 2],
            border: None,
            pattern: [0xff; 8],
            transfer: [[0, 7]; 2],
        };
        let token = ppc::PpcMemory::instruction_cache_token(&mut memory, 0x1000).unwrap();
        assert!(try_native_rectangle(
            &mut memory,
            RasterTarget {
                base: 0x1000,
                row_bytes: 8,
                depth: 8,
                origin: [0, 0]
            },
            command.clone()
        ));
        assert_eq!(memory.read_u32_be(0x1000), Some(0xa5a5a5a5));
        assert_eq!(memory.read_u32_be(0x1008), Some(0xa50707a5));
        assert_ne!(
            ppc::PpcMemory::instruction_cache_token(&mut memory, 0x1000),
            Some(token)
        );
        assert_eq!(calls.get(), 1);
        bus.set_quickdraw_gpu_renderer(QuickDrawGpuSlot::default());
        assert!(memory.quickdraw_gpu_renderer().is_none());
        assert!(!try_native_rectangle(
            &mut memory,
            RasterTarget {
                base: 0x1000,
                row_bytes: 8,
                depth: 8,
                origin: [0, 0]
            },
            command
        ));
    }

    #[test]
    fn gpu_even_repeated_xor_notifies_guest_code_observers() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x1000, vec![0xa5; 4]);
        memory
            .shared_view()
            .set_quickdraw_gpu_renderer(QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                calls: Rc::new(Cell::new(0)),
                failure: 0,
            })));
        let token = PpcMemory::instruction_cache_token(&mut memory, 0x1000);
        let command = QuickDrawGpuCommand {
            mask: Some(QuickDrawGpuMask {
                origin: [0, 0],
                size: [4, 1],
                coverage: vec![0, 2, 0, 0],
                threshold: 1,
                repeat_transfer: true,
            }),
            clip_rows: None,
            polygon_edges: None,
            area: [0, 0, 4, 1],
            rect: [0, 0, 4, 1],
            border: None,
            pattern: [255; 8],
            transfer: [[255, 103]; 2],
        };
        assert!(try_native_rectangle(
            &mut memory,
            RasterTarget {
                base: 0x1000,
                row_bytes: 4,
                depth: 8,
                origin: [0, 0]
            },
            command
        ));
        assert_eq!(memory.read_u32_be(0x1000), Some(0xa5a5a5a5));
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, 0x1000),
            token
        );
    }

    #[test]
    fn gpu_native_preflight_and_backend_errors_preserve_pixels_and_code_tokens() {
        for failure in [0, 1, 2] {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(0x1000, vec![0xa5; 16]);
            if failure == 0 {
                memory.add_readonly_region(0x1008, vec![0xa5; 4]);
            }
            memory
                .shared_view()
                .set_quickdraw_gpu_renderer(QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                    calls: Rc::new(Cell::new(0)),
                    failure,
                })));
            let token = ppc::PpcMemory::instruction_cache_token(&mut memory, 0x1000);
            let command = QuickDrawGpuCommand {
                mask: None,
                clip_rows: None,
                polygon_edges: None,
                area: [0, 0, 4, 2],
                rect: [0, 0, 4, 2],
                border: None,
                pattern: [0xff; 8],
                transfer: [[0, 7]; 2],
            };
            assert!(!try_native_rectangle(
                &mut memory,
                RasterTarget {
                    base: 0x1000,
                    row_bytes: 8,
                    depth: 8,
                    origin: [0, 0]
                },
                command
            ));
            assert_eq!(memory.read_u32_be(0x1000), Some(0xa5a5a5a5));
            assert_eq!(memory.read_u32_be(0x1008), Some(0xa5a5a5a5));
            assert_eq!(
                ppc::PpcMemory::instruction_cache_token(&mut memory, 0x1000),
                token
            );
        }
    }

    #[test]
    fn gpu_rectangle_writeback_preserves_packed_neighbors_padding_and_frame_holes() {
        for depth in [1, 2, 4, 8, 16, 32] {
            let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            bus.quickdraw_gpu_renderer =
                crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
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
                mask: None,
                polygon_edges: None,
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
        for failure in [0, 1, 2, 3, 4, 5] {
            let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
            bus.write_bytes(0x1000, &[0xa5; 16]);
            if failure != 0 {
                bus.quickdraw_gpu_renderer =
                    crate::quickdraw::gpu::QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                        calls: Default::default(),
                        failure,
                    }));
            }
            let mut command = QuickDrawGpuCommand {
                mask: None,
                polygon_edges: None,
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
            if failure == 4 {
                command.polygon_edges = Some(vec![[0., 1., f32::NAN, 0.]]);
            }
            if failure == 5 {
                command.mask = Some(QuickDrawGpuMask {
                    repeat_transfer: false,
                    origin: [0, 0],
                    size: [2, 2],
                    coverage: vec![255; 3],
                    threshold: 128,
                });
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
