use super::*;

#[test]
fn load_pef_application_honors_configured_stack_size() {
    let pef = synthetic_pef();
    let loaded = load_pef_application_with_config(
        &pef,
        PpcLoadConfig::from_cfrg_app_stack_size(0x0003_2000),
    )
    .unwrap();

    assert_eq!(loaded.stack_size, 0x0003_2000);
    assert_eq!(loaded.stack_base, PPC_STACK_TOP - 0x0003_2000);
    assert_eq!(loaded.stack_pointer, PPC_STACK_TOP - 64);
    assert_eq!(loaded.cpu.gpr[1], loaded.stack_pointer);
}

#[test]
fn load_pef_application_maps_classic_application_memory_band() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();
    let legacy_pixmap_addr = 0x0003_2a00;

    assert!(loaded
        .memory
        .write_u32_be(legacy_pixmap_addr, 0x1234_5678)
        .is_some());
    assert_eq!(
        loaded.memory.read_u32_be(legacy_pixmap_addr),
        Some(0x1234_5678)
    );
}

#[test]
fn load_pef_application_rounds_small_stack_size_to_valid_frame() {
    let pef = synthetic_pef();
    let loaded = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            stack_size: PPC_INITIAL_STACK_FRAME_SIZE - 1,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap();

    assert_eq!(loaded.stack_size, PPC_INITIAL_STACK_FRAME_SIZE);
    assert_eq!(
        loaded.stack_base,
        PPC_STACK_TOP - PPC_INITIAL_STACK_FRAME_SIZE
    );
}

#[test]
fn load_pef_application_rejects_stack_size_that_overlaps_loader_space() {
    let pef = synthetic_pef();
    let error = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            stack_size: PPC_MAX_STACK_SIZE + 16,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        PpcLoadError::StackSizeOutOfRange {
            requested: PPC_MAX_STACK_SIZE + 16
        }
    );
}

#[test]
fn load_pef_application_configures_every_supported_main_display_depth() {
    let pef = synthetic_pef();
    for depth in [1, 2, 4, 8, 16] {
        let mut loaded = load_pef_application_with_config(
            &pef,
            PpcLoadConfig {
                screen_depth: depth,
                ..PpcLoadConfig::default()
            },
        )
        .unwrap();
        let row_bytes = ppc_row_bytes(ppc_main_screen_width(), depth).unwrap();

        assert_eq!(loaded.gworlds[0].depth, depth);
        assert_eq!(loaded.gworlds[0].row_bytes, row_bytes);
        assert_eq!(loaded.current_front_buffer().unwrap().depth, depth);
        assert_eq!(loaded.current_front_buffer().unwrap().row_bytes, row_bytes);
        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 4),
            Some(0x8000 | row_bytes as u16)
        );
        assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32),
            Some(depth as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42),
            Some(if depth <= 8 {
                PPC_MAIN_CTABLE_HANDLE
            } else {
                0
            })
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
        assert_eq!(
            loaded
                .memory
                .read_u16_be(PPC_MAIN_GDEVICE_RECORD + 20)
                .unwrap()
                & 1
                != 0,
            depth != 1
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.cpu.gpr[4], 0);
    }
}

#[test]
fn load_pef_application_rejects_unsupported_screen_depth() {
    let pef = synthetic_pef();
    let error = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            screen_depth: 32,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap_err();

    assert_eq!(error, PpcLoadError::ScreenDepthOutOfRange { requested: 32 });
}

