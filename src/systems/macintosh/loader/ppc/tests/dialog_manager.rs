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
    let caller = (loaded.cpu.gpr, loaded.cpu.lr, loaded.cpu.ctr, loaded.cpu.cr);
    assert_eq!(caller.0[3..8], [3, base + 32, 0, 0, base]);
    loaded.run_with_hle_imports(128);
    assert_eq!(
        loaded.window_list.len(),
        window_count,
        "waiting must reuse the alert"
    );
    assert_eq!(
        (loaded.cpu.gpr, loaded.cpu.lr, loaded.cpu.ctr, loaded.cpu.cr),
        caller,
        "an idle pass must leave the caller's registers intact"
    );

    loaded.set_event_queue([
        PpcQueuedEvent {
            what: 3,
            message: (u32::from(PPC_KEY_RETURN) << 8) | 13,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 6,
            message: dialog,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
    ]);
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
        .event_queue()
        .iter()
        .any(|event| event.what == 6 && event.message == dialog));
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
        if key == 0 {
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: true, mouse_v: point.0, mouse_h: point.1,
                ..PpcInputSnapshot::default()
            });
            loaded.run_with_hle_imports(128);
            assert!(ppc_window_is_visible(&mut loaded.memory, dialog));
            assert_eq!(loaded.memory.read_u16_be(base), Some(0));
            let handles = loaded.handles();
            let items = ppc_dialog_items_for_dialog(&mut loaded.memory, &handles, dialog).unwrap();
            let control = ppc_control_ptr(&mut loaded.memory, items[2].handle).unwrap();
            assert_eq!(loaded.memory.read_u8(control + PPC_CONTROL_HILITE_OFFSET), Some(10));
            loaded.event_queue.push_back(PpcQueuedEvent {
                what: 2, message: 0, when: 1,
                where_v: point.0, where_h: point.1, modifiers: 0,
            });
            // The release event, not a later pointer move, determines the hit.
            loaded.set_input_snapshot(PpcInputSnapshot::default());
        }
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
    loaded.window_list.push(PPC_MAIN_GWORLD);
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
fn is_dialog_event_ignores_dialog_behind_front_window() {
    let pef = synthetic_pef_with_import(b"IsDialogEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    let overlay = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(event_ptr, vec![0; 16]);
    loaded.memory.add_region(overlay, vec![0; 256]);
    loaded
        .memory
        .write_u16_be(PPC_MAIN_GWORLD + PPC_CWINDOW_WINDOW_KIND_OFFSET, 2)
        .unwrap();
    loaded
        .memory
        .write_u8(PPC_MAIN_GWORLD + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u8(overlay + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.window_list.with_mut(|windows| {
        windows.retain(|window| *window != PPC_MAIN_GWORLD);
        windows.insert(0, PPC_MAIN_GWORLD);
    });
    ppc_write_event_record(&mut loaded.memory, event_ptr, 3, b'G' as u32, 0, 20, 30, 0);
    loaded.cpu.gpr[3] = event_ptr;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.window_list.with_mut(|windows| windows.insert(0, overlay));
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
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
    loaded.window_list.push(PPC_MAIN_GWORLD);
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
        d_form_u(14, 3, 0, 0x1234),                // overwrite volatile argument registers
        d_form_u(14, 4, 0, 0x5678),
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
    loaded.cpu.gpr[4] = 0xcafe_babe;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(result), Some(dialog));
    assert_eq!(loaded.memory.read_u32_be(result + 4), Some(1));
    assert_eq!(loaded.cpu.gpr[3], dialog);
    assert_eq!(loaded.cpu.gpr[4], 0xcafe_babe);
    assert_eq!(*loaded.current_gworld, dialog);
    assert!(loaded.dialog_callback_stack.is_empty());

    // ModalDialog must retain its itemHit pointer across the same callback.
    let item_hit_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(item_hit_ptr, vec![0; 2]);
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ModalDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = item_hit_ptr;
    loaded.set_event_queue([PpcQueuedEvent {
        what: 6,
        message: dialog,
        when: 1,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    }]);
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.cpu.gpr[4], item_hit_ptr);
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: 0x0d,
        when: 2,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    }]);
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(1));
}

const MODAL_FILTER_FALSE: u16 = 0x0100;
const MODAL_FILTER_TRUE: u16 = 0x7f01;

/// Install a native ModalFilterProc at `code` that records each call at
/// `record` and answers `answer` in r3: the low byte is the Boolean, the
/// high byte is noise the caller must ignore. With `item`, the filter
/// also stores that item number through itemHit.
///
/// Record: call count, r3 (dialog), r4 (&event), r5 (&itemHit), r1, then a
/// copy of the 16-byte EventRecord.
fn install_recording_modal_filter(
    loaded: &mut PpcLoadedApp,
    code: u32,
    record: u32,
    answer: u16,
    item: Option<u16>,
) {
    let mut words = vec![
        d_form_u(15, 11, 0, (record >> 16) as u16), // lis r11, record@h
        d_form_u(24, 11, 11, record as u16),        // ori r11, r11, record@l
        d_form_u(32, 12, 11, 0),                    // lwz r12, 0(r11)
        d_form_u(14, 12, 12, 1),                    // addi r12, r12, 1
        d_form_u(36, 12, 11, 0),                    // stw r12, 0(r11)
        d_form_u(36, 3, 11, 4),                     // stw r3, 4(r11)
        d_form_u(36, 4, 11, 8),                     // stw r4, 8(r11)
        d_form_u(36, 5, 11, 12),                    // stw r5, 12(r11)
        d_form_u(36, 1, 11, 16),                    // stw r1, 16(r11)
    ];
    for offset in [0u16, 4, 8, 12] {
        words.push(d_form_u(32, 12, 4, offset)); // lwz r12, offset(r4)
        words.push(d_form_u(36, 12, 11, 20 + offset)); // stw r12, 20+offset(r11)
    }
    if let Some(item) = item {
        words.push(d_form_u(14, 12, 0, item)); // li r12, item
        words.push(d_form_u(44, 12, 5, 0)); // sth r12, 0(r5)
    }
    words.extend([
        d_form_u(14, 3, 0, answer), // li r3, answer
        d_form_u(14, 4, 0, 0x5678), // clobber volatile argument registers
        d_form_u(14, 5, 0, 0x1234),
        BLR,
    ]);
    let bytes = words.iter().flat_map(|word| word.to_be_bytes()).collect();
    loaded.memory.add_region(code, bytes);
    loaded.memory.add_region(record, vec![0; 36]);
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedModalFilterCall {
    count: u32,
    dialog: u32,
    event_ptr: u32,
    item_hit_ptr: u32,
    sp: u32,
    event: PpcQueuedEvent,
}

fn recorded_modal_filter_call(loaded: &mut PpcLoadedApp, record: u32) -> RecordedModalFilterCall {
    let memory = &mut loaded.memory;
    RecordedModalFilterCall {
        count: memory.read_u32_be(record).unwrap(),
        dialog: memory.read_u32_be(record + 4).unwrap(),
        event_ptr: memory.read_u32_be(record + 8).unwrap(),
        item_hit_ptr: memory.read_u32_be(record + 12).unwrap(),
        sp: memory.read_u32_be(record + 16).unwrap(),
        event: PpcQueuedEvent {
            what: memory.read_u16_be(record + 20).unwrap(),
            message: memory.read_u32_be(record + 22).unwrap(),
            when: memory.read_u32_be(record + 26).unwrap(),
            where_v: memory.read_u16_be(record + 30).unwrap() as i16,
            where_h: memory.read_u16_be(record + 32).unwrap() as i16,
            modifiers: memory.read_u16_be(record + 34).unwrap(),
        },
    }
}

/// A GetNewDialog dialog with one OK button at local (12, 20, 32, 90),
/// left with the import rebound to ModalDialog(filter, itemHit).
fn modal_dialog_with_filter(filter: u32) -> (PpcLoadedApp, u32, u32) {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut dlog = vec![0; 22];
    for (offset, value) in [(0, 60i16), (2, 80), (4, 160), (6, 280)] {
        dlog[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    let ditl = make_test_ditl(&[make_test_ditl_item(12, 20, 32, 90, b"OK")]);
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
    loaded.cpu.gpr[5] = u32::MAX;
    loaded.run_with_hle_imports(128);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);

    let item_hit_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(item_hit_ptr, vec![0; 2]);
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ModalDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = filter;
    loaded.cpu.gpr[4] = item_hit_ptr;
    (loaded, dialog, item_hit_ptr)
}

fn modal_filter_mouse_down(v: i16, h: i16) -> PpcQueuedEvent {
    PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 77,
        where_v: v,
        where_h: h,
        modifiers: 0x0100,
    }
}

#[test]
fn modal_dialog_offers_each_event_to_its_filter_before_hit_testing() {
    let filter = PPC_CODE_BASE + 0x1000;
    let record = PPC_DATA_BASE + 0x1800;
    let (mut loaded, dialog, item_hit_ptr) = modal_dialog_with_filter(filter);
    install_recording_modal_filter(&mut loaded, filter, record, MODAL_FILTER_FALSE, None);
    let caller_sp = loaded.cpu.gpr[1];
    // A click in the OK button (dialog global origin 60, 80).
    let click = modal_filter_mouse_down(60 + 20, 80 + 50);
    loaded.set_event_queue([click]);

    let probe = loaded.run_with_hle_imports(512);

    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    let call = recorded_modal_filter_call(&mut loaded, record);
    assert_eq!(call.count, 1);
    assert_eq!(call.dialog, dialog);
    assert_eq!(call.item_hit_ptr, item_hit_ptr);
    assert_eq!(call.event, click);
    // The EventRecord lives in a frame below the caller's stack.
    assert!(call.sp < caller_sp && call.sp % 16 == 0);
    assert!(call.event_ptr > call.sp && call.event_ptr + 16 <= caller_sp);
    // Declined: ModalDialog hit-tests the event itself.
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(1));
    assert_eq!(loaded.cpu.gpr[1], caller_sp);
    assert_eq!(loaded.cpu.gpr[3..5], [filter, item_hit_ptr]);
    assert!(loaded.dialog_callback_stack.is_empty());
}

#[test]
fn modal_dialog_returns_the_item_a_filter_stores_for_a_disabled_button() {
    let filter = PPC_CODE_BASE + 0x1000;
    let record = PPC_DATA_BASE + 0x1800;
    let (mut loaded, dialog, item_hit_ptr) = modal_dialog_with_filter(0);
    install_recording_modal_filter(&mut loaded, filter, record, MODAL_FILTER_TRUE, Some(1));
    // The application keeps the button inactive and hit-tests it itself.
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let control_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    let control = loaded.memory.read_u32_be(control_handle).unwrap();
    loaded
        .memory
        .write_u8(control + PPC_CONTROL_HILITE_OFFSET, 255)
        .unwrap();
    let click = modal_filter_mouse_down(60 + 20, 80 + 50);

    // Without a filter the click on the inactive button selects nothing.
    loaded.set_event_queue([click]);
    let probe = loaded.run_with_hle_imports(512);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(record), Some(0));

    loaded.cpu.gpr[3] = filter;
    loaded.set_event_queue([click]);
    let probe = loaded.run_with_hle_imports(512);

    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(recorded_modal_filter_call(&mut loaded, record).count, 1);
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(1));
    assert_eq!(loaded.cpu.gpr[3..5], [filter, item_hit_ptr]);
    assert!(loaded.dialog_callback_stack.is_empty());
}

#[test]
fn modal_dialog_offers_null_events_to_its_filter_on_every_idle_pass() {
    let filter = PPC_CODE_BASE + 0x1000;
    let record = PPC_DATA_BASE + 0x1800;
    let (mut loaded, dialog, item_hit_ptr) = modal_dialog_with_filter(filter);
    install_recording_modal_filter(&mut loaded, filter, record, MODAL_FILTER_FALSE, None);
    loaded.set_tick_count(4321);
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map: [0; 16],
        mouse_button: false,
        mouse_v: 33,
        mouse_h: 44,
    });
    loaded.set_event_queue([]);
    let caller_sp = loaded.cpu.gpr[1];

    for pass in 1..=3 {
        let probe = loaded.run_with_hle_imports(512);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        let call = recorded_modal_filter_call(&mut loaded, record);
        assert_eq!(call.count, pass);
        assert_eq!(call.dialog, dialog);
        assert_eq!(call.item_hit_ptr, item_hit_ptr);
        // Each pass reuses the stack frame rather than allocating a record.
        assert!(call.event_ptr > call.sp && call.event_ptr + 16 <= caller_sp);
        // `when` is the current tick count, which includes the guest time
        // elapsed in the execution slice.
        assert!((4321..4321 + 60).contains(&call.event.when));
        assert_eq!(
            call.event,
            PpcQueuedEvent {
                what: 0,
                message: 0,
                when: call.event.when,
                where_v: 33,
                where_h: 44,
                // btnState: the button is up.
                modifiers: 0x0080,
            }
        );
        assert_eq!(loaded.cpu.gpr[1], caller_sp);
        assert_eq!(loaded.cpu.gpr[3..5], [filter, item_hit_ptr]);
        assert!(loaded.dialog_callback_stack.is_empty());
    }
    assert_eq!(loaded.memory.read_u16_be(item_hit_ptr), Some(0));
}

