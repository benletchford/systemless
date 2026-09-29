use super::*;

#[test]
fn carbon_standard_alert_weak_import_binds_and_dispatches() {
    let weak_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"CarbonLib",
        b"StandardAlert",
        0x82,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut weak_loaded = load_pef_application(&weak_pef).unwrap();
    assert_eq!(weak_loaded.imports[0].dispatcher_target, PpcImportDispatcherTarget::StandardAlert);
    assert_ne!(weak_loaded.imports[0].address, 0);
    assert_eq!(
        weak_loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(weak_loaded.imports[0].address)
    );

    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"StandardAlert");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[7] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn text_services_leave_events_for_the_application_without_an_input_method() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"TSMEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    let event = [0x00, 0x01, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78];
    loaded.memory.add_region(event_ptr, event.to_vec());
    loaded.cpu.gpr[3] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(event_ptr), Some(0x0001_0000));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 4), Some(0x1234_5678));
}

#[test]
fn standard_alert_waits_for_input_then_disposes_the_dialog() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"StandardAlert");
    let mut loaded = load_pef_application(&pef).unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 256]);
    loaded.memory.write_u16_be(base, 0x7fff).unwrap();
    loaded
        .memory
        .write_bytes(base + 32, b"\x0cVideo setup?")
        .unwrap();
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = base + 32;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = base;

    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(loaded.memory.read_u16_be(base), Some(0x7fff));
    let dialog = *loaded.current_gworld;
    assert!(ppc_window_is_visible(&mut loaded.memory, dialog));
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let bounds = ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let point = (i32::from(bounds.1), i32::from(bounds.0));
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let border_pixel = ppc_quickdraw_read_pixel(&mut loaded.memory, front, point);
    let handles = loaded.handles();
    let items = ppc_dialog_items_for_dialog(&mut loaded.memory, &handles, dialog).unwrap();
    assert!(items.iter().any(
        |item| ppc_handle_bytes(&mut loaded.memory, &handles, item.handle).as_deref()
            == Some(b"Video setup?")
    ));
    let window_count = loaded.window_list.len();
    loaded.run_with_hle_imports(128);
    assert_eq!(
        loaded.window_list.len(),
        window_count,
        "waiting must reuse the alert"
    );

    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (u32::from(PPC_KEY_RETURN) << 8) | 13,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    }]);
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(base), Some(1));
    assert!(!loaded.window_list.contains(&dialog));
    assert!(!loaded
        .handles()
        .iter()
        .any(|record| record.handle == items_handle));
    let desktop_pixel = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, point.0, point.1),
        &loaded.screen_clut,
    );
    assert_ne!(border_pixel, desktop_pixel);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, point),
        desktop_pixel
    );
}

#[test]
fn standard_alert_preserves_optional_button_ids_for_keyboard_and_mouse() {
    for (key, character, expected) in [(PPC_KEY_RETURN, 13, 2), (PPC_KEY_ESCAPE, 27, 3), (0, 0, 3)]
    {
        let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"StandardAlert");
        let mut loaded = load_pef_application(&pef).unwrap();
        let base = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(base, vec![0; 256]);
        let params = base + 32;
        // No OK button. The second and third buttons retain IDs 2 and 3.
        loaded.memory.write_u32_be(params + 10, u32::MAX).unwrap();
        loaded.memory.write_u32_be(params + 14, base + 100).unwrap();
        loaded.memory.write_bytes(base + 100, b"\x04Quit").unwrap();
        loaded.memory.write_u16_be(params + 18, 2).unwrap();
        loaded.memory.write_u16_be(params + 20, 3).unwrap();
        loaded.cpu.gpr[3] = 3;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = params;
        loaded.cpu.gpr[7] = base;
        loaded.run_with_hle_imports(128);
        let dialog = *loaded.current_gworld;
        assert!(ppc_window_is_visible(&mut loaded.memory, dialog));
        let point = if key == 0 {
            let handles = loaded.handles();
            let items = ppc_dialog_items_for_dialog(&mut loaded.memory, &handles, dialog).unwrap();
            let bounds =
                ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
            (
                bounds.0 + items[2].rect.0 + 5,
                bounds.1 + items[2].rect.1 + 5,
            )
        } else {
            (0, 0)
        };
        loaded.set_event_queue([PpcQueuedEvent {
            what: if key == 0 { 1 } else { 3 },
            message: (u32::from(key) << 8) | character,
            when: 0,
            where_v: point.0,
            where_h: point.1,
            modifiers: 0,
        }]);
        loaded.run_with_hle_imports(128);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(base), Some(expected));
        assert!(!loaded.window_list.contains(&dialog));
    }
}

#[test]
fn standard_alert_rejects_an_invalid_output_pointer_without_creating_a_window() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"StandardAlert");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[7] = 0;
    let count = loaded.window_list.len();
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.window_list.len(), count);
}

#[test]
fn get_dialog_item_as_control_returns_control_handle() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"GetDialogItemAsControl");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0; 4]);
    let mut ditl = vec![0; 18];
    ditl[2..6].copy_from_slice(&0x1234u32.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    let items = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = output;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234));
}

#[test]
fn dialog_static_text_preserves_resource_line_breaks_before_wrapping() {
    let text =
        b"Toolbox Showcase 2.0\rClassic Macintosh Fat-App Fixture\rRunning 68K and PowerPC slices";

    assert_eq!(
        ppc_dialog_text_lines(text, 260),
        vec![
            b"Toolbox Showcase 2.0".to_vec(),
            b"Classic Macintosh Fat-App Fixture".to_vec(),
            b"Running 68K and PowerPC slices".to_vec(),
        ]
    );
}

