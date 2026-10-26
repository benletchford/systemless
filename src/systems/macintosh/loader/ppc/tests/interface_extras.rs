use super::*;

const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;

fn loaded_with(symbol: &[u8]) -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
    loaded.memory.add_region(SCRATCH, vec![0; 0x400]);
    loaded
}

#[test]
fn interface_imports_resolve_to_native_targets() {
    for (symbol, target) in [
        ("HMShowBalloon", PpcImportDispatcherTarget::HMShowBalloon),
        ("SetMCEntries", PpcImportDispatcherTarget::SetMCEntries),
        (
            "LGetCellDataLocation",
            PpcImportDispatcherTarget::LGetCellDataLocation,
        ),
        ("LAddColumn", PpcImportDispatcherTarget::LAddColumn),
        ("LCellSize", PpcImportDispatcherTarget::LCellSize),
        ("LRect", PpcImportDispatcherTarget::LRect),
        (
            "TEGetStyleHandle",
            PpcImportDispatcherTarget::TEGetStyleHandle,
        ),
        ("TEReplaceStyle", PpcImportDispatcherTarget::TEReplaceStyle),
        (
            "DisposeIconSuite",
            PpcImportDispatcherTarget::DisposeIconSuite,
        ),
        ("StdText", PpcImportDispatcherTarget::StdText),
        ("StdTxMeas", PpcImportDispatcherTarget::StdTxMeas),
        ("IsOutline", PpcImportDispatcherTarget::IsOutline),
        ("OutlineMetrics", PpcImportDispatcherTarget::OutlineMetrics),
        (
            "FrameArc",
            PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::FrameArc),
        ),
        (
            "StdRect",
            PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::StdRect),
        ),
        (
            "StdRgn",
            PpcImportDispatcherTarget::QuickDrawShape(PpcQuickDrawShapeOperation::StdRgn),
        ),
        (
            "PtToAngle",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::PtToAngle),
        ),
        (
            "IsCmdChar",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::IsCmdChar),
        ),
        ("GetColor", PpcImportDispatcherTarget::GetColor),
        (
            "FindServiceWindow",
            PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::FindServiceWindow),
        ),
        ("SetControlColor", PpcImportDispatcherTarget::ReturnNoErr),
        ("AddSearch", PpcImportDispatcherTarget::ReturnNoErr),
        ("DelSearch", PpcImportDispatcherTarget::ReturnNoErr),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn pt_to_angle_is_scaled_to_the_rectangle() {
    let rect = (0, 0, 100, 200);
    for (point, angle) in [
        ((0, 100), 0),
        ((0, 200), 45),
        ((50, 200), 90),
        ((100, 200), 135),
        ((100, 100), 180),
        ((50, 0), 270),
        ((0, 0), 315),
    ] {
        assert_eq!(
            dispatch_toolbox_compat::ppc_pt_to_angle(rect, point),
            angle,
            "{point:?}"
        );
    }

    let mut loaded = loaded_with(b"PtToAngle");
    ppc_write_rect(&mut loaded.memory, SCRATCH, 0, 0, 100, 200).unwrap();
    loaded.cpu.gpr[3] = SCRATCH;
    loaded.cpu.gpr[4] = (50 << 16) | 200;
    loaded.cpu.gpr[5] = SCRATCH + 0x10;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::PtToAngle),
    );
    assert_eq!(loaded.memory.read_u16_be(SCRATCH + 0x10), Some(90));
}

#[test]
fn is_cmd_char_requires_a_command_key_event_for_the_character() {
    let mut loaded = loaded_with(b"IsCmdChar");
    let target = PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::IsCmdChar);
    for (what, modifiers, test, expected) in [
        (3u16, 0x0100u16, b'q', 1),
        (5, 0x0100, b'q', 1),
        (3, 0x0100, b'w', 0),
        (3, 0, b'q', 0),
        (1, 0x0100, b'q', 0),
    ] {
        loaded.memory.write_u16_be(SCRATCH, what).unwrap();
        loaded
            .memory
            .write_u32_be(SCRATCH + 2, 0x0c00 | u32::from(b'q'))
            .unwrap();
        loaded.memory.write_u16_be(SCRATCH + 14, modifiers).unwrap();
        loaded.cpu.gpr[3] = SCRATCH;
        loaded.cpu.gpr[4] = u32::from(test);
        run_test_import(&mut loaded, target.clone());
        assert_eq!(
            loaded.cpu.gpr[3], expected,
            "what={what} modifiers={modifiers:#x}"
        );
    }
}

