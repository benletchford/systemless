use super::*;

#[test]
fn scrollbar_tracking_without_action_leaves_arrow_and_page_values_to_the_caller() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TrackControl")).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(loaded, |controls| ppc_new_control_record_values(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        controls,
        PPC_MAIN_GWORLD,
        (0, 0, 100, 16),
        b"",
        true,
        12,
        0,
        25,
        16,
        0,
    ));
    assert_ne!(handle, 0);
    let control = ppc_control_ptr(&mut loaded.memory, handle).unwrap();
    // Macintosh Toolbox Essentials (1992), pp. 5-79--5-80 and 5-91:
    // Arrow/page actions belong to the caller; TrackControl only changes
    // the value itself when tracking the scroll-box indicator.
    for action in [0, u32::MAX] {
        for (v, expected_part) in [(5, 20), (95, 21), (25, 22), (75, 23)] {
            loaded.cpu.gpr[3] = handle;
            loaded.cpu.gpr[4] = (v << 16) | 8;
            loaded.cpu.gpr[5] = action;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TrackControl),
            );
            assert_eq!(loaded.cpu.gpr[3], expected_part);
            assert_eq!(
                loaded
                    .memory
                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
                Some(12)
            );
        }
    }
}


#[test]
fn dialog_scrollbar_tracking_changes_the_value_for_arrow_clicks() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(loaded, |controls| ppc_new_control_record_values(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        controls,
        PPC_MAIN_GWORLD,
        (0, 0, 100, 16),
        b"",
        true,
        0,
        0,
        25,
        16,
        0,
    ));
    assert_ne!(handle, 0);
    let control = ppc_control_ptr(&mut loaded.memory, handle).unwrap();
    let handles = test_handle_records!(loaded);
    let controls = loaded.controls.records();
    let resources = &loaded.process_file_system.vfs_resources;
    let refnum = *loaded.process_file_system.current_resource_file;

    assert_eq!(ppc_track_scroll_control_value(&mut loaded.memory, &handles, &controls, &loaded.gworlds, resources, refnum, handle, 95, 8), Some(21));
    assert_eq!(loaded.memory.read_u16_be(control + PPC_CONTROL_VALUE_OFFSET), Some(1));
    assert_eq!(ppc_track_scroll_control_value(&mut loaded.memory, &handles, &controls, &loaded.gworlds, resources, refnum, handle, 5, 8), Some(20));
    assert_eq!(loaded.memory.read_u16_be(control + PPC_CONTROL_VALUE_OFFSET), Some(0));
}

#[test]
fn hle_import_runner_creates_and_links_a_classic_control_record() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewControl")).unwrap();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        64,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 10, 20, 40, 140).unwrap();
    write_ppc_pstring(&mut loaded.memory, scratch + 8, b"Launch");
    let ref_con_slot =
        ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
            .unwrap();
    loaded
        .memory
        .write_u32_be(ref_con_slot, 0x1234_5678)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 3;
    loaded.cpu.gpr[8] = 1;
    loaded.cpu.gpr[9] = 9;
    loaded.cpu.gpr[10] = 16;

    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    let control = loaded.memory.read_u32_be(handle).unwrap();
    assert_ne!(handle, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
        Some(handle)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, control + PPC_CONTROL_RECT_OFFSET),
        Some((10, 20, 40, 140))
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(control + PPC_CONTROL_OWNER_OFFSET),
        Some(PPC_MAIN_GWORLD)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(3)
    );
    assert_eq!(
        loaded.memory.read_u16_be(control + PPC_CONTROL_MIN_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u16_be(control + PPC_CONTROL_MAX_OFFSET),
        Some(9)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(control + PPC_CONTROL_REF_CON_OFFSET),
        Some(0x1234_5678)
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, control + PPC_CONTROL_TITLE_OFFSET),
        Some(b"Launch".to_vec())
    );
    assert_eq!(
        loaded.controls.records(),
        vec![PpcControlRecord {
            handle,
            pointer: control,
            proc_id: 16,
            popup_menu_id: 0,
            popup_title_width: None,
        }]
    );
}