#[test]
fn is_dialog_event_routes_front_dialog_input_and_rejects_foreign_updates() {
    let pef = synthetic_pef_with_import(b"IsDialogEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(event_ptr, vec![0; 16]);
    loaded
        .memory
        .write_u16_be(PPC_MAIN_GWORLD + PPC_CWINDOW_WINDOW_KIND_OFFSET, 2)
        .unwrap();
    loaded
        .memory
        .write_u8(PPC_MAIN_GWORLD + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    ppc_write_event_record(&mut loaded.memory, event_ptr, 3, b'G' as u32, 0, 20, 30, 0);
    loaded.cpu.gpr[3] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    ppc_write_event_record(
        &mut loaded.memory,
        event_ptr,
        6,
        PPC_MAIN_GWORLD + 0x100,
        0,
        20,
        30,
        0,
    );
    loaded.cpu.gpr[3] = event_ptr;
    loaded.run_with_hle_imports(64);

    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn dialog_select_reports_enabled_item_hit_in_front_dialog() {
    let pef = synthetic_pef_with_import(b"DialogSelect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let event_ptr = scratch;
    let dialog_out_ptr = scratch + 16;
    let item_hit_ptr = scratch + 20;
    loaded.memory.add_region(scratch, vec![0; 32]);
    loaded
        .memory
        .write_u16_be(PPC_MAIN_GWORLD + PPC_CWINDOW_WINDOW_KIND_OFFSET, 2)
        .unwrap();
    loaded
        .memory
        .write_u8(PPC_MAIN_GWORLD + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&40i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&90i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    let items = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items)
        .unwrap();
    ppc_write_event_record(&mut loaded.memory, event_ptr, 1, 0, 0, 20, 30, 0);
    loaded.cpu.gpr[3] = event_ptr;
    loaded.cpu.gpr[4] = dialog_out_ptr;
    loaded.cpu.gpr[5] = item_hit_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(dialog_out_ptr),
        Some(PPC_MAIN_GWORLD)
    );
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(1));
}

#[test]
fn find_dialog_item_returns_zero_based_index_and_minus_one_for_miss() {
    let pef = synthetic_pef_with_import(b"FindDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&40i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&90i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    let items = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items)
        .unwrap();

    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = (20 << 16) | 30;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = (50 << 16) | 30;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], u32::MAX);
}

#[test]
fn draw_dialog_calls_native_user_item_procedure_with_dialog_and_item_number() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&200i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 16];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&12i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&40i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&80i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_USER_ITEM;
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    loaded.run_with_hle_imports(64);
    let dialog = loaded.cpu.gpr[3];
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let callback = PPC_CODE_BASE + 0x1000;
    let result = PPC_DATA_BASE + 0x1000;
    let mut callback_code = Vec::new();
    for word in [
        d_form_u(15, 5, 0, (result >> 16) as u16), // lis r5, result@h
        d_form_u(24, 5, 5, result as u16),         // ori r5, r5, result@l
        d_form_u(36, 3, 5, 0),                     // stw r3, 0(r5)
        d_form_u(36, 4, 5, 4),                     // stw r4, 4(r5)
        BLR,
    ] {
        callback_code.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(callback, callback_code);
    loaded.memory.add_region(result, vec![0; 8]);
    loaded.memory.write_u32_be(items_ptr + 2, callback).unwrap();

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DrawDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(result), Some(dialog));
    assert_eq!(loaded.memory.read_u32_be(result + 4), Some(1));
    assert_eq!(*loaded.current_gworld, dialog);
    assert!(loaded.dialog_callback_stack.is_empty());
}

#[test]
fn hle_import_runner_handles_get_new_dialog_allocation() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&200i16.to_be_bytes());
    dlog[10] = 1;
    dlog[12] = 1;
    dlog[14..18].copy_from_slice(&0x1234_5678u32.to_be_bytes());
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    dlog[20] = 1;
    dlog[21] = b'T';
    let mut ditl = vec![0; 22];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&10i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&26i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&190i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_STATIC_TEXT;
    ditl[15] = 5;
    ditl[16..21].copy_from_slice(b"Hello");
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    assert_eq!(*loaded.current_gworld, dialog);
    assert!(loaded.heap_cursor() > dialog);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    assert_ne!(
        loaded.memory.read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(dialog + PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
        Some(0x1234_5678)
    );
}

#[test]
fn hle_import_runner_constructs_new_dialog_from_native_arguments() {
    let pef = synthetic_pef_with_import(b"NewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    let title_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 64]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 40, 60, 180, 300).unwrap();
    write_ppc_pstring(&mut loaded.memory, title_ptr, b"Pilot");
    let items = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &[0xff, 0xff],
    );
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = title_ptr;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 5;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x1234_5678;
    loaded
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            items,
        )
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    assert_eq!(*loaded.current_gworld, dialog);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET),
        Some(2)
    );
    assert_eq!(
        loaded.memory.read_u8(dialog + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(dialog + PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
        Some(0x1234_5678)
    );
    assert_eq!(
        loaded.memory.read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET),
        Some(items)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET),
        Some(u16::MAX)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, dialog + 16),
        Some((0, 0, 140, 240))
    );
}

#[test]
fn new_dialog_keeps_missing_resource_items_nil_without_failing_the_dialog() {
    let pef = synthetic_pef_with_import(b"NewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    let title_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 64]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 40, 60, 180, 300).unwrap();
    write_ppc_pstring(&mut loaded.memory, title_ptr, b"Missing system icon");
    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&12i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&44i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&52i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_ICON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(&2i16.to_be_bytes());
    let items = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = title_ptr;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 1;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            items,
        )
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    assert_eq!(*loaded.current_gworld, dialog);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    let items_ptr = loaded.memory.read_u32_be(items).unwrap();
    assert_eq!(loaded.memory.read_u32_be(items_ptr + 2), Some(0));
}

#[test]
fn get_new_dialog_installs_owned_control_records_in_the_live_ditl() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&200i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&12i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&32i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&90i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;

    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let control_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    assert_ne!(control_handle, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(dialog + PPC_CWINDOW_CONTROL_LIST_OFFSET),
        Some(control_handle)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(control + PPC_CONTROL_OWNER_OFFSET),
        Some(dialog)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, control + PPC_CONTROL_RECT_OFFSET),
        Some((12, 20, 32, 90))
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, control + PPC_CONTROL_TITLE_OFFSET),
        Some(b"OK".to_vec())
    );
    assert_eq!(
        loaded.controls.records(),
        vec![PpcControlRecord {
            handle: control_handle,
            pointer: control,
            proc_id: 0,
            popup_menu_id: 0,
            popup_title_width: None,
            active: true,
            font_style: None,
        }]
    );
    // The dialog item hit test only accepts control items whose record
    // reports a part, so the DITL path must register a live record.
    assert_eq!(
        ppc_control_part_at_point(
            &mut loaded.memory,
            &loaded.controls.records(),
            control_handle,
            20,
            50
        ),
        Some(10)
    );
}

