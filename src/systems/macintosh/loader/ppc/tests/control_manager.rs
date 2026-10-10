use super::*;

fn prepare_standard_control_tracking(proc_id: i16) -> (PpcLoadedApp, u32, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TrackControl")).unwrap();
    let mut error = loaded.last_mem_error();
    let handle = with_test_controls!(loaded, |controls| ppc_new_control_record_values(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut error,
        test_handles!(loaded),
        controls,
        PPC_MAIN_GWORLD,
        (10, 10, 30, 100),
        b"Choice",
        true,
        1,
        0,
        1,
        proc_id,
        0,
    ));
    let pointer = ppc_control_ptr(&mut loaded.memory, handle).unwrap();
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = (20 << 16) | 20;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.lr = PPC_HALT_PC;
    (loaded, handle, pointer)
}

fn begin_standard_control_tracking(proc_id: i16) -> (PpcLoadedApp, u32, u32) {
    let (mut loaded, handle, pointer) = prepare_standard_control_tracking(proc_id);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: 20,
        mouse_h: 20,
        ..PpcInputSnapshot::default()
    });
    let sp = loaded.cpu.gpr[1];
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(loaded.cpu.gpr[1], sp);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert!(loaded.toolbox_startup.simple_control_tracking.is_some());
    assert_eq!(
        loaded.memory.read_u8(pointer + PPC_CONTROL_HILITE_OFFSET),
        Some(if proc_id == 0 { 10 } else { 11 })
    );
    (loaded, handle, pointer)
}

#[test]
fn standard_control_tracking_handles_release_queued_before_entry() {
    for proc_id in [0, 1, 2] {
        for inside in [true, false] {
            let (mut loaded, _, pointer) = prepare_standard_control_tracking(proc_id);
            loaded.event_queue.push_back(PpcQueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: if inside { 20 } else { 40 },
                where_h: 20,
                modifiers: 0,
            });
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: if inside { 40 } else { 20 },
                mouse_h: 20,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
            assert_eq!(
                loaded.cpu.gpr[3],
                if inside {
                    if proc_id == 0 {
                        10
                    } else {
                        11
                    }
                } else {
                    0
                }
            );
            assert_eq!(
                loaded
                    .memory
                    .read_u16_be(pointer + PPC_CONTROL_VALUE_OFFSET),
                Some(1)
            );
            assert_eq!(
                loaded.memory.read_u8(pointer + PPC_CONTROL_HILITE_OFFSET),
                Some(0)
            );
            assert!(!loaded.event_queue.iter().any(|event| event.what == 2));
        }
    }
}

#[test]
fn standard_control_tracking_uses_queued_release_without_mutating_value() {
    // TrackControl returns the released part; the caller owns value changes.
    // Macintosh Toolbox Essentials (1992), pp. 5-90--5-92.
    for proc_id in [0, 1, 2] {
        for inside in [true, false] {
            let (mut loaded, _, pointer) = begin_standard_control_tracking(proc_id);
            loaded.event_queue.push_back(PpcQueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: if inside { 20 } else { 40 },
                where_h: 20,
                modifiers: 0,
            });
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: if inside { 40 } else { 20 },
                mouse_h: 20,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
            assert_eq!(
                loaded.cpu.gpr[3],
                if inside {
                    if proc_id == 0 {
                        10
                    } else {
                        11
                    }
                } else {
                    0
                }
            );
            assert_eq!(
                loaded
                    .memory
                    .read_u16_be(pointer + PPC_CONTROL_VALUE_OFFSET),
                Some(1)
            );
            assert_eq!(
                loaded.memory.read_u8(pointer + PPC_CONTROL_HILITE_OFFSET),
                Some(0)
            );
            assert!(loaded.toolbox_startup.simple_control_tracking.is_none());
            assert!(!loaded.event_queue.iter().any(|event| event.what == 2));
        }
    }
}

#[test]
fn standard_control_tracking_rejects_disposal_and_reused_identity() {
    for mutation in ["dispose", "generation", "pointer"] {
        let (mut loaded, handle, pointer) = begin_standard_control_tracking(2);
        if mutation == "dispose" {
            let mut error = loaded.last_mem_error();
            let mut free = loaded.free_handle_blocks();
            with_test_controls!(loaded, |controls| ppc_dispose_control(
                None,
                Some(&mut free),
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                &mut error,
                test_handles!(loaded),
                controls,
                handle,
            ));
        } else if mutation == "generation" {
            with_test_controls!(loaded, |controls| {
                controls
                    .iter_mut()
                    .find(|record| record.handle == handle)
                    .unwrap()
                    .generation += 1;
            });
        } else {
            loaded.memory.write_u32_be(handle, pointer + 4).unwrap();
        }
        let before = ppc_memory_read_bytes(&mut loaded.memory, pointer, 48).unwrap();
        loaded.set_input_snapshot(PpcInputSnapshot::default());
        let probe = loaded.run_with_hle_imports(64);
        assert!(
            matches!(probe.result, PpcRunResult::Halted { .. }),
            "{mutation}"
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "{mutation}");
        assert!(loaded.toolbox_startup.simple_control_tracking.is_none());
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pointer, 48),
            Some(before),
            "stale tracking must not redraw or restore state into a different lifetime: {mutation}"
        );
    }
}

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
fn scrollbar_thumb_hit_region_matches_the_painted_sixteen_pixels() {
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
    let records = loaded.controls.records();
    assert_eq!(
        ppc_control_part_at_point(&mut loaded.memory, &records, handle, 30, 8),
        Some(129)
    );
    assert_eq!(
        ppc_control_part_at_point(&mut loaded.memory, &records, handle, 32, 8),
        Some(23)
    );
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
    assert_ne!(loaded.controls.records()[0].generation, 0);
    assert_eq!(
        loaded.controls.records(),
        vec![PpcControlRecord {
            handle,
            pointer: control,
            generation: loaded.controls.records()[0].generation,
            proc_id: 16,
            popup_menu_id: 0,
            popup_title_width: None,
            active: true,
            font_style: None,
            paint: crate::control_manager::paint::ControlPaintSlot::default(),
            is_root: false,
            parent: 0,
            sub_controls: Vec::new(),
            properties: Vec::new(),
            color_proc: 0,
            control_id: (0, 0),
            command_id: 0,
            has_focus: false,
            focus_part: 0,
            drag_tracking_enabled: false,
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
    let private_handle = loaded
        .memory
        .read_u32_be(control + PPC_CONTROL_DATA_OFFSET)
        .unwrap();
    assert_ne!(private_handle, 0);
    let private_data = loaded.memory.read_u32_be(private_handle).unwrap();
    assert_ne!(private_data, 0);
    assert_eq!(loaded.memory.read_u16_be(private_data + 4), Some(143));
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
fn popup_control_exposes_its_menu_resource_through_private_data() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetNewControl")).unwrap();
    let mut cntl = vec![0; 23];
    cntl[10] = 1;
    cntl[14..16].copy_from_slice(&143i16.to_be_bytes());
    cntl[16..18].copy_from_slice(&1008i16.to_be_bytes());
    let ref_num = *loaded.process_file_system.current_resource_file;
    for (res_type, res_id, data) in [
        (u32::from_be_bytes(*b"CNTL"), 128, cntl),
        (
            u32::from_be_bytes(*b"MENU"),
            143,
            new_standard_menu_record(143, 0, b"Shapes"),
        ),
    ] {
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num,
                path: String::new(),
                res_type,
                res_id,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let control = loaded.memory.read_u32_be(loaded.cpu.gpr[3]).unwrap();
    let data_handle = loaded
        .memory
        .read_u32_be(control + PPC_CONTROL_DATA_OFFSET)
        .unwrap();
    let data = loaded.memory.read_u32_be(data_handle).unwrap();
    let menu_handle = loaded.memory.read_u32_be(data).unwrap();
    assert_ne!(menu_handle, 0);
    let menu = loaded.memory.read_u32_be(menu_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(menu), Some(143));
    assert_eq!(loaded.memory.read_u16_be(data + 4), Some(143));
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
fn checkbox_title_stops_at_control_right_edge() {
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
            (10, 20, 30, 70),
            b"Balloon Help",
            true,
            0,
            0,
            1,
            1,
            0,
        )
    );
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 40, 160),
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
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    for y in 10..30 {
        for x in 70..120 {
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                Some(white),
                "checkbox title painted outside its right edge at ({x}, {y})"
            );
        }
    }
}

#[test]
fn compact_custom_control_does_not_paint_its_metadata_title_beside_it() {
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
            (10, 20, 26, 36),
            b"Custom Control",
            true,
            0,
            0,
            1,
            3000,
            0,
        )
    );
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 40, 160),
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
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert!(
        (12..24).any(|y| {
            (22..34).any(|x| {
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)) != Some(white)
            })
        }),
        "compact custom control should show a placeholder glyph"
    );
    for y in 10..26 {
        for x in 36..120 {
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                Some(white),
                "custom control title painted outside its bounds at ({x}, {y})"
            );
        }
    }
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
fn draw_controls_draws_newest_first_with_oldest_control_frontmost() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"DrawControls")).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let mut handles = Vec::new();
    for bounds in [(10, 20, 50, 120), (20, 40, 60, 140)] {
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
                bounds,
                b"",
                true,
                0,
                0,
                1,
                0,
                0,
            )
        );
        assert_ne!(handle, 0);
        handles.push(handle);
    }
    let newer_control = ppc_control_ptr(&mut loaded.memory, handles[1]).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
        Some(handles[1])
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(newer_control + PPC_CONTROL_NEXT_OFFSET),
        Some(handles[0])
    );
    assert!(ppc_paint_rect_bounds(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
        (0, 0, 80, 160),
        PPC_RGB_WHITE,
        None,
    ));

    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (60, 20)),
        Some(white),
        "the oldest button's interior must cover the newer button's top edge"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (119, 35)),
        Some(black),
        "the oldest button's right edge must cover the newer button's interior"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (139, 40)),
        Some(black),
        "the newer button must still draw outside the overlap"
    );
}

