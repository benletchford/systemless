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
fn display_manager_get_follows_live_mode_and_list_advertises_geometries_and_depths() {
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
        assert_eq!(loaded.memory.read_u32_be(outputs), Some(3));
        let list = loaded.memory.read_u32_be(outputs + 4).unwrap();
        assert_eq!(
            loaded.memory.read_u32_be(list),
            Some(PPC_DM_MODE_LIST_MAGIC)
        );
        let live_mode = ppc_dm_live_display_mode(&mut loaded.memory, PPC_MAIN_GDEVICE).unwrap();
        let live_depth_index = [1u16, 2, 4, 8, 16]
            .iter()
            .position(|candidate| *candidate == depth as u16)
            .unwrap() as u32;
        for (geometry_index, (mode_id, width, height)) in [
            (PPC_DM_512_342_MODE_ID, 512, 342),
            (PPC_DM_640_480_MODE_ID, 640, 480),
            (
                PPC_DM_NATIVE_MODE_ID,
                ppc_main_screen_width(),
                ppc_main_screen_height(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let entry_base = list + geometry_index as u32 * PPC_DM_MODE_LIST_ENTRY_STRIDE;
            let resolution = entry_base + PPC_DM_MODE_LIST_RESOLUTION_INFO_OFFSET;
            let timing = entry_base + PPC_DM_MODE_LIST_TIMING_INFO_OFFSET;
            let depth_block = entry_base + PPC_DM_MODE_LIST_DEPTH_BLOCK_OFFSET;
            assert_eq!(
                loaded
                    .memory
                    .read_u32_be(entry_base + PPC_DM_MODE_LIST_ENTRY_OFFSET + 4),
                Some(
                    entry_base + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + live_depth_index * 16
                )
            );
            assert_eq!(loaded.memory.read_u32_be(resolution + 8), Some(width));
            assert_eq!(loaded.memory.read_u32_be(resolution + 12), Some(height));
            assert_eq!(loaded.memory.read_u32_be(resolution + 20), Some(live_mode.depth_mode));
            assert_eq!(loaded.memory.read_u32_be(timing), Some(mode_id));
            assert_eq!(loaded.memory.read_u32_be(depth_block), Some(5));
            for (index, listed_depth) in [1u16, 2, 4, 8, 16].into_iter().enumerate() {
                let offset = index as u32;
                let listed_switch =
                    entry_base + PPC_DM_MODE_LIST_SWITCH_INFO_OFFSET + offset * 16;
                let depth_info = entry_base + PPC_DM_MODE_LIST_DEPTH_INFO_OFFSET + offset * 20;
                let vp_block = entry_base + PPC_DM_MODE_LIST_VP_BLOCK_OFFSET + offset * 42;
                assert_eq!(loaded.memory.read_u32_be(depth_info), Some(listed_switch));
                assert_eq!(loaded.memory.read_u32_be(depth_info + 4), Some(vp_block));
                assert_eq!(
                    loaded.memory.read_u16_be(listed_switch),
                    Some(crate::display::classic_depth_mode(listed_depth).unwrap())
                );
                assert_eq!(loaded.memory.read_u32_be(listed_switch + 2), Some(mode_id));
                assert_eq!(loaded.memory.read_u16_be(vp_block + 32), Some(listed_depth));
                assert_eq!(
                    ppc_read_rect(&mut loaded.memory, vp_block + 6),
                    Some((0, 0, height as i16, width as i16))
                );
            }
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
    loaded.cpu.gpr[4] = PPC_DM_640_480_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = PPC_DATA_BASE + 0x2000;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 32), Some(8));
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_PIXMAP + 4),
        Some(0x8000 | ppc_row_bytes(640, 8).unwrap() as u16)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6),
        Some((0, 0, 480, 640))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, PPC_MAIN_GDEVICE_RECORD + 34),
        Some((0, 0, 480, 640))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, PPC_MAIN_GWORLD + 16),
        Some((0, 0, 480, 640))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, PPC_MAIN_VIS_RGN + 2),
        Some((0, 0, 480, 640))
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
        Some(u32::from(crate::display::classic_depth_mode(8).unwrap()))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = PPC_DM_NATIVE_MODE_ID;
    loaded.cpu.gpr[5] = depth_mode_ptr;
    loaded.cpu.gpr[6] = PPC_DATA_BASE + 0x2000;
    loaded.cpu.gpr[7] = 0;

    let restore_probe = loaded.run_with_hle_imports(64);

    assert_eq!(restore_probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6),
        Some((
            0,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16,
        ))
    );
}

