//! Classic deep-mask staging. Guest stores occur only after complete validation.
use super::{CopyBitmapInfo, MacMemoryBus};
use crate::quickdraw::gpu::{QuickDrawGpuBlit, QuickDrawGpuBlitClip, QuickDrawGpuDeepMask};
use crate::trap::TrapDispatcher;

// Conservatively require plain RAM for the complete bitmap. In particular,
// snapshots must not reorder MMIO reads or overlap a sequential CPU transfer.
fn span(bus: &MacMemoryBus, info: CopyBitmapInfo) -> Option<(u64, u64)> {
    if !matches!(info.pixel_size, 1 | 2 | 4 | 8 | 16 | 32) {
        return None;
    }
    let width = u32::try_from(i32::from(info.bounds_right) - i32::from(info.bounds_left)).ok()?;
    let height = u32::try_from(i32::from(info.bounds_bottom) - i32::from(info.bounds_top)).ok()?;
    if width == 0 || height == 0 || width.checked_mul(info.pixel_size)?.div_ceil(8) > info.row_bytes
    {
        return None;
    }
    let len = height.checked_mul(info.row_bytes)? as usize;
    if !bus.presented_bytes_writable(info.base, len) {
        return None;
    }
    Some((u64::from(info.base), u64::from(info.base) + len as u64))
}

fn coordinates(
    rect: [i16; 4],
    destination: [i16; 4],
    clip: [i16; 4],
) -> Option<(Vec<i16>, Vec<i16>)> {
    let [top, left, bottom, right] = rect;
    let [dt, dl, db, dr] = destination;
    let xs = (clip[1]..clip[3])
        .map(|x| i16::try_from(TrapDispatcher::scale_coord(left, right, dl, dr, x)?).ok())
        .collect::<Option<Vec<_>>>()?;
    let ys = (clip[0]..clip[2])
        .map(|y| i16::try_from(TrapDispatcher::scale_coord(top, bottom, dt, db, y)?).ok())
        .collect::<Option<Vec<_>>>()?;
    Some((xs, ys))
}

fn snapshot(bus: &MacMemoryBus, info: CopyBitmapInfo, xs: &[i16], ys: &[i16]) -> Option<Vec<u32>> {
    let mut pixels = Vec::with_capacity(xs.len().checked_mul(ys.len())?);
    for &y in ys {
        for &x in xs {
            pixels.push(TrapDispatcher::read_bitmap_raw_pixel(bus, &info, y, x)?);
        }
    }
    Some(pixels)
}

fn colors(depth: u32, clut: Option<&[[u16; 3]; 256]>) -> Option<Vec<[u16; 3]>> {
    Some(if matches!(depth, 2 | 4 | 8) {
        clut?[..1usize << depth].to_vec()
    } else {
        Vec::new()
    })
}