#[test]
fn selecting_active_dialog_preserves_its_contents() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[0..2].copy_from_slice(&40i16.to_be_bytes());
    dlog[2..4].copy_from_slice(&60i16.to_be_bytes());
    dlog[4..6].copy_from_slice(&140i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&260i16.to_be_bytes());
    dlog[8..10].copy_from_slice(&1i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 32];
    ditl[6..8].copy_from_slice(&12i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&32i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&190i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_CHECKBOX;
    ditl[15] = 13;
    ditl[16..29].copy_from_slice(b"Sound Effects");
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    loaded.run_with_hle_imports(128);
    let dialog = loaded.cpu.gpr[3];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectWindow);
    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let marker =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        surface.front_buffer,
        (200, 110),
        marker,
    ));
    loaded.cpu.gpr[3] = dialog;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectWindow);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (200, 110)),
        Some(marker),
        "selecting an already-active dialog erased its contents"
    );
}

#[test]
fn draw_dialog_uses_live_checkbox_control_instead_of_button_fallback() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[0..2].copy_from_slice(&40i16.to_be_bytes());
    dlog[2..4].copy_from_slice(&60i16.to_be_bytes());
    dlog[4..6].copy_from_slice(&140i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&260i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 32];
    ditl[6..8].copy_from_slice(&12i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&32i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&190i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_CHECKBOX;
    ditl[15] = 13;
    ditl[16..29].copy_from_slice(b"Sound Effects");
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    loaded.run_with_hle_imports(128);
    let dialog = loaded.cpu.gpr[3];
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let control_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    loaded
        .memory
        .write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, 1)
        .unwrap();

    // Supply an existing surface; this test exercises item redraw, not
    // window-background initialization.
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!(ppc_fill_front_rect(
        &mut loaded.memory,
        front,
        (40, 60, 140, 260),
        PPC_RGB_WHITE,
    ));
    // Application drawing outside standard items survives DrawDialog.
    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let marker =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        surface.front_buffer,
        (200, 110),
        marker,
    ));

    assert!(ppc_draw_dialog(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.screen_clut,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        dialog,
    ));

    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (200, 110)),
        Some(marker),
        "DrawDialog erased application drawing outside its items"
    );
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (82, 56)),
        Some(black),
        "live checkbox indicator was not drawn"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (87, 61)),
        Some(black),
        "live checkbox value did not draw its checkmark"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (249, 52)),
        Some(white),
        "checkbox title area retained the rectangular button frame"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (77, 49)),
        Some(white),
        "the first checkbox incorrectly received the default-button outline"
    );
}

#[test]
fn dialog_popup_controls_use_ditl_bounds_and_menu_resource_items() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&180i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&320i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&38i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&226i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&58i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&326i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_RESOURCE_CONTROL;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(&200i16.to_be_bytes());
    let mut cntl = vec![0; 23];
    cntl[4..6].copy_from_slice(&20i16.to_be_bytes());
    cntl[6..8].copy_from_slice(&100i16.to_be_bytes());
    cntl[10] = 1;
    cntl[14..16].copy_from_slice(&300i16.to_be_bytes());
    cntl[16..18].copy_from_slice(&1008i16.to_be_bytes());
    let mut menu = vec![0; 15];
    menu[0..2].copy_from_slice(&300i16.to_be_bytes());
    menu[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
    for title in [b"Red".as_slice(), b"Orange".as_slice()] {
        menu.push(title.len() as u8);
        menu.extend_from_slice(title);
        menu.extend_from_slice(&[0; 4]);
    }
    menu.push(0);
    for (res_type, res_id, data) in [
        (*b"DLOG", 128, dlog),
        (*b"DITL", 128, ditl),
        (*b"CNTL", 200, cntl),
        (*b"MENU", 300, menu),
    ] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
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

    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let control_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, control + PPC_CONTROL_RECT_OFFSET),
        Some((38, 226, 58, 326))
    );
    assert_eq!(loaded.controls.records()[0].popup_menu_id, 300);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u16_be(control + PPC_CONTROL_MIN_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u16_be(control + PPC_CONTROL_MAX_OFFSET),
        Some(2)
    );
    assert_eq!(loaded.controls.records()[0].popup_title_width, Some(0));
    assert_eq!(
        ppc_popup_control_selected_text(
            &mut loaded.memory,
            &loaded.process_file_system.vfs_resources,
            *loaded.process_file_system.current_resource_file,
            300,
            2,
        ),
        b"Orange"
    );
    loaded
        .memory
        .write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, 1)
        .unwrap();
    assert!(ppc_track_dialog_popup(
        &mut loaded.memory,
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        dialog,
        (38, 226, 58, 326),
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 54,
            mouse_h: 250,
            ..PpcInputSnapshot::default()
        },
    ));
    assert_eq!(
        loaded
            .memory
            .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET),
        Some(2)
    );

    // A control placed below its dialog must not draw onto the desktop.
    let dialog_bounds =
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let top = dialog_bounds.2 - dialog_bounds.0 + 10;
    ppc_write_rect(
        &mut loaded.memory,
        control + PPC_CONTROL_RECT_OFFSET,
        top,
        10,
        top + 20,
        120,
    )
    .unwrap();
    let front = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
        .unwrap()
        .front_buffer;
    let length = front.row_bytes * front.height;
    let before = ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, length).unwrap();
    let handles = loaded.handles();
    ppc_draw_control_inner(
        &mut loaded.memory,
        &handles,
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        control_handle,
        true,
    );
    let after = ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, length).unwrap();
    assert!(
        before == after,
        "off-dialog popups must be clipped by their owning port"
    );
}

#[test]
fn hle_import_runner_handles_get_dialog_item_outputs() {
    let pef = synthetic_pef_with_import(b"GetDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let item_type_ptr = scratch;
    let item_handle_ptr = scratch + 4;
    let item_rect_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0xaa; 32]);
    let text_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"Hello",
    );
    let mut ditl = vec![0; 22];
    ditl[2..6].copy_from_slice(&text_handle.to_be_bytes());
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&10i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&26i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&220i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_STATIC_TEXT;
    ditl[15] = 5;
    ditl[16..21].copy_from_slice(b"Hello");
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    let dialog = PPC_HEAP_BASE + 0x1000;
    loaded
        .memory
        .write_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = item_type_ptr;
    loaded.cpu.gpr[6] = item_handle_ptr;
    loaded.cpu.gpr[7] = item_rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], dialog);
    assert_eq!(
        loaded.memory.read_u16_be(item_type_ptr),
        Some(u16::from(PPC_DIALOG_ITEM_STATIC_TEXT))
    );
    assert_eq!(
        loaded.memory.read_u32_be(item_handle_ptr),
        Some(text_handle)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, item_rect_ptr),
        Some((10, 10, 26, 220))
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_get_dialog_item_outputs_are_all_or_nothing() {
    let pef = synthetic_pef_with_import(b"GetDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let item_type_ptr = scratch;
    let item_handle_ptr = scratch + 4;
    let item_rect_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0xee; 12]);
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    let heap_cursor = loaded.heap_cursor();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x100;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = item_type_ptr;
    loaded.cpu.gpr[6] = item_handle_ptr;
    loaded.cpu.gpr[7] = item_rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(item_type_ptr), Some(0xeeee));
    assert_eq!(
        loaded.memory.read_u32_be(item_handle_ptr),
        Some(0xeeee_eeee)
    );
    assert_eq!(loaded.memory.read_u32_be(item_rect_ptr), Some(0xeeee_eeee));
    assert_eq!(test_handle_records!(loaded).len(), 0);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);

    let pef = synthetic_pef_with_import(b"ModalDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let item_hit_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(item_hit_ptr, vec![0xab; 1]);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = item_hit_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(item_hit_ptr), Some(0xab));
}

