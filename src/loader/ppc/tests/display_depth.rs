use super::*;

#[test]
fn hle_import_runner_handles_test_device_attribute() {
    let pef = synthetic_pef_with_import(b"TestDeviceAttribute");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 13;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xffff);

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 14;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_has_depth_only_advertises_settable_modes() {
    let pef = synthetic_pef_with_import(b"HasDepth");
    let cases = [
        (1, 0, 1, 1, false),
        (2, 0, 1, 2, true),
        (4, 0, 1, 4, true),
        (8, 0, 0, 8, true),
        (16, 0, 0, 16, true),
        (32, 0, 1, 0, false),
        (99, 0, 1, 0, false),
        (1, 1, 0, 1, false),
        (1, 1, 1, 0, false),
        (2, 1, 0, 2, false),
        (2, 1, 1, 2, true),
        (4, 1, 0, 4, false),
        (4, 1, 1, 4, true),
        (8, 1, 0, 8, false),
        (8, 1, 1, 8, true),
        (16, 1, 0, 0, false),
        (16, 1, 1, 16, true),
        (8, 1 << 11, 1 << 11, 8, true),
        (8, 1 << 11, 0, 0, false),
    ];

    for (depth, which_flags, flags, expected_depth, expected_color) in cases {
        let expected_mode = if expected_depth == 0 {
            0
        } else {
            u32::from(crate::display::classic_depth_mode(expected_depth).unwrap())
        };
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = which_flags;
        loaded.cpu.gpr[6] = flags;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HasDepth);
        assert_eq!(
            loaded.cpu.gpr[3], expected_mode,
            "unexpected HasDepth result for depth={depth} whichFlags={which_flags:#06x} flags={flags:#06x}"
        );

        if expected_mode != 0 {
            loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
            loaded.cpu.gpr[4] = expected_mode;
            loaded.cpu.gpr[5] = which_flags;
            loaded.cpu.gpr[6] = flags;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
                Some(expected_mode)
            );
            assert_eq!(
                loaded
                    .memory
                    .read_u16_be(PPC_MAIN_GDEVICE_RECORD + 20)
                    .unwrap()
                    & 1
                    != 0,
                expected_color
            );
        }
    }

    for gdevice in [0, 0x06ff_0000] {
        for depth in [1, 2, 4, 8, 16] {
            let mut loaded = load_pef_application(&pef).unwrap();
            loaded.cpu.gpr[3] = gdevice;
            loaded.cpu.gpr[4] = depth;
            loaded.cpu.gpr[5] = 1;
            loaded.cpu.gpr[6] = u32::from(depth != 1);
            run_test_import(&mut loaded, PpcImportDispatcherTarget::HasDepth);
            let mode = loaded.cpu.gpr[3];
            assert_eq!(
                mode,
                if gdevice == 0 {
                    u32::from(crate::display::classic_depth_mode(depth as u16).unwrap())
                } else {
                    0
                },
                "NIL aliases the main device; other invalid handles do not"
            );
            if mode != 0 {
                loaded.cpu.gpr[3] = gdevice;
                loaded.cpu.gpr[4] = mode;
                loaded.cpu.gpr[5] = 1;
                loaded.cpu.gpr[6] = u32::from(depth != 1);
                run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
        }
    }
}

#[test]
fn hle_import_runner_set_depth_installs_each_advertised_screen_personality() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    for (depth, is_color) in [
        (1, false),
        (2, false),
        (2, true),
        (4, false),
        (4, true),
        (8, false),
        (8, true),
        (16, true),
    ] {
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(is_color);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_NO_ERR),
            "SetDepth rejected {depth}-bit {}",
            if is_color { "color" } else { "grayscale" }
        );
        let row_bytes = ppc_row_bytes(ppc_main_screen_width(), depth).unwrap();
        assert_eq!(loaded.gworlds[0].depth, depth);
        assert_eq!(loaded.gworlds[0].row_bytes, row_bytes);
        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 4),
            Some(0x8000 | row_bytes as u16)
        );
        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32),
            Some(depth as u16)
        );
        assert_eq!(
            loaded
                .memory
                .read_u16_be(PPC_MAIN_GDEVICE_RECORD + 20)
                .unwrap()
                & 1
                != 0,
            is_color
        );
        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_GDEVICE_RECORD + 4),
            Some(if depth <= 8 { 0 } else { 2 })
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
            Some(u32::from(
                crate::display::classic_depth_mode(depth as u16).unwrap()
            )),
            "gdMode for depth {depth}"
        );
        if depth <= 8 {
            let (expected_clut, entry_count) =
                ppc_standard_screen_clut(depth, is_color).unwrap();
            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42),
                Some(PPC_MAIN_CTABLE_HANDLE)
            );
            assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 30), Some(0));
            assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 34), Some(1));
            assert_eq!(
                loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 36),
                Some(depth as u16)
            );
            assert_eq!(
                loaded.memory.read_u16_be(PPC_MAIN_CTABLE + 6),
                Some((entry_count - 1) as u16)
            );
            assert_eq!(
                loaded.memory.read_u16_be(PPC_MAIN_CTABLE + 4).unwrap() & 0x8000,
                0x8000
            );
            for (index, expected) in expected_clut.into_iter().take(entry_count).enumerate() {
                let entry = PPC_MAIN_CTABLE + 8 + index as u32 * 8;
                assert_eq!(loaded.memory.read_u16_be(entry), Some(index as u16));
                assert_eq!(loaded.memory.read_u16_be(entry + 2), Some(expected[0]));
                assert_eq!(loaded.memory.read_u16_be(entry + 4), Some(expected[1]));
                assert_eq!(loaded.memory.read_u16_be(entry + 6), Some(expected[2]));
            }
            assert_eq!(loaded.screen_clut, expected_clut);
            assert_eq!(loaded.color_manager_clut, expected_clut);
        } else {
            assert_eq!(loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42), Some(0));
            assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 30), Some(16));
            assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 34), Some(3));
            assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 36), Some(5));
        }
    }
}

