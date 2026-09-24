use super::*;
use crate::cpu::{CpuOps, Register};
use crate::trap::test_helpers::MockCpu;

#[test]
fn hle_import_runner_converts_one_bit_bitmaps_to_regions() {
    let pef = synthetic_pef_with_import(b"BitMapToRegion");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bitmap = PPC_DATA_BASE + 0x1000;
    let pixels = bitmap + 0x40;
    loaded.memory.add_region(bitmap, vec![0; 0x80]);
    loaded.memory.write_u32_be(bitmap, pixels).unwrap();
    loaded.memory.write_u16_be(bitmap + 4, 1).unwrap();
    ppc_write_rect(&mut loaded.memory, bitmap + 6, 0, 0, 1, 8).unwrap();
    loaded.memory.write_u8(pixels, 0b1011_0000).unwrap();
    let region = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    loaded.cpu.gpr[3] = region;
    loaded.cpu.gpr[4] = bitmap;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, region),
        Some((0, 0, 1, 4))
    );
    let storage = ppc_region_storage(&mut loaded.memory, region).unwrap();
    assert_eq!(
        ppc_region_rows_for_band(&storage, 0, 1),
        Some(vec![vec![0, 1, 2, 4]])
    );

    loaded.memory.write_u16_be(bitmap + 4, 0x8001).unwrap();
    loaded.memory.write_u16_be(bitmap + 32, 8).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = region;
    loaded.cpu.gpr[4] = bitmap;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PIXMAP_TOO_DEEP_ERR));
}

#[test]
fn hle_import_runner_handles_region_basics() {
    let pef = synthetic_pef_with_import(b"NewRgn");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let rgn_handle = loaded.cpu.gpr[3];
    assert_ne!(rgn_handle, 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let rgn_ptr = loaded.memory.read_u32_be(rgn_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(rgn_ptr), Some(10));
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle),
        Some((0, 0, 0, 0))
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = rgn_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetRectRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = rgn_handle;
    loaded.cpu.gpr[4] = 10;
    loaded.cpu.gpr[5] = 20;
    loaded.cpu.gpr[6] = 110;
    loaded.cpu.gpr[7] = 220;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle),
        Some((20, 10, 220, 110))
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = rgn_handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn open_and_close_rgn_record_guest_outline_bounds() {
    let pef = synthetic_pef_with_import(b"NewRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let destination = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    assert_ne!(destination, 0);

    let mut handle_states = loaded.handle_states();
    ppc_open_rgn(
        None,
        &mut loaded.memory,
        PPC_MAIN_GWORLD,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);
    assert_ne!(loaded.toolbox_startup.open_region_save_handle, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_RGN_SAVE_OFFSET),
        Some(loaded.toolbox_startup.open_region_save_handle)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PN_VIS_OFFSET),
        Some(u16::MAX)
    );
    for (h, v) in [(10, 20), (110, 20), (110, 70), (10, 70), (10, 20)] {
        ppc_open_region_include_point(&mut loaded.toolbox_startup, h, v);
    }

    let mut handle_states = loaded.handle_states();
    ppc_close_rgn(
        None,
        &mut loaded.memory,
        destination,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);

    assert_eq!(last_mem_error, PPC_NO_ERR);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, destination),
        Some((20, 10, 70, 110))
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_RGN_SAVE_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PN_VIS_OFFSET),
        Some(0)
    );
    assert_eq!(loaded.toolbox_startup.open_region_port, 0);
}

#[test]
fn open_and_close_rgn_preserve_powerpc_curved_shape_rows() {
    let pef = synthetic_pef_with_import(b"FrameOval");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, scratch, 10, 10, 30, 30).unwrap();
    ppc_write_rect(&mut loaded.memory, scratch + 8, 40, 10, 60, 50).unwrap();

    let oval = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    let mut handle_states = loaded.handle_states();
    ppc_open_rgn(
        None,
        &mut loaded.memory,
        PPC_MAIN_GWORLD,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);
    loaded.cpu.gpr[3] = scratch;
    loaded.run_with_hle_imports(64);
    let mut handle_states = loaded.handle_states();
    ppc_close_rgn(
        None,
        &mut loaded.memory,
        oval,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);
    assert!(ppc_point_in_region(&mut loaded.memory, oval, 20, 20));
    assert!(!ppc_point_in_region(&mut loaded.memory, oval, 10, 10));
    assert!(
        loaded
            .memory
            .read_u32_be(oval)
            .and_then(|ptr| loaded.memory.read_u16_be(ptr))
            .unwrap()
            > 10
    );

    let rounded = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    let mut handle_states = loaded.handle_states();
    ppc_open_rgn(
        None,
        &mut loaded.memory,
        PPC_MAIN_GWORLD,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FrameRoundRect;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = 12;
    loaded.cpu.gpr[5] = 12;
    loaded.run_with_hle_imports(64);
    let mut handle_states = loaded.handle_states();
    ppc_close_rgn(
        None,
        &mut loaded.memory,
        rounded,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        Some(&mut handle_states),
        &mut loaded.toolbox_startup,
    );
    loaded.replace_handle_states(handle_states);
    assert!(ppc_point_in_region(&mut loaded.memory, rounded, 50, 10));
    assert!(!ppc_point_in_region(&mut loaded.memory, rounded, 40, 10));
    assert!(
        loaded
            .memory
            .read_u32_be(rounded)
            .and_then(|ptr| loaded.memory.read_u16_be(ptr))
            .unwrap()
            > 10
    );
}

#[test]
fn powerpc_openpicture_records_and_replays_dynamic_commands() {
    // Imaging With QuickDraw (1994), pp. 7-39--7-45: commands issued
    // between OpenPicture and ClosePicture are hidden, retained, and replayed.
    let pef = synthetic_pef_with_import(b"OpenPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1800;
    let frame = scratch;
    let color = scratch + 8;
    let text = scratch + 16;
    let destination = scratch + 32;
    loaded.memory.add_region(scratch, vec![0; 64]);
    ppc_write_rect(&mut loaded.memory, frame, 0, 0, 40, 40).unwrap();
    ppc_write_rgb_color(
        &mut loaded.memory,
        color,
        PpcRgbColor {
            red: 0xEEEE,
            green: 0x7777,
            blue: 0x1111,
        },
    )
    .unwrap();
    loaded.memory.write_bytes(text, b"\x04PICT").unwrap();
    ppc_write_rect(&mut loaded.memory, destination, 50, 50, 90, 90).unwrap();
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, *loaded.current_gworld)
            .unwrap();
    let before = ppc_quickdraw_read_pixel(
        &mut loaded.memory,
        surface.front_buffer,
        surface.local_point((20, 20)),
    );

    loaded.cpu.gpr[3] = frame;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::OpenPicture,
        ),
    );
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    loaded.cpu.gpr[3] = color;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RGBForeColor);
    loaded.cpu.gpr[3] = frame;
    loaded.cpu.gpr[4] = 10;
    loaded.cpu.gpr[5] = 10;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::PaintRoundRect);
    loaded.cpu.gpr[3] = frame;
    loaded.cpu.gpr[4] = 10;
    loaded.cpu.gpr[5] = 10;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FrameRoundRect);
    loaded.cpu.gpr[3] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextFont);
    loaded.cpu.gpr[3] = 9;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextSize);
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextFace);
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 25;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveTo);
    loaded.cpu.gpr[3] = text;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawString);
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            surface.front_buffer,
            surface.local_point((20, 20)),
        ),
        before,
        "recording must not paint into the live port"
    );

    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::ClosePicture,
        ),
    );
    let picture =
        ppc_handle_bytes(&mut loaded.memory, &test_handle_records!(loaded), handle).unwrap();
    for opcode in [[0x00, 0x41], [0x00, 0x40], [0x00, 0x28]] {
        assert!(picture.windows(2).any(|word| word == opcode));
    }
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = destination;
    assert!(with_test_color_manager_clut!(
        loaded,
        |color_manager_clut| ppc_draw_picture(
            &mut loaded.cpu,
            &mut loaded.memory,
            &test_handle_records!(loaded),
            &loaded.process_file_system.vfs_resources,
            &loaded.gworlds,
            *loaded.current_gworld,
            &loaded.screen_clut,
            color_manager_clut,
        )
    ));
    assert_ne!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            surface.front_buffer,
            surface.local_point((70, 70)),
        ),
        before
    );
}

#[test]
fn hle_import_runner_handles_rect_rgn() {
    let pef = synthetic_pef_with_import(b"RectRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rgn_ptr = PPC_DATA_BASE + 0x1000;
    let rgn_handle = PPC_DATA_BASE + 0x1100;
    let rect_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(rgn_ptr, vec![0; 10]);
    loaded.memory.add_region(rgn_handle, vec![0; 4]);
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    loaded.memory.write_u32_be(rgn_handle, rgn_ptr).unwrap();
    ppc_write_rect(&mut loaded.memory, rect_ptr, 3, 4, 30, 40).unwrap();
    loaded.cpu.gpr[3] = rgn_handle;
    loaded.cpu.gpr[4] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(rgn_ptr), Some(10));
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle),
        Some((3, 4, 30, 40))
    );
}