#[test]
fn move_control_redraws_at_new_bounds_only_when_visible() {
    for visible in [true, false] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"MoveControl")).unwrap();
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
                b"",
                visible,
                0,
                0,
                1,
                0,
                0,
            )
        );
        assert_ne!(handle, 0);
        let control = ppc_control_ptr(&mut loaded.memory, handle).unwrap();
        assert!(ppc_paint_rect_bounds(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (0, 0, 100, 160),
            PPC_RGB_WHITE,
            None,
        ));
        // An interior marker distinguishes a hidden no-op from repainting
        // the button's white fill at its new location.
        assert!(ppc_paint_rect_bounds(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
            (70, 90, 71, 91),
            PPC_RGB_BLACK,
            None,
        ));

        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 50;
        loaded.cpu.gpr[5] = 60;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            ppc_read_rect(&mut loaded.memory, control + PPC_CONTROL_RECT_OFFSET),
            Some((60, 50, 80, 130)),
            "MoveControl must preserve dimensions, visible={visible}"
        );
        assert_eq!(
            loaded.memory.read_u8(control + PPC_CONTROL_VISIBLE_OFFSET),
            Some(if visible { 0xff } else { 0 })
        );
        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap();
        let black =
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
        let white =
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (90, 60)),
            Some(if visible { black } else { white }),
            "only a visible control should paint its new frame, visible={visible}"
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (90, 70)),
            Some(if visible { white } else { black }),
            "only a visible control should repaint its new interior, visible={visible}"
        );
    }
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
            let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
            assert_eq!(
                tracking.popup_left, 6 + 52,
                "popup must anchor after the control title"
            );
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
        ("GetControlAction", PpcLegacyControlOperation::GetControlAction),
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
        ("SetControlAction", PpcLegacyControlOperation::SetControlAction),
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
fn control_action_imports_update_and_read_the_classic_control_record() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SetControlAction")).unwrap();
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
    loaded.cpu.gpr[4] = 0x0012_3450;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(record + PPC_CONTROL_ACTION_OFFSET),
        Some(0x0012_3450)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlAction);
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0012_3450);
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

fn appearance_push_button(loaded: &mut PpcLoadedApp, proc_id: i16) -> u32 {
    let mut last_mem_error = loaded.last_mem_error();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        32,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 5, 6, 65, 186).unwrap();
    write_ppc_pstring(&mut loaded.memory, scratch + 8, b"Group");
    with_test_controls!(
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
            proc_id,
            0,
        )
    )
}

fn run_appearance_import(
    loaded: &mut PpcLoadedApp,
    target: &PpcImportDispatcherTarget,
    gprs: &[u32],
) -> u32 {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target.clone();
    for (index, value) in gprs.iter().enumerate() {
        loaded.cpu.gpr[3 + index] = *value;
    }
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None, "{target:?}");
    loaded.cpu.gpr[3]
}

#[test]
fn appearance_imports_map_to_typed_targets() {
    for (symbol, expected) in [
        ("RegisterAppearanceClient", PpcImportDispatcherTarget::RegisterAppearanceClient),
        ("UnregisterAppearanceClient", PpcImportDispatcherTarget::UnregisterAppearanceClient),
        ("ActivateControl", PpcImportDispatcherTarget::ActivateControl),
        ("DeactivateControl", PpcImportDispatcherTarget::DeactivateControl),
        ("IsControlActive", PpcImportDispatcherTarget::IsControlActive),
        ("SetControlFontStyle", PpcImportDispatcherTarget::SetControlFontStyle),
        ("CollapseWindow", PpcImportDispatcherTarget::CollapseWindow),
        ("IsWindowCollapsed", PpcImportDispatcherTarget::IsWindowCollapsed),
    ] {
        assert_eq!(dispatcher_target_for_import("AppearanceLib", symbol), expected, "{symbol}");
    }
}

#[test]
fn carbon_appearance_client_imports_bind_and_dispatch() {
    for (symbol, target) in [
        (
            b"RegisterAppearanceClient".as_slice(),
            PpcImportDispatcherTarget::RegisterAppearanceClient,
        ),
        (
            b"UnregisterAppearanceClient".as_slice(),
            PpcImportDispatcherTarget::UnregisterAppearanceClient,
        ),
    ] {
        let weak_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"CarbonLib",
            symbol,
            0x82,
            &[sm_index_reloc(0x30, 0)],
        ));
        let mut weak_loaded = load_pef_application(&weak_pef).unwrap();
        assert_eq!(weak_loaded.imports[0].dispatcher_target, target);
        assert_ne!(weak_loaded.imports[0].address, 0);
        assert_eq!(
            weak_loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(weak_loaded.imports[0].address)
        );

        let pef = synthetic_pef_with_library_import(b"CarbonLib", symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    }
}

#[test]
fn appearance_client_and_collapse_imports_report_their_results() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"RegisterAppearanceClient")).unwrap();
    for target in [
        PpcImportDispatcherTarget::RegisterAppearanceClient,
        PpcImportDispatcherTarget::UnregisterAppearanceClient,
    ] {
        assert_eq!(run_appearance_import(&mut loaded, &target, &[]), ppc_i16_result(PPC_NO_ERR));
    }
    let collapse = PpcImportDispatcherTarget::CollapseWindow;
    assert_eq!(
        run_appearance_import(&mut loaded, &collapse, &[PPC_MAIN_GWORLD, 0]),
        ppc_i16_result(PPC_NO_ERR)
    );
    // There is no collapsed-window representation, so collapsing is
    // refused with unimpErr rather than reported as done.
    assert_eq!(
        run_appearance_import(&mut loaded, &collapse, &[PPC_MAIN_GWORLD, 1]),
        ppc_i16_result(-4)
    );
    assert_eq!(
        run_appearance_import(&mut loaded, &collapse, &[0, 0]),
        ppc_i16_result(PPC_PARAM_ERR)
    );
    assert_eq!(
        run_appearance_import(
            &mut loaded,
            &PpcImportDispatcherTarget::IsWindowCollapsed,
            &[PPC_MAIN_GWORLD]
        ),
        0
    );
}

#[test]
fn deactivated_controls_report_inactive_and_cannot_be_hit() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TestControl")).unwrap();
    let handle = appearance_push_button(&mut loaded, 0);
    let test = PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TestControl);
    let point = (20 << 16) | 20;
    assert_eq!(run_appearance_import(&mut loaded, &test, &[handle, point]), 10);

    assert_eq!(
        run_appearance_import(&mut loaded, &PpcImportDispatcherTarget::DeactivateControl, &[handle]),
        ppc_i16_result(PPC_NO_ERR)
    );
    assert_eq!(
        run_appearance_import(&mut loaded, &PpcImportDispatcherTarget::IsControlActive, &[handle]),
        0
    );
    assert_eq!(run_appearance_import(&mut loaded, &test, &[handle, point]), 0);
    let control = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(
        loaded.memory.read_u8(control + PPC_CONTROL_HILITE_OFFSET),
        Some(0),
        "deactivation must not touch contrlHilite"
    );

    assert_eq!(
        run_appearance_import(&mut loaded, &PpcImportDispatcherTarget::ActivateControl, &[handle]),
        ppc_i16_result(PPC_NO_ERR)
    );
    assert_eq!(
        run_appearance_import(&mut loaded, &PpcImportDispatcherTarget::IsControlActive, &[handle]),
        1
    );
    assert_eq!(run_appearance_import(&mut loaded, &test, &[handle, point]), 10);

    // A handle the Control Manager never created is rejected.
    assert_eq!(
        run_appearance_import(
            &mut loaded,
            &PpcImportDispatcherTarget::DeactivateControl,
            &[PPC_MAIN_GWORLD]
        ),
        ppc_i16_result(PPC_PARAM_ERR)
    );
}

