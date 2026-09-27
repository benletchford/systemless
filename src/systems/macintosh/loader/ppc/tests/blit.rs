use super::*;

    #[test]
    fn hle_import_runner_copybits_overlapping_rows_preserve_padding() {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let pixels = PPC_HEAP_BASE + 0x10000;
        let src_pixmap = pixels + 0x100;
        let dst_pixmap = pixels + 0x200;
        let rect = pixels + 0x300;
        loaded.memory.add_region(pixels, vec![0; 0x400]);
        loaded
            .memory
            .write_bytes(
                pixels,
                &[1, 2, 3, 90, 4, 5, 6, 91, 7, 8, 9, 92, 10, 11, 12, 93],
            )
            .unwrap();
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, pixels, 4, 0, 0, 3, 4, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, pixels + 4, 4, 0, 0, 3, 4, 8).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 3, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 16];
        loaded.memory.read_bytes_into(pixels, &mut actual).unwrap();
        assert_eq!(actual, [1, 2, 3, 90, 1, 2, 3, 91, 4, 5, 6, 92, 7, 8, 9, 93]);
    }

    #[test]
    fn hle_import_runner_copybits_snapshots_distinct_guest_aliases() {
        const SOURCE: u32 = 0x0900_0000;
        const DESTINATION: u32 = 0x0A00_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes((0..16).collect());
        // SAFETY: the import accesses both aliases serially through one
        // operation, and no borrowed byte slice survives a memory call.
        unsafe {
            loaded.memory.add_shared_region(SOURCE, backing.clone());
            loaded.memory.add_shared_region(DESTINATION, backing);
        }
        let records = PPC_HEAP_BASE + 0x16000;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rect = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 4, 0, 0, 3, 4, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            DESTINATION + 4,
            4,
            0,
            0,
            3,
            4,
            8,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 3, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0x40; // ditherCopy flag, srcCopy base mode
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 16];
        loaded.memory.read_bytes_into(SOURCE, &mut actual).unwrap();
        assert_eq!(actual, [0, 1, 2, 3, 0, 1, 2, 7, 4, 5, 6, 11, 8, 9, 10, 15]);
    }

    #[test]
    fn hle_import_runner_copybits_does_not_fallback_after_later_row_write_failure() {
        const SOURCE: u32 = 0x0900_0000;
        const DESTINATION: u32 = 0x0A00_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        loaded
            .memory
            .add_region(SOURCE, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        loaded.memory.add_region(DESTINATION, vec![0xAA; 8]);
        loaded
            .memory
            .add_readonly_region(DESTINATION + 6, vec![0xAA]);
        let records = PPC_HEAP_BASE + 0x16000;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rect = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 4, 0, 0, 2, 4, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            DESTINATION,
            4,
            0,
            0,
            2,
            4,
            8,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 2, 4).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 8];
        loaded
            .memory
            .read_bytes_into(DESTINATION, &mut actual)
            .unwrap();
        assert_eq!(
            actual,
            [1, 2, 3, 4, 0xAA, 0xAA, 0xAA, 0xAA],
            "a fallback would partially overwrite the refused row"
        );
    }

    #[test]
    fn hle_import_runner_copybits_does_not_fallback_after_source_read_failure() {
        const SOURCE: u32 = 0x0900_0000;
        const DESTINATION: u32 = 0x0A00_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        loaded.memory.add_region(SOURCE, vec![1, 2, 3, 4]);
        loaded.memory.add_region(DESTINATION, vec![0xAA; 8]);
        let records = PPC_HEAP_BASE + 0x16000;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rect = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 4, 0, 0, 2, 4, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            DESTINATION,
            4,
            0,
            0,
            2,
            4,
            8,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 2, 4).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 8];
        loaded
            .memory
            .read_bytes_into(DESTINATION, &mut actual)
            .unwrap();
        assert_eq!(
            actual, [0xAA; 8],
            "a fallback would write after the failed source snapshot"
        );
    }

    #[test]
    fn hle_import_runner_copybits_copies_16bpp_pixmap_rect() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let src_pixels = PPC_HEAP_BASE + 0x10000;
        let src_pixmap = PPC_HEAP_BASE + 0x11000;
        let src_pixmap_handle = PPC_HEAP_BASE + 0x11100;
        let rects = PPC_HEAP_BASE + 0x11200;
        let dst_pixels = PPC_HEAP_BASE + 0x11300;
        let dst_pixmap = PPC_HEAP_BASE + 0x11400;
        let src_width = 4u32;
        let src_height = 4u32;
        let src_row_bytes = 8u32;
        loaded
            .memory
            .add_region(src_pixels, vec![0; (src_row_bytes * src_height) as usize]);
        loaded
            .memory
            .add_region(src_pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
        loaded.memory.add_region(src_pixmap_handle, vec![0; 4]);
        loaded.memory.add_region(rects, vec![0; 16]);
        loaded.memory.add_region(dst_pixels, vec![0; 50]);
        loaded
            .memory
            .add_region(dst_pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
        loaded
            .memory
            .write_u32_be(src_pixmap_handle, src_pixmap)
            .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            src_pixmap,
            src_pixels,
            src_row_bytes,
            0,
            0,
            src_height as i16,
            src_width as i16,
            16,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            10,
            19,
            9,
            24,
            14,
            16,
        )
        .unwrap();
        for y in 0..src_height {
            for x in 0..src_width {
                let addr = src_pixels + y * src_row_bytes + x * 2;
                let pixel = 0x1000 | ((y as u16) << 4) | x as u16;
                loaded.memory.write_u16_be(addr, pixel).unwrap();
            }
        }
        ppc_write_rect(&mut loaded.memory, rects, 1, 1, 3, 4).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 20, 10, 22, 13).unwrap();

        let dst_addr = |x: u32, y: u32| dst_pixels + (y - 19) * 10 + (x - 9) * 2;
        loaded.memory.write_u16_be(dst_addr(9, 20), 0x0bad).unwrap();
        loaded.cpu.gpr[3] = src_pixmap_handle;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(dst_addr(10, 20)), Some(0x1011));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(11, 20)), Some(0x1012));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(12, 20)), Some(0x1013));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(10, 21)), Some(0x1021));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(12, 21)), Some(0x1023));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(9, 20)), Some(0x0bad));
        assert_eq!(loaded.memory.read_u16_be(dst_addr(13, 20)), Some(0));
    }

    #[test]
    fn hle_import_runner_copybits_blends_direct_color_with_op_color_weights() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x11600;
        let src_pixels = scratch;
        let dst_pixels = scratch + 4;
        let src_pixmap = scratch + 8;
        let dst_pixmap = scratch + 64;
        let rect = scratch + 120;
        loaded.memory.add_region(scratch, vec![0; 128]);
        ppc_write_pixmap(
            &mut loaded.memory,
            src_pixmap,
            src_pixels,
            2,
            0,
            0,
            1,
            1,
            16,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            2,
            0,
            0,
            1,
            1,
            16,
        )
        .unwrap();
        loaded.memory.write_u16_be(src_pixels, 0x7c00).unwrap();
        loaded.memory.write_u16_be(dst_pixels, 0x001f).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        let op_color = PpcRgbColor {
            red: 0x8000,
            green: 0x8000,
            blue: 0x8000,
        };
        loaded.quickdraw_op_colors.set_quickdraw_op_color(
            PPC_MAIN_GWORLD,
            (op_color.red, op_color.green, op_color.blue),
        );
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 32;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(dst_pixels), Some(0x3c0f));
    }

    #[test]
    fn hle_import_runner_copybits_scales_transparent_bitmap_into_direct_color() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x11700;
        let src_pixels = scratch;
        let src_bitmap = scratch + 0x10;
        let dst_pixels = scratch + 0x30;
        let dst_pixmap = scratch + 0x60;
        let rects = scratch + 0xa0;
        loaded.memory.add_region(scratch, vec![0; 0xc0]);
        loaded.memory.write_u8(src_pixels, 0b1010_0000).unwrap();
        loaded.memory.write_u32_be(src_bitmap, src_pixels).unwrap();
        loaded.memory.write_u16_be(src_bitmap + 4, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, src_bitmap + 6, 0, 0, 1, 4).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            16,
            0,
            0,
            2,
            8,
            16,
        )
        .unwrap();
        for offset in (0..32).step_by(2) {
            loaded
                .memory
                .write_u16_be(dst_pixels + offset, 0x1234)
                .unwrap();
        }
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 4).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 2, 8).unwrap();
        loaded.quickdraw_fore_color = PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        };
        loaded.quickdraw_back_color = PPC_RGB_WHITE;
        loaded.cpu.gpr[3] = src_bitmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 36;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let red = ppc_rgb_color_to_rgb555(loaded.quickdraw_fore_color);
        for y in 0..2 {
            let row = dst_pixels + y * 16;
            assert_eq!(loaded.memory.read_u16_be(row), Some(red));
            assert_eq!(loaded.memory.read_u16_be(row + 2), Some(red));
            assert_eq!(loaded.memory.read_u16_be(row + 4), Some(0x1234));
            assert_eq!(loaded.memory.read_u16_be(row + 6), Some(0x1234));
            assert_eq!(loaded.memory.read_u16_be(row + 8), Some(red));
            assert_eq!(loaded.memory.read_u16_be(row + 10), Some(red));
            assert_eq!(loaded.memory.read_u16_be(row + 12), Some(0x1234));
            assert_eq!(loaded.memory.read_u16_be(row + 14), Some(0x1234));
        }
    }

    #[test]
    fn hle_import_runner_copybits_transparent_direct_color_preserves_background_pixels() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x117c0;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        loaded.memory.add_region(scratch, vec![0; 0xb0]);
        ppc_write_pixmap(
            &mut loaded.memory,
            src_pixmap,
            src_pixels,
            4,
            0,
            0,
            1,
            2,
            16,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            4,
            0,
            0,
            1,
            2,
            16,
        )
        .unwrap();
        let red = ppc_rgb_color_to_rgb555(PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        });
        loaded
            .memory
            .write_u16_be(src_pixels, ppc_rgb_color_to_rgb555(PPC_RGB_WHITE))
            .unwrap();
        loaded.memory.write_u16_be(src_pixels + 2, red).unwrap();
        loaded.memory.write_u16_be(dst_pixels, 0x001f).unwrap();
        loaded.memory.write_u16_be(dst_pixels + 2, 0x001f).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 2).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 36;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(dst_pixels), Some(0x001f));
        assert_eq!(loaded.memory.read_u16_be(dst_pixels + 2), Some(red));
    }

    #[test]
    fn hle_import_runner_copybits_clips_to_complex_mask_region() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let mut last_mem_error = loaded.last_mem_error();
        let scratch = PPC_HEAP_BASE + 0x11800;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rect = scratch + 0xc0;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 4, 4, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 0, 0, 4, 4, 8).unwrap();
        loaded
            .memory
            .write_bytes(
                src_pixels,
                &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            )
            .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 4, 4).unwrap();

        let mask_rgn = ppc_new_rgn(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
        );
        let mask_storage = ppc_region_storage_from_rows(
            0,
            &[vec![1, 3], vec![0, 1, 3, 4], vec![1, 3], Vec::new()],
        )
        .unwrap();
        assert_eq!(
            ppc_write_region_storage(
                None,
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                &mut last_mem_error,
                test_handles!(loaded),
                mask_rgn,
                &mask_storage,
            ),
            PPC_NO_ERR
        );

        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = mask_rgn;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 16];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0, 2, 3, 0, 5, 0, 0, 8, 0, 10, 11, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn hle_import_runner_copybits_copies_8bpp_pixmap_rect() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let src_pixels = PPC_HEAP_BASE + 0x12000;
        let dst_pixels = PPC_HEAP_BASE + 0x12100;
        let src_pixmap = PPC_HEAP_BASE + 0x12200;
        let dst_pixmap = PPC_HEAP_BASE + 0x12300;
        let rect = PPC_HEAP_BASE + 0x12400;
        for (base, size) in [
            (src_pixels, 12),
            (dst_pixels, 12),
            (src_pixmap, PPC_PIXMAP_SIZE as usize),
            (dst_pixmap, PPC_PIXMAP_SIZE as usize),
            (rect, 8),
        ] {
            loaded.memory.add_region(base, vec![0; size]);
        }
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 3, 4, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 0, 0, 3, 4, 8).unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
            .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 3, 4).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 12];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn hle_import_runner_copybits_converts_8bpp_to_16bpp() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12500;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let src_ctable_handle = scratch + 0xa0;
        let src_ctable = scratch + 0xb0;
        let rect = scratch + 0xd0;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 2, 0, 0, 1, 2, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            4,
            0,
            0,
            1,
            2,
            16,
        )
        .unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 8, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 10, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 12, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 14, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 16, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 18, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 20, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 22, 0).unwrap();
        loaded.memory.write_bytes(src_pixels, &[1, 0]).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 2).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(dst_pixels), Some(0x7c00));
        assert_eq!(loaded.memory.read_u16_be(dst_pixels + 2), Some(0x001f));
    }

    #[test]
    fn hle_import_runner_copybits_converts_16bpp_to_8bpp() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12600;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        loaded.memory.add_region(scratch, vec![0; 0xb0]);
        ppc_write_pixmap(
            &mut loaded.memory,
            src_pixmap,
            src_pixels,
            4,
            0,
            0,
            1,
            2,
            16,
        )
        .unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 2, 0, 0, 1, 2, 8).unwrap();
        for index in 0..256u32 {
            let spec = PPC_MAIN_CTABLE + 8 + index * 8;
            loaded.memory.write_u16_be(spec, index as u16).unwrap();
            loaded.memory.write_u16_be(spec + 2, 0).unwrap();
            loaded.memory.write_u16_be(spec + 4, 0).unwrap();
            loaded.memory.write_u16_be(spec + 6, 0).unwrap();
        }
        loaded
            .memory
            .write_u16_be(PPC_MAIN_CTABLE + 8 + 42 * 8 + 2, 0xffff)
            .unwrap();
        loaded
            .memory
            .write_u16_be(PPC_MAIN_CTABLE + 8 + 17 * 8 + 6, 0xffff)
            .unwrap();
        loaded.memory.write_u16_be(src_pixels, 0x7c00).unwrap();
        loaded.memory.write_u16_be(src_pixels + 2, 0x001f).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 2).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(42));
        assert_eq!(loaded.memory.read_u8(dst_pixels + 1), Some(17));
    }

    #[test]
    fn hle_import_runner_copybits_expands_bitmap_foreground_and_background() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12700;
        let src_pixels = scratch;
        let dst8_pixels = scratch + 0x10;
        let dst16_pixels = scratch + 0x20;
        let src_bitmap = scratch + 0x30;
        let dst8_pixmap = scratch + 0x50;
        let dst16_pixmap = scratch + 0x90;
        let rect = scratch + 0xd0;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        loaded.memory.write_u8(src_pixels, 0x80).unwrap();
        loaded.memory.write_u32_be(src_bitmap, src_pixels).unwrap();
        loaded.memory.write_u16_be(src_bitmap + 4, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, src_bitmap + 6, 0, 0, 1, 2).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst8_pixmap,
            dst8_pixels,
            2,
            0,
            0,
            1,
            2,
            8,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst16_pixmap,
            dst16_pixels,
            4,
            0,
            0,
            1,
            2,
            16,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 2).unwrap();
        loaded.quickdraw_fore_color = PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        };
        loaded.quickdraw_back_color = PpcRgbColor {
            red: 0,
            green: 0xffff,
            blue: 0,
        };
        loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
        loaded
            .toolbox_startup
            .quickdraw_back_indices
            .insert(PPC_MAIN_GWORLD, 42);
        loaded.cpu.gpr[3] = src_bitmap;
        loaded.cpu.gpr[4] = dst8_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(dst8_pixels), Some(103));
        assert_eq!(loaded.memory.read_u8(dst8_pixels + 1), Some(42));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[4] = dst16_pixmap;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u16_be(dst16_pixels), Some(0x7c00));
        assert_eq!(loaded.memory.read_u16_be(dst16_pixels + 2), Some(0x03e0));
    }

    #[test]
    fn packed_indexed_pixel_helpers_are_msb_first_and_preserve_row_padding() {
        let cases: &[(u32, &[u16], [u8; 2])] = &[
            (
                1,
                &[1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0],
                [0xa5, 0x5a],
            ),
            (2, &[0, 1, 2, 3, 3, 2, 1, 0], [0x1b, 0xe4]),
            (4, &[10, 5, 1, 14], [0xa5, 0x1e]),
        ];
        for &(depth, pixels, expected) in cases {
            let mut memory = PpcSectionMem::new();
            let base_addr = 0x1000;
            memory.add_region(base_addr, vec![0xcc; 3]);
            let front = PpcFrontBuffer {
                base_addr,
                row_bytes: 3,
                width: pixels.len() as u32,
                height: 1,
                depth,
            };
            for (x, pixel) in pixels.iter().copied().enumerate() {
                assert!(ppc_quickdraw_write_raw_pixel(
                    &mut memory,
                    front,
                    (x as i32, 0),
                    pixel,
                ));
            }
            assert_eq!(memory.read_u8(base_addr), Some(expected[0]));
            assert_eq!(memory.read_u8(base_addr + 1), Some(expected[1]));
            assert_eq!(memory.read_u8(base_addr + 2), Some(0xcc));
            for (x, pixel) in pixels.iter().copied().enumerate() {
                assert_eq!(
                    ppc_quickdraw_read_pixel(&mut memory, front, (x as i32, 0)),
                    Some(pixel)
                );
            }
            assert!(!ppc_quickdraw_write_raw_pixel(
                &mut memory,
                front,
                (-1, 0),
                0,
            ));
            assert!(!ppc_quickdraw_write_raw_pixel(
                &mut memory,
                front,
                (pixels.len() as i32, 0),
                0,
            ));
            assert_eq!(memory.read_u8(base_addr + 2), Some(0xcc));
        }
    }

    #[test]
    fn short_two_bit_pixmap_rows_are_rejected_before_cross_row_access() {
        let mut memory = PpcSectionMem::new();
        let pixmap = 0x1000;
        let pixels = 0x1100;
        memory.add_region(pixmap, vec![0; 0x200]);
        ppc_write_pixmap(&mut memory, pixmap, pixels, 1, 0, 0, 2, 8, 2).unwrap();
        memory.write_bytes(pixels, &[0x55, 0xa5]).unwrap();

        assert!(ppc_read_pixmap_bits(&mut memory, pixmap).is_none());

        let front = PpcFrontBuffer {
            base_addr: pixels,
            row_bytes: 1,
            width: 8,
            height: 2,
            depth: 2,
        };
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut memory, front, (3, 0)),
            Some(1)
        );
        assert_eq!(ppc_quickdraw_read_pixel(&mut memory, front, (4, 0)), None);
        assert!(!ppc_quickdraw_write_raw_pixel(
            &mut memory,
            front,
            (4, 0),
            2,
        ));
        assert_eq!(memory.read_u8(pixels), Some(0x55));
        assert_eq!(memory.read_u8(pixels + 1), Some(0xa5));
    }

    #[test]
    fn hle_import_runner_copybits_copies_one_two_and_four_bit_rows_without_touching_padding() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        for (depth, width, packed) in [
            (1, 16, [0xa5, 0x5a]),
            (2, 8, [0x1b, 0xe4]),
            (4, 4, [0xa5, 0x1e]),
        ] {
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch = PPC_HEAP_BASE + 0x12780;
            let src_pixels = scratch;
            let dst_pixels = scratch + 0x10;
            let src_pixmap = scratch + 0x20;
            let dst_pixmap = scratch + 0x60;
            let rect = scratch + 0xa0;
            loaded.memory.add_region(scratch, vec![0; 0xb0]);
            ppc_write_pixmap(
                &mut loaded.memory,
                src_pixmap,
                src_pixels,
                3,
                0,
                0,
                1,
                width,
                depth,
            )
            .unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                dst_pixmap,
                dst_pixels,
                3,
                0,
                0,
                1,
                width,
                depth,
            )
            .unwrap();
            loaded
                .memory
                .write_bytes(src_pixels, &[packed[0], packed[1], 0x99])
                .unwrap();
            loaded
                .memory
                .write_bytes(dst_pixels, &[0xcc, 0xcc, 0x77])
                .unwrap();
            ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, width).unwrap();
            loaded.cpu.gpr[3] = src_pixmap;
            loaded.cpu.gpr[4] = dst_pixmap;
            loaded.cpu.gpr[5] = rect;
            loaded.cpu.gpr[6] = rect;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut copied = [0; 3];
            loaded
                .memory
                .read_bytes_into(dst_pixels, &mut copied)
                .unwrap();
            assert_eq!(copied, [packed[0], packed[1], 0x77]);
        }
    }

    #[test]
    fn hle_import_runner_copybits_packed_identity_preserves_edges_and_padding() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        for (depth, width, copy_right, source_row, destination_row, expected_row) in [
            (
                2,
                8,
                17,
                &[0x1b, 0xe4, 0x91][..],
                &[0x80, 0x01, 0x5a][..],
                &[0x9b, 0xe5, 0x5a][..],
            ),
            (
                4,
                6,
                15,
                &[0x01, 0x23, 0x45, 0x92][..],
                &[0xa0, 0x00, 0x0b, 0x5a][..],
                &[0xa1, 0x23, 0x4b, 0x5a][..],
            ),
        ] {
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch = PPC_HEAP_BASE + 0x127c0;
            let source_pixels = scratch;
            let destination_pixels = scratch + 0x10;
            let source_pixmap = scratch + 0x20;
            let destination_pixmap = scratch + 0x60;
            let rects = scratch + 0xa0;
            loaded.memory.add_region(scratch, vec![0; 0xc0]);
            ppc_write_pixmap(
                &mut loaded.memory,
                source_pixmap,
                source_pixels,
                source_row.len() as u32,
                -2,
                10,
                0,
                10 + width,
                depth,
            )
            .unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                destination_pixmap,
                destination_pixels,
                destination_row.len() as u32,
                20,
                30,
                22,
                30 + width,
                depth,
            )
            .unwrap();
            for pixmap in [source_pixmap, destination_pixmap] {
                loaded
                    .memory
                    .write_u32_be(pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
                    .unwrap();
            }
            let mut source = source_row.repeat(2);
            *source.last_mut().unwrap() ^= 1;
            let mut destination = destination_row.repeat(2);
            *destination.last_mut().unwrap() ^= 1;
            loaded.memory.write_bytes(source_pixels, &source).unwrap();
            loaded
                .memory
                .write_bytes(destination_pixels, &destination)
                .unwrap();
            ppc_write_rect(&mut loaded.memory, rects, -2, 11, 0, copy_right).unwrap();
            ppc_write_rect(
                &mut loaded.memory,
                rects + 8,
                20,
                31,
                22,
                31 + (copy_right - 11),
            )
            .unwrap();
            loaded.cpu.gpr[3] = source_pixmap;
            loaded.cpu.gpr[4] = destination_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = vec![0; destination.len()];
            loaded
                .memory
                .read_bytes_into(destination_pixels, &mut actual)
                .unwrap();
            let mut expected = expected_row.repeat(2);
            *expected.last_mut().unwrap() ^= 1;
            assert_eq!(actual, expected, "depth={depth}");

            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded
                .memory
                .write_bytes(destination_pixels, &destination)
                .unwrap();
            let (unequal_source_right, unequal_destination_right, unequal_expected) = if depth == 2
            {
                (17, 36, &[0x6f, 0x91, 0x5a][..])
            } else {
                (15, 34, &[0x12, 0x34, 0x0b, 0x5a][..])
            };
            ppc_write_rect(&mut loaded.memory, rects, -2, 11, -1, unequal_source_right).unwrap();
            ppc_write_rect(
                &mut loaded.memory,
                rects + 8,
                20,
                30,
                21,
                unequal_destination_right,
            )
            .unwrap();

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = vec![0; destination_row.len()];
            loaded
                .memory
                .read_bytes_into(destination_pixels, &mut actual)
                .unwrap();
            assert_eq!(
                actual, unequal_expected,
                "unequal field offsets: depth={depth}"
            );
        }
    }

    #[test]
    fn hle_import_runner_copybits_packed_distinct_aliases_snapshot_source() {
        const SOURCE: u32 = 0x0900_0000;
        const DESTINATION: u32 = 0x0A00_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes(vec![
            0x1b, 0xe4, 0xaa, 0xe4, 0x1b, 0xbb, 0, 0, 0xcc,
        ]);
        // SAFETY: the import accesses both aliases serially through one
        // operation, and no byte slice survives a memory call.
        unsafe {
            loaded.memory.add_shared_region(SOURCE, backing.clone());
            loaded.memory.add_shared_region(DESTINATION, backing);
        }
        let records = PPC_HEAP_BASE + 0x16090;
        let source_pixmap = records;
        let destination_pixmap = records + 0x40;
        let rect = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, source_pixmap, SOURCE, 3, 0, 0, 2, 8, 2).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            destination_pixmap,
            DESTINATION + 3,
            3,
            0,
            0,
            2,
            8,
            2,
        )
        .unwrap();
        for pixmap in [source_pixmap, destination_pixmap] {
            loaded
                .memory
                .write_u32_be(pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
                .unwrap();
        }
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 2, 8).unwrap();
        loaded.cpu.gpr[3] = source_pixmap;
        loaded.cpu.gpr[4] = destination_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 9];
        loaded.memory.read_bytes_into(SOURCE, &mut actual).unwrap();
        assert_eq!(
            actual,
            [0x1b, 0xe4, 0xaa, 0x1b, 0xe4, 0xbb, 0xe4, 0x1b, 0xcc]
        );
    }

    #[test]
    fn hle_import_runner_copybits_packed_failures_do_not_fallback() {
        const SOURCE: u32 = 0x0900_0000;
        const DESTINATION: u32 = 0x0A00_0000;
        for source_failure in [true, false] {
            let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
            loaded.memory.add_region(
                SOURCE,
                if source_failure {
                    vec![0x1b, 0xe4, 0xaa]
                } else {
                    vec![0x1b, 0xe4, 0xaa, 0xe4, 0x1b, 0xbb]
                },
            );
            loaded
                .memory
                .add_region(DESTINATION, vec![0x80, 0x01, 0x5a, 0x81, 0x02, 0x5b]);
            if !source_failure {
                loaded
                    .memory
                    .add_readonly_region(DESTINATION + 3, vec![0x81, 0x02]);
            }
            let records = PPC_HEAP_BASE + 0x16090;
            let source_pixmap = records;
            let destination_pixmap = records + 0x40;
            let rect = records + 0x80;
            loaded.memory.add_region(records, vec![0; 0x90]);
            ppc_write_pixmap(&mut loaded.memory, source_pixmap, SOURCE, 3, 0, 0, 2, 8, 2).unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                destination_pixmap,
                DESTINATION,
                3,
                0,
                0,
                2,
                8,
                2,
            )
            .unwrap();
            for pixmap in [source_pixmap, destination_pixmap] {
                loaded
                    .memory
                    .write_u32_be(pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
                    .unwrap();
            }
            ppc_write_rect(&mut loaded.memory, rect, 0, 1, 2, 7).unwrap();
            loaded.cpu.gpr[3] = source_pixmap;
            loaded.cpu.gpr[4] = destination_pixmap;
            loaded.cpu.gpr[5] = rect;
            loaded.cpu.gpr[6] = rect;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = [0; 6];
            loaded
                .memory
                .read_bytes_into(DESTINATION, &mut actual)
                .unwrap();
            let expected = if source_failure {
                [0x80, 0x01, 0x5a, 0x81, 0x02, 0x5b]
            } else {
                [0x9b, 0xe5, 0x5a, 0x81, 0x02, 0x5b]
            };
            assert_eq!(actual, expected, "source_failure={source_failure}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_copies_4bpp_odd_and_even_nibbles() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12800;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rects = scratch + 0xa0;
        loaded.memory.add_region(scratch, vec![0; 0xc0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 3, 0, 0, 1, 5, 4).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 5, 4).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[0x12, 0x34, 0x50])
            .unwrap();
        loaded
            .memory
            .write_bytes(dst_pixels, &[0xaa, 0xbb, 0xcc])
            .unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 1, 1, 4).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 3];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0x23, 0x4b, 0xcc]);
    }

    #[test]
    fn hle_import_runner_copybits_4bpp_matches_within_active_destination_table() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12880;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        let src_ctable_handle = scratch + 0xb0;
        let src_ctable = scratch + 0xc0;
        loaded.memory.add_region(scratch, vec![0; 0x200]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 2, 4).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 2, 4).unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded.memory.write_u32_be(src_ctable, 0x1234_5678).unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 14).unwrap();
        let standard = TrapDispatcher::standard_mac_8bpp_clut();
        for (index, mut color) in standard.into_iter().take(15).enumerate() {
            if index == 14 {
                color = [0x4000, 0x4000, 0x4000];
            }
            let spec = src_ctable + 8 + index as u32 * 8;
            loaded.memory.write_u16_be(spec, index as u16).unwrap();
            loaded.memory.write_u16_be(spec + 2, color[0]).unwrap();
            loaded.memory.write_u16_be(spec + 4, color[1]).unwrap();
            loaded.memory.write_u16_be(spec + 6, color[2]).unwrap();
        }
        let main_ctable = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        for index in [5u32, 252] {
            let spec = main_ctable + 8 + index * 8;
            loaded.memory.write_u16_be(spec + 2, 0x4000).unwrap();
            loaded.memory.write_u16_be(spec + 4, 0x4000).unwrap();
            loaded.memory.write_u16_be(spec + 6, 0x4000).unwrap();
        }
        loaded.memory.write_u8(src_pixels, 0xe0).unwrap();
        loaded.memory.write_u8(dst_pixels, 0x0a).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        // Both index 5 and the misleading 8-bit index 252 are exact matches.
        // A 4-bit destination can represent only the first 16 active entries,
        // so CopyBits must choose 5 rather than choose 252 and mask it to 12.
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(0x5a));
    }

    #[test]
    fn hle_import_runner_copybits_4bpp_same_seed_preserves_indices() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12b00;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        let src_ctable_handle = scratch + 0xb0;
        let src_ctable = scratch + 0xc0;
        let dst_ctable_handle = scratch + 0x150;
        let dst_ctable = scratch + 0x160;
        loaded.memory.add_region(scratch, vec![0; 0x300]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 2, 4).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 2, 4).unwrap();

        let standard_4bpp = TrapDispatcher::standard_mac_4bpp_gworld_clut();
        let ctable = ppc_color_table_bytes(4, &standard_4bpp[..16]).unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_ctable_handle, dst_ctable)
            .unwrap();
        loaded.memory.write_bytes(src_ctable, &ctable).unwrap();
        loaded.memory.write_bytes(dst_ctable, &ctable).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, dst_ctable_handle)
            .unwrap();
        assert_eq!(
            ppc_resolve_pixmap_ctable_handle(&mut loaded.memory, &loaded.gworlds, src_pixmap),
            Some(src_ctable_handle)
        );
        assert_eq!(
            ppc_resolve_pixmap_ctable_handle(&mut loaded.memory, &loaded.gworlds, dst_pixmap),
            Some(dst_ctable_handle)
        );
        assert!(ppc_color_tables_share_index_space(
            &mut loaded.memory,
            Some(src_ctable_handle),
            Some(dst_ctable_handle)
        ));
        loaded.memory.write_u8(src_pixels, 0xe0).unwrap();
        loaded.memory.write_u8(dst_pixels, 0x0a).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(src_ctable_handle, dst_ctable_handle);
        assert_eq!(loaded.memory.read_u32_be(src_ctable), Some(4));
        assert_eq!(loaded.memory.read_u32_be(dst_ctable), Some(4));
        assert_eq!(loaded.memory.read_u16_be(src_ctable + 6), Some(15));
        assert_eq!(loaded.memory.read_u16_be(dst_ctable + 6), Some(15));
        // Distinct GetCTable(4)-style handles with the same nonzero ctSeed
        // identify the same indexed colors, so the source index is retained
        // even though the current main GDevice has a different inverse table.
        // The neighboring low nibble remains untouched.
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(0xea));

        loaded.memory.write_u8(src_pixels, 0x0d).unwrap();
        loaded.memory.write_u8(dst_pixels, 0xa0).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 1, 1, 2).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(0xad));

        // A reused seed is insufficient when the ordinary image tables no
        // longer describe the same colors.
        loaded
            .memory
            .write_u16_be(dst_ctable + 8 + 13 * 8 + 2, 0x1234)
            .unwrap();
        assert!(!ppc_color_tables_share_index_space(
            &mut loaded.memory,
            Some(src_ctable_handle),
            Some(dst_ctable_handle)
        ));

        loaded.memory.write_u8(src_pixels, 0x0d).unwrap();
        loaded.memory.write_u8(dst_pixels, 0xa0).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let [red, green, blue] = standard_4bpp[13];
        let active_clut = ppc_copy_bits_clut(
            &mut loaded.memory,
            PPC_MAIN_CTABLE_HANDLE,
            &loaded.color_manager_clut,
        );
        let mapped =
            ppc_rgb_color_to_index_in_clut(PpcRgbColor { red, green, blue }, &active_clut, 16);
        assert_ne!(mapped, 13);
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(0xa0 | mapped));

        loaded
            .memory
            .write_u16_be(dst_ctable + 8 + 13 * 8 + 2, standard_4bpp[13][0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(dst_ctable + 8 + 14 * 8, 13)
            .unwrap();
        assert!(!ppc_color_tables_share_index_space(
            &mut loaded.memory,
            Some(src_ctable_handle),
            Some(dst_ctable_handle)
        ));
    }

    #[test]
    fn hle_import_runner_copybits_4bpp_uses_non_main_device_clut() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12c00;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        let src_ctable_handle = scratch + 0xb0;
        let src_ctable = scratch + 0xc0;
        let gdevice_handle = scratch + 0x180;
        let gdevice = scratch + 0x190;
        let device_pixmap_handle = scratch + 0x1d0;
        let device_pixmap = scratch + 0x1e0;
        let device_ctable_handle = scratch + 0x220;
        let device_ctable = scratch + 0x230;
        loaded.memory.add_region(scratch, vec![0; 0x400]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 2, 4).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 2, 4).unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 14).unwrap();

        loaded.memory.write_u32_be(gdevice_handle, gdevice).unwrap();
        loaded
            .memory
            .write_u32_be(gdevice + 22, device_pixmap_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_pixmap_handle, device_pixmap)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_pixmap + 42, device_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_ctable_handle, device_ctable)
            .unwrap();
        loaded
            .memory
            .write_u16_be(device_ctable + 4, 0x8000)
            .unwrap();
        loaded.memory.write_u16_be(device_ctable + 6, 14).unwrap();

        let standard = TrapDispatcher::standard_mac_8bpp_clut();
        for (index, mut color) in standard.into_iter().take(15).enumerate() {
            if index == 0 {
                color = [0x1234, 0, 0];
            } else if index == 14 {
                color = [0x4000, 0x4000, 0x4000];
            }
            let spec = src_ctable + 8 + index as u32 * 8;
            loaded.memory.write_u16_be(spec, index as u16).unwrap();
            loaded.memory.write_u16_be(spec + 2, color[0]).unwrap();
            loaded.memory.write_u16_be(spec + 4, color[1]).unwrap();
            loaded.memory.write_u16_be(spec + 6, color[2]).unwrap();
        }
        for (index, mut color) in TrapDispatcher::standard_mac_8bpp_clut()
            .into_iter()
            .take(15)
            .enumerate()
        {
            if index == 14 {
                color = [0x4000, 0x4000, 0x4000];
            }
            let spec = device_ctable + 8 + index as u32 * 8;
            loaded.memory.write_u16_be(spec, index as u16).unwrap();
            loaded.memory.write_u16_be(spec + 2, color[0]).unwrap();
            loaded.memory.write_u16_be(spec + 4, color[1]).unwrap();
            loaded.memory.write_u16_be(spec + 6, color[2]).unwrap();
        }
        loaded.memory.write_u8(src_pixels, 0xe0).unwrap();
        loaded.memory.write_u8(dst_pixels, 0x0a).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = gdevice_handle);
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            TrapDispatcher::standard_itable_lookup(0x4000, 0x4000, 0x4000),
            252
        );
        // The synthetic current device instead has an exact match at index
        // 14, proving that the main device's startup inverse table is not
        // applied to every GDevice. The neighboring low nibble is preserved.
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(0xea));
    }

    #[test]
    fn hle_import_runner_copybits_transparent_4bpp_preserves_back_color_pixels() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12900;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let rect = scratch + 0xa0;
        loaded.memory.add_region(scratch, vec![0; 0xb0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 2, 0, 0, 1, 4, 4).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 2, 0, 0, 1, 4, 4).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[0x23, 0x24])
            .unwrap();
        loaded
            .memory
            .write_bytes(dst_pixels, &[0x9a, 0xbc])
            .unwrap();
        let [red, green, blue] = loaded.color_manager_clut[2];
        loaded.quickdraw_back_color = PpcRgbColor { red, green, blue };
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 4).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 36;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 2];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0x93, 0xb4]);
    }

    #[test]
    fn hle_import_runner_copybits_clips_unscaled_copy_to_source_bounds() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12a00;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rects = scratch + 0xc0;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 3, 4, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 2, 0, 6, 4, 8).unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[42; 16]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 4, 4).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 2, 0, 6, 4).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 16];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(
            copied,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 42, 42, 42, 42]
        );
    }

    #[test]
    fn hle_import_runner_copybits_reduces_physical_source_bound_crossing() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12c00;
        let source_allocation = scratch;
        let src_pixels = source_allocation + 2;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rects = scratch + 0xc0;
        let ctable_handle = scratch + 0xd0;
        let ctable = scratch + 0xe0;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 8, 0, 0, 1, 8, 8).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 8, 2).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 12, 7).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, ctable_handle)
            .unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u32_be(ctable, 0x1234_5678).unwrap();
        loaded.memory.write_u16_be(ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 0).unwrap();
        loaded.memory.write_u16_be(ctable + 8, 0).unwrap();
        let [red, green, blue] = loaded.color_manager_clut[0];
        loaded.memory.write_u16_be(ctable + 10, red).unwrap();
        loaded.memory.write_u16_be(ctable + 12, green).unwrap();
        loaded.memory.write_u16_be(ctable + 14, blue).unwrap();
        loaded
            .memory
            .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [241, 30, 50, 0xa5]);
    }

    #[test]
    fn pixmap_ctable_resolution_uses_fallback_after_failed_tracked_port_lookup() {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let pixmap = PPC_HEAP_BASE + 0x16400;
        let ctable_handle = pixmap + 0x40;
        loaded.memory.add_region(pixmap, vec![0; 0x50]);
        ppc_write_pixmap(&mut loaded.memory, pixmap, pixmap + 0x48, 8, 0, 0, 1, 8, 8).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();
        let tracked_port = PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: pixmap,
            pixmap_handle: 0,
            pixmap: pixmap + 0x1000,
            base_addr: pixmap + 0x48,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 1,
            depth: 8,
            row_bytes: 8,
            pixels_locked: false,
            pixels_no_purge: false,
        };

        let resolved = ppc_resolve_pixmap_ctable_handle_with_provenance(
            &mut loaded.memory,
            &[tracked_port],
            pixmap,
        );

        assert_eq!(resolved.handle, Some(ctable_handle));
        assert!(resolved.known);
    }

    #[test]
    fn pixmap_ctable_resolution_tries_indirect_when_direct_rowbytes_are_unreadable() {
        const INDIRECT: u32 = 0x0952_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let pixmap = PPC_HEAP_BASE + 0x16500;
        let ctable_handle = pixmap + 0x40;
        loaded
            .memory
            .add_region(INDIRECT, pixmap.to_be_bytes().to_vec());
        loaded.memory.add_region(pixmap, vec![0; 0x50]);
        ppc_write_pixmap(&mut loaded.memory, pixmap, pixmap + 0x48, 8, 0, 0, 1, 8, 8).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();

        let resolved =
            ppc_resolve_pixmap_ctable_handle_with_provenance(&mut loaded.memory, &[], INDIRECT);

        assert_eq!(resolved.handle, Some(ctable_handle));
        assert!(resolved.known);
    }

    #[test]
    fn gdevice_ctable_resolution_preserves_mapped_low_memory_zero_handle_chain() {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let records = PPC_HEAP_BASE + 0x16580;
        let gdevice = records;
        let pixmap_handle = records + 0x40;
        let pixmap = records + 0x50;
        let ctable_handle = records + 0x90;
        loaded.memory.add_region(0, gdevice.to_be_bytes().to_vec());
        loaded.memory.add_region(records, vec![0; 0xa0]);
        loaded
            .memory
            .write_u32_be(gdevice + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        loaded
            .memory
            .write_u32_be(pixmap + 42, ctable_handle)
            .unwrap();

        let resolved = ppc_gdevice_ctable_handle_with_provenance(&mut loaded.memory, 0);

        assert_eq!(resolved.handle, Some(ctable_handle));
        assert!(resolved.known);
        assert_eq!(
            ppc_gdevice_ctable_handle(&mut loaded.memory, 0),
            Some(ctable_handle)
        );
    }

    fn ppc_indexed_vertical_oracle_groups(
        destination_height: usize,
    ) -> &'static [std::ops::Range<usize>] {
        match destination_height {
            7 => &[0..2, 2..4, 4..7, 7..9, 9..11, 11..14, 14..16],
            18 => &[
                0..1,
                1..2,
                2..3,
                3..4,
                4..5,
                5..6,
                6..7,
                7..8,
                7..8,
                8..9,
                9..10,
                10..11,
                11..12,
                12..13,
                13..14,
                14..15,
                15..16,
                16..17,
            ],
            _ => unreachable!(),
        }
    }

    fn run_ppc_indexed_vertical_route(
        destination_height: usize,
        source_top: usize,
        visible_rows: std::ops::Range<usize>,
        visible_columns: std::ops::Range<usize>,
        impulse: usize,
    ) -> Vec<u8> {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let scratch = PPC_HEAP_BASE + 0x17400;
        let source_pixels = scratch;
        let destination_allocation = scratch + 0x80;
        let destination_pixels =
            destination_allocation + visible_rows.start as u32 * 4 + visible_columns.start as u32;
        let source_pixmap = scratch + 0x100;
        let destination_pixmap = scratch + 0x140;
        let rects = scratch + 0x180;
        loaded.memory.add_region(scratch, vec![0; 0x1a0]);
        ppc_write_pixmap(
            &mut loaded.memory,
            source_pixmap,
            source_pixels,
            4,
            0,
            0,
            (source_top + 17) as i16,
            4,
            8,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            destination_pixmap,
            destination_pixels,
            4,
            visible_rows.start as i16,
            visible_columns.start as i16,
            visible_rows.end as i16,
            visible_columns.end as i16,
            8,
        )
        .unwrap();
        let mut source = vec![0xcc; (source_top + 17) * 4];
        for row in 0..17 {
            for column in 0..3 {
                source[(source_top + row) * 4 + column] = u8::from(row == impulse) * 255;
            }
        }
        loaded.memory.write_bytes(source_pixels, &source).unwrap();
        loaded
            .memory
            .write_bytes(destination_allocation, &vec![0xa5; destination_height * 4])
            .unwrap();
        ppc_write_rect(
            &mut loaded.memory,
            rects,
            source_top as i16,
            0,
            (source_top + 17) as i16,
            3,
        )
        .unwrap();
        assert_eq!(loaded.memory.read_u16_be(source_pixmap + 6), Some(0));
        assert_eq!(loaded.memory.read_u16_be(rects), Some(source_top as u16));
        ppc_write_rect(
            &mut loaded.memory,
            rects + 8,
            0,
            0,
            destination_height as i16,
            3,
        )
        .unwrap();
        loaded.cpu.gpr[3] = source_pixmap;
        loaded.cpu.gpr[4] = destination_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = vec![0; destination_height * 4];
        loaded
            .memory
            .read_bytes_into(destination_allocation, &mut actual)
            .unwrap();
        actual
    }

    #[test]
    fn hle_import_runner_copybits_matches_indexed_vertical_phase_oracles() {
        // Literal one-hot source-row memberships captured through classic
        // CopyBits/StdBits on Mac OS 8.1 and reused to validate the PPC ABI.
        // Repeated columns and horizontal clipping extend only the orthogonal
        // coverage; they do not claim an additional PPC oracle capture.
        let cases = [
            (7, 0, 0..7, 0..3),
            (7, 1, 0..7, 0..3),
            (7, 0, 2..7, 0..3),
            (7, 1, 0..5, 0..3),
            (18, 0, 0..18, 0..3),
            (18, 1, 0..18, 0..3),
            (18, 1, 2..18, 0..3),
            (18, 1, 0..16, 0..3),
            (7, 1, 2..7, 1..3),
        ];
        for (destination_height, source_top, visible_rows, visible_columns) in cases {
            for impulse in 0..17 {
                let actual = run_ppc_indexed_vertical_route(
                    destination_height,
                    source_top,
                    visible_rows.clone(),
                    visible_columns.clone(),
                    impulse,
                );
                let groups = ppc_indexed_vertical_oracle_groups(destination_height);
                let mut expected = vec![0xa5; destination_height * 4];
                for row in visible_rows.clone() {
                    for column in visible_columns.clone() {
                        expected[row * 4 + column] = u8::from(groups[row].contains(&impulse)) * 255;
                    }
                }
                assert_eq!(
                    actual, expected,
                    "destination_height={destination_height}, source_top={source_top}, visible_rows={visible_rows:?}, visible_columns={visible_columns:?}, impulse={impulse}"
                );
            }
        }
    }

    fn run_ppc_indexed_vertical_legacy_case(
        raw_mode: u16,
        nonidentity_palette: bool,
        two_axis: bool,
        source_outside_bounds: bool,
    ) -> [u8; 6] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let source_width = if two_axis { 3 } else { 2 };
        let scratch = PPC_HEAP_BASE + 0x17c00;
        let source_pixels = scratch;
        let destination_pixels = scratch + 0x40;
        let source_pixmap = scratch + 0x80;
        let destination_pixmap = scratch + 0xc0;
        let rects = scratch + 0x100;
        let table_handle = scratch + 0x110;
        let table = scratch + 0x120;
        loaded.memory.add_region(scratch, vec![0; 0x240]);
        ppc_write_pixmap(
            &mut loaded.memory,
            source_pixmap,
            source_pixels,
            source_width,
            if source_outside_bounds { 1 } else { 0 },
            0,
            5,
            source_width as i16,
            8,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            destination_pixmap,
            destination_pixels,
            2,
            0,
            0,
            3,
            2,
            8,
        )
        .unwrap();
        if nonidentity_palette {
            loaded
                .memory
                .write_u32_be(source_pixmap + 42, table_handle)
                .unwrap();
            loaded.memory.write_u32_be(table_handle, table).unwrap();
            loaded.memory.write_u32_be(table, 0x1234_5678).unwrap();
            loaded.memory.write_u16_be(table + 4, 0).unwrap();
            loaded.memory.write_u16_be(table + 6, 30).unwrap();
            for index in 0..=30u32 {
                let color_index = if index == 10 { 200 } else { index as usize };
                let [red, green, blue] = loaded.color_manager_clut[color_index];
                let entry = table + 8 + index * 8;
                loaded.memory.write_u16_be(entry, index as u16).unwrap();
                loaded.memory.write_u16_be(entry + 2, red).unwrap();
                loaded.memory.write_u16_be(entry + 4, green).unwrap();
                loaded.memory.write_u16_be(entry + 6, blue).unwrap();
            }
        }
        let rows: [[u8; 3]; 5] = if nonidentity_palette {
            [[10, 10, 0], [1, 1, 0], [20, 20, 0], [2, 2, 0], [30, 30, 0]]
        } else {
            [
                [10, 11, 12],
                [20, 21, 22],
                [30, 31, 32],
                [40, 41, 42],
                [50, 51, 52],
            ]
        };
        for row in 0..5usize {
            loaded
                .memory
                .write_bytes(
                    source_pixels + (row * source_width as usize) as u32,
                    &rows[row][..source_width as usize],
                )
                .unwrap();
        }
        loaded
            .memory
            .write_bytes(destination_pixels, &[0xa5; 6])
            .unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 5, source_width as i16).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 3, 2).unwrap();
        loaded.cpu.gpr[3] = source_pixmap;
        loaded.cpu.gpr[4] = destination_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = raw_mode as u32;
        loaded.cpu.gpr[8] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 6];
        loaded
            .memory
            .read_bytes_into(destination_pixels, &mut actual)
            .unwrap();
        actual
    }

    #[test]
    fn hle_import_runner_copybits_keeps_vertical_exclusions_on_legacy_scaler() {
        assert_eq!(
            run_ppc_indexed_vertical_legacy_case(0x40, false, false, false),
            [10, 11, 20, 21, 40, 41]
        );
        assert_eq!(
            run_ppc_indexed_vertical_legacy_case(0, true, false, false),
            [200, 200, 1, 1, 2, 2]
        );
        assert_eq!(
            run_ppc_indexed_vertical_legacy_case(0, false, true, false),
            [10, 11, 20, 21, 40, 41]
        );
        assert_eq!(
            run_ppc_indexed_vertical_legacy_case(0, false, false, true),
            [0xa5, 0xa5, 10, 11, 30, 31]
        );
    }

    fn run_ppc_indexed_horizontal_adapter_case(
        raw_mode: u16,
        clip_left_column: bool,
        synthesized_source: bool,
        nonidentity_palette: bool,
    ) -> [u8; 4] {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x16440;
        let source_allocation = scratch;
        let src_pixels = source_allocation + 2;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rects = scratch + 0xc0;
        let ctable_handle = scratch + 0xd0;
        let ctable = scratch + 0xe0;
        loaded.memory.add_region(scratch, vec![0; 0x160]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 8, 0, 2, 1, 7, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
        let source_bits_ptr = if synthesized_source {
            const SYNTHESIZED_PIXMAP: u32 = 0x0951_0000;
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: SYNTHESIZED_PIXMAP + 0x1000,
                pixmap_handle: 0,
                pixmap: SYNTHESIZED_PIXMAP,
                base_addr: src_pixels,
                gdevice: PPC_MAIN_GDEVICE,
                width: 5,
                height: 1,
                depth: 8,
                row_bytes: 8,
                pixels_locked: false,
                pixels_no_purge: false,
            });
            SYNTHESIZED_PIXMAP
        } else {
            src_pixmap
        };
        if nonidentity_palette {
            loaded
                .memory
                .write_u32_be(src_pixmap + 42, ctable_handle)
                .unwrap();
            loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
            loaded.memory.write_u32_be(ctable, 0x1234_5678).unwrap();
            loaded.memory.write_u16_be(ctable + 4, 0).unwrap();
            loaded.memory.write_u16_be(ctable + 6, 10).unwrap();
            for index in 0..=10u32 {
                let color_index = if index == 10 { 200 } else { index as usize };
                let [red, green, blue] = loaded.color_manager_clut[color_index];
                let entry = ctable + 8 + index * 8;
                loaded.memory.write_u16_be(entry, index as u16).unwrap();
                loaded.memory.write_u16_be(entry + 2, red).unwrap();
                loaded.memory.write_u16_be(entry + 4, green).unwrap();
                loaded.memory.write_u16_be(entry + 6, blue).unwrap();
            }
        }
        loaded
            .memory
            .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        if clip_left_column {
            loaded.memory.write_u16_be(dst_pixmap + 8, 1).unwrap();
        }
        loaded.cpu.gpr[3] = source_bits_ptr;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = u32::from(raw_mode);
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        copied
    }

    #[test]
    fn hle_import_runner_copybits_keeps_dithered_horizontal_shrink_on_legacy_scaler() {
        assert_eq!(
            run_ppc_indexed_horizontal_adapter_case(0x40, false, false, false),
            [0xa5, 10, 30, 0xa5]
        );
    }

    #[test]
    fn hle_import_runner_copybits_keeps_global_groups_after_destination_clip() {
        assert_eq!(
            run_ppc_indexed_horizontal_adapter_case(0, true, false, false),
            [30, 50, 0xa5, 0xa5]
        );
    }

    #[test]
    fn hle_import_runner_copybits_keeps_valid_nonidentity_palette_on_legacy_scaler() {
        assert_eq!(
            run_ppc_indexed_horizontal_adapter_case(0, false, false, true),
            [0xa5, 200, 30, 0xa5]
        );
    }

    #[test]
    fn hle_import_runner_copybits_does_not_select_synthesized_source_bounds() {
        assert_eq!(
            run_ppc_indexed_horizontal_adapter_case(0, false, true, false),
            [10, 30, 50, 0xa5]
        );
    }

    fn run_ppc_indexed_horizontal_oracle_case(
        source_width: u16,
        destination_width: u16,
        source_left: u16,
        source_row: &[u8],
    ) -> Vec<u8> {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x16800;
        let source_stride = (source_row.len() + 1) & !1;
        let destination_stride = (usize::from(destination_width) + 5) & !1;
        let dst_pixels = scratch + u32::try_from((source_stride + 0x3f) & !0x3f).unwrap();
        let records = dst_pixels + u32::try_from(destination_stride).unwrap() + 0x20;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rects = records + 0x80;
        let allocation_size = usize::try_from(rects + 0x10 - scratch).unwrap();
        loaded.memory.add_region(scratch, vec![0; allocation_size]);
        ppc_write_pixmap(
            &mut loaded.memory,
            src_pixmap,
            scratch,
            source_stride as u32,
            0,
            0,
            1,
            (source_left + source_width) as i16,
            8,
        )
        .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            dst_pixels,
            destination_stride as u32,
            0,
            0,
            1,
            destination_width as i16,
            8,
        )
        .unwrap();
        loaded.memory.write_bytes(scratch, source_row).unwrap();
        loaded
            .memory
            .write_bytes(dst_pixels, &vec![0xa5; destination_stride])
            .unwrap();
        ppc_write_rect(
            &mut loaded.memory,
            rects,
            0,
            source_left as i16,
            1,
            (source_left + source_width) as i16,
        )
        .unwrap();
        ppc_write_rect(
            &mut loaded.memory,
            rects + 8,
            0,
            0,
            1,
            destination_width as i16,
        )
        .unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = vec![0; destination_stride];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        copied
    }

    fn ppc_indexed_horizontal_tail_low(source_width: usize) -> Vec<u8> {
        let mut row = vec![200; (source_width + 3) & !3];
        row[source_width - 1] = 0;
        row[source_width] = 254;
        row
    }

    fn ppc_indexed_horizontal_nonmonotonic(source_width: usize, case_index: usize) -> Vec<u8> {
        let mut row = vec![0; (source_width + 3) & !3];
        for (position, value) in row[..source_width].iter_mut().enumerate() {
            *value = ((position * 73 + case_index * 19) % 251 + 1) as u8;
        }
        row[source_width] = 254;
        row
    }

    #[test]
    fn hle_import_runner_copybits_matches_large_indexed_horizontal_oracles() {
        // Literal results from controlled Mac OS 8.1 CopyBits and StdBits
        // captures. The tail-low rows isolate the staged physical tail; the
        // nonmonotonic rows use the captured position encoding verbatim.
        for source_left in 0..4u16 {
            let mut row = vec![0x11; usize::from(source_left) + 194];
            row[..usize::from(source_left)].fill(250);
            row[usize::from(source_left)..usize::from(source_left) + 190].fill(20);
            row[usize::from(source_left) + 189] = 30;
            row[usize::from(source_left) + 190..usize::from(source_left) + 193]
                .copy_from_slice(&[200, 150, 140]);
            let actual = run_ppc_indexed_horizontal_oracle_case(190, 1, source_left, &row);
            assert_eq!(actual[0], 200, "190->1, source_left={source_left}");
            assert!(actual[1..].iter().all(|&byte| byte == 0xa5));

            row.fill(0x11);
            row[..usize::from(source_left)].fill(250);
            row[usize::from(source_left)..usize::from(source_left) + 191].fill(20);
            row[usize::from(source_left) + 190] = 30;
            row[usize::from(source_left) + 191..usize::from(source_left) + 194]
                .copy_from_slice(&[240, 150, 140]);
            let actual = run_ppc_indexed_horizontal_oracle_case(191, 1, source_left, &row);
            assert_eq!(actual[0], 30, "191->1, source_left={source_left}");
            assert!(actual[1..].iter().all(|&byte| byte == 0xa5));
        }

        for (source_width, destination_width, source, expected) in [
            (
                285,
                2,
                ppc_indexed_horizontal_tail_low(285),
                &[200, 254][..],
            ),
            (
                285,
                2,
                ppc_indexed_horizontal_nonmonotonic(285, 1),
                &[251, 254][..],
            ),
            (
                511,
                3,
                ppc_indexed_horizontal_tail_low(511),
                &[200, 200, 254][..],
            ),
            (
                511,
                3,
                ppc_indexed_horizontal_nonmonotonic(511, 7),
                &[251, 249, 254][..],
            ),
        ] {
            let actual =
                run_ppc_indexed_horizontal_oracle_case(source_width, destination_width, 0, &source);
            assert_eq!(&actual[..expected.len()], expected);
            assert!(actual[expected.len()..].iter().all(|&byte| byte == 0xa5));
        }

        for (source_width, expected) in [(4_097, 65), (5_000, 8), (5_001, 7), (10_924, u8::MAX)] {
            let source = vec![0; (source_width + 3) & !3];
            let actual = run_ppc_indexed_horizontal_oracle_case(source_width as u16, 1, 0, &source);
            assert_eq!(actual[0], expected, "{source_width}->1");
            assert!(actual[1..].iter().all(|&byte| byte == 0xa5));
        }
    }
    #[test]
    fn hle_import_runner_copybits_does_not_select_unresolved_equal_fallback_cluts() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x12d00;
        let source_allocation = scratch;
        let src_pixels = source_allocation + 2;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rects = scratch + 0xc0;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 8, 0, 0, 1, 8, 8).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 8, 2).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 12, 7).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, 0x0bad_0000)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, 0x0bad_1000)
            .unwrap();
        loaded
            .memory
            .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0xa5, 10, 30, 0xa5]);
    }

    #[test]
    fn hle_import_runner_copybits_does_not_select_when_source_table_field_is_unreadable() {
        const TRUNCATED_PIXMAP: u32 = 0x0950_0000;
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x16000;
        let source_allocation = scratch;
        let src_pixels = source_allocation + 2;
        let dst_pixels = scratch + 0x20;
        let dst_pixmap = scratch + 0x40;
        let rects = scratch + 0x80;
        loaded.memory.add_region(scratch, vec![0; 0xa0]);
        loaded.memory.add_region(TRUNCATED_PIXMAP, vec![0; 34]);
        // The resolver can read pixelSize at +32 but cannot read pmTable at
        // +42. The legacy path still receives its existing fallback CLUT;
        // only the new raw-index reducer must reject this unresolved lookup.
        assert_eq!(
            ppc_write_pixmap(
                &mut loaded.memory,
                TRUNCATED_PIXMAP,
                src_pixels,
                8,
                0,
                0,
                1,
                8,
                8,
            ),
            None
        );
        loaded.memory.write_u16_be(TRUNCATED_PIXMAP + 8, 2).unwrap();
        loaded
            .memory
            .write_u16_be(TRUNCATED_PIXMAP + 12, 7)
            .unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
        loaded
            .memory
            .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        loaded.cpu.gpr[3] = TRUNCATED_PIXMAP;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0xa5, 10, 30, 0xa5]);
    }

    #[test]
    fn hle_import_runner_copybits_does_not_select_through_broken_current_device_chain() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x16300;
        let source_allocation = scratch;
        let src_pixels = source_allocation + 2;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let dst_pixmap = scratch + 0x80;
        let rects = scratch + 0xc0;
        let gdevice_handle = scratch + 0xd0;
        let gdevice = scratch + 0xe0;
        loaded.memory.add_region(scratch, vec![0; 0x110]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 8, 0, 0, 1, 8, 8).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 8, 2).unwrap();
        loaded.memory.write_u16_be(src_pixmap + 12, 7).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
        loaded.memory.write_u32_be(gdevice_handle, gdevice).unwrap();
        loaded
            .memory
            .write_u32_be(gdevice + 22, 0x0bad_2000)
            .unwrap();
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = gdevice_handle);
        loaded
            .memory
            .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut copied = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut copied)
            .unwrap();
        assert_eq!(copied, [0xa5, 10, 30, 0xa5]);
    }

    #[test]
    fn hle_import_runner_copybits_uses_signed_source_height_gate() {
        for (bounds_bottom, expected) in [(32_766i16, [20, 50, 70, 0xa5]), (32_767i16, [0xa5; 4])] {
            let pef = synthetic_pef_with_import(b"CopyBits");
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch = PPC_HEAP_BASE + 0x12e00;
            let src_pixels = scratch;
            let dst_pixels = scratch + 0x20;
            let src_pixmap = scratch + 0x40;
            let dst_pixmap = scratch + 0x80;
            let rects = scratch + 0xc0;
            loaded.memory.add_region(scratch, vec![0; 0xe0]);
            ppc_write_pixmap(
                &mut loaded.memory,
                src_pixmap,
                src_pixels,
                8,
                -1,
                0,
                1,
                8,
                8,
            )
            .unwrap();
            loaded
                .memory
                .write_u16_be(src_pixmap + 10, bounds_bottom as u16)
                .unwrap();
            ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 3, 0, 0, 1, 3, 8).unwrap();
            loaded
                .memory
                .write_bytes(src_pixels, &[10, 20, 30, 40, 50, 60, 70, 0])
                .unwrap();
            loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
            ppc_write_rect(&mut loaded.memory, rects, -1, 0, 0, 7).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
            loaded.cpu.gpr[3] = src_pixmap;
            loaded.cpu.gpr[4] = dst_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut copied = [0; 4];
            loaded
                .memory
                .read_bytes_into(dst_pixels, &mut copied)
                .unwrap();
            assert_eq!(copied, expected, "bounds_bottom={bounds_bottom}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_matches_vertical_signed_source_height_capture() {
        for (bounds_bottom, expected_pixels) in [
            (32_766i16, vec![20, 40, 70, 90, 110, 140, 160]),
            (32_767i16, vec![0xa5; 7]),
        ] {
            let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
            let scratch = PPC_HEAP_BASE + 0x17600;
            let source_pixels = scratch;
            let destination_pixels = scratch + 0x80;
            let source_pixmap = scratch + 0x100;
            let destination_pixmap = scratch + 0x140;
            let rects = scratch + 0x180;
            loaded.memory.add_region(scratch, vec![0; 0x1a0]);
            ppc_write_pixmap(
                &mut loaded.memory,
                source_pixmap,
                source_pixels,
                2,
                -1,
                0,
                16,
                1,
                8,
            )
            .unwrap();
            loaded
                .memory
                .write_u16_be(source_pixmap + 10, bounds_bottom as u16)
                .unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                destination_pixmap,
                destination_pixels,
                2,
                0,
                0,
                7,
                1,
                8,
            )
            .unwrap();
            let mut source = Vec::new();
            for value in (10..=170).step_by(10) {
                source.extend_from_slice(&[value, 0xcc]);
            }
            loaded.memory.write_bytes(source_pixels, &source).unwrap();
            loaded
                .memory
                .write_bytes(destination_pixels, &[0xa5; 14])
                .unwrap();
            ppc_write_rect(&mut loaded.memory, rects, -1, 0, 16, 1).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 7, 1).unwrap();
            loaded.cpu.gpr[3] = source_pixmap;
            loaded.cpu.gpr[4] = destination_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = [0; 14];
            loaded
                .memory
                .read_bytes_into(destination_pixels, &mut actual)
                .unwrap();
            let mut expected = [0xa5; 14];
            for (row, value) in expected_pixels.into_iter().enumerate() {
                expected[row * 2] = value;
            }
            assert_eq!(actual, expected, "bounds_bottom={bounds_bottom}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_selects_only_exact_record_destination_bounds() {
        for (record_width, expected) in [
            (3u32, [241, 30, 50, 0xa5]),
            (40_000u32, [0xa5, 10, 30, 0xa5]),
        ] {
            let pef = synthetic_pef_with_import(b"CopyBits");
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch = PPC_HEAP_BASE + 0x12f00;
            let source_allocation = scratch;
            let src_pixels = source_allocation + 2;
            let dst_pixels = scratch + 0x20;
            let src_pixmap = scratch + 0x40;
            let destination_record = scratch + 0x80;
            let rects = scratch + 0xc0;
            loaded.memory.add_region(scratch, vec![0; 0xe0]);
            ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 8, 0, 0, 1, 8, 8).unwrap();
            loaded.memory.write_u16_be(src_pixmap + 8, 2).unwrap();
            loaded.memory.write_u16_be(src_pixmap + 12, 7).unwrap();
            loaded.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: destination_record + 0x1000,
                pixmap_handle: 0,
                pixmap: destination_record,
                base_addr: dst_pixels,
                gdevice: PPC_MAIN_GDEVICE,
                width: record_width,
                height: 1,
                depth: 8,
                row_bytes: 3,
                pixels_locked: false,
                pixels_no_purge: false,
            });
            loaded
                .memory
                .write_bytes(source_allocation, &[240, 241, 10, 20, 30, 40, 50, 0, 0, 0])
                .unwrap();
            loaded.memory.write_bytes(dst_pixels, &[0xa5; 4]).unwrap();
            ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
            loaded.cpu.gpr[3] = src_pixmap;
            loaded.cpu.gpr[4] = destination_record;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut copied = [0; 4];
            loaded
                .memory
                .read_bytes_into(dst_pixels, &mut copied)
                .unwrap();
            assert_eq!(copied, expected, "record_width={record_width}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_snapshots_indexed_horizontal_aliases() {
        const SOURCE: u32 = 0x0920_0000;
        const DESTINATION: u32 = 0x0a20_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes(vec![
            10, 20, 30, 40, 50, 60, 70, 0, 80, 90, 100, 110, 120, 130, 140, 0,
        ]);
        // SAFETY: the import accesses both aliases serially through one
        // operation, and no borrowed byte slice survives a memory call.
        unsafe {
            loaded.memory.add_shared_region(SOURCE, backing.clone());
            loaded.memory.add_shared_region(DESTINATION, backing);
        }
        let records = PPC_HEAP_BASE + 0x16120;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rects = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 8, 0, 0, 2, 8, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            DESTINATION + 8,
            3,
            0,
            0,
            2,
            3,
            8,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 2, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 2, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 16];
        loaded.memory.read_bytes_into(SOURCE, &mut actual).unwrap();
        assert_eq!(
            actual,
            [10, 20, 30, 40, 50, 60, 70, 0, 20, 50, 70, 90, 120, 140, 140, 0]
        );
    }

    #[test]
    fn hle_import_runner_copybits_selected_failures_do_not_fallback() {
        const SOURCE: u32 = 0x0930_0000;
        const DESTINATION: u32 = 0x0a30_0000;
        for source_failure in [true, false] {
            let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
            loaded.memory.add_region(
                SOURCE,
                if source_failure {
                    vec![10, 20, 30, 40, 50, 60]
                } else {
                    vec![10, 20, 30, 40, 50, 60, 70, 0]
                },
            );
            loaded.memory.add_region(DESTINATION, vec![0xa5; 4]);
            if !source_failure {
                loaded
                    .memory
                    .add_readonly_region(DESTINATION, vec![0xa5; 3]);
            }
            let records = PPC_HEAP_BASE + 0x161b0;
            let src_pixmap = records;
            let dst_pixmap = records + 0x40;
            let rects = records + 0x80;
            loaded.memory.add_region(records, vec![0; 0x90]);
            ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 8, 0, 0, 1, 8, 8).unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                dst_pixmap,
                DESTINATION,
                3,
                0,
                0,
                1,
                3,
                8,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 7).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 3).unwrap();
            loaded.cpu.gpr[3] = src_pixmap;
            loaded.cpu.gpr[4] = dst_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = [0; 4];
            loaded
                .memory
                .read_bytes_into(DESTINATION, &mut actual)
                .unwrap();
            assert_eq!(actual, [0xa5; 4], "source_failure={source_failure}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_selected_refusal_preserves_later_rows() {
        const SOURCE: u32 = 0x0940_0000;
        const DESTINATION: u32 = 0x0a40_0000;
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
        loaded.memory.add_region(
            SOURCE,
            vec![
                10, 20, 30, 40, 50, 60, 70, 0, 80, 90, 100, 110, 120, 130, 140, 0,
            ],
        );
        loaded.memory.add_region(DESTINATION, vec![0xa5; 6]);
        loaded
            .memory
            .add_readonly_region(DESTINATION + 3, vec![0xa5; 3]);
        let records = PPC_HEAP_BASE + 0x16240;
        let src_pixmap = records;
        let dst_pixmap = records + 0x40;
        let rects = records + 0x80;
        loaded.memory.add_region(records, vec![0; 0x90]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, SOURCE, 8, 0, 0, 2, 8, 8).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            dst_pixmap,
            DESTINATION,
            3,
            0,
            0,
            2,
            3,
            8,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 2, 7).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 2, 3).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rects;
        loaded.cpu.gpr[6] = rects + 8;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut actual = [0; 6];
        loaded
            .memory
            .read_bytes_into(DESTINATION, &mut actual)
            .unwrap();
        assert_eq!(actual, [20, 50, 70, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
    fn hle_import_runner_copybits_vertical_failures_are_terminal_and_row_atomic() {
        const SOURCE: u32 = 0x0960_0000;
        const DESTINATION: u32 = 0x0a60_0000;
        for failure in [0, 1, 2] {
            let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
            loaded.memory.add_region(
                SOURCE,
                if failure == 0 {
                    vec![1, 11, 4, 14, 3, 13, 8, 18]
                } else {
                    vec![1, 11, 4, 14, 3, 13, 8, 18, 2, 12]
                },
            );
            loaded.memory.add_region(DESTINATION, vec![0xa5; 6]);
            match failure {
                0 => {}
                1 => loaded
                    .memory
                    .add_readonly_region(DESTINATION, vec![0xa5; 6]),
                2 => loaded
                    .memory
                    .add_readonly_region(DESTINATION + 2, vec![0xa5; 4]),
                _ => unreachable!(),
            }
            let records = PPC_HEAP_BASE + 0x17800;
            let source_pixmap = records;
            let destination_pixmap = records + 0x40;
            let rects = records + 0x80;
            loaded.memory.add_region(records, vec![0; 0x90]);
            ppc_write_pixmap(&mut loaded.memory, source_pixmap, SOURCE, 2, 0, 0, 5, 2, 8).unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                destination_pixmap,
                DESTINATION,
                2,
                0,
                0,
                3,
                2,
                8,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, rects, 0, 0, 5, 2).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 3, 2).unwrap();
            loaded.cpu.gpr[3] = source_pixmap;
            loaded.cpu.gpr[4] = destination_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = [0; 6];
            loaded
                .memory
                .read_bytes_into(DESTINATION, &mut actual)
                .unwrap();
            let expected = if failure == 2 {
                [1, 11, 0xa5, 0xa5, 0xa5, 0xa5]
            } else {
                [0xa5; 6]
            };
            assert_eq!(actual, expected, "failure={failure}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_snapshots_indexed_vertical_aliases() {
        const SOURCE: u32 = 0x0970_0000;
        const DESTINATION: u32 = 0x0a70_0000;
        for offset in [0, 2] {
            let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyBits")).unwrap();
            let backing = crate::memory::bus::SharedRamRegion::from_owned_bytes(vec![
                1, 11, 4, 14, 3, 13, 8, 18, 2, 12,
            ]);
            // SAFETY: CopyBits accesses the aliases serially and retains no
            // borrowed slice across a memory operation.
            unsafe {
                loaded.memory.add_shared_region(SOURCE, backing.clone());
                loaded.memory.add_shared_region(DESTINATION, backing);
            }
            let records = PPC_HEAP_BASE + 0x17a00;
            let source_pixmap = records;
            let destination_pixmap = records + 0x40;
            let rects = records + 0x80;
            loaded.memory.add_region(records, vec![0; 0x90]);
            ppc_write_pixmap(&mut loaded.memory, source_pixmap, SOURCE, 2, 0, 0, 5, 2, 8).unwrap();
            ppc_write_pixmap(
                &mut loaded.memory,
                destination_pixmap,
                if offset == 0 {
                    SOURCE
                } else {
                    DESTINATION + offset
                },
                2,
                0,
                0,
                3,
                2,
                8,
            )
            .unwrap();
            ppc_write_rect(&mut loaded.memory, rects, 0, 0, 5, 2).unwrap();
            ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 3, 2).unwrap();
            loaded.cpu.gpr[3] = source_pixmap;
            loaded.cpu.gpr[4] = destination_pixmap;
            loaded.cpu.gpr[5] = rects;
            loaded.cpu.gpr[6] = rects + 8;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let mut actual = [0; 10];
            loaded.memory.read_bytes_into(SOURCE, &mut actual).unwrap();
            let expected = if offset == 0 {
                [1, 11, 4, 14, 8, 18, 8, 18, 2, 12]
            } else {
                [1, 11, 1, 11, 4, 14, 8, 18, 2, 12]
            };
            assert_eq!(actual, expected, "offset={offset}");
        }
    }

    #[test]
    fn hle_import_runner_copybits_matches_colors_between_indexed_pixmaps() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13000;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let src_ctable_handle = scratch + 0xa0;
        let src_ctable = scratch + 0xb0;
        let rect = scratch + 0xd0;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded.memory.write_u32_be(src_ctable, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 8, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 10, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 12, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 14, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 16, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 18, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 20, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 22, 0).unwrap();
        loaded.memory.write_u8(src_pixels, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.memory.read_u8(dst_pixels),
            Some(pict::closest_clut_index(
                0xffff,
                0,
                0,
                &loaded.color_manager_clut,
            ))
        );
    }

    #[test]
    fn hle_import_runner_copybits_preserves_device_table_indices() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13300;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let src_ctable_handle = scratch + 0xa0;
        let src_ctable = scratch + 0xb0;
        let rect = scratch + 0xd0;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0x8000).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 8, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 10, 0xffff).unwrap();
        loaded.memory.write_u16_be(src_ctable + 16, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 18, 0xffff).unwrap();
        loaded.memory.write_u8(src_pixels, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        // A device ColorTable's RGB may change independently during palette
        // animation. The source pixel is already a device index.
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(1));
    }

    #[test]
    fn copybits_uses_current_gdevice_table_not_destination_pixmap_table() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x15000;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let src_ctable_handle = scratch + 0xa0;
        let src_ctable = scratch + 0xb0;
        let dst_ctable_handle = scratch + 0xd0;
        let dst_ctable = scratch + 0xe0;
        let gdevice_handle = scratch + 0x900;
        let gdevice = scratch + 0x910;
        let device_pixmap_handle = scratch + 0x950;
        let device_pixmap = scratch + 0x960;
        let device_ctable_handle = scratch + 0x9a0;
        let device_ctable = scratch + 0x9b0;
        let rect = scratch + 0x1300;
        loaded.memory.add_region(scratch, vec![0; 0x1400]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, dst_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_ctable_handle, dst_ctable)
            .unwrap();
        loaded.memory.write_u16_be(src_ctable + 4, 0).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 16, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 18, 0xffff).unwrap();
        loaded.memory.write_u16_be(dst_ctable + 4, 0x8000).unwrap();
        loaded.memory.write_u16_be(dst_ctable + 6, 255).unwrap();
        for index in 0..256u32 {
            loaded
                .memory
                .write_u16_be(dst_ctable + 8 + index * 8, index as u16)
                .unwrap();
        }
        loaded
            .memory
            .write_u16_be(dst_ctable + 8 + 42 * 8 + 2, 0xffff)
            .unwrap();

        loaded.memory.write_u32_be(gdevice_handle, gdevice).unwrap();
        loaded
            .memory
            .write_u32_be(gdevice + 22, device_pixmap_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_pixmap_handle, device_pixmap)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_pixmap + 42, device_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(device_ctable_handle, device_ctable)
            .unwrap();
        loaded
            .memory
            .write_u16_be(device_ctable + 4, 0x8000)
            .unwrap();
        loaded.memory.write_u16_be(device_ctable + 6, 255).unwrap();
        for index in 0..256u32 {
            loaded
                .memory
                .write_u16_be(device_ctable + 8 + index * 8, index as u16)
                .unwrap();
        }
        loaded
            .memory
            .write_u16_be(device_ctable + 8 + 77 * 8 + 2, 0xffff)
            .unwrap();
        loaded.memory.write_u8(src_pixels, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = gdevice_handle);
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(77));
    }

    #[test]
    fn copybits_mono_expansion_uses_explicit_foreground_index() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13400;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_bitmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x40;
        let rect = scratch + 0x80;
        loaded.memory.add_region(scratch, vec![0; 0xa0]);
        loaded.memory.write_u8(src_pixels, 0x80).unwrap();
        loaded.memory.write_u32_be(src_bitmap, src_pixels).unwrap();
        loaded.memory.write_u16_be(src_bitmap + 4, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, src_bitmap + 6, 0, 0, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 8, 0, 0, 1, 8, 8).unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 8).unwrap();
        loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);
        loaded.cpu.gpr[3] = src_bitmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 36;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(103));
        assert_eq!(loaded.memory.read_u8(dst_pixels + 1), Some(0));
    }

    #[test]
    fn hle_import_runner_copybits_honors_palette_index_color_tables() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13500;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let src_ctable_handle = scratch + 0xa0;
        let src_ctable = scratch + 0xb0;
        let rect = scratch + 0xd0;
        let palette_handle = scratch + 0xe0;
        let palette = scratch + 0xf0;
        let dst_ctable_handle = scratch + 0x140;
        let dst_ctable = scratch + 0x150;
        loaded.memory.add_region(scratch, vec![0; 0x180]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, src_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, dst_ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(src_ctable_handle, src_ctable)
            .unwrap();
        loaded.memory.write_u32_be(src_ctable, 0x1357_2468).unwrap();
        let device_clut = ppc_copy_bits_clut(
            &mut loaded.memory,
            PPC_MAIN_CTABLE_HANDLE,
            &loaded.color_manager_clut,
        );
        loaded.memory.write_u16_be(src_ctable + 4, 0x4000).unwrap();
        loaded.memory.write_u16_be(src_ctable + 6, 1).unwrap();
        loaded.memory.write_u16_be(src_ctable + 8, 1).unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 10, device_clut[1][0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 12, device_clut[1][1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 14, device_clut[1][2])
            .unwrap();
        loaded.memory.write_u16_be(src_ctable + 16, 0).unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 18, device_clut[0][0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 20, device_clut[0][1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(src_ctable + 22, device_clut[0][2])
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_ctable_handle, dst_ctable)
            .unwrap();
        let linked_ctable = ppc_copy_color_table_bytes(&mut loaded.memory, src_ctable_handle)
            .expect("linked source ColorTable should be readable");
        loaded
            .memory
            .write_bytes(dst_ctable, &linked_ctable)
            .unwrap();
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 2).unwrap();
        loaded
            .memory
            .write_u16_be(palette + 16 + 6, 0x0008)
            .unwrap();
        loaded
            .memory
            .write_u16_be(palette + 32 + 6, 0x0008)
            .unwrap();
        for (entry, color) in [device_clut[1], device_clut[0]].into_iter().enumerate() {
            loaded
                .memory
                .write_u16_be(palette + 16 + entry as u32 * 16, color[0])
                .unwrap();
            loaded
                .memory
                .write_u16_be(palette + 18 + entry as u32 * 16, color[1])
                .unwrap();
            loaded
                .memory
                .write_u16_be(palette + 20 + entry as u32 * 16, color[2])
                .unwrap();
        }
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        assert_eq!(
            ppc_copy_bits_linked_palette_clut(
                &mut loaded.memory,
                src_ctable_handle,
                PPC_MAIN_GWORLD,
                PPC_MAIN_GDEVICE,
                &loaded.toolbox_startup,
                &loaded.color_manager_clut,
            ),
            Some(device_clut)
        );
        loaded.memory.write_u8(src_pixels, 0).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(src_ctable), Some(0x1357_2468));
        assert_eq!(loaded.memory.read_u32_be(dst_ctable), Some(0x1357_2468));
        assert_eq!(loaded.memory.read_u8(dst_pixels), Some(1));

        loaded.memory.write_u16_be(src_ctable + 4, 0xc000).unwrap();
        assert_eq!(
            ppc_copy_bits_palette_index_map(
                &mut loaded.memory,
                src_ctable_handle,
                PPC_MAIN_GWORLD,
                PPC_MAIN_GDEVICE,
                &loaded.toolbox_startup,
                &device_clut,
            ),
            None
        );
    }

    #[test]
    fn linked_copybits_table_resolves_palette_usage_and_rgb_fallbacks() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13700;
        let ctable_handle = scratch;
        let ctable = scratch + 0x10;
        let palette_handle = scratch + 0x60;
        let palette = scratch + 0x70;
        loaded.memory.add_region(scratch, vec![0; 0xe0]);
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 4, 0x4000).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 5).unwrap();
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 5).unwrap();
        loaded
            .memory
            .write_u32_be(
                PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET,
                palette_handle,
            )
            .unwrap();
        let mut dst_clut = [[0; 3]; 256];
        let colors = [
            [0x1000, 0x2000, 0x3000],
            [0x2000, 0x3000, 0x4000],
            [0x3000, 0x4000, 0x5000],
            [0x4000, 0x5000, 0x6000],
            [0x5000, 0x6000, 0x7000],
        ];
        for (entry, (usage, color)) in [0x0000, 0x0002, 0x0008, 0x0004, 0x000c]
            .into_iter()
            .zip(colors)
            .enumerate()
        {
            let info = palette + 16 + entry as u32 * 16;
            ppc_write_rgb_color(
                &mut loaded.memory,
                info,
                PpcRgbColor {
                    red: color[0],
                    green: color[1],
                    blue: color[2],
                },
            )
            .unwrap();
            loaded.memory.write_u16_be(info + 6, usage).unwrap();
            let spec = ctable + 8 + entry as u32 * 8;
            loaded.memory.write_u16_be(spec, entry as u16).unwrap();
            dst_clut[[10, 11, 12, 37, 38][entry]] = color;
        }
        let short_fallback = [0x6666, 0x7777, 0x8888];
        let short_spec = ctable + 8 + 5 * 8;
        loaded.memory.write_u16_be(short_spec, 9).unwrap();
        loaded
            .memory
            .write_u16_be(short_spec + 2, short_fallback[0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(short_spec + 4, short_fallback[1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(short_spec + 6, short_fallback[2])
            .unwrap();
        dst_clut[55] = short_fallback;

        let map = ppc_copy_bits_palette_index_map(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();

        assert_eq!(&map[..6], &[10, 11, 2, 37, 4, 55]);
        let linked_clut = ppc_copy_bits_linked_palette_clut(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();
        assert_eq!(linked_clut[0], colors[0]);
        assert_eq!(linked_clut[5], short_fallback);

        let absent_fallback = [0x9999, 0xaaaa, 0xbbbb];
        loaded
            .memory
            .write_u16_be(ctable + 10, absent_fallback[0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 12, absent_fallback[1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 14, absent_fallback[2])
            .unwrap();
        dst_clut[56] = absent_fallback;
        loaded
            .memory
            .write_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, 0)
            .unwrap();
        let map = ppc_copy_bits_palette_index_map(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();
        assert_eq!(map[0], 56);
        let linked_clut = ppc_copy_bits_linked_palette_clut(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();
        assert_eq!(linked_clut[0], absent_fallback);

        loaded.toolbox_startup.application_palette = palette_handle;
        let map = ppc_copy_bits_palette_index_map(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();
        assert_eq!(map[0], 10, "linked table falls back to application palette");
        let linked_clut = ppc_copy_bits_linked_palette_clut(
            &mut loaded.memory,
            ctable_handle,
            PPC_MAIN_GWORLD,
            PPC_MAIN_GDEVICE,
            &loaded.toolbox_startup,
            &dst_clut,
        )
        .unwrap();
        assert_eq!(linked_clut[0], colors[0]);
    }

    #[test]
    fn linked_copybits_without_an_available_palette_uses_colorspec_rgb() {
        let pef = synthetic_pef_with_import(b"CopyBits");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13a00;
        let src_pixels = scratch;
        let dst_pixels = scratch + 0x10;
        let src_pixmap = scratch + 0x20;
        let dst_pixmap = scratch + 0x60;
        let ctable_handle = scratch + 0xa0;
        let ctable = scratch + 0xb0;
        let rect = scratch + 0xd0;
        let palette_handle = scratch + 0xe0;
        let palette = scratch + 0xf0;
        loaded.memory.add_region(scratch, vec![0; 0x400]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 1, 0, 0, 1, 1, 8).unwrap();
        loaded
            .memory
            .write_u32_be(src_pixmap + 42, ctable_handle)
            .unwrap();
        loaded
            .memory
            .write_u32_be(dst_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 4, 0x4000).unwrap();
        loaded.memory.write_u16_be(ctable + 6, 0).unwrap();
        loaded.memory.write_u16_be(ctable + 8, 24).unwrap();
        let fallback_index = 56usize;
        let fallback = loaded.color_manager_clut[fallback_index];
        loaded
            .memory
            .write_u16_be(ctable + 10, fallback[0])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 12, fallback[1])
            .unwrap();
        loaded
            .memory
            .write_u16_be(ctable + 14, fallback[2])
            .unwrap();
        loaded.memory.write_u32_be(palette_handle, palette).unwrap();
        loaded.memory.write_u16_be(palette, 25).unwrap();
        ppc_write_rgb_color(
            &mut loaded.memory,
            palette + 16 + 24 * 16,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            },
        )
        .unwrap();
        loaded
            .toolbox_startup
            .palette_allocations
            .push(PpcPaletteAllocation {
                palette: palette_handle,
                gdevice: PPC_MAIN_GDEVICE,
                entry_to_index: vec![None; 25],
                entry_mappings: {
                    let mut mappings = vec![PpcPaletteEntryMapping::Unallocated; 25];
                    mappings[24] = PpcPaletteEntryMapping::AnimatedReserved(42);
                    mappings
                },
                reserved_indices: vec![42],
            });
        loaded
            .toolbox_startup
            .active_device_palettes
            .insert(PPC_MAIN_GDEVICE, palette_handle);
        loaded
            .memory
            .write_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, 0)
            .unwrap();
        loaded.toolbox_startup.application_palette = 0;
        loaded.memory.write_u8(src_pixels, 0).unwrap();
        ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = dst_pixmap;
        loaded.cpu.gpr[5] = rect;
        loaded.cpu.gpr[6] = rect;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            loaded.memory.read_u8(dst_pixels),
            Some(fallback_index as u8)
        );
    }

    #[test]
    fn hle_import_runner_copymask_transfers_only_black_mask_pixels() {
        let pef = synthetic_pef_with_import(b"CopyMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13800;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x10;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let mask_bitmap = scratch + 0x80;
        let dst_pixmap = scratch + 0xa0;
        let rects = scratch + 0xe0;
        loaded.memory.add_region(scratch, vec![0; 0x120]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 2, 4, 8).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 0, 0, 2, 4, 8).unwrap();
        loaded
            .memory
            .write_u32_be(mask_bitmap, mask_pixels)
            .unwrap();
        loaded.memory.write_u16_be(mask_bitmap + 4, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, mask_bitmap + 6, 0, 0, 2, 4).unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[42; 8]).unwrap();
        loaded
            .memory
            .write_bytes(mask_pixels, &[0b1010_0000, 0b0101_0000])
            .unwrap();
        for offset in [0, 8, 16] {
            ppc_write_rect(&mut loaded.memory, rects + offset, 0, 0, 2, 4).unwrap();
        }
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_bitmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut result = [0; 8];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut result)
            .unwrap();
        assert_eq!(result, [1, 42, 3, 42, 42, 6, 42, 8]);
    }

    #[test]
    fn hle_import_runner_copymask_preserves_unaligned_2bpp_fields_and_padding() {
        let pef = synthetic_pef_with_import(b"CopyMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13900;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x10;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let mask_bitmap = scratch + 0x80;
        let dst_pixmap = scratch + 0xa0;
        let rects = scratch + 0xe0;
        loaded.memory.add_region(scratch, vec![0; 0x120]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 1, 9, 2).unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 0, 0, 1, 9, 2).unwrap();
        loaded
            .memory
            .write_u32_be(mask_bitmap, mask_pixels)
            .unwrap();
        loaded.memory.write_u16_be(mask_bitmap + 4, 3).unwrap();
        ppc_write_rect(&mut loaded.memory, mask_bitmap + 6, 0, 0, 1, 9).unwrap();

        // Source indexes are [0,2,3,0,3,2,0,2,3]. The destination begins
        // entirely at index 1, including the two visible boundary pixels.
        // Both rows reserve a fourth byte as scanline padding.
        loaded
            .memory
            .write_bytes(src_pixels, &[0x2c, 0xe2, 0xc0, 0x9a])
            .unwrap();
        loaded
            .memory
            .write_bytes(dst_pixels, &[0x55, 0x55, 0x55, 0xcc])
            .unwrap();
        // Copy x=1,3,4,7. x=0 and x=8 are outside the unaligned rectangles;
        // the low seven bits of the second mask byte are deliberately nonzero
        // tail bits and the third byte is mask-row padding.
        loaded
            .memory
            .write_bytes(mask_pixels, &[0b0101_1001, 0b0010_1101, 0xee])
            .unwrap();
        for offset in [0, 8, 16] {
            ppc_write_rect(&mut loaded.memory, rects + offset, 0, 1, 1, 8).unwrap();
        }
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_bitmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut result = [0; 4];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut result)
            .unwrap();
        assert_eq!(result, [0x64, 0xd6, 0x55, 0xcc]);
        let dst_bits = ppc_resolve_pixmap_bits(&mut loaded.memory, &loaded.gworlds, dst_pixmap)
            .expect("destination PixMap should remain live");
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut loaded.memory, dst_bits, 0, 0),
            Some(1),
            "left neighbour must survive the packed read-modify-write"
        );
        assert_eq!(
            loaded.memory.read_u8(dst_pixels + 2).unwrap() & 0x3f,
            0x15,
            "unused tail fields must survive the packed read-modify-write"
        );
        assert_eq!(loaded.memory.read_u8(dst_pixels + 3), Some(0xcc));
    }

    #[test]
    fn hle_import_runner_copydeepmask_scales_deep_mask_and_region_clip() {
        let pef = synthetic_pef_with_import(b"CopyDeepMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13a00;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x10;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let mask_pixmap = scratch + 0x80;
        let dst_pixmap = scratch + 0xc0;
        let rects = scratch + 0x100;
        let region_handle = scratch + 0x120;
        let region_ptr = scratch + 0x124;
        loaded.memory.add_region(scratch, vec![0; 0x180]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 1, 4, 8)
            .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            mask_pixmap,
            mask_pixels,
            8,
            0,
            0,
            1,
            8,
            8,
        )
        .unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 8, 0, 0, 1, 8, 8)
            .unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[10, 20, 30, 40])
            .unwrap();
        loaded
            .memory
            .write_bytes(mask_pixels, &[0xff; 8])
            .unwrap();
        loaded
            .memory
            .write_bytes(dst_pixels, &[42; 8])
            .unwrap();
        loaded
            .memory
            .write_u32_be(region_handle, region_ptr)
            .unwrap();
        ppc_set_rect_rgn(&mut loaded.memory, region_handle, 4, 0, 8, 1).unwrap();
        ppc_write_rect(&mut loaded.memory, rects, 0, 0, 1, 4).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 1, 8).unwrap();
        ppc_write_rect(&mut loaded.memory, rects + 16, 0, 0, 1, 8).unwrap();
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_pixmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;
        loaded.cpu.gpr[9] = 0; // srcCopy
        loaded.cpu.gpr[10] = region_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut result = [0; 8];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut result)
            .unwrap();
        assert_eq!(
            result,
            [42, 42, 42, 42, 30, 30, 40, 40],
            "CopyDeepMask should scale the 8-bit source and honor maskRgn"
        );
    }

    #[test]
    fn hle_import_runner_copydeepmask_blends_direct_color_with_weighted_mask() {
        let pef = synthetic_pef_with_import(b"CopyDeepMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13c00;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x10;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let mask_pixmap = scratch + 0x80;
        let dst_pixmap = scratch + 0xc0;
        let rects = scratch + 0x100;
        loaded.memory.add_region(scratch, vec![0; 0x180]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 2, 0, 0, 1, 1, 16)
            .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            mask_pixmap,
            mask_pixels,
            2,
            0,
            0,
            1,
            1,
            16,
        )
        .unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 2, 0, 0, 1, 1, 16)
            .unwrap();
        let source_pixel = 0x7c00u16; // red in RGB555
        let destination_pixel = 0x001fu16; // blue in RGB555
        let mask_pixel = 0x4210u16; // a non-extreme, approximately half-weight mask
        loaded.memory.write_u16_be(src_pixels, source_pixel).unwrap();
        loaded.memory.write_u16_be(mask_pixels, mask_pixel).unwrap();
        loaded
            .memory
            .write_u16_be(dst_pixels, destination_pixel)
            .unwrap();
        for offset in [0, 8, 16] {
            ppc_write_rect(&mut loaded.memory, rects + offset, 0, 0, 1, 1).unwrap();
        }
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_pixmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;
        loaded.cpu.gpr[9] = 0; // srcCopy
        loaded.cpu.gpr[10] = 0; // no maskRgn

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let source_rgb = ppc_rgb555_to_rgb16(source_pixel);
        let destination_rgb = ppc_rgb555_to_rgb16(destination_pixel);
        let mask_rgb = ppc_rgb555_to_rgb16(mask_pixel);
        let expected_rgb = ppc_blend_deep_mask_rgb(source_rgb, destination_rgb, mask_rgb);
        let expected = ppc_rgb_color_to_rgb555(PpcRgbColor {
            red: expected_rgb[0],
            green: expected_rgb[1],
            blue: expected_rgb[2],
        });
        assert_eq!(
            loaded.memory.read_u16_be(dst_pixels),
            Some(expected),
            "deep masks must blend each direct-color component instead of treating nonzero as opaque"
        );
        assert_ne!(expected, source_pixel);
        assert_ne!(expected, destination_pixel);
    }

    #[test]
    fn hle_import_runner_copymask_blends_through_a_deep_mask() {
        // Imaging With QuickDraw (1994), pp. 3-119--3-122: a pixel-map mask
        // weights the average of source and destination by its color.
        let pef = synthetic_pef_with_import(b"CopyMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13c00;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x10;
        let dst_pixels = scratch + 0x20;
        let src_pixmap = scratch + 0x40;
        let mask_pixmap = scratch + 0x80;
        let dst_pixmap = scratch + 0xc0;
        let rects = scratch + 0x100;
        loaded.memory.add_region(scratch, vec![0; 0x180]);
        for (pixmap, pixels) in [
            (src_pixmap, src_pixels),
            (mask_pixmap, mask_pixels),
            (dst_pixmap, dst_pixels),
        ] {
            ppc_write_pixmap(&mut loaded.memory, pixmap, pixels, 6, 0, 0, 1, 3, 16).unwrap();
        }
        let source_pixel = 0x7c00u16; // red in RGB555
        let destination_pixel = 0x001fu16; // blue in RGB555
        let grey_mask = 0x4210u16;
        for (index, mask) in [0x0000u16, 0x7fff, grey_mask].into_iter().enumerate() {
            let offset = index as u32 * 2;
            loaded.memory.write_u16_be(src_pixels + offset, source_pixel).unwrap();
            loaded.memory.write_u16_be(mask_pixels + offset, mask).unwrap();
            loaded
                .memory
                .write_u16_be(dst_pixels + offset, destination_pixel)
                .unwrap();
        }
        for offset in [0, 8, 16] {
            ppc_write_rect(&mut loaded.memory, rects + offset, 0, 0, 1, 3).unwrap();
        }
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_pixmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let blend = ppc_blend_deep_mask_rgb(
            ppc_rgb555_to_rgb16(source_pixel),
            ppc_rgb555_to_rgb16(destination_pixel),
            ppc_rgb555_to_rgb16(grey_mask),
        );
        let blended = ppc_rgb_color_to_rgb555(PpcRgbColor {
            red: blend[0],
            green: blend[1],
            blue: blend[2],
        });
        let result: Vec<_> = (0..3)
            .map(|index| loaded.memory.read_u16_be(dst_pixels + index * 2))
            .collect();
        assert_eq!(
            result,
            [Some(source_pixel), Some(destination_pixel), Some(blended)],
            "black copies the source, white keeps the destination, grey blends"
        );
    }

    #[test]
    fn hle_import_runner_copydeepmask_preserves_transparent_and_region_excluded_pixels() {
        let pef = synthetic_pef_with_import(b"CopyDeepMask");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x13e00;
        let src_pixels = scratch;
        let mask_pixels = scratch + 0x20;
        let dst_pixels = scratch + 0x40;
        let src_pixmap = scratch + 0x80;
        let mask_pixmap = scratch + 0xc0;
        let dst_pixmap = scratch + 0x100;
        let rects = scratch + 0x140;
        let region_handle = scratch + 0x160;
        let region_ptr = scratch + 0x164;
        loaded.memory.add_region(scratch, vec![0; 0x220]);
        ppc_write_pixmap(&mut loaded.memory, src_pixmap, src_pixels, 4, 0, 0, 2, 4, 8)
            .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            mask_pixmap,
            mask_pixels,
            4,
            0,
            0,
            2,
            4,
            8,
        )
        .unwrap();
        ppc_write_pixmap(&mut loaded.memory, dst_pixmap, dst_pixels, 4, 0, 0, 2, 4, 8)
            .unwrap();
        loaded
            .memory
            .write_bytes(src_pixels, &[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap();
        // Index 255 is black in the canonical 8-bit ColorTable (opaque); 0
        // is white (transparent for the one-bit-style semantic at this
        // endpoint), so the second pixel exercises an in-region transparent
        // mask while the region shape exercises exact membership.
        loaded
            .memory
            .write_bytes(mask_pixels, &[255, 0, 255, 255, 255, 255, 255, 255])
            .unwrap();
        loaded.memory.write_bytes(dst_pixels, &[42; 8]).unwrap();
        loaded
            .memory
            .write_u32_be(region_handle, region_ptr)
            .unwrap();
        let rows = vec![vec![0, 2], vec![1, 4]];
        let storage = ppc_region_storage_from_rows(0, &rows).unwrap();
        loaded.memory.write_bytes(region_ptr, &storage).unwrap();
        for offset in [0, 8, 16] {
            ppc_write_rect(&mut loaded.memory, rects + offset, 0, 0, 2, 4).unwrap();
        }
        loaded.cpu.gpr[3] = src_pixmap;
        loaded.cpu.gpr[4] = mask_pixmap;
        loaded.cpu.gpr[5] = dst_pixmap;
        loaded.cpu.gpr[6] = rects;
        loaded.cpu.gpr[7] = rects + 8;
        loaded.cpu.gpr[8] = rects + 16;
        loaded.cpu.gpr[9] = 0; // srcCopy
        loaded.cpu.gpr[10] = region_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let mut result = [0; 8];
        loaded
            .memory
            .read_bytes_into(dst_pixels, &mut result)
            .unwrap();
        assert_eq!(
            result,
            [1, 42, 42, 42, 42, 6, 7, 8],
            "transparent mask pixels and points outside the exact L-shaped maskRgn must preserve dst"
        );
        assert!(ppc_point_in_region_storage(&storage, 0, 0));
        assert!(!ppc_point_in_region_storage(&storage, 0, 2));
        assert!(ppc_point_in_region_storage(&storage, 1, 3));
        assert!(!ppc_point_in_region_storage(&storage, 1, 0));
    }