#[test]
fn hle_import_runner_copies_complete_region_storage() {
    let pef = synthetic_pef_with_import(b"NewRgn");
    let mut loaded = load_pef_application(&pef).unwrap();

    loaded.run_with_hle_imports(64);
    let src_rgn = loaded.cpu.gpr[3];
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.run_with_hle_imports(64);
    let dst_rgn = loaded.cpu.gpr[3];

    assert_eq!(
        ppc_set_handle_size(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            src_rgn,
            16,
        ),
        PPC_NO_ERR
    );
    let src_ptr = loaded.memory.read_u32_be(src_rgn).unwrap();
    let region = [0, 16, 0, 1, 0, 2, 0, 20, 0, 30, 0, 3, 0, 10, 0x7f, 0xff];
    for (offset, byte) in region.into_iter().enumerate() {
        loaded
            .memory
            .write_u8(src_ptr + offset as u32, byte)
            .unwrap();
    }

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::CopyRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = src_rgn;
    loaded.cpu.gpr[4] = dst_rgn;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let dst_ptr = loaded.memory.read_u32_be(dst_rgn).unwrap();
    assert_ne!(src_ptr, dst_ptr);
    let copied = (0..16)
        .map(|offset| loaded.memory.read_u8(dst_ptr + offset).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(copied, region);

    loaded.memory.write_u8(src_ptr + 10, 0xaa).unwrap();
    assert_eq!(loaded.memory.read_u8(dst_ptr + 10), Some(0));
}

#[test]
fn hle_import_runner_unions_disjoint_rectangles_as_complex_region() {
    let pef = synthetic_pef_with_import(b"NewRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut regions = Vec::new();
    for _ in 0..3 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.run_with_hle_imports(64);
        regions.push(loaded.cpu.gpr[3]);
    }
    ppc_write_rgn_bbox(&mut loaded.memory, regions[0], 0, 0, 2, 2).unwrap();
    ppc_write_rgn_bbox(&mut loaded.memory, regions[1], 0, 3, 2, 5).unwrap();

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UnionRgn;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = regions[0];
    loaded.cpu.gpr[4] = regions[1];
    loaded.cpu.gpr[5] = regions[2];
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let storage = ppc_region_storage(&mut loaded.memory, regions[2]).unwrap();
    assert!(storage.len() > 10);
    assert_eq!(ppc_region_storage_bbox(&storage), Some((0, 0, 2, 5)));
    assert_eq!(
        ppc_region_rows_for_band(&storage, 0, 2),
        Some(vec![vec![0, 2, 3, 5], vec![0, 2, 3, 5]])
    );
}

#[test]
fn hle_import_runner_draws_quickdraw_color_rects_into_current_gworld() {
    let pef = synthetic_pef_with_import(b"ForeColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let color_ptr = scratch;
    let rect_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 32]);

    loaded.cpu.gpr[3] = 205; // redColor
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_fore_color, ppc_legacy_qd_color_to_rgb(205));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetForeColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rgb_color(&mut loaded.memory, color_ptr),
        Some(ppc_legacy_qd_color_to_rgb(205))
    );

    let green = PpcRgbColor {
        red: 0,
        green: 0xffff,
        blue: 0,
    };
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, green).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RGBForeColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_fore_color, green);

    ppc_write_rect(&mut loaded.memory, rect_ptr, 1, 2, 4, 5).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PaintRect;
    loaded.cpu.gpr[3] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 1)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(green)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (4, 3)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(green)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 1)),
        Some(0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::BackColor;
    loaded.cpu.gpr[3] = 409; // blueColor
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_back_color, ppc_legacy_qd_color_to_rgb(409));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetBackColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rgb_color(&mut loaded.memory, color_ptr),
        Some(ppc_legacy_qd_color_to_rgb(409))
    );

    let cyan = PpcRgbColor {
        red: 0,
        green: 0xffff,
        blue: 0xffff,
    };
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, cyan).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RGBBackColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_back_color, cyan);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetBackColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rgb_color(&mut loaded.memory, color_ptr),
        Some(cyan)
    );

    ppc_write_rect(&mut loaded.memory, rect_ptr, 2, 3, 5, 6).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::EraseRect;
    loaded.cpu.gpr[3] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (3, 2)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(cyan)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 1)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(green)))
    );

    let red = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, red).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RGBForeColor;
    loaded.cpu.gpr[3] = color_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);

    ppc_write_rect(&mut loaded.memory, rect_ptr, 6, 6, 9, 9).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FrameRect;
    loaded.cpu.gpr[3] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (6, 6)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (8, 8)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (7, 7)),
        Some(0)
    );
}

#[test]
fn hle_import_runner_restores_quickdraw_colors_when_switching_ports() {
    let pef = synthetic_pef_with_import(b"ForeColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let offscreen_port = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(offscreen_port, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u16_be(offscreen_port + 6, 0xc000)
        .unwrap();
    ppc_write_rgb_color(
        &mut loaded.memory,
        offscreen_port + PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
        PPC_RGB_BLACK,
    )
    .unwrap();
    ppc_write_rgb_color(
        &mut loaded.memory,
        offscreen_port + PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
        PPC_RGB_WHITE,
    )
    .unwrap();

    let red = ppc_legacy_qd_color_to_rgb(205);
    loaded.cpu.gpr[3] = 205;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quickdraw_fore_color, red);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPort;
    loaded.cpu.gpr[3] = offscreen_port;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quickdraw_fore_color, PPC_RGB_BLACK);

    let green = ppc_legacy_qd_color_to_rgb(341);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ForeColor;
    loaded.cpu.gpr[3] = 341;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quickdraw_fore_color, green);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPort;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.quickdraw_fore_color, red);
    assert_eq!(
        ppc_read_rgb_color(
            &mut loaded.memory,
            offscreen_port + PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
        ),
        Some(green)
    );
}

#[test]
fn hle_import_runner_erases_oval_with_the_background_color() {
    let pef = synthetic_pef_with_import(b"EraseOval");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 20, 30).unwrap();
    let cyan = PpcRgbColor {
        red: 0,
        green: 0xffff,
        blue: 0xffff,
    };
    loaded.quickdraw_back_color = cyan;
    loaded.cpu.gpr[3] = rect_ptr;
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, *loaded.current_gworld).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 15)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(cyan)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (20, 10)),
        Some(0)
    );
}

#[test]
fn hle_import_runner_inverts_quickdraw_rect_pixels() {
    let pef = synthetic_pef_with_import(b"InvertRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 1, 2, 4, 5).unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, *loaded.current_gworld).unwrap();
    assert_eq!(front.depth, 8);
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (2, 1),
        0x12,
    ));
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (1, 1),
        0x34,
    ));
    loaded.cpu.gpr[3] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 1)),
        Some(0xed)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 1)),
        Some(0x34)
    );
}

#[test]
fn hle_import_runner_inverts_one_bit_bitmap_before_region_conversion() {
    let pef = synthetic_pef_with_import(b"InvertRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 0, 0, 1, 8).unwrap();

    let port = PPC_HEAP_BASE + 0x1000;
    let pixels = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(port, vec![0; 0x80]);
    loaded.memory.add_region(pixels, vec![0b1011_0000]);
    loaded.memory.write_u32_be(port + 2, pixels).unwrap();
    loaded.memory.write_u16_be(port + 6, 1).unwrap();
    ppc_write_rect(&mut loaded.memory, port + 8, 0, 0, 1, 8).unwrap();
    ppc_write_rect(&mut loaded.memory, port + 16, 0, 0, 1, 8).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 1,
        depth: 1,
        row_bytes: 1,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);
    loaded.cpu.gpr[3] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(pixels), Some(0b0100_1111));

    let region = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::BitMapToRegion;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = region;
    loaded.cpu.gpr[4] = port + 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, region),
        Some((0, 1, 1, 8))
    );
    let storage = ppc_region_storage(&mut loaded.memory, region).unwrap();
    assert_eq!(
        ppc_region_rows_for_band(&storage, 0, 1),
        Some(vec![vec![1, 2, 4, 8]])
    );
    assert!(ppc_point_in_region_storage(&storage, 0, 1));
    assert!(!ppc_point_in_region_storage(&storage, 0, 0));
}

#[test]
fn hle_import_runner_inverts_only_pixels_inside_quickdraw_region() {
    let pef = synthetic_pef_with_import(b"InvertRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let region = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, region, 1, 2, 4, 5).unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, *loaded.current_gworld).unwrap();
    assert_eq!(front.depth, 8);
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (2, 1),
        0x12,
    ));
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (1, 1),
        0x34,
    ));
    loaded.cpu.gpr[3] = region;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 1)),
        Some(0xed)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 1)),
        Some(0x34)
    );
}

#[test]
fn quickdraw_uses_the_live_pixmap_coordinate_origin_and_color_table() {
    let pef = synthetic_pef_with_import(b"PaintRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let port = scratch;
    let pixmap_handle = scratch + 0x100;
    let pixmap = scratch + 0x200;
    let stale_pixels = scratch + 0x300;
    let live_pixels = scratch + 0x400;
    let rect = scratch + 0x500;
    let ctable_handle = scratch + 0x520;
    let ctable = scratch + 0x540;
    loaded.memory.add_region(scratch, vec![0; 0x600]);
    loaded.memory.write_u32_be(port + 2, pixmap_handle).unwrap();
    loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    ppc_write_pixmap(
        &mut loaded.memory,
        pixmap,
        live_pixels,
        4,
        320,
        100,
        323,
        104,
        8,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(pixmap + 42, ctable_handle)
        .unwrap();
    loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
    loaded.memory.write_u16_be(ctable + 4, 0).unwrap();
    loaded.memory.write_u16_be(ctable + 6, 0).unwrap();
    loaded.memory.write_u16_be(ctable + 8, 77).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr: stale_pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: 4,
        height: 3,
        depth: 8,
        row_bytes: 4,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);
    loaded.quickdraw_fore_color = PpcRgbColor {
        red: 0x1234,
        green: 0x5678,
        blue: 0x9abc,
    };
    let [red, green, blue] =
        ppc_rgb555_to_rgb16(ppc_rgb_color_to_rgb555(loaded.quickdraw_fore_color));
    loaded.memory.write_u16_be(ctable + 10, red).unwrap();
    loaded.memory.write_u16_be(ctable + 12, green).unwrap();
    loaded.memory.write_u16_be(ctable + 14, blue).unwrap();
    // Imaging With QuickDraw (1994), pp. 2-9 and 2-45--2-46: the
    // PixMap boundary rectangle defines the port's local coordinate
    // system, so these coordinates address the first two rows and
    // columns even though neither coordinate begins at zero.
    ppc_write_rect(&mut loaded.memory, rect, 320, 100, 322, 102).unwrap();
    loaded.cpu.gpr[3] = rect;

    let live =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, port).unwrap();
    assert_eq!(live.base_addr, live_pixels);
    assert_eq!((live.width, live.height, live.depth), (4, 3, 8));

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_ne!(ppc_rgb_color_to_8bpp_index(loaded.quickdraw_fore_color), 77);
    assert_eq!(loaded.memory.read_u8(live_pixels), Some(77));
    assert_eq!(loaded.memory.read_u8(live_pixels + 5), Some(77));
    assert_eq!(loaded.memory.read_u8(stale_pixels), Some(0));
}