#[test]
fn group_boxes_report_no_part_except_for_interactive_titles() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TestControl")).unwrap();
    let text_title = appearance_push_button(&mut loaded, 160);
    let checkbox_title = appearance_push_button(&mut loaded, 161);
    let records = loaded.controls.records();
    // Rect is (5, 6, 65, 186); the title band is the top 10 pixels.
    for (handle, title_part) in [(text_title, 0), (checkbox_title, 10)] {
        assert_eq!(
            ppc_control_part_at_point(&mut loaded.memory, &records, handle, 8, 30),
            Some(title_part)
        );
        assert_eq!(
            ppc_control_part_at_point(&mut loaded.memory, &records, handle, 40, 30),
            Some(0)
        );
    }
}

#[test]
fn every_control_creation_path_registers_a_hittable_record() {
    // Dialog hit testing ignores control items without a live record, so
    // a creation path that skips registration would make its control dead.
    // NewControl and DITL controls are pinned by their own tests; this
    // covers GetNewControl and the List Manager's scroll bars.
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetNewControl")).unwrap();
    let mut cntl = vec![0; 23];
    for (index, value) in [10i16, 10, 30, 90].into_iter().enumerate() {
        cntl[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
    }
    cntl[10] = 1;
    cntl[12..14].copy_from_slice(&1i16.to_be_bytes());
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: String::new(),
        res_type: u32::from_be_bytes(*b"CNTL"),
        res_id: 128,
        name: Vec::new(),
        data: cntl,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 128;
    loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(
        ppc_control_part_at_point(&mut loaded.memory, &loaded.controls.records(), handle, 20, 50),
        Some(10)
    );

    let mut list_app = load_pef_application(&synthetic_pef_with_import(b"LNew")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    list_app.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut list_app.memory, scratch, 10, 20, 90, 220).unwrap();
    ppc_write_rect(&mut list_app.memory, scratch + 8, 0, 0, 20, 1).unwrap();
    list_app.cpu.gpr[3] = scratch;
    list_app.cpu.gpr[4] = scratch + 8;
    list_app.cpu.gpr[5] = (16u32 << 16) | 200;
    list_app.cpu.gpr[6] = 0;
    list_app.cpu.gpr[7] = PPC_MAIN_GWORLD;
    // drawIt: scroll bars stay hidden until drawing is on.
    list_app.cpu.gpr[8] = 1;
    list_app.cpu.gpr[9] = 0;
    list_app.cpu.gpr[10] = 0;
    list_app
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(list_app.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            1,
        )
        .unwrap();
    let probe = list_app.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let list_ptr = list_app.memory.read_u32_be(list_app.cpu.gpr[3]).unwrap();
    let scroll = list_app
        .memory
        .read_u32_be(list_ptr + PPC_LIST_VSCROLL_OFFSET)
        .unwrap();
    assert_ne!(scroll, 0);
    let scroll_ptr = list_app.memory.read_u32_be(scroll).unwrap();
    let (top, left, bottom, right) =
        ppc_read_rect(&mut list_app.memory, scroll_ptr + PPC_CONTROL_RECT_OFFSET).unwrap();
    assert!(ppc_control_part_at_point(
        &mut list_app.memory,
        &list_app.controls.records(),
        scroll,
        (top + bottom) / 2,
        (left + right) / 2,
    )
    .is_some_and(|part| part != 0));
}

#[test]
fn set_control_font_style_stores_the_style_the_title_painter_uses() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SetControlFontStyle")).unwrap();
    let handle = appearance_push_button(&mut loaded, 0);
    let style_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(style_ptr, vec![0; 24]);
    // kControlUseFaceMask | kControlUseForeColorMask | kControlAddFontSizeMask,
    // without kControlUseSizeMask: size is a +2 delta, bold, red ink. The ink
    // applies only to static text controls.
    let mut rec = [0u8; 24];
    rec[0..2].copy_from_slice(&0x010au16.to_be_bytes());
    rec[4..6].copy_from_slice(&2i16.to_be_bytes());
    rec[6..8].copy_from_slice(&1i16.to_be_bytes());
    rec[12..14].copy_from_slice(&0xffffu16.to_be_bytes());
    loaded.memory.write_bytes(style_ptr, &rec).unwrap();

    let set = PpcImportDispatcherTarget::SetControlFontStyle;
    assert_eq!(
        run_appearance_import(&mut loaded, &set, &[handle, style_ptr]),
        ppc_i16_result(PPC_NO_ERR)
    );
    let records = loaded.controls.records();
    let style = records[0].font_style.expect("style stored");
    let push_button = ppc_control_title_style(records[0].proc_id, Some(&style));
    assert_eq!(
        push_button,
        PpcControlTitleStyle {
            font: PPC_QD_TEXT_FONT_DEFAULT,
            size: 14,
            face: 1,
            foreground: None,
        }
    );
    // kControlStaticTextProc.
    assert_eq!(
        ppc_control_title_style(288, Some(&style)),
        PpcControlTitleStyle {
            foreground: Some(PpcRgbColor { red: 0xffff, green: 0, blue: 0 }),
            ..push_button
        }
    );
    assert_eq!(
        run_appearance_import(&mut loaded, &set, &[PPC_MAIN_GWORLD, style_ptr]),
        ppc_i16_result(PPC_PARAM_ERR)
    );

    // Clearing the flags drops the override.
    loaded.memory.write_bytes(style_ptr, &[0; 24]).unwrap();
    run_appearance_import(&mut loaded, &set, &[handle, style_ptr]);
    assert_eq!(loaded.controls.records()[0].font_style, None);
}

#[test]
fn control_title_style_resolves_appearance_meta_fonts() {
    let style = |flags: u16, font: i16, size: i16| crate::control_manager::ControlFontStyle {
        flags: flags as i16,
        font,
        size,
        style: 0,
        mode: 0,
        justification: 0,
        foreground: [0; 3],
        background: [0; 3],
    };
    let resolve = |flags, font, size| {
        let resolved = ppc_control_title_style(0, Some(&style(flags, font, size)));
        (resolved.font, resolved.size, resolved.face)
    };
    assert_eq!(ppc_control_title_style(0, None).font, PPC_QD_TEXT_FONT_DEFAULT);
    // kControlFontBigSystemFont .. kControlFontViewSystemFont.
    assert_eq!(resolve(0x0001, -1, 0), (PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM, 0));
    assert_eq!(resolve(0x0001, -2, 0), (3, 10, 0));
    assert_eq!(resolve(0x0001, -3, 0), (3, 10, 1));
    assert_eq!(resolve(0x0001, -4, 0), (3, 10, 0));
    // Unknown guest meta-font IDs retain the default, including the signed
    // minimum whose negation cannot be represented by an i16.
    assert_eq!(resolve(0x0001, i16::MIN, 0),
        (PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM, 0));
    assert_eq!(resolve(0x0001, -5, 0),
        (PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM, 0));
    // Theme font IDs under kControlUseThemeFontIDMask.
    assert_eq!(resolve(0x0081, 2, 0), (3, 10, 1));
    // A plain family ID with an absolute size, and a delta on a meta font.
    assert_eq!(resolve(0x0005, 21, 18), (21, 18, 0));
    assert_eq!(resolve(0x0105, -2, -1), (3, 9, 0));
}

#[test]
fn import_bindings_classify_control_creation_and_disposal_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // NewControl
        assert_eq!(
            dispatcher_target_for_import(lib, "NewControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::NewControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "newcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::NewControl)
        );

        // GetNewControl
        assert_eq!(
            dispatcher_target_for_import(lib, "GetNewControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetNewControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getnewcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetNewControl)
        );

        // DisposeControl / DisposControl
        assert_eq!(
            dispatcher_target_for_import(lib, "DisposeControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "disposecontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "DisposControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "disposcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl)
        );

        // KillControls
        assert_eq!(
            dispatcher_target_for_import(lib, "KillControls"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::KillControls)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "killcontrols"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::KillControls)
        );
    }
}