#[test]
fn popup_control_records_preserve_item_values_across_set_control_value() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SetControlValue"))
        .unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_record_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            (10, 20, 30, 180),
            b"Loadout",
            true,
            1,
            143,
            60,
            1008,
            0,
        )
    );
    assert_ne!(handle, 0);
    let control = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded
            .controls
            .records()
            .into_iter()
            .find(|record| record.handle == handle)
            .map(|record| (record.popup_menu_id, record.popup_title_width)),
        Some((143, Some(60)))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 4;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(4)
    );
}

#[test]
fn selected_checkbox_draws_indicator_and_checkmark_without_framing_title() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_record_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            (10, 20, 30, 180),
            b"Sound Effects",
            true,
            1,
            0,
            1,
            1,
            0,
        )
    );
    assert_ne!(handle, 0);
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 40, 200),
        PPC_RGB_WHITE,
        None,
    ));

    assert!(ppc_draw_control(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        handle,
    ));

    let front = surface.front_buffer;
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (22, 20)),
        Some(black)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (179, 10)),
        Some(white)
    );
}

#[test]
fn selected_radio_button_draws_round_indicator_and_inner_dot() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_record_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            (10, 20, 30, 180),
            b"Veteran",
            true,
            1,
            0,
            1,
            2,
            0,
        )
    );
    assert_ne!(handle, 0);
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 40, 200),
        PPC_RGB_WHITE,
        None,
    ));

    assert!(ppc_draw_control(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        handle,
    ));

    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (27, 14)),
        Some(black),
        "the outer radio indicator should be round"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (22, 14)),
        Some(white),
        "the indicator corner must remain outside the round outline"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (27, 19)),
        Some(black),
        "contrlValue 1 should draw the inner selection dot"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (179, 10)),
        Some(white),
        "radioButProc must not frame the full control rectangle"
    );
}

#[test]
fn draw_controls_redraws_visible_controls_in_a_document_window() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawControls")).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_record_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            (10, 20, 30, 100),
            b"Redraw",
            true,
            0,
            0,
            1,
            0,
            0,
        )
    );
    assert_ne!(handle, 0);
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 40, 120),
        PPC_RGB_WHITE,
        None,
    ));

    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.run_with_hle_imports(64);

    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (60, 10)),
        Some(black)
    );
}

#[test]
fn classic_control_hit_testing_and_disposal_follow_the_window_list() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        32,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 5, 6, 25, 86).unwrap();
    write_ppc_pstring(&mut loaded.memory, scratch + 8, b"OK");
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            scratch,
            scratch + 8,
            true,
            0,
            0,
            1,
            0,
            0,
        )
    );
    assert_ne!(handle, 0);
    assert_eq!(
        ppc_find_control_at_point(
            &mut loaded.memory,
            &loaded.controls.records(),
            PPC_MAIN_GWORLD,
            10,
            10,
        ),
        Some((handle, 10))
    );

    with_test_controls!(
        loaded,
        |controls| ppc_dispose_control(
            None,
            Some(&mut free_handle_blocks),
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            handle,
        )
    );

    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
        Some(0)
    );
    assert!(loaded.controls.is_empty());
}

#[test]
fn hle_import_runner_test_control_returns_the_hit_part() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TestControl")).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        32,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 5, 6, 25, 86).unwrap();
    write_ppc_pstring(&mut loaded.memory, scratch + 8, b"OK");
    let handle = with_test_controls!(
        loaded,
        |controls| ppc_new_control_values(
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            controls,
            PPC_MAIN_GWORLD,
            scratch,
            scratch + 8,
            true,
            0,
            0,
            1,
            0,
            0,
        )
    );
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = (10 << 16) | 10;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 10);
}

