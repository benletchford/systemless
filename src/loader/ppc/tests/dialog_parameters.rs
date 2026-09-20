use super::*;

#[test]
fn hle_import_runner_handles_param_text_strings() {
    let pef = synthetic_pef_with_import(b"ParamText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    let values: [&[u8]; 4] = [b"alpha", b"bravo", b"chi", b"d"];
    for (index, value) in values.iter().enumerate() {
        let ptr = scratch + index as u32 * 16;
        write_ppc_pstring(&mut loaded.memory, ptr, value);
        loaded.cpu.gpr[3 + index] = ptr;
    }

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.param_text.slot(0).as_deref(), Some(b"alpha".as_slice()));
    assert_eq!(loaded.param_text.slot(1).as_deref(), Some(b"bravo".as_slice()));
    assert_eq!(loaded.param_text.slot(2).as_deref(), Some(b"chi".as_slice()));
    assert_eq!(loaded.param_text.slot(3).as_deref(), Some(b"d".as_slice()));
}

#[test]
fn ppc_param_text_expands_only_numbered_placeholders() {
    let params = [
        b"first".to_vec(),
        Vec::new(),
        b"third".to_vec(),
        b"fourth".to_vec(),
    ];

    assert_eq!(
        ppc_apply_param_text(b"^0/^1/^2/^3", &params).as_ref(),
        b"first//third/fourth"
    );
    assert_eq!(
        ppc_apply_param_text(b"^^0 ^9 trailing^", &params).as_ref(),
        b"^first ^9 trailing^"
    );
    assert!(matches!(
        ppc_apply_param_text(b"plain", &params),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn hle_import_runner_handles_legacy_lowercase_param_text_symbol() {
    let pef = synthetic_pef_with_import(b"paramtext");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string = PPC_HEAP_BASE;
    loaded.memory.add_region(string, vec![0; 16]);
    write_ppc_pstring(&mut loaded.memory, string, b"legacy");
    loaded.cpu.gpr[3] = string;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.param_text.slot(0).as_deref(), Some(b"legacy".as_slice()));
}

#[test]
fn hle_import_runner_param_text_nil_preserves_previous_slots() {
    let pef = synthetic_pef_with_import(b"ParamText");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.param_text.set_slots([
        b"old0".to_vec(),
        b"old1".to_vec(),
        b"old2".to_vec(),
        b"old3".to_vec(),
    ]);
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 32]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"new0");
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.param_text.slot(0).as_deref(), Some(b"new0".as_slice()));
    assert_eq!(loaded.param_text.slot(1).as_deref(), Some(b"old1".as_slice()));
    assert_eq!(loaded.param_text.slot(2).as_deref(), Some(b"old2".as_slice()));
    assert_eq!(loaded.param_text.slot(3).as_deref(), Some(b"old3".as_slice()));
}

#[test]
fn attached_dialog_parameter_text_mutations_cross_isa_immediately() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"ParamText")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let native_text = PPC_DATA_BASE + 0x2900;
    native.memory.add_region(native_text, vec![0; 32]);
    write_ppc_pstring(&mut native.memory, native_text, b"Native");
    native.cpu.gpr[3] = native_text;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::ParamText);
    assert_eq!(classic.apply_param_text("Hello ^0"), "Hello Native");

    let classic_text = 0x0002_9000;
    classic_bus.write_pstring(classic_text, b"Classic");
    classic_bus.write_long(TEST_SP, 0);
    classic_bus.write_long(TEST_SP + 4, 0);
    classic_bus.write_long(TEST_SP + 8, 0);
    classic_bus.write_long(TEST_SP + 12, classic_text);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_dialog(true, 0x18B, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(native.param_text.slot(0).as_deref(), Some(b"Classic".as_slice()));
    assert_eq!(native.param_text.slot(1).as_deref(), Some(b"".as_slice()));
}

#[test]
fn cloned_native_adapter_detaches_dialog_parameter_text() {
    let original =
        load_pef_application(&synthetic_pef_with_import(b"ParamText")).unwrap();
    original.param_text.set_slot(0, b"Original".to_vec());
    let mut detached = original.clone();
    let text = PPC_DATA_BASE + 0x2900;
    detached.memory.add_region(text, vec![0; 32]);
    write_ppc_pstring(&mut detached.memory, text, b"Detached");
    detached.cpu.gpr[3] = text;
    detached.cpu.gpr[4] = 0;
    detached.cpu.gpr[5] = 0;
    detached.cpu.gpr[6] = 0;
    run_test_import(&mut detached, PpcImportDispatcherTarget::ParamText);

    assert_eq!(original.param_text.slot(0).as_deref(), Some(b"Original".as_slice()));
    assert_eq!(detached.param_text.slot(0).as_deref(), Some(b"Detached".as_slice()));
}