#[test]
fn onscreen_color_port_keeps_its_logical_pm_table_during_a_hardware_fade() {
    let pef = synthetic_pef_with_import(b"PaintRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let port = scratch;
    let pixmap_handle = scratch + 0x100;
    let pixmap = scratch + 0x200;
    let private_ctable_handle = scratch + 0x300;
    let private_ctable = scratch + 0x400;
    loaded.memory.add_region(scratch, vec![0; 0x1000]);
    loaded.memory.write_u32_be(port + 2, pixmap_handle).unwrap();
    loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    ppc_write_pixmap(
        &mut loaded.memory,
        pixmap,
        PPC_MAIN_SCREEN_BASE,
        ppc_main_screen_row_bytes(),
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
        8,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(pixmap + 42, private_ctable_handle)
        .unwrap();
    loaded
        .memory
        .write_u32_be(private_ctable_handle, private_ctable)
        .unwrap();
    loaded.memory.write_u16_be(private_ctable + 6, 0).unwrap();
    loaded.memory.write_u16_be(private_ctable + 8, 42).unwrap();
    loaded
        .memory
        .write_u16_be(private_ctable + 10, 0x4444)
        .unwrap();
    loaded
        .memory
        .write_u16_be(private_ctable + 12, 0x5555)
        .unwrap();
    loaded
        .memory
        .write_u16_be(private_ctable + 14, 0x6666)
        .unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr: PPC_MAIN_SCREEN_BASE,
        gdevice: PPC_MAIN_GDEVICE,
        width: ppc_main_screen_width(),
        height: ppc_main_screen_height(),
        depth: 8,
        row_bytes: ppc_main_screen_row_bytes(),
        pixels_locked: false,
        pixels_no_purge: false,
    });
    let screen_clut = [[0; 3]; 256];
    let color_manager_clut = [[0; 3]; 256];

    let clut = ppc_live_gworld_clut(
        &mut loaded.memory,
        &loaded.gworlds,
        port,
        &screen_clut,
        &color_manager_clut,
    );

    assert_eq!(clut[42], [0x4444, 0x5555, 0x6666]);
}

#[test]
fn hle_import_runner_draws_quickdraw_primitives_into_8bpp_current_gworld() {
    let pef = synthetic_pef_with_import(b"PaintRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let port = scratch;
    let rect_ptr = scratch + 0x100;
    let string_ptr = scratch + 0x120;
    let pixels = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x400]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: 16,
        height: 16,
        depth: 8,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);
    loaded
        .memory
        .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1)
        .unwrap();
    let red = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_fore_color = red;
    let red_index = ppc_rgb_color_to_8bpp_index(red);
    let pixel_addr = |x: u32, y: u32| pixels + y * 16 + x;

    ppc_write_rect(&mut loaded.memory, rect_ptr, 2, 3, 5, 7).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(pixel_addr(3, 2)), Some(red_index));
    assert_eq!(loaded.memory.read_u8(pixel_addr(6, 4)), Some(red_index));
    assert_eq!(loaded.memory.read_u8(pixel_addr(2, 2)), Some(0));

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FrameRect;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    ppc_write_rect(&mut loaded.memory, rect_ptr, 8, 8, 12, 12).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(pixel_addr(8, 8)), Some(red_index));
    assert_eq!(loaded.memory.read_u8(pixel_addr(11, 11)), Some(red_index));
    assert_eq!(loaded.memory.read_u8(pixel_addr(9, 9)), Some(0));

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 14;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LineTo;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = 14;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for x in 1..=4 {
        assert_eq!(loaded.memory.read_u8(pixel_addr(x, 14)), Some(red_index));
    }

    loaded.memory.write_u8(string_ptr, 1).unwrap();
    loaded.memory.write_u8(string_ptr + 1, b'H').unwrap();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 10;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawString;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = string_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let (glyph, data) =
        get_glyph(PPC_QD_TEXT_FONT_DEFAULT, loaded.quickdraw_text_size, 'H').unwrap();
    let mut covered = false;
    for row in 0..glyph.height as usize {
        for col in 0..glyph.width as usize {
            let index = glyph.data_offset + row * glyph.width as usize + col;
            if index < data.len() && data[index] >= 128 {
                let x = 2 + i32::from(glyph.origin_x) + col as i32;
                let y = 10 + i32::from(glyph.origin_y) + row as i32;
                if x >= 0
                    && y >= 0
                    && x < 16
                    && y < 16
                    && loaded.memory.read_u8(pixel_addr(x as u32, y as u32)) == Some(red_index)
                {
                    covered = true;
                }
            }
        }
    }
    assert!(covered, "8bpp DrawString should write covered glyph pixels");
}

#[test]
fn hle_import_runner_does_not_cross_short_two_bit_pixmap_rows() {
    let pef = synthetic_pef_with_import(b"FillRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1c00;
    let port = scratch;
    let pixmap_handle = scratch + 0x100;
    let pixmap = scratch + 0x120;
    let rect = scratch + 0x180;
    let pixels = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    ppc_write_gworld_port(&mut loaded.memory, port, pixmap_handle, 0, 0, 2, 8).unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    ppc_write_pixmap(&mut loaded.memory, pixmap, pixels, 1, 0, 0, 2, 8, 2).unwrap();
    loaded.memory.write_bytes(pixels, &[0x55, 0xa5]).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr: pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 2,
        depth: 2,
        row_bytes: 1,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);
    loaded.quickdraw_fore_indices.insert(port, 2);
    ppc_write_rect(&mut loaded.memory, rect, 0, 0, 1, 8).unwrap();
    loaded.cpu.gpr[3] = rect;

    assert!(ppc_read_pixmap_bits(&mut loaded.memory, pixmap).is_none());
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);

    assert_eq!(loaded.memory.read_u8(pixels), Some(0xaa));
    assert_eq!(loaded.memory.read_u8(pixels + 1), Some(0xa5));
}

#[test]
fn hle_import_runner_draws_representative_quickdraw_primitives_into_2bpp_gworld() {
    const WIDTH: u32 = 9;
    const HEIGHT: u32 = 32;
    const ROW_BYTES: u32 = 4;

    let pef = synthetic_pef_with_import(b"PaintRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1800;
    let port = scratch;
    let rect_ptr = scratch + 0x100;
    let string_ptr = scratch + 0x120;
    let pixels = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x400]);
    loaded
        .memory
        .write_bytes(pixels, &vec![0x55; (ROW_BYTES * HEIGHT) as usize])
        .unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: WIDTH,
        height: HEIGHT,
        depth: 2,
        row_bytes: ROW_BYTES,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);
    loaded
        .memory
        .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u16_be(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1)
        .unwrap();
    loaded.quickdraw_fore_color = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_fore_indices.insert(port, 2);
    loaded.quickdraw_text_size = 12;
    loaded.quickdraw_text_mode = PPC_QD_TEXT_MODE_SRC_OR;
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, port).unwrap();

    // Start each field at index 1. PaintRect begins at odd x=1 and must
    // replace only x=1..2 within the first packed byte.
    ppc_write_rect(&mut loaded.memory, rect_ptr, 0, 1, 1, 3).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::PaintRect);
    assert_eq!(loaded.memory.read_u8(pixels), Some(0x69));
    assert_eq!(loaded.memory.read_u8(pixels + 1), Some(0x55));

    // FillRect begins at odd x=5 and spans the remaining fields of its
    // byte without changing the adjacent x=4 field.
    ppc_write_rect(&mut loaded.memory, rect_ptr, 1, 5, 2, 8).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FillRect);
    assert_eq!(loaded.memory.read_u8(pixels + ROW_BYTES), Some(0x55));
    assert_eq!(loaded.memory.read_u8(pixels + ROW_BYTES + 1), Some(0x6a));

    // Inverting index 1 at odd x=1..2 produces index 2 while retaining
    // both neighbouring fields.
    ppc_write_rect(&mut loaded.memory, rect_ptr, 2, 1, 3, 3).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InvertRect);
    assert_eq!(loaded.memory.read_u8(pixels + 2 * ROW_BYTES), Some(0x69));

    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveTo);
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LineTo);
    assert_eq!(loaded.memory.read_u8(pixels + 3 * ROW_BYTES), Some(0x6a));
    assert_eq!(
        loaded.memory.read_u8(pixels + 3 * ROW_BYTES + 1),
        Some(0xa9)
    );

    let first_covered_glyph_pixel = |ch: char| {
        let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, ch).unwrap();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index < data.len() && data[index] >= 128 {
                    return (
                        i32::from(glyph.origin_x) + col as i32,
                        i32::from(glyph.origin_y) + row as i32,
                    );
                }
            }
        }
        panic!("glyph should contain at least one covered pixel");
    };
    let (glyph_dx, glyph_dy) = first_covered_glyph_pixel('H');

    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 16;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveTo);
    loaded.cpu.gpr[3] = b'H' as u32;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawChar);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1 + glyph_dx, 16 + glyph_dy)),
        Some(2),
        "DrawChar must reach the packed 2bpp destination"
    );

    loaded.memory.write_u8(string_ptr, 1).unwrap();
    loaded.memory.write_u8(string_ptr + 1, b'H').unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 30;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveTo);
    loaded.cpu.gpr[3] = string_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawString);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1 + glyph_dx, 30 + glyph_dy)),
        Some(2),
        "DrawString must reach the packed 2bpp destination"
    );

    // Width 9 consumes only the high field of byte 2. Every operation
    // above must preserve its low six tail bits, the fourth padding byte,
    // and the x=0 neighbour on each touched scanline.
    for y in 0..HEIGHT {
        let row = pixels + y * ROW_BYTES;
        assert_eq!(loaded.memory.read_u8(row + 2).unwrap() & 0x3f, 0x15);
        assert_eq!(loaded.memory.read_u8(row + 3), Some(0x55));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, y as i32)),
            Some(1),
            "left neighbour changed on row {y}"
        );
    }
}

