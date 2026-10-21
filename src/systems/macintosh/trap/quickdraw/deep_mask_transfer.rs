//! Shared classic CopyDeepMask and deep/direct-color CopyMask execution.
use super::*;

impl super::super::TrapDispatcher {
    pub(super) fn copy_deep_mask_transfer(
        &mut self,
        bus: &mut MacMemoryBus,
        src_bits_ptr: u32,
        mask_bits_ptr: u32,
        dst_bits_ptr: u32,
        src_rect_ptr: u32,
        mask_rect_ptr: u32,
        dst_rect_ptr: u32,
        mode: i16,
        mask_rgn: u32,
    ) -> Result<()> {
        let src_info = self.resolve_copy_bitmap(bus, src_bits_ptr);
        let mask_info = self.resolve_copy_bitmap(bus, mask_bits_ptr);
        let dst_info = self.resolve_copy_bitmap(bus, dst_bits_ptr);

        let src_top = bus.read_word(src_rect_ptr) as i16;
        let src_left = bus.read_word(src_rect_ptr + 2) as i16;
        let src_bottom = bus.read_word(src_rect_ptr + 4) as i16;
        let src_right = bus.read_word(src_rect_ptr + 6) as i16;
        let mask_top = bus.read_word(mask_rect_ptr) as i16;
        let mask_left = bus.read_word(mask_rect_ptr + 2) as i16;
        let mask_bottom = bus.read_word(mask_rect_ptr + 4) as i16;
        let mask_right = bus.read_word(mask_rect_ptr + 6) as i16;
        let dst_top = bus.read_word(dst_rect_ptr) as i16;
        let dst_left = bus.read_word(dst_rect_ptr + 2) as i16;
        let dst_bottom = bus.read_word(dst_rect_ptr + 4) as i16;
        let dst_right = bus.read_word(dst_rect_ptr + 6) as i16;
        let mode_base = (mode & 0x3F) as u16;

        // CopyDeepMask follows CopyBits' color model for every
        // PixMap depth: indexed pixels are interpreted through their
        // logical ColorTable, while 16/32-bit pixels are blended in
        // RGB space. The destination table is the current GDevice's
        // table for a screen target and the destination PixMap table
        // for an offscreen target (Imaging With QuickDraw,
        // pp. 3-117, 3-120--3-122, 4-31).
        let dst_ctab_handle =
            self.copy_bits_destination_ctab_handle(bus, *self.current_port, &dst_info);
        let screen_destination = dst_info.base == self.screen_mode.0
            && dst_info.row_bytes == self.screen_mode.1
            && dst_info.pixel_size == u32::from(self.screen_mode.4);
        let src_clut = matches!(src_info.pixel_size, 2 | 4 | 8)
            .then(|| self.read_port_clut(bus, src_info.ctab_handle));
        let mask_clut = matches!(mask_info.pixel_size, 2 | 4 | 8)
            .then(|| self.read_port_clut(bus, mask_info.ctab_handle));
        let dst_clut = self.read_indexed_destination_clut(
            bus,
            dst_ctab_handle,
            dst_info.pixel_size,
            screen_destination,
        );

        if src_bottom <= src_top
            || src_right <= src_left
            || mask_bottom <= mask_top
            || mask_right <= mask_left
            || dst_bottom <= dst_top
            || dst_right <= dst_left
        {
            return Ok(());
        }

        let mut clip_t = dst_top.max(dst_info.bounds_top);
        let mut clip_l = dst_left.max(dst_info.bounds_left);
        let mut clip_b = dst_bottom.min(dst_info.bounds_bottom);
        let mut clip_r = dst_right.min(dst_info.bounds_right);

        if !Self::clip_rect_to_region_bbox(
            bus,
            mask_rgn,
            &mut clip_t,
            &mut clip_l,
            &mut clip_b,
            &mut clip_r,
        ) {
            return Ok(());
        }

        if clip_b <= clip_t || clip_r <= clip_l {
            return Ok(());
        }

        if gpu_deep_mask::try_copy(
            bus,
            src_info,
            mask_info,
            dst_info,
            [src_top, src_left, src_bottom, src_right],
            [mask_top, mask_left, mask_bottom, mask_right],
            [dst_top, dst_left, dst_bottom, dst_right],
            [clip_t, clip_l, clip_b, clip_r],
            mode_base,
            mask_rgn,
            src_clut.as_ref(),
            mask_clut.as_ref(),
            dst_clut.as_ref(),
        )
        .is_some()
        {
            return Ok(());
        }

        let mask_membership = Self::build_region_membership_cache(bus, mask_rgn, clip_t, clip_b);

        let src_w = i32::from(src_right) - i32::from(src_left);
        let src_h = i32::from(src_bottom) - i32::from(src_top);
        let mask_w = i32::from(mask_right) - i32::from(mask_left);
        let mask_h = i32::from(mask_bottom) - i32::from(mask_top);
        let dst_w = i32::from(dst_right) - i32::from(dst_left);
        let dst_h = i32::from(dst_bottom) - i32::from(dst_top);
        let no_src_scaling = src_w == dst_w && src_h == dst_h && src_w > 0 && src_h > 0;
        let no_mask_scaling = mask_w == dst_w && mask_h == dst_h && mask_w > 0 && mask_h > 0;

        for dy in clip_t..clip_b {
            let sy = if no_src_scaling {
                i32::from(src_top) + (i32::from(dy) - i32::from(dst_top))
            } else {
                let Some(sy) = Self::scale_coord(src_top, src_bottom, dst_top, dst_bottom, dy)
                else {
                    continue;
                };
                sy
            } as i16;
            let my = if no_mask_scaling {
                i32::from(mask_top) + (i32::from(dy) - i32::from(dst_top))
            } else {
                let Some(my) = Self::scale_coord(mask_top, mask_bottom, dst_top, dst_bottom, dy)
                else {
                    continue;
                };
                my
            } as i16;
            for dx in clip_l..clip_r {
                if !Self::region_contains_point_cached(
                    bus,
                    mask_rgn,
                    mask_membership.as_ref(),
                    dy,
                    dx,
                ) {
                    continue;
                }
                let sx = if no_src_scaling {
                    i32::from(src_left) + (i32::from(dx) - i32::from(dst_left))
                } else {
                    let Some(sx) = Self::scale_coord(src_left, src_right, dst_left, dst_right, dx)
                    else {
                        continue;
                    };
                    sx
                } as i16;
                let mx = if no_mask_scaling {
                    i32::from(mask_left) + (i32::from(dx) - i32::from(dst_left))
                } else {
                    let Some(mx) =
                        Self::scale_coord(mask_left, mask_right, dst_left, dst_right, dx)
                    else {
                        continue;
                    };
                    mx
                } as i16;
                let Some(mask_pixel) = Self::read_bitmap_raw_pixel(bus, &mask_info, my, mx) else {
                    continue;
                };
                let deep_mask = mask_info.pixel_size != 1;
                let mask_rgb = if deep_mask {
                    let Some(mask_rgb) =
                        Self::bitmap_pixel_rgb(bus, &mask_info, mask_pixel, mask_clut.as_ref())
                    else {
                        continue;
                    };
                    Some(mask_rgb)
                } else {
                    // A one-bit mask follows the classic BitMap
                    // convention: black (set) selects the source;
                    // white (clear) preserves the destination.
                    if mask_pixel == 0 {
                        continue;
                    }
                    None
                };
                let Some(src_pixel) = Self::read_bitmap_raw_pixel(bus, &src_info, sy, sx) else {
                    continue;
                };
                let Some(source_rgb) =
                    Self::bitmap_pixel_rgb(bus, &src_info, src_pixel, src_clut.as_ref())
                else {
                    continue;
                };
                let Some(destination_pixel) = Self::read_bitmap_raw_pixel(bus, &dst_info, dy, dx)
                else {
                    continue;
                };
                let Some(destination_rgb) =
                    Self::bitmap_pixel_rgb(bus, &dst_info, destination_pixel, dst_clut.as_ref())
                else {
                    continue;
                };

                // Convert the source into the destination pixel space
                // before applying boolean modes. This preserves
                // palette translation for indexed blits and gives
                // direct-color destinations their documented packed
                // representation. Arithmetic modes are represented by
                // the RGB source and destination values below; the
                // deep-mask weighting itself is always component-wise.
                let Some(source_for_destination) =
                    Self::bitmap_rgb_to_pixel(&dst_info, source_rgb, dst_clut.as_ref())
                else {
                    continue;
                };
                let source_for_blend = if mode_base <= 7 && mode_base != 0 {
                    let mode_pixel = Self::deep_mask_boolean_pixel(
                        source_for_destination,
                        destination_pixel,
                        dst_info.pixel_size,
                        mode_base,
                    );
                    let Some(mode_rgb) =
                        Self::bitmap_pixel_rgb(bus, &dst_info, mode_pixel, dst_clut.as_ref())
                    else {
                        continue;
                    };
                    mode_rgb
                } else {
                    source_rgb
                };
                let output_pixel = if let Some(mask_rgb) = mask_rgb {
                    let blended =
                        Self::blend_deep_mask_rgb(source_for_blend, destination_rgb, mask_rgb);
                    let Some(output) =
                        Self::bitmap_rgb_to_pixel(&dst_info, blended, dst_clut.as_ref())
                    else {
                        continue;
                    };
                    output
                } else if mode_base <= 7 {
                    Self::deep_mask_boolean_pixel(
                        source_for_destination,
                        destination_pixel,
                        dst_info.pixel_size,
                        mode_base,
                    )
                } else {
                    source_for_destination
                };
                let _ = Self::write_bitmap_raw_pixel(bus, &dst_info, dy, dx, output_pixel);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quickdraw::gpu::{QuickDrawGpuSlot, ReferenceRenderer};
    use crate::trap::test_helpers::{setup, TEST_SP};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn classic_copymask_direct_color_and_deep_mask_match_copydeepmask() {
        for mask_depth in [1u16, 16] {
            let mut expected = None;
            for (deep_trap, gpu) in [(true, false), (false, false), (true, true), (false, true)] {
                let (mut dispatcher, mut cpu, mut bus) = setup();
                let calls = Rc::new(Cell::new(0));
                if gpu {
                    bus.quickdraw_gpu_renderer =
                        QuickDrawGpuSlot::new(Box::new(ReferenceRenderer {
                            calls: calls.clone(),
                            failure: 0,
                        }));
                }
                let mut bitmap = |depth: u16, pixel: u16| {
                    let bits = bus.alloc(50);
                    let base = bus.alloc(2);
                    bus.write_long(bits, base);
                    bus.write_word(bits + 4, 0x8002);
                    bus.write_word(bits + 10, 1);
                    bus.write_word(bits + 12, 1);
                    bus.write_word(bits + 30, if depth == 16 { 16 } else { 0 });
                    bus.write_word(bits + 32, depth);
                    bus.write_word(bits + 34, if depth == 16 { 3 } else { 1 });
                    bus.write_word(bits + 36, if depth == 16 { 5 } else { 1 });
                    bus.write_word(base, pixel);
                    (bits, base)
                };
                let (source, _) = bitmap(16, 0x7c00);
                let (mask, _) = bitmap(mask_depth, if mask_depth == 1 { 0x8000 } else { 0x4210 });
                let (destination, destination_base) = bitmap(16, 0x001f);
                let rect = bus.alloc(8);
                bus.write_word(rect + 4, 1);
                bus.write_word(rect + 6, 1);
                let offset = if deep_trap { 6 } else { 0 };
                if deep_trap {
                    bus.write_long(TEST_SP, 0);
                    bus.write_word(TEST_SP + 4, 0);
                }
                for (i, pointer) in [rect, rect, rect, destination, mask, source]
                    .into_iter()
                    .enumerate()
                {
                    bus.write_long(TEST_SP + offset + i as u32 * 4, pointer);
                }
                assert!(dispatcher
                    .dispatch_quickdraw(
                        true,
                        if deep_trap { 0x251 } else { 0x017 },
                        &mut cpu,
                        &mut bus
                    )
                    .unwrap()
                    .is_ok());
                assert_eq!(cpu.read_reg(Register::A7), TEST_SP + offset + 24);
                let pixel = bus.read_word(destination_base);
                if let Some(expected) = expected {
                    assert_eq!(pixel, expected);
                } else {
                    expected = Some(pixel);
                }
                assert_ne!(
                    pixel, 0x001f,
                    "CopyMask must paint direct-color destinations"
                );
                assert_eq!(calls.get(), usize::from(gpu));
            }
        }
    }
}