#[test]
fn color_picker_service_window_and_balloon_calls_report_absent_ui() {
    let mut loaded = loaded_with(b"GetColor");
    let (input, output) = (SCRATCH, SCRATCH + 8);
    loaded
        .memory
        .write_bytes(input, &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc])
        .unwrap();
    loaded.cpu.gpr[5] = input;
    loaded.cpu.gpr[6] = output;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetColor);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output, 6),
        Some(vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc])
    );

    loaded
        .memory
        .write_u32_be(SCRATCH + 0x20, 0xdead_beef)
        .unwrap();
    loaded.cpu.gpr[3] = (10 << 16) | 10;
    loaded.cpu.gpr[4] = SCRATCH + 0x20;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ToolboxCompat(PpcToolboxCompatOperation::FindServiceWindow),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(SCRATCH + 0x20), Some(0));

    loaded.toolbox_startup.help_balloons_enabled = false;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HMShowBalloon);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-850));
    loaded.toolbox_startup.help_balloons_enabled = true;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HMShowBalloon);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-853));

    run_test_import(&mut loaded, PpcImportDispatcherTarget::IsOutline);
    assert_eq!(loaded.cpu.gpr[3], 0);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OutlineMetrics);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-32615));
}

#[test]
fn std_tx_meas_matches_text_width_and_fills_font_info() {
    let mut loaded = loaded_with(b"StdTxMeas");
    let text = SCRATCH;
    loaded.memory.write_bytes(text, b"Measure me").unwrap();
    loaded.cpu.gpr[3] = text;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 10;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TextWidth);
    let width = loaded.cpu.gpr[3];
    assert!(width > 0);

    let info = SCRATCH + 0x40;
    loaded.cpu.gpr[3] = 10;
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = SCRATCH + 0x20;
    loaded.cpu.gpr[6] = SCRATCH + 0x24;
    loaded.cpu.gpr[7] = info;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::StdTxMeas);
    assert_eq!(loaded.cpu.gpr[3], width);
    assert_ne!(loaded.memory.read_u16_be(info), Some(0), "ascent");
}

fn new_list(loaded: &mut PpcLoadedApp) -> u32 {
    let view = SCRATCH + 0x100;
    let bounds = SCRATCH + 0x108;
    ppc_write_rect(&mut loaded.memory, view, 10, 20, 90, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, bounds, 0, 0, 2, 2).unwrap();
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = bounds;
    loaded.cpu.gpr[5] = (20u32 << 16) | 100;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = PPC_MAIN_GWORLD;
    for register in 8..=10 {
        loaded.cpu.gpr[register] = 0;
    }
    let slot =
        ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT).unwrap();
    loaded.memory.write_u32_be(slot, 0).unwrap();
    run_test_import(loaded, PpcImportDispatcherTarget::LNew);
    loaded.cpu.gpr[3]
}

fn set_cell(loaded: &mut PpcLoadedApp, list: u32, (v, h): (i16, i16), text: &[u8]) {
    loaded.memory.write_bytes(SCRATCH + 0x180, text).unwrap();
    loaded.cpu.gpr[3] = SCRATCH + 0x180;
    loaded.cpu.gpr[4] = text.len() as u32;
    loaded.cpu.gpr[5] = (u32::from(v as u16) << 16) | u32::from(h as u16);
    loaded.cpu.gpr[6] = list;
    run_test_import(loaded, PpcImportDispatcherTarget::LSetCell);
}