#[test]
fn hle_import_runner_set_depth_grows_private_screen_ctable_without_replacing_handle() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let private_ctable_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ppc_default_color_table_bytes(1).unwrap(),
    );
    assert_ne!(private_ctable_handle, 0);
    let original_ptr = loaded.memory.read_u32_be(private_ctable_handle).unwrap();
    loaded
        .memory
        .write_u16_be(original_ptr + 4, 0x1234)
        .unwrap();
    let blocker = ppc_alloc_handle(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        32,
        true,
    );
    assert_ne!(blocker, 0);

    let scratch = PPC_DATA_BASE + 0xb000;
    let port = scratch;
    let pixmap_handle = scratch + 0x100;
    let pixmap = scratch + 0x110;
    loaded.memory.add_region(scratch, vec![0; 0x160]);
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
    ppc_write_gworld_port(
        &mut loaded.memory,
        port,
        pixmap_handle,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    )
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

    for depth in [4, 8, 1] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(pixmap + 42),
            Some(private_ctable_handle)
        );
        let table = loaded.memory.read_u32_be(private_ctable_handle).unwrap();
        assert_eq!(loaded.memory.read_u16_be(table + 4), Some(0x1234));
        assert_eq!(
            loaded.memory.read_u16_be(table + 6),
            Some(((1u32 << depth) - 1) as u16)
        );
    }

    let grown = loaded
        .handles()
        .iter()
        .find(|record| record.handle == private_ctable_handle)
        .copied()
        .unwrap();
    assert_ne!(grown.ptr, original_ptr);
    assert_eq!(
        grown.ptr,
        loaded.memory.read_u32_be(private_ctable_handle).unwrap()
    );
    assert_eq!(grown.size, PPC_MAIN_CTABLE_SIZE);
    assert_eq!(grown.capacity, PPC_MAIN_CTABLE_SIZE);
}

#[test]
fn hle_import_runner_set_depth_private_ctable_growth_failure_is_atomic() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let private_ctable_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ppc_default_color_table_bytes(1).unwrap(),
    );
    assert_ne!(private_ctable_handle, 0);
    let scratch = PPC_DATA_BASE + 0xb200;
    let port = scratch;
    let pixmap_handle = scratch + 0x100;
    let pixmap = scratch + 0x110;
    loaded.memory.add_region(scratch, vec![0; 0x160]);
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
    ppc_write_gworld_port(
        &mut loaded.memory,
        port,
        pixmap_handle,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    )
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
    loaded
        .memory
        .write_bytes(
            PPC_MAIN_SCREEN_BASE,
            &vec![0x5a; ppc_main_screen_buffer_size() as usize],
        )
        .unwrap();

    let before_heap_cursor = loaded.heap_cursor();
    let before_handles = test_handle_records!(loaded).clone();
    let before_gworlds = loaded.gworlds.clone();
    let before_main_pixmap =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap();
    let before_private_pixmap =
        ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE).unwrap();
    let before_gdevice = ppc_memory_read_bytes(
        &mut loaded.memory,
        PPC_MAIN_GDEVICE_RECORD,
        PPC_GDEVICE_SIZE,
    )
    .unwrap();
    let before_main_ctable =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
            .unwrap();
    let private_ptr = loaded.memory.read_u32_be(private_ctable_handle).unwrap();
    let before_private_ctable =
        ppc_memory_read_bytes(&mut loaded.memory, private_ptr, 24).unwrap();
    let before_framebuffer = ppc_memory_read_bytes(
        &mut loaded.memory,
        PPC_MAIN_SCREEN_BASE,
        ppc_main_screen_buffer_size(),
    )
    .unwrap();
    let before_startup = loaded.toolbox_startup.clone();
    let before_screen_clut = *loaded.screen_clut;
    let before_color_manager_clut = *loaded.color_manager_clut;
    loaded.set_heap_limit(loaded.heap_cursor() + 32);

    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.heap_cursor(), before_heap_cursor);
    assert_eq!(test_handle_records!(loaded), before_handles);
    assert_eq!(loaded.gworlds, before_gworlds);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE),
        Some(before_main_pixmap)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE),
        Some(before_private_pixmap)
    );
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        ),
        Some(before_gdevice)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE),
        Some(before_main_ctable)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, private_ptr, 24),
        Some(before_private_ctable)
    );
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_SCREEN_BASE,
            ppc_main_screen_buffer_size(),
        ),
        Some(before_framebuffer)
    );
    assert_eq!(loaded.toolbox_startup, before_startup);
    assert_eq!(loaded.screen_clut, before_screen_clut);
    assert_eq!(loaded.color_manager_clut, before_color_manager_clut);
}

#[test]
fn hle_import_runner_set_depth_rejects_undersized_untracked_ctable_without_overwrite() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0xb600;
    let private_ctable_handle = scratch;
    let private_ctable = scratch + 0x20;
    let private_ctable_bytes = ppc_default_color_table_bytes(1).unwrap();
    let canary = private_ctable + private_ctable_bytes.len() as u32;
    let port = scratch + 0x200;
    let pixmap_handle = scratch + 0x300;
    let pixmap = scratch + 0x310;
    loaded.memory.add_region(scratch, vec![0; 0x380]);
    loaded
        .memory
        .write_u32_be(private_ctable_handle, private_ctable)
        .unwrap();
    loaded
        .memory
        .write_bytes(private_ctable, &private_ctable_bytes)
        .unwrap();
    loaded.memory.write_bytes(canary, &[0xa5; 64]).unwrap();
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
    ppc_write_gworld_port(
        &mut loaded.memory,
        port,
        pixmap_handle,
        0,
        0,
        ppc_main_screen_height() as i16,
        ppc_main_screen_width() as i16,
    )
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

    let before_heap_cursor = loaded.heap_cursor();
    let before_handles = test_handle_records!(loaded).clone();
    let before_gworlds = loaded.gworlds.clone();
    let before_private_table = ppc_memory_read_bytes(
        &mut loaded.memory,
        private_ctable,
        private_ctable_bytes.len() as u32,
    )
    .unwrap();
    let before_canary = ppc_memory_read_bytes(&mut loaded.memory, canary, 64).unwrap();
    let before_main_pixmap =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap();
    let before_private_pixmap =
        ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE).unwrap();
    let before_gdevice = ppc_memory_read_bytes(
        &mut loaded.memory,
        PPC_MAIN_GDEVICE_RECORD,
        PPC_GDEVICE_SIZE,
    )
    .unwrap();

    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.heap_cursor(), before_heap_cursor);
    assert_eq!(test_handle_records!(loaded), before_handles);
    assert_eq!(loaded.gworlds, before_gworlds);
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            private_ctable,
            private_ctable_bytes.len() as u32,
        ),
        Some(before_private_table)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, canary, 64),
        Some(before_canary)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE),
        Some(before_main_pixmap)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE),
        Some(before_private_pixmap)
    );
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        ),
        Some(before_gdevice)
    );
}

#[test]
fn ppc_preflight_ctable_growth_rejects_invalid_untracked_ctsize() {
    for ct_size in [256, u16::MAX] {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0xb800;
        let ctable_handle = scratch;
        let ctable = scratch + 0x20;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);
        loaded.memory.write_u32_be(ctable_handle, ctable).unwrap();
        loaded.memory.write_u16_be(ctable + 6, ct_size).unwrap();
        let heap_cursor = loaded.heap_cursor();
        let heap_limit = test_heap_limit!(loaded);

        assert_eq!(
            ppc_preflight_ctable_growth(
                &mut loaded.memory,
                &[],
                &[ctable_handle],
                PPC_MAIN_CTABLE_SIZE,
                heap_cursor,
                heap_limit,
            ),
            Err(PPC_PARAM_ERR),
            "ctSize={ct_size:#06x}"
        );
    }
}