#[test]
fn hle_import_runner_round_trips_quickdraw_pen_state() {
    let pef = synthetic_pef_with_import(b"GetPenState");
    let mut loaded = load_pef_application(&pef).unwrap();
    let state_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(state_ptr, vec![0; 18]);
    let expected = [
        0x00, 0x0a, 0x00, 0x14, 0x00, 0x02, 0x00, 0x03, 0x00, 0x08, 0xaa, 0x55, 0xaa, 0x55,
        0xaa, 0x55, 0xaa, 0x55,
    ];
    loaded
        .memory
        .write_bytes(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PN_LOC_OFFSET, &expected)
        .unwrap();
    loaded.cpu.gpr[3] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, state_ptr, 18),
        Some(expected.to_vec())
    );

    let replacement = [
        0x00, 0x1e, 0x00, 0x28, 0x00, 0x04, 0x00, 0x05, 0x00, 0x09, 0xff, 0x00, 0xff, 0x00,
        0xff, 0x00, 0xff, 0x00,
    ];
    loaded.memory.write_bytes(state_ptr, &replacement).unwrap();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPenState;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GWORLD + PPC_CGRAF_PORT_PN_LOC_OFFSET,
            18,
        ),
        Some(replacement.to_vec())
    );
}

#[test]
fn hle_import_runner_tracks_quickdraw_pen_and_draws_lines_into_current_gworld() {
    let pef = synthetic_pef_with_import(b"MoveTo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let red = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_fore_color = red;

    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 3;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_pen_h, 2);
    assert_eq!(loaded.quickdraw_pen_v, 3);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 48), Some(3));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 50), Some(2));

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 3)),
        Some(0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LineTo;
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 3;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_pen_h, 6);
    assert_eq!(loaded.quickdraw_pen_v, 3);
    for x in 2..=6 {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, 3)),
            Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
        );
    }
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (2, 4)),
        Some(0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 6;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_pen_h, 6);
    assert_eq!(loaded.quickdraw_pen_v, 6);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 48), Some(6));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 50), Some(6));
    for y in 3..=6 {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (6, y)),
            Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
        );
    }
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (5, 6)),
        Some(0)
    );

    let clip_rgn = loaded
        .memory
        .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    ppc_write_rgn_bbox(&mut loaded.memory, clip_rgn, 8, 4, 9, 7).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 8;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LineTo;
    loaded.cpu.gpr[3] = 9;
    loaded.cpu.gpr[4] = 8;
    loaded.run_with_hle_imports(64);
    for x in 2..=9 {
        let expected = if (4..7).contains(&x) {
            u16::from(ppc_rgb_color_to_8bpp_index(red))
        } else {
            0
        };
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, 8)),
            Some(expected)
        );
    }

    ppc_write_rgn_bbox(&mut loaded.memory, clip_rgn, 0, 0, 600, 800).unwrap();
    let vis_rgn = loaded
        .memory
        .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
        .unwrap();
    ppc_write_rgn_bbox(&mut loaded.memory, vis_rgn, 12, 4, 13, 7).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 12;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LineTo;
    loaded.cpu.gpr[3] = 9;
    loaded.cpu.gpr[4] = 12;
    loaded.run_with_hle_imports(64);
    for x in 2..=9 {
        let expected = if (4..7).contains(&x) {
            u16::from(ppc_rgb_color_to_8bpp_index(red))
        } else {
            0
        };
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, 12)),
            Some(expected)
        );
    }

    ppc_write_rgn_bbox(&mut loaded.memory, vis_rgn, 0, 0, 600, 800).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HidePen;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = 10;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LineTo;
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 10;
    loaded.run_with_hle_imports(64);
    for x in 2..=6 {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, 10)),
            Some(0)
        );
    }
}

#[test]
fn hle_import_runner_records_and_fills_classic_quickdraw_polygons() {
    let pef = synthetic_pef_with_import(b"OpenPoly");
    let mut loaded = load_pef_application(&pef).unwrap();
    let red = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_fore_color = red;
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let polygon = loaded.cpu.gpr[3];
    assert_ne!(polygon, 0);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GWORLD + 100),
        Some(polygon)
    );

    for (target, h, v) in [
        (PpcImportDispatcherTarget::MoveTo, 2, 2),
        (PpcImportDispatcherTarget::LineTo, 10, 2),
        (PpcImportDispatcherTarget::LineTo, 2, 10),
        (PpcImportDispatcherTarget::LineTo, 2, 2),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = h;
        loaded.cpu.gpr[4] = v;
        loaded.run_with_hle_imports(64);
    }
    assert_eq!(
        ppc_polygon_points(&mut loaded.memory, polygon),
        Some(vec![(2, 2), (10, 2), (2, 10), (2, 2)])
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ClosePoly;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.memory.read_u32_be(PPC_MAIN_GWORLD + 100), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FillPoly;
    loaded.cpu.gpr[3] = polygon;
    loaded.run_with_hle_imports(64);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (3, 3)),
        Some(103)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::KillPoly;
    loaded.cpu.gpr[3] = polygon;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.memory.read_u32_be(polygon), Some(0));
}

#[test]
fn region_membership_and_clip_copy_use_full_region_storage() {
    let pef = synthetic_pef_with_import(b"PtInRgn");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let region = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, region, 5, 10, 20, 30).unwrap();
    loaded.cpu.gpr[3] = (6u32 << 16) | 11;
    loaded.cpu.gpr[4] = region;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 19, 29, 25, 35).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RectInRgn;
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = region;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let saved = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetClip;
    loaded.cpu.gpr[3] = region;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetClip;
    loaded.cpu.gpr[3] = saved;
    loaded.run_with_hle_imports(64);
    assert_eq!(
        ppc_region_storage(&mut loaded.memory, saved),
        ppc_region_storage(&mut loaded.memory, region)
    );
}

#[test]
fn rectangle_fill_respects_disjoint_clip_spans_at_every_depth() {
    for depth in [1, 2, 4, 8, 16] {
        let mut loaded = load_pef_application_with_config(
            &synthetic_pef(),
            PpcLoadConfig {
                screen_depth: depth,
                ..PpcLoadConfig::default()
            },
        )
        .unwrap();
        assert!(ppc_paint_rect_bounds(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (0, 0, 8, 10),
            PPC_RGB_WHITE,
            None
        ));
        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 512]);
        let clip = ppc_region_storage_from_rows(2, &vec![vec![1, 3, 5, 9]; 3]).unwrap();
        let vis = ppc_region_storage_from_rows(1, &vec![vec![2, 8]; 5]).unwrap();
        for (handle, ptr, bytes, field) in [
            (scratch, scratch + 16, clip, PPC_CGRAF_PORT_CLIP_RGN_OFFSET),
            (
                scratch + 4,
                scratch + 128,
                vis,
                PPC_CGRAF_PORT_VIS_RGN_OFFSET,
            ),
        ] {
            loaded.memory.write_u32_be(handle, ptr).unwrap();
            loaded.memory.write_bytes(ptr, &bytes).unwrap();
            loaded
                .memory
                .write_u32_be(PPC_MAIN_GWORLD + field, handle)
                .unwrap();
        }
        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap();
        let front = surface.front_buffer;
        let before: Vec<_> = (0..8)
            .flat_map(|y| (0..10).map(move |x| (x, y)))
            .map(|point| ppc_quickdraw_read_pixel(&mut loaded.memory, front, point).unwrap())
            .collect();
        let color =
            ppc_quickdraw_surface_fore_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK, None)
                .unwrap();
        assert!(ppc_paint_rect_bounds(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (0, 0, 8, 10),
            PPC_RGB_BLACK,
            None
        ));
        for y in 0..8 {
            for x in 0..10 {
                let painted = (2..5).contains(&y) && (x == 2 || (5..8).contains(&x));
                assert_eq!(
                    ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                    Some(if painted {
                        color
                    } else {
                        before[(y * 10 + x) as usize]
                    }),
                    "depth {depth}, pixel ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn frame_round_rect_respects_the_current_port_clip_region() {
    let pef = synthetic_pef_with_import(b"ClipRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    let clip_ptr = rect_ptr + 8;
    loaded.memory.add_region(rect_ptr, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 10, 30, 30).unwrap();
    ppc_write_rect(&mut loaded.memory, clip_ptr, 0, 20, 40, 40).unwrap();
    loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 103);

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let clipped_pixel_before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 20));

    loaded.cpu.gpr[3] = clip_ptr;
    loaded.run_with_hle_imports(64);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FrameRoundRect;
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 8;
    loaded.run_with_hle_imports(64);

    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 20)),
        clipped_pixel_before,
        "the rounded rectangle must not draw left of clipRgn"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (29, 20)),
        Some(103),
        "the rounded rectangle must draw inside clipRgn"
    );
}

#[test]
fn hle_import_runner_draws_quickdraw_text_into_current_gworld() {
    let pef = synthetic_pef_with_import(b"TextSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let string_ptr = scratch;
    loaded.memory.add_region(scratch, vec![0; 16]);
    loaded.memory.write_u8(string_ptr, 2).unwrap();
    loaded.memory.write_u8(string_ptr + 1, b'H').unwrap();
    loaded.memory.write_u8(string_ptr + 2, b'i').unwrap();
    let red = PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    };
    loaded.quickdraw_fore_color = red;

    loaded.cpu.gpr[3] = 12;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_text_size, 12);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 74), Some(12));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TextMode;
    loaded.cpu.gpr[3] = PPC_QD_TEXT_MODE_SRC_OR as u32;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_text_mode, PPC_QD_TEXT_MODE_SRC_OR);
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 72),
        Some(PPC_QD_TEXT_MODE_SRC_OR as u16)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.gpr[3] = 20;
    loaded.cpu.gpr[4] = 30;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let first_covered_glyph_pixel = |ch: char| {
        let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, ch).unwrap();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index < data.len() && data[index] >= 128 {
                    return (
                        i32::from(glyph.origin_x) + col as i32,
                        i32::from(glyph.origin_y) + row as i32,
                    );
                }
            }
        }
        panic!("glyph should contain at least one covered pixel");
    };
    let (glyph_dx, glyph_dy) = first_covered_glyph_pixel('H');

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawChar;
    loaded.cpu.gpr[3] = b'H' as u32;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let h_advance = ppc_text_byte_advance(b'H', 12);
    assert_eq!(loaded.quickdraw_pen_h, 20 + h_advance);
    assert_eq!(loaded.quickdraw_pen_v, 30);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (20 + glyph_dx, 30 + glyph_dy),),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TextWidth;
    loaded.cpu.gpr[3] = string_ptr + 1;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 2;
    let probe = loaded.run_with_hle_imports(64);
    let string_advance = ppc_text_bytes_advance(b"Hi", 12);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_advance as u32);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::StringWidth;
    loaded.cpu.gpr[3] = string_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_advance as u32);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveTo;
    loaded.cpu.gpr[3] = 50;
    loaded.cpu.gpr[4] = 30;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawString;
    loaded.cpu.gpr[3] = string_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.quickdraw_pen_h, 50 + string_advance);
    assert_eq!(loaded.quickdraw_pen_v, 30);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (50 + glyph_dx, 30 + glyph_dy),),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(red)))
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 48), Some(30));
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 50),
        Some((50 + string_advance) as u16)
    );
}