#[test]
fn list_columns_cell_geometry_and_data_locations() {
    let mut loaded = loaded_with(b"LNew");
    let list = new_list(&mut loaded);
    assert_ne!(list, 0);
    set_cell(&mut loaded, list, (0, 0), b"ab");
    set_cell(&mut loaded, list, (0, 1), b"cde");
    set_cell(&mut loaded, list, (1, 0), b"f");

    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LAddColumn);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let record = loaded.list_manager.get_record(list).unwrap();
    assert_eq!(record.data_bounds, (0, 0, 2, 3));
    assert_eq!(record.cells.get(&(0, 2)), Some(&b"cde".to_vec()));
    assert_eq!(record.cells.get(&(0, 1)), None);

    // Row-major data: (0,0) "ab", (0,1) empty, (0,2) "cde", (1,0) "f".
    let (offset, length) = (SCRATCH, SCRATCH + 2);
    for (cell, expected) in [((0i16, 2i16), (2, 3)), ((1, 0), (5, 1)), ((5, 5), (-1, -1))] {
        loaded.cpu.gpr[3] = offset;
        loaded.cpu.gpr[4] = length;
        loaded.cpu.gpr[5] = (u32::from(cell.0 as u16) << 16) | u32::from(cell.1 as u16);
        loaded.cpu.gpr[6] = list;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LGetCellDataLocation);
        assert_eq!(
            (
                loaded.memory.read_u16_be(offset).unwrap() as i16,
                loaded.memory.read_u16_be(length).unwrap() as i16
            ),
            expected,
            "{cell:?}"
        );
    }

    loaded.cpu.gpr[3] = SCRATCH + 0x20;
    loaded.cpu.gpr[4] = (1 << 16) | 1;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LRect);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, SCRATCH + 0x20),
        Some((30, 120, 50, 220))
    );
    loaded.cpu.gpr[4] = (9 << 16) | 9;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LRect);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, SCRATCH + 0x20),
        Some((0, 0, 0, 0))
    );

    loaded.cpu.gpr[3] = (16 << 16) | 50;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LCellSize);
    assert_eq!(
        loaded.list_manager.get_record(list).unwrap().cell_size,
        (16, 50)
    );
    let list_ptr = loaded.memory.read_u32_be(list).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u32_be(list_ptr + PPC_LIST_CELL_SIZE_OFFSET),
        Some((16 << 16) | 50)
    );
}

fn write_text_style(memory: &mut PpcSectionMem, address: u32, font: i16) {
    memory.write_u16_be(address, font as u16).unwrap();
    memory.write_u8(address + 2, 0).unwrap();
    memory.write_u16_be(address + 4, 12).unwrap();
    for offset in [6, 8, 10] {
        memory.write_u16_be(address + offset, 0).unwrap();
    }
}