#[test]
fn control_creation_and_disposal_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        // 1. NewControl & DisposeControl
        {
            let pef = synthetic_pef_with_library_import(lib, b"NewControl");
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(scratch, vec![0; 64]);
            ppc_write_rect(&mut loaded.memory, scratch, 10, 20, 40, 140).unwrap();
            write_ppc_pstring(&mut loaded.memory, scratch + 8, b"Button");
            let ref_con_slot =
                ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                    .unwrap();
            loaded
                .memory
                .write_u32_be(ref_con_slot, 0x1234_5678)
                .unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "NewControl");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
            loaded.cpu.gpr[4] = scratch;
            loaded.cpu.gpr[5] = scratch + 8;
            loaded.cpu.gpr[6] = 1;
            loaded.cpu.gpr[7] = 3;
            loaded.cpu.gpr[8] = 1;
            loaded.cpu.gpr[9] = 9;
            loaded.cpu.gpr[10] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            let handle = loaded.cpu.gpr[3];
            assert_ne!(handle, 0);
            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
                Some(handle)
            );
            assert!(loaded.controls.records().iter().any(|c| c.handle == handle));

            // Now test DisposeControl on this control
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DisposeControl");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
                Some(0)
            );
            assert!(!loaded.controls.records().iter().any(|c| c.handle == handle));
        }

        // 2. GetNewControl
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetNewControl");
            let mut loaded = load_pef_application(&pef).unwrap();
            let mut cntl = vec![0; 23];
            for (index, value) in [10i16, 10, 30, 90].into_iter().enumerate() {
                cntl[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
            }
            cntl[10] = 1; // visible
            cntl[12..14].copy_from_slice(&100i16.to_be_bytes()); // max
            cntl[14..16].copy_from_slice(&0i16.to_be_bytes()); // min
            cntl[18..22].copy_from_slice(&0xABCD_EF01u32.to_be_bytes()); // refCon
            let current_resource_refnum = *loaded.process_file_system.current_resource_file;
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(*b"CNTL"),
                res_id: 200,
                name: Vec::new(),
                data: cntl,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetNewControl");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 200;
            loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            let handle = loaded.cpu.gpr[3];
            assert_ne!(handle, 0);
            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
                Some(handle)
            );
            assert!(loaded.controls.records().iter().any(|c| c.handle == handle));
        }

        // 3. KillControls
        {
            let pef = synthetic_pef_with_library_import(lib, b"KillControls");
            let mut loaded = load_pef_application(&pef).unwrap();
            let scratch1 = PPC_DATA_BASE + 0x1000;
            let scratch2 = PPC_DATA_BASE + 0x1100;
            loaded.memory.add_region(scratch1, vec![0; 64]);
            loaded.memory.add_region(scratch2, vec![0; 64]);
            ppc_write_rect(&mut loaded.memory, scratch1, 10, 20, 40, 140).unwrap();
            ppc_write_rect(&mut loaded.memory, scratch2, 50, 20, 80, 140).unwrap();

            let ref_con_slot =
                ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                    .unwrap();
            loaded.memory.write_u32_be(ref_con_slot, 0).unwrap();

            // Create first control
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "NewControl");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
            loaded.cpu.gpr[4] = scratch1;
            loaded.cpu.gpr[5] = scratch1 + 8;
            loaded.cpu.gpr[6] = 1;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            loaded.cpu.gpr[9] = 1;
            loaded.cpu.gpr[10] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            let handle1 = loaded.cpu.gpr[3];
            assert_ne!(handle1, 0);

            // Create second control
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
            loaded.cpu.gpr[4] = scratch2;
            loaded.cpu.gpr[5] = scratch2 + 8;
            loaded.cpu.gpr[6] = 1;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            loaded.cpu.gpr[9] = 1;
            loaded.cpu.gpr[10] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            let handle2 = loaded.cpu.gpr[3];
            assert_ne!(handle2, 0);

            assert_ne!(
                loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
                Some(0)
            );

            // Now KillControls
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "KillControls");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            assert_eq!(
                loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET),
                Some(0)
            );
            assert!(!loaded.controls.records().iter().any(|c| c.handle == handle1 || c.handle == handle2));
        }
    }
}

#[test]
fn import_bindings_classify_control_display_and_geometry_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // ShowControl
        assert_eq!(
            dispatcher_target_for_import(lib, "ShowControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ShowControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "showcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ShowControl)
        );

        // HideControl
        assert_eq!(
            dispatcher_target_for_import(lib, "HideControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::HideControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "hidecontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::HideControl)
        );

        // Draw1Control / DrawOneControl
        assert_eq!(
            dispatcher_target_for_import(lib, "Draw1Control"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawOneControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "draw1control"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawOneControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "DrawOneControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawOneControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "drawonecontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawOneControl)
        );

        // DrawControls
        assert_eq!(
            dispatcher_target_for_import(lib, "DrawControls"),
            PpcImportDispatcherTarget::DrawControls
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "drawcontrols"),
            PpcImportDispatcherTarget::DrawControls
        );

        // UpdateControls
        assert_eq!(
            dispatcher_target_for_import(lib, "UpdateControls"),
            PpcImportDispatcherTarget::UpdateControls
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "updatecontrols"),
            PpcImportDispatcherTarget::UpdateControls
        );

        // MoveControl
        assert_eq!(
            dispatcher_target_for_import(lib, "MoveControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::MoveControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "movecontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::MoveControl)
        );

        // SizeControl
        assert_eq!(
            dispatcher_target_for_import(lib, "SizeControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SizeControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "sizecontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SizeControl)
        );
    }
}

#[test]
fn control_display_and_geometry_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"ShowControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

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
                (10, 20, 40, 140),
                b"TestBtn",
                true,
                0,
                0,
                1,
                0,
                0,
            )
        );
        assert_ne!(handle, 0);
        let ctrl_ptr = loaded.memory.read_u32_be(handle).unwrap();
        assert_ne!(ctrl_ptr, 0);

        // 1. HideControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "HideControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.memory.read_u8(ctrl_ptr + PPC_CONTROL_VISIBLE_OFFSET),
            Some(0)
        );

        // 2. ShowControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ShowControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.memory.read_u8(ctrl_ptr + PPC_CONTROL_VISIBLE_OFFSET),
            Some(0xff)
        );

        // 3. MoveControl(theControl, h, v)
        // contrlRect initially: top=10, left=20, bottom=40, right=140. Width = 120, Height = 30.
        // Move to h=50, v=60 -> top=60, left=50, bottom=90, right=170.
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "MoveControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 50;
        loaded.cpu.gpr[5] = 60;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            ppc_read_rect(&mut loaded.memory, ctrl_ptr + PPC_CONTROL_RECT_OFFSET),
            Some((60, 50, 90, 170))
        );

        // 4. SizeControl(theControl, w, h)
        // Size to w=80, h=50 -> top=60, left=50, bottom=110, right=130.
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SizeControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 80;
        loaded.cpu.gpr[5] = 50;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            ppc_read_rect(&mut loaded.memory, ctrl_ptr + PPC_CONTROL_RECT_OFFSET),
            Some((60, 50, 110, 130))
        );

        // 5. Draw1Control(theControl)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "Draw1Control");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // 6. DrawControls(theWindow)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "DrawControls");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // 7. UpdateControls(theWindow, updateRgn)
        let update_rgn = ppc_new_rgn(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
        );
        assert_ne!(update_rgn, 0);
        ppc_write_rgn_bbox(&mut loaded.memory, update_rgn, 50, 40, 120, 140).unwrap();

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "UpdateControls");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = update_rgn;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn import_bindings_classify_control_values_ranges_and_state_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // SetControlValue / SetCtlValue
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlValue"),
            PpcImportDispatcherTarget::SetControlValue
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolvalue"),
            PpcImportDispatcherTarget::SetControlValue
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetCtlValue"),
            PpcImportDispatcherTarget::SetControlValue
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setctlvalue"),
            PpcImportDispatcherTarget::SetControlValue
        );

        // GetControlValue / GetCtlValue
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlValue"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlValue)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolvalue"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlValue)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCtlValue"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlValue)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getctlvalue"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlValue)
        );

        // SetControlMinimum / SetCtlMin
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlMinimum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolminimum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetCtlMin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setctlmin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlMin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolmin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum)
        );

        // GetControlMinimum / GetCtlMin
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlMinimum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolminimum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCtlMin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getctlmin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlMin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolmin"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum)
        );

        // SetControlMaximum / SetCtlMax
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlMaximum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolmaximum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetCtlMax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setctlmax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlMax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolmax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum)
        );

        // GetControlMaximum / GetCtlMax
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlMaximum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolmaximum"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCtlMax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getctlmax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlMax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolmax"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum)
        );

        // HiliteControl
        assert_eq!(
            dispatcher_target_for_import(lib, "HiliteControl"),
            PpcImportDispatcherTarget::HiliteControl
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "hilitecontrol"),
            PpcImportDispatcherTarget::HiliteControl
        );

        // SetControlAction
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlAction"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlAction)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolaction"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlAction)
        );

        // GetControlAction
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlAction"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlAction)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolaction"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlAction)
        );

        // SetControlReference / SetCRefCon
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlReference"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontrolreference"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetCRefCon"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcrefcon"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlReference)
        );

        // GetControlReference / GetCRefCon
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlReference"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolreference"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCRefCon"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlReference)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcrefcon"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlReference)
        );

        // SetControlTitle / SetCTitle
        assert_eq!(
            dispatcher_target_for_import(lib, "SetControlTitle"),
            PpcImportDispatcherTarget::SetControlTitle
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setcontroltitle"),
            PpcImportDispatcherTarget::SetControlTitle
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetCTitle"),
            PpcImportDispatcherTarget::SetControlTitle
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setctitle"),
            PpcImportDispatcherTarget::SetControlTitle
        );

        // GetControlTitle / GetCTitle
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlTitle"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontroltitle"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCTitle"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getctitle"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlTitle)
        );
    }
}