#[test]
fn text_face_writes_the_cgrafport_style_byte() {
    let pef = synthetic_pef_with_import(b"TextFace");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u8(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_TX_FACE_OFFSET + 1, 0xa5)
        .unwrap();
    loaded.cpu.gpr[3] = QuickDrawTextStyle::BOLD_BIT.into();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_TX_FACE_OFFSET),
        Some(QuickDrawTextStyle::BOLD_BIT)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_TX_FACE_OFFSET + 1),
        Some(0xa5),
        "TextFace must not overwrite the adjacent filler byte"
    );

    loaded.quickdraw_pen_h = 20;
    loaded.quickdraw_pen_v = 30;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawChar;
    loaded.cpu.gpr[3] = b'H'.into();
    loaded.run_with_hle_imports(64);

    let plain_advance = ppc_text_byte_advance(b'H', loaded.quickdraw_text_size);
    assert_eq!(loaded.quickdraw_pen_h, 20 + plain_advance + 1);
}

#[test]
fn hle_import_runner_op_color_records_the_arithmetic_transfer_operand() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OpColor"),
        PpcImportDispatcherTarget::OpColor
    );
    let pef = synthetic_pef_with_import(b"OpColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    let color = PpcRgbColor {
        red: 0x1234,
        green: 0x5678,
        blue: 0x9abc,
    };
    loaded.memory.add_region(color_ptr, vec![0; 6]);
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, color).unwrap();
    loaded.cpu.gpr[3] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.quickdraw_op_colors.quickdraw_op_color(PPC_MAIN_GWORLD),
        Some((color.red, color.green, color.blue))
    );
}

#[test]
fn hle_import_runner_hilite_color_records_the_highlight_operand() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HiliteColor"),
        PpcImportDispatcherTarget::HiliteColor
    );
    let pef = synthetic_pef_with_import(b"HiliteColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    let color = PpcRgbColor {
        red: 0x1234,
        green: 0x5678,
        blue: 0x9abc,
    };
    loaded.memory.add_region(color_ptr, vec![0; 6]);
    ppc_write_rgb_color(&mut loaded.memory, color_ptr, color).unwrap();
    loaded.cpu.gpr[3] = color_ptr;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteColor);

    assert_eq!(
        loaded
            .quickdraw_hilite_colors
            .quickdraw_hilite_color(PPC_MAIN_GWORLD),
        Some((color.red, color.green, color.blue))
    );
    assert_eq!(
        ppc_current_hilite_color(
            &mut loaded.memory,
            PPC_MAIN_GWORLD,
            &loaded.quickdraw_hilite_colors,
        ),
        color
    );
}

#[test]
fn classic_hilite_color_is_immediately_visible_to_attached_native_get_ctable() {
    let pef = synthetic_pef_with_import(b"GetCTable");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());
    context.attach_classic_memory_bus(&mut classic_bus);
    let mut classic_cpu = MockCpu::new();
    let sp = 0x0100;
    let color_ptr = 0x0120;
    classic_cpu.write_reg(Register::A7, sp);
    classic_bus.write_long(sp, color_ptr);
    classic_bus.write_word(color_ptr, 0x1357);
    classic_bus.write_word(color_ptr + 2, 0x2468);
    classic_bus.write_word(color_ptr + 4, 0x369a);
    assert!(classic
        .dispatch_quickdraw(true, 0x222, &mut classic_cpu, &mut classic_bus)
        .expect("HiliteColor trap")
        .is_ok());
    assert_eq!(classic_cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(
        native
            .quickdraw_hilite_colors
            .quickdraw_hilite_color(PPC_MAIN_GWORLD),
        Some((0x1357, 0x2468, 0x369a))
    );

    native.cpu.gpr[3] = 66;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetCTable);
    let ctable = native.cpu.gpr[3];
    let ctable_ptr = native
        .memory
        .read_u32_be(ctable)
        .expect("native enhanced ColorTable handle");
    assert_eq!(native.memory.read_u16_be(ctable_ptr + 6), Some(3));
    assert_eq!(native.memory.read_u16_be(ctable_ptr + 8 + 2 * 8 + 2), Some(0x1357));
    assert_eq!(native.memory.read_u16_be(ctable_ptr + 8 + 2 * 8 + 4), Some(0x2468));
    assert_eq!(native.memory.read_u16_be(ctable_ptr + 8 + 2 * 8 + 6), Some(0x369a));
}

#[test]
fn native_hilite_color_is_immediately_visible_to_attached_classic_grafvars() {
    let pef = synthetic_pef_with_import(b"HiliteColor");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    let base = PPC_HEAP_BASE + 0x15_000;
    let port = base;
    let graf_vars_handle = base + 0x100;
    let graf_vars = base + 0x110;
    let color_ptr = base + 0x120;
    native.memory.add_region(base, vec![0; 0x140]);
    native.memory.write_u16_be(port + 6, 0xc000).unwrap();
    native
        .memory
        .write_u32_be(port + PPC_CGRAF_PORT_GRAF_VARS_OFFSET, graf_vars_handle)
        .unwrap();
    native.memory.write_u32_be(graf_vars_handle, graf_vars).unwrap();
    let color = PpcRgbColor {
        red: 0x1357,
        green: 0x2468,
        blue: 0x369a,
    };
    ppc_write_rgb_color(&mut native.memory, color_ptr, color).unwrap();
    native
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);

    native.cpu.gpr[3] = color_ptr;
    run_test_import(&mut native, PpcImportDispatcherTarget::HiliteColor);

    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());
    context.attach_classic_memory_bus(&mut classic_bus);
    assert_eq!(*classic.current_port, port);
    assert_eq!(classic_bus.read_word(graf_vars + 6), color.red);
    assert_eq!(classic_bus.read_word(graf_vars + 8), color.green);
    assert_eq!(classic_bus.read_word(graf_vars + 10), color.blue);
    assert_eq!(classic.current_hilite_color(&classic_bus), (color.red, color.green, color.blue));
}

#[test]
fn hilite_color_keeps_distinct_values_when_switching_between_ports() {
    let pef = synthetic_pef_with_import(b"HiliteColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let base = PPC_HEAP_BASE + 0x16_000;
    let first_port = base;
    let second_port = base + 0x40;
    let basic_port = base + 0x80;
    let first_color_ptr = base + 0x100;
    let second_color_ptr = base + 0x110;
    let basic_color_ptr = base + 0x120;
    loaded.memory.add_region(base, vec![0; 0x140]);
    for port in [first_port, second_port] {
        loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    }
    loaded.memory.write_u16_be(basic_port + 6, 0).unwrap();
    let first_color = PpcRgbColor {
        red: 0x1111,
        green: 0x2222,
        blue: 0x3333,
    };
    let second_color = PpcRgbColor {
        red: 0xaaaa,
        green: 0xbbbb,
        blue: 0xcccc,
    };
    let basic_color = PpcRgbColor {
        red: 0xdddd,
        green: 0xeeee,
        blue: 0xffff,
    };
    ppc_write_rgb_color(&mut loaded.memory, first_color_ptr, first_color).unwrap();
    ppc_write_rgb_color(&mut loaded.memory, second_color_ptr, second_color).unwrap();
    ppc_write_rgb_color(&mut loaded.memory, basic_color_ptr, basic_color).unwrap();

    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = first_port);
    loaded.cpu.gpr[3] = first_color_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteColor);
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = second_port);
    loaded.cpu.gpr[3] = second_color_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteColor);

    assert_eq!(
        ppc_current_hilite_color(
            &mut loaded.memory,
            first_port,
            &loaded.quickdraw_hilite_colors,
        ),
        first_color
    );
    assert_eq!(
        ppc_current_hilite_color(
            &mut loaded.memory,
            second_port,
            &loaded.quickdraw_hilite_colors,
        ),
        second_color
    );

    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = basic_port);
    loaded.cpu.gpr[3] = basic_color_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteColor);
    assert_eq!(
        loaded
            .quickdraw_hilite_colors
            .quickdraw_hilite_color(basic_port),
        None
    );
    let (red, green, blue) = DEFAULT_QUICKDRAW_HILITE_COLOR;
    assert_eq!(
        ppc_current_hilite_color(
            &mut loaded.memory,
            basic_port,
            &loaded.quickdraw_hilite_colors,
        ),
        PpcRgbColor { red, green, blue }
    );
}

#[test]
fn detached_ppc_clone_has_independent_quickdraw_hilite_colors() {
    let pef = synthetic_pef_with_import(b"HiliteColor");
    let loaded = load_pef_application(&pef).unwrap();
    let port = PPC_HEAP_BASE + 0x17_000;
    loaded
        .quickdraw_hilite_colors
        .set_quickdraw_hilite_color(port, (0x1111, 0x2222, 0x3333));
    let detached = loaded.clone();
    loaded
        .quickdraw_hilite_colors
        .set_quickdraw_hilite_color(port, (0xaaaa, 0xbbbb, 0xcccc));

    assert_eq!(
        loaded
            .quickdraw_hilite_colors
            .quickdraw_hilite_color(port),
        Some((0xaaaa, 0xbbbb, 0xcccc))
    );
    assert_eq!(
        detached
            .quickdraw_hilite_colors
            .quickdraw_hilite_color(port),
        Some((0x1111, 0x2222, 0x3333))
    );
}