#[test]
fn popup_track_control_requires_a_visible_enabled_hit_and_cdef_action() {
    for (label, visible, hilite, point, requested, stored, opens) in [
        ("outside", true, 0, (0i16, 0i16), u32::MAX, u32::MAX, false),
        ("hidden", false, 0, (10, 10), u32::MAX, u32::MAX, false),
        ("inactive", true, 0xff, (10, 10), u32::MAX, u32::MAX, false),
        ("nil action", true, 0, (10, 10), 0, u32::MAX, false),
        ("nil stored action", true, 0, (10, 10), u32::MAX, 0, false),
        ("CDEF action", true, 0, (10, 10), u32::MAX, u32::MAX, true),
    ] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"TrackControl")).unwrap();
        let menu_handle = install_test_popup_menu(
            &mut loaded,
            PPC_DATA_BASE + 0x1000,
            304,
            b"Popup",
            b"One;Two",
        );
        let mut last_mem_error = loaded.last_mem_error();
        let scratch = ppc_heap_alloc(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            32,
            true,
        );
        ppc_write_rect(&mut loaded.memory, scratch, 5, 6, 25, 86).unwrap();
        write_ppc_pstring(&mut loaded.memory, scratch + 8, b"Popup:");
        let control_handle = with_test_controls!(
            loaded,
            |controls| ppc_new_control_values(
                None,
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                &mut last_mem_error,
                test_handles!(loaded),
                controls,
                PPC_MAIN_GWORLD,
                scratch,
                scratch + 8,
                visible,
                1,
                304,
                52,
                1008,
                0,
            )
        );
        assert_ne!(control_handle, 0, "{label} control allocation");
        let control = ppc_control_ptr(&mut loaded.memory, control_handle).unwrap();
        loaded
            .memory
            .write_u8(control + PPC_CONTROL_HILITE_OFFSET, hilite)
            .unwrap();
        loaded.cpu.gpr[3] = control_handle;
        loaded.cpu.gpr[4] = (u32::from(point.0 as u16) << 16) | u32::from(point.1 as u16);
        assert_eq!(loaded.memory.read_u32_be(control + 32), Some(u32::MAX));
        loaded.memory.write_u32_be(control + 32, stored).unwrap();
        loaded.cpu.gpr[5] = requested;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: point.0,
            mouse_h: point.1,
            ..PpcInputSnapshot::default()
        });

        let probe = loaded.run_with_hle_imports(64);

        if opens {
            assert!(loaded.toolbox_startup.execution.menu().is_some(), "{label}");
            assert!(
                loaded
                    .toolbox_startup
                    .execution.menu()
                    .context()
                    .native_popup()
                    .is_some(),
                "{label}"
            );
            continue;
        }
        assert!(
            matches!(probe.result, PpcRunResult::Halted { .. }),
            "{label}"
        );
        assert_eq!(
            loaded.cpu.gpr[3],
            if visible && hilite == 0 && point == (10, 10) {
                10
            } else {
                0
            },
            "{label} TrackControl result",
        );
        assert!(loaded.toolbox_startup.execution.menu().is_none(), "{label}");
        assert_eq!(
            loaded.toolbox_startup.execution.menu().context().native_popup(),
            None,
            "{label}"
        );
        assert_eq!(
            loaded
                .memory
                .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
            Some(1),
            "{label} TrackControl must not mutate the value"
        );
        assert_ne!(menu_handle, 0);
    }
}

#[test]
fn hle_import_runner_updates_control_titles_and_values() {
    let pef = synthetic_pef_with_import(b"SetControlTitle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let title_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(title_ptr, vec![0; 256]);
    write_ppc_pstring(&mut loaded.memory, title_ptr, b"Continue");
    let control_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &[0],
    );
    loaded.cpu.gpr[3] = control_handle;
    loaded.cpu.gpr[4] = title_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, control + PPC_CONTROL_TITLE_OFFSET),
        Some(b"Continue".to_vec())
    );
    loaded
        .memory
        .write_u16_be(control + PPC_CONTROL_MAX_OFFSET, 1)
        .unwrap();

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetControlValue;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = control_handle;
    loaded.cpu.gpr[4] = 1;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(1)
    );
}