#[test]
fn pef_dump_json_includes_loader_sections_imports_and_entry() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let loaded = load_pef_application(&pef).unwrap();
    let header = parse_pef_header(&pef).unwrap();
    let loader = parse_pef_loader_header(&pef).unwrap();
    let raw_sections = parse_pef_sections(&pef).unwrap();
    let mapped_sections = map_instantiated_sections(&pef).unwrap();
    let reloc_headers = parse_pef_reloc_headers(&pef).unwrap_or_default();
    let report = format_pef_dump_json(&PefDumpContext {
        data_len: pef.len(),
        header,
        loader,
        raw_sections: &raw_sections,
        mapped_sections: &mapped_sections,
        imports: &loaded.imports,
        reloc_headers: &reloc_headers,
        entry_pc: loaded.entry_pc,
        rtoc: loaded.rtoc,
        stack_base: loaded.stack_base,
        stack_size: loaded.stack_size,
        stack_top: PPC_STACK_TOP,
    });

    assert!(report.contains("\"format\": \"systemless_pef_dump_v1\""));
    assert!(report.contains("\"architecture\": \"pwpc\""));
    assert!(report.contains("\"kind\": \"code\""));
    assert!(report.contains("\"mapped_base\": \"0x01000000\""));
    assert!(report.contains("\"library\": \"InterfaceLib\""));
    assert!(report.contains("\"symbol\": \"NewPtrClear\""));
    assert!(report.contains("\"dispatcher_target\": \"NewPtr { clear: true }\""));
    assert!(report.contains("\"entry_pc\": \"0x01000000\""));
    assert!(report.contains("\"rtoc\": \"0x02000000\""));
    assert!(report.contains("\"base\": \"0x04FF0000\""));
}

#[test]
fn load_pef_application_reports_detailed_relocation_apply_failure() {
    let pef = synthetic_pef_with_reloc_chunks(
        b"InterfaceLib",
        b"OnlyImport",
        &[delt(8), sm_index_reloc(0x30, 0)],
    );

    let error = load_pef_application(&pef).unwrap_err();

    assert_eq!(
        error,
        PpcLoadError::RelocationApply {
            section_index: 1,
            reloc_instr_offset: 2,
            section_position: 8,
            import_index: Some(0),
            import_symbol: Some(PpcRelocationImportSymbol {
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "OnlyImport".to_string(),
                class: 2,
                weak: false,
            }),
            error: PefRelocApplyError::OutOfRange {
                position: 8,
                section_len: 8,
            },
        }
    );
}