#[test]
fn hle_import_runner_handles_dialog_text_and_modal_defaults() {
    let pef = synthetic_pef_with_import(b"GetDialogItemText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let text_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(text_ptr, vec![0xaa; 256]);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;
    loaded.cpu.gpr[4] = text_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(loaded.memory.read_u8(text_ptr), Some(0));

    let pef = synthetic_pef_with_import(b"SetDialogItemText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let text_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(text_ptr, vec![0; 256]);
    write_ppc_pstring(&mut loaded.memory, text_ptr, b"Gridz");
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"old",
    );
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = text_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(
        ppc_handle_bytes(&mut loaded.memory, &test_handle_records!(loaded), handle),
        Some(b"Gridz".to_vec())
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let pef = synthetic_pef_with_import(b"ModalDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let item_hit_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(item_hit_ptr, vec![0; 2]);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = item_hit_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(0));
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
}

#[test]
fn get_new_dialog_opens_its_first_edit_text_item_with_a_borrowed_text_handle() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&260i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let mut ditl = vec![0; 42];
    ditl[0..2].copy_from_slice(&1u16.to_be_bytes());
    ditl[6..8].copy_from_slice(&12i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&32i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&220i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_EDIT_TEXT;
    ditl[15] = 5;
    ditl[16..21].copy_from_slice(b"Pilot");
    ditl[26..28].copy_from_slice(&40i16.to_be_bytes());
    ditl[28..30].copy_from_slice(&20i16.to_be_bytes());
    ditl[30..32].copy_from_slice(&60i16.to_be_bytes());
    ditl[32..34].copy_from_slice(&220i16.to_be_bytes());
    ditl[34] = PPC_DIALOG_ITEM_EDIT_TEXT;
    ditl[35] = 5;
    ditl[36..41].copy_from_slice(b"Alias");
    for (res_type, data) in [(*b"DLOG", dlog), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: String::new(),
                res_type: u32::from_be_bytes(res_type),
                res_id: 128,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
    }
    loaded.cpu.gpr[3] = 128;
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let item_text_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    let second_item_text_handle = loaded.memory.read_u32_be(items_ptr + 22).unwrap();
    let te_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
        .unwrap();
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    assert_ne!(te_handle, 0);
    assert!(loaded
        .event_queue()
        .iter()
        .any(|event| event.what == 6 && event.message == dialog));
    assert_eq!(
        loaded.memory.read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET),
        Some(item_text_handle)
    );
    assert_eq!(
        ppc_te_text_bytes(&mut loaded.memory, &test_handle_records!(loaded), te_handle),
        Some(b"Pilot".to_vec())
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_DIALOG_EDIT_OPEN_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET),
        Some(5)
    );

    let item_hit_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(item_hit_ptr, vec![0; 2]);
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ModalDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[4] = item_hit_ptr;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = dialog);
    loaded.set_event_queue([PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 10,
        where_v: 50,
        where_h: 30,
        modifiers: 0,
    }]);

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let second_te_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
        .unwrap();
    let second_te_ptr = loaded.memory.read_u32_be(second_te_handle).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(second_te_ptr + PPC_TE_HTEXT_OFFSET),
        Some(second_item_text_handle)
    );
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (2 << 8) | u32::from(b'D'),
        when: 0,
        where_v: 20,
        where_h: 30,
        modifiers: 0,
    }]);

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        ppc_te_text_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            second_te_handle
        ),
        Some(b"ADlias".to_vec())
    );
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(0));

    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (u32::from(PPC_KEY_RETURN) << 8) | 0x0d,
        when: 0,
        where_v: 20,
        where_h: 30,
        modifiers: 0,
    }]);
    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(1));

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, None);
    assert!(!loaded.gworlds.iter().any(|record| record.port == dialog));
    assert!(!test_handle_records!(loaded).iter().any(|record| {
        matches!(record.handle, h if h == te_handle
            || h == second_te_handle
            || h == item_text_handle
            || h == second_item_text_handle
            || h == items_handle)
    }));
    assert_eq!(loaded.memory.read_u32_be(te_handle), Some(0));
    assert!(loaded
        .free_ptr_blocks()
        .iter()
        .any(|record| record.ptr == dialog));
}