pub(super) fn try_copy(
    bus: &mut MacMemoryBus,
    source: CopyBitmapInfo,
    mask: CopyBitmapInfo,
    destination: CopyBitmapInfo,
    source_rect: [i16; 4],
    mask_rect: [i16; 4],
    destination_rect: [i16; 4],
    clip: [i16; 4],
    mode: u16,
    region: u32,
    source_clut: Option<&[[u16; 3]; 256]>,
    mask_clut: Option<&[[u16; 3]; 256]>,
    destination_clut: Option<&[[u16; 3]; 256]>,
) -> Option<()> {
    let width = u32::try_from(i32::from(clip[3]) - i32::from(clip[1])).ok()?;
    let height = u32::try_from(i32::from(clip[2]) - i32::from(clip[0])).ok()?;
    let count = width.checked_mul(height)? as usize;
    if count == 0
        || count > 4_194_304
        || !bus.presented_bytes_gates_open()
        || !bus.quickdraw_gpu_renderer.prefers_blit(count, true)
    {
        return None;
    }
    let src_span = span(bus, source)?;
    let mask_span = span(bus, mask)?;
    let dst_span = span(bus, destination)?;
    let overlaps = |a: (u64, u64)| a.0 < dst_span.1 && dst_span.0 < a.1;
    if overlaps(src_span) || overlaps(mask_span) {
        return None;
    }
    let (sx, sy) = coordinates(source_rect, destination_rect, clip)?;
    let (mx, my) = coordinates(mask_rect, destination_rect, clip)?;
    let dx: Vec<_> = (clip[1]..clip[3]).collect();
    let dy: Vec<_> = (clip[0]..clip[2]).collect();
    let pixels = snapshot(bus, source, &sx, &sy)?;
    let mask_pixels = snapshot(bus, mask, &mx, &my)?;
    let background = snapshot(bus, destination, &dx, &dy)?;
    let membership = TrapDispatcher::build_region_membership_cache(bus, region, clip[0], clip[2]);
    let mut coverage = Vec::with_capacity(count);
    for &y in &dy {
        for &x in &dx {
            coverage.push(u8::from(TrapDispatcher::region_contains_point_cached(
                bus,
                region,
                membership.as_ref(),
                y,
                x,
            )));
        }
    }
    // Sampling coordinates follow the classic trap; color conversion, boolean
    // transfer and component blending remain in the GPU kernel.
    let command = QuickDrawGpuBlit {
        width,
        height,
        pixels,
        columns: (0..width).map(|x| [x, x + 1]).collect(),
        rows: (0..height).map(|y| [y, y + 1]).collect(),
        palette: None,
        direct: None,
        arithmetic: None,
        clip: Some(QuickDrawGpuBlitClip {
            coverage: Some(coverage),
            transparent: None,
        }),
        deep_mask: Some(QuickDrawGpuDeepMask {
            classic_colors: true,
            source_depth: source.pixel_size,
            destination_depth: destination.pixel_size,
            mask_depth: mask.pixel_size,
            mode: u32::from(mode),
            source_clut: colors(source.pixel_size, source_clut)?,
            destination_clut: colors(destination.pixel_size, destination_clut)?,
            mask_clut: colors(mask.pixel_size, mask_clut)?,
            mask_width: width,
            mask_height: height,
            mask_pixels,
            mask_columns: (0..width).collect(),
            mask_rows: (0..height).collect(),
            background,
        }),
    };
    if !command.valid() {
        return None;
    }
    let output = bus
        .quickdraw_gpu_renderer
        .render_blit_cells(&command)
        .ok()?;
    let limit = if destination.pixel_size == 32 {
        u32::MAX
    } else {
        (1 << destination.pixel_size) - 1
    };
    if output.len() != count || output.iter().any(|p| p.value > limit) {
        return None;
    }
    for (i, pixel) in output.into_iter().enumerate() {
        if pixel.covered {
            let written = TrapDispatcher::write_bitmap_raw_pixel(
                bus,
                &destination,
                dy[i / width as usize],
                dx[i % width as usize],
                pixel.value,
            );
            debug_assert!(written, "preflighted classic GPU write failed");
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBus;
    use crate::quickdraw::gpu::{QuickDrawGpuSlot, ReferenceRenderer};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn classic_deep_mask_extreme_origins_preserve_identity_without_overflow() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let calls = Rc::new(Cell::new(0));
        bus.quickdraw_gpu_renderer = QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
            calls: calls.clone(),
            failure: 0,
        }));
        let bitmap = |base| CopyBitmapInfo {
            base,
            pixel_size: 8,
            row_bytes: 65535,
            bounds_top: 0,
            bounds_left: i16::MIN,
            bounds_bottom: 1,
            bounds_right: i16::MAX,
            ctab_handle: 0,
        };
        let source = bitmap(0x1000);
        let mask = bitmap(0x12000);
        let destination = bitmap(0x23000);
        bus.write_bytes(source.base + 65533, &[5, 9]);
        bus.write_bytes(destination.base, &vec![0xa5; 65535]);
        let clut = std::array::from_fn(|i| [(i * 257) as u16; 3]);
        let rect = [0, i16::MIN, 1, i16::MAX];
        assert!(try_copy(
            &mut bus,
            source,
            mask,
            destination,
            rect,
            rect,
            rect,
            [0, i16::MAX - 2, 1, i16::MAX],
            0,
            0,
            Some(&clut),
            Some(&clut),
            Some(&clut)
        )
        .is_some());
        assert_eq!(calls.get(), 1);
        assert_eq!(
            bus.read_bytes(destination.base + 65532, 4),
            vec![0xa5, 5, 9, 0]
        );
        assert_eq!(
            TrapDispatcher::scale_coord(i16::MIN, i16::MAX, 0, i16::MAX, i16::MAX - 1),
            Some(32764)
        );
    }

    #[test]
    fn classic_deep_mask_gpu_matches_cpu_colors_modes_and_packed_writes() {
        for depth in [1, 2, 4, 8, 16, 32] {
            for mask_depth in [1, 2, 4, 8, 16, 32] {
                for mode in [0, 1, 2, 3, 4, 5, 6, 7, 36] {
                    let mut bus = MacMemoryBus::new(1024 * 1024);
                    let calls = Rc::new(Cell::new(0));
                    bus.quickdraw_gpu_renderer =
                        QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                            calls: calls.clone(),
                            failure: 0,
                        }));
                    let bitmap = |base, pixel_size| CopyBitmapInfo {
                        base,
                        pixel_size,
                        row_bytes: 32,
                        bounds_top: 0,
                        bounds_left: 0,
                        bounds_bottom: 2,
                        bounds_right: 7,
                        ctab_handle: 0,
                    };
                    let source = bitmap(0x1000, 16);
                    let mask = bitmap(0x2000, mask_depth);
                    let destination = bitmap(0x3000, depth);
                    let clut = std::array::from_fn(|i| {
                        [
                            ((i * 991) % 65536) as u16,
                            ((i * 1709) % 65536) as u16,
                            ((i * 3079) % 65536) as u16,
                        ]
                    });
                    for (base, seed) in
                        [(source.base, 7u8), (mask.base, 23), (destination.base, 41)]
                    {
                        bus.write_bytes(
                            base,
                            &(0..64u8)
                                .map(|i| i.wrapping_mul(seed).wrapping_add(17))
                                .collect::<Vec<_>>(),
                        );
                    }
                    let mut expected = bus.read_bytes(destination.base, 64);
                    // Use the existing classic CPU color/transfer helpers as the
                    // oracle, including full 16-bit palette distance and ties.
                    for y in 0..2i16 {
                        for x in 1..6i16 {
                            let src =
                                TrapDispatcher::read_bitmap_raw_pixel(&bus, &source, y, x).unwrap();
                            let m =
                                TrapDispatcher::read_bitmap_raw_pixel(&bus, &mask, y, x).unwrap();
                            let dst =
                                TrapDispatcher::read_bitmap_raw_pixel(&bus, &destination, y, x)
                                    .unwrap();
                            if mask_depth == 1 && m == 0 {
                                continue;
                            }
                            let rgb = |info: &CopyBitmapInfo, pixel| {
                                TrapDispatcher::bitmap_pixel_rgb(&bus, info, pixel, Some(&clut))
                                    .unwrap()
                            };
                            let source_rgb = rgb(&source, src);
                            let converted = TrapDispatcher::bitmap_rgb_to_pixel(
                                &destination,
                                source_rgb,
                                Some(&clut),
                            )
                            .unwrap();
                            let boolean = TrapDispatcher::deep_mask_boolean_pixel(
                                converted, dst, depth, mode,
                            );
                            let value = if mask_depth == 1 {
                                boolean
                            } else {
                                let s = if (1..=7).contains(&mode) {
                                    rgb(&destination, boolean)
                                } else {
                                    source_rgb
                                };
                                TrapDispatcher::bitmap_rgb_to_pixel(
                                    &destination,
                                    TrapDispatcher::blend_deep_mask_rgb(
                                        s,
                                        rgb(&destination, dst),
                                        rgb(&mask, m),
                                    ),
                                    Some(&clut),
                                )
                                .unwrap()
                            };
                            let bit = y as usize * 32 * 8 + x as usize * depth as usize;
                            for lane in 0..depth as usize {
                                let at = bit + lane;
                                let field = 0x80 >> (at % 8);
                                expected[at / 8] = (expected[at / 8] & !field)
                                    | if value & (1 << (depth as usize - 1 - lane)) != 0 {
                                        field
                                    } else {
                                        0
                                    };
                            }
                        }
                    }
                    assert!(try_copy(
                        &mut bus,
                        source,
                        mask,
                        destination,
                        [0, 0, 2, 7],
                        [0, 0, 2, 7],
                        [0, 0, 2, 7],
                        [0, 1, 2, 6],
                        mode,
                        0,
                        Some(&clut),
                        Some(&clut),
                        Some(&clut)
                    )
                    .is_some());
                    assert_eq!(calls.get(), 1);
                    assert_eq!(
                        bus.read_bytes(destination.base, 64),
                        expected,
                        "depth={depth},mask={mask_depth},mode={mode}"
                    );
                }
            }
        }
    }

    #[test]
    fn classic_deep_mask_declines_aliases_invalid_memory_and_backend_failures_atomically() {
        for failure in 0..5 {
            let mut bus = MacMemoryBus::new(1024 * 1024);
            let calls = Rc::new(Cell::new(0));
            bus.quickdraw_gpu_renderer = QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                calls: calls.clone(),
                failure: if failure < 2 { failure + 1 } else { 0 },
            }));
            let bitmap = |base| CopyBitmapInfo {
                base,
                pixel_size: 16,
                row_bytes: 8,
                bounds_top: 0,
                bounds_left: 0,
                bounds_bottom: 2,
                bounds_right: 4,
                ctab_handle: 0,
            };
            let destination = bitmap(0x3000);
            let mut source = bitmap(if failure == 2 {
                destination.base
            } else {
                0x1000
            });
            let mask = bitmap(if failure == 3 {
                destination.base
            } else {
                0x2000
            });
            if failure == 4 {
                source.base = u32::MAX - 4;
            }
            bus.write_bytes(destination.base, &[0xa5; 16]);
            assert!(try_copy(
                &mut bus,
                source,
                mask,
                destination,
                [0, 0, 2, 4],
                [0, 0, 2, 4],
                [0, 0, 2, 4],
                [0, 0, 2, 4],
                0,
                0,
                None,
                None,
                None
            )
            .is_none());
            assert_eq!(bus.read_bytes(destination.base, 16), vec![0xa5; 16]);
            assert_eq!(calls.get(), usize::from(failure < 2));
        }
    }
}