#[test]
fn control_values_ranges_and_state_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"SetControlValue");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

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
                (10, 20, 40, 140),
                b"OldTitle",
                true,
                5,
                0,
                10,
                1,
                0x1111_2222,
            )
        );
        assert_ne!(handle, 0);
        let ctrl_ptr = loaded.memory.read_u32_be(handle).unwrap();
        assert_ne!(ctrl_ptr, 0);

        // 1. SetControlMinimum(handle, 2)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlMinimum");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlMinimum(handle) -> 2
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlMinimum");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 2);

        // 2. SetControlMaximum(handle, 20)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlMaximum");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 20;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlMaximum(handle) -> 20
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlMaximum");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 20);

        // 3. SetControlValue(handle, 15)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlValue");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 15;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlValue(handle) -> 15
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlValue");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 15);

        // 4. SetControlReference(handle, 0xcafe_babe)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlReference");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 0xcafe_babe;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlReference(handle) -> 0xcafe_babe
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlReference");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0xcafe_babe);

        // 5. SetControlAction(handle, 0x5555_6666)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlAction");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 0x5555_6666;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlAction(handle) -> 0x5555_6666
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlAction");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x5555_6666);

        // 6. SetControlTitle(handle, "NewTitle")
        let title_in = PPC_DATA_BASE + 0x2000;
        let title_out = PPC_DATA_BASE + 0x2100;
        loaded.memory.add_region(title_in, vec![0; 64]);
        loaded.memory.add_region(title_out, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, title_in, b"NewTitle");

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = title_in;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // GetControlTitle(handle, title_out)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = title_out;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, title_out),
            Some(b"NewTitle".to_vec())
        );

        // 7. HiliteControl(handle, 255)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "HiliteControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 255;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(ctrl_ptr + 17), Some(255));
    }
}

#[test]
fn import_bindings_classify_control_hit_testing_and_tracking_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // FindControl
        assert_eq!(
            dispatcher_target_for_import(lib, "FindControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::FindControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "findcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::FindControl)
        );

        // TestControl
        assert_eq!(
            dispatcher_target_for_import(lib, "TestControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TestControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "testcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TestControl)
        );

        // TrackControl
        assert_eq!(
            dispatcher_target_for_import(lib, "TrackControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TrackControl)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "trackcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TrackControl)
        );

        // GetControlVariant / GetCVariant
        assert_eq!(
            dispatcher_target_for_import(lib, "GetControlVariant"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlVariant)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcontrolvariant"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlVariant)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetCVariant"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlVariant)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getcvariant"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlVariant)
        );
    }
}

#[test]
fn control_hit_testing_and_tracking_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"FindControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

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
                (10, 20, 40, 140),
                b"HitTestCtl",
                true,
                0,
                0,
                100,
                0x0033,
                0x9999_8888,
            )
        );
        assert_ne!(handle, 0);

        // 1. GetControlVariant(handle) -> 3 (since proc_id 0x0033 & 0x0F == 3)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlVariant");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 3);

        // 2. TestControl(handle, point)
        // Hit inside bounds (v=15, h=25) -> part 10
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "TestControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = (15 << 16) | 25;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 10);

        // Miss outside bounds (v=5, h=5) -> part 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = (5 << 16) | 5;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // 3. FindControl(point, window, &out_ctl)
        let out_ctl_ptr = PPC_DATA_BASE + 0x3000;
        loaded.memory.add_region(out_ctl_ptr, vec![0; 16]);

        // Hit inside bounds
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "FindControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = (15 << 16) | 25;
        loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[5] = out_ctl_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 10);
        assert_eq!(loaded.memory.read_u32_be(out_ctl_ptr), Some(handle));

        // Miss outside bounds
        loaded.memory.write_u32_be(out_ctl_ptr, 0).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = (5 << 16) | 5;
        loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[5] = out_ctl_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ctl_ptr), Some(0));

        // 4. TrackControl(handle, start_point, action_proc)
        // Hit with nil action_proc (0) -> part 10
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "TrackControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = (15 << 16) | 25;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 10);

        // Miss with nil action_proc (0) -> part 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = (5 << 16) | 5;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
    }
}

#[test]
fn import_bindings_classify_control_hierarchy_and_property_imports() {
    for library in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        assert_eq!(
            dispatcher_target_for_import(library, "AutoEmbedControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AutoEmbedControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "autoembedcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AutoEmbedControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "ChangeControlPropertyAttributes"),
            PpcImportDispatcherTarget::LegacyControl(
                PpcLegacyControlOperation::ChangeControlPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(library, "changecontrolpropertyattributes"),
            PpcImportDispatcherTarget::LegacyControl(
                PpcLegacyControlOperation::ChangeControlPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(library, "CountSubControls"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CountSubControls)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "countsubcontrols"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CountSubControls)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "CreateRootControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CreateRootControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "createrootcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CreateRootControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "EmbedControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EmbedControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "embedcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EmbedControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetControlProperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getcontrolproperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetControlPropertyAttributes"),
            PpcImportDispatcherTarget::LegacyControl(
                PpcLegacyControlOperation::GetControlPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getcontrolpropertyattributes"),
            PpcImportDispatcherTarget::LegacyControl(
                PpcLegacyControlOperation::GetControlPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetControlPropertySize"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlPropertySize)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getcontrolpropertysize"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlPropertySize)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetIndexedSubControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetIndexedSubControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getindexedsubcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetIndexedSubControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetRootControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetRootControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getrootcontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetRootControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "GetSuperControl"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetSuperControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "getsupercontrol"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetSuperControl)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "RemoveControlProperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::RemoveControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "removecontrolproperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::RemoveControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "SetControlProperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "setcontrolproperty"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "SetControlSupervisor"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlSupervisor)
        );
        assert_eq!(
            dispatcher_target_for_import(library, "setcontrolsupervisor"),
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlSupervisor)
        );
    }
}

#[test]
fn control_hierarchy_and_property_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"CreateRootControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

        let out_ref_ptr = PPC_DATA_BASE + 0x3000;
        let out_attr_ptr = PPC_DATA_BASE + 0x3010;
        let out_size_ptr = PPC_DATA_BASE + 0x3014;
        let out_data_buf = PPC_DATA_BASE + 0x3020;
        let in_data_buf = PPC_DATA_BASE + 0x3040;
        let bounds1_ptr = PPC_DATA_BASE + 0x3060;
        let bounds2_ptr = PPC_DATA_BASE + 0x3070;
        let title_ptr = PPC_DATA_BASE + 0x3080;
        loaded.memory.add_region(out_ref_ptr, vec![0; 0x1000]);
        let window = PPC_MAIN_GWORLD;

        // 1. CreateRootControl(window, out_ref_ptr) -> creates root control
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "CreateRootControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        let root_handle = loaded.memory.read_u32_be(out_ref_ptr).unwrap();
        assert_ne!(root_handle, 0);

        // 2. CreateRootControl again -> returns -30587 (errRootAlreadyExists)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -30587);
        assert_eq!(loaded.memory.read_u32_be(out_ref_ptr), Some(root_handle));

        // 3. GetRootControl(window, out_ref_ptr) -> returns 0 and root_handle
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetRootControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ref_ptr), Some(root_handle));

        // Create child controls 1 and 2
        ppc_write_rect(&mut loaded.memory, bounds1_ptr, 10, 10, 50, 100);
        ppc_write_rect(&mut loaded.memory, bounds2_ptr, 60, 10, 100, 100);
        ppc_write_pstring_bytes(&mut loaded.memory, title_ptr, b"Item");

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "NewControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = bounds1_ptr;
        loaded.cpu.gpr[5] = title_ptr;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 100;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let child1 = loaded.cpu.gpr[3];
        assert_ne!(child1, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = bounds2_ptr;
        loaded.cpu.gpr[5] = title_ptr;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 100;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let child2 = loaded.cpu.gpr[3];
        assert_ne!(child2, 0);

        // 4. CountSubControls(root_handle, out_size_ptr) -> initially 0
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "CountSubControls");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = root_handle;
        loaded.cpu.gpr[4] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u16_be(out_size_ptr), Some(0));

        // 5. EmbedControl(child1, root_handle) -> embeds child1 into root
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "EmbedControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = root_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 6. EmbedControl(child1, child1) -> returns -30594 (errCantEmbedIntoSelf)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = child1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -30594);

        // 7. AutoEmbedControl(child2, window) -> automatically embeds into root
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "AutoEmbedControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child2;
        loaded.cpu.gpr[4] = window;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 8. CountSubControls(root_handle) -> now 2
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "CountSubControls");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = root_handle;
        loaded.cpu.gpr[4] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u16_be(out_size_ptr), Some(2));

        // 9. GetIndexedSubControl(root, 1) -> child1, GetIndexedSubControl(root, 2) -> child2
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetIndexedSubControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = root_handle;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ref_ptr), Some(child1));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = root_handle;
        loaded.cpu.gpr[4] = 2;
        loaded.cpu.gpr[5] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ref_ptr), Some(child2));

        // Out of range index 3 -> -30590 (errControlIsNotEmbedder)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = root_handle;
        loaded.cpu.gpr[4] = 3;
        loaded.cpu.gpr[5] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -30590);

        // 10. GetSuperControl(child1) -> root_handle
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetSuperControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ref_ptr), Some(root_handle));

        // 11. SetControlSupervisor(child1, 0) -> unparent child1
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlSupervisor");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // Child1 has no parent now -> GetSuperControl returns -30592
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetSuperControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = out_ref_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -30592);

        // 12. SetControlProperty(child1, 'TEST', 'TAG1', 4, data)
        let prop_creator = 0x5445_5354; // 'TEST'
        let prop_tag = 0x5441_4731;     // 'TAG1'
        let _ = loaded.memory.write_bytes(in_data_buf, &[0xDE, 0xAD, 0xBE, 0xEF]);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = 4;
        loaded.cpu.gpr[7] = in_data_buf;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 13. GetControlPropertySize(child1, 'TEST', 'TAG1', out_size_ptr) -> 4
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlPropertySize");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));

        // 14. GetControlProperty(child1, 'TEST', 'TAG1', 16, out_size_ptr, out_data_buf) -> data
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = out_data_buf;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, out_data_buf, 4),
            Some(vec![0xDE, 0xAD, 0xBE, 0xEF])
        );

        // 15. ChangeControlPropertyAttributes(child1, 'TEST', 'TAG1', 0x20, 0)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ChangeControlPropertyAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = 0x20;
        loaded.cpu.gpr[7] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 16. GetControlPropertyAttributes(child1, 'TEST', 'TAG1', out_attr_ptr) -> 0x20
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlPropertyAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = out_attr_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_attr_ptr), Some(0x20));

        // 17. RemoveControlProperty(child1, 'TEST', 'TAG1')
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RemoveControlProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 18. GetControlProperty after removal -> -5604 (controlPropertyNotFoundErr)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = child1;
        loaded.cpu.gpr[4] = prop_creator;
        loaded.cpu.gpr[5] = prop_tag;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = out_data_buf;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -5604);
    }
}