#[test]
fn hle_import_runner_returns_minus_one_for_missing_note_alert() {
    let pef = synthetic_pef_with_import(b"NoteAlert");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 128;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
}

#[test]
fn hle_alert_materializes_expanded_param_text() {
    let pef = synthetic_pef_with_import(b"Alert");
    let mut loaded = load_pef_application(&pef).unwrap();
    let alert_id = 128i16;
    let mut alert = vec![0; 14];
    for (offset, value) in [(0, 130i16), (2, 150), (4, 260), (6, 450)] {
        alert[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    alert[8..10].copy_from_slice(&alert_id.to_be_bytes());
    alert[10..12].copy_from_slice(&0x8888u16.to_be_bytes());

    let mut ditl = vec![0; 34];
    ditl[0..2].copy_from_slice(&1i16.to_be_bytes());
    for (offset, value) in [(6, 20i16), (8, 20), (10, 70), (12, 280)] {
        ditl[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    ditl[14] = PPC_DIALOG_ITEM_STATIC_TEXT | PPC_DIALOG_ITEM_DISABLED;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"^0");
    for (offset, value) in [(22, 90i16), (24, 210), (26, 110), (28, 280)] {
        ditl[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    ditl[30] = PPC_DIALOG_ITEM_BUTTON;
    ditl[31] = 2;
    ditl[32..34].copy_from_slice(b"OK");

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
    loaded
        .param_text
        .set_slot(0, b"Configured for this display".to_vec());
    loaded.cpu.gpr[3] = alert_id as u16 as u32;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let dialog = *loaded.current_gworld;
    let items_handle = loaded
        .memory
        .read_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET)
        .unwrap();
    let items_ptr = loaded.memory.read_u32_be(items_handle).unwrap();
    let text_handle = loaded.memory.read_u32_be(items_ptr + 2).unwrap();
    assert_eq!(
        ppc_handle_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            text_handle
        ),
        Some(b"Configured for this display".to_vec())
    );

    loaded.param_text.set_slot(0, b"later value".to_vec());
    assert_eq!(
        ppc_handle_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            text_handle
        ),
        Some(b"Configured for this display".to_vec()),
        "ParamText applies when the alert is created"
    );
}

#[test]
fn hle_import_runner_keeps_resource_alert_modal_until_default_item() {
    let pef = synthetic_pef_with_import(b"Alert");
    let mut loaded = load_pef_application(&pef).unwrap();
    let alert_id = 128i16;
    let mut alert = vec![0; 14];
    for (offset, value) in [(0, 130i16), (2, 150), (4, 260), (6, 450)] {
        alert[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    alert[8..10].copy_from_slice(&alert_id.to_be_bytes());
    alert[10..12].copy_from_slice(&0x4444u16.to_be_bytes());
    let mut ditl = vec![0; 18];
    for (offset, value) in [(6, 90i16), (8, 210), (10, 110), (12, 280)] {
        ditl[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    ditl[14] = PPC_DIALOG_ITEM_BUTTON;
    ditl[15] = 2;
    ditl[16..18].copy_from_slice(b"OK");
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
    loaded.cpu.gpr[3] = alert_id as u16 as u32;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let dialog = *loaded.current_gworld;
    assert_eq!(loaded.window_list.first(), Some(dialog));
    assert_eq!(
        loaded
            .memory
            .read_u16_be(dialog + PPC_DIALOG_RESOURCE_ID_OFFSET),
        Some(alert_id as u16)
    );
    assert!(ppc_window_is_visible(&mut loaded.memory, dialog));

    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (u32::from(PPC_KEY_RETURN) << 8) | 0x0d,
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
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(!loaded.gworlds.iter().any(|record| record.port == dialog));
    assert!(!loaded.window_list.contains(&dialog));
    assert_eq!(loaded.memory.read_u32_be(0x09D6), Some(0));
}