#[test]
fn display_manager_get_follows_live_depth_and_mode_list_advertises_supported_depths() {
    for depth in [1, 2, 4, 8, 16] {
        let get_pef = synthetic_pef_with_import(b"DMGetDisplayMode");
        let mut loaded = load_pef_application_with_config(
            &get_pef,
            PpcLoadConfig {
                screen_depth: depth,
                ..PpcLoadConfig::default()
            },
        )
        .unwrap();
        let switch_info = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(switch_info, vec![0xa5; 16]);
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = switch_info;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "depth {depth}");
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let depth_mode = loaded
            .memory
            .read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42)
            .unwrap();
        assert_eq!(
            loaded.memory.read_u16_be(switch_info),
            Some(depth_mode as u16),
            "depth {depth}"
        );
        assert_eq!(
            loaded.memory.read_u32_be(switch_info + 2),
            Some(PPC_DM_CURRENT_DISPLAY_MODE_ID)
        );
        assert_eq!(
            loaded.memory.read_u32_be(switch_info + 8),
            loaded.memory.read_u32_be(PPC_MAIN_PIXMAP)
        );

        let list_pef = synthetic_pef_with_import(b"DMNewDisplayModeList");
        let mut loaded = load_pef_application_with_config(
            &list_pef,
            PpcLoadConfig {
                screen_depth: depth,
                ..PpcLoadConfig::default()
            },
        )
        .unwrap();
        let outputs = PPC_DATA_BASE + 0x1200;
        loaded.memory.add_region(outputs, vec![0; 8]);
        loaded.cpu.gpr[3] = PPC_DSP_DISPLAY_ID;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = outputs;
        loaded.cpu.gpr[7] = outputs + 4;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "depth {depth}");
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(outputs), Some(1));
        let list = loaded.memory.read_u32_be(outputs + 4).unwrap();
        assert_eq!(
            loaded.memory.read_u32_be(list),
            Some(PPC_DM_MODE_LIST_MAGIC)
        );
        let mode = ppc_dm_live_display_mode(&mut loaded.memory, PPC_MAIN_GDEVICE).unwrap();
        let resolution = list + PPC_DM_MODE_LIST_RESOLUTION_INFO_OFFSET;
        let timing = list + PPC_DM_MODE_LIST_TIMING_INFO_OFFSET;
        let depth_block = list + PPC_DM_MODE_LIST_DEPTH_BLOCK_OFFSET;
        let live_depth_index = [1u16, 2, 4, 8, 16]
            .iter()
            .position(|candidate| *candidate == depth as u16)
            .unwrap() as u32;
        assert_eq!(
            loaded.memory.read_u32_be(list + PPC_DM_MODE_LIST_ENTRY_OFFSET + 4),
            Some(list + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + live_depth_index * 16)
        );
        assert_eq!(loaded.memory.read_u32_be(resolution + 8), Some(mode.width));
        assert_eq!(
            loaded.memory.read_u32_be(resolution + 12),
            Some(mode.height)
        );
        assert_eq!(
            loaded.memory.read_u32_be(resolution + 20),
            Some(mode.depth_mode)
        );
        assert_eq!(
            loaded.memory.read_u32_be(timing),
            Some(mode.display_mode_id)
        );
        assert_eq!(loaded.memory.read_u32_be(depth_block), Some(5));
        for (index, listed_depth) in [1u16, 2, 4, 8, 16].into_iter().enumerate() {
            let offset = index as u32;
            let listed_switch = list + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + offset * 16;
            let depth_info = list + PPC_DM_MODE_LIST_DEPTH_INFO_OFFSET + offset * 20;
            let vp_block = list + PPC_DM_MODE_LIST_VP_BLOCK_OFFSET + offset * 42;
            assert_eq!(loaded.memory.read_u32_be(depth_info), Some(listed_switch));
            assert_eq!(loaded.memory.read_u32_be(depth_info + 4), Some(vp_block));
            assert_eq!(
                loaded.memory.read_u16_be(listed_switch),
                Some(crate::display::classic_depth_mode(listed_depth).unwrap())
            );
            assert_eq!(
                loaded.memory.read_u16_be(vp_block + 32),
                Some(listed_depth)
            );
            assert_eq!(
                ppc_read_rect(&mut loaded.memory, vp_block + 6),
                Some(mode.bounds)
            );
        }
    }
}