#[test]
fn import_bindings_classify_dialog_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNewDialog"),
        PpcImportDispatcherTarget::GetNewDialog
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewDialog"),
        PpcImportDispatcherTarget::NewDialog
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDialogItem"),
        PpcImportDispatcherTarget::GetDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDialogItemText"),
        PpcImportDispatcherTarget::GetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "getdialogitemtext"),
        PpcImportDispatcherTarget::GetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDialogItemText"),
        PpcImportDispatcherTarget::SetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "setdialogitemtext"),
        PpcImportDispatcherTarget::SetDialogItemText
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDialogTracksCursor"),
        PpcImportDispatcherTarget::SetDialogTracksCursor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveDialogItem"),
        PpcImportDispatcherTarget::MoveDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("AppearanceLib", "MoveDialogItem"),
        PpcImportDispatcherTarget::MoveDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SizeDialogItem"),
        PpcImportDispatcherTarget::SizeDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("AppearanceLib", "SizeDialogItem"),
        PpcImportDispatcherTarget::SizeDialogItem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AppendDialogItemList"),
        PpcImportDispatcherTarget::AppendDialogItemList
    );
    assert_eq!(
        dispatcher_target_for_import("AppearanceLib", "AppendDialogItemList"),
        PpcImportDispatcherTarget::AppendDialogItemList
    );
    assert_eq!(
        dispatcher_target_for_import("DialogsLib", "AppendDialogItemList"),
        PpcImportDispatcherTarget::AppendDialogItemList
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "AppendDialogItemList"),
        PpcImportDispatcherTarget::AppendDialogItemList
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StdFilterProc"),
        PpcImportDispatcherTarget::StdFilterProc
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetStdFilterProc"),
        PpcImportDispatcherTarget::GetStdFilterProc
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ModalDialog"),
        PpcImportDispatcherTarget::ModalDialog
    );
    for (symbol, operation) in [
        ("AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        (
            "DialogSelect",
            PpcDialogCompatibilityOperation::DialogSelect,
        ),
        (
            "FindDialogItem",
            PpcDialogCompatibilityOperation::FindDialogItem,
        ),
        (
            "HideDialogItem",
            PpcDialogCompatibilityOperation::HideDialogItem,
        ),
        (
            "IsDialogEvent",
            PpcDialogCompatibilityOperation::IsDialogEvent,
        ),
        ("ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        (
            "ShowDialogItem",
            PpcDialogCompatibilityOperation::ShowDialogItem,
        ),
        (
            "UpdateDialog",
            PpcDialogCompatibilityOperation::UpdateDialog,
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::DialogCompatibility(operation),
        );
    }
}

#[test]
fn get_std_filter_proc_returns_a_guest_callable_standard_filter() {
    let pef = synthetic_pef_with_import(b"GetStdFilterProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(output, vec![0; 4]);
    loaded.cpu.gpr[3] = output;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_STD_FILTER_TVECTOR));
    let callback_pc = PPC_IMPORT_TRAP_BASE + PPC_STD_FILTER_IMPORT_INDEX * 4;
    assert_eq!(loaded.memory.read_u32_be(PPC_STD_FILTER_TVECTOR), Some(callback_pc));
    assert_eq!(loaded.memory.read_u32_be(PPC_STD_FILTER_TVECTOR + 4), Some(0));

    loaded.cpu.pc = callback_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0xdead_beef;
    let callback = loaded.run_with_hle_imports(64);
    assert_eq!(callback.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    let dialog = PPC_DATA_BASE + 0x5100;
    let event = PPC_DATA_BASE + 0x5300;
    let item_hit = PPC_DATA_BASE + 0x5400;
    loaded.memory.add_region(dialog, vec![0; PPC_DIALOG_RECORD_SIZE as usize]);
    loaded.memory.add_region(event, vec![0; 16]);
    loaded.memory.add_region(item_hit, vec![0; 2]);
    loaded
        .memory
        .write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, 2)
        .unwrap();
    loaded.memory.write_u16_be(event, 3).unwrap();
    loaded.memory.write_u32_be(event + 2, b'\r' as u32).unwrap();
    loaded.cpu.pc = callback_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = item_hit;
    let callback = loaded.run_with_hle_imports(64);
    assert_eq!(callback.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(item_hit), Some(2));
}

#[test]
fn get_std_filter_proc_rejects_unwritable_output() {
    let pef = synthetic_pef_with_import(b"GetStdFilterProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn std_filter_proc_dispatches_with_canonical_evaluation() {
    let pef = synthetic_pef_with_import(b"StdFilterProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let dialog = PPC_DATA_BASE + 0x5100;
    let event = PPC_DATA_BASE + 0x5300;
    let item_hit = PPC_DATA_BASE + 0x5400;
    loaded.memory.add_region(dialog, vec![0; PPC_DIALOG_RECORD_SIZE as usize]);
    loaded.memory.add_region(event, vec![0; 16]);
    loaded.memory.add_region(item_hit, vec![0; 2]);

    let app_code_pc = loaded.cpu.pc;

    // 1. NIL parameters reject
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = item_hit;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = item_hit;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2. Return key with no default item (default_item == 0) returns FALSE (0)
    loaded.memory.write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, 0).unwrap();
    loaded.memory.write_u16_be(event, 3).unwrap(); // keyDown
    loaded.memory.write_u32_be(event + 2, b'\r' as u32).unwrap();
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = item_hit;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 3. Return key with default item (e.g. 2) returns TRUE (1) and sets itemHit
    loaded.memory.write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, 2).unwrap();
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = item_hit;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(item_hit), Some(2));

    // 4. Other key (e.g. Escape) returns FALSE (0)
    loaded.memory.write_u32_be(event + 2, 0x1B).unwrap();
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = item_hit;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn find_dialog_item_falls_through_group_boxes_to_enclosed_controls() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&200i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&300i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let item = |rect: [i16; 4], item_type: u8, data: &[u8]| {
        let mut bytes = vec![0; 4];
        for value in rect {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.push(item_type);
        bytes.push(data.len() as u8);
        bytes.extend_from_slice(data);
        if !bytes.len().is_multiple_of(2) {
            bytes.push(0);
        }
        bytes
    };
    // Item 1: a kControlGroupBoxTextTitleProc group box around item 2.
    let mut ditl = 1i16.to_be_bytes().to_vec();
    ditl.extend(item([10, 10, 150, 250], PPC_DIALOG_ITEM_RESOURCE_CONTROL, &200i16.to_be_bytes()));
    ditl.extend(item([40, 30, 60, 110], PPC_DIALOG_ITEM_BUTTON, b"OK"));
    let mut cntl = vec![0; 23];
    for (index, value) in [10i16, 10, 150, 250].into_iter().enumerate() {
        cntl[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
    }
    cntl[10] = 1;
    cntl[16..18].copy_from_slice(&160i16.to_be_bytes());
    for (res_type, res_id, data) in [(*b"DLOG", 128, dlog), (*b"DITL", 128, ditl), (*b"CNTL", 200, cntl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_resource_refnum,
            path: String::new(),
            res_type: u32::from_be_bytes(res_type),
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
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);

    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "FindDialogItem");
    for (point, expected) in [
        // Inside the button: the enclosing group box does not swallow it.
        ((50u32 << 16) | 70, 1),
        // Inside the group box but over no control: nothing is hit.
        ((120u32 << 16) | 200, u32::MAX),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = dialog;
        loaded.cpu.gpr[4] = point;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.gpr[3], expected, "point {point:08X}");
    }
}