#[test]
fn legacy_control_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("DisposeControl", PpcLegacyControlOperation::DisposeControl),
        ("Draw1Control", PpcLegacyControlOperation::DrawOneControl),
        ("FindControl", PpcLegacyControlOperation::FindControl),
        ("GetControlMaximum", PpcLegacyControlOperation::GetControlMaximum),
        ("GetControlReference", PpcLegacyControlOperation::GetControlReference),
        ("GetControlMinimum", PpcLegacyControlOperation::GetControlMinimum),
        ("GetControlTitle", PpcLegacyControlOperation::GetControlTitle),
        ("GetControlValue", PpcLegacyControlOperation::GetControlValue),
        ("GetNewControl", PpcLegacyControlOperation::GetNewControl),
        ("HideControl", PpcLegacyControlOperation::HideControl),
        ("KillControls", PpcLegacyControlOperation::KillControls),
        ("MoveControl", PpcLegacyControlOperation::MoveControl),
        ("NewControl", PpcLegacyControlOperation::NewControl),
        ("SetControlMaximum", PpcLegacyControlOperation::SetControlMaximum),
        ("SetControlReference", PpcLegacyControlOperation::SetControlReference),
        ("SetControlMinimum", PpcLegacyControlOperation::SetControlMinimum),
        ("ShowControl", PpcLegacyControlOperation::ShowControl),
        ("SizeControl", PpcLegacyControlOperation::SizeControl),
        ("TestControl", PpcLegacyControlOperation::TestControl),
        ("TrackControl", PpcLegacyControlOperation::TrackControl),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyControl(operation),
        );
    }
}

#[test]
fn control_reference_imports_update_and_read_the_classic_control_record() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SetControlReference"))
        .unwrap();
    let handle = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        4,
        true,
    );
    let record = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        PPC_CONTROL_RECORD_SIZE,
        true,
    );
    loaded.memory.write_u32_be(handle, record).unwrap();
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(record + PPC_CONTROL_REF_CON_OFFSET),
        Some(0x1234_5678)
    );

    let mut getter = load_pef_application(&synthetic_pef_with_import(b"GetControlReference"))
        .unwrap();
    let getter_handle = ppc_heap_alloc(
        &mut getter.memory,
        test_heap_cursor!(getter),
        test_heap_limit!(getter),
        4,
        true,
    );
    let getter_record = ppc_heap_alloc(
        &mut getter.memory,
        test_heap_cursor!(getter),
        test_heap_limit!(getter),
        PPC_CONTROL_RECORD_SIZE,
        true,
    );
    getter.memory.write_u32_be(getter_handle, getter_record).unwrap();
    getter
        .memory
        .write_u32_be(getter_record + PPC_CONTROL_REF_CON_OFFSET, 0x8765_4321)
        .unwrap();
    getter.cpu.gpr[3] = getter_handle;

    let getter_probe = getter.run_with_hle_imports(64);

    assert_eq!(getter_probe.unsupported_import_index, None);
    assert_eq!(getter.cpu.gpr[3], 0x8765_4321);
}

#[test]
fn import_bindings_classify_control_title_and_value_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetControlTitle"),
        PpcImportDispatcherTarget::SetControlTitle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetControlValue"),
        PpcImportDispatcherTarget::SetControlValue
    );
}

#[test]
fn hle_import_runner_handles_set_control_value_defaults() {
    let pef = synthetic_pef_with_import(b"SetControlValue");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x100;
    loaded.cpu.gpr[4] = 7;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x100);
    assert_eq!(loaded.cpu.gpr[4], 7);
}
