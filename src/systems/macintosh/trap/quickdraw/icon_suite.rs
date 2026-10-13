//! Resource-backed Icon Utilities family drawing.
use super::*;

impl super::super::TrapDispatcher {
    fn icon_suite_member_bytes(
        &self,
        bus: &mut MacMemoryBus,
        handle: u32,
        minimum: usize,
    ) -> Option<Vec<u8>> {
        if handle == 0 {
            return None;
        }
        let ptr = bus.try_read_long(handle)?;
        let size = self
            .process_handle_size(bus, handle)
            .or_else(|| bus.get_alloc_size(ptr))?;
        if ptr == 0 || size < minimum as u32 || !Self::guest_range_in_ram(bus, ptr, minimum as u32)
        {
            return None;
        }
        Some(bus.read_bytes(ptr, minimum))
    }

    /// Plot a loaded suite without changing or disposing any family member.
    /// QD18 defines size/depth selection and the mandatory monochrome mask.
    pub(crate) fn plot_loaded_icon_suite(
        &mut self,
        bus: &mut MacMemoryBus,
        port: u32,
        rect: Rect,
        alignment: u16,
        transform: u16,
        suite: u32,
    ) -> i16 {
        // Preserve explicit errors for still-unimplemented picture/label/open/
        // offline paths rather than silently treating them as ordinary icons.
        if self.recording_picture.is_some() || alignment & !0x000F != 0 || transform & !0x4001 != 0
        {
            return -50;
        }
        let Some(header) = self.icon_suite_member_bytes(bus, suite, 10) else {
            return -50;
        };
        if &header[..4] != b"ISUT" || header[4..8] != [0, 0, 0, 0] {
            return -50;
        }
        let count = usize::from(u16::from_be_bytes([header[8], header[9]]));
        if count > 9 {
            return -50;
        }
        let Some(bytes) = self.icon_suite_member_bytes(bus, suite, 10 + count * 8) else {
            return -50;
        };
        let members: Vec<([u8; 4], u32)> = bytes[10..]
            .chunks_exact(8)
            .map(|r| {
                (
                    r[..4].try_into().unwrap(),
                    u32::from_be_bytes(r[4..8].try_into().unwrap()),
                )
            })
            .collect();
        let find = |kind: [u8; 4]| members.iter().find(|(t, _)| *t == kind).map(|(_, h)| *h);
        if port == 0 || !Self::guest_range_in_ram(bus, port, 32) {
            return -50;
        }
        let dst = self.resolve_copy_bitmap(bus, self.bitmap_ptr_for_port(bus, port));
        if !matches!(dst.pixel_size, 1 | 2 | 4 | 8 | 16 | 32)
            || dst.row_bytes == 0
            || dst.base == 0
            || dst.base == u32::MAX
        {
            return -50;
        }
        let width = i32::from(rect.right) - i32::from(rect.left);
        let height = i32::from(rect.bottom) - i32::from(rect.top);
        if width <= 0 || height <= 0 {
            return 0;
        }
        // Mini resources are 16 wide by 12 high (Mac OS glossary), despite
        // QD18's shorthand description of the mini size category as 12 pixels.
        let sizes = [
            (32usize, 32usize, *b"ICN#", *b"icl4", *b"icl8"),
            (16, 16, *b"ics#", *b"ics4", *b"ics8"),
            (16, 12, *b"icm#", *b"icm4", *b"icm8"),
        ];
        let preferred = if width > 16 || height > 16 {
            0
        } else if height > 12 {
            1
        } else {
            2
        };
        let order = match preferred {
            0 => [0, 1, 2],
            1 => [1, 0, 2],
            _ => [2, 1, 0],
        };
        let mut selected = None;
        for i in order {
            let (w, h, mono, four, eight) = sizes[i];
            let plane = w.div_ceil(8) * h;
            let Some(mask) =
                find(mono).and_then(|handle| self.icon_suite_member_bytes(bus, handle, plane * 2))
            else {
                continue;
            };
            let mut pixels = mask[..plane].to_vec();
            let mut depth = 1u32;
            for (candidate_depth, kind) in [(8u32, eight), (4, four)] {
                if candidate_depth > dst.pixel_size {
                    continue;
                }
                let length = (w * candidate_depth as usize).div_ceil(8) * h;
                if let Some(data) =
                    find(kind).and_then(|handle| self.icon_suite_member_bytes(bus, handle, length))
                {
                    pixels = data;
                    depth = candidate_depth;
                    break;
                }
            }
            selected = Some((w, h, depth, pixels, mask[plane..].to_vec()));
            break;
        }
        let Some((w, h, depth, pixels, mask)) = selected else {
            return -1000;
        };
        let mask_at = |x: usize, y: usize| mask[y * w.div_ceil(8) + x / 8] & (0x80 >> (x % 8)) != 0;
        let mut ink = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if mask_at(x, y) {
                    ink.0 = ink.0.min(x);
                    ink.1 = ink.1.min(y);
                    ink.2 = ink.2.max(x + 1);
                    ink.3 = ink.3.max(y + 1);
                }
            }
        }
        if ink.0 >= ink.2 || ink.1 >= ink.3 {
            return 0;
        }
        let scaled = (
            ink.0 as i32 * width / w as i32,
            ink.1 as i32 * height / h as i32,
            (ink.2 as i32 * width + w as i32 - 1) / w as i32,
            (ink.3 as i32 * height + h as i32 - 1) / h as i32,
        );
        let shift_x = match alignment & 12 {
            4 => (width - (scaled.2 - scaled.0)) / 2 - scaled.0,
            8 => -scaled.0,
            12 => width - scaled.2,
            _ => 0,
        };
        let shift_y = match alignment & 3 {
            1 => (height - (scaled.3 - scaled.1)) / 2 - scaled.1,
            2 => -scaled.1,
            3 => height - scaled.3,
            _ => 0,
        };
        let source_clut = Self::standard_mac_indexed_clut(depth as u16).unwrap().0;
        let ctab = self.effective_destination_ctab_handle(
            dst.base,
            dst.row_bytes,
            dst.pixel_size,
            dst.ctab_handle,
        );
        let destination_clut = if ctab == 0 {
            *self.device_clut
        } else {
            self.read_port_clut(bus, ctab)
        };
        let vis = bus.read_long(port + 24);
        let clip = bus.read_long(port + 28);
        let vis_rows = Self::build_region_membership_cache(bus, vis, rect.top, rect.bottom);
        let clip_rows = Self::build_region_membership_cache(bus, clip, rect.top, rect.bottom);
        let row = (w * depth as usize).div_ceil(8);
        for y in i32::from(rect.top).max(i32::from(dst.bounds_top))
            ..i32::from(rect.bottom).min(i32::from(dst.bounds_bottom))
        {
            for x in i32::from(rect.left).max(i32::from(dst.bounds_left))
                ..i32::from(rect.right).min(i32::from(dst.bounds_right))
            {
                let local_x = x - i32::from(rect.left) - shift_x;
                let local_y = y - i32::from(rect.top) - shift_y;
                if local_x < 0 || local_y < 0 || local_x >= width || local_y >= height {
                    continue;
                }
                let sx = local_x * w as i32 / width;
                let sy = local_y * h as i32 / height;
                if sx < 0
                    || sy < 0
                    || sx >= w as i32
                    || sy >= h as i32
                    || !mask_at(sx as usize, sy as usize)
                {
                    continue;
                }
                if !Self::region_contains_point_cached(
                    bus,
                    vis,
                    vis_rows.as_ref(),
                    y as i16,
                    x as i16,
                ) || !Self::region_contains_point_cached(
                    bus,
                    clip,
                    clip_rows.as_ref(),
                    y as i16,
                    x as i16,
                ) {
                    continue;
                }
                let bit = sx as usize * depth as usize;
                let value = (pixels[sy as usize * row + bit / 8] >> (8 - depth - (bit % 8) as u32))
                    & (255 >> (8 - depth));
                let mut rgb = source_clut[value as usize];
                if transform & 1 != 0 {
                    for c in &mut rgb {
                        *c = ((*c as u32 + 65535) / 2) as u16;
                    }
                }
                if transform & 0x4000 != 0 {
                    for c in &mut rgb {
                        *c /= 2;
                    }
                }
                if let Some(pixel) = Self::bitmap_rgb_to_pixel(&dst, rgb, Some(&destination_clut)) {
                    Self::write_bitmap_raw_pixel(bus, &dst, y as i16, x as i16, pixel);
                }
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::test_helpers::{setup, MockCpu, TEST_SP};

    fn handle(bus: &mut MacMemoryBus, data: &[u8]) -> u32 {
        let p = bus.alloc(data.len() as u32);
        bus.write_bytes(p, data);
        let h = bus.alloc(4);
        bus.write_long(h, p);
        h
    }
    fn family(bus: &mut MacMemoryBus, parts: &[([u8; 4], Vec<u8>)]) -> u32 {
        let mut data = b"ISUT\0\0\0\0".to_vec();
        data.extend_from_slice(&(parts.len() as u16).to_be_bytes());
        for (kind, bytes) in parts {
            data.extend_from_slice(kind);
            data.extend_from_slice(&handle(bus, bytes).to_be_bytes());
        }
        handle(bus, &data)
    }
    fn destination(
        depth: u16,
    ) -> (
        super::super::super::TrapDispatcher,
        MockCpu,
        MacMemoryBus,
        u32,
        u32,
    ) {
        let (mut d, mut cpu, mut bus) = setup();
        let row = 32 * u32::from(depth) / 8;
        let pixels = bus.alloc(row * 32);
        bus.fill_zeros(pixels, row * 32);
        let pm = bus.alloc(50);
        bus.fill_zeros(pm, 50);
        bus.write_long(pm, pixels);
        bus.write_word(pm + 4, 0x8000 | row as u16);
        bus.write_word(pm + 10, 32);
        bus.write_word(pm + 12, 32);
        bus.write_word(pm + 32, depth);
        let pmh = bus.alloc(4);
        bus.write_long(pmh, pm);
        let port = bus.alloc(128);
        bus.fill_zeros(port, 128);
        bus.write_long(port + 2, pmh);
        bus.write_word(port + 6, 0xC000);
        d.set_current_port_state(&mut bus, &mut cpu, port, None);
        if let Some((clut, _)) =
            super::super::super::TrapDispatcher::standard_mac_indexed_clut(depth)
        {
            d.device_clut.with_mut(|table| *table = clut);
        }
        (d, cpu, bus, pixels, port)
    }
    fn rect(w: i16, h: i16) -> Rect {
        Rect {
            top: 0,
            left: 0,
            bottom: h,
            right: w,
        }
    }

    #[test]
    fn ploticonsuite_selects_large_small_and_sixteen_by_twelve_mini() {
        let (mut d, _, mut bus, pixels, port) = destination(32);
        let suite = family(
            &mut bus,
            &[
                (*b"ICN#", vec![255; 256]),
                (*b"icl8", vec![35; 1024]),
                (*b"ics#", vec![255; 64]),
                (*b"ics8", vec![255; 256]),
                (*b"icm#", vec![255; 48]),
                (*b"icm8", vec![5; 192]),
            ],
        );
        for (w, h, expected) in [(32, 32, 0x00FF0000), (16, 16, 0), (16, 12, 0x00FFFF00)] {
            bus.fill_zeros(pixels, 4096);
            assert_eq!(
                d.plot_loaded_icon_suite(&mut bus, port, rect(w, h), 0, 0, suite),
                0
            );
            assert_eq!(bus.read_long(pixels), expected);
            assert_eq!(
                bus.read_long(pixels + (h as u32 - 1) * 128 + (w as u32 - 1) * 4),
                expected
            );
        }
    }
    #[test]
    fn ploticonsuite_uses_deepest_usable_member_and_direct_destinations() {
        for depth in [1u16, 4, 8, 16, 32] {
            let (mut d, _, mut bus, pixels, port) = destination(depth);
            let suite = family(
                &mut bus,
                &[
                    (*b"ics#", vec![255; 64]),
                    (*b"ics4", vec![0x11; 128]),
                    (*b"ics8", vec![35; 256]),
                ],
            );
            assert_eq!(
                d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 5, 0, suite),
                0
            );
            match depth {
                1 => assert_eq!(bus.read_byte(pixels), 255),
                4 => assert_eq!(bus.read_byte(pixels), 0x11),
                8 => assert_eq!(bus.read_byte(pixels), 35),
                16 => assert_eq!(bus.read_word(pixels), 0x7C00),
                32 => assert_eq!(bus.read_long(pixels), 0x00FF0000),
                _ => unreachable!(),
            }
        }
    }
    #[test]
    fn ploticonsuite_centers_mask_ink_and_observes_port_regions() {
        let (mut d, _, mut bus, pixels, port) = destination(32);
        let mut mono = vec![0; 64];
        for y in 0..16 {
            mono[32 + y * 2] = 0x80;
        }
        let suite = family(&mut bus, &[(*b"ics#", mono), (*b"ics8", vec![35; 256])]);
        let rp = bus.alloc(10);
        let rh = bus.alloc(4);
        bus.write_long(rh, rp);
        bus.write_word(rp, 10);
        bus.write_word(rp + 2, 0);
        bus.write_word(rp + 4, 0);
        bus.write_word(rp + 6, 4);
        bus.write_word(rp + 8, 32);
        bus.write_long(port + 28, rh);
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 5, 0, suite),
            0
        );
        assert_eq!(bus.read_long(pixels), 0);
        assert_eq!(bus.read_long(pixels + 7 * 4), 0x00FF0000);
        assert_eq!(bus.read_long(pixels + 4 * 128 + 7 * 4), 0);
    }
    #[test]
    fn ploticonsuite_reports_missing_mask_and_invalid_suite_without_painting() {
        let (mut d, _, mut bus, pixels, port) = destination(32);
        let no_mask = family(&mut bus, &[(*b"ics8", vec![35; 256])]);
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 0, no_mask),
            -1000
        );
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 0, 0),
            -50
        );
        let invalid = handle(&mut bus, b"BROKEN1234");
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 0, invalid),
            -50
        );
        let valid = family(&mut bus, &[(*b"ics#", vec![255; 64])]);
        bus.write_long(0, bus.read_long(valid));
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 0, 0),
            -50
        );
        assert!(bus.read_bytes(pixels, 4096).iter().all(|b| *b == 0));
    }
    #[test]
    fn ploticonsuite_draws_resource_members_loaded_through_geticonsuite() {
        let (mut d, mut cpu, mut bus, pixels, _) = destination(32);
        d.install_test_resource(&mut bus, *b"ics#", 1000, &vec![255; 64]);
        d.install_test_resource(&mut bus, *b"ics8", 1000, &vec![35; 256]);
        let out = bus.alloc(4);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x0105);
        bus.write_long(TEST_SP, 0x0000FF00);
        bus.write_word(TEST_SP + 4, 1000);
        bus.write_long(TEST_SP + 6, out);
        d.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let suite = bus.read_long(out);
        let r = bus.alloc(8);
        bus.write_word(r + 4, 16);
        bus.write_word(r + 6, 16);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x0306);
        bus.write_long(TEST_SP, suite);
        bus.write_word(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 6, 5);
        bus.write_long(TEST_SP + 8, r);
        d.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pixels), 0x00FF0000);
    }
    #[test]
    fn ploticonsuite_selected_disabled_and_unsupported_transforms_are_explicit() {
        let (mut d, _, mut bus, pixels, port) = destination(32);
        let suite = family(
            &mut bus,
            &[(*b"ics#", vec![255; 64]), (*b"ics8", vec![35; 256])],
        );
        for (transform, pixel) in [(0x4000, 0x007F0000), (1, 0x00FF7F7F)] {
            assert_eq!(
                d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, transform, suite),
                0
            );
            assert_eq!(bus.read_long(pixels), pixel);
        }
        let before = bus.read_bytes(pixels, 4096);
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 2, suite),
            -50
        );
        assert_eq!(
            d.plot_loaded_icon_suite(&mut bus, port, rect(16, 16), 0, 0x100, suite),
            -50
        );
        assert_eq!(bus.read_bytes(pixels, 4096), before);
    }
    #[test]
    fn ploticonsuite_pascal_frame_returns_error_and_preserves_nonresult_registers() {
        let (mut d, mut cpu, mut bus, _, port) = destination(32);
        let suite = family(&mut bus, &[(*b"ics#", vec![255; 64])]);
        let r = bus.alloc(8);
        bus.write_word(r + 4, 16);
        bus.write_word(r + 6, 16);
        let a5 = cpu.read_reg(Register::A5);
        let globals = bus.read_long(a5);
        bus.write_long(globals, port);
        for supplied in [suite, 0] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, 0x0306);
            cpu.write_reg(Register::D1, 0x12345678);
            cpu.write_reg(Register::A0, 0x87654321);
            bus.write_long(TEST_SP, supplied);
            bus.write_word(TEST_SP + 4, 0);
            bus.write_word(TEST_SP + 6, 5);
            bus.write_long(TEST_SP + 8, r);
            d.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
            assert_eq!(
                cpu.read_reg(Register::D0) as i32,
                if supplied == 0 { -50 } else { 0 }
            );
            assert_eq!(
                bus.read_word(TEST_SP + 12) as i16,
                if supplied == 0 { -50 } else { 0 }
            );
            assert_eq!(cpu.read_reg(Register::D1), 0x12345678);
            assert_eq!(cpu.read_reg(Register::A0), 0x87654321);
        }
    }
}