#[test]
fn load_pef_application_seeds_ppc_cpu_and_reaches_import_trace() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.entry_pc, PPC_CODE_BASE);
    assert_eq!(loaded.rtoc, PPC_DATA_BASE);
    assert_eq!(loaded.cpu.pc, PPC_CODE_BASE);
    assert_eq!(loaded.cpu.gpr[1], PPC_STACK_TOP - 64);
    assert_eq!(loaded.stack_base, PPC_STACK_BASE);
    assert_eq!(loaded.stack_size, PPC_DEFAULT_STACK_SIZE);
    assert_eq!(loaded.heap_base(), PPC_HEAP_BASE);
    assert_eq!(loaded.heap_cursor(), PPC_HEAP_BASE);
    assert_eq!(test_heap_limit!(loaded), PPC_STACK_BASE);
    assert_eq!(loaded.last_mem_error(), 0);
    assert_eq!(*loaded.process_file_system.current_resource_file, 0);
    assert_eq!(loaded.test_resource_error(), 0);
    assert_eq!(loaded.cfm.as_ref().unwrap().connections.len(), 0);
    assert_eq!(
        loaded.cfm.as_ref().unwrap().next_connection_id,
        PPC_FIRST_CFM_CONNECTION_ID
    );
    assert_eq!(test_handle_records!(loaded).len(), 0);
    assert_eq!(loaded.gworlds.len(), 2);
    assert_eq!(loaded.gworlds[0].port, PPC_MAIN_GWORLD);
    assert_eq!(loaded.gworlds[0].pixmap_handle, PPC_MAIN_PIXMAP_HANDLE);
    assert_eq!(loaded.gworlds[0].base_addr, PPC_MAIN_SCREEN_BASE);
    assert_eq!(
        loaded.memory.read_u16_be(PPC_MAIN_GDEVICE_RECORD + 4),
        Some(0),
        "the default indexed display is a clutType GDevice"
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GDEVICE_RECORD + 42),
        Some(0x83),
        "the default 8-bit display publishes the classic eightBitMode token"
    );
    assert_eq!(loaded.gworlds[1].port, PPC_DSP_BACK_GWORLD);
    assert_eq!(loaded.gworlds[1].pixmap_handle, PPC_DSP_BACK_PIXMAP_HANDLE);
    assert_eq!(loaded.gworlds[1].base_addr, PPC_DSP_BACK_SCREEN_BASE);
    assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
    assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
    assert_eq!(loaded.q3_objects.len(), 0);
    assert_eq!(loaded.next_q3_object, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_error_state, PpcQ3ErrorState::default());
    assert_eq!(loaded.q3_lifecycle, PpcQ3LifecycleState::default());
    assert_eq!(loaded.q3_memory_storages.len(), 0);
    assert_eq!(loaded.q3_files.len(), 0);
    assert_eq!(loaded.q3_group_memberships.len(), 0);
    assert_eq!(loaded.q3_file_groups.len(), 0);
    assert_eq!(loaded.q3_views.len(), 0);
    assert_eq!(loaded.q3_submissions.len(), 0);
    assert_eq!(loaded.q3_view_transforms.len(), 0);
    assert_eq!(loaded.q3_submission_transforms.len(), 0);
    assert_eq!(loaded.q3_view_materials.len(), 0);
    assert_eq!(loaded.q3_submission_materials.len(), 0);
    assert_eq!(loaded.q3_submission_lights.len(), 0);
    assert_eq!(loaded.q3_fog_styles.len(), 0);
    assert_eq!(loaded.q3_attributes.len(), 0);
    assert_eq!(loaded.q3_shader_uv_transforms.len(), 0);
    assert_eq!(loaded.q3_mipmap_textures.len(), 0);
    assert_eq!(loaded.q3_texture_shaders.len(), 0);
    assert_eq!(loaded.q3_trimeshes.len(), 0);
    assert_eq!(loaded.q3_styles.len(), 0);
    assert_eq!(loaded.q3_cameras.len(), 0);
    assert_eq!(loaded.q3_lights.len(), 0);
    assert_eq!(loaded.input_sprocket, PpcInputSprocketState::default());
    assert_eq!(loaded.quicktime, PpcQuickTimeState::default());
    assert_eq!(loaded.sound, PpcSoundState::default());
    assert_eq!(loaded.draw_sprocket, PpcDrawSprocketState::default());
    assert_eq!(loaded.toolbox_startup, PpcToolboxStartupState::default());
    assert_eq!(loaded.files.len(), 0);
    assert_eq!(loaded.vfs_files.len(), 0);
    assert_eq!(loaded.resource_files.len(), 0);
    assert_eq!(loaded.vfs_resource_files.len(), 0);
    assert_eq!(loaded.process_file_system.vfs_resources.len(), 0);
    assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM);
    assert_eq!(
        loaded.sound.manager.default_output_volume(),
        PPC_DEFAULT_OUTPUT_VOLUME
    );
    assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(loaded.default_dir_id, PPC_ROOT_DIR_ID);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_GWORLD + 2),
        Some(PPC_MAIN_PIXMAP_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP_HANDLE),
        Some(PPC_MAIN_PIXMAP)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP),
        Some(PPC_MAIN_SCREEN_BASE)
    );
    assert_eq!(loaded.cpu.gpr[2], PPC_DATA_BASE);
    assert_eq!(loaded.memory.read_u32_be(PPC_CFM_MAIN_STUB_BASE), Some(BLR));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_PORT_LIST_ADDR),
        Some(PPC_PORT_LIST_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_PORT_LIST_HANDLE),
        Some(PPC_PORT_LIST)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_PORT_LIST), Some(0));
    assert_eq!(loaded.memory.read_u32_be(0), Some(0));
    assert!(loaded.memory.write_u32_be(0, 0xdead_beef).is_some());
    assert_eq!(loaded.memory.read_u32_be(0), Some(0xdead_beef));
    assert_eq!(loaded.stack_pointer & 0x0f, 0);
    assert_eq!(loaded.import_count, 1);
    assert_eq!(loaded.imports[0].library_name, "InterfaceLib");
    assert_eq!(loaded.imports[0].symbol_name, "TestImport");
    assert_eq!(loaded.imports[0].class, 2);
    assert!(!loaded.imports[0].weak);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(
        loaded.imports[0].tvector_address,
        Some(PPC_IMPORT_TVECTOR_BASE)
    );
    assert_eq!(loaded.imports[0].trap_pc, PPC_IMPORT_TRAP_BASE);
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::Unsupported
    );

    let (result, trace) = loaded.run_import_trace(64);
    assert!(
        matches!(
            result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                ..
            }
        ),
        "{result:?}"
    );
    assert_eq!(trace, vec![0]);
}