#[test]
fn set_dialog_cancel_item_dispatches_with_canonical_evaluation() {
    let pef = synthetic_pef_with_import(b"SetDialogCancelItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let dialog = PPC_DATA_BASE + 0x5100;
    loaded.memory.add_region(dialog, vec![0; PPC_DIALOG_RECORD_SIZE as usize]);

    let app_code_pc = loaded.cpu.pc;

    // 1. NIL dialog rejects with paramErr
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 2. Valid dialog writes cancel item to DIALOG_CANCEL_ITEM_OFFSET
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_CANCEL_ITEM_OFFSET),
        Some(2)
    );

    // 3. Test canonical evaluation against items
    let dummy_items: [(u8, &[u8]); 2] = [
        (crate::dialog_manager::DIALOG_ITEM_BUTTON, b"OK"),
        (crate::dialog_manager::DIALOG_ITEM_BUTTON, b"Cancel"),
    ];
    let eval = crate::dialog_manager::evaluate_dialog_cancel_item(
        loaded
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_CANCEL_ITEM_OFFSET)
            .map(|item| item as i16),
        dummy_items.iter().copied(),
    );
    assert_eq!(eval.item_no(), Some(2));
    assert_eq!(eval.to_u16(), Some(2));
    assert!(eval.has_item());
    assert!(eval.matches_item(2));
}

#[test]
fn reset_alert_stage_resets_acount_to_initial_stage() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"ResetAlertStage");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u16_be(crate::memory::globals::addr::ALERT_STAGE, 2)
        .unwrap();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::ALERT_STAGE),
        Some(crate::dialog_manager::INITIAL_ALERT_STAGE)
    );
}

#[test]
fn init_dialogs_initializes_dialog_globals_in_memory() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"InitDialogs");
    let mut loaded = load_pef_application(&pef).unwrap();
    use crate::memory::globals::addr;
    loaded.memory.write_u32_be(addr::RESUME_PROC, 0xDEAD_BEEF).unwrap();
    loaded.memory.write_u32_be(addr::DA_BEEPER, 0x00AA_BBCC).unwrap();
    loaded.memory.write_u16_be(addr::ALERT_STAGE, 3).unwrap();
    for i in 0..4u32 {
        loaded.memory.write_u32_be(addr::DA_STRINGS + i * 4, 0x00D0_0000 | i).unwrap();
    }
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(addr::RESUME_PROC),
        Some(0x1234_5678)
    );
    assert_eq!(loaded.memory.read_u32_be(addr::DA_BEEPER), Some(0));
    assert_eq!(
        loaded.memory.read_u16_be(addr::ALERT_STAGE),
        Some(crate::dialog_manager::INITIAL_ALERT_STAGE)
    );
    for i in 0..4u32 {
        assert_eq!(loaded.memory.read_u32_be(addr::DA_STRINGS + i * 4), Some(0));
    }
}

#[test]
fn alert_stage_progression_suppression_and_anumber_recording() {
    let pef = synthetic_pef_with_import(b"Alert");
    let mut loaded = load_pef_application(&pef).unwrap();
    let alert_id = 140i16;
    // Template with 4 stages:
    // Stage 0 (nibble 0): 0x4 -> box_drawn = true, default_item = 1
    // Stage 1 (nibble 1): 0xC -> box_drawn = true, default_item = 2
    // Stage 2 (nibble 2): 0x0 -> box_drawn = false (suppressed!)
    // Stage 3 (nibble 3): 0x4 -> box_drawn = true, default_item = 1
    // Word: (4 << 12) | (0 << 8) | (12 << 4) | 4 = 0x40C4
    let stages_word = 0x40C4u16;
    let mut alert = vec![0; 14];
    for (offset, value) in [(0, 130i16), (2, 150), (4, 260), (6, 450)] {
        alert[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    alert[8..10].copy_from_slice(&alert_id.to_be_bytes());
    alert[10..12].copy_from_slice(&stages_word.to_be_bytes());

    let mut ditl = vec![0; 38];
    ditl[0..2].copy_from_slice(&1i16.to_be_bytes());
    for (offset, value) in [(6, 20i16), (8, 20), (10, 70), (12, 280)] {
        ditl[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    for (offset, value) in [(22, 90i16), (24, 210), (26, 110), (28, 280)] {
        ditl[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    ditl[30] = PPC_DIALOG_ITEM_BUTTON;
    ditl[31] = 6;
    ditl[32..38].copy_from_slice(b"Cancel");

    for (res_type, data) in [(*b"ALRT", alert), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_resource_refnum,
            path: String::new(),
            res_type: u32::from_be_bytes(res_type),
            res_id: alert_id,
            name: Vec::new(),
            data,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
    }

    use crate::memory::globals::addr;
    let app_code_pc = loaded.cpu.pc;

    // 1. Stage 2 in stages_word is nibble 0x0: suppressed!
    loaded.memory.write_u16_be(addr::ALERT_STAGE, 2).unwrap();
    loaded.memory.write_u16_be(addr::ANUMBER, 0xCAFE).unwrap();
    loaded.cpu.gpr[3] = alert_id as u16 as u32;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(crate::dialog_manager::ALERT_SUPPRESSED_RESULT)
    );
    assert_eq!(
        loaded.memory.read_u16_be(addr::ALERT_STAGE),
        Some(3),
        "suppressed alert advances ALERT_STAGE to 3"
    );
    assert_eq!(
        loaded.memory.read_u16_be(addr::ANUMBER),
        Some(alert_id as u16),
        "suppressed alert records ANUMBER"
    );

    // 2. Stage 3 in stages_word is nibble 0x4: drawn! Next stage stays clamped at 3.
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = alert_id as u16 as u32;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        loaded.memory.read_u16_be(addr::ALERT_STAGE),
        Some(3),
        "clamped alert stage stays at 3"
    );
    assert_eq!(
        loaded.memory.read_u16_be(addr::ANUMBER),
        Some(alert_id as u16)
    );
    let dialog = *loaded.current_gworld;
    assert!(ppc_window_is_visible(&mut loaded.memory, dialog));
}

#[test]
fn error_sound_dispatches_with_canonical_evaluation() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"ErrorSound");
    let mut loaded = load_pef_application(&pef).unwrap();
    use crate::memory::globals::addr;

    let app_code_pc = loaded.cpu.pc;
    loaded
        .memory
        .write_u32_be(addr::DA_BEEPER, 0xDEAD_BEEF)
        .unwrap();

    // 1. ErrorSound with custom procedure pointer
    loaded.cpu.gpr[3] = 0x00AB_CDEF;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(addr::DA_BEEPER),
        Some(0x00AB_CDEF)
    );

    // 2. ErrorSound with NIL (silence)
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(addr::DA_BEEPER), Some(0));
}