#[test]
fn import_bindings_classify_control_activation_styling_and_bounds_imports() {
    for library in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        for (symbol, expected) in [
            ("ActivateControl", PpcImportDispatcherTarget::ActivateControl),
            ("activatecontrol", PpcImportDispatcherTarget::ActivateControl),
            ("DeactivateControl", PpcImportDispatcherTarget::DeactivateControl),
            ("deactivatecontrol", PpcImportDispatcherTarget::DeactivateControl),
            ("IsControlActive", PpcImportDispatcherTarget::IsControlActive),
            ("iscontrolactive", PpcImportDispatcherTarget::IsControlActive),
            ("SetControlFontStyle", PpcImportDispatcherTarget::SetControlFontStyle),
            ("setcontrolfontstyle", PpcImportDispatcherTarget::SetControlFontStyle),
            (
                "IsControlVisible",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlVisible),
            ),
            (
                "iscontrolvisible",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlVisible),
            ),
            (
                "IsControlEnabled",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlEnabled),
            ),
            (
                "iscontrolenabled",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlEnabled),
            ),
            (
                "EnableControl",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EnableControl),
            ),
            (
                "enablecontrol",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EnableControl),
            ),
            (
                "DisableControl",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisableControl),
            ),
            (
                "disablecontrol",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisableControl),
            ),
            (
                "IsControlHilited",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlHilited),
            ),
            (
                "iscontrolhilited",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlHilited),
            ),
            (
                "GetControlHilite",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlHilite),
            ),
            (
                "getcontrolhilite",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlHilite),
            ),
            (
                "IsValidControlHandle",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsValidControlHandle),
            ),
            (
                "isvalidcontrolhandle",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsValidControlHandle),
            ),
            (
                "IsValidControlRef",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsValidControlHandle),
            ),
            (
                "isvalidcontrolref",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsValidControlHandle),
            ),
            (
                "GetControlBounds",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlBounds),
            ),
            (
                "getcontrolbounds",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlBounds),
            ),
            (
                "SetControlBounds",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlBounds),
            ),
            (
                "setcontrolbounds",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlBounds),
            ),
            (
                "IdleControls",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IdleControls),
            ),
            (
                "idlecontrols",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IdleControls),
            ),
            (
                "DragControl",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DragControl),
            ),
            (
                "dragcontrol",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DragControl),
            ),
        ] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                expected,
                "library={library} symbol={symbol}"
            );
        }
    }
}

#[test]
fn control_activation_styling_and_bounds_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"NewControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

        let out_rect_ptr = PPC_DATA_BASE + 0x3000;
        let in_rect_ptr = PPC_DATA_BASE + 0x3020;
        let style_ptr = PPC_DATA_BASE + 0x3040;
        let rect_ptr = PPC_DATA_BASE + 0x3060;
        let title_ptr = PPC_DATA_BASE + 0x3080;
        loaded.memory.add_region(PPC_DATA_BASE + 0x3000, vec![0; 0x1000]);

        ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 30, 80);
        ppc_write_pstring_bytes(&mut loaded.memory, title_ptr, b"Button");
        ppc_write_rect(&mut loaded.memory, in_rect_ptr, 15, 25, 35, 95);

        // Write ControlFontStyleRec (24 bytes)
        let _ = loaded.memory.write_bytes(
            style_ptr,
            &[
                0x00, 0x01, // flags = 1
                0x00, 0x00, // font = 0
                0x00, 0x0C, // size = 12
                0x00, 0x00, // style = 0
                0x00, 0x00, // mode = 0
                0x00, 0x00, // just = 0
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // fore = 0,0,0
                0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // back = white
            ],
        );

        // 1. Create control
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = rect_ptr;
        loaded.cpu.gpr[5] = title_ptr;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0; // value
        loaded.cpu.gpr[8] = 0; // min
        loaded.cpu.gpr[9] = 1; // max
        loaded.cpu.gpr[10] = 0; // procID
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let ctrl_handle = loaded.cpu.gpr[3];
        assert_ne!(ctrl_handle, 0);

        // 2. IsValidControlHandle
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsValidControlHandle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0xDEAD_BEEF;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 3. IsControlVisible
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlVisible");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        // 4. IsControlActive (initially active)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlActive");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        // 5. DeactivateControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "DeactivateControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlActive");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 6. ActivateControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ActivateControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlActive");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        // 7. SetControlFontStyle
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlFontStyle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = style_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 8. GetControlBounds -> (10, 20, 30, 80)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlBounds");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect_ptr), Some((10, 20, 30, 80)));

        // 9. SetControlBounds -> (15, 25, 35, 95)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlBounds");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = in_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlBounds");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect_ptr), Some((15, 25, 35, 95)));

        // 10. IsControlEnabled (initially enabled)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlEnabled");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        // 11. DisableControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "DisableControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlEnabled");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 12. GetControlHilite -> 255
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlHilite");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 255);

        // 13. EnableControl
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "EnableControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlEnabled");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlHilite");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 14. IsControlHilited
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlHilited");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // Write hilite = 10
        let ctrl_ptr = loaded.memory.read_u32_be(ctrl_handle).unwrap();
        let _ = loaded.memory.write_u8(ctrl_ptr + PPC_CONTROL_HILITE_OFFSET, 10);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlHilite");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 10);

        // 15. IdleControls and DragControl (safe no-ops)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IdleControls");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "DragControl");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn import_bindings_classify_control_data_features_and_rendering_imports() {
    for library in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        for (symbol, expected) in [
            (
                "GetControlData",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlData),
            ),
            (
                "getcontroldata",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlData),
            ),
            (
                "SetControlData",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlData),
            ),
            (
                "setcontroldata",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlData),
            ),
            (
                "GetControlDataSize",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlDataSize),
            ),
            (
                "getcontroldatasize",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlDataSize),
            ),
            (
                "GetControlFeatures",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlFeatures),
            ),
            (
                "getcontrolfeatures",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlFeatures),
            ),
            (
                "GetBestControlRect",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetBestControlRect),
            ),
            (
                "getbestcontrolrect",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetBestControlRect),
            ),
            (
                "SetControlVisibility",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlVisibility),
            ),
            (
                "setcontrolvisibility",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlVisibility),
            ),
            (
                "SetControlColorProc",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlColorProc),
            ),
            (
                "setcontrolcolorproc",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlColorProc),
            ),
            (
                "GetControlColorProc",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlColorProc),
            ),
            (
                "getcontrolcolorproc",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlColorProc),
            ),
            (
                "DrawControlInCurrentPort",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawControlInCurrentPort),
            ),
            (
                "drawcontrolincurrentport",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawControlInCurrentPort),
            ),
            (
                "SetUpControlBackground",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetUpControlBackground),
            ),
            (
                "setupcontrolbackground",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetUpControlBackground),
            ),
        ] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                expected,
                "library={library} symbol={symbol}"
            );
        }
    }
}