#[test]
fn hle_import_runner_set_depth_rejections_are_atomic() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let rejected = [
        (PPC_MAIN_GDEVICE, 1, 1, 1),
        (PPC_MAIN_GDEVICE, 16, 1, 0),
        (PPC_MAIN_GDEVICE, 32, 0, 1),
        (PPC_MAIN_GDEVICE, 99, 0, 1),
        (PPC_MAIN_GDEVICE, 8, 1 << 11, 0),
        (0x06ff_0000, 16, 0, 1),
    ];

    for (gdevice, depth, which_flags, flags) in rejected {
        let mut loaded = load_pef_application(&pef).unwrap();
        let globals_base = PPC_DATA_BASE + 0x6000;
        let global_ptr = globals_base + 126;
        loaded.memory.add_region(globals_base, vec![0x6a; 130]);
        assert!(ppc_init_graf(
            &mut loaded.memory,
            &loaded.gworlds,
            global_ptr
        ));
        loaded.toolbox_startup.init_graf_global_ptr = global_ptr;
        loaded
            .memory
            .write_u16_be(PPC_MAIN_CTABLE + 8 + 37 * 8 + 2, 0x1234)
            .unwrap();
        loaded
            .memory
            .write_bytes(
                PPC_MAIN_SCREEN_BASE,
                &vec![0x5a; ppc_main_screen_buffer_size() as usize],
            )
            .unwrap();

        let before_gworlds = loaded.gworlds.clone();
        let before_pixmaps = loaded
            .gworlds
            .iter()
            .map(|record| {
                (
                    record.pixmap,
                    ppc_memory_read_bytes(&mut loaded.memory, record.pixmap, PPC_PIXMAP_SIZE)
                        .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let before_gdevice_handle =
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_GDEVICE, 4).unwrap();
        let before_gdevice = ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        )
        .unwrap();
        let before_ctable_handle =
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE_HANDLE, 4).unwrap();
        let before_ctable =
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
                .unwrap();
        let before_globals =
            ppc_memory_read_bytes(&mut loaded.memory, globals_base, 130).unwrap();
        let before_framebuffer = ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_SCREEN_BASE,
            ppc_main_screen_buffer_size(),
        )
        .unwrap();
        let before_screen_clut = *loaded.screen_clut;
        let before_color_manager_clut = *loaded.color_manager_clut;
        let before_toolbox_startup = loaded.toolbox_startup.clone();

        loaded.cpu.gpr[3] = gdevice;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = which_flags;
        loaded.cpu.gpr[6] = flags;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_PARAM_ERR),
            "unexpected result for gdevice={gdevice:#010x} depth={depth} whichFlags={which_flags:#06x} flags={flags:#06x}"
        );
        assert_eq!(loaded.gworlds, before_gworlds);
        for (pixmap, bytes) in before_pixmaps {
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE),
                Some(bytes)
            );
        }
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_GDEVICE, 4),
            Some(before_gdevice_handle)
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                PPC_MAIN_GDEVICE_RECORD,
                PPC_GDEVICE_SIZE,
            ),
            Some(before_gdevice)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE_HANDLE, 4),
            Some(before_ctable_handle)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE,),
            Some(before_ctable)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, globals_base, 130),
            Some(before_globals)
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                PPC_MAIN_SCREEN_BASE,
                ppc_main_screen_buffer_size(),
            ),
            Some(before_framebuffer)
        );
        assert_eq!(loaded.screen_clut, before_screen_clut);
        assert_eq!(loaded.color_manager_clut, before_color_manager_clut);
        assert_eq!(loaded.toolbox_startup, before_toolbox_startup);
    }
}

#[test]
fn hle_import_runner_set_depth_cancels_tracking_only_after_success() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let tracking = PpcMenuTracking {
        kind: MenuTrackingKind::MenuBar,
        menu_handle: 0x1234,
        popup_left: 11,
        popup_top: 20,
        content_top: 20,
        scroll_direction: None,
        popup_width: 32,
        popup_height: 20,
        highlighted_item: 0,
        definition: None,
        flash_remaining: 0,
        flash_tick: None,
        flash_deadline: 0,
        flash_result: 0,
        saved_width: 33,
        saved_height: 21,
        front_buffer: Some(
            ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap()
                .into(),
        ),
        saved_pixels: vec![0; 33 * 21].into(),
        item_appearances: Vec::new(),
        submenus: Vec::new(),
    };
    loaded
        .toolbox_startup
        .execution
        .set_menu_state(Some(tracking.clone()));
    loaded.memory.write_u16_be(PPC_THE_MENU_ADDR, 128).unwrap();

    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 32;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.toolbox_startup.execution.menu().as_ref(), Some(&tracking));
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(128));

    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 16;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.toolbox_startup.execution.menu().is_none());
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));

    let mut popup_tracking = tracking;
    popup_tracking.kind = MenuTrackingKind::PopUp;
    loaded.toolbox_startup.execution.with_menu_context_mut(|context| {
        context.call = Some(MenuTrackingCall {
            request: MenuTrackingRequest::PopUp(PopupMenuRequest {
                menu_handle: popup_tracking.menu_handle,
                anchor: (100, 50),
                requested_item: 1,
            }),
            origin: MenuTrackingOrigin::PowerPc {
                stack_pointer: loaded.cpu.gpr[1],
                return_address: loaded.cpu.lr,
            },
        });
    });
    loaded
        .toolbox_startup
        .execution
        .set_menu_state(Some(popup_tracking));
    loaded
        .memory
        .write_u16_be(PPC_THE_MENU_ADDR, 0x2468)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.toolbox_startup.execution.menu().is_none());
    assert_eq!(
        loaded.memory.read_u16_be(PPC_THE_MENU_ADDR),
        Some(0x2468),
        "SetDepth cancellation gave popup tracking ownership of TheMenu"
    );
}