#[test]
fn styled_text_edit_style_handle_and_replace_style() {
    let mut loaded = loaded_with(b"TEStyleNew");
    let rects = SCRATCH;
    ppc_write_rect(&mut loaded.memory, rects, 10, 20, 80, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 10, 20, 80, 220).unwrap();
    loaded.cpu.gpr[3] = rects;
    loaded.cpu.gpr[4] = rects + 8;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEStyleNew);
    let te = loaded.cpu.gpr[3];
    let te_ptr = loaded.memory.read_u32_be(te).unwrap();
    let style_handle = loaded
        .memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap();
    loaded.cpu.gpr[3] = te;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEGetStyleHandle);
    assert_eq!(loaded.cpu.gpr[3], style_handle);
    assert_ne!(style_handle, 0);

    let text = SCRATCH + 0x20;
    let style = SCRATCH + 0x40;
    let old_style = SCRATCH + 0x60;
    loaded.memory.write_bytes(text, b"Styled").unwrap();
    loaded.cpu.gpr[3] = text;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetText);
    for (start, end, font) in [(0u32, 3u32, 3i16), (3, 6, 4)] {
        write_text_style(&mut loaded.memory, style, font);
        loaded.cpu.gpr[3] = start;
        loaded.cpu.gpr[4] = end;
        loaded.cpu.gpr[5] = te;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
        loaded.cpu.gpr[3] = 0x0001;
        loaded.cpu.gpr[4] = style;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = te;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetStyle);
    }

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = te;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TESetSelect);
    write_text_style(&mut loaded.memory, old_style, 4);
    write_text_style(&mut loaded.memory, style, 22);
    loaded.cpu.gpr[3] = 0x0001;
    loaded.cpu.gpr[4] = old_style;
    loaded.cpu.gpr[5] = style;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = te;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEReplaceStyle);
    let runs = ppc_te_style_runs(&mut loaded.memory, &test_handle_records!(loaded), te, 6);
    assert_eq!(
        runs.iter()
            .map(|run| (run.start, run.style.font))
            .collect::<Vec<_>>(),
        vec![(0, 3), (3, 22)]
    );

    // addSize must reach the style editor instead of being masked away.
    // Only the matching font run changes; the other run retains its size.
    let previous_sizes: Vec<_> = runs.iter().map(|run| run.style.size).collect();
    write_text_style(&mut loaded.memory, old_style, 22);
    loaded.memory.write_u16_be(style + 4, 3).unwrap();
    loaded.cpu.gpr[3] = 0x0011; // doFont | addSize
    loaded.cpu.gpr[4] = old_style;
    loaded.cpu.gpr[5] = style;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = te;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::TEReplaceStyle);
    let runs = ppc_te_style_runs(&mut loaded.memory, &test_handle_records!(loaded), te, 6);
    assert_eq!(runs[0].style.size, previous_sizes[0]);
    assert_eq!(runs[1].style.size, previous_sizes[1] + 3);

    let mut plain = loaded_with(b"TENew");
    ppc_write_rect(&mut plain.memory, rects, 10, 20, 80, 220).unwrap();
    ppc_write_rect(&mut plain.memory, rects + 8, 10, 20, 80, 220).unwrap();
    plain.cpu.gpr[3] = rects;
    plain.cpu.gpr[4] = rects + 8;
    run_test_import(&mut plain, PpcImportDispatcherTarget::TENew);
    run_test_import(&mut plain, PpcImportDispatcherTarget::TEGetStyleHandle);
    assert_eq!(plain.cpu.gpr[3], 0);
}

#[test]
fn dispose_icon_suite_releases_the_suite_but_not_resource_members() {
    let mut loaded = loaded_with(b"GetIconSuite");
    let ref_num = *loaded.process_file_system.current_resource_file;
    loaded
        .process_file_system
        .extend_vfs_resources([PpcVfsResourceRecord {
            ref_num,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"ICN#"),
            res_id: 128,
            name: Vec::new(),
            data: vec![0x11; 256].into(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        }]);
    loaded.cpu.gpr[3] = SCRATCH;
    loaded.cpu.gpr[4] = 128;
    loaded.cpu.gpr[5] = 0x0000_0001;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIconSuite);
    let suite = loaded.memory.read_u32_be(SCRATCH).unwrap();
    let members = ppc_icon_suite_member_handles(&mut loaded.memory, suite).unwrap();
    assert_eq!(members.len(), 1);

    loaded.cpu.gpr[3] = suite;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeIconSuite);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(ppc_icon_suite_member_handles(&mut loaded.memory, suite).is_none());
    assert!(test_handle_records!(loaded)
        .iter()
        .any(|record| record.handle == members[0]));

    loaded.cpu.gpr[3] = SCRATCH + 0x10;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeIconSuite);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn set_mc_entries_merges_menu_color_entries() {
    let mut loaded = loaded_with(b"SetMCEntries");
    let table = SCRATCH;
    let entry = |id: i16, item: i16| {
        let mut bytes = vec![0u8; 30];
        bytes[..2].copy_from_slice(&id.to_be_bytes());
        bytes[2..4].copy_from_slice(&item.to_be_bytes());
        bytes[4] = 0x7f;
        bytes
    };
    let mut bytes = entry(128, 0);
    bytes.extend(entry(-99, 0));
    loaded.memory.write_bytes(table, &bytes).unwrap();
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = table;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetMCEntries);
    let handles = test_handle_records!(loaded);
    let current = ppc_menu_color_table_bytes(&mut loaded.memory, &handles);
    assert_eq!(current, entry(128, 0));
}