#[test]
fn resource_alert_calls_its_filter_with_modal_dialog_arguments() {
    let filter_false = PPC_CODE_BASE + 0x1000;
    let filter_true = PPC_CODE_BASE + 0x1100;
    let record = PPC_DATA_BASE + 0x1800;
    let record_true = PPC_DATA_BASE + 0x1900;
    let pef = synthetic_pef_with_import(b"Alert");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_recording_modal_filter(&mut loaded, filter_false, record, MODAL_FILTER_FALSE, None);
    install_recording_modal_filter(
        &mut loaded,
        filter_true,
        record_true,
        MODAL_FILTER_TRUE,
        Some(1),
    );
    let alert_id = 128i16;
    let mut alert = vec![0; 12];
    for (offset, value) in [(0, 130i16), (2, 150), (4, 260), (6, 450)] {
        alert[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    alert[8..10].copy_from_slice(&alert_id.to_be_bytes());
    alert[10..12].copy_from_slice(&0x4444u16.to_be_bytes());
    let ditl = make_test_ditl(&[make_test_ditl_item(90, 210, 110, 280, b"OK")]);
    for (res_type, data) in [(*b"ALRT", alert), (*b"DITL", ditl)] {
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded
            .process_file_system
            .push_vfs_resource(PpcVfsResourceRecord {
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
    loaded.cpu.gpr[3] = alert_id as u16 as u32;
    loaded.cpu.gpr[4] = filter_false;
    let caller_gprs = loaded.cpu.gpr[1..11].to_vec();
    let mut caller_lr = None;

    // Idle passes call the filter with ModalDialog's arguments and leave the
    // caller's registers, including Alert's own arguments, intact.
    for pass in 1..=2 {
        let probe = loaded.run_with_hle_imports(512);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        let dialog = *loaded.current_gworld;
        let call = recorded_modal_filter_call(&mut loaded, record);
        assert_eq!(call.count, pass);
        assert_eq!(call.dialog, dialog);
        assert_eq!(
            call.item_hit_ptr,
            dialog + crate::dialog_manager::DIALOG_ALERT_HIT_OFFSET
        );
        assert_eq!(call.event.what, 0);
        assert!(call.event_ptr > call.sp && call.event_ptr + 16 <= caller_gprs[0]);
        assert_eq!(loaded.cpu.gpr[1..11], caller_gprs);
        assert_ne!(loaded.cpu.lr, loaded.cpu.pc);
        assert_eq!(*caller_lr.get_or_insert(loaded.cpu.lr), loaded.cpu.lr);
        assert_eq!(loaded.window_list.first(), Some(dialog));
        assert!(loaded.dialog_callback_stack.is_empty());
    }
    let dialog = *loaded.current_gworld;
    let window_count = loaded.window_list.len();

    // A click outside every item, which the filter claims as item 1.
    loaded.cpu.gpr[4] = filter_true;
    let click = modal_filter_mouse_down(130 + 5, 150 + 5);
    loaded.set_event_queue([click]);
    let probe = loaded.run_with_hle_imports(512);

    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    let call = recorded_modal_filter_call(&mut loaded, record_true);
    assert_eq!(call.count, 1);
    assert_eq!(call.dialog, dialog);
    assert_eq!(call.event, click);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.cpu.gpr[1], caller_gprs[0]);
    assert!(!loaded.window_list.contains(&dialog));
    assert_eq!(loaded.window_list.len(), window_count - 1);
    assert!(loaded.dialog_callback_stack.is_empty());
}

#[test]
fn standard_alert_calls_the_filter_from_its_parameter_record() {
    let filter_false = PPC_CODE_BASE + 0x1000;
    let filter_true = PPC_CODE_BASE + 0x1100;
    let record = PPC_DATA_BASE + 0x1800;
    let record_true = PPC_DATA_BASE + 0x1900;
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"StandardAlert");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_recording_modal_filter(&mut loaded, filter_false, record, MODAL_FILTER_FALSE, None);
    install_recording_modal_filter(
        &mut loaded,
        filter_true,
        record_true,
        MODAL_FILTER_TRUE,
        Some(1),
    );
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 256]);
    loaded.memory.write_u16_be(base, 0x7fff).unwrap();
    loaded
        .memory
        .write_bytes(base + 32, b"\x0cVideo setup?")
        .unwrap();
    // AlertStdAlertParamRec: filterProc, the default "OK" text, default
    // button 1, no cancel button.
    let alert_params = base + 64;
    loaded
        .memory
        .write_u32_be(alert_params + 2, filter_false)
        .unwrap();
    loaded
        .memory
        .write_u32_be(alert_params + 6, u32::MAX)
        .unwrap();
    loaded.memory.write_u16_be(alert_params + 18, 1).unwrap();
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = base + 32;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = alert_params;
    loaded.cpu.gpr[7] = base;
    let caller_gprs = loaded.cpu.gpr[1..11].to_vec();
    let mut caller_lr = None;

    for pass in 1..=2 {
        let probe = loaded.run_with_hle_imports(512);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        let dialog = *loaded.current_gworld;
        let call = recorded_modal_filter_call(&mut loaded, record);
        assert_eq!(call.count, pass);
        assert_eq!(call.dialog, dialog);
        assert_eq!(
            call.item_hit_ptr,
            dialog + crate::dialog_manager::DIALOG_ALERT_HIT_OFFSET
        );
        assert_eq!(call.event.what, 0);
        assert_eq!(loaded.cpu.gpr[1..11], caller_gprs);
        assert_ne!(loaded.cpu.lr, loaded.cpu.pc);
        assert_eq!(*caller_lr.get_or_insert(loaded.cpu.lr), loaded.cpu.lr);
        assert!(ppc_window_is_visible(&mut loaded.memory, dialog));
        assert!(loaded.dialog_callback_stack.is_empty());
    }
    let dialog = *loaded.current_gworld;
    assert_eq!(loaded.memory.read_u16_be(base), Some(0x7fff));

    loaded
        .memory
        .write_u32_be(alert_params + 2, filter_true)
        .unwrap();
    let bounds = ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let click = modal_filter_mouse_down(bounds.0 + 2, bounds.1 + 2);
    loaded.set_event_queue([click]);
    let probe = loaded.run_with_hle_imports(512);

    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    let call = recorded_modal_filter_call(&mut loaded, record_true);
    assert_eq!(call.count, 1);
    assert_eq!(call.dialog, dialog);
    assert_eq!(call.event, click);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u16_be(base), Some(1));
    assert!(!loaded.window_list.contains(&dialog));
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
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_TX_FONT_OFFSET),
        Some(0)
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
    assert_ne!(loaded.controls.records()[0].generation, 0);
    assert_eq!(
        loaded.controls.records(),
        vec![PpcControlRecord {
            handle: control_handle,
            pointer: control,
            generation: loaded.controls.records()[0].generation,
            proc_id: 0,
            popup_menu_id: 0,
            popup_title_width: None,
            active: true,
            font_style: None,
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
        Some(0)
    );

    let item_hit_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(item_hit_ptr, vec![0; 2]);
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ModalDialog;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0; // no filter proc
    loaded.cpu.gpr[4] = item_hit_ptr;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = dialog);
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (2 << 8) | u32::from(b'X'),
        when: 0,
        where_v: 20,
        where_h: 30,
        modifiers: 0,
    }]);
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        ppc_te_text_bytes(&mut loaded.memory, &test_handle_records!(loaded), te_handle),
        Some(b"XPilot".to_vec())
    );
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
    // Redrawing the edited second field must leave guest drawing over the
    // first item intact. A whole-dialog repaint would overwrite this mark.
    let surface = ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    let marker = ppc_quickdraw_surface_color_pixel(
        &mut loaded.memory,
        surface,
        PpcRgbColor {
            red: 0xffff,
            green: 0,
            blue: 0xffff,
        },
    )
    .unwrap();
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        surface.front_buffer,
        (40, 20),
        marker,
    ));
    assert!(ppc_draw_dialog_selected(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        &loaded.controls.records(),
        &loaded.gworlds,
        &loaded.screen_clut,
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
        dialog,
        Some(2),
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (40, 20)),
        Some(marker),
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
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "GetNewDialog", PpcImportDispatcherTarget::GetNewDialog),
        ("AppearanceLib", "GetNewDialog", PpcImportDispatcherTarget::GetNewDialog),
        ("DialogsLib", "GetNewDialog", PpcImportDispatcherTarget::GetNewDialog),
        ("CarbonLib", "GetNewDialog", PpcImportDispatcherTarget::GetNewDialog),
        ("InterfaceLib", "NewDialog", PpcImportDispatcherTarget::NewDialog),
        ("AppearanceLib", "NewDialog", PpcImportDispatcherTarget::NewDialog),
        ("DialogsLib", "NewDialog", PpcImportDispatcherTarget::NewDialog),
        ("CarbonLib", "NewDialog", PpcImportDispatcherTarget::NewDialog),
        ("InterfaceLib", "NewColorDialog", PpcImportDispatcherTarget::NewDialog),
        ("AppearanceLib", "NewColorDialog", PpcImportDispatcherTarget::NewDialog),
        ("DialogsLib", "NewColorDialog", PpcImportDispatcherTarget::NewDialog),
        ("CarbonLib", "NewColorDialog", PpcImportDispatcherTarget::NewDialog),
        ("InterfaceLib", "NewCDialog", PpcImportDispatcherTarget::NewDialog),
        ("AppearanceLib", "NewCDialog", PpcImportDispatcherTarget::NewDialog),
        ("DialogsLib", "NewCDialog", PpcImportDispatcherTarget::NewDialog),
        ("CarbonLib", "NewCDialog", PpcImportDispatcherTarget::NewDialog),
        ("InterfaceLib", "NewFeaturesDialog", PpcImportDispatcherTarget::NewFeaturesDialog),
        ("AppearanceLib", "NewFeaturesDialog", PpcImportDispatcherTarget::NewFeaturesDialog),
        ("DialogsLib", "NewFeaturesDialog", PpcImportDispatcherTarget::NewFeaturesDialog),
        ("CarbonLib", "NewFeaturesDialog", PpcImportDispatcherTarget::NewFeaturesDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
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
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "MoveDialogItem", PpcImportDispatcherTarget::MoveDialogItem),
        ("AppearanceLib", "MoveDialogItem", PpcImportDispatcherTarget::MoveDialogItem),
        ("DialogsLib", "MoveDialogItem", PpcImportDispatcherTarget::MoveDialogItem),
        ("CarbonLib", "MoveDialogItem", PpcImportDispatcherTarget::MoveDialogItem),
        ("InterfaceLib", "SizeDialogItem", PpcImportDispatcherTarget::SizeDialogItem),
        ("AppearanceLib", "SizeDialogItem", PpcImportDispatcherTarget::SizeDialogItem),
        ("DialogsLib", "SizeDialogItem", PpcImportDispatcherTarget::SizeDialogItem),
        ("CarbonLib", "SizeDialogItem", PpcImportDispatcherTarget::SizeDialogItem),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
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
        dispatcher_target_for_import("InterfaceLib", "AutoSizeDialog"),
        PpcImportDispatcherTarget::AutoSizeDialog
    );
    assert_eq!(
        dispatcher_target_for_import("AppearanceLib", "AutoSizeDialog"),
        PpcImportDispatcherTarget::AutoSizeDialog
    );
    assert_eq!(
        dispatcher_target_for_import("DialogsLib", "AutoSizeDialog"),
        PpcImportDispatcherTarget::AutoSizeDialog
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "AutoSizeDialog"),
        PpcImportDispatcherTarget::AutoSizeDialog
    );
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "CouldDialog", PpcImportDispatcherTarget::CouldDialog),
        ("AppearanceLib", "CouldDialog", PpcImportDispatcherTarget::CouldDialog),
        ("DialogsLib", "CouldDialog", PpcImportDispatcherTarget::CouldDialog),
        ("CarbonLib", "CouldDialog", PpcImportDispatcherTarget::CouldDialog),
        ("InterfaceLib", "FreeDialog", PpcImportDispatcherTarget::FreeDialog),
        ("AppearanceLib", "FreeDialog", PpcImportDispatcherTarget::FreeDialog),
        ("DialogsLib", "FreeDialog", PpcImportDispatcherTarget::FreeDialog),
        ("CarbonLib", "FreeDialog", PpcImportDispatcherTarget::FreeDialog),
        ("InterfaceLib", "CouldAlert", PpcImportDispatcherTarget::CouldAlert),
        ("AppearanceLib", "CouldAlert", PpcImportDispatcherTarget::CouldAlert),
        ("DialogsLib", "CouldAlert", PpcImportDispatcherTarget::CouldAlert),
        ("CarbonLib", "CouldAlert", PpcImportDispatcherTarget::CouldAlert),
        ("InterfaceLib", "FreeAlert", PpcImportDispatcherTarget::FreeAlert),
        ("AppearanceLib", "FreeAlert", PpcImportDispatcherTarget::FreeAlert),
        ("DialogsLib", "FreeAlert", PpcImportDispatcherTarget::FreeAlert),
        ("CarbonLib", "FreeAlert", PpcImportDispatcherTarget::FreeAlert),
        ("InterfaceLib", "ErrorSound", PpcImportDispatcherTarget::ErrorSound),
        ("AppearanceLib", "ErrorSound", PpcImportDispatcherTarget::ErrorSound),
        ("DialogsLib", "ErrorSound", PpcImportDispatcherTarget::ErrorSound),
        ("CarbonLib", "ErrorSound", PpcImportDispatcherTarget::ErrorSound),
        ("InterfaceLib", "StdFilterProc", PpcImportDispatcherTarget::StdFilterProc),
        ("AppearanceLib", "StdFilterProc", PpcImportDispatcherTarget::StdFilterProc),
        ("DialogsLib", "StdFilterProc", PpcImportDispatcherTarget::StdFilterProc),
        ("CarbonLib", "StdFilterProc", PpcImportDispatcherTarget::StdFilterProc),
        ("InterfaceLib", "GetStdFilterProc", PpcImportDispatcherTarget::GetStdFilterProc),
        ("AppearanceLib", "GetStdFilterProc", PpcImportDispatcherTarget::GetStdFilterProc),
        ("DialogsLib", "GetStdFilterProc", PpcImportDispatcherTarget::GetStdFilterProc),
        ("CarbonLib", "GetStdFilterProc", PpcImportDispatcherTarget::GetStdFilterProc),
        ("InterfaceLib", "InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("AppearanceLib", "InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("DialogsLib", "InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("CarbonLib", "InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("InterfaceLib", "GetAlertStage", PpcImportDispatcherTarget::GetAlertStage),
        ("AppearanceLib", "GetAlertStage", PpcImportDispatcherTarget::GetAlertStage),
        ("DialogsLib", "GetAlertStage", PpcImportDispatcherTarget::GetAlertStage),
        ("CarbonLib", "GetAlertStage", PpcImportDispatcherTarget::GetAlertStage),
        ("InterfaceLib", "SetDialogFont", PpcImportDispatcherTarget::SetDialogFont),
        ("AppearanceLib", "SetDialogFont", PpcImportDispatcherTarget::SetDialogFont),
        ("DialogsLib", "SetDialogFont", PpcImportDispatcherTarget::SetDialogFont),
        ("CarbonLib", "SetDialogFont", PpcImportDispatcherTarget::SetDialogFont),
        ("InterfaceLib", "SetDAFont", PpcImportDispatcherTarget::SetDialogFont),
        ("AppearanceLib", "SetDAFont", PpcImportDispatcherTarget::SetDialogFont),
        ("DialogsLib", "SetDAFont", PpcImportDispatcherTarget::SetDialogFont),
        ("CarbonLib", "SetDAFont", PpcImportDispatcherTarget::SetDialogFont),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "DrawDialog", PpcImportDispatcherTarget::DrawDialog),
        ("AppearanceLib", "DrawDialog", PpcImportDispatcherTarget::DrawDialog),
        ("DialogsLib", "DrawDialog", PpcImportDispatcherTarget::DrawDialog),
        ("CarbonLib", "DrawDialog", PpcImportDispatcherTarget::DrawDialog),
        ("InterfaceLib", "ModalDialog", PpcImportDispatcherTarget::ModalDialog),
        ("AppearanceLib", "ModalDialog", PpcImportDispatcherTarget::ModalDialog),
        ("DialogsLib", "ModalDialog", PpcImportDispatcherTarget::ModalDialog),
        ("CarbonLib", "ModalDialog", PpcImportDispatcherTarget::ModalDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("AppearanceLib", "CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("DialogsLib", "CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("CarbonLib", "CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("InterfaceLib", "DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("AppearanceLib", "DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("DialogsLib", "DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("CarbonLib", "DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("InterfaceLib", "DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("AppearanceLib", "DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("DialogsLib", "DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("CarbonLib", "DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "GetDialogItem", PpcImportDispatcherTarget::GetDialogItem),
        ("AppearanceLib", "GetDialogItem", PpcImportDispatcherTarget::GetDialogItem),
        ("DialogsLib", "GetDialogItem", PpcImportDispatcherTarget::GetDialogItem),
        ("CarbonLib", "GetDialogItem", PpcImportDispatcherTarget::GetDialogItem),
        ("InterfaceLib", "GetDItem", PpcImportDispatcherTarget::GetDialogItem),
        ("AppearanceLib", "GetDItem", PpcImportDispatcherTarget::GetDialogItem),
        ("DialogsLib", "GetDItem", PpcImportDispatcherTarget::GetDialogItem),
        ("CarbonLib", "GetDItem", PpcImportDispatcherTarget::GetDialogItem),
        ("InterfaceLib", "SetDialogItem", PpcImportDispatcherTarget::SetDialogItem),
        ("AppearanceLib", "SetDialogItem", PpcImportDispatcherTarget::SetDialogItem),
        ("DialogsLib", "SetDialogItem", PpcImportDispatcherTarget::SetDialogItem),
        ("CarbonLib", "SetDialogItem", PpcImportDispatcherTarget::SetDialogItem),
        ("InterfaceLib", "SetDItem", PpcImportDispatcherTarget::SetDialogItem),
        ("AppearanceLib", "SetDItem", PpcImportDispatcherTarget::SetDialogItem),
        ("DialogsLib", "SetDItem", PpcImportDispatcherTarget::SetDialogItem),
        ("CarbonLib", "SetDItem", PpcImportDispatcherTarget::SetDialogItem),
        ("InterfaceLib", "GetDialogItemText", PpcImportDispatcherTarget::GetDialogItemText),
        ("AppearanceLib", "GetDialogItemText", PpcImportDispatcherTarget::GetDialogItemText),
        ("DialogsLib", "GetDialogItemText", PpcImportDispatcherTarget::GetDialogItemText),
        ("CarbonLib", "GetDialogItemText", PpcImportDispatcherTarget::GetDialogItemText),
        ("InterfaceLib", "GetIText", PpcImportDispatcherTarget::GetDialogItemText),
        ("AppearanceLib", "GetIText", PpcImportDispatcherTarget::GetDialogItemText),
        ("DialogsLib", "GetIText", PpcImportDispatcherTarget::GetDialogItemText),
        ("CarbonLib", "GetIText", PpcImportDispatcherTarget::GetDialogItemText),
        ("InterfaceLib", "SetDialogItemText", PpcImportDispatcherTarget::SetDialogItemText),
        ("AppearanceLib", "SetDialogItemText", PpcImportDispatcherTarget::SetDialogItemText),
        ("DialogsLib", "SetDialogItemText", PpcImportDispatcherTarget::SetDialogItemText),
        ("CarbonLib", "SetDialogItemText", PpcImportDispatcherTarget::SetDialogItemText),
        ("InterfaceLib", "SetIText", PpcImportDispatcherTarget::SetDialogItemText),
        ("AppearanceLib", "SetIText", PpcImportDispatcherTarget::SetDialogItemText),
        ("DialogsLib", "SetIText", PpcImportDispatcherTarget::SetDialogItemText),
        ("CarbonLib", "SetIText", PpcImportDispatcherTarget::SetDialogItemText),
        ("InterfaceLib", "SelectDialogItemText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("AppearanceLib", "SelectDialogItemText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("DialogsLib", "SelectDialogItemText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("CarbonLib", "SelectDialogItemText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("InterfaceLib", "SelIText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("AppearanceLib", "SelIText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("DialogsLib", "SelIText", PpcImportDispatcherTarget::SelectDialogItemText),
        ("CarbonLib", "SelIText", PpcImportDispatcherTarget::SelectDialogItemText),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "SetDialogDefaultItem", PpcImportDispatcherTarget::SetDialogDefaultItem),
        ("AppearanceLib", "SetDialogDefaultItem", PpcImportDispatcherTarget::SetDialogDefaultItem),
        ("DialogsLib", "SetDialogDefaultItem", PpcImportDispatcherTarget::SetDialogDefaultItem),
        ("CarbonLib", "SetDialogDefaultItem", PpcImportDispatcherTarget::SetDialogDefaultItem),
        ("InterfaceLib", "GetDialogDefaultItem", PpcImportDispatcherTarget::GetDialogDefaultItem),
        ("AppearanceLib", "GetDialogDefaultItem", PpcImportDispatcherTarget::GetDialogDefaultItem),
        ("DialogsLib", "GetDialogDefaultItem", PpcImportDispatcherTarget::GetDialogDefaultItem),
        ("CarbonLib", "GetDialogDefaultItem", PpcImportDispatcherTarget::GetDialogDefaultItem),
        ("InterfaceLib", "SetDialogCancelItem", PpcImportDispatcherTarget::SetDialogCancelItem),
        ("AppearanceLib", "SetDialogCancelItem", PpcImportDispatcherTarget::SetDialogCancelItem),
        ("DialogsLib", "SetDialogCancelItem", PpcImportDispatcherTarget::SetDialogCancelItem),
        ("CarbonLib", "SetDialogCancelItem", PpcImportDispatcherTarget::SetDialogCancelItem),
        ("InterfaceLib", "GetDialogCancelItem", PpcImportDispatcherTarget::GetDialogCancelItem),
        ("AppearanceLib", "GetDialogCancelItem", PpcImportDispatcherTarget::GetDialogCancelItem),
        ("DialogsLib", "GetDialogCancelItem", PpcImportDispatcherTarget::GetDialogCancelItem),
        ("CarbonLib", "GetDialogCancelItem", PpcImportDispatcherTarget::GetDialogCancelItem),
        ("InterfaceLib", "SetDialogTracksCursor", PpcImportDispatcherTarget::SetDialogTracksCursor),
        ("AppearanceLib", "SetDialogTracksCursor", PpcImportDispatcherTarget::SetDialogTracksCursor),
        ("DialogsLib", "SetDialogTracksCursor", PpcImportDispatcherTarget::SetDialogTracksCursor),
        ("CarbonLib", "SetDialogTracksCursor", PpcImportDispatcherTarget::SetDialogTracksCursor),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "ParamText", PpcImportDispatcherTarget::ParamText),
        ("AppearanceLib", "ParamText", PpcImportDispatcherTarget::ParamText),
        ("DialogsLib", "ParamText", PpcImportDispatcherTarget::ParamText),
        ("CarbonLib", "ParamText", PpcImportDispatcherTarget::ParamText),
        ("InterfaceLib", "paramtext", PpcImportDispatcherTarget::ParamText),
        ("AppearanceLib", "paramtext", PpcImportDispatcherTarget::ParamText),
        ("DialogsLib", "paramtext", PpcImportDispatcherTarget::ParamText),
        ("CarbonLib", "paramtext", PpcImportDispatcherTarget::ParamText),
        ("InterfaceLib", "Alert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Alert)),
        ("AppearanceLib", "Alert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Alert)),
        ("DialogsLib", "Alert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Alert)),
        ("CarbonLib", "Alert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Alert)),
        ("InterfaceLib", "StopAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Stop)),
        ("AppearanceLib", "StopAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Stop)),
        ("DialogsLib", "StopAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Stop)),
        ("CarbonLib", "StopAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Stop)),
        ("InterfaceLib", "NoteAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Note)),
        ("AppearanceLib", "NoteAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Note)),
        ("DialogsLib", "NoteAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Note)),
        ("CarbonLib", "NoteAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Note)),
        ("InterfaceLib", "CautionAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Caution)),
        ("AppearanceLib", "CautionAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Caution)),
        ("DialogsLib", "CautionAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Caution)),
        ("CarbonLib", "CautionAlert", PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Caution)),
        ("InterfaceLib", "StandardAlert", PpcImportDispatcherTarget::StandardAlert),
        ("AppearanceLib", "StandardAlert", PpcImportDispatcherTarget::StandardAlert),
        ("DialogsLib", "StandardAlert", PpcImportDispatcherTarget::StandardAlert),
        ("CarbonLib", "StandardAlert", PpcImportDispatcherTarget::StandardAlert),
        (
            "InterfaceLib",
            "ResetAlertStage",
            PpcImportDispatcherTarget::SystemCompatibility(
                PpcSystemCompatibilityOperation::ResetAlertStage,
            ),
        ),
        (
            "AppearanceLib",
            "ResetAlertStage",
            PpcImportDispatcherTarget::SystemCompatibility(
                PpcSystemCompatibilityOperation::ResetAlertStage,
            ),
        ),
        (
            "DialogsLib",
            "ResetAlertStage",
            PpcImportDispatcherTarget::SystemCompatibility(
                PpcSystemCompatibilityOperation::ResetAlertStage,
            ),
        ),
        (
            "CarbonLib",
            "ResetAlertStage",
            PpcImportDispatcherTarget::SystemCompatibility(
                PpcSystemCompatibilityOperation::ResetAlertStage,
            ),
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
    for lib in ["InterfaceLib", "AppearanceLib", "DialogsLib", "CarbonLib"] {
        for (symbol, expected_target) in [
            ("LMSetResumeProc", PpcImportDispatcherTarget::LMSetResumeProc),
            ("lmsetresumeproc", PpcImportDispatcherTarget::LMSetResumeProc),
            ("LMGetResumeProc", PpcImportDispatcherTarget::LMGetResumeProc),
            ("lmgetresumeproc", PpcImportDispatcherTarget::LMGetResumeProc),
            ("LMSetACount", PpcImportDispatcherTarget::LMSetACount),
            ("lmsetacount", PpcImportDispatcherTarget::LMSetACount),
            ("LMGetACount", PpcImportDispatcherTarget::LMGetACount),
            ("lmgetacount", PpcImportDispatcherTarget::LMGetACount),
            ("LMSetANumber", PpcImportDispatcherTarget::LMSetANumber),
            ("lmsetanumber", PpcImportDispatcherTarget::LMSetANumber),
            ("LMGetANumber", PpcImportDispatcherTarget::LMGetANumber),
            ("lmgetanumber", PpcImportDispatcherTarget::LMGetANumber),
            ("LMSetDABeeper", PpcImportDispatcherTarget::LMSetDABeeper),
            ("lmsetdabeeper", PpcImportDispatcherTarget::LMSetDABeeper),
            ("LMGetDABeeper", PpcImportDispatcherTarget::LMGetDABeeper),
            ("lmgetdabeeper", PpcImportDispatcherTarget::LMGetDABeeper),
            ("LMGetDAStrings", PpcImportDispatcherTarget::LMGetDAStrings),
            ("lmgetdastrings", PpcImportDispatcherTarget::LMGetDAStrings),
            ("LMSetDlgFont", PpcImportDispatcherTarget::LMSetDlgFont),
            ("lmsetdlgfont", PpcImportDispatcherTarget::LMSetDlgFont),
            ("LMGetDlgFont", PpcImportDispatcherTarget::LMGetDlgFont),
            ("lmgetdlgfont", PpcImportDispatcherTarget::LMGetDlgFont),
            (
                "GetDialogItemAsControl",
                PpcImportDispatcherTarget::GetDialogItemAsControl,
            ),
            (
                "getdialogitemascontrol",
                PpcImportDispatcherTarget::GetDialogItemAsControl,
            ),
        ] {
            assert_eq!(
                dispatcher_target_for_import(lib, symbol),
                expected_target,
                "classification failed for {lib}::{symbol}",
            );
        }
    }
    for (lib, symbol, expected_target) in [
        ("InterfaceLib", "GetDialogPort", PpcImportDispatcherTarget::GetDialogPort),
        ("AppearanceLib", "GetDialogPort", PpcImportDispatcherTarget::GetDialogPort),
        ("DialogsLib", "GetDialogPort", PpcImportDispatcherTarget::GetDialogPort),
        ("CarbonLib", "GetDialogPort", PpcImportDispatcherTarget::GetDialogPort),
        ("InterfaceLib", "GetDialogWindow", PpcImportDispatcherTarget::GetDialogWindow),
        ("AppearanceLib", "GetDialogWindow", PpcImportDispatcherTarget::GetDialogWindow),
        ("DialogsLib", "GetDialogWindow", PpcImportDispatcherTarget::GetDialogWindow),
        ("CarbonLib", "GetDialogWindow", PpcImportDispatcherTarget::GetDialogWindow),
        ("InterfaceLib", "GetDialogFromWindow", PpcImportDispatcherTarget::GetDialogFromWindow),
        ("AppearanceLib", "GetDialogFromWindow", PpcImportDispatcherTarget::GetDialogFromWindow),
        ("DialogsLib", "GetDialogFromWindow", PpcImportDispatcherTarget::GetDialogFromWindow),
        ("CarbonLib", "GetDialogFromWindow", PpcImportDispatcherTarget::GetDialogFromWindow),
        ("InterfaceLib", "SetPortDialogPort", PpcImportDispatcherTarget::SetPortDialogPort),
        ("AppearanceLib", "SetPortDialogPort", PpcImportDispatcherTarget::SetPortDialogPort),
        ("DialogsLib", "SetPortDialogPort", PpcImportDispatcherTarget::SetPortDialogPort),
        ("CarbonLib", "SetPortDialogPort", PpcImportDispatcherTarget::SetPortDialogPort),
        ("InterfaceLib", "DialogCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("AppearanceLib", "DialogCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("DialogsLib", "DialogCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("CarbonLib", "DialogCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("InterfaceLib", "DlgCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("AppearanceLib", "DlgCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("DialogsLib", "DlgCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("CarbonLib", "DlgCut", PpcImportDispatcherTarget::TECopy { cut: true, dialog: true }),
        ("InterfaceLib", "DialogCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("AppearanceLib", "DialogCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("DialogsLib", "DialogCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("CarbonLib", "DialogCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("InterfaceLib", "DlgCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("AppearanceLib", "DlgCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("DialogsLib", "DlgCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("CarbonLib", "DlgCopy", PpcImportDispatcherTarget::TECopy { cut: false, dialog: true }),
        ("InterfaceLib", "DialogPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("AppearanceLib", "DialogPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("DialogsLib", "DialogPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("CarbonLib", "DialogPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("InterfaceLib", "DlgPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("AppearanceLib", "DlgPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("DialogsLib", "DlgPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("CarbonLib", "DlgPaste", PpcImportDispatcherTarget::TEPaste { dialog: true }),
        ("InterfaceLib", "DialogDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("AppearanceLib", "DialogDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("DialogsLib", "DialogDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("CarbonLib", "DialogDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("InterfaceLib", "DlgDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("AppearanceLib", "DlgDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("DialogsLib", "DlgDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
        ("CarbonLib", "DlgDelete", PpcImportDispatcherTarget::TEDelete { dialog: true }),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
            expected_target,
        );
    }
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
    for (lib, symbol, operation) in [
        ("InterfaceLib", "AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("AppearanceLib", "AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("DialogsLib", "AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("CarbonLib", "AppendDITL", PpcDialogCompatibilityOperation::AppendDitl),
        ("InterfaceLib", "AppendDitl", PpcDialogCompatibilityOperation::AppendDitl),
        ("AppearanceLib", "AppendDitl", PpcDialogCompatibilityOperation::AppendDitl),
        ("DialogsLib", "AppendDitl", PpcDialogCompatibilityOperation::AppendDitl),
        ("CarbonLib", "AppendDitl", PpcDialogCompatibilityOperation::AppendDitl),
        ("InterfaceLib", "CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        ("AppearanceLib", "CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        ("DialogsLib", "CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        ("CarbonLib", "CountDITL", PpcDialogCompatibilityOperation::CountDitl),
        ("InterfaceLib", "CountDitl", PpcDialogCompatibilityOperation::CountDitl),
        ("AppearanceLib", "CountDitl", PpcDialogCompatibilityOperation::CountDitl),
        ("DialogsLib", "CountDitl", PpcDialogCompatibilityOperation::CountDitl),
        ("CarbonLib", "CountDitl", PpcDialogCompatibilityOperation::CountDitl),
        ("InterfaceLib", "ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        ("AppearanceLib", "ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        ("DialogsLib", "ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        ("CarbonLib", "ShortenDITL", PpcDialogCompatibilityOperation::ShortenDitl),
        ("InterfaceLib", "ShortenDitl", PpcDialogCompatibilityOperation::ShortenDitl),
        ("AppearanceLib", "ShortenDitl", PpcDialogCompatibilityOperation::ShortenDitl),
        ("DialogsLib", "ShortenDitl", PpcDialogCompatibilityOperation::ShortenDitl),
        ("CarbonLib", "ShortenDitl", PpcDialogCompatibilityOperation::ShortenDitl),
        ("InterfaceLib", "FindDialogItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("AppearanceLib", "FindDialogItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("DialogsLib", "FindDialogItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("CarbonLib", "FindDialogItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("InterfaceLib", "FindDItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("AppearanceLib", "FindDItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("DialogsLib", "FindDItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("CarbonLib", "FindDItem", PpcDialogCompatibilityOperation::FindDialogItem),
        ("InterfaceLib", "HideDialogItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("AppearanceLib", "HideDialogItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("DialogsLib", "HideDialogItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("CarbonLib", "HideDialogItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("InterfaceLib", "HideDItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("AppearanceLib", "HideDItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("DialogsLib", "HideDItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("CarbonLib", "HideDItem", PpcDialogCompatibilityOperation::HideDialogItem),
        ("InterfaceLib", "ShowDialogItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("AppearanceLib", "ShowDialogItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("DialogsLib", "ShowDialogItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("CarbonLib", "ShowDialogItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("InterfaceLib", "ShowDItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("AppearanceLib", "ShowDItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("DialogsLib", "ShowDItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("CarbonLib", "ShowDItem", PpcDialogCompatibilityOperation::ShowDialogItem),
        ("InterfaceLib", "UpdateDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("AppearanceLib", "UpdateDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("DialogsLib", "UpdateDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("CarbonLib", "UpdateDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("InterfaceLib", "UpdtDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("AppearanceLib", "UpdtDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("DialogsLib", "UpdtDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("CarbonLib", "UpdtDialog", PpcDialogCompatibilityOperation::UpdateDialog),
        ("InterfaceLib", "DialogSelect", PpcDialogCompatibilityOperation::DialogSelect),
        ("AppearanceLib", "DialogSelect", PpcDialogCompatibilityOperation::DialogSelect),
        ("DialogsLib", "DialogSelect", PpcDialogCompatibilityOperation::DialogSelect),
        ("CarbonLib", "DialogSelect", PpcDialogCompatibilityOperation::DialogSelect),
        ("InterfaceLib", "IsDialogEvent", PpcDialogCompatibilityOperation::IsDialogEvent),
        ("AppearanceLib", "IsDialogEvent", PpcDialogCompatibilityOperation::IsDialogEvent),
        ("DialogsLib", "IsDialogEvent", PpcDialogCompatibilityOperation::IsDialogEvent),
        ("CarbonLib", "IsDialogEvent", PpcDialogCompatibilityOperation::IsDialogEvent),
        ("InterfaceLib", "GetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("AppearanceLib", "GetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("DialogsLib", "GetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("CarbonLib", "GetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("InterfaceLib", "getdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("AppearanceLib", "getdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("DialogsLib", "getdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("CarbonLib", "getdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem),
        ("InterfaceLib", "SetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("AppearanceLib", "SetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("DialogsLib", "SetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("CarbonLib", "SetDialogKeyboardFocusItem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("InterfaceLib", "setdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("AppearanceLib", "setdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("DialogsLib", "setdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("CarbonLib", "setdialogkeyboardfocusitem", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("InterfaceLib", "SetDialogKeyboardFocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("AppearanceLib", "SetDialogKeyboardFocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("DialogsLib", "SetDialogKeyboardFocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("CarbonLib", "SetDialogKeyboardFocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("InterfaceLib", "setdialogkeyboardfocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("AppearanceLib", "setdialogkeyboardfocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("DialogsLib", "setdialogkeyboardfocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("CarbonLib", "setdialogkeyboardfocus", PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem),
        ("InterfaceLib", "GetDialogTextEditHandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("AppearanceLib", "GetDialogTextEditHandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("DialogsLib", "GetDialogTextEditHandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("CarbonLib", "GetDialogTextEditHandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("InterfaceLib", "getdialogtextedithandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("AppearanceLib", "getdialogtextedithandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("DialogsLib", "getdialogtextedithandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("CarbonLib", "getdialogtextedithandle", PpcDialogCompatibilityOperation::GetDialogTextEditHandle),
        ("InterfaceLib", "GetParamText", PpcDialogCompatibilityOperation::GetParamText),
        ("AppearanceLib", "GetParamText", PpcDialogCompatibilityOperation::GetParamText),
        ("DialogsLib", "GetParamText", PpcDialogCompatibilityOperation::GetParamText),
        ("CarbonLib", "GetParamText", PpcDialogCompatibilityOperation::GetParamText),
        ("InterfaceLib", "getparamtext", PpcDialogCompatibilityOperation::GetParamText),
        ("AppearanceLib", "getparamtext", PpcDialogCompatibilityOperation::GetParamText),
        ("DialogsLib", "getparamtext", PpcDialogCompatibilityOperation::GetParamText),
        ("CarbonLib", "getparamtext", PpcDialogCompatibilityOperation::GetParamText),
        ("InterfaceLib", "SetDialogTimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("AppearanceLib", "SetDialogTimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("DialogsLib", "SetDialogTimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("CarbonLib", "SetDialogTimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("InterfaceLib", "setdialogtimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("AppearanceLib", "setdialogtimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("DialogsLib", "setdialogtimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("CarbonLib", "setdialogtimeout", PpcDialogCompatibilityOperation::SetDialogTimeout),
        ("InterfaceLib", "GetDialogTimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("AppearanceLib", "GetDialogTimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("DialogsLib", "GetDialogTimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("CarbonLib", "GetDialogTimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("InterfaceLib", "getdialogtimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("AppearanceLib", "getdialogtimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("DialogsLib", "getdialogtimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("CarbonLib", "getdialogtimeout", PpcDialogCompatibilityOperation::GetDialogTimeout),
        ("InterfaceLib", "CloseStandardSheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("AppearanceLib", "CloseStandardSheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("DialogsLib", "CloseStandardSheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("CarbonLib", "CloseStandardSheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("InterfaceLib", "closestandardsheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("AppearanceLib", "closestandardsheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("DialogsLib", "closestandardsheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("CarbonLib", "closestandardsheet", PpcDialogCompatibilityOperation::CloseStandardSheet),
        ("InterfaceLib", "CreateStandardAlert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("AppearanceLib", "CreateStandardAlert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("DialogsLib", "CreateStandardAlert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("CarbonLib", "CreateStandardAlert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("InterfaceLib", "createstandardalert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("AppearanceLib", "createstandardalert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("DialogsLib", "createstandardalert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("CarbonLib", "createstandardalert", PpcDialogCompatibilityOperation::CreateStandardAlert),
        ("InterfaceLib", "CreateStandardSheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("AppearanceLib", "CreateStandardSheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("DialogsLib", "CreateStandardSheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("CarbonLib", "CreateStandardSheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("InterfaceLib", "createstandardsheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("AppearanceLib", "createstandardsheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("DialogsLib", "createstandardsheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("CarbonLib", "createstandardsheet", PpcDialogCompatibilityOperation::CreateStandardSheet),
        ("InterfaceLib", "FlashDialogControl", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("AppearanceLib", "FlashDialogControl", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("DialogsLib", "FlashDialogControl", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("CarbonLib", "FlashDialogControl", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("InterfaceLib", "flashdialogcontrol", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("AppearanceLib", "flashdialogcontrol", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("DialogsLib", "flashdialogcontrol", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("CarbonLib", "flashdialogcontrol", PpcDialogCompatibilityOperation::FlashDialogControl),
        ("InterfaceLib", "GetDialogItemInit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("AppearanceLib", "GetDialogItemInit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("DialogsLib", "GetDialogItemInit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("CarbonLib", "GetDialogItemInit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("InterfaceLib", "getdialogiteminit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("AppearanceLib", "getdialogiteminit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("DialogsLib", "getdialogiteminit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("CarbonLib", "getdialogiteminit", PpcDialogCompatibilityOperation::GetDialogItemInit),
        ("InterfaceLib", "GetModalDialogEventMask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("AppearanceLib", "GetModalDialogEventMask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("DialogsLib", "GetModalDialogEventMask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("CarbonLib", "GetModalDialogEventMask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("InterfaceLib", "getmodaldialogeventmask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("AppearanceLib", "getmodaldialogeventmask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("DialogsLib", "getmodaldialogeventmask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("CarbonLib", "getmodaldialogeventmask", PpcDialogCompatibilityOperation::GetModalDialogEventMask),
        ("InterfaceLib", "GetStandardAlertDefaultParams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("AppearanceLib", "GetStandardAlertDefaultParams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("DialogsLib", "GetStandardAlertDefaultParams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("CarbonLib", "GetStandardAlertDefaultParams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("InterfaceLib", "getstandardalertdefaultparams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("AppearanceLib", "getstandardalertdefaultparams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("DialogsLib", "getstandardalertdefaultparams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("CarbonLib", "getstandardalertdefaultparams", PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams),
        ("InterfaceLib", "RunStandardAlert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("AppearanceLib", "RunStandardAlert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("DialogsLib", "RunStandardAlert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("CarbonLib", "RunStandardAlert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("InterfaceLib", "runstandardalert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("AppearanceLib", "runstandardalert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("DialogsLib", "runstandardalert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("CarbonLib", "runstandardalert", PpcDialogCompatibilityOperation::RunStandardAlert),
        ("InterfaceLib", "SetDialogFilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("AppearanceLib", "SetDialogFilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("DialogsLib", "SetDialogFilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("CarbonLib", "SetDialogFilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("InterfaceLib", "setdialogfilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("AppearanceLib", "setdialogfilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("DialogsLib", "setdialogfilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("CarbonLib", "setdialogfilter", PpcDialogCompatibilityOperation::SetDialogFilter),
        ("InterfaceLib", "SetModalDialogEventMask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("AppearanceLib", "SetModalDialogEventMask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("DialogsLib", "SetModalDialogEventMask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("CarbonLib", "SetModalDialogEventMask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("InterfaceLib", "setmodaldialogeventmask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("AppearanceLib", "setmodaldialogeventmask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("DialogsLib", "setmodaldialogeventmask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("CarbonLib", "setmodaldialogeventmask", PpcDialogCompatibilityOperation::SetModalDialogEventMask),
        ("InterfaceLib", "AutoPositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("AppearanceLib", "AutoPositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("DialogsLib", "AutoPositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("CarbonLib", "AutoPositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("InterfaceLib", "autopositiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("AppearanceLib", "autopositiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("DialogsLib", "autopositiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("CarbonLib", "autopositiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("InterfaceLib", "PositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("AppearanceLib", "PositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("DialogsLib", "PositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("CarbonLib", "PositionDialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("InterfaceLib", "positiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("AppearanceLib", "positiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("DialogsLib", "positiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("CarbonLib", "positiondialog", PpcDialogCompatibilityOperation::AutoPositionDialog),
        ("InterfaceLib", "GetDialogTracksCursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("AppearanceLib", "GetDialogTracksCursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("DialogsLib", "GetDialogTracksCursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("CarbonLib", "GetDialogTracksCursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("InterfaceLib", "getdialogtrackscursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("AppearanceLib", "getdialogtrackscursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("DialogsLib", "getdialogtrackscursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("CarbonLib", "getdialogtrackscursor", PpcDialogCompatibilityOperation::GetDialogTracksCursor),
        ("InterfaceLib", "IsDialogTracksCursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("AppearanceLib", "IsDialogTracksCursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("DialogsLib", "IsDialogTracksCursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("CarbonLib", "IsDialogTracksCursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("InterfaceLib", "isdialogtrackscursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("AppearanceLib", "isdialogtrackscursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("DialogsLib", "isdialogtrackscursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("CarbonLib", "isdialogtrackscursor", PpcDialogCompatibilityOperation::IsDialogTracksCursor),
        ("InterfaceLib", "GetDialogFilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("AppearanceLib", "GetDialogFilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("DialogsLib", "GetDialogFilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("CarbonLib", "GetDialogFilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("InterfaceLib", "getdialogfilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("AppearanceLib", "getdialogfilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("DialogsLib", "getdialogfilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("CarbonLib", "getdialogfilter", PpcDialogCompatibilityOperation::GetDialogFilter),
        ("InterfaceLib", "ShowSheetWindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("AppearanceLib", "ShowSheetWindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("DialogsLib", "ShowSheetWindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("CarbonLib", "ShowSheetWindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("InterfaceLib", "showsheetwindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("AppearanceLib", "showsheetwindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("DialogsLib", "showsheetwindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("CarbonLib", "showsheetwindow", PpcDialogCompatibilityOperation::ShowSheetWindow),
        ("InterfaceLib", "HideSheetWindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("AppearanceLib", "HideSheetWindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("DialogsLib", "HideSheetWindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("CarbonLib", "HideSheetWindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("InterfaceLib", "hidesheetwindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("AppearanceLib", "hidesheetwindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("DialogsLib", "hidesheetwindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("CarbonLib", "hidesheetwindow", PpcDialogCompatibilityOperation::HideSheetWindow),
        ("InterfaceLib", "GetSheetWindowParent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("AppearanceLib", "GetSheetWindowParent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("DialogsLib", "GetSheetWindowParent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("CarbonLib", "GetSheetWindowParent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("InterfaceLib", "getsheetwindowparent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("AppearanceLib", "getsheetwindowparent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("DialogsLib", "getsheetwindowparent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("CarbonLib", "getsheetwindowparent", PpcDialogCompatibilityOperation::GetSheetWindowParent),
        ("InterfaceLib", "InsertDialogItem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("AppearanceLib", "InsertDialogItem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("DialogsLib", "InsertDialogItem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("CarbonLib", "InsertDialogItem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("InterfaceLib", "insertdialogitem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("AppearanceLib", "insertdialogitem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("DialogsLib", "insertdialogitem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("CarbonLib", "insertdialogitem", PpcDialogCompatibilityOperation::InsertDialogItem),
        ("InterfaceLib", "RemoveDialogItems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("AppearanceLib", "RemoveDialogItems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("DialogsLib", "RemoveDialogItems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("CarbonLib", "RemoveDialogItems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("InterfaceLib", "removedialogitems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("AppearanceLib", "removedialogitems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("DialogsLib", "removedialogitems", PpcDialogCompatibilityOperation::RemoveDialogItems),
        ("CarbonLib", "removedialogitems", PpcDialogCompatibilityOperation::RemoveDialogItems),
    ] {
        assert_eq!(
            dispatcher_target_for_import(lib, symbol),
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
    loaded.memory.write_u16_be(addr::DLG_FONT, 5).unwrap();
    for i in 0..4u32 {
        loaded.memory.write_u32_be(addr::DA_STRINGS + i * 4, 0x00D0_0000 | i).unwrap();
    }
    loaded.param_text.set_slot(0, b"InitialParamText".to_vec());
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
    assert_eq!(
        loaded.memory.read_u16_be(addr::DLG_FONT),
        Some(0)
    );
    for i in 0..4u32 {
        assert_eq!(loaded.memory.read_u32_be(addr::DA_STRINGS + i * 4), Some(0));
    }
    assert!(loaded.param_text.is_empty());
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
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_TX_FONT_OFFSET),
        Some(0)
    );
}

#[test]
fn dialog_and_alert_creation_initializes_tx_font_from_dlg_font() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    use crate::memory::globals::addr;
    loaded.memory.write_u16_be(addr::DLG_FONT, 4).unwrap(); // Monaco font (4)

    let mut dlog = vec![0; 22];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&200i16.to_be_bytes());
    dlog[10] = 1;
    dlog[12] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    dlog[20] = 1;
    dlog[21] = b'T';
    let mut ditl = vec![0; 22];
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&10i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&26i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&190i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_STATIC_TEXT;
    ditl[15] = 4;
    ditl[16..20].copy_from_slice(b"Font");
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
    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_TX_FONT_OFFSET),
        Some(4),
        "GetNewDialog must initialize txFont from low-memory DlgFont ($0AFA)"
    );
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

    let mut loaded_get_anumber_neg = load_pef_application(&pef_get_anumber).unwrap();
    loaded_get_anumber_neg
        .memory
        .write_u16_be(addr::ANUMBER, 0xFF80)
        .unwrap();
    let probe = loaded_get_anumber_neg.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get_anumber_neg.cpu.gpr[3], ppc_i16_result(-128));
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

    let mut loaded_get_neg = load_pef_application(&pef_get).unwrap();
    loaded_get_neg
        .memory
        .write_u16_be(addr::DLG_FONT, 0xFFFF)
        .unwrap();
    let probe = loaded_get_neg.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get_neg.cpu.gpr[3], ppc_i16_result(-1));
}

#[test]
fn lm_resumeproc_and_dastrings_accessors_manage_dialog_globals() {
    use crate::memory::globals::addr;

    // LMSetResumeProc
    let pef_set = synthetic_pef_with_library_import(b"InterfaceLib", b"LMSetResumeProc");
    let mut loaded_set = load_pef_application(&pef_set).unwrap();
    loaded_set.cpu.gpr[3] = 0x0034_5678;
    let probe = loaded_set.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded_set.memory.read_u32_be(addr::RESUME_PROC),
        Some(0x0034_5678)
    );

    // LMGetResumeProc
    let pef_get = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetResumeProc");
    let mut loaded_get = load_pef_application(&pef_get).unwrap();
    loaded_get
        .memory
        .write_u32_be(addr::RESUME_PROC, 0x0034_5678)
        .unwrap();
    let probe = loaded_get.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get.cpu.gpr[3], 0x0034_5678);

    // LMGetDAStrings
    let pef_dastrings = synthetic_pef_with_library_import(b"InterfaceLib", b"LMGetDAStrings");
    let mut loaded_dastrings = load_pef_application(&pef_dastrings).unwrap();
    let probe = loaded_dastrings.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_dastrings.cpu.gpr[3], addr::DA_STRINGS);
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

#[test]
fn auto_size_dialog_resizes_bounds_and_rejects_nil_dialog() {
    let pef = synthetic_pef_with_library_import(b"AppearanceLib", b"AutoSizeDialog");
    let mut loaded = load_pef_application(&pef).unwrap();

    let mut ditl = vec![0; 36];
    ditl[0..2].copy_from_slice(&1i16.to_be_bytes()); // 2 items (count - 1 = 1)
    // Item 1: rect (10, 20, 50, 100), Button "OK"
    ditl[6..8].copy_from_slice(&10i16.to_be_bytes());
    ditl[8..10].copy_from_slice(&20i16.to_be_bytes());
    ditl[10..12].copy_from_slice(&50i16.to_be_bytes());
    ditl[12..14].copy_from_slice(&100i16.to_be_bytes());
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
    // Item 2: rect (60, 20, 90, 180), Button "More"
    ditl[22..24].copy_from_slice(&60i16.to_be_bytes());
    ditl[24..26].copy_from_slice(&20i16.to_be_bytes());
    ditl[26..28].copy_from_slice(&90i16.to_be_bytes());
    ditl[28..30].copy_from_slice(&180i16.to_be_bytes());
    ditl[30] = PPC_DIALOG_ITEM_BUTTON;
    ditl[31] = 4;
    ditl[32..36].copy_from_slice(b"More");

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

    // Initial gworld dimensions: 300x400
    if let Some(gw) = loaded.gworlds.iter_mut().find(|gw| gw.port == PPC_MAIN_GWORLD) {
        gw.height = 300;
        gw.width = 400;
    }
    let _ = ppc_write_rect(&mut loaded.memory, PPC_MAIN_GWORLD + 16, 0, 0, 300, 400);

    // 1. Valid call: AutoSizeDialog(PPC_MAIN_GWORLD)
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    // Bounds tightened to enclosing items: bottom=90, right=180
    let gw = loaded.gworlds.iter().find(|gw| gw.port == PPC_MAIN_GWORLD).unwrap();
    assert_eq!(gw.height, 90);
    assert_eq!(gw.width, 180);

    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 20), Some(90));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_GWORLD + 22), Some(180));

    // 2. Reject NIL dialog
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn get_alert_stage_and_set_dialog_font_dispatch_with_canonical_evaluation() {
    use crate::memory::globals::addr;

    // 1. SetDialogFont writes DLG_FONT
    let pef_set = synthetic_pef_with_library_import(b"InterfaceLib", b"SetDialogFont");
    let mut loaded_set = load_pef_application(&pef_set).unwrap();
    loaded_set.cpu.gpr[3] = 4;
    let probe = loaded_set.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_set.memory.read_u16_be(addr::DLG_FONT), Some(4));

    // 2. SetDAFont writes DLG_FONT
    let pef_da = synthetic_pef_with_library_import(b"InterfaceLib", b"SetDAFont");
    let mut loaded_da = load_pef_application(&pef_da).unwrap();
    loaded_da.cpu.gpr[3] = 12;
    let probe = loaded_da.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_da.memory.read_u16_be(addr::DLG_FONT), Some(12));

    // 3. GetAlertStage reads ALERT_STAGE
    let pef_get_stage = synthetic_pef_with_library_import(b"InterfaceLib", b"GetAlertStage");
    let mut loaded_get_stage = load_pef_application(&pef_get_stage).unwrap();
    loaded_get_stage
        .memory
        .write_u16_be(addr::ALERT_STAGE, 2)
        .unwrap();
    let probe = loaded_get_stage.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_get_stage.cpu.gpr[3], 2);
}

#[test]
fn get_dialog_port_and_window_accessors_manage_dialog_references() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let window_ptr = PPC_DATA_BASE + 0x2000;

    // 1. GetDialogPort returns dialog pointer, or 0 for NULL
    let pef_port = synthetic_pef_with_library_import(b"InterfaceLib", b"GetDialogPort");
    let mut loaded_port = load_pef_application(&pef_port).unwrap();
    loaded_port.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_port.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_port.cpu.gpr[3], dialog_ptr);

    let mut loaded_null_port = load_pef_application(&pef_port).unwrap();
    loaded_null_port.cpu.gpr[3] = 0;
    let probe = loaded_null_port.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_null_port.cpu.gpr[3], 0);

    // 2. GetDialogWindow returns dialog pointer, or 0 for NULL
    let pef_window = synthetic_pef_with_library_import(b"AppearanceLib", b"GetDialogWindow");
    let mut loaded_window = load_pef_application(&pef_window).unwrap();
    loaded_window.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_window.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_window.cpu.gpr[3], dialog_ptr);

    let mut loaded_null_window = load_pef_application(&pef_window).unwrap();
    loaded_null_window.cpu.gpr[3] = 0;
    let probe = loaded_null_window.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_null_window.cpu.gpr[3], 0);

    // 3. GetDialogFromWindow: validates windowKind == dialogKind (2)
    let pef_from_window = synthetic_pef_with_library_import(b"CarbonLib", b"GetDialogFromWindow");

    // NULL window returns 0
    let mut loaded_null_win = load_pef_application(&pef_from_window).unwrap();
    loaded_null_win.cpu.gpr[3] = 0;
    let probe = loaded_null_win.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_null_win.cpu.gpr[3], 0);

    // Non-dialog window (windowKind == 8) returns 0
    let mut loaded_doc_win = load_pef_application(&pef_from_window).unwrap();
    loaded_doc_win.memory.add_region(window_ptr, vec![0; 256]);
    loaded_doc_win
        .memory
        .write_u16_be(
            window_ptr + crate::dialog_manager::DIALOG_WINDOW_KIND_OFFSET,
            8,
        )
        .unwrap();
    loaded_doc_win.cpu.gpr[3] = window_ptr;
    let probe = loaded_doc_win.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_doc_win.cpu.gpr[3], 0);

    // Dialog window (windowKind == 2) returns dialog pointer
    let mut loaded_dlg_win = load_pef_application(&pef_from_window).unwrap();
    loaded_dlg_win.memory.add_region(dialog_ptr, vec![0; 256]);
    loaded_dlg_win
        .memory
        .write_u16_be(
            dialog_ptr + crate::dialog_manager::DIALOG_WINDOW_KIND_OFFSET,
            crate::dialog_manager::DIALOG_WINDOW_KIND,
        )
        .unwrap();
    loaded_dlg_win.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_dlg_win.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_dlg_win.cpu.gpr[3], dialog_ptr);

    // 4. SetPortDialogPort makes dialog port current
    let pef_set_port = synthetic_pef_with_library_import(b"DialogsLib", b"SetPortDialogPort");
    let mut loaded_set_port = load_pef_application(&pef_set_port).unwrap();
    loaded_set_port.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_set_port.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(*loaded_set_port.current_gworld, dialog_ptr);
}

#[test]
fn dialog_clipboard_editing_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let rects = PPC_DATA_BASE + 0x2000;

    // 1. Safe no-ops on NULL dialog, inactive edit field, and NULL text handle
    let pef_cut = synthetic_pef_with_library_import(b"InterfaceLib", b"DialogCut");
    let mut loaded_cut = load_pef_application(&pef_cut).unwrap();
    loaded_cut.memory.add_region(dialog_ptr, vec![0; 256]);
    loaded_cut.memory.add_region(rects, vec![0; 64]);
    ppc_write_rect(&mut loaded_cut.memory, rects, 0, 0, 80, 200).unwrap();
    ppc_write_rect(&mut loaded_cut.memory, rects + 8, 0, 0, 80, 200).unwrap();

    // 1a. NULL dialog pointer
    loaded_cut.cpu.gpr[3] = 0;
    let probe = loaded_cut.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    // 1b. Inactive edit field (editField == -1 / 0xFFFF)
    loaded_cut
        .memory
        .write_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET, 0xFFFF)
        .unwrap();
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_cut.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    // 1c. Active edit field (editField == 0), but NULL text handle
    loaded_cut
        .memory
        .write_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET, 0)
        .unwrap();
    loaded_cut
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_TEXT_HANDLE_OFFSET, 0)
        .unwrap();
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    let probe = loaded_cut.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    // 2. Active editing workflow: Copy, Paste, Cut, Delete with legacy aliases
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded_cut.memory,
        test_heap_cursor!(loaded_cut),
        test_heap_limit!(loaded_cut),
        &mut last_mem_error,
        test_handles!(loaded_cut),
        rects,
        rects + 8,
        PPC_MAIN_GWORLD,
        0,
        PPC_QD_TEXT_MODE_SRC_OR,
        12,
        PPC_RGB_BLACK,
        false,
    );
    ppc_te_set_text(
        None,
        &mut loaded_cut.memory,
        test_heap_cursor!(loaded_cut),
        test_heap_limit!(loaded_cut),
        &mut last_mem_error,
        test_handles!(loaded_cut),
        te_handle,
        b"Systemless",
    );
    loaded_cut
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_TEXT_HANDLE_OFFSET, te_handle)
        .unwrap();

    let te_ptr = loaded_cut.memory.read_u32_be(te_handle).unwrap();
    // Select first 6 bytes: "System"
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0)
        .unwrap();
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, 6)
        .unwrap();

    // 2a. DialogCopy copies selection to scrap without modifying text
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded_cut,
        PpcImportDispatcherTarget::TECopy {
            cut: false,
            dialog: true,
        },
    );
    assert_eq!(ppc_te_scrap_bytes(&mut loaded_cut.memory), b"System");
    assert_eq!(
        ppc_te_text_bytes(
            &mut loaded_cut.memory,
            &test_handle_records!(loaded_cut),
            te_handle
        ),
        Some(b"Systemless".to_vec())
    );

    // 2b. DialogCut removes selected text and copies to scrap
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded_cut,
        PpcImportDispatcherTarget::TECopy {
            cut: true,
            dialog: true,
        },
    );
    assert_eq!(ppc_te_scrap_bytes(&mut loaded_cut.memory), b"System");
    assert_eq!(
        ppc_te_text_bytes(
            &mut loaded_cut.memory,
            &test_handle_records!(loaded_cut),
            te_handle
        ),
        Some(b"less".to_vec())
    );

    // 2c. DialogPaste pastes scrap ("System") at insertion point
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0)
        .unwrap();
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, 0)
        .unwrap();
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded_cut,
        PpcImportDispatcherTarget::TEPaste { dialog: true },
    );
    assert_eq!(
        ppc_te_text_bytes(
            &mut loaded_cut.memory,
            &test_handle_records!(loaded_cut),
            te_handle
        ),
        Some(b"Systemless".to_vec())
    );

    // 2d. DialogDelete removes selection without modifying scrap
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 6)
        .unwrap();
    loaded_cut
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, 10)
        .unwrap();
    loaded_cut.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded_cut,
        PpcImportDispatcherTarget::TEDelete { dialog: true },
    );
    assert_eq!(
        ppc_te_text_bytes(
            &mut loaded_cut.memory,
            &test_handle_records!(loaded_cut),
            te_handle
        ),
        Some(b"System".to_vec())
    );
    // Scrap still contains "System", untouched by DialogDelete!
    assert_eq!(ppc_te_scrap_bytes(&mut loaded_cut.memory), b"System");

    // 3. Verify PEF execution using legacy Dlg* aliases from AppearanceLib and CarbonLib
    let pef_dlg_delete = synthetic_pef_with_library_import(b"CarbonLib", b"DlgDelete");
    let mut loaded_dlg_del = load_pef_application(&pef_dlg_delete).unwrap();
    loaded_dlg_del.cpu.gpr[3] = 0;
    let probe = loaded_dlg_del.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_dlg_paste = synthetic_pef_with_library_import(b"AppearanceLib", b"DlgPaste");
    let mut loaded_dlg_paste = load_pef_application(&pef_dlg_paste).unwrap();
    loaded_dlg_paste.cpu.gpr[3] = 0;
    let probe = loaded_dlg_paste.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_dlg_copy = synthetic_pef_with_library_import(b"DialogsLib", b"DlgCopy");
    let mut loaded_dlg_copy = load_pef_application(&pef_dlg_copy).unwrap();
    loaded_dlg_copy.cpu.gpr[3] = 0;
    let probe = loaded_dlg_copy.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_dlg_cut = synthetic_pef_with_library_import(b"InterfaceLib", b"DlgCut");
    let mut loaded_dlg_cut = load_pef_application(&pef_dlg_cut).unwrap();
    loaded_dlg_cut.cpu.gpr[3] = 0;
    let probe = loaded_dlg_cut.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

fn make_test_ditl_item_typed(
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    item_type: u8,
    title: &[u8],
) -> Vec<u8> {
    let mut item = Vec::new();
    item.extend_from_slice(&0u32.to_be_bytes()); // handle placeholder
    item.extend_from_slice(&top.to_be_bytes());
    item.extend_from_slice(&left.to_be_bytes());
    item.extend_from_slice(&bottom.to_be_bytes());
    item.extend_from_slice(&right.to_be_bytes());
    item.push(item_type);
    item.push(title.len() as u8);
    item.extend_from_slice(title);
    if title.len() % 2 != 0 {
        item.push(0); // word alignment pad
    }
    item
}

fn make_test_ditl_item(top: i16, left: i16, bottom: i16, right: i16, title: &[u8]) -> Vec<u8> {
    make_test_ditl_item_typed(top, left, bottom, right, PPC_DIALOG_ITEM_BUTTON, title)
}

fn make_test_ditl(items: &[Vec<u8>]) -> Vec<u8> {
    let count_minus_one = items.len().saturating_sub(1) as i16;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&count_minus_one.to_be_bytes());
    for item in items {
        bytes.extend_from_slice(item);
    }
    bytes
}

#[test]
fn ditl_manipulation_and_query_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"CountDITL");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);

    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: dialog_ptr,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 300,
        height: 200,
        depth: 8,
        row_bytes: 300,
        pixels_locked: false,
        pixels_no_purge: false,
    });

    // 1. Safe no-ops and zero counts on empty or NULL dialogs
    // 1a. NULL dialog pointer
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 1b. Non-null dialog, but items_handle == 0
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 1c. AppendDITL with NULL dialog or NULL ditl_handle is a safe no-op
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::AppendDitl),
    );
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::AppendDitl),
    );

    // 1d. ShortenDITL with NULL dialog is a safe no-op
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::ShortenDitl),
    );

    // 2. Install initial DITL with 2 items ("OK", "Cancel")
    let item1 = make_test_ditl_item(10, 10, 30, 80, b"OK");
    let item2 = make_test_ditl_item(10, 90, 30, 160, b"Cancel");
    let ditl_bytes = make_test_ditl(&[item1, item2]);
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl_bytes,
    );
    loaded
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 2a. CountDITL reports 2 items
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 2);

    // 3. AppendDITL: Append 1 item ("Help")
    let item3 = make_test_ditl_item(10, 170, 30, 240, b"Help");
    let append_ditl_bytes = make_test_ditl(&[item3]);
    let append_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &append_ditl_bytes,
    );

    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = append_handle;
    loaded.cpu.gpr[5] = 0; // overlayDITL
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::AppendDitl),
    );

    // 3a. CountDITL reports 3 items after AppendDITL
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 3);

    // 4. ShortenDITL: Remove 1 item from the end
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::ShortenDitl),
    );

    // 4a. CountDITL reports 2 items
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 2);

    // 4b. ShortenDITL: Remove remaining 2 items
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::ShortenDitl),
    );

    // 4c. CountDITL reports 0 items
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::CountDitl),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 5. Verify PEF execution using mixed-case aliases from AppearanceLib, DialogsLib, and CarbonLib
    let pef_count = synthetic_pef_with_library_import(b"AppearanceLib", b"CountDitl");
    let mut loaded_count = load_pef_application(&pef_count).unwrap();
    loaded_count.cpu.gpr[3] = 0;
    let probe = loaded_count.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded_count.cpu.gpr[3], 0);

    let pef_append = synthetic_pef_with_library_import(b"DialogsLib", b"AppendDitl");
    let mut loaded_append = load_pef_application(&pef_append).unwrap();
    loaded_append.cpu.gpr[3] = 0;
    let probe = loaded_append.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_shorten = synthetic_pef_with_library_import(b"CarbonLib", b"ShortenDitl");
    let mut loaded_shorten = load_pef_application(&pef_shorten).unwrap();
    loaded_shorten.cpu.gpr[3] = 0;
    let probe = loaded_shorten.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_item_visibility_query_and_update_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"FindDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);

    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: dialog_ptr,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 300,
        height: 200,
        depth: 8,
        row_bytes: 300,
        pixels_locked: false,
        pixels_no_purge: false,
    });

    // 1. FindDialogItem: Safe no-op / miss on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0x0014_0028; // pt (20, 40)
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::FindDialogItem),
    );
    assert_eq!(loaded.cpu.gpr[3], 0xFFFF_FFFF); // -1 as u32

    // 2. Set up dialog with 2 items:
    // item 1: rect (top: 10, left: 10, bottom: 30, right: 80), title "OK"
    // item 2: rect (top: 10, left: 90, bottom: 30, right: 160), title "Cancel"
    let item1 = make_test_ditl_item(10, 10, 30, 80, b"OK");
    let item2 = make_test_ditl_item(10, 90, 30, 160, b"Cancel");
    let ditl_bytes = make_test_ditl(&[item1, item2]);
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl_bytes,
    );
    loaded
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 2a. FindDialogItem hit item 1 at point (20, 40) -> returns 0 (0-indexed)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0x0014_0028; // (20, 40)
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::FindDialogItem),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2b. FindDialogItem hit item 2 at point (20, 120) -> returns 1
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0x0014_0078; // (20, 120)
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::FindDialogItem),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 2c. FindDialogItem miss at point (200, 200) -> returns -1 (0xFFFF_FFFF)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0x00C8_00C8; // (200, 200)
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::FindDialogItem),
    );
    assert_eq!(loaded.cpu.gpr[3], 0xFFFF_FFFF);

    // 3. HideDialogItem & ShowDialogItem
    // 3a. HideDialogItem(dialog_ptr, 1) moves item 1 offscreen
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    // In DITL: header is 2 bytes; item 1 starts at offset 2; rect is offset + 4..+ 12 (top, left, bottom, right)
    // So item 1 left is at items_ptr + 2 + 6 = items_ptr + 8
    assert_eq!(
        loaded.memory.read_u16_be(items_ptr + 8).map(|v| v as i16),
        Some(10)
    );
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1; // itemNo 1 (1-indexed)
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::HideDialogItem),
    );
    // After hiding, left is offset by +16384 (10 + 16384 = 16394)
    assert_eq!(
        loaded.memory.read_u16_be(items_ptr + 8).map(|v| v as i16),
        Some(16394)
    );

    // 3b. ShowDialogItem(dialog_ptr, 1) restores item 1 onscreen
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1; // itemNo 1
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::ShowDialogItem),
    );
    // After showing, left is restored to 10
    assert_eq!(
        loaded.memory.read_u16_be(items_ptr + 8).map(|v| v as i16),
        Some(10)
    );

    // 4. UpdateDialog redraws dialog cleanly
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0; // updateRgn
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::UpdateDialog),
    );

    // 5. Verify PEF execution using classic aliases across AppearanceLib, DialogsLib, and CarbonLib
    let pef_find = synthetic_pef_with_library_import(b"AppearanceLib", b"FindDItem");
    let mut loaded_find = load_pef_application(&pef_find).unwrap();
    loaded_find.cpu.gpr[3] = 0;
    let probe = loaded_find.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_hide = synthetic_pef_with_library_import(b"DialogsLib", b"HideDItem");
    let mut loaded_hide = load_pef_application(&pef_hide).unwrap();
    loaded_hide.cpu.gpr[3] = 0;
    let probe = loaded_hide.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_show = synthetic_pef_with_library_import(b"CarbonLib", b"ShowDItem");
    let mut loaded_show = load_pef_application(&pef_show).unwrap();
    loaded_show.cpu.gpr[3] = 0;
    let probe = loaded_show.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_updt = synthetic_pef_with_library_import(b"CarbonLib", b"UpdtDialog");
    let mut loaded_updt = load_pef_application(&pef_updt).unwrap();
    loaded_updt.cpu.gpr[3] = 0;
    let probe = loaded_updt.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_event_loop_and_rendering_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"DrawDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);

    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: dialog_ptr,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 300,
        height: 200,
        depth: 8,
        row_bytes: 300,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.window_list.push(dialog_ptr);

    // 1. DrawDialog: Safe no-op on NULL dialog
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawDialog);

    // 1b. DrawDialog with valid dialog pointer
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawDialog);

    // 2. IsDialogEvent:
    // 2a. Safe false (0) on NULL event pointer
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::IsDialogEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2b. True (1) for event targeted at front dialog window
    let event_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(event_ptr, vec![0; 16]);
    loaded
        .memory
        .write_u16_be(dialog_ptr + PPC_CWINDOW_WINDOW_KIND_OFFSET, 2) // dialogKind
        .unwrap();
    loaded
        .memory
        .write_u8(dialog_ptr + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    ppc_write_event_record(&mut loaded.memory, event_ptr, 3, b'A' as u32, 0, 20, 30, 0); // keyDown
    loaded.cpu.gpr[3] = event_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::IsDialogEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 3. DialogSelect:
    // 3a. Safe false (0) on NULL event pointer
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(PpcDialogCompatibilityOperation::DialogSelect),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 4. ModalDialog:
    // Safe completion when item_hit pointer is NULL
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ModalDialog);

    // 5. Verify PEF execution using AppearanceLib, DialogsLib, and CarbonLib
    let pef_draw = synthetic_pef_with_library_import(b"AppearanceLib", b"DrawDialog");
    let mut loaded_draw = load_pef_application(&pef_draw).unwrap();
    loaded_draw.cpu.gpr[3] = 0;
    let probe = loaded_draw.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_modal = synthetic_pef_with_library_import(b"DialogsLib", b"ModalDialog");
    let mut loaded_modal = load_pef_application(&pef_modal).unwrap();
    let hit_ptr = PPC_DATA_BASE + 0x2100;
    loaded_modal.memory.add_region(hit_ptr, vec![0; 2]);
    loaded_modal.cpu.gpr[3] = 0;
    loaded_modal.cpu.gpr[4] = hit_ptr;
    let probe = loaded_modal.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_select = synthetic_pef_with_library_import(b"CarbonLib", b"DialogSelect");
    let mut loaded_select = load_pef_application(&pef_select).unwrap();
    loaded_select.cpu.gpr[3] = 0;
    let probe = loaded_select.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_is_dlg = synthetic_pef_with_library_import(b"CarbonLib", b"IsDialogEvent");
    let mut loaded_is_dlg = load_pef_application(&pef_is_dlg).unwrap();
    loaded_is_dlg.cpu.gpr[3] = 0;
    let probe = loaded_is_dlg.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_teardown_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"CloseDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);

    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: dialog_ptr,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 300,
        height: 200,
        depth: 8,
        row_bytes: 300,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.window_list.push(dialog_ptr);

    // 1. CloseDialog: Safe no-op on NULL dialog pointer
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseDialog);
    assert!(loaded.window_list.contains(&dialog_ptr));

    // 2. DisposeDialog: Safe no-op on NULL dialog pointer
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeDialog);
    assert!(loaded.window_list.contains(&dialog_ptr));

    // 3. CloseDialog with valid dialog pointer removes from window_list
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseDialog);
    assert!(!loaded.window_list.contains(&dialog_ptr));

    // 4. DisposeDialog with second dialog removes from window_list and gworlds
    let dialog2_ptr = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(dialog2_ptr, vec![0; 256]);
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: dialog2_ptr,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 300,
        height: 200,
        depth: 8,
        row_bytes: 300,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.window_list.push(dialog2_ptr);
    assert!(loaded.window_list.contains(&dialog2_ptr));

    loaded.cpu.gpr[3] = dialog2_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeDialog);
    assert!(!loaded.window_list.contains(&dialog2_ptr));
    assert!(!loaded.gworlds.iter().any(|gw| gw.port == dialog2_ptr));

    // 5. Verify PEF execution using AppearanceLib, DialogsLib, and CarbonLib
    let pef_close = synthetic_pef_with_library_import(b"AppearanceLib", b"CloseDialog");
    let mut loaded_close = load_pef_application(&pef_close).unwrap();
    loaded_close.cpu.gpr[3] = 0;
    let probe = loaded_close.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_dispose = synthetic_pef_with_library_import(b"DialogsLib", b"DisposeDialog");
    let mut loaded_dispose = load_pef_application(&pef_dispose).unwrap();
    loaded_dispose.cpu.gpr[3] = 0;
    let probe = loaded_dispose.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_dispos = synthetic_pef_with_library_import(b"CarbonLib", b"DisposDialog");
    let mut loaded_dispos = load_pef_application(&pef_dispos).unwrap();
    loaded_dispos.cpu.gpr[3] = 0;
    let probe = loaded_dispos.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_item_and_text_access_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 1024]);

    // Memory layout for test buffers:
    // type_out: dialog_ptr + 0x200 (u16)
    // handle_out: dialog_ptr + 0x204 (u32)
    // rect_out: dialog_ptr + 0x208 (8 bytes: top, left, bottom, right)
    // text_out: dialog_ptr + 0x220 (Str255: 1 byte len + up to 255 bytes)
    // text_in: dialog_ptr + 0x330 (Str255)
    let type_out = dialog_ptr + 0x200;
    let handle_out = dialog_ptr + 0x204;
    let rect_out = dialog_ptr + 0x208;
    let text_out = dialog_ptr + 0x220;
    let text_in = dialog_ptr + 0x330;

    // 1. Create text handles:
    // text_handle for item 1 ("Systemless")
    // edit_text_handle for item 2 ("Edit")
    let text_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"Systemless",
    );
    let edit_text_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"Edit",
    );

    // 2. Set up dialog with 2 items:
    // Item 1: StatText, rect (10, 20, 30, 80), handle = text_handle
    // Item 2: EditText, rect (40, 20, 60, 180), handle = edit_text_handle
    let item1 = make_test_ditl_item_typed(10, 20, 30, 80, PPC_DIALOG_ITEM_STATIC_TEXT, b"OK");
    let item2 = make_test_ditl_item_typed(40, 20, 60, 180, PPC_DIALOG_ITEM_EDIT_TEXT, b"Edit");
    let ditl_bytes = make_test_ditl(&[item1, item2]);
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl_bytes,
    );
    loaded
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // Attach text handles to items in DITL:
    // Item 1 starts at offset 2; handle is at offset 2 + 0
    // Item 1 length = 4 + 8 + 1 + 1 + 2 = 16 bytes.
    // Item 2 starts at offset 2 + 16 = 18; handle is at offset 18 + 0
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    loaded.memory.write_u32_be(items_ptr + 2, text_handle).unwrap();
    loaded.memory.write_u32_be(items_ptr + 18, edit_text_handle).unwrap();

    // 3. Test GetDialogItem:
    // 3a. Safe no-op on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = type_out;
    loaded.cpu.gpr[6] = handle_out;
    loaded.cpu.gpr[7] = rect_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItem);
    assert_eq!(loaded.memory.read_u16_be(type_out).unwrap(), 0);

    // 3b. Safe no-op on invalid item number 0
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItem);
    assert_eq!(loaded.memory.read_u16_be(type_out).unwrap(), 0);

    // 3c. Valid GetDialogItem for item 1
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItem);
    assert_eq!(
        loaded.memory.read_u16_be(type_out).unwrap(),
        u16::from(PPC_DIALOG_ITEM_STATIC_TEXT)
    );
    assert_eq!(loaded.memory.read_u32_be(handle_out).unwrap(), text_handle);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, rect_out),
        Some((10, 20, 30, 80))
    );

    // 4. Test SetDialogItem:
    // Write new rect into rect_out: (15, 25, 35, 85)
    let new_rect_ptr = rect_out;
    assert!(ppc_write_rect(&mut loaded.memory, new_rect_ptr, 15, 25, 35, 85).is_some());

    let new_handle = 0xABCD_1234;
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = u32::from(PPC_DIALOG_ITEM_BUTTON);
    loaded.cpu.gpr[6] = new_handle;
    loaded.cpu.gpr[7] = new_rect_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogItem);

    // Read back with GetDialogItem
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = type_out;
    loaded.cpu.gpr[6] = handle_out;
    loaded.cpu.gpr[7] = rect_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItem);
    assert_eq!(
        loaded.memory.read_u16_be(type_out).unwrap(),
        u16::from(PPC_DIALOG_ITEM_BUTTON)
    );
    assert_eq!(loaded.memory.read_u32_be(handle_out).unwrap(), new_handle);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, rect_out),
        Some((15, 25, 35, 85))
    );

    // 5. Test GetDialogItemText:
    // 5a. Safe no-op on NULL text_out_ptr
    loaded.cpu.gpr[3] = text_handle;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemText);

    // 5b. Read text from text_handle ("Systemless") into text_out
    loaded.cpu.gpr[3] = text_handle;
    loaded.cpu.gpr[4] = text_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemText);
    let out_len = loaded.memory.read_u8(text_out).unwrap() as usize;
    assert_eq!(out_len, 10);
    let mut out_str = vec![0u8; out_len];
    for (i, b) in out_str.iter_mut().enumerate() {
        *b = loaded.memory.read_u8(text_out + 1 + i as u32).unwrap();
    }
    assert_eq!(out_str.as_slice(), b"Systemless");

    // 6. Test SetDialogItemText:
    // Write Pascal string "\x07Classic" to text_in
    loaded.memory.write_u8(text_in, 7).unwrap();
    for (i, &b) in b"Classic".iter().enumerate() {
        loaded.memory.write_u8(text_in + 1 + i as u32, b).unwrap();
    }

    // SetDialogItemText on text_handle
    loaded.cpu.gpr[3] = text_handle;
    loaded.cpu.gpr[4] = text_in;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogItemText);

    // Read back with GetDialogItemText into text_out
    loaded.cpu.gpr[3] = text_handle;
    loaded.cpu.gpr[4] = text_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemText);
    let out_len2 = loaded.memory.read_u8(text_out).unwrap() as usize;
    assert_eq!(out_len2, 7);
    let mut out_str2 = vec![0u8; out_len2];
    for (i, b) in out_str2.iter_mut().enumerate() {
        *b = loaded.memory.read_u8(text_out + 1 + i as u32).unwrap();
    }
    assert_eq!(out_str2.as_slice(), b"Classic");

    // 7. Test SelectDialogItemText:
    // 7a. Safe no-op on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 2; // item 2
    loaded.cpu.gpr[5] = 0; // selStart
    loaded.cpu.gpr[6] = 3; // selEnd
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectDialogItemText);

    // 7b. Safe no-op on non-edit item (item 1 was changed to button)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectDialogItemText);

    // 7c. Select item 2 (editText):
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2; // item 2
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectDialogItemText);
    // DialogRecord.editField at offset PPC_DIALOG_EDIT_FIELD_OFFSET is 0-indexed item index = 1
    assert_eq!(
        loaded.memory.read_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET).unwrap(),
        1
    );

    // 8. Verify PEF execution using AppearanceLib, DialogsLib, and CarbonLib
    let pef_get = synthetic_pef_with_library_import(b"AppearanceLib", b"GetDItem");
    let mut loaded_get = load_pef_application(&pef_get).unwrap();
    loaded_get.cpu.gpr[3] = 0;
    let probe = loaded_get.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_set = synthetic_pef_with_library_import(b"DialogsLib", b"SetDItem");
    let mut loaded_set = load_pef_application(&pef_set).unwrap();
    loaded_set.cpu.gpr[3] = 0;
    let probe = loaded_set.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_get_text = synthetic_pef_with_library_import(b"CarbonLib", b"GetIText");
    let mut loaded_get_text = load_pef_application(&pef_get_text).unwrap();
    loaded_get_text.cpu.gpr[3] = 0;
    let probe = loaded_get_text.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_set_text = synthetic_pef_with_library_import(b"CarbonLib", b"SetIText");
    let mut loaded_set_text = load_pef_application(&pef_set_text).unwrap();
    loaded_set_text.cpu.gpr[3] = 0;
    let probe = loaded_set_text.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_sel_text = synthetic_pef_with_library_import(b"CarbonLib", b"SelIText");
    let mut loaded_sel_text = load_pef_application(&pef_sel_text).unwrap();
    loaded_sel_text.cpu.gpr[3] = 0;
    let probe = loaded_sel_text.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_default_cancel_and_cursor_tracking_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"SetDialogDefaultItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 512]);

    let out_default_ptr = dialog_ptr + 0x180;
    let out_cancel_ptr = dialog_ptr + 0x184;

    // 1. Set up dialog with items:
    // item 1: "OK" (button)
    // item 2: "Cancel" (button)
    // item 3: "Help" (button)
    let item1 = make_test_ditl_item_typed(10, 10, 30, 80, PPC_DIALOG_ITEM_BUTTON, b"OK");
    let item2 = make_test_ditl_item_typed(10, 90, 30, 160, PPC_DIALOG_ITEM_BUTTON, b"Cancel");
    let item3 = make_test_ditl_item_typed(40, 10, 60, 80, PPC_DIALOG_ITEM_BUTTON, b"Help");
    let ditl_bytes = make_test_ditl(&[item1, item2, item3]);
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl_bytes,
    );
    loaded
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 2. SetDialogDefaultItem:
    // 2a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2b. Valid SetDialogDefaultItem to 3
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(dialog_ptr + PPC_DIALOG_DEFAULT_ITEM_OFFSET).unwrap(),
        3
    );

    // 3. GetDialogDefaultItem:
    // 3a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_default_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 3b. Safe error on NULL output pointer
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 3c. Valid GetDialogDefaultItem (returns configured 3)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_default_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_default_ptr).unwrap(), 3);

    // 3d. Clear default item via SetDialogDefaultItem(dialog, 0)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(dialog_ptr + PPC_DIALOG_DEFAULT_ITEM_OFFSET).unwrap(),
        0
    );

    // 3e. GetDialogDefaultItem when default is 0 falls back to item 1
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_default_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogDefaultItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_default_ptr).unwrap(), 1);

    // 4. SetDialogCancelItem:
    // 4a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 2;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 4b. Valid SetDialogCancelItem to 3
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(dialog_ptr + PPC_DIALOG_CANCEL_ITEM_OFFSET).unwrap(),
        3
    );

    // 5. GetDialogCancelItem:
    // 5a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_cancel_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 5b. Safe error on NULL out pointer
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 5c. Valid GetDialogCancelItem (returns configured 3)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_cancel_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_cancel_ptr).unwrap(), 3);

    // 5d. Clear cancel item via SetDialogCancelItem(dialog, 0)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(dialog_ptr + PPC_DIALOG_CANCEL_ITEM_OFFSET).unwrap(),
        0
    );

    // 5e. GetDialogCancelItem auto-resolves item with title "Cancel" (item 2)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_cancel_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogCancelItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_cancel_ptr).unwrap(), 2);

    // 6. SetDialogTracksCursor:
    // 6a. Global cursor tracking with NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogTracksCursor);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 6b. Dialog-specific cursor tracking
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogTracksCursor);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 7. Verify PEF execution using AppearanceLib, DialogsLib, and CarbonLib
    let pef_set_def = synthetic_pef_with_library_import(b"DialogsLib", b"SetDialogDefaultItem");
    let mut loaded_set_def = load_pef_application(&pef_set_def).unwrap();
    loaded_set_def.cpu.gpr[3] = 0;
    let probe = loaded_set_def.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_get_def = synthetic_pef_with_library_import(b"CarbonLib", b"GetDialogDefaultItem");
    let mut loaded_get_def = load_pef_application(&pef_get_def).unwrap();
    loaded_get_def.cpu.gpr[3] = 0;
    let probe = loaded_get_def.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_set_can = synthetic_pef_with_library_import(b"CarbonLib", b"SetDialogCancelItem");
    let mut loaded_set_can = load_pef_application(&pef_set_can).unwrap();
    loaded_set_can.cpu.gpr[3] = 0;
    let probe = loaded_set_can.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_get_can = synthetic_pef_with_library_import(b"DialogsLib", b"GetDialogCancelItem");
    let mut loaded_get_can = load_pef_application(&pef_get_can).unwrap();
    loaded_get_can.cpu.gpr[3] = 0;
    let probe = loaded_get_can.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_tracks = synthetic_pef_with_library_import(b"AppearanceLib", b"SetDialogTracksCursor");
    let mut loaded_tracks = load_pef_application(&pef_tracks).unwrap();
    loaded_tracks.cpu.gpr[3] = 0;
    let probe = loaded_tracks.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_item_positioning_and_geometry_commands_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"MoveDialogItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 512]);

    // Set up item 1 with control handle:
    // Initial rect: (top: 10, left: 20, bottom: 50, right: 100) -> width 80, height 40
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
        .write_u32_be(dialog_ptr + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 1. MoveDialogItem:
    // 1a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 150;
    loaded.cpu.gpr[6] = 120;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1b. Safe error on invalid item 0
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 150;
    loaded.cpu.gpr[6] = 120;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1c. Valid MoveDialogItem: move item 1 to inHoriz=150, inVert=120
    // Width was 80, height was 40.
    // New rect: top = 120, left = 150, bottom = 120 + 40 = 160, right = 150 + 80 = 230
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 150;
    loaded.cpu.gpr[6] = 120;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    let ditl_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, ditl_ptr + 6),
        Some((120, 150, 160, 230))
    );
    let ctrl_ptr = loaded.memory.read_u32_be(ctrl_handle).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, ctrl_ptr + PPC_CONTROL_RECT_OFFSET),
        Some((120, 150, 160, 230))
    );

    // 2. SizeDialogItem:
    // 2a. Safe error on NULL dialog
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 200;
    loaded.cpu.gpr[6] = 80;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SizeDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2b. Safe error on invalid item 2 (does not exist in 1-item DITL)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 200;
    loaded.cpu.gpr[6] = 80;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SizeDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2c. Valid SizeDialogItem: resize item 1 to inWidth=200, inHeight=80
    // Origin was (120, 150).
    // New rect: top = 120, left = 150, bottom = 120 + 80 = 200, right = 150 + 200 = 350
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 200;
    loaded.cpu.gpr[6] = 80;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SizeDialogItem);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    assert_eq!(
        ppc_read_rect(&mut loaded.memory, ditl_ptr + 6),
        Some((120, 150, 200, 350))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, ctrl_ptr + PPC_CONTROL_RECT_OFFSET),
        Some((120, 150, 200, 350))
    );

    // 3. Verify PEF execution using DialogsLib and CarbonLib
    let pef_move_dlg = synthetic_pef_with_library_import(b"DialogsLib", b"MoveDialogItem");
    let mut loaded_move_dlg = load_pef_application(&pef_move_dlg).unwrap();
    loaded_move_dlg.cpu.gpr[3] = 0;
    let probe = loaded_move_dlg.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_move_carb = synthetic_pef_with_library_import(b"CarbonLib", b"MoveDialogItem");
    let mut loaded_move_carb = load_pef_application(&pef_move_carb).unwrap();
    loaded_move_carb.cpu.gpr[3] = 0;
    let probe = loaded_move_carb.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_size_dlg = synthetic_pef_with_library_import(b"DialogsLib", b"SizeDialogItem");
    let mut loaded_size_dlg = load_pef_application(&pef_size_dlg).unwrap();
    loaded_size_dlg.cpu.gpr[3] = 0;
    let probe = loaded_size_dlg.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);

    let pef_size_carb = synthetic_pef_with_library_import(b"CarbonLib", b"SizeDialogItem");
    let mut loaded_size_carb = load_pef_application(&pef_size_carb).unwrap();
    loaded_size_carb.cpu.gpr[3] = 0;
    let probe = loaded_size_carb.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn dialog_alert_and_param_text_commands_dispatch_with_canonical_evaluation() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"ParamText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 256]);

    // 1. ParamText:
    // 1a. Write 4 Pascal strings to scratch memory
    let strings: [&[u8]; 4] = [b"FirstParam", b"SecondParam", b"ThirdParam", b"FourthParam"];
    for (i, text) in strings.iter().enumerate() {
        let ptr = scratch + i as u32 * 32;
        write_ppc_pstring(&mut loaded.memory, ptr, text);
        loaded.cpu.gpr[3 + i] = ptr;
    }
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ParamText);
    assert_eq!(loaded.param_text.slot(0).as_deref(), Some(b"FirstParam".as_slice()));
    assert_eq!(loaded.param_text.slot(1).as_deref(), Some(b"SecondParam".as_slice()));
    assert_eq!(loaded.param_text.slot(2).as_deref(), Some(b"ThirdParam".as_slice()));
    assert_eq!(loaded.param_text.slot(3).as_deref(), Some(b"FourthParam".as_slice()));

    // 1b. Verify NULL pointer retains existing slot, and empty string updates to empty
    let empty_ptr = scratch + 128;
    write_ppc_pstring(&mut loaded.memory, empty_ptr, b"");
    loaded.cpu.gpr[3] = 0;         // NULL: preserves FirstParam
    loaded.cpu.gpr[4] = empty_ptr; // empty: updates to empty
    loaded.cpu.gpr[5] = 0;         // NULL: preserves ThirdParam
    loaded.cpu.gpr[6] = 0;         // NULL: preserves FourthParam
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ParamText);
    assert_eq!(loaded.param_text.slot(0).as_deref(), Some(b"FirstParam".as_slice()));
    assert_eq!(loaded.param_text.slot(1).as_deref(), Some(b"".as_slice()));
    assert_eq!(loaded.param_text.slot(2).as_deref(), Some(b"ThirdParam".as_slice()));
    assert_eq!(loaded.param_text.slot(3).as_deref(), Some(b"FourthParam".as_slice()));

    // 2. AlertReturnDefault (Alert, StopAlert, NoteAlert, CautionAlert) with missing resource ID:
    for kind in [
        crate::dialog_manager::AlertKind::Alert,
        crate::dialog_manager::AlertKind::Stop,
        crate::dialog_manager::AlertKind::Note,
        crate::dialog_manager::AlertKind::Caution,
    ] {
        loaded.cpu.gpr[3] = 999; // non-existent alert ID
        loaded.cpu.gpr[4] = 0;   // filterProc = NULL
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AlertReturnDefault(kind));
        assert_eq!(loaded.cpu.gpr[3] as i16, -1);
    }

    // 3. StandardAlert:
    // Safe error on NULL outItemHit
    loaded.cpu.gpr[3] = 0; // kAlertStopAlert
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0; // NULL outItemHit
    run_test_import(&mut loaded, PpcImportDispatcherTarget::StandardAlert);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 4. ResetAlertStage:
    loaded
        .memory
        .write_u16_be(crate::memory::globals::addr::ALERT_STAGE, 3)
        .unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::ResetAlertStage,
        ),
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::ALERT_STAGE),
        Some(crate::dialog_manager::INITIAL_ALERT_STAGE)
    );

    // 5. Verify PEF execution using DialogsLib and CarbonLib
    for (lib, symbol) in [
        (b"DialogsLib".as_slice(), b"ParamText".as_slice()),
        (b"CarbonLib".as_slice(), b"ParamText".as_slice()),
        (b"DialogsLib".as_slice(), b"paramtext".as_slice()),
        (b"CarbonLib".as_slice(), b"paramtext".as_slice()),
        (b"DialogsLib".as_slice(), b"Alert".as_slice()),
        (b"CarbonLib".as_slice(), b"Alert".as_slice()),
        (b"DialogsLib".as_slice(), b"StopAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"StopAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"NoteAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"NoteAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"CautionAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"CautionAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"StandardAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"StandardAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"ResetAlertStage".as_slice()),
        (b"CarbonLib".as_slice(), b"ResetAlertStage".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn dialog_creation_commands_dispatch_with_canonical_evaluation() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 128]);

    // 1. GetNewDialog:
    // Safe return 0 (NULL) when DLOG resource does not exist
    loaded.cpu.gpr[3] = 999; // non-existent dialog ID
    loaded.cpu.gpr[4] = 0;   // storage = NULL
    loaded.cpu.gpr[5] = u32::MAX; // behind = -1
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetNewDialog);
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2. NewDialog:
    // Setup programmatic bounds, title, and empty items handle
    let bounds_ptr = scratch;
    let title_ptr = scratch + 8;
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 40, 60, 180, 300).unwrap();
    write_ppc_pstring(&mut loaded.memory, title_ptr, b"CreationTest");
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

    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewDialog);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    assert_eq!(*loaded.current_gworld, dialog);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET),
        Some(2)
    );
    assert_eq!(
        loaded.memory.read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET),
        Some(items)
    );

    // 3. NewFeaturesDialog:
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewFeaturesDialog);
    let feat_dialog = loaded.cpu.gpr[3];
    assert_ne!(feat_dialog, 0);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(feat_dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET),
        Some(2)
    );

    // 4. Verify PEF execution using DialogsLib and CarbonLib
    for (lib, symbol) in [
        (b"DialogsLib".as_slice(), b"GetNewDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"GetNewDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"NewDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"NewDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"NewColorDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"NewColorDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"NewCDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"NewCDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"NewFeaturesDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"NewFeaturesDialog".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn dialog_preloading_filter_sound_and_init_commands_dispatch_with_canonical_evaluation() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"InitDialogs");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 128]);

    // 1. CouldDialog and FreeDialog
    loaded.cpu.gpr[3] = 128;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CouldDialog);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FreeDialog);

    // 2. CouldAlert and FreeAlert
    loaded.cpu.gpr[3] = 128;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CouldAlert);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FreeAlert);

    // 3. ErrorSound:
    // 3a. Set sound procedure pointer
    loaded.cpu.gpr[3] = 0x1122_3344;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ErrorSound);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::DA_BEEPER),
        Some(0x1122_3344)
    );

    // 3b. Set sound procedure pointer to 0 (silent)
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ErrorSound);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::DA_BEEPER),
        Some(0)
    );

    // 4. GetStdFilterProc:
    loaded.cpu.gpr[3] = scratch;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetStdFilterProc);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded.memory.read_u32_be(scratch),
        Some(PPC_STD_FILTER_TVECTOR)
    );

    // 5. StdFilterProc:
    run_test_import(&mut loaded, PpcImportDispatcherTarget::StdFilterProc);
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 6. InitDialogs:
    use crate::memory::globals::addr;
    loaded.memory.write_u32_be(addr::RESUME_PROC, 0xDEAD_BEEF).unwrap();
    loaded.memory.write_u32_be(addr::DA_BEEPER, 0x00AA_BBCC).unwrap();
    loaded.memory.write_u16_be(addr::ALERT_STAGE, 3).unwrap();
    loaded.memory.write_u16_be(addr::DLG_FONT, 5).unwrap();
    loaded.cpu.gpr[3] = 0x5566_7788;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InitDialogs);
    assert_eq!(loaded.memory.read_u32_be(addr::RESUME_PROC), Some(0x5566_7788));
    assert_eq!(loaded.memory.read_u32_be(addr::DA_BEEPER), Some(0));
    assert_eq!(
        loaded.memory.read_u16_be(addr::ALERT_STAGE),
        Some(crate::dialog_manager::INITIAL_ALERT_STAGE)
    );
    assert_eq!(loaded.memory.read_u16_be(addr::DLG_FONT), Some(0));

    // 7. Verify PEF execution using DialogsLib and CarbonLib
    for (lib, symbol) in [
        (b"DialogsLib".as_slice(), b"CouldDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"CouldDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"FreeDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"FreeDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"CouldAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"CouldAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"FreeAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"FreeAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"ErrorSound".as_slice()),
        (b"CarbonLib".as_slice(), b"ErrorSound".as_slice()),
        (b"DialogsLib".as_slice(), b"StdFilterProc".as_slice()),
        (b"CarbonLib".as_slice(), b"StdFilterProc".as_slice()),
        (b"DialogsLib".as_slice(), b"GetStdFilterProc".as_slice()),
        (b"CarbonLib".as_slice(), b"GetStdFilterProc".as_slice()),
        (b"DialogsLib".as_slice(), b"InitDialogs".as_slice()),
        (b"CarbonLib".as_slice(), b"InitDialogs".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn dialog_control_conversion_and_lowmem_commands_dispatch_with_canonical_evaluation() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetDialogItemAsControl");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0; 4]);

    // Construct dialog DITL with 2 items:
    // Item 1: Button with control handle 0x1234
    // Item 2: StaticText (non-control)
    let mut ditl = 1i16.to_be_bytes().to_vec();
    let mut item1 = vec![0; 16];
    item1[0..4].copy_from_slice(&0x1234u32.to_be_bytes());
    item1[12] = PPC_DIALOG_ITEM_BUTTON;
    item1[13] = 2;
    item1[14..16].copy_from_slice(b"OK");
    ditl.extend(item1);

    let mut item2 = vec![0; 18];
    item2[0..4].copy_from_slice(&0u32.to_be_bytes());
    item2[12] = PPC_DIALOG_ITEM_STATIC_TEXT;
    item2[13] = 4;
    item2[14..18].copy_from_slice(b"Text");
    ditl.extend(item2);

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

    // 1. GetDialogItemAsControl on button item (item 1) -> success
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = output;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemAsControl);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234));

    // 2. GetDialogItemAsControl on static text item (item 2) -> paramErr
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = output;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemAsControl);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 3. GetDialogItemAsControl on invalid item (item 99) -> paramErr
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 99;
    loaded.cpu.gpr[5] = output;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemAsControl);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 4. GetDialogItemAsControl with NULL dialog -> paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = output;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetDialogItemAsControl);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 5. LMSetResumeProc / LMGetResumeProc
    loaded.cpu.gpr[3] = 0x1122_3344;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetResumeProc);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetResumeProc);
    assert_eq!(loaded.cpu.gpr[3], 0x1122_3344);

    // 6. LMSetACount / LMGetACount
    loaded.cpu.gpr[3] = 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetACount);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetACount);
    assert_eq!(loaded.cpu.gpr[3] as i16, 3);

    // 7. LMSetANumber / LMGetANumber
    loaded.cpu.gpr[3] = 42;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetANumber);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetANumber);
    assert_eq!(loaded.cpu.gpr[3] as i16, 42);

    // 8. LMSetDABeeper / LMGetDABeeper
    loaded.cpu.gpr[3] = 0xAABB_CCDD;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetDABeeper);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetDABeeper);
    assert_eq!(loaded.cpu.gpr[3], 0xAABB_CCDD);

    // 9. LMGetDAStrings
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetDAStrings);
    assert_eq!(loaded.cpu.gpr[3], crate::memory::globals::addr::DA_STRINGS);

    // 10. LMSetDlgFont / LMGetDlgFont
    loaded.cpu.gpr[3] = 12;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetDlgFont);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetDlgFont);
    assert_eq!(loaded.cpu.gpr[3] as i16, 12);

    // 11. Synthetic PEF execution for all 12 symbols across DialogsLib and CarbonLib
    for (lib, symbol) in [
        (b"DialogsLib".as_slice(), b"GetDialogItemAsControl".as_slice()),
        (b"CarbonLib".as_slice(), b"GetDialogItemAsControl".as_slice()),
        (b"DialogsLib".as_slice(), b"LMSetResumeProc".as_slice()),
        (b"CarbonLib".as_slice(), b"LMSetResumeProc".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetResumeProc".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetResumeProc".as_slice()),
        (b"DialogsLib".as_slice(), b"LMSetACount".as_slice()),
        (b"CarbonLib".as_slice(), b"LMSetACount".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetACount".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetACount".as_slice()),
        (b"DialogsLib".as_slice(), b"LMSetANumber".as_slice()),
        (b"CarbonLib".as_slice(), b"LMSetANumber".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetANumber".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetANumber".as_slice()),
        (b"DialogsLib".as_slice(), b"LMSetDABeeper".as_slice()),
        (b"CarbonLib".as_slice(), b"LMSetDABeeper".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetDABeeper".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetDABeeper".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetDAStrings".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetDAStrings".as_slice()),
        (b"DialogsLib".as_slice(), b"LMSetDlgFont".as_slice()),
        (b"CarbonLib".as_slice(), b"LMSetDlgFont".as_slice()),
        (b"DialogsLib".as_slice(), b"LMGetDlgFont".as_slice()),
        (b"CarbonLib".as_slice(), b"LMGetDlgFont".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn dialog_keyboard_focus_textedit_paramtext_and_timeout_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetDialogKeyboardFocusItem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);

    // 1. GetDialogKeyboardFocusItem:
    // 1a. NULL dialog returns 0
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 1b. Dialog with editField = -1 (no field active) returns 0
    loaded
        .memory
        .write_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET, (-1i16) as u16)
        .unwrap();
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 1c. Dialog with editField = 0 (item 1 has focus) returns 1
    loaded
        .memory
        .write_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET, 0)
        .unwrap();
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 1d. Dialog with editField = 2 (item 3 has focus) returns 3
    loaded
        .memory
        .write_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET, 2)
        .unwrap();
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 3);

    // 2. SetDialogKeyboardFocusItem:
    // 2a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2b. Setting item 2 sets editField = 1 and returns noErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET),
        Some(1)
    );

    // 2c. Clearing focus with item 0 sets editField = -1 and returns noErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog_ptr + PPC_DIALOG_EDIT_FIELD_OFFSET)
            .map(|f| f as i16),
        Some(-1)
    );

    // 3. GetDialogTextEditHandle:
    // 3a. NULL dialog returns 0
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTextEditHandle,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 3b. Dialog with textH returns textH handle
    let fake_te_handle = 0x55AA_1234;
    loaded
        .memory
        .write_u32_be(dialog_ptr + PPC_DIALOG_TEXT_HANDLE_OFFSET, fake_te_handle)
        .unwrap();
    loaded.cpu.gpr[3] = dialog_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTextEditHandle,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], fake_te_handle);

    // 4. GetParamText:
    let out_buf0 = PPC_DATA_BASE + 0x2000;
    let out_buf1 = PPC_DATA_BASE + 0x2100;
    let out_buf3 = PPC_DATA_BASE + 0x2200;
    loaded.memory.add_region(out_buf0, vec![0; 256]);
    loaded.memory.add_region(out_buf1, vec![0; 256]);
    loaded.memory.add_region(out_buf3, vec![0; 256]);

    loaded.param_text.set_slot(0, b"First".to_vec());
    loaded.param_text.set_slot(1, b"SecondParam".to_vec());
    loaded.param_text.set_slot(2, b"ThirdUnqueried".to_vec());
    loaded.param_text.set_slot(3, b"FourthParamText".to_vec());

    loaded.cpu.gpr[3] = out_buf0;
    loaded.cpu.gpr[4] = out_buf1;
    loaded.cpu.gpr[5] = 0; // NULL pointer for param2
    loaded.cpu.gpr[6] = out_buf3;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetParamText,
        ),
    );
    // Verify Pascal strings written to out_buf0, out_buf1, out_buf3
    assert_eq!(loaded.memory.read_u8(out_buf0), Some(5));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, out_buf0),
        Some(b"First".to_vec())
    );
    assert_eq!(loaded.memory.read_u8(out_buf1), Some(11));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, out_buf1),
        Some(b"SecondParam".to_vec())
    );
    assert_eq!(loaded.memory.read_u8(out_buf3), Some(15));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, out_buf3),
        Some(b"FourthParamText".to_vec())
    );

    // 5. SetDialogTimeout & GetDialogTimeout:
    // 5a. SetDialogTimeout with NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 45;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogTimeout,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 5b. SetDialogTimeout with valid dialog records button and duration
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2; // button 2 (Cancel)
    loaded.cpu.gpr[5] = 60; // 60 seconds
    loaded.set_tick_count(1200);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogTimeout,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 5c. GetDialogTimeout with NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTimeout,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 5d. GetDialogTimeout reads back button, seconds, and remaining duration
    let out_btn = PPC_DATA_BASE + 0x2300;
    let out_secs = PPC_DATA_BASE + 0x2304;
    let out_rem = PPC_DATA_BASE + 0x2308;
    loaded.memory.add_region(out_btn, vec![0; 16]);

    // Advance 600 ticks (10 seconds elapsed out of 60)
    loaded.set_tick_count(1800);
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_btn;
    loaded.cpu.gpr[5] = out_secs;
    loaded.cpu.gpr[6] = out_rem;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTimeout,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_btn), Some(2));
    assert_eq!(loaded.memory.read_u32_be(out_secs), Some(60));
    assert_eq!(loaded.memory.read_u32_be(out_rem), Some(50));

    // 5e. GetDialogTimeout with NULL out pointers succeeds without writing
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTimeout,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 6. Synthetic PEF execution for all 6 routines across all four libraries
    for (lib, symbol) in [
        (
            b"InterfaceLib".as_slice(),
            b"GetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"AppearanceLib".as_slice(),
            b"GetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"DialogsLib".as_slice(),
            b"GetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"CarbonLib".as_slice(),
            b"GetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"InterfaceLib".as_slice(),
            b"SetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"AppearanceLib".as_slice(),
            b"SetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"DialogsLib".as_slice(),
            b"SetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"CarbonLib".as_slice(),
            b"SetDialogKeyboardFocusItem".as_slice(),
        ),
        (
            b"InterfaceLib".as_slice(),
            b"GetDialogTextEditHandle".as_slice(),
        ),
        (
            b"AppearanceLib".as_slice(),
            b"GetDialogTextEditHandle".as_slice(),
        ),
        (
            b"DialogsLib".as_slice(),
            b"GetDialogTextEditHandle".as_slice(),
        ),
        (
            b"CarbonLib".as_slice(),
            b"GetDialogTextEditHandle".as_slice(),
        ),
        (b"InterfaceLib".as_slice(), b"GetParamText".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetParamText".as_slice()),
        (b"DialogsLib".as_slice(), b"GetParamText".as_slice()),
        (b"CarbonLib".as_slice(), b"GetParamText".as_slice()),
        (b"InterfaceLib".as_slice(), b"SetDialogTimeout".as_slice()),
        (b"AppearanceLib".as_slice(), b"SetDialogTimeout".as_slice()),
        (b"DialogsLib".as_slice(), b"SetDialogTimeout".as_slice()),
        (b"CarbonLib".as_slice(), b"SetDialogTimeout".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetDialogTimeout".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetDialogTimeout".as_slice()),
        (b"DialogsLib".as_slice(), b"GetDialogTimeout".as_slice()),
        (b"CarbonLib".as_slice(), b"GetDialogTimeout".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn standard_alert_sheet_and_event_mask_dispatch_with_canonical_evaluation() {
    let dialog_ptr = PPC_DATA_BASE + 0x1000;
    let out_buf = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetModalDialogEventMask");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(dialog_ptr, vec![0; 256]);
    loaded.memory.add_region(out_buf, vec![0; 256]);

    // 1. GetModalDialogEventMask and SetModalDialogEventMask
    // 1a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1b. NULL outMask returns paramErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1c. Valid dialog returns default event mask (0xFFFF)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_buf), Some(0xFFFF));

    // 1d. SetModalDialogEventMask: NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0x01FF;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1e. SetModalDialogEventMask: sets mask to 0x01FF
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0x01FF;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 1f. GetModalDialogEventMask reads back 0x01FF
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetModalDialogEventMask,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_buf), Some(0x01FF));

    // 2. GetStandardAlertDefaultParams
    // 2a. NULL param_ptr returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2b. Unsupported version != 1 returns paramErr
    loaded.cpu.gpr[3] = out_buf;
    loaded.cpu.gpr[4] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2c. Valid param_ptr with version 1 populates record
    loaded.cpu.gpr[3] = out_buf;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u32_be(out_buf), Some(1)); // version 1
    assert_eq!(loaded.memory.read_u16_be(out_buf + 20), Some(1)); // defaultButton = 1 (OK)
    assert_eq!(loaded.memory.read_u16_be(out_buf + 22), Some(0)); // cancelButton = 0

    // 3. FlashDialogControl
    // 3a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::FlashDialogControl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 3b. item <= 0 returns paramErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::FlashDialogControl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 3c. Valid dialog and item returns preserve (gpr[3] unchanged)
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::FlashDialogControl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], dialog_ptr);

    // 4. SetDialogFilter
    // 4a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0x1234;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 4b. Valid dialog returns noErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0x1234;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 5. CreateStandardAlert & CreateStandardSheet validation
    // 5a. NULL outAlert returns paramErr
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CreateStandardAlert,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CreateStandardSheet,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 6. RunStandardAlert
    // 6a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RunStandardAlert,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 6b. NULL outItemHit returns paramErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RunStandardAlert,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 6c. Valid dialog returns hit item
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RunStandardAlert,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(out_buf), Some(1));

    // 7. CloseStandardSheet
    // 7a. NULL sheet returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CloseStandardSheet,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 7b. Valid sheet writes result command and returns noErr
    let sheet_ptr = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(sheet_ptr, vec![0; 256]);
    loaded.cpu.gpr[3] = sheet_ptr;
    loaded.cpu.gpr[4] = 42;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CloseStandardSheet,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(sheet_ptr + crate::dialog_manager::DIALOG_STANDARD_SHEET_COMMAND_OFFSET),
        Some(42)
    );

    // 8. GetDialogItemInit
    // 8a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = out_buf;
    loaded.cpu.gpr[6] = out_buf + 4;
    loaded.cpu.gpr[7] = out_buf + 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogItemInit,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 8b. item <= 0 returns paramErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = out_buf;
    loaded.cpu.gpr[6] = out_buf + 4;
    loaded.cpu.gpr[7] = out_buf + 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogItemInit,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 8c. Item not found on empty dialog returns paramErr
    loaded.cpu.gpr[3] = dialog_ptr;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = out_buf;
    loaded.cpu.gpr[6] = out_buf + 4;
    loaded.cpu.gpr[7] = out_buf + 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogItemInit,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 9. All 10 operations bind cleanly across InterfaceLib, AppearanceLib, DialogsLib, and CarbonLib
    for (lib, symbol) in [
        (b"InterfaceLib".as_slice(), b"CloseStandardSheet".as_slice()),
        (b"AppearanceLib".as_slice(), b"CloseStandardSheet".as_slice()),
        (b"DialogsLib".as_slice(), b"CloseStandardSheet".as_slice()),
        (b"CarbonLib".as_slice(), b"CloseStandardSheet".as_slice()),
        (b"InterfaceLib".as_slice(), b"CreateStandardAlert".as_slice()),
        (b"AppearanceLib".as_slice(), b"CreateStandardAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"CreateStandardAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"CreateStandardAlert".as_slice()),
        (b"InterfaceLib".as_slice(), b"CreateStandardSheet".as_slice()),
        (b"AppearanceLib".as_slice(), b"CreateStandardSheet".as_slice()),
        (b"DialogsLib".as_slice(), b"CreateStandardSheet".as_slice()),
        (b"CarbonLib".as_slice(), b"CreateStandardSheet".as_slice()),
        (b"InterfaceLib".as_slice(), b"FlashDialogControl".as_slice()),
        (b"AppearanceLib".as_slice(), b"FlashDialogControl".as_slice()),
        (b"DialogsLib".as_slice(), b"FlashDialogControl".as_slice()),
        (b"CarbonLib".as_slice(), b"FlashDialogControl".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetDialogItemInit".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetDialogItemInit".as_slice()),
        (b"DialogsLib".as_slice(), b"GetDialogItemInit".as_slice()),
        (b"CarbonLib".as_slice(), b"GetDialogItemInit".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetModalDialogEventMask".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetModalDialogEventMask".as_slice()),
        (b"DialogsLib".as_slice(), b"GetModalDialogEventMask".as_slice()),
        (b"CarbonLib".as_slice(), b"GetModalDialogEventMask".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetStandardAlertDefaultParams".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetStandardAlertDefaultParams".as_slice()),
        (b"DialogsLib".as_slice(), b"GetStandardAlertDefaultParams".as_slice()),
        (b"CarbonLib".as_slice(), b"GetStandardAlertDefaultParams".as_slice()),
        (b"InterfaceLib".as_slice(), b"RunStandardAlert".as_slice()),
        (b"AppearanceLib".as_slice(), b"RunStandardAlert".as_slice()),
        (b"DialogsLib".as_slice(), b"RunStandardAlert".as_slice()),
        (b"CarbonLib".as_slice(), b"RunStandardAlert".as_slice()),
        (b"InterfaceLib".as_slice(), b"SetDialogFilter".as_slice()),
        (b"AppearanceLib".as_slice(), b"SetDialogFilter".as_slice()),
        (b"DialogsLib".as_slice(), b"SetDialogFilter".as_slice()),
        (b"CarbonLib".as_slice(), b"SetDialogFilter".as_slice()),
        (b"InterfaceLib".as_slice(), b"SetModalDialogEventMask".as_slice()),
        (b"AppearanceLib".as_slice(), b"SetModalDialogEventMask".as_slice()),
        (b"DialogsLib".as_slice(), b"SetModalDialogEventMask".as_slice()),
        (b"CarbonLib".as_slice(), b"SetModalDialogEventMask".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

#[test]
fn dialog_auto_positioning_and_cursor_tracking_dispatch_with_canonical_evaluation() {
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    let parent_bounds_ptr = PPC_DATA_BASE + 0x1100;
    let out_buf = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"AutoPositionDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    loaded.memory.add_region(parent_bounds_ptr, vec![0; 32]);
    loaded.memory.add_region(out_buf, vec![0; 256]);

    let dialog = window_manager::create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (40, 50, 140, 250),
        0,
        true,
        u32::MAX,
    );
    let parent_window = window_manager::create_test_cwindow(
        &mut loaded,
        parent_bounds_ptr,
        (100, 150, 400, 550),
        0,
        true,
        u32::MAX,
    );

    // 1. AutoPositionDialog validation
    // 1a. NULL dialog returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1b. Invalid position method 0 returns paramErr
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1c. Invalid position method > 9 returns paramErr
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 10;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1d. Valid AutoPositionDialog centering on main screen (method 1 / kWindowCenterOnMainScreen)
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    let centered_bounds =
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    assert_eq!(centered_bounds, (260, 300, 360, 500));

    // 1e. Classic position code 0x280A (center main screen) also centers on main screen
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0x280A;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog),
        Some((260, 300, 360, 500))
    );

    // 1f. AutoPositionDialog staggering relative to parent window (method 6 / kWindowStaggerParentWindow)
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = parent_window;
    loaded.cpu.gpr[5] = 6;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    let staggered_bounds =
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();
    assert_eq!(staggered_bounds, (128, 178, 228, 378));

    // 1g. Classic position code 0xB80A (stagger parent window)
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = parent_window;
    loaded.cpu.gpr[5] = 0xB80A;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog),
        Some((128, 178, 228, 378))
    );

    // 2. Cursor tracking: SetDialogTracksCursor, GetDialogTracksCursor, IsDialogTracksCursor
    let cursor_dialog = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(cursor_dialog, vec![0; 256]);

    // 2a. IsDialogTracksCursor on NULL returns 0 (false)
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2b. IsDialogTracksCursor initially returns 0 (false)
    loaded.cpu.gpr[3] = cursor_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    // 2c. GetDialogTracksCursor on NULL dialog returns noErr (global setting)
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(out_buf), Some(0));

    // 2d. GetDialogTracksCursor with unwritable output pointer returns paramErr
    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2e. GetDialogTracksCursor initially returns noErr with 0 (false)
    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(out_buf), Some(0));

    // 2f. SetDialogTracksCursor on NULL returns noErr (sets tracking for all dialogs)
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogTracksCursor);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 2g. SetDialogTracksCursor to true
    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogTracksCursor);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 2h. IsDialogTracksCursor now returns 1 (true)
    loaded.cpu.gpr[3] = cursor_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 2i. GetDialogTracksCursor now writes 1 (true)
    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(out_buf), Some(1));

    // 2j. SetDialogTracksCursor to false
    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDialogTracksCursor);
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    // 2k. IsDialogTracksCursor and GetDialogTracksCursor reflect false again
    loaded.cpu.gpr[3] = cursor_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.gpr[3] = cursor_dialog;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(out_buf), Some(0));

    // 3. Synthetic PEF binding across InterfaceLib, AppearanceLib, DialogsLib, and CarbonLib
    for (lib, symbol) in [
        (b"InterfaceLib".as_slice(), b"AutoPositionDialog".as_slice()),
        (b"AppearanceLib".as_slice(), b"AutoPositionDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"AutoPositionDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"AutoPositionDialog".as_slice()),
        (b"InterfaceLib".as_slice(), b"PositionDialog".as_slice()),
        (b"AppearanceLib".as_slice(), b"PositionDialog".as_slice()),
        (b"DialogsLib".as_slice(), b"PositionDialog".as_slice()),
        (b"CarbonLib".as_slice(), b"PositionDialog".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetDialogTracksCursor".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetDialogTracksCursor".as_slice()),
        (b"DialogsLib".as_slice(), b"GetDialogTracksCursor".as_slice()),
        (b"CarbonLib".as_slice(), b"GetDialogTracksCursor".as_slice()),
        (b"InterfaceLib".as_slice(), b"IsDialogTracksCursor".as_slice()),
        (b"AppearanceLib".as_slice(), b"IsDialogTracksCursor".as_slice()),
        (b"DialogsLib".as_slice(), b"IsDialogTracksCursor".as_slice()),
        (b"CarbonLib".as_slice(), b"IsDialogTracksCursor".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}

/// Expected bounds under the 68K Dialog Manager rule: auto-position against
/// the main screen minus the live MBarHeight.
fn expected_auto_position(
    loaded: &PpcLoadedApp,
    content: (i16, i16, i16, i16),
    position: u16,
    menu_bar_height: i32,
) -> (i16, i16, i16, i16) {
    let screen = loaded
        .gworlds
        .iter()
        .find(|record| record.port == PPC_MAIN_GWORLD);
    crate::dialog_manager::evaluate_dialog_position_bounds(
        content,
        position,
        Some(crate::dialog_manager::dialog_dbox_frame_rect(content)),
        None,
        screen.map_or(ppc_main_screen_width(), |record| record.width) as i32,
        screen.map_or(ppc_main_screen_height(), |record| record.height) as i32,
        menu_bar_height,
    )
}

#[test]
fn get_new_dialog_auto_positions_against_the_live_menu_bar_height() {
    let pef = synthetic_pef_with_import(b"GetNewDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    // A hidden menu bar: the application zeroed MBarHeight.
    loaded.memory.write_u16_be(PPC_MBAR_HEIGHT_ADDR, 0).unwrap();
    let mut dlog = vec![0; 24];
    dlog[4..6].copy_from_slice(&100i16.to_be_bytes());
    dlog[6..8].copy_from_slice(&200i16.to_be_bytes());
    dlog[10] = 1;
    dlog[18..20].copy_from_slice(&128i16.to_be_bytes());
    dlog[20] = 1;
    dlog[21] = b'T';
    // alertPositionMainScreen
    dlog[22..24].copy_from_slice(&0x300Au16.to_be_bytes());
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

    assert_eq!(probe.unsupported_import_index, None);
    let dialog = loaded.cpu.gpr[3];
    assert_ne!(dialog, 0);
    let expected = expected_auto_position(&loaded, (0, 0, 100, 200), 0x300A, 0);
    assert_ne!(
        expected,
        expected_auto_position(&loaded, (0, 0, 100, 200), 0x300A, 20),
        "the alert position must depend on the menu bar height"
    );
    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog),
        Some(expected)
    );
}