#[test]
fn hle_import_runner_set_depth_preserves_device_state_and_clear_bounds() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let gdevice = PPC_MAIN_GDEVICE_RECORD;
    let custom_flags = (PPC_MAIN_GDEVICE_FLAGS | (1 << 7)) & !(1 << 10);
    loaded.memory.write_u16_be(gdevice, 0x1234).unwrap();
    loaded.memory.write_u16_be(gdevice + 2, 0x2345).unwrap();
    loaded
        .memory
        .write_u32_be(gdevice + 6, 0x3456_789a)
        .unwrap();
    loaded.memory.write_u16_be(gdevice + 10, 5).unwrap();
    loaded
        .memory
        .write_u32_be(gdevice + 12, 0x4567_89ab)
        .unwrap();
    loaded
        .memory
        .write_u32_be(gdevice + 16, 0x5678_9abc)
        .unwrap();
    loaded
        .memory
        .write_u16_be(gdevice + 20, custom_flags)
        .unwrap();
    loaded
        .memory
        .write_u32_be(gdevice + 26, 0x6789_abcd)
        .unwrap();
    loaded
        .memory
        .write_u32_be(gdevice + 30, 0x789a_bcde)
        .unwrap();
    let cursor_fields = (0u8..16).map(|value| value ^ 0xa5).collect::<Vec<_>>();
    loaded
        .memory
        .write_bytes(gdevice + 46, &cursor_fields)
        .unwrap();
    let gd_rect = ppc_memory_read_bytes(&mut loaded.memory, gdevice + 34, 8).unwrap();
    let gd_pmap = loaded.memory.read_u32_be(gdevice + 22).unwrap();

    let ctable_before_canary = PPC_MAIN_CTABLE - 32;
    let ctable_after_canary = PPC_MAIN_CTABLE + PPC_MAIN_CTABLE_SIZE;
    let framebuffer_after_canary = PPC_MAIN_SCREEN_BASE + ppc_main_screen_buffer_size();
    loaded
        .memory
        .add_region(ctable_before_canary, vec![0xb1; 32]);
    loaded
        .memory
        .add_region(ctable_after_canary, vec![0xb2; 32]);
    loaded
        .memory
        .add_region(framebuffer_after_canary, vec![0xb3; 32]);
    loaded
        .memory
        .write_u16_be(PPC_MAIN_CTABLE + 8 + 37 * 8 + 2, 0x1357)
        .unwrap();
    loaded
        .memory
        .write_u16_be(PPC_MAIN_CTABLE + 8 + 37 * 8 + 4, 0x2468)
        .unwrap();
    let ctable_handle_before = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
    let ctable_before =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
            .unwrap();

    for depth in [16, 8, 4, 2, 1] {
        let storage_size = ppc_main_screen_buffer_size() as usize;
        loaded
            .memory
            .write_bytes(PPC_MAIN_SCREEN_BASE, &vec![0x5a; storage_size])
            .unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        let row_bytes = ppc_row_bytes(ppc_main_screen_width(), depth).unwrap();
        let clear_len = (row_bytes * ppc_main_screen_height()) as usize;
        let framebuffer = ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_SCREEN_BASE,
            ppc_main_screen_buffer_size(),
        )
        .unwrap();
        let clear_byte = if depth == 8 { 0xff } else { 0x00 };
        assert!(framebuffer[..clear_len]
            .iter()
            .all(|byte| *byte == clear_byte));
        assert!(framebuffer[clear_len..].iter().all(|byte| *byte == 0x5a));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, framebuffer_after_canary, 32),
            Some(vec![0xb3; 32])
        );

        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 4),
            Some(0x8000 | row_bytes as u16)
        );
        assert_eq!(
            (
                loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 30),
                loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32),
                loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 34),
                loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 36),
                loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42),
            ),
            if depth <= 8 {
                (
                    Some(0),
                    Some(depth as u16),
                    Some(1),
                    Some(depth as u16),
                    Some(PPC_MAIN_CTABLE_HANDLE),
                )
            } else {
                (Some(16), Some(16), Some(3), Some(5), Some(0))
            }
        );
        assert_eq!(
            loaded.memory.read_u16_be(gdevice + 4),
            Some(if depth <= 8 { 0 } else { 2 })
        );
        assert_eq!(
            loaded.memory.read_u16_be(gdevice + 20),
            Some(if depth == 1 {
                custom_flags & !1
            } else {
                custom_flags | 1
            })
        );
        assert_eq!(loaded.memory.read_u32_be(gdevice + 22), Some(gd_pmap));
        assert_eq!(
            loaded.memory.read_u32_be(gdevice + 42),
            Some(u32::from(
                crate::display::classic_depth_mode(depth as u16).unwrap()
            ))
        );
        assert_eq!(loaded.memory.read_u16_be(gdevice), Some(0x1234));
        assert_eq!(loaded.memory.read_u16_be(gdevice + 2), Some(0x2345));
        assert_eq!(loaded.memory.read_u32_be(gdevice + 6), Some(0x3456_789a));
        assert_eq!(loaded.memory.read_u16_be(gdevice + 10), Some(5));
        assert_eq!(loaded.memory.read_u32_be(gdevice + 12), Some(0x4567_89ab));
        assert_eq!(loaded.memory.read_u32_be(gdevice + 16), Some(0x5678_9abc));
        assert_eq!(loaded.memory.read_u32_be(gdevice + 26), Some(0x6789_abcd));
        assert_eq!(loaded.memory.read_u32_be(gdevice + 30), Some(0x789a_bcde));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, gdevice + 34, 8),
            Some(gd_rect.clone())
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, gdevice + 46, 16),
            Some(cursor_fields.clone())
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE),
            Some(ctable_handle_before)
        );
        if depth >= 8 {
            assert_eq!(
                ppc_memory_read_bytes(
                    &mut loaded.memory,
                    PPC_MAIN_CTABLE,
                    PPC_MAIN_CTABLE_SIZE,
                ),
                Some(ctable_before.clone())
            );
        } else {
            assert_eq!(
                loaded.memory.read_u16_be(PPC_MAIN_CTABLE + 6),
                Some((1u16 << depth) - 1)
            );
        }
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, ctable_before_canary, 32),
            Some(vec![0xb1; 32])
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, ctable_after_canary, 32),
            Some(vec![0xb2; 32])
        );
    }
}

