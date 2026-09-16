//! Experimental GPU transport. Uniform guest cells cost one word; only cells
//! containing retained outline detail carry their scale × scale samples.
use super::{MacMemoryBus, Presentation};

/// Opaque RGB transport for the experimental desktop GPU presenter.
/// A cell with bit 31 clear stores RGB in bits 0..23; with bit 31 set, bits
/// 0..30 index its row-major scale × scale tile in `detail`. This is host
/// presentation data, never guest memory or a source for guest CopyBits.
#[derive(Default)]
pub struct CompactPresentation {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub cells: Vec<u32>,
    pub detail: Vec<u32>,
}

impl Presentation {
    fn compact_sample(&self, x: usize, y: usize, sx: usize, sy: usize) -> u32 {
        let lanes = self.bytes_per_pixel() as usize;
        let mut rgb = [0u8; 3];
        for lane in 0..lanes {
            let bx = x * lanes + lane;
            let cell = y * self.width as usize + bx;
            let color = if self.text_cells[cell] {
                let scale = self.scale as usize;
                let offset = ((y * scale + sy) * self.width as usize * scale + bx * scale + sx) * 3;
                [
                    self.pixels[offset],
                    self.pixels[offset + 1],
                    self.pixels[offset + 2],
                ]
            } else {
                self.palette_at(bx as u32)[self.guest_values[cell] as u8 as usize]
            };
            for channel in 0..3 {
                rgb[channel] = rgb[channel].saturating_add(color[channel]);
            }
        }
        (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
    }
}

impl MacMemoryBus {
    /// Export retained opaque coverage without expanding ordinary pixels.
    /// Returns false when the inputs cannot be represented by this transport;
    /// callers must then use their existing software presentation path.
    pub fn compact_presentation(
        &self,
        guest: &[u32],
        overlays: &[u32],
        output: &mut CompactPresentation,
    ) -> bool {
        let Some(p) = self.presentation.as_ref() else {
            return false;
        };
        let width = p.logical_width();
        let count = width as usize * p.height as usize;
        if guest.len() != count
            || overlays.len() != count
            || overlays.iter().any(|pixel| pixel >> 24 != 255)
            || count
                .checked_mul((p.scale * p.scale) as usize)
                .is_none_or(|n| n >= 0x80000000)
        {
            return false;
        }
        output.width = width;
        output.height = p.height;
        output.scale = p.scale;
        output.cells.clear();
        output.cells.reserve(count);
        output.detail.clear();
        if p.depth == 8 {
            // SC2K's indexed framebuffer is the common case. Resolve its
            // palette once, and avoid per-cell direct-color lane iteration.
            let palette = p.palette.map(|rgb| {
                (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
            });
            let scale = p.scale as usize;
            let stride = width as usize * scale * 3;
            for (cell, (&before, &after)) in guest.iter().zip(overlays).enumerate() {
                if before != after {
                    output.cells.push(after & 0xffffff);
                } else if !p.text_cells[cell] {
                    output
                        .cells
                        .push(palette[p.guest_values[cell] as u8 as usize]);
                } else {
                    let x = cell % width as usize;
                    let y = cell / width as usize;
                    output.cells.push(0x80000000 | output.detail.len() as u32);
                    for sy in 0..scale {
                        let start = (y * scale + sy) * stride + x * scale * 3;
                        for rgb in p.pixels[start..start + scale * 3].chunks_exact(3) {
                            output.detail.push(
                                (u32::from(rgb[0]) << 16)
                                    | (u32::from(rgb[1]) << 8)
                                    | u32::from(rgb[2]),
                            );
                        }
                    }
                }
            }
            return true;
        }
        let lanes = p.bytes_per_pixel() as usize;
        for y in 0..p.height as usize {
            for x in 0..width as usize {
                let logical = y * width as usize + x;
                let cell = y * p.width as usize + x * lanes;
                if guest[logical] != overlays[logical] {
                    output.cells.push(overlays[logical] & 0xffffff);
                } else if p.text_cells[cell..cell + lanes].iter().any(|&v| v) {
                    output.cells.push(0x80000000 | output.detail.len() as u32);
                    for sy in 0..p.scale as usize {
                        for sx in 0..p.scale as usize {
                            output.detail.push(p.compact_sample(x, y, sx, sy));
                        }
                    }
                } else {
                    output.cells.push(p.compact_sample(x, y, 0, 0));
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;

    #[test]
    fn compact_transport_matches_full_retained_image_after_updates() {
        for depth in [8u16, 16, 32] {
            for scale in 2..=4 {
                let mut bus = super::super::tests::bus();
                let palette = std::array::from_fn(|i| [i as u8, (i * 7) as u8, (255 - i) as u8]);
                bus.enable_outline_presentation(
                    (0x1000, u32::from(depth), 8, 8, depth),
                    palette,
                    scale,
                );
                let guest = vec![0xff123456; 64];
                let mut overlays = guest.clone();
                let mut compact = CompactPresentation::default();
                for step in 0..4 {
                    match step {
                        0 => super::super::tests::paint_detail(&mut bus, 0x1000),
                        1 => overlays[0] = 0xffa71d6b,
                        2 => overlays[0] = guest[0],
                        _ => bus.write_byte(0x1000, 27),
                    }
                    assert!(bus.compact_presentation(&guest, &overlays, &mut compact));
                    let (w, h, expected) = bus.presented_argb(&guest, &overlays).unwrap();
                    let mut actual = Vec::new();
                    for y in 0..h {
                        for x in 0..w {
                            let cell = compact.cells[((y / scale) * 8 + x / scale) as usize];
                            actual.push(
                                0xff000000
                                    | if cell >> 31 == 0 {
                                        cell
                                    } else {
                                        compact.detail[(cell & 0x7fffffff) as usize
                                            + ((y % scale) * scale + x % scale) as usize]
                                    },
                            );
                        }
                    }
                    assert_eq!(actual, expected, "depth={depth} scale={scale} step={step}");
                }
                overlays[0] = 0x7f123456;
                assert!(!bus.compact_presentation(&guest, &overlays, &mut compact));
            }
        }
    }
}