#[test]
fn control_data_features_and_rendering_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"NewControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

        let data_in_ptr = PPC_DATA_BASE + 0x3000;
        let data_out_ptr = PPC_DATA_BASE + 0x3020;
        let size_out_ptr = PPC_DATA_BASE + 0x3040;
        let features_out_ptr = PPC_DATA_BASE + 0x3060;
        let best_rect_out_ptr = PPC_DATA_BASE + 0x3080;
        let baseline_out_ptr = PPC_DATA_BASE + 0x30A0;
        let color_proc_out_ptr = PPC_DATA_BASE + 0x30C0;
        let rect_ptr = PPC_DATA_BASE + 0x30E0;
        let title_ptr = PPC_DATA_BASE + 0x3100;
        loaded.memory.add_region(PPC_DATA_BASE + 0x3000, vec![0; 0x1000]);

        ppc_write_rect(&mut loaded.memory, rect_ptr, 10, 20, 30, 80);
        ppc_write_pstring_bytes(&mut loaded.memory, title_ptr, b"Button");

        // 1. Create control
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = rect_ptr;
        loaded.cpu.gpr[5] = title_ptr;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0; // value
        loaded.cpu.gpr[8] = 0; // min
        loaded.cpu.gpr[9] = 1; // max
        loaded.cpu.gpr[10] = 0; // procID
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let ctrl_handle = loaded.cpu.gpr[3];
        assert_ne!(ctrl_handle, 0);

        // 2. GetControlFeatures
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlFeatures");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = features_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(features_out_ptr), Some(3));

        // Invalid control features query
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = features_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

        // 3. SetControlData
        let test_payload = b"SystemlessData";
        let _ = loaded.memory.write_bytes(data_in_ptr, test_payload);
        let tag = u32::from_be_bytes(*b"data");

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlData");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0; // part
        loaded.cpu.gpr[5] = tag;
        loaded.cpu.gpr[6] = test_payload.len() as u32;
        loaded.cpu.gpr[7] = data_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 4. GetControlDataSize
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlDataSize");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = tag;
        loaded.cpu.gpr[6] = size_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(size_out_ptr), Some(test_payload.len() as u32));

        // GetControlDataSize for missing tag
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"none");
        loaded.cpu.gpr[6] = size_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_CONTROL_PROPERTY_NOT_FOUND_ERR);
        assert_eq!(loaded.memory.read_u32_be(size_out_ptr), Some(0));

        // 5. GetControlData
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlData");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = tag;
        loaded.cpu.gpr[6] = 32; // buffer size
        loaded.cpu.gpr[7] = data_out_ptr;
        loaded.cpu.gpr[8] = size_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(size_out_ptr), Some(test_payload.len() as u32));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, data_out_ptr, test_payload.len() as u32).as_deref(),
            Some(test_payload.as_slice())
        );

        // GetControlData for missing tag
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"none");
        loaded.cpu.gpr[6] = 32;
        loaded.cpu.gpr[7] = data_out_ptr;
        loaded.cpu.gpr[8] = size_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_CONTROL_PROPERTY_NOT_FOUND_ERR);

        // 6. GetBestControlRect
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetBestControlRect");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = best_rect_out_ptr;
        loaded.cpu.gpr[5] = baseline_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, best_rect_out_ptr), Some((10, 20, 30, 80)));
        assert_eq!(loaded.memory.read_u16_be(baseline_out_ptr), Some(0));

        // GetBestControlRect invalid handle
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = best_rect_out_ptr;
        loaded.cpu.gpr[5] = baseline_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

        // 7. SetControlColorProc
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlColorProc");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0x1234_5678;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 8. GetControlColorProc
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlColorProc");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = color_proc_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(color_proc_out_ptr), Some(0x1234_5678));

        // 9. SetControlVisibility
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlVisibility");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 0; // invisible
        loaded.cpu.gpr[5] = 0; // doDraw = false
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        let ctrl_ptr = loaded.memory.read_u32_be(ctrl_handle).unwrap();
        assert_eq!(loaded.memory.read_u8(ctrl_ptr + PPC_CONTROL_VISIBLE_OFFSET), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 1; // visible
        loaded.cpu.gpr[5] = 0; // doDraw = false
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u8(ctrl_ptr + PPC_CONTROL_VISIBLE_OFFSET), Some(0xFF));

        // 10. DrawControlInCurrentPort
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "DrawControlInCurrentPort");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);

        // 11. SetUpControlBackground
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetUpControlBackground");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl_handle;
        loaded.cpu.gpr[4] = 8; // depth
        loaded.cpu.gpr[5] = 1; // isColorDevice
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // SetUpControlBackground invalid handle
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);
    }
}

#[test]
fn import_bindings_classify_control_ownership_region_id_and_focus_imports() {
    for library in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        for (symbol, expected) in [
            (
                "GetControlOwner",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlOwner),
            ),
            (
                "getcontrolowner",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlOwner),
            ),
            (
                "GetControlRegion",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlRegion),
            ),
            (
                "getcontrolregion",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlRegion),
            ),
            (
                "SetControlID",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlID),
            ),
            (
                "setcontrolid",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlID),
            ),
            (
                "GetControlID",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlID),
            ),
            (
                "getcontrolid",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlID),
            ),
            (
                "SetControlCommandID",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlCommandID),
            ),
            (
                "setcontrolcommandid",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlCommandID),
            ),
            (
                "GetControlCommandID",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlCommandID),
            ),
            (
                "getcontrolcommandid",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlCommandID),
            ),
            (
                "SetKeyboardFocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetKeyboardFocus),
            ),
            (
                "setkeyboardfocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetKeyboardFocus),
            ),
            (
                "GetKeyboardFocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetKeyboardFocus),
            ),
            (
                "getkeyboardfocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetKeyboardFocus),
            ),
            (
                "AdvanceKeyboardFocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AdvanceKeyboardFocus),
            ),
            (
                "advancekeyboardfocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AdvanceKeyboardFocus),
            ),
            (
                "ReverseKeyboardFocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ReverseKeyboardFocus),
            ),
            (
                "reversekeyboardfocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ReverseKeyboardFocus),
            ),
            (
                "ClearKeyboardFocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ClearKeyboardFocus),
            ),
            (
                "clearkeyboardfocus",
                PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ClearKeyboardFocus),
            ),
        ] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                expected,
                "library={library} symbol={symbol}"
            );
        }
    }
}

#[test]
fn control_ownership_region_id_and_focus_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let pef = synthetic_pef_with_library_import(lib, b"NewControl");
        let mut loaded = load_pef_application(&pef).unwrap();
        let lib_str = std::str::from_utf8(lib).unwrap();

        let in_id_ptr = PPC_DATA_BASE + 0x3000;
        let out_id_ptr = PPC_DATA_BASE + 0x3020;
        let out_cmd_ptr = PPC_DATA_BASE + 0x3040;
        let out_focus_ptr = PPC_DATA_BASE + 0x3060;
        let rgn_ptr = PPC_DATA_BASE + 0x3080;
        let rgn_handle = PPC_DATA_BASE + 0x30A0;
        let rect1_ptr = PPC_DATA_BASE + 0x30C0;
        let title1_ptr = PPC_DATA_BASE + 0x30E0;
        let rect2_ptr = PPC_DATA_BASE + 0x3100;
        let title2_ptr = PPC_DATA_BASE + 0x3120;
        loaded.memory.add_region(PPC_DATA_BASE + 0x3000, vec![0; 0x1000]);

        // Set up region handle pointing to master pointer
        let _ = loaded.memory.write_u32_be(rgn_handle, rgn_ptr);

        ppc_write_rect(&mut loaded.memory, rect1_ptr, 10, 20, 30, 80);
        ppc_write_pstring_bytes(&mut loaded.memory, title1_ptr, b"Btn1");
        ppc_write_rect(&mut loaded.memory, rect2_ptr, 40, 20, 60, 80);
        ppc_write_pstring_bytes(&mut loaded.memory, title2_ptr, b"Btn2");

        // 1. Create first control
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = rect1_ptr;
        loaded.cpu.gpr[5] = title1_ptr;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let ctrl1 = loaded.cpu.gpr[3];
        assert_ne!(ctrl1, 0);

        // 2. Create second control
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = rect2_ptr;
        loaded.cpu.gpr[5] = title2_ptr;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let ctrl2 = loaded.cpu.gpr[3];
        assert_ne!(ctrl2, 0);

        // 3. GetControlOwner
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlOwner");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GWORLD);

        // Invalid control owner returns 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 4. GetControlRegion
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlRegion");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 0; // inPart
        loaded.cpu.gpr[5] = 0; // inTag
        loaded.cpu.gpr[6] = rgn_handle; // outRgn
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle), Some((10, 20, 30, 80)));

        // Invalid control region query
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = rgn_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

        // 5. SetControlID & GetControlID
        let sig = u32::from_be_bytes(*b"BENL");
        let id_val = 42i32;
        let _ = loaded.memory.write_u32_be(in_id_ptr, sig);
        let _ = loaded.memory.write_u32_be(in_id_ptr + 4, id_val as u32);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = in_id_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = out_id_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_id_ptr), Some(sig));
        assert_eq!(loaded.memory.read_u32_be(out_id_ptr + 4), Some(id_val as u32));

        // 6. SetControlCommandID & GetControlCommandID
        let cmd = 0x5052_4F43; // 'PROC'
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlCommandID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = cmd;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlCommandID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = out_cmd_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_cmd_ptr), Some(cmd));

        // 7. Keyboard focus operations
        // Initially no focus
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(0));

        // SetKeyboardFocus to ctrl1
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = ctrl1;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // GetKeyboardFocus returns ctrl1
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(ctrl1));

        // AdvanceKeyboardFocus moves to next control (ctrl2)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "AdvanceKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(ctrl2));

        // AdvanceKeyboardFocus again wraps around to ctrl1
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "AdvanceKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(ctrl1));

        // ReverseKeyboardFocus moves back to ctrl2
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ReverseKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(ctrl2));

        // ClearKeyboardFocus clears focus
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ClearKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetKeyboardFocus");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = out_focus_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(out_focus_ptr), Some(0));
    }
}