#[test]
fn hle_import_runner_set_depth_synchronizes_only_screen_backed_color_ports() {
    let pef = synthetic_pef_with_import(b"InitGraf");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x7000;
    let global_ptr = scratch + 126;
    let window_bounds = scratch + 0x100;
    let cport = scratch + 0x200;
    let gworld_out = scratch + 0x300;
    let gworld_bounds = scratch + 0x310;
    let paint_rect = scratch + 0x320;
    loaded.memory.add_region(scratch, vec![0; 0x500]);

    loaded.cpu.gpr[3] = global_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InitGraf);
    assert_eq!(loaded.toolbox_startup.init_graf_global_ptr, global_ptr);

    ppc_write_rect(&mut loaded.memory, window_bounds, 40, 30, 140, 230).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = window_bounds;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0x1020_3040;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let window = loaded.cpu.gpr[3];
    assert_ne!(window, 0);

    loaded.cpu.gpr[3] = cport;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OpenCPort);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    ppc_write_rect(&mut loaded.memory, gworld_bounds, 0, 0, 24, 32).unwrap();
    loaded.cpu.gpr[3] = gworld_out;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = gworld_bounds;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let offscreen = loaded.memory.read_u32_be(gworld_out).unwrap();
    assert_ne!(offscreen, 0);

    let screen_ports = [PPC_MAIN_GWORLD, window, cport];
    let screen_pixmaps = screen_ports.map(|port| {
        let record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == port)
            .copied()
            .unwrap();
        let live_handle = loaded.memory.read_u32_be(port + 2).unwrap();
        let live_pixmap = loaded.memory.read_u32_be(live_handle).unwrap();
        loaded
            .memory
            .write_u16_be(live_pixmap + 14, 0x1234)
            .unwrap();
        loaded
            .memory
            .write_u16_be(live_pixmap + 16, 0x2345)
            .unwrap();
        loaded
            .memory
            .write_u32_be(live_pixmap + 18, 0x3456_789a)
            .unwrap();
        loaded
            .memory
            .write_u32_be(live_pixmap + 22, 0x4567_89ab)
            .unwrap();
        loaded
            .memory
            .write_u32_be(live_pixmap + 26, 0x5678_9abc)
            .unwrap();
        loaded
            .memory
            .write_u32_be(live_pixmap + 38, 0x6789_abcd)
            .unwrap();
        loaded
            .memory
            .write_u32_be(live_pixmap + 46, 0x789a_bcde)
            .unwrap();
        let bounds = ppc_memory_read_bytes(&mut loaded.memory, live_pixmap + 6, 8).unwrap();
        let private_fields = (
            loaded.memory.read_u16_be(live_pixmap + 14),
            loaded.memory.read_u16_be(live_pixmap + 16),
            loaded.memory.read_u32_be(live_pixmap + 18),
            loaded.memory.read_u32_be(live_pixmap + 22),
            loaded.memory.read_u32_be(live_pixmap + 26),
            loaded.memory.read_u32_be(live_pixmap + 38),
            loaded.memory.read_u32_be(live_pixmap + 46),
        );
        (record, live_handle, live_pixmap, bounds, private_fields)
    });
    let offscreen_before = loaded
        .gworlds
        .iter()
        .find(|record| record.port == offscreen)
        .copied()
        .unwrap();
    loaded
        .memory
        .write_bytes(
            offscreen_before.base_addr,
            &vec![0x6d; (offscreen_before.row_bytes * offscreen_before.height) as usize],
        )
        .unwrap();
    let offscreen_pixmap_before =
        ppc_memory_read_bytes(&mut loaded.memory, offscreen_before.pixmap, PPC_PIXMAP_SIZE)
            .unwrap();
    let offscreen_ctable_handle = loaded
        .memory
        .read_u32_be(offscreen_before.pixmap + 42)
        .unwrap();
    let offscreen_ctable_before =
        ppc_copy_color_table_bytes(&mut loaded.memory, offscreen_ctable_handle).unwrap();
    let offscreen_pixels_before = ppc_memory_read_bytes(
        &mut loaded.memory,
        offscreen_before.base_addr,
        offscreen_before.row_bytes * offscreen_before.height,
    )
    .unwrap();
    let dsp_before = loaded
        .gworlds
        .iter()
        .find(|record| record.port == PPC_DSP_BACK_GWORLD)
        .copied()
        .unwrap();
    let dsp_pixmap_before =
        ppc_memory_read_bytes(&mut loaded.memory, dsp_before.pixmap, PPC_PIXMAP_SIZE).unwrap();

    for depth in [16, 8, 4, 2, 1] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let row_bytes = ppc_row_bytes(ppc_main_screen_width(), depth).unwrap();

        for (before, live_handle, live_pixmap, bounds, private_fields) in &screen_pixmaps {
            let record = loaded
                .gworlds
                .iter()
                .find(|record| record.port == before.port)
                .unwrap();
            assert_eq!(record.pixmap_handle, before.pixmap_handle);
            assert_eq!(record.pixmap, before.pixmap);
            assert_eq!(record.base_addr, PPC_MAIN_SCREEN_BASE);
            assert_eq!(record.width, before.width);
            assert_eq!(record.height, before.height);
            assert_eq!(record.depth, depth);
            assert_eq!(record.row_bytes, row_bytes);
            assert_eq!(
                loaded.memory.read_u32_be(record.port + 2),
                Some(*live_handle)
            );
            assert_eq!(loaded.memory.read_u32_be(*live_handle), Some(*live_pixmap));
            assert_eq!(
                loaded.memory.read_u32_be(*live_pixmap),
                Some(PPC_MAIN_SCREEN_BASE)
            );
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, *live_pixmap + 6, 8),
                Some(bounds.clone())
            );
            assert_eq!(
                (
                    loaded.memory.read_u16_be(*live_pixmap + 14),
                    loaded.memory.read_u16_be(*live_pixmap + 16),
                    loaded.memory.read_u32_be(*live_pixmap + 18),
                    loaded.memory.read_u32_be(*live_pixmap + 22),
                    loaded.memory.read_u32_be(*live_pixmap + 26),
                    loaded.memory.read_u32_be(*live_pixmap + 38),
                    loaded.memory.read_u32_be(*live_pixmap + 46),
                ),
                *private_fields
            );
            assert_eq!(
                (
                    loaded.memory.read_u16_be(*live_pixmap + 4),
                    loaded.memory.read_u16_be(*live_pixmap + 30),
                    loaded.memory.read_u16_be(*live_pixmap + 32),
                    loaded.memory.read_u16_be(*live_pixmap + 34),
                    loaded.memory.read_u16_be(*live_pixmap + 36),
                    loaded.memory.read_u32_be(*live_pixmap + 42),
                ),
                if depth <= 8 {
                    (
                        Some(0x8000 | row_bytes as u16),
                        Some(0),
                        Some(depth as u16),
                        Some(1),
                        Some(depth as u16),
                        Some(PPC_MAIN_CTABLE_HANDLE),
                    )
                } else {
                    (
                        Some(0x8000 | row_bytes as u16),
                        Some(16),
                        Some(16),
                        Some(3),
                        Some(5),
                        Some(0),
                    )
                }
            );
        }

        assert_eq!(
            loaded
                .gworlds
                .iter()
                .find(|record| record.port == offscreen),
            Some(&offscreen_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, offscreen_before.pixmap, PPC_PIXMAP_SIZE,),
            Some(offscreen_pixmap_before.clone())
        );
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, offscreen_ctable_handle),
            Some(offscreen_ctable_before.clone())
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                offscreen_before.base_addr,
                offscreen_before.row_bytes * offscreen_before.height,
            ),
            Some(offscreen_pixels_before.clone())
        );
        assert_eq!(
            loaded
                .gworlds
                .iter()
                .find(|record| record.port == PPC_DSP_BACK_GWORLD),
            Some(&dsp_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, dsp_before.pixmap, PPC_PIXMAP_SIZE),
            Some(dsp_pixmap_before.clone())
        );

        let screen_bits = global_ptr - 122;
        assert_eq!(
            loaded.memory.read_u32_be(screen_bits),
            Some(PPC_MAIN_SCREEN_BASE)
        );
        assert_eq!(
            loaded.memory.read_u16_be(screen_bits + 4),
            Some(row_bytes as u16)
        );
        assert_eq!(
            ppc_read_rect(&mut loaded.memory, screen_bits + 6),
            Some((
                0,
                0,
                ppc_main_screen_height() as i16,
                ppc_main_screen_width() as i16,
            ))
        );
        assert_eq!(loaded.memory.read_u32_be(global_ptr), Some(PPC_MAIN_GWORLD));

        let red = PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0,
        };
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = window);
        loaded.quickdraw_fore_color = red;
        loaded.quickdraw_fore_indices.remove(&window);
        ppc_write_rect(&mut loaded.memory, paint_rect, 1, 1, 4, 4).unwrap();
        loaded.cpu.gpr[3] = paint_rect;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::PaintRect);
        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, window).unwrap();
        let expected =
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, red).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                surface.front_buffer,
                surface.local_point((1, 1)),
            ),
            Some(expected)
        );
    }
}

