//! Retained corner coverage for standard controls. The guest CDEF still owns
//! the logical pixels and hit rectangle (Inside Macintosh I, I-318 and I-405).
use super::{Arc, DetailCell, IndexedColor, Ink, PixelIndexMap, PresentationSlot};

pub(crate) struct RoundedControlDetail(Vec<(u32, DetailCell)>);

impl PresentationSlot {
    /// Capture the background before drawing a standard rounded control. Only
    /// its corner squares need extra detail; integer straight edges stay sharp.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rounded_control_corners(
        &self,
        rect: (i32, i32, i32, i32),
        oval: i32,
        pen: i32,
        depth: u16,
        fill: Option<u32>,
        foreground: u32,
        mut pixel: impl FnMut(i32, i32, u32) -> Option<(u32, u8)>,
    ) -> Option<RoundedControlDetail> {
        let p = self.as_ref()?;
        if !matches!(depth, 8 | 16 | 32) {
            return None;
        }
        let (top, left, bottom, right) = rect;
        let radius = (oval.max(0) as f64 / 2.0)
            .min((right - left).max(0) as f64 / 2.0)
            .min((bottom - top).max(0) as f64 / 2.0);
        if radius <= 0.0 {
            return None;
        }
        let corner = radius.ceil() as i32;
        let inside = |x: f64, y: f64, inset: f64| {
            let (l, t, r, b) = (
                left as f64 + inset,
                top as f64 + inset,
                right as f64 - inset,
                bottom as f64 - inset,
            );
            if x < l || y < t || x >= r || y >= b {
                return false;
            }
            let radius = (radius - inset).max(0.0);
            let cx = x.clamp(l + radius, r - radius);
            let cy = y.clamp(t + radius, b - radius);
            (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
        };
        let mut cells = Vec::new();
        for y in top..bottom {
            if y >= top + corner && y < bottom - corner {
                continue;
            }
            for x in left..right {
                if x >= left + corner && x < right - corner {
                    continue;
                }
                for lane in 0..u32::from(depth / 8) {
                    let shift = (u32::from(depth / 8) - 1 - lane) * 8;
                    let foreground = (foreground >> shift) as u8;
                    let fill = fill.map(|value| (value >> shift) as u8);
                    let Some((address, value)) = pixel(x, y, lane) else {
                        continue;
                    };
                    let old = p.detail(address);
                    let mut cell = DetailCell {
                        value,
                        indices: vec![value; (p.scale * p.scale) as usize],
                        ink: PixelIndexMap::default(),
                    };
                    for sy in 0..p.scale {
                        for sx in 0..p.scale {
                            let i = (sy * p.scale + sx) as usize;
                            // Resolve binary geometry on the retained grid, then let
                            // the common output resolver integrate its coverage. This
                            // avoids accumulating edge alpha on repeated CDEF draws.
                            let px = x as f64 + (sx as f64 + 0.5) / p.scale as f64;
                            let py = y as f64 + (sy as f64 + 0.5) / p.scale as f64;
                            let inside_outer = inside(px, py, 0.0);
                            let stroke = inside_outer && !inside(px, py, pen as f64);
                            let background = if inside_outer && fill.is_some() {
                                IndexedColor::Solid(fill.unwrap())
                            } else {
                                old.as_ref().map_or(IndexedColor::Solid(value), |old| {
                                    old.ink.get(&i).map_or(
                                        IndexedColor::Solid(old.indices[i]),
                                        |ink| {
                                            ink.background.clone().over(ink.foreground, ink.alpha)
                                        },
                                    )
                                })
                            };
                            cell.ink.insert(
                                i,
                                Ink {
                                    foreground,
                                    alpha: if stroke { 255 } else { 0 },
                                    background,
                                },
                            );
                        }
                    }
                    cells.push((address, cell));
                }
            }
        }
        Some(RoundedControlDetail(cells))
    }

    pub(crate) fn finish_rounded_control(
        &self,
        detail: Option<RoundedControlDetail>,
        mut byte: impl FnMut(u32) -> u8,
    ) {
        let Some(detail) = detail else {
            return;
        };
        let Some(mut p) = self.as_mut() else {
            return;
        };
        for (address, mut cell) in detail.0 {
            cell.value = byte(address);
            p.put_detail(address, &Arc::new(cell));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::bus;
    use crate::memory::MemoryBus;

    #[test]
    fn direct_and_indexed_controls_have_identical_corner_geometry() {
        let mut expected = None;
        for depth in [8, 16, 32] {
            let mut bus = bus();
            let lanes = u32::from(depth / 8);
            bus.fill_bytes(0x1000, 64 * lanes, 255);
            bus.enable_outline_presentation(
                (0x1000, 8 * lanes, 8, 8, depth),
                std::array::from_fn(|i| [i as u8; 3]),
                4,
            );
            let slot = bus.presentation.clone();
            let detail = slot.rounded_control_corners(
                (0, 0, 8, 8),
                6,
                1,
                depth,
                Some(u32::MAX),
                0,
                |x, y, lane| {
                    let address = 0x1000 + (y as u32 * 8 + x as u32) * lanes + lane;
                    Some((address, bus.read_byte(address)))
                },
            );
            slot.finish_rounded_control(detail, |address| bus.read_byte(address));
            let pixels = bus.outline_presentation_rgb().unwrap().2;
            if let Some(expected) = &expected {
                assert_eq!(&pixels, expected, "depth={depth}");
            } else {
                expected = Some(pixels);
            }
        }
    }

    #[test]
    fn control_corners_preserve_guest_pixels_background_and_copy_detail() {
        let mut bus = bus();
        let slot = bus.presentation.clone();
        let detail =
            slot.rounded_control_corners((0, 0, 8, 8), 6, 1, 8, Some(200), 0, |x, y, _| {
                let address = 0x1000 + y as u32 * 8 + x as u32;
                Some((address, bus.read_byte(address)))
            });
        // Simulate a logical CDEF; retaining corners must not alter its bytes.
        bus.fill_bytes(0x1000, 64, 200);
        slot.finish_rounded_control(detail, |address| bus.read_byte(address));
        assert_eq!(bus.read_bytes(0x1000, 64), vec![200; 64]);
        let (w, _, before, _) = bus.outline_presentation_rgb().unwrap();
        assert_eq!(&before[0..3], &[255; 3]);
        assert_eq!(
            &before[((8 * w + 8) * 3) as usize..((8 * w + 8) * 3 + 3) as usize],
            &[200; 3]
        );
        let mut output = Vec::new();
        bus.presented_argb_scaled(&[0; 64], &[0; 64], 1, &mut output)
            .unwrap();
        assert!(output.iter().any(|p| (p & 255) > 0 && (p & 255) < 200));
        let detail =
            slot.rounded_control_corners((0, 0, 8, 8), 6, 1, 8, Some(200), 0, |x, y, _| {
                let address = 0x1000 + y as u32 * 8 + x as u32;
                Some((address, bus.read_byte(address)))
            });
        bus.fill_bytes(0x1000, 64, 200);
        slot.finish_rounded_control(detail, |address| bus.read_byte(address));
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, before);
        let saved = bus.save_pixel_bytes(0x1000, 64);
        bus.fill_bytes(0x1000, 64, 0);
        bus.restore_saved_pixels(0x1000, &saved, 0, 64);
        assert_eq!(bus.outline_presentation_rgb().unwrap().2, before);
    }
}