#[test]
fn import_bindings_preserve_symbol_metadata_and_synthetic_addresses() {
    let bindings = PpcImportBindingPlan::prepare(
        vec![
            PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "Gestalt".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 1,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FindSymbol".to_string(),
                class: 0,
                weak: true,
            },
        ],
        2,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .unwrap()
    .into_initial_bindings();

    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].library_name, "InterfaceLib");
    assert_eq!(bindings[0].symbol_name, "Gestalt");
    assert_eq!(bindings[0].address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(bindings[0].tvector_address, Some(PPC_IMPORT_TVECTOR_BASE));
    assert_eq!(bindings[0].trap_pc, PPC_IMPORT_TRAP_BASE);
    assert!(!bindings[0].weak);

    assert_eq!(bindings[1].symbol_name, "FindSymbol");
    assert_eq!(bindings[1].address, PPC_IMPORT_TRAP_BASE + 4);
    assert_eq!(bindings[1].tvector_address, None);
    assert_eq!(bindings[1].trap_pc, PPC_IMPORT_TRAP_BASE + 4);
    assert!(bindings[1].weak);
    assert_eq!(
        bindings[1].dispatcher_target,
        PpcImportDispatcherTarget::FindSymbol
    );
}

#[test]
fn unavailable_weak_import_relocates_to_cfm_unresolved_symbol_address() {
    let pef = synthetic_pef_with_import_class(b"MissingOptionalAPI", 0x82);
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].address, 0);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::UnresolvedWeak
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_DATA_BASE), Some(0));
}


#[test]
fn import_bindings_reject_symbol_indexes_outside_import_table() {
    let error = PpcImportBindingPlan::prepare(
        vec![PefResolvedImport {
            library_index: 0,
            symbol_index: 3,
            library_name: "InterfaceLib".to_string(),
            symbol_name: "BadImport".to_string(),
            class: 2,
            weak: false,
        }],
        1,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .map_err(ppc_initial_import_error)
    .unwrap_err();

    assert_eq!(
        error,
        PpcLoadError::ImportBindingOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }
    );
}

#[test]
fn initial_import_plan_preserves_reachable_loader_errors() {
    let result = load_pef_application(&synthetic_pef_with_loader(
        synthetic_loader_with_repeated_imports(PPC_IMPORT_CAPACITY + 1),
    ));
    let error = match result {
        Ok(_) => panic!("over-capacity launch unexpectedly returned an app"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        PpcLoadError::ImportCapacityExceeded {
            import_count: PPC_IMPORT_CAPACITY + 1,
            capacity: PPC_IMPORT_CAPACITY,
        }
    );
}

#[test]
fn overlapping_library_ranges_preserve_duplicate_import_projection() {
    let pef = synthetic_pef_with_loader_and_data(
        synthetic_loader_with_overlapping_library_ranges(),
        &[0; 8],
    );
    let resolved = resolve_pef_imports(&pef).unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].symbol_index, 0);
    assert_eq!(resolved[1].symbol_index, 0);
    assert_ne!(resolved[0].library_name, resolved[1].library_name);

    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.imports.len(), 2);
    let first = loaded.import_binding(0).unwrap();
    assert_eq!(first.library_name, "InterfaceLib");
    assert_eq!(first.address, PPC_IMPORT_TVECTOR_BASE);
    assert_eq!(loaded.imports[1].address, PPC_IMPORT_STD_ERRNO);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_ERRNO)
    );
}

#[test]
fn import_binding_error_maps_preserve_loader_and_fragment_results() {
    assert_eq!(
        ppc_initial_import_error(PpcImportBindingError::SymbolIndexOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }),
        PpcLoadError::ImportBindingOutOfRange {
            symbol_index: 3,
            import_count: 1,
        }
    );
    assert_eq!(
        ppc_initial_import_error(PpcImportBindingError::CapacityExceeded {
            import_count: 9,
            capacity: 8,
        }),
        PpcLoadError::ImportCapacityExceeded {
            import_count: 9,
            capacity: 8,
        }
    );
    for error in [
        PpcImportBindingError::CountOverflow,
        PpcImportBindingError::BindingAddressOverflow,
        PpcImportBindingError::AddressTableOutOfRange,
    ] {
        assert_eq!(ppc_initial_import_error(error), PpcLoadError::AddressOverflow);
    }
    for (error, expected) in [
        (PpcImportBindingError::CountOverflow, PPC_FRAG_NO_MEM),
        (
            PpcImportBindingError::CapacityExceeded {
                import_count: 9,
                capacity: 8,
            },
            PPC_FRAG_NO_MEM,
        ),
        (PpcImportBindingError::AddressTableOutOfRange, PPC_FRAG_NO_MEM),
        (
            PpcImportBindingError::SymbolIndexOutOfRange {
                symbol_index: 3,
                import_count: 1,
            },
            PPC_FRAG_CORRUPT_ERR,
        ),
        (
            PpcImportBindingError::BindingAddressOverflow,
            PPC_FRAG_CORRUPT_ERR,
        ),
        (PpcImportBindingError::RegistryChanged, PPC_FRAG_CORRUPT_ERR),
    ] {
        assert_eq!(ppc_dynamic_import_error(error), expected);
    }
}