#[test]
fn hle_import_runner_set_depth_follows_live_set_port_pix_aliases() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x9000;
    let first_port = scratch;
    let second_port = scratch + 0xc0;
    let live_pixmap_handle = scratch + 0x180;
    let live_pixmap = scratch + 0x190;
    let first_owned_handle = scratch + 0x1d0;
    let first_owned_pixmap = scratch + 0x1e0;
    let second_owned_handle = scratch + 0x220;
    let second_owned_pixmap = scratch + 0x230;
    let private_ctable_handle = scratch + 0x270;
    let private_ctable = scratch + 0x300;
    let first_pixels = scratch + 0xc00;
    let second_pixels = scratch + 0xd00;
    loaded.memory.add_region(scratch, vec![0; 0xe00]);

    loaded
        .memory
        .write_u32_be(live_pixmap_handle, live_pixmap)
        .unwrap();
    ppc_write_pixmap(
        &mut loaded.memory,
        live_pixmap,
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
        .write_u32_be(live_pixmap + 42, private_ctable_handle)
        .unwrap();
    loaded
        .memory
        .write_u32_be(private_ctable_handle, private_ctable)
        .unwrap();
    let main_ctable =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
            .unwrap();
    loaded
        .memory
        .write_bytes(private_ctable, &main_ctable)
        .unwrap();
    for index in 0..256u32 {
        let entry = private_ctable + 8 + index * 8;
        for offset in [2, 4, 6] {
            loaded.memory.write_u16_be(entry + offset, 0x8000).unwrap();
        }
    }
    let private_red_index = 47u32;
    let red_entry = private_ctable + 8 + private_red_index * 8;
    loaded.memory.write_u16_be(red_entry + 2, 0xffff).unwrap();
    loaded.memory.write_u16_be(red_entry + 4, 0).unwrap();
    loaded.memory.write_u16_be(red_entry + 6, 0).unwrap();

    for (port, owned_handle, owned_pixmap, pixels) in [
        (
            first_port,
            first_owned_handle,
            first_owned_pixmap,
            first_pixels,
        ),
        (
            second_port,
            second_owned_handle,
            second_owned_pixmap,
            second_pixels,
        ),
    ] {
        loaded
            .memory
            .write_u32_be(owned_handle, owned_pixmap)
            .unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            owned_pixmap,
            pixels,
            16,
            0,
            0,
            16,
            16,
            8,
        )
        .unwrap();
        loaded
            .memory
            .write_u32_be(owned_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
            .unwrap();
        ppc_write_gworld_port(&mut loaded.memory, port, owned_handle, 0, 0, 16, 16).unwrap();
        ppc_set_port_bits(&mut loaded.memory, port, live_pixmap_handle, true);
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port,
            pixmap_handle: owned_handle,
            pixmap: owned_pixmap,
            base_addr: pixels,
            gdevice: 0x0bad_f00d,
            width: 16,
            height: 16,
            depth: 8,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        });
    }

    let private_handle_before = loaded.memory.read_u32_be(private_ctable_handle).unwrap();
    let private_table_before =
        ppc_memory_read_bytes(&mut loaded.memory, private_ctable, PPC_MAIN_CTABLE_SIZE)
            .unwrap();
    let live_bounds = ppc_memory_read_bytes(&mut loaded.memory, live_pixmap + 6, 8).unwrap();
    let owned_before = [
        (first_owned_pixmap, first_pixels),
        (second_owned_pixmap, second_pixels),
    ]
    .map(|(pixmap, pixels)| {
        (
            pixmap,
            ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE).unwrap(),
            pixels,
            ppc_memory_read_bytes(&mut loaded.memory, pixels, 256).unwrap(),
        )
    });
    let alias_records_before = [first_port, second_port].map(|port| {
        loaded
            .gworlds
            .iter()
            .find(|record| record.port == port)
            .copied()
            .unwrap()
    });

    for depth in [16, 8, 4, 2, 1] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let row_bytes = ppc_row_bytes(ppc_main_screen_width(), depth).unwrap();

        for (port, before) in [first_port, second_port]
            .into_iter()
            .zip(alias_records_before)
        {
            let record = loaded
                .gworlds
                .iter()
                .find(|record| record.port == port)
                .unwrap();
            assert_eq!(record, &before);
            assert_eq!(
                loaded.memory.read_u32_be(port + 2),
                Some(live_pixmap_handle)
            );
        }
        assert_eq!(
            (
                loaded.memory.read_u16_be(live_pixmap + 4),
                loaded.memory.read_u16_be(live_pixmap + 30),
                loaded.memory.read_u16_be(live_pixmap + 32),
                loaded.memory.read_u16_be(live_pixmap + 34),
                loaded.memory.read_u16_be(live_pixmap + 36),
                loaded.memory.read_u32_be(live_pixmap + 42),
            ),
            if depth <= 8 {
                (
                    Some(0x8000 | row_bytes as u16),
                    Some(0),
                    Some(depth as u16),
                    Some(1),
                    Some(depth as u16),
                    Some(private_ctable_handle),
                )
            } else {
                (
                    Some(0x8000 | row_bytes as u16),
                    Some(16),
                    Some(16),
                    Some(3),
                    Some(5),
                    Some(0),
                )
            }
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, live_pixmap + 6, 8),
            Some(live_bounds.clone())
        );
        assert_eq!(
            loaded.memory.read_u32_be(private_ctable_handle),
            Some(private_handle_before)
        );
        if depth >= 8 {
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, private_ctable, PPC_MAIN_CTABLE_SIZE,),
                Some(private_table_before.clone())
            );
        } else {
            assert_eq!(
                loaded.memory.read_u16_be(private_ctable + 6),
                Some((1u16 << depth) - 1)
            );
        }
        for (pixmap, pixmap_bytes, pixels, pixel_bytes) in &owned_before {
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, *pixmap, PPC_PIXMAP_SIZE),
                Some(pixmap_bytes.clone())
            );
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, *pixels, 256),
                Some(pixel_bytes.clone())
            );
        }
    }

    loaded
        .memory
        .write_bytes(private_ctable, &private_table_before)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded
        .memory
        .write_bytes(private_ctable, &private_table_before)
        .unwrap();
    assert_eq!(
        loaded
            .toolbox_startup
            .indexed_screen_ctables
            .get(&live_pixmap_handle),
        Some(&private_ctable_handle)
    );
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, first_port).unwrap();
    assert_eq!(
        ppc_quickdraw_surface_color_pixel(
            &mut loaded.memory,
            surface,
            PpcRgbColor {
                red: 0xffff,
                green: 0,
                blue: 0,
            },
        ),
        Some(private_red_index as u16)
    );

    for (port, owned_handle) in [
        (first_port, first_owned_handle),
        (second_port, second_owned_handle),
    ] {
        ppc_set_port_bits(&mut loaded.memory, port, owned_handle, true);
    }
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 16;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    for ((port, owned_handle), before) in [
        (first_port, first_owned_handle),
        (second_port, second_owned_handle),
    ]
    .into_iter()
    .zip(alias_records_before)
    {
        assert_eq!(loaded.memory.read_u32_be(port + 2), Some(owned_handle));
        assert_eq!(
            loaded.gworlds.iter().find(|record| record.port == port),
            Some(&before)
        );
    }
    for (pixmap, pixmap_bytes, pixels, pixel_bytes) in &owned_before {
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, *pixmap, PPC_PIXMAP_SIZE),
            Some(pixmap_bytes.clone())
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, *pixels, 256),
            Some(pixel_bytes.clone())
        );
    }
}