#[test]
fn classic_op_color_is_consumed_by_attached_native_copybits() {
    // The native CopyBits implementation is the current arithmetic-mode
    // consumer. Exercise the actual 68K -> process state -> PPC path.
    let pef = synthetic_pef_with_import(b"CopyBits");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());
    context.attach_classic_memory_bus(&mut classic_bus);
    let mut classic_cpu = MockCpu::new();
    let sp = 0x0100;
    let color_ptr = 0x0120;
    classic_cpu.write_reg(Register::A7, sp);
    classic_bus.write_long(sp, color_ptr);
    classic_bus.write_word(color_ptr, 0x8000);
    classic_bus.write_word(color_ptr + 2, 0x8000);
    classic_bus.write_word(color_ptr + 4, 0x8000);
    assert!(classic
        .dispatch_quickdraw(true, 0x221, &mut classic_cpu, &mut classic_bus)
        .expect("OpColor trap")
        .is_ok());
    assert_eq!(classic_cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(
        native.quickdraw_op_colors.quickdraw_op_color(PPC_MAIN_GWORLD),
        Some((0x8000, 0x8000, 0x8000))
    );

    let scratch = PPC_HEAP_BASE + 0x11600;
    let src_pixels = scratch;
    let dst_pixels = scratch + 4;
    let src_pixmap = scratch + 8;
    let dst_pixmap = scratch + 64;
    let rect = scratch + 120;
    native.memory.add_region(scratch, vec![0; 128]);
    ppc_write_pixmap(
        &mut native.memory,
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
        &mut native.memory,
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
    native.memory.write_u16_be(src_pixels, 0x7c00).unwrap();
    native.memory.write_u16_be(dst_pixels, 0x001f).unwrap();
    ppc_write_rect(&mut native.memory, rect, 0, 0, 1, 1).unwrap();
    native.cpu.gpr[3] = src_pixmap;
    native.cpu.gpr[4] = dst_pixmap;
    native.cpu.gpr[5] = rect;
    native.cpu.gpr[6] = rect;
    native.cpu.gpr[7] = 32;
    native.cpu.gpr[8] = 0;

    run_test_import(&mut native, PpcImportDispatcherTarget::CopyBits);

    assert_eq!(native.memory.read_u16_be(dst_pixels), Some(0x3c0f));
}

#[test]
fn native_op_color_is_immediately_visible_to_attached_classic_grafvars() {
    let pef = synthetic_pef_with_import(b"OpColor");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    let base = PPC_HEAP_BASE + 0x12_000;
    let port = base;
    let graf_vars_handle = base + 0x100;
    let graf_vars = base + 0x110;
    let color_ptr = base + 0x120;
    native.memory.add_region(base, vec![0; 0x140]);
    native.memory.write_u16_be(port + 6, 0xc000).unwrap();
    native
        .memory
        .write_u32_be(port + PPC_CGRAF_PORT_GRAF_VARS_OFFSET, graf_vars_handle)
        .unwrap();
    native.memory.write_u32_be(graf_vars_handle, graf_vars).unwrap();
    let color = PpcRgbColor {
        red: 0x1234,
        green: 0x5678,
        blue: 0x9abc,
    };
    ppc_write_rgb_color(&mut native.memory, color_ptr, color).unwrap();
    native
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = port);

    native.cpu.gpr[3] = color_ptr;
    run_test_import(&mut native, PpcImportDispatcherTarget::OpColor);

    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());
    context.attach_classic_memory_bus(&mut classic_bus);
    assert_eq!(*classic.current_port, port);
    assert_eq!(classic_bus.read_word(graf_vars), color.red);
    assert_eq!(classic_bus.read_word(graf_vars + 2), color.green);
    assert_eq!(classic_bus.read_word(graf_vars + 4), color.blue);
}

#[test]
fn op_color_keeps_distinct_values_when_switching_between_ports() {
    let pef = synthetic_pef_with_import(b"OpColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let base = PPC_HEAP_BASE + 0x13_000;
    let first_port = base;
    let second_port = base + 0x40;
    let first_color_ptr = base + 0x80;
    let second_color_ptr = base + 0x90;
    loaded.memory.add_region(base, vec![0; 0xa0]);
    for port in [first_port, second_port] {
        loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    }
    let first_color = PpcRgbColor {
        red: 0x1111,
        green: 0x2222,
        blue: 0x3333,
    };
    let second_color = PpcRgbColor {
        red: 0xaaaa,
        green: 0xbbbb,
        blue: 0xcccc,
    };
    ppc_write_rgb_color(&mut loaded.memory, first_color_ptr, first_color).unwrap();
    ppc_write_rgb_color(&mut loaded.memory, second_color_ptr, second_color).unwrap();

    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = first_port);
    loaded.cpu.gpr[3] = first_color_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OpColor);
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = second_port);
    loaded.cpu.gpr[3] = second_color_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OpColor);

    assert_eq!(
        ppc_current_op_color(&mut loaded.memory, first_port, &loaded.quickdraw_op_colors),
        first_color
    );
    assert_eq!(
        ppc_current_op_color(&mut loaded.memory, second_port, &loaded.quickdraw_op_colors),
        second_color
    );
}

#[test]
fn detached_ppc_clone_has_independent_quickdraw_op_colors() {
    let pef = synthetic_pef_with_import(b"OpColor");
    let loaded = load_pef_application(&pef).unwrap();
    let port = PPC_HEAP_BASE + 0x14_000;
    loaded
        .quickdraw_op_colors
        .set_quickdraw_op_color(port, (0x1111, 0x2222, 0x3333));
    let detached = loaded.clone();
    loaded
        .quickdraw_op_colors
        .set_quickdraw_op_color(port, (0xaaaa, 0xbbbb, 0xcccc));

    assert_eq!(
        loaded.quickdraw_op_colors.quickdraw_op_color(port),
        Some((0xaaaa, 0xbbbb, 0xcccc))
    );
    assert_eq!(
        detached.quickdraw_op_colors.quickdraw_op_color(port),
        Some((0x1111, 0x2222, 0x3333))
    );
}

#[test]
fn hle_import_runner_handles_get_picture_as_valid_handle() {
    let pef = synthetic_pef_with_import(b"GetPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(test_handle_records!(loaded).len(), 1);
    assert_eq!(test_handle_records!(loaded)[0].handle, handle);
    assert_eq!(
        test_handle_records!(loaded)[0].size,
        minimal_pict_bytes().len() as u32
    );
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(ptr, test_handle_records!(loaded)[0].ptr);
    assert_eq!(loaded.memory.read_u16_be(ptr), Some(0x000c));
    assert_eq!(loaded.memory.read_u16_be(ptr + 10), Some(0x00ff));
    assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn hle_import_runner_get_picture_prefers_loaded_pict_resource() {
    let pef = synthetic_pef_with_import(b"GetPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(42);
    let pict = test_v1_one_bit_packbits_pict();
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 42,
        path: "Data/Images".to_string(),
        res_type: u32::from_be_bytes(*b"PICT"),
        res_id: 128,
        name: b"Title".to_vec(),
        data: pict.clone(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    for (offset, byte) in pict.iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(ptr + offset as u32), Some(byte));
    }
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_get_pix_pat_returns_fresh_resource_copies() {
    let pef = synthetic_pef_with_import(b"GetPixPat");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(42);
    let pattern = vec![
        0x00, 0x01, // patType
        0x00, 0x00, 0x00, 0x1c, // patMap offset
        0x00, 0x00, 0x00, 0x4e, // patData offset
        0x00, 0x00, 0x00, 0x00, // patXData
        0xff, 0xff, // patXValid
        0x00, 0x00, 0x00, 0x00, // patXMap
        0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, // pat1Data
    ];
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 42,
        path: "Escape Velocity".to_string(),
        res_type: u32::from_be_bytes(*b"ppat"),
        res_id: 129,
        name: b"Dialog Pattern".to_vec(),
        data: pattern.clone(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 129;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let first_handle = loaded.cpu.gpr[3];
    assert_ne!(first_handle, 0);
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);
    assert_eq!(
        ppc_handle_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            first_handle
        ),
        Some(pattern.clone())
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 129;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let second_handle = loaded.cpu.gpr[3];
    assert_ne!(second_handle, 0);
    assert_ne!(second_handle, first_handle);
    assert_eq!(
        ppc_handle_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            second_handle
        ),
        Some(pattern)
    );
}

#[test]
fn hle_import_runner_draws_picture_handle_into_current_gworld() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pict = test_v1_one_bit_packbits_pict();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &pict,
    );
    assert_ne!(handle, 0);
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 5, 6, 6, 14).unwrap();

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let red = ppc_rgb_color_to_rgb555(PpcRgbColor {
        red: 0xffff,
        green: 0,
        blue: 0,
    });
    let red_index = u16::from(ppc_rgb555_to_clut_index(red, &loaded.screen_clut));
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (6, 5),
        red_index,
    ));
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (7, 5),
        red_index,
    ));
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (20, 5),
        red_index,
    ));

    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (6, 5)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_BLACK)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (7, 5)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_WHITE)))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (20, 5)),
        Some(red_index)
    );
}

#[test]
fn hle_import_runner_draws_picture_handle_into_8bpp_current_gworld() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pict = test_v1_one_bit_packbits_pict();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &pict,
    );
    assert_ne!(handle, 0);
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 5, 6, 6, 14).unwrap();

    let gworld = PPC_HEAP_BASE + 0x1000;
    let pix_base = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(pix_base, vec![42; 8 * 8]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pix_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 8,
        row_bytes: 8,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = gworld);

    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(pix_base + 5 * 8 + 6), Some(255));
    assert_eq!(loaded.memory.read_u8(pix_base + 5 * 8 + 7), Some(0));
    assert_eq!(loaded.memory.read_u8(pix_base + 5 * 8), Some(42));
}

#[test]
fn hle_import_runner_draws_picture_handle_into_4bpp_current_gworld() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pict = test_v1_one_bit_packbits_pict();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &pict,
    );
    assert_ne!(handle, 0);
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 5, 0, 6, 8).unwrap();

    let gworld = PPC_HEAP_BASE + 0x1000;
    let pix_base = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(pix_base, vec![0; 4 * 8]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pix_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 4,
        row_bytes: 4,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = gworld);

    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    // The source row alternates black/white for its first four pixels;
    // black is index 11 in the standard 4-bpp GWorld table. Packed output
    // must update both nibbles instead of reporting a successful parse
    // while leaving the destination bytes untouched.
    assert_eq!(loaded.memory.read_u8(pix_base + 5 * 4), Some(0xb0));
    assert_eq!(loaded.memory.read_u8(pix_base + 5 * 4 + 1), Some(0xb0));
    assert_eq!(loaded.memory.read_u8(pix_base + 4 * 4), Some(0));
    assert_eq!(loaded.memory.read_u8(pix_base + 6 * 4), Some(0));
}

