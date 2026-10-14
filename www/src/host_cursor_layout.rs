//! Stable image origin for encountered native cursor shapes. New shapes can
//! require growth; switching among known shapes does not change the hotspot.
//! This mitigates macOS cursor transitions, not transitions to the OS arrow.

use super::NativeSize;

// Store geometry only, not every animated cursor image or encoded PNG.
const MAX_SHAPES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    size: NativeSize,
    // Half-open alpha bounds in source pixels.
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

#[derive(Default)]
pub(super) struct CursorLayouts {
    shapes: Vec<Shape>,
}

impl CursorLayouts {
    /// False leaves an unregistered shape in its original native layout.
    pub(super) fn observe(&mut self, size: NativeSize, pixels: &[u8]) -> bool {
        let mut shape = Shape {
            size,
            left: size.width,
            top: size.height,
            right: 0,
            bottom: 0,
        };
        for y in 0..size.height {
            for x in 0..size.width {
                if pixels[(y * size.width + x) * 4 + 3] != 0 {
                    shape.left = shape.left.min(x);
                    shape.top = shape.top.min(y);
                    shape.right = shape.right.max(x + 1);
                    shape.bottom = shape.bottom.max(y + 1);
                }
            }
        }
        if shape.right == 0 || self.shapes.contains(&shape) {
            return true;
        }
        if self.shapes.len() == MAX_SHAPES {
            return false;
        }
        self.shapes.push(shape);
        true
    }

    pub(super) fn plan(&self, scale: f64) -> Option<NativeSize> {
        if self.shapes.is_empty() || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        // Do not assume a system-arrow hotspot: it is not exposed by CSS and
        // can differ by platform or accessibility settings. Use the minimum
        // space needed by the guest shapes themselves.
        let (mut left, mut top, mut right, mut bottom) = (0, 0, 0, 0);
        for shape in &self.shapes {
            let Some(size) = shape.size.scaled(scale) else {
                continue;
            };
            // Nearest sampling selects floor(dest * source_size / dest_size).
            // Transform bounds with that rule, not by rounding their scale.
            let x0 = (shape.left * size.width).div_ceil(shape.size.width);
            let y0 = (shape.top * size.height).div_ceil(shape.size.height);
            let x1 = (shape.right * size.width).div_ceil(shape.size.width);
            let y1 = (shape.bottom * size.height).div_ceil(shape.size.height);
            if x0 == x1 || y0 == y1 {
                continue;
            }
            left = left.max(size.hot_h.saturating_sub(x0));
            top = top.max(size.hot_v.saturating_sub(y0));
            right = right.max(x1.saturating_sub(size.hot_h + 1));
            bottom = bottom.max(y1.saturating_sub(size.hot_v + 1));
        }
        // Equal canvas sizes also avoid changing the cursor image dimensions
        // when the guest changes shape. Padding may exceed a browser's native
        // limit even when the original image fits: decline it in that case.
        let side = (left + right + 1)
            .max(top + bottom + 1)
            .next_power_of_two()
            .max(32);
        (side <= 128).then_some(NativeSize {
            width: side,
            height: side,
            hot_h: left,
            hot_v: top,
        })
    }
}

pub(super) fn repack(pixels: &[u8], size: NativeSize, layout: NativeSize) -> Option<Vec<u8>> {
    let mut result = vec![0; layout.width * layout.height * 4];
    for y in 0..size.height {
        for x in 0..size.width {
            let from = (y * size.width + x) * 4;
            if pixels[from + 3] == 0 {
                continue;
            }
            let dx = (x + layout.hot_h).checked_sub(size.hot_h)?;
            let dy = (y + layout.hot_v).checked_sub(size.hot_v)?;
            if dx >= layout.width || dy >= layout.height {
                return None;
            }
            let to = (dy * layout.width + dx) * 4;
            result[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_metadata_deduplicates_colours_and_declines_unseen_shapes() {
        let mut layouts = CursorLayouts::default();
        let size = NativeSize {
            width: 16,
            height: 16,
            hot_h: 0,
            hot_v: 0,
        };
        for index in 0..MAX_SHAPES {
            let mut pixels = vec![0; 16 * 16 * 4];
            pixels[index * 4 + 3] = 255;
            assert!(layouts.observe(size, &pixels));
            pixels[index * 4] = 100;
            assert!(layouts.observe(size, &pixels));
        }
        assert_eq!(layouts.shapes.len(), MAX_SHAPES);
        assert!(!layouts.observe(size, &vec![255; 16 * 16 * 4]));
        assert!(layouts.observe(size, &vec![0; 16 * 16 * 4]));
        assert_eq!(layouts.shapes.len(), MAX_SHAPES);
    }

    #[test]
    fn sparse_shapes_with_edge_hotspots_preserve_every_visible_sample() {
        let mut seed = 0x82a19_u32;
        let mut random = |max| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as usize % max
        };
        for _ in 0..40 {
            let mut layouts = CursorLayouts::default();
            let mut family = Vec::new();
            for _ in 0..5 {
                let (width, height) = (1 + random(24), 1 + random(24));
                let size = NativeSize {
                    width,
                    height,
                    hot_h: random(width),
                    hot_v: random(height),
                };
                let mut pixels = vec![0; width * height * 4];
                for pixel in pixels.chunks_exact_mut(4) {
                    if random(4) == 0 {
                        pixel.copy_from_slice(&[
                            random(256) as u8,
                            random(256) as u8,
                            random(256) as u8,
                            1 + random(255) as u8,
                        ]);
                    }
                }
                assert!(layouts.observe(size, &pixels));
                family.push((size, pixels));
            }
            for scale in [0.375, 0.8, 1.0, 1.213, 1.215, 1.75, 2.0] {
                let layout = layouts.plan(scale).unwrap();
                for (source, pixels) in &family {
                    let size = source.scaled(scale).unwrap();
                    let mut scaled = vec![0; size.width * size.height * 4];
                    for y in 0..size.height {
                        for x in 0..size.width {
                            let from = ((y * source.height / size.height) * source.width
                                + x * source.width / size.width)
                                * 4;
                            scaled[(y * size.width + x) * 4..][..4]
                                .copy_from_slice(&pixels[from..from + 4]);
                        }
                    }
                    let packed = repack(&scaled, size, layout)
                        .expect("Plan must enclose all visible samples");
                    let visible = |data: &[u8], s: NativeSize| {
                        data.chunks_exact(4)
                            .enumerate()
                            .filter(|(_, p)| p[3] != 0)
                            .map(|(i, p)| {
                                (
                                    i as isize % s.width as isize - s.hot_h as isize,
                                    i as isize / s.width as isize - s.hot_v as isize,
                                    p.to_vec(),
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(visible(&scaled, size), visible(&packed, layout));
                }
            }
        }
    }
}