#[test]
fn hle_import_runner_depth_ctable_cache_drops_closed_color_ports() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0xa000;
    let bounds = scratch;
    let cport = scratch + 0x100;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    ppc_write_rect(&mut loaded.memory, bounds, 40, 30, 140, 230).unwrap();

    let mut windows = Vec::new();
    for _ in 0..2 {
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = bounds;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 0;
        loaded.cpu.gpr[10] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
        assert_ne!(loaded.cpu.gpr[3], 0);
        windows.push(loaded.cpu.gpr[3]);
    }
    loaded.cpu.gpr[3] = cport;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OpenCPort);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let owned_pixmap_handles = [windows[0], windows[1], cport].map(|port| {
        loaded
            .gworlds
            .iter()
            .find(|record| record.port == port)
            .map(|record| record.pixmap_handle)
            .unwrap()
    });
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 16;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
    for pixmap_handle in owned_pixmap_handles {
        assert_eq!(
            loaded
                .toolbox_startup
                .indexed_screen_ctables
                .get(&pixmap_handle),
            Some(&PPC_MAIN_CTABLE_HANDLE)
        );
    }

    loaded.cpu.gpr[3] = windows[0];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseWindow);
    loaded.cpu.gpr[3] = cport;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseCPort);
    loaded.cpu.gpr[3] = windows[1];
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow),
    );

    for pixmap_handle in owned_pixmap_handles {
        assert!(
            !loaded
                .toolbox_startup
                .indexed_screen_ctables
                .contains_key(&pixmap_handle),
            "teardown retained PixMap Handle {pixmap_handle:#010x}"
        );
    }
    assert_eq!(
        loaded
            .toolbox_startup
            .indexed_screen_ctables
            .get(&PPC_MAIN_PIXMAP_HANDLE),
        Some(&PPC_MAIN_CTABLE_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE),
        Some(PPC_MAIN_CTABLE)
    );
}