#[test]
fn auto_position_dialog_uses_the_live_menu_bar_height() {
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"AutoPositionDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let dialog = window_manager::create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (40, 50, 140, 250),
        0,
        true,
        u32::MAX,
    );
    // A taller-than-default menu bar.
    loaded
        .memory
        .write_u16_be(PPC_MBAR_HEIGHT_ADDR, 44)
        .unwrap();
    let content = ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog).unwrap();

    // kWindowAlertPositionOnMainScreen
    loaded.cpu.gpr[3] = dialog;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
    );

    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    let expected = expected_auto_position(&loaded, content, 2, 44);
    assert_ne!(expected, expected_auto_position(&loaded, content, 2, 20));
    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, dialog),
        Some(expected)
    );
}

#[test]
fn dialog_sheet_window_filter_and_item_insertion_dispatch_with_canonical_evaluation() {
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    let parent_bounds_ptr = PPC_DATA_BASE + 0x1100;
    let out_buf = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"ShowSheetWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    loaded.memory.add_region(parent_bounds_ptr, vec![0; 32]);
    loaded.memory.add_region(out_buf, vec![0; 256]);

    let sheet_dialog = window_manager::create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (0, 0, 100, 200),
        0,
        false,
        u32::MAX,
    );
    let parent_window = window_manager::create_test_cwindow(
        &mut loaded,
        parent_bounds_ptr,
        (100, 100, 500, 700),
        0,
        true,
        u32::MAX,
    );

    // 1. GetDialogFilter & SetDialogFilter
    // 1a. GetDialogFilter error cases
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 1b. SetDialogFilter stores filter and GetDialogFilter reads it back
    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = 0x1122_3344;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogFilter,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u32_be(out_buf), Some(0x1122_3344));

    // 2. Sheet window routines
    // 2a. ShowSheetWindow NULL sheet returns paramErr
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = parent_window;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::ShowSheetWindow,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_PARAM_ERR);

    // 2b. ShowSheetWindow positions sheet and makes it visible
    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = parent_window;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::ShowSheetWindow,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(sheet_dialog + 104), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(sheet_dialog + crate::dialog_manager::DIALOG_SHEET_PARENT_OFFSET),
        Some(parent_window)
    );

    // 2c. GetSheetWindowParent reads parent window
    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = out_buf;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetSheetWindowParent,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u32_be(out_buf), Some(parent_window));

    // 2d. HideSheetWindow hides sheet
    loaded.cpu.gpr[3] = sheet_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::HideSheetWindow,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u8(sheet_dialog + 104), Some(0));

    // 3. InsertDialogItem & RemoveDialogItems
    let item1 = make_test_ditl_item(10, 10, 30, 80, b"OK");
    let ditl_bytes = make_test_ditl(&[item1]);
    let items_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &ditl_bytes,
    );
    loaded
        .memory
        .write_u32_be(sheet_dialog + PPC_DIALOG_ITEMS_OFFSET, items_handle)
        .unwrap();

    // 3a. CountDITL initially reports 1
    loaded.cpu.gpr[3] = sheet_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CountDitl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 3b. InsertDialogItem adds a second item
    let box_ptr = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(box_ptr, vec![0; 16]);
    loaded.memory.write_u16_be(box_ptr, 10).unwrap();
    loaded.memory.write_u16_be(box_ptr + 2, 90).unwrap();
    loaded.memory.write_u16_be(box_ptr + 4, 30).unwrap();
    loaded.memory.write_u16_be(box_ptr + 6, 160).unwrap();

    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = 1; // after item 1
    loaded.cpu.gpr[5] = 4; // button
    loaded.cpu.gpr[6] = 0; // handle
    loaded.cpu.gpr[7] = box_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::InsertDialogItem,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    loaded.cpu.gpr[3] = sheet_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CountDitl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 2);

    // 3c. RemoveDialogItems removes the second item
    loaded.cpu.gpr[3] = sheet_dialog;
    loaded.cpu.gpr[4] = 2; // item 2
    loaded.cpu.gpr[5] = 1; // amount 1
    loaded.cpu.gpr[6] = 0; // dispose_data = false
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RemoveDialogItems,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3] as i16, PPC_NO_ERR);

    loaded.cpu.gpr[3] = sheet_dialog;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CountDitl,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    // 4. Synthetic PEF binding across InterfaceLib, AppearanceLib, DialogsLib, and CarbonLib
    for (lib, symbol) in [
        (b"InterfaceLib".as_slice(), b"GetDialogFilter".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetDialogFilter".as_slice()),
        (b"DialogsLib".as_slice(), b"GetDialogFilter".as_slice()),
        (b"CarbonLib".as_slice(), b"GetDialogFilter".as_slice()),
        (b"InterfaceLib".as_slice(), b"ShowSheetWindow".as_slice()),
        (b"AppearanceLib".as_slice(), b"ShowSheetWindow".as_slice()),
        (b"DialogsLib".as_slice(), b"ShowSheetWindow".as_slice()),
        (b"CarbonLib".as_slice(), b"ShowSheetWindow".as_slice()),
        (b"InterfaceLib".as_slice(), b"HideSheetWindow".as_slice()),
        (b"AppearanceLib".as_slice(), b"HideSheetWindow".as_slice()),
        (b"DialogsLib".as_slice(), b"HideSheetWindow".as_slice()),
        (b"CarbonLib".as_slice(), b"HideSheetWindow".as_slice()),
        (b"InterfaceLib".as_slice(), b"GetSheetWindowParent".as_slice()),
        (b"AppearanceLib".as_slice(), b"GetSheetWindowParent".as_slice()),
        (b"DialogsLib".as_slice(), b"GetSheetWindowParent".as_slice()),
        (b"CarbonLib".as_slice(), b"GetSheetWindowParent".as_slice()),
        (b"InterfaceLib".as_slice(), b"InsertDialogItem".as_slice()),
        (b"AppearanceLib".as_slice(), b"InsertDialogItem".as_slice()),
        (b"DialogsLib".as_slice(), b"InsertDialogItem".as_slice()),
        (b"CarbonLib".as_slice(), b"InsertDialogItem".as_slice()),
        (b"InterfaceLib".as_slice(), b"RemoveDialogItems".as_slice()),
        (b"AppearanceLib".as_slice(), b"RemoveDialogItems".as_slice()),
        (b"DialogsLib".as_slice(), b"RemoveDialogItems".as_slice()),
        (b"CarbonLib".as_slice(), b"RemoveDialogItems".as_slice()),
    ] {
        let pef = synthetic_pef_with_library_import(lib, symbol);
        let mut loaded_app = load_pef_application(&pef).unwrap();
        loaded_app.cpu.gpr[3] = 0;
        let probe = loaded_app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
    }
}