#[test]
fn basic_grafport_uses_port_rect_when_copied_bitmap_bounds_exceed_row_bytes() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let port = PPC_HEAP_BASE + 0x1000;
    let pixels = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(port, vec![0xff; 0x80]);
    loaded.memory.add_region(pixels, vec![0; 8]);
    loaded.memory.write_u32_be(port + 2, pixels).unwrap();
    loaded.memory.write_u16_be(port + 6, 1).unwrap();
    ppc_write_rect(&mut loaded.memory, port + 8, 0, 0, 480, 640).unwrap();
    ppc_write_rect(&mut loaded.memory, port + 16, 0, 0, 8, 8).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pixels,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 1,
        row_bytes: 1,
        pixels_locked: false,
        pixels_no_purge: false,
    });

    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, port).unwrap();

    assert_eq!(surface.front_buffer.base_addr, pixels);
    assert_eq!(surface.front_buffer.row_bytes, 1);
    assert_eq!(surface.front_buffer.width, 8);
    assert_eq!(surface.front_buffer.height, 8);
    assert_eq!(surface.front_buffer.depth, 1);
}

#[test]
fn picture_renderer_draws_into_packed_one_bit_bitmap() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pixels = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(pixels, vec![0x55; 8]);
    let front = PpcFrontBuffer {
        base_addr: pixels,
        row_bytes: 1,
        width: 8,
        height: 8,
        depth: 1,
    };

    // Only entries 0 and 1 are representable. Black deliberately has an
    // exact match at the even index 42 in the inherited logical tail; an
    // unclamped 256-entry match would truncate that to bit 0 and invert
    // the source's black pixels.
    let mut one_bit_clut = [[0xffff; 3]; 256];
    one_bit_clut[1] = [0x1111, 0x1111, 0x1111];
    one_bit_clut[42] = [0, 0, 0];
    assert!(ppc_draw_pict_bytes_to_16bpp(
        &mut loaded.memory,
        front,
        &test_v1_one_bit_packbits_pict(),
        (5, 0, 6, 8),
        &one_bit_clut,
        0,
        false,
    ));
    assert_eq!(loaded.memory.read_u8(pixels + 5), Some(0b1010_0000));
    assert_eq!(loaded.memory.read_u8(pixels + 4), Some(0x55));
    assert_eq!(loaded.memory.read_u8(pixels + 6), Some(0x55));
}

#[test]
fn hle_import_runner_draws_raw_quilt_picture_frame_into_8bpp_current_gworld() {
    let pef = synthetic_pef_with_import(b"DrawPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut pict = Vec::new();
    pict.extend_from_slice(&56u16.to_be_bytes());
    pict.extend_from_slice(&[0, 0, 0, 0, 0, 2, 0, 3]);
    pict.extend_from_slice(&[0; 14]);
    let mut raw = vec![0u8; 32];
    raw[0] = 11;
    raw[1] = 12;
    raw[2] = 13;
    raw[16] = 21;
    raw[17] = 0;
    raw[18] = 23;
    pict.extend_from_slice(&raw);
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &pict,
    );
    assert_ne!(handle, 0);
    let rect_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 1, 2, 3, 5).unwrap();

    let gworld = PPC_HEAP_BASE + 0x1000;
    let pix_base = PPC_HEAP_BASE + 0x2000;
    loaded.memory.add_region(pix_base, vec![42; 8 * 8]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: gworld,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: pix_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 8,
        row_bytes: 8,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = gworld);

    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(pix_base + 8 + 2), Some(11));
    assert_eq!(loaded.memory.read_u8(pix_base + 8 + 3), Some(12));
    assert_eq!(loaded.memory.read_u8(pix_base + 8 + 4), Some(13));
    assert_eq!(loaded.memory.read_u8(pix_base + 16 + 2), Some(21));
    assert_eq!(loaded.memory.read_u8(pix_base + 16 + 3), Some(0));
    assert_eq!(loaded.memory.read_u8(pix_base + 16 + 4), Some(23));
    assert_eq!(loaded.memory.read_u8(pix_base), Some(42));

    assert!(ppc_draw_pict_bytes_to_16bpp(
        &mut loaded.memory,
        PpcFrontBuffer {
            base_addr: pix_base,
            row_bytes: 8,
            width: 8,
            height: 8,
            depth: 8,
        },
        &pict,
        (1, 2, 3, 5),
        &loaded.screen_clut,
        0,
        true,
    ));
    assert_eq!(
        loaded.memory.read_u8(pix_base + 16 + 3),
        Some(pict::closest_clut_index(0, 0, 0, &loaded.screen_clut))
    );
}

#[test]
fn quilt_img_compositing_mode_controls_whether_zero_is_opaque() {
    let picture_handle = PPC_HEAP_BASE + 0x1000;
    let mut img = vec![0; 18];
    img[10..12].copy_from_slice(&3u16.to_be_bytes());
    let mut resources = vec![
        PpcVfsResourceRecord {
            ref_num: 128,
            path: "Sprites/Robot.PICR".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 1000,
            name: Vec::new(),
            data: Vec::new(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: picture_handle,
        },
        PpcVfsResourceRecord {
            ref_num: 128,
            path: "Sprites/Robot.PICR".to_string(),
            res_type: u32::from_be_bytes(*b"#Img"),
            res_id: 1000,
            name: Vec::new(),
            data: img,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        },
    ];

    assert!(ppc_quilt_picture_zero_is_opaque(&resources, picture_handle));
    resources[1].data[10..12].copy_from_slice(&2u16.to_be_bytes());
    assert!(!ppc_quilt_picture_zero_is_opaque(
        &resources,
        picture_handle
    ));
}

#[test]
fn hle_import_runner_kill_picture_invalidates_tracked_handle() {
    let pef = synthetic_pef_with_import(b"KillPicture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &minimal_pict_bytes(),
    );
    assert_ne!(handle, 0);
    assert_eq!(test_handle_records!(loaded).len(), 1);

    loaded.cpu.gpr[3] = handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(test_handle_records!(loaded).is_empty());
    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
}

#[test]
fn hle_import_runner_handles_get_pict_info_zero_fill() {
    let pef = synthetic_pef_with_import(b"GetPictInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pict_info_ptr = PPC_HEAP_BASE;
    loaded
        .memory
        .add_region(pict_info_ptr, vec![0xaa; PPC_PICT_INFO_SIZE as usize]);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + PPC_PICT_INFO_SIZE;
    loaded.cpu.gpr[4] = pict_info_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    for offset in 0..PPC_PICT_INFO_SIZE {
        assert_eq!(loaded.memory.read_u8(pict_info_ptr + offset), Some(0));
    }
}

#[test]
fn hle_import_runner_get_pict_info_output_is_all_or_nothing() {
    let pef = synthetic_pef_with_import(b"GetPictInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let pict_info_ptr = 0x06ff_0000;
    loaded.memory.add_region(pict_info_ptr, vec![0xbb; 4]);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + PPC_PICT_INFO_SIZE;
    loaded.cpu.gpr[4] = pict_info_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_PARAM_ERR);
    for offset in 0..4 {
        assert_eq!(loaded.memory.read_u8(pict_info_ptr + offset), Some(0xbb));
    }
}

#[test]
fn hle_import_runner_handles_get_gworld_outputs() {
    let pef = synthetic_pef_with_import(b"GetGWorld");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 16]);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = scratch + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], scratch);
    assert_eq!(loaded.memory.read_u32_be(scratch), Some(PPC_MAIN_GWORLD));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 4),
        Some(PPC_MAIN_GDEVICE)
    );

    let pef = synthetic_pef_with_import(b"GetPort");
    let mut loaded = load_pef_application(&pef).unwrap();
    let port_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(port_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = port_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(port_ptr), Some(PPC_MAIN_GWORLD));

    let pef = synthetic_pef_with_import(b"GetWMgrPort");
    let mut loaded = load_pef_application(&pef).unwrap();
    let port_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(port_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = port_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(port_ptr), Some(PPC_MAIN_GWORLD));
}

#[test]
fn hle_import_runner_handles_set_gworld_state() {
    let pef = synthetic_pef_with_import(b"SetGWorld");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u16_be(PPC_HEAP_BASE + PPC_CGRAF_PORT_PN_LOC_OFFSET, 23)
        .unwrap();
    loaded
        .memory
        .write_u16_be(PPC_HEAP_BASE + PPC_CGRAF_PORT_PN_LOC_OFFSET + 2, 17)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;
    loaded.cpu.gpr[4] = PPC_HEAP_BASE + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(*loaded.current_gworld, PPC_HEAP_BASE);
    assert_eq!(*loaded.current_gdevice, PPC_HEAP_BASE + 4);
    assert_eq!(loaded.quickdraw_pen_h, 17);
    assert_eq!(loaded.quickdraw_pen_v, 23);
}