#[test]
fn hle_import_runner_draw_menu_bar_matches_at_supported_depths() {
    let pef = synthetic_pef_with_import(b"DrawMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let title = PPC_DATA_BASE + 0x7f00;
    let item = title + 8;
    let set_entries_color = title + 24;
    let paint_rect = title + 40;
    loaded.memory.add_region(title, vec![0; 64]);
    assert!(ppc_write_pstring_bytes(&mut loaded.memory, title, b"File"));
    assert!(ppc_write_pstring_bytes(&mut loaded.memory, item, b"Open"));
    let menu = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        128,
        title,
    );
    assert_ne!(menu, 0);
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            menu,
            item,
            i16::MAX,
        ),
        PPC_NO_ERR
    );
    let menu_list = ppc_alloc_menu_list_handle(
        &[menu],
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
    );
    assert_ne!(menu_list, 0);
    ppc_set_current_menu_list(&mut loaded.memory, menu_list);

    let logical_black = 37usize;
    let logical_white = 73usize;
    for index in 0..256u32 {
        let entry = PPC_MAIN_CTABLE + 8 + index * 8;
        loaded.memory.write_u16_be(entry, index as u16).unwrap();
        for component in [2, 4, 6] {
            loaded
                .memory
                .write_u16_be(entry + component, 0x8000)
                .unwrap();
        }
    }
    for (index, component) in [(logical_black, 0), (logical_white, 0xffff)] {
        let entry = PPC_MAIN_CTABLE + 8 + index as u32 * 8;
        for offset in [2, 4, 6] {
            loaded
                .memory
                .write_u16_be(entry + offset, component)
                .unwrap();
        }
    }
    let ctable_before =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
            .unwrap();

    let mut masks = Vec::new();
    for depth in [16, 8] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = 1;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        if depth == 16 {
            loaded
                .memory
                .write_u16_be(set_entries_color, logical_black as u16)
                .unwrap();
            loaded
                .memory
                .write_u16_be(set_entries_color + 2, 0xffff)
                .unwrap();
            loaded
                .memory
                .write_u16_be(set_entries_color + 4, 0)
                .unwrap();
            loaded
                .memory
                .write_u16_be(set_entries_color + 6, 0)
                .unwrap();
            loaded.cpu.gpr[3] = logical_black as u32;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = set_entries_color;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::SetEntries);
            assert_eq!(loaded.screen_clut[logical_black], [0xffff, 0, 0]);
            assert_eq!(loaded.color_manager_clut[logical_black], [0xffff, 0, 0]);
            assert_eq!(
                ppc_memory_read_bytes(
                    &mut loaded.memory,
                    PPC_MAIN_CTABLE,
                    PPC_MAIN_CTABLE_SIZE,
                ),
                Some(ctable_before.clone())
            );
        } else {
            let logical_clut = ppc_read_ctable_clut(
                &mut loaded.memory,
                PPC_MAIN_CTABLE_HANDLE,
                &TrapDispatcher::standard_mac_8bpp_clut(),
            )
            .unwrap();
            assert_eq!(loaded.screen_clut, logical_clut);
            assert_eq!(loaded.color_manager_clut, logical_clut);
            assert_eq!(logical_clut[logical_black], [0; 3]);
            assert_eq!(logical_clut[logical_white], [0xffff; 3]);

            // A hardware-only CLUT change must affect Menu Manager pixel
            // selection without replacing QuickDraw's logical CTable.
            loaded.screen_clut.set_entry(logical_black, [0x8000; 3]);
            loaded.screen_clut.set_entry(logical_white, [0x8000; 3]);
            loaded.screen_clut.set_entry(91, [0; 3]);
            loaded.screen_clut.set_entry(92, [0xffff; 3]);
        }
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);

        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap();
        assert_eq!(surface.front_buffer.depth, depth);
        let (white, black) = if depth == 8 {
            (
                u16::from(ppc_rgb_color_to_8bpp_index_in_clut(
                    PPC_RGB_WHITE,
                    &loaded.screen_clut,
                )),
                u16::from(ppc_rgb_color_to_8bpp_index_in_clut(
                    PPC_RGB_BLACK,
                    &loaded.screen_clut,
                )),
            )
        } else {
            (
                ppc_rgb_color_to_rgb555(PPC_RGB_WHITE),
                ppc_rgb_color_to_rgb555(PPC_RGB_BLACK),
            )
        };
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (0, 0),),
            Some(black),
            "{depth}bpp DrawMenuBar did not preserve the rounded corner mask",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (5, 0),),
            Some(white),
            "{depth}bpp rounded corner mask extended into the menu-bar interior",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (0, 19),),
            Some(black)
        );
        let mut mask = Vec::with_capacity((ppc_main_screen_width() * 20) as usize);
        for y in 0..20 {
            for x in 0..ppc_main_screen_width() as i32 {
                mask.push(
                    ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (x, y))
                        == Some(black),
                );
            }
        }
        assert!(mask.iter().filter(|pixel| **pixel).count() > ppc_main_screen_width() as usize);
        masks.push(mask);

        if depth == 8 {
            assert_eq!(black, 91);
            assert_eq!(white, 92);
            loaded
                .current_gworld
                .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
            loaded.quickdraw_fore_color = PPC_RGB_BLACK;
            loaded.quickdraw_fore_indices.remove(&PPC_MAIN_GWORLD);
            ppc_write_rect(&mut loaded.memory, paint_rect, 30, 30, 32, 32).unwrap();
            loaded.cpu.gpr[3] = paint_rect;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::PaintRect);
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    surface.front_buffer,
                    surface.local_point((30, 30)),
                ),
                Some(logical_black as u16)
            );

            let popup_probe = (12, 20);
            let popup_before =
                ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, popup_probe)
                    .unwrap();
            ppc_track_menu_while_held(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                12,
                PpcInputSnapshot {
                    mouse_button: true,
                    mouse_v: 10,
                    mouse_h: 12,
                    ..PpcInputSnapshot::default()
                },
            );
            let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
            assert_eq!((tracking.popup_left, tracking.popup_top), (11, 20));
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    surface.front_buffer,
                    (
                        i32::from(tracking.popup_left),
                        i32::from(tracking.popup_top)
                    ),
                ),
                Some(black)
            );
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    surface.front_buffer,
                    (
                        i32::from(tracking.popup_left) + 2,
                        i32::from(tracking.popup_top) + 2,
                    ),
                ),
                Some(white)
            );
            let popup_left = tracking.popup_left;
            let popup_top = tracking.popup_top;
            let popup_width = tracking.popup_width;
            let row_top = popup_top;
            let unhighlighted_black_glyphs = (row_top..row_top + 16)
                .flat_map(|y| {
                    (popup_left + 18..popup_left + popup_width - 1).map(move |x| (x, y))
                })
                .filter(|&(x, y)| {
                    ppc_quickdraw_read_pixel(
                        &mut loaded.memory,
                        surface.front_buffer,
                        (i32::from(x), i32::from(y)),
                    ) == Some(black)
                })
                .count();
            assert!(unhighlighted_black_glyphs > 0);

            let selected_input = PpcInputSnapshot {
                mouse_button: true,
                mouse_v: row_top + 8,
                mouse_h: popup_left + 20,
                ..PpcInputSnapshot::default()
            };
            ppc_track_menu_while_held(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                12,
                selected_input,
            );
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    surface.front_buffer,
                    (i32::from(popup_left) + 2, i32::from(popup_top) + 2),
                ),
                Some(black)
            );
            let highlighted_white_glyphs = (row_top..row_top + 16)
                .flat_map(|y| {
                    (popup_left + 18..popup_left + popup_width - 1).map(move |x| (x, y))
                })
                .filter(|&(x, y)| {
                    ppc_quickdraw_read_pixel(
                        &mut loaded.memory,
                        surface.front_buffer,
                        (i32::from(x), i32::from(y)),
                    ) == Some(white)
                })
                .count();
            assert!(highlighted_white_glyphs > 0);
            assert_eq!(
                ppc_finish_menu_bar_tracking(
                    &mut loaded.memory,
                    &loaded.gworlds,
                    &loaded.screen_clut,
                    &mut loaded.toolbox_startup,
                    PpcInputSnapshot {
                        mouse_v: selected_input.mouse_v,
                        mouse_h: selected_input.mouse_h,
                        ..PpcInputSnapshot::default()
                    },
                ),
                Some((128u32 << 16) | 1)
            );
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, popup_probe,),
                Some(popup_before)
            );
        }
    }
    assert_eq!(masks[0], masks[1]);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE),
        Some(ctable_before)
    );
}

#[test]
fn hle_import_runner_handles_set_depth() {
    let pef = synthetic_pef_with_import(b"SetDepth");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.gworlds[0].depth, 8);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32), Some(8));

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 16;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.gworlds[0].depth, 16);
    assert_eq!(loaded.gworlds[0].row_bytes, ppc_main_screen_width() * 2 + 16);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 30), Some(16));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32), Some(16));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 34), Some(3));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 36), Some(5));
    assert_eq!(loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[4] = 8;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.gworlds[0].depth, 8);
    assert_eq!(loaded.gworlds[0].row_bytes, ppc_main_screen_width() + 16);
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 30), Some(0));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32), Some(8));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 34), Some(1));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 36), Some(8));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42),
        Some(PPC_MAIN_CTABLE_HANDLE)
    );
}