#[test]
fn import_bindings_classify_control_lookup_interaction_and_tracking_imports() {
    let cases = [
        ("GetControlByID", PpcLegacyControlOperation::GetControlByID),
        ("getcontrolbyid", PpcLegacyControlOperation::GetControlByID),
        ("FindControlUnderMouse", PpcLegacyControlOperation::FindControlUnderMouse),
        ("findcontrolundermouse", PpcLegacyControlOperation::FindControlUnderMouse),
        ("HandleControlClick", PpcLegacyControlOperation::HandleControlClick),
        ("handlecontrolclick", PpcLegacyControlOperation::HandleControlClick),
        ("HandleControlKey", PpcLegacyControlOperation::HandleControlKey),
        ("handlecontrolkey", PpcLegacyControlOperation::HandleControlKey),
        ("GetControlClickActivation", PpcLegacyControlOperation::GetControlClickActivation),
        ("getcontrolclickactivation", PpcLegacyControlOperation::GetControlClickActivation),
        ("GetControlKind", PpcLegacyControlOperation::GetControlKind),
        ("getcontrolkind", PpcLegacyControlOperation::GetControlKind),
        ("SetControlFocusPart", PpcLegacyControlOperation::SetControlFocusPart),
        ("setcontrolfocuspart", PpcLegacyControlOperation::SetControlFocusPart),
        ("GetControlFocusPart", PpcLegacyControlOperation::GetControlFocusPart),
        ("getcontrolfocuspart", PpcLegacyControlOperation::GetControlFocusPart),
        ("SendControlMessage", PpcLegacyControlOperation::SendControlMessage),
        ("sendcontrolmessage", PpcLegacyControlOperation::SendControlMessage),
        ("ScrollControlValues", PpcLegacyControlOperation::ScrollControlValues),
        ("scrollcontrolvalues", PpcLegacyControlOperation::ScrollControlValues),
        ("IsControlDragTrackingEnabled", PpcLegacyControlOperation::IsControlDragTrackingEnabled),
        ("iscontroldragtrackingenabled", PpcLegacyControlOperation::IsControlDragTrackingEnabled),
        ("SetControlDragTrackingEnabled", PpcLegacyControlOperation::SetControlDragTrackingEnabled),
        ("setcontroldragtrackingenabled", PpcLegacyControlOperation::SetControlDragTrackingEnabled),
    ];

    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        for (symbol, expected_op) in cases {
            let pef = synthetic_pef_with_library_import(lib, symbol.as_bytes());
            let app = load_pef_application(&pef).unwrap();
            assert_eq!(
                app.imports[0].dispatcher_target,
                PpcImportDispatcherTarget::LegacyControl(expected_op),
                "Library {:?} symbol {}",
                std::str::from_utf8(lib).unwrap(),
                symbol
            );
        }
    }
}

#[test]
fn control_lookup_interaction_and_tracking_commands_dispatch_with_canonical_evaluation() {
    let libs: &[&[u8]] = &[b"InterfaceLib", b"AppearanceLib", b"CarbonLib"];
    for &lib in libs {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetControlByID");
        let mut loaded = load_pef_application(&pef).unwrap();

        // Register two controls in window
        let ctrl1 = 0x2200;
        let ctrl1_ptr = 0x3300;
        loaded.memory.write_u32_be(ctrl1, ctrl1_ptr).unwrap();
        loaded.memory.write_u32_be(ctrl1_ptr + PPC_CONTROL_OWNER_OFFSET, PPC_MAIN_GWORLD).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_RECT_OFFSET, 10).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_RECT_OFFSET + 2, 20).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_RECT_OFFSET + 4, 50).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_RECT_OFFSET + 6, 80).unwrap();
        loaded.memory.write_u8(ctrl1_ptr + PPC_CONTROL_HILITE_OFFSET, 0).unwrap();
        loaded.memory.write_u8(ctrl1_ptr + PPC_CONTROL_VISIBLE_OFFSET, 255).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_VALUE_OFFSET, 10).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_MIN_OFFSET, 0).unwrap();
        loaded.memory.write_u16_be(ctrl1_ptr + PPC_CONTROL_MAX_OFFSET, 100).unwrap();
        loaded.memory.write_u32_be(ctrl1_ptr + PPC_CONTROL_NEXT_OFFSET, 0).unwrap();

        loaded.controls.register(ctrl1, ctrl1_ptr, 0, 0);

        // Put ctrl1 into window control list
        loaded.memory.write_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_CONTROL_LIST_OFFSET, ctrl1).unwrap();

        // Scratch pointers
        let in_id_ptr = 0x5000;
        let out_ctrl_ptr = 0x5010;
        let out_part_ptr = 0x5020;
        let out_result_ptr = 0x5030;
        let out_kind_ptr = 0x5040;
        let out_tracks_ptr = 0x5050;

        let sig = 0x5445_5354; // 'TEST'
        let id_val = 42i32;
        loaded.memory.write_u32_be(in_id_ptr, sig).unwrap();
        loaded.memory.write_u32_be(in_id_ptr + 4, id_val as u32).unwrap();

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = in_id_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 1. GetControlByID: found
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlByID");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = in_id_ptr;
        loaded.cpu.gpr[5] = out_ctrl_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ctrl_ptr), Some(ctrl1));

        // GetControlByID: not found
        loaded.memory.write_u32_be(in_id_ptr + 4, 999).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[4] = in_id_ptr;
        loaded.cpu.gpr[5] = out_ctrl_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, -30580);

        // 2. FindControlUnderMouse: point inside (v=30, h=40)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "FindControlUnderMouse");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = (30 << 16) | 40;
        loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[5] = out_part_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ctrl1);
        assert_ne!(loaded.memory.read_u16_be(out_part_ptr), Some(0));

        // FindControlUnderMouse: point outside (v=200, h=200)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = (200 << 16) | 200;
        loaded.cpu.gpr[4] = PPC_MAIN_GWORLD;
        loaded.cpu.gpr[5] = out_part_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 3. HandleControlClick
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "HandleControlClick");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = (30 << 16) | 40;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(loaded.cpu.gpr[3] as i16, 0);

        // 4. HandleControlKey
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "HandleControlKey");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 0x31;
        loaded.cpu.gpr[5] = 0x20;
        loaded.cpu.gpr[6] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 1);

        // 5. GetControlClickActivation
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlClickActivation");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = (30 << 16) | 40;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = out_result_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_result_ptr), Some(1));

        // 6. GetControlKind
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlKind");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = out_kind_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u32_be(out_kind_ptr), Some(u32::from_be_bytes(*b"appl")));

        // 7. SetControlFocusPart & GetControlFocusPart
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlFocusPart");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetControlFocusPart");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = out_part_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u16_be(out_part_ptr), Some(1));

        // 8. SendControlMessage
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SendControlMessage");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        // 9. ScrollControlValues
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ScrollControlValues");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 15;
        loaded.cpu.gpr[6] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u16_be(ctrl1_ptr + PPC_CONTROL_VALUE_OFFSET), Some(25));

        // 10. SetControlDragTrackingEnabled & IsControlDragTrackingEnabled
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetControlDragTrackingEnabled");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsControlDragTrackingEnabled");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ctrl1;
        loaded.cpu.gpr[4] = out_tracks_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i32, 0);
        assert_eq!(loaded.memory.read_u8(out_tracks_ptr), Some(1));
    }
}