#[test]
fn lm_dabeeper_accessors_manage_da_beeper_global() {
    use crate::memory::globals::addr;

    // LMSetDABeeper
    let pef_set = synthetic_pef_with_library_import(b"InterfaceLib", b"LMSetDABeeper");
    let mut loaded_set = load_pef_application(&pef_set).unwrap();
    loaded_set.cpu.gpr[3] = 0x0012_3456;
    let probe = loaded_set.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded_set.memory.read_u32_be(addr::DA_BEEPER),
        Some(0x0012_3456)
    );

    // LMGetDABeeper
    let pef_get = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetDABeeper");
    let mut loaded_get = load_pef_application(&pef_get).unwrap();
    loaded_get
        .memory
        .write_u32_be(addr::DA_BEEPER, 0x0012_3456)
        .unwrap();
    let probe = loaded_get.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get.cpu.gpr[3], 0x0012_3456);
}

#[test]
fn lm_acount_and_anumber_accessors_manage_dialog_globals() {
    use crate::memory::globals::addr;

    // LMSetACount and LMGetACount
    let pef_set_acount = synthetic_pef_with_library_import(b"InterfaceLib", b"LMSetACount");
    let mut loaded_set_acount = load_pef_application(&pef_set_acount).unwrap();
    loaded_set_acount.cpu.gpr[3] = 2;
    let probe = loaded_set_acount.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded_set_acount.memory.read_u16_be(addr::ALERT_STAGE),
        Some(2)
    );

    let pef_get_acount = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetACount");
    let mut loaded_get_acount = load_pef_application(&pef_get_acount).unwrap();
    loaded_get_acount
        .memory
        .write_u16_be(addr::ALERT_STAGE, 3)
        .unwrap();
    let probe = loaded_get_acount.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get_acount.cpu.gpr[3], 3);

    // LMSetANumber and LMGetANumber
    let pef_set_anumber = synthetic_pef_with_library_import(b"InterfaceLib", b"LMSetANumber");
    let mut loaded_set_anumber = load_pef_application(&pef_set_anumber).unwrap();
    loaded_set_anumber.cpu.gpr[3] = 128;
    let probe = loaded_set_anumber.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded_set_anumber.memory.read_u16_be(addr::ANUMBER),
        Some(128)
    );

    let pef_get_anumber = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetANumber");
    let mut loaded_get_anumber = load_pef_application(&pef_get_anumber).unwrap();
    loaded_get_anumber
        .memory
        .write_u16_be(addr::ANUMBER, 256)
        .unwrap();
    let probe = loaded_get_anumber.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get_anumber.cpu.gpr[3], 256);
}

#[test]
fn lm_dlgfont_accessors_manage_dlg_font_global() {
    use crate::memory::globals::addr;

    let pef_set = synthetic_pef_with_library_import(b"InterfaceLib", b"LMSetDlgFont");
    let mut loaded_set = load_pef_application(&pef_set).unwrap();
    loaded_set.cpu.gpr[3] = 4;
    let probe = loaded_set.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_set.memory.read_u16_be(addr::DLG_FONT), Some(4));

    let pef_get = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetDlgFont");
    let mut loaded_get = load_pef_application(&pef_get).unwrap();
    loaded_get
        .memory
        .write_u16_be(addr::DLG_FONT, 7)
        .unwrap();
    let probe = loaded_get.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get.cpu.gpr[3], 7);
}

#[test]
fn get_dialog_default_item_reads_default_item_and_rejects_invalid_pointers() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetDialogDefaultItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let dialog = PPC_DATA_BASE + 0x1000;
    let out_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(dialog, vec![0; PPC_DIALOG_RECORD_SIZE as usize]);
    loaded.memory.add_region(out_ptr, vec![0; 4]);

    let app_code_pc = loaded.cpu.pc;

    // 1. Unset default item resolves to 1 (DEFAULT_DIALOG_ITEM)
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(1));

    // 2. Explicitly configured default item (3) is returned
    loaded
        .memory
        .write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, 3)
        .unwrap();
    loaded.memory.write_u16_be(out_ptr, 0).unwrap();
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(3));

    // 3. NIL dialog pointer returns paramErr
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 4. NIL out pointer returns paramErr
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn get_dialog_cancel_item_reads_cancel_item_and_rejects_invalid_pointers() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"GetDialogCancelItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let dialog = PPC_DATA_BASE + 0x1000;
    let out_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(dialog, vec![0; PPC_DIALOG_RECORD_SIZE as usize]);
    loaded.memory.add_region(out_ptr, vec![0; 4]);

    // Build DITL with 2 buttons: item 1 "OK", item 2 "Cancel"
    let mut ditl = Vec::new();
    ditl.extend_from_slice(&1u16.to_be_bytes()); // 2 items (0-based count)
    // Item 1: OK
    ditl.extend_from_slice(&0u32.to_be_bytes());
    ditl.extend_from_slice(&[0, 10, 0, 10, 0, 30, 0, 80]);
    ditl.push(PPC_DIALOG_ITEM_BUTTON);
    ditl.push(2);
    ditl.extend_from_slice(b"OK");
    if ditl.len() % 2 != 0 {
        ditl.push(0);
    }
    // Item 2: Cancel
    ditl.extend_from_slice(&0u32.to_be_bytes());
    ditl.extend_from_slice(&[0, 10, 0, 90, 0, 30, 0, 160]);
    ditl.push(PPC_DIALOG_ITEM_BUTTON);
    ditl.push(6);
    ditl.extend_from_slice(b"Cancel");
    if ditl.len() % 2 != 0 {
        ditl.push(0);
    }

    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    let app_code_pc = loaded.cpu.pc;

    // 1. Inferred cancel button titled "Cancel" (item 2)
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(2));

    // 2. Explicitly configured cancel button (5) overrides title search
    loaded
        .memory
        .write_u16_be(dialog + PPC_DIALOG_CANCEL_ITEM_OFFSET, 5)
        .unwrap();
    loaded.memory.write_u16_be(out_ptr, 0).unwrap();
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(5));

    // 3. NIL dialog pointer returns paramErr
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 4. NIL out pointer returns paramErr
    loaded.cpu.pc = app_code_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn could_dialog_and_free_dialog_dispatch_with_canonical_evaluation() {
    use crate::memory::globals::addr;

    let pef_could = synthetic_pef_with_library_import(b"InterfaceLib", b"CouldDialog");
    let mut loaded_could = load_pef_application(&pef_could).unwrap();
    loaded_could
        .memory
        .write_u16_be(addr::RES_ERR, 0xBEEF)
        .unwrap();
    loaded_could.cpu.gpr[3] = 128;
    assert_eq!(loaded_could.imports[0].dispatcher_target, PpcImportDispatcherTarget::CouldDialog);
    let probe = loaded_could.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_could.cpu.gpr[3], 128); // Preserved
    assert_eq!(loaded_could.memory.read_u16_be(addr::RES_ERR), Some(0));

    let pef_free = synthetic_pef_with_library_import(b"InterfaceLib", b"FreeDialog");
    let mut loaded_free = load_pef_application(&pef_free).unwrap();
    loaded_free
        .memory
        .write_u16_be(addr::RES_ERR, 0xCAFE)
        .unwrap();
    loaded_free.cpu.gpr[3] = 128;
    let probe = loaded_free.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_free.cpu.gpr[3], 128); // Preserved
    assert_eq!(loaded_free.memory.read_u16_be(addr::RES_ERR), Some(0));
}