#[test]
fn hle_import_runner_scrollrect_clips_and_reports_exact_l_shape() {
    let pef = synthetic_pef_with_import(b"ScrollRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE + 0x14000;
    let rect_ptr = scratch + 0x80;
    let pixpat_handle = scratch + 0x1c0;
    let pixpat_ptr = scratch + 0x1c4;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    // Replace the main PixMap's live base/bounds with a small test raster;
    // the live-port resolver must follow these fields rather than the
    // creation-time GWorld record.
    ppc_write_pixmap(
        &mut loaded.memory,
        PPC_MAIN_PIXMAP,
        scratch,
        8,
        0,
        0,
        6,
        6,
        8,
    )
    .unwrap();
    for y in 0..8u32 {
        for x in 0..8u32 {
            loaded
                .memory
                .write_u8(scratch + y * 8 + x, (100 + y * 8 + x) as u8)
                .unwrap();
        }
    }
    // A live bkPixPat should supply the fill pattern in preference to
    // the legacy cache. The differing fallback makes that precedence
    // observable in the first exposed row.
    loaded
        .memory
        .write_u32_be(pixpat_handle, pixpat_ptr)
        .unwrap();
    loaded
        .memory
        .write_bytes(pixpat_ptr + 20, &[0x80, 0, 0, 0, 0, 0, 0, 0])
        .unwrap();
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_BK_PIXPAT_OFFSET, pixpat_handle)
        .unwrap();
    loaded.toolbox_startup.quickdraw_back_pattern = [0; 8];
    let region = {
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
        loaded.cpu.gpr[3]
    };
    ppc_write_rect(&mut loaded.memory, rect_ptr, -1, -1, 5, 5).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = region;

    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::ScrollRect,
        ),
    );

    // The requested rect is clipped to [0,5)×[0,5), and the source is
    // snapshotted before overlapping writes. x=5/y=5 remain untouched.
    assert_eq!(loaded.memory.read_u8(scratch), Some(255));
    assert_eq!(loaded.memory.read_u8(scratch + 1), Some(0));
    assert_eq!(loaded.memory.read_u8(scratch + 8), Some(0));
    assert_eq!(loaded.memory.read_u8(scratch + 2 + 2 * 8), Some(100));
    assert_eq!(loaded.memory.read_u8(scratch + 5), Some(105));
    assert_eq!(loaded.memory.read_u8(scratch + 5 * 8), Some(140));

    let storage = ppc_region_storage(&mut loaded.memory, region).unwrap();
    assert_eq!(ppc_region_storage_bbox(&storage), Some((0, 0, 5, 5)));
    assert!(ppc_point_in_region_storage(&storage, 0, 4));
    assert!(ppc_point_in_region_storage(&storage, 1, 0));
    assert!(ppc_point_in_region_storage(&storage, 2, 1));
    assert!(!ppc_point_in_region_storage(&storage, 2, 2));
    assert!(!ppc_point_in_region_storage(&storage, 4, 2));
    assert!(!ppc_point_in_region_storage(&storage, 5, 0));
    assert_eq!(loaded.cpu.gpr[3], rect_ptr);
}

#[test]
fn hle_import_runner_backpat_and_backpixpat_install_fill_patterns() {
    let backpat_pef = synthetic_pef_with_import(b"BackPat");
    let mut loaded = load_pef_application(&backpat_pef).unwrap();
    let pattern_ptr = PPC_HEAP_BASE + 0x14200;
    loaded.memory.add_region(pattern_ptr, vec![0; 8]);
    loaded.memory.write_bytes(pattern_ptr, &[0x80; 8]).unwrap();
    loaded.cpu.gpr[3] = pattern_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::BackPat,
        ),
    );
    assert_eq!(loaded.toolbox_startup.quickdraw_back_pattern, [0x80; 8]);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_BK_PIXPAT_OFFSET),
        Some(0),
        "BackPat must clear a stale bkPixPat so its Pattern is authoritative"
    );

    let pixpat_pef = synthetic_pef_with_import(b"BackPixPat");
    let mut loaded = load_pef_application(&pixpat_pef).unwrap();
    let scratch = PPC_HEAP_BASE + 0x14400;
    let pattern_handle = scratch;
    let pattern_record = scratch + 4;
    loaded.memory.add_region(scratch, vec![0; 0x40]);
    loaded.memory.write_u32_be(pattern_handle, pattern_record).unwrap();
    loaded
        .memory
        .write_bytes(pattern_record + 20, &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0])
        .unwrap();
    loaded.cpu.gpr[3] = pattern_handle;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::BackPixPat,
        ),
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_BK_PIXPAT_OFFSET),
        Some(pattern_handle)
    );
    assert_eq!(
        loaded.toolbox_startup.quickdraw_back_pattern,
        [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]
    );
}

#[test]
fn hle_import_runner_handles_set_rect() {
    let pef = synthetic_pef_with_import(b"SetRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(rect_ptr, vec![0xaa; 8]);
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 20; // left
    loaded.cpu.gpr[5] = 10; // top
    loaded.cpu.gpr[6] = 120; // right
    loaded.cpu.gpr[7] = 80; // bottom

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], rect_ptr);
    assert_eq!(loaded.memory.read_u16_be(rect_ptr), Some(10));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 2), Some(20));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 4), Some(80));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 6), Some(120));
}

#[test]
fn hle_import_runner_handles_union_rect_with_aliased_destination() {
    let pef = synthetic_pef_with_import(b"UnionRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let first = PPC_HEAP_BASE;
    let second = PPC_HEAP_BASE + 8;
    loaded.memory.add_region(first, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, first, 10, 30, 80, 100).unwrap();
    ppc_write_rect(&mut loaded.memory, second, -5, 40, 60, 120).unwrap();
    loaded.cpu.gpr[3] = first;
    loaded.cpu.gpr[4] = second;
    loaded.cpu.gpr[5] = first;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, first),
        Some((-5, 30, 80, 120))
    );
}

#[test]
fn hle_import_runner_handles_empty_rect() {
    let pef = synthetic_pef_with_import(b"EmptyRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 10, 40).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn hle_import_runner_handles_set_pt() {
    let pef = synthetic_pef_with_import(b"SetPt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
    loaded.cpu.gpr[3] = point_ptr;
    loaded.cpu.gpr[4] = 123; // h
    loaded.cpu.gpr[5] = 45; // v

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(45));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(123));
}

#[test]
fn hle_import_runner_handles_equal_pt() {
    let pef = synthetic_pef_with_import(b"EqualPt");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = (45u32 << 16) | 123;
    loaded.cpu.gpr[4] = (45u32 << 16) | 123;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = (45u32 << 16) | 123;
    loaded.cpu.gpr[4] = (46u32 << 16) | 123;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn quickdraw_point_transforms_use_current_port_offset() {
    let pef = synthetic_pef_with_import(b"SetPt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(point_ptr, vec![0; 4]);
    ppc_write_rect(&mut loaded.memory, PPC_MAIN_GWORLD + 16, 0, 0, 460, 590).unwrap();
    ppc_write_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6, -20, -50, 460, 590).unwrap();
    loaded.memory.write_u16_be(point_ptr, 100).unwrap();
    loaded.memory.write_u16_be(point_ptr + 2, 200).unwrap();

    ppc_transform_port_point(&mut loaded.memory, PPC_MAIN_GWORLD, point_ptr, true).unwrap();
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(120));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(250));

    ppc_transform_port_point(&mut loaded.memory, PPC_MAIN_GWORLD, point_ptr, false).unwrap();
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(100));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(200));
}

#[test]
fn hle_import_runner_handles_pt_in_rect() {
    let pef = synthetic_pef_with_import(b"PtInRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(rect_ptr, vec![0xaa; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 30, 40).unwrap();
    loaded.cpu.gpr[3] = (10u32 << 16) | 20;
    loaded.cpu.gpr[4] = rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = (30u32 << 16) | 20;
    loaded.cpu.gpr[4] = rect_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_offset_rect() {
    let pef = synthetic_pef_with_import(b"OffsetRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(rect_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 80, 120).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = (-3i16 as u16) as u32; // dh
    loaded.cpu.gpr[5] = 7; // dv

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], rect_ptr);
    assert_eq!(loaded.memory.read_u16_be(rect_ptr), Some(17));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 2), Some(17));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 4), Some(87));
    assert_eq!(loaded.memory.read_u16_be(rect_ptr + 6), Some(117));
}

#[test]
fn hle_import_runner_handles_map_rect() {
    let pef = synthetic_pef_with_import(b"MapRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_HEAP_BASE;
    let src_ptr = rect_ptr + 8;
    let dst_ptr = src_ptr + 8;
    loaded.memory.add_region(rect_ptr, vec![0; 24]);
    ppc_write_rect(&mut loaded.memory, rect_ptr, 20, 25, 60, 75).unwrap();
    ppc_write_rect(&mut loaded.memory, src_ptr, 10, 20, 110, 120).unwrap();
    ppc_write_rect(&mut loaded.memory, dst_ptr, -30, 200, 170, 600).unwrap();
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = src_ptr;
    loaded.cpu.gpr[5] = dst_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, rect_ptr),
        Some((-10, 220, 70, 420))
    );
}

#[test]
fn hle_import_runner_quickdraw_outputs_are_all_or_nothing() {
    let pef = synthetic_pef_with_import(b"GetForeColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let color_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(color_ptr, vec![0xab; 4]);
    loaded.cpu.gpr[3] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(color_ptr), Some(0xabab_abab));

    let pef = synthetic_pef_with_import(b"GetPort");
    let mut loaded = load_pef_application(&pef).unwrap();
    let port_ptr = PPC_DATA_BASE + 0x1080;
    loaded.memory.add_region(port_ptr, vec![0xba; 2]);
    loaded.cpu.gpr[3] = port_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(port_ptr), Some(0xbaba));

    let pef = synthetic_pef_with_import(b"GetGWorld");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(scratch, vec![0xcc; 6]);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = scratch + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for offset in 0..6 {
        assert_eq!(loaded.memory.read_u8(scratch + offset), Some(0xcc));
    }

    let pef = synthetic_pef_with_import(b"DMGetGDeviceByDisplayID");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gdevice_ptr = PPC_DATA_BASE + 0x1180;
    loaded.memory.add_region(gdevice_ptr, vec![0xcd; 2]);
    loaded.cpu.gpr[3] = PPC_DSP_DISPLAY_ID;
    loaded.cpu.gpr[4] = gdevice_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u16_be(gdevice_ptr), Some(0xcdcd));

    let pef = synthetic_pef_with_import(b"SetRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(rect_ptr, vec![0xdd; 4]);
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 20;
    loaded.cpu.gpr[5] = 10;
    loaded.cpu.gpr[6] = 120;
    loaded.cpu.gpr[7] = 80;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(rect_ptr), Some(0xdddd_dddd));

    let pef = synthetic_pef_with_import(b"OffsetRect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rect_ptr = PPC_DATA_BASE + 0x1300;
    loaded.memory.add_region(rect_ptr, vec![0xee; 4]);
    loaded.cpu.gpr[3] = rect_ptr;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = 7;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(rect_ptr), Some(0xeeee_eeee));
}