#[test]
fn display_manager_check_accepts_only_the_live_timing_and_depth_mode() {
    let pef = synthetic_pef_with_import(b"DMCheckDisplayMode");
    let mut loaded = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            screen_depth: 4,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap();
    let outputs = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(outputs, vec![0xa5; 8]);
    let depth_mode = loaded
        .memory
        .read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode;
    loaded.cpu.gpr[6] = outputs;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = outputs + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(outputs),
        Some(PPC_DM_NO_SWITCH_CONFIRM_MASK)
    );
    assert_eq!(loaded.memory.read_u8(outputs + 4), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID + 1;
    loaded.cpu.gpr[5] = depth_mode;
    loaded.cpu.gpr[6] = outputs;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = outputs + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(outputs),
        Some(PPC_DM_DEPTH_NOT_AVAILABLE_MASK)
    );
    assert_eq!(loaded.memory.read_u8(outputs + 4), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.memory.write_u32_be(outputs, 0xa5a5_5a5a).unwrap();
    loaded.memory.write_u8(outputs + 4, 0xc3).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode;
    loaded.cpu.gpr[6] = outputs;
    loaded.cpu.gpr[7] = 1;
    loaded.cpu.gpr[8] = outputs + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(outputs), Some(0xa5a5_5a5a));
    assert_eq!(loaded.memory.read_u8(outputs + 4), Some(0xc3));
}

#[test]
fn display_manager_set_rejects_unknown_modes_and_accepts_opaque_display_state() {
    let pef = synthetic_pef_with_import(b"DMSetDisplayMode");
    let mut loaded = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            screen_depth: 2,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap();
    let depth_mode_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(depth_mode_ptr, vec![0; 4]);
    let depth_mode = loaded
        .memory
        .read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42)
        .unwrap();
    loaded
        .memory
        .write_u32_be(depth_mode_ptr, depth_mode)
        .unwrap();
    let gdevice_before = ppc_memory_read_bytes(
        &mut loaded.memory,
        PPC_MAIN_GDEVICE_RECORD,
        PPC_GDEVICE_SIZE,
    )
    .unwrap();
    let pixmap_before =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap();
    let screen_row_bytes =
        u32::from(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 4).unwrap() & 0x3fff);
    let screen_before =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, screen_row_bytes)
            .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(depth_mode_ptr), Some(depth_mode));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID + 1;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DM_MODE_NOT_FOUND_ERR));
    assert_eq!(loaded.memory.read_u32_be(depth_mode_ptr), Some(depth_mode));
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        )
        .unwrap(),
        gdevice_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap(),
        pixmap_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, screen_row_bytes,)
            .unwrap(),
        screen_before
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded
        .memory
        .write_u32_be(depth_mode_ptr, 0xdead)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DM_MODE_NOT_FOUND_ERR));
    assert_eq!(loaded.memory.read_u32_be(depth_mode_ptr), Some(0xdead));
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        )
        .unwrap(),
        gdevice_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap(),
        pixmap_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, screen_row_bytes,)
            .unwrap(),
        screen_before
    );

    loaded
        .memory
        .write_u32_be(depth_mode_ptr, depth_mode)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = PPC_DATA_BASE + 0x2000;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(depth_mode_ptr), Some(depth_mode));
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_MAIN_GDEVICE_RECORD,
            PPC_GDEVICE_SIZE,
        )
        .unwrap(),
        gdevice_before,
        "the opaque display-state parameter must not be dereferenced"
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_PIXMAP, PPC_PIXMAP_SIZE).unwrap(),
        pixmap_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, screen_row_bytes,)
            .unwrap(),
        screen_before
    );
}

#[test]
fn display_manager_set_switches_to_an_advertised_depth() {
    let pef = synthetic_pef_with_import(b"DMSetDisplayMode");
    let mut loaded = load_pef_application_with_config(
        &pef,
        PpcLoadConfig {
            screen_depth: 16,
            ..PpcLoadConfig::default()
        },
    )
    .unwrap();
    let depth_mode_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(depth_mode_ptr, vec![0; 4]);
    loaded
        .memory
        .write_u32_be(
            depth_mode_ptr,
            u32::from(crate::display::classic_depth_mode(8).unwrap()),
        )
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_CURRENT_DISPLAY_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = PPC_DATA_BASE + 0x2000;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32), Some(8));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
        Some(u32::from(crate::display::classic_depth_mode(8).unwrap()))
    );
}