#[test]
fn could_alert_and_free_alert_dispatch_with_canonical_evaluation() {
    use crate::memory::globals::addr;

    let pef_could = synthetic_pef_with_library_import(b"InterfaceLib", b"CouldAlert");
    let mut loaded_could = load_pef_application(&pef_could).unwrap();
    loaded_could
        .memory
        .write_u16_be(addr::RES_ERR, 0xBEEF)
        .unwrap();
    loaded_could.cpu.gpr[3] = 256;
    let probe = loaded_could.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_could.cpu.gpr[3], 256); // Preserved
    assert_eq!(loaded_could.memory.read_u16_be(addr::RES_ERR), Some(0));

    let pef_free = synthetic_pef_with_library_import(b"InterfaceLib", b"FreeAlert");
    let mut loaded_free = load_pef_application(&pef_free).unwrap();
    loaded_free
        .memory
        .write_u16_be(addr::RES_ERR, 0xCAFE)
        .unwrap();
    loaded_free.cpu.gpr[3] = 256;
    let probe = loaded_free.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_free.cpu.gpr[3], 256); // Preserved
    assert_eq!(loaded_free.memory.read_u16_be(addr::RES_ERR), Some(0));
}

#[test]
fn move_dialog_item_moves_ditl_item_and_control_and_rejects_invalid_params() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"MoveDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();

    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&50i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&100i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");

    let mut control_rec = vec![0u8; 40];
    control_rec[8..10].copy_from_slice(&10i16.to_be_bytes());
    control_rec[10..12].copy_from_slice(&20i16.to_be_bytes());
    control_rec[12..14].copy_from_slice(&50i16.to_be_bytes());
    control_rec[14..16].copy_from_slice(&100i16.to_be_bytes());
    let ctrl_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &control_rec,
    );
    ditl[2..6].copy_from_slice(&ctrl_handle.to_be_bytes());

    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 1. Move item 1 to inHoriz=150, inVert=120
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 150;
    loaded.cpu.gpr[6] = 120;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    let ditl_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 6), Some(120));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 8), Some(150));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 10), Some(160));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 12), Some(230));

    let ctrl_ptr = loaded.memory.read_u32_be(ctrl_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 8), Some(120));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 10), Some(150));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 12), Some(160));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 14), Some(230));

    // 2. Reject NIL dialog
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 100;
    loaded.cpu.gpr[6] = 100;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 3. Reject invalid item number
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 99;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn size_dialog_item_sizes_ditl_item_and_control_and_rejects_invalid_params() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"SizeDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();

    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&50i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&100i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");

    let mut control_rec = vec![0u8; 40];
    control_rec[8..10].copy_from_slice(&10i16.to_be_bytes());
    control_rec[10..12].copy_from_slice(&20i16.to_be_bytes());
    control_rec[12..14].copy_from_slice(&50i16.to_be_bytes());
    control_rec[14..16].copy_from_slice(&100i16.to_be_bytes());
    let ctrl_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &control_rec,
    );
    ditl[2..6].copy_from_slice(&ctrl_handle.to_be_bytes());

    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 1. Resize item 1 to inWidth=120, inHeight=70
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 120;
    loaded.cpu.gpr[6] = 70;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    let ditl_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 6), Some(10));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 8), Some(20));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 10), Some(80));
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr + 12), Some(140));

    let ctrl_ptr = loaded.memory.read_u32_be(ctrl_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 8), Some(10));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 10), Some(20));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 12), Some(80));
    assert_eq!(loaded.memory.read_u16_be(ctrl_ptr + 14), Some(140));

    // 2. Reject NIL dialog
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 80;
    loaded.cpu.gpr[6] = 80;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 3. Reject invalid item number
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 99;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn append_dialog_item_list_appends_items_and_rejects_missing_resources_and_invalid_params() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"AppendDialogItemList");
    let mut loaded = load_pef_application(&pef).unwrap();

    let mut ditl = vec![0; 18];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&50i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&100i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");

    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl,
    );
    loaded
        .memory
        .write_u32_be(PPC_MAIN_GWORLD + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    let mut ditl_res = vec![0; 20];
    ditl_res[0..2].copy_from_slice(&0i16.to_be_bytes());
    ditl_res[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl_res[8..10].copy_from_slice(&10i16.to_be_bytes());
    ditl_res[10..12].copy_from_slice(&30i16.to_be_bytes());
    ditl_res[12..14].copy_from_slice(&80i16.to_be_bytes());
    ditl_res[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl_res[15] = 4;
    ditl_res[16..20].copy_from_slice(b"More");

    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: String::new(),
        res_type: u32::from_be_bytes(*b"DITL"),
        res_id: 300,
        name: Vec::new(),
        data: ditl_res,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });

    // 1. Valid call: AppendDialogItemList(PPC_MAIN_GWORLD, 300, appendDITLBottom = 2)
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 300;
    loaded.cpu.gpr[5] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    // Live items count should now be 2 (count minus 1 = 1)
    let ditl_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(ditl_ptr), Some(1));

    // 2. Reject NIL dialog
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 300;
    loaded.cpu.gpr[5] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 3. Reject ditl_id == 0
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    // 4. Reject missing DITL resource
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 9999;
    loaded.cpu.gpr[5] = 2;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RES_NOT_FOUND_ERR));
}
