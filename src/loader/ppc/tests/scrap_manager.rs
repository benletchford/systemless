use super::*;

#[test]
fn native_textedit_and_scrap_manager_share_the_text_flavor() {
    let pef = synthetic_pef_with_import(b"TECopy");
    let mut loaded = load_pef_application(&pef).unwrap();
    let rects = PPC_DATA_BASE + 0x1000;
    let offset_ptr = rects + 0x20;
    loaded.memory.add_region(rects, vec![0; 0x40]);
    ppc_write_rect(&mut loaded.memory, rects, 0, 0, 80, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, rects + 8, 0, 0, 80, 220).unwrap();
    let mut last_mem_error = PPC_NO_ERR;
    let te_handle = ppc_te_initialize_record(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
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
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        te_handle,
        b"Pilot Name",
    );
    let te_ptr = loaded.memory.read_u32_be(te_handle).unwrap();
    loaded
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0)
        .unwrap();
    loaded
        .memory
        .write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, 5)
        .unwrap();
    loaded.cpu.gpr[3] = te_handle;
    loaded.run_with_hle_imports(64);
    assert_eq!(ppc_te_scrap_bytes(&mut loaded.memory), b"Pilot");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ZeroScrap;
    loaded.run_with_hle_imports(64);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::TETransferScrap {
        from_desktop: false,
    };
    loaded.run_with_hle_imports(64);

    let destination = ppc_alloc_handle(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        0,
        true,
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetScrap;
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[5] = offset_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 5);
    assert_eq!(loaded.memory.read_u32_be(offset_ptr), Some(0));
    assert_eq!(
        ppc_handle_bytes(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            destination
        ),
        Some(b"Pilot".to_vec())
    );
}

#[test]
fn attached_desktop_scrap_mutations_cross_isa_immediately() {
    let pef = synthetic_pef_with_import(b"TestImport");
    let mut native = load_pef_application(&pef).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    run_test_import(&mut native, PpcImportDispatcherTarget::ZeroScrap);
    let native_source = PPC_DATA_BASE + 0x2600;
    native
        .memory
        .add_region(native_source, b"native scrap".to_vec());
    native.cpu.gpr[3] = 12;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[5] = native_source;
    run_test_import(&mut native, PpcImportDispatcherTarget::PutScrap);
    assert_eq!(native.cpu.gpr[3], 0);

    let classic_offset = 0x0002_7000;
    classic_bus.write_long(TEST_SP, classic_offset);
    classic_bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"TEXT"));
    classic_bus.write_long(TEST_SP + 8, 0);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x1FD, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_long(TEST_SP + 12), 12);
    assert_eq!(classic_bus.read_long(classic_offset), 8);

    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x1FC, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    let classic_source = 0x0002_7100;
    for (offset, byte) in b"classic scrap".iter().copied().enumerate() {
        classic_bus.write_byte(classic_source + offset as u32, byte);
    }
    classic_bus.write_long(TEST_SP, classic_source);
    classic_bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"PICT"));
    classic_bus.write_long(TEST_SP + 8, 13);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x1FE, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_long(TEST_SP + 12), 0);

    let native_offset = PPC_DATA_BASE + 0x2680;
    native.memory.add_region(native_offset, vec![0; 4]);
    native.cpu.gpr[3] = 0;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"PICT");
    native.cpu.gpr[5] = native_offset;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetScrap);
    assert_eq!(native.cpu.gpr[3], 13);
    assert_eq!(native.memory.read_u32_be(native_offset), Some(0));
    assert_eq!(native.scrap.desktop.count, 2);
}

#[test]
fn attached_textedit_features_cross_isa_immediately() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let te_handle = 0x0033_1000;

    classic_bus.write_long(TEST_SP, te_handle);
    classic_bus.write_byte(TEST_SP + 4, 1);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_dialog(true, 0x013, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(native.scrap.text_edit.feature_bit(te_handle, 0));

    native.cpu.gpr[3] = 0;
    native.cpu.gpr[4] = te_handle;
    run_test_import(&mut native, PpcImportDispatcherTarget::TEAutoView);
    assert!(!classic.te_auto_scroll_enabled(te_handle));

    native.cpu.gpr[3] = 1;
    native.cpu.gpr[4] = te_handle;
    run_test_import(&mut native, PpcImportDispatcherTarget::TEAutoView);
    assert!(classic.te_auto_scroll_enabled(te_handle));

    native.cpu.gpr[3] = te_handle;
    run_test_import(&mut native, PpcImportDispatcherTarget::TEDispose);
    assert!(!classic.te_auto_scroll_enabled(te_handle));
}

#[test]
fn attached_textedit_scrap_uses_canonical_low_memory_globals() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, _classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    let ram_end = classic_bus.ram_size();
    for (base, end) in native.memory.mapping_holes(0x0010_0000, ram_end) {
        let memory = classic_bus
            .shared_ram_region(base, end - base)
            .expect("classic adapter owns the native mapping hole");
        context.attach_memory(base, memory, &mut native.memory);
    }
    let shared = native.memory.shared_view();
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);

    run_test_import(&mut native, PpcImportDispatcherTarget::TEInit);
    let native_handle = native
        .memory
        .read_u32_be(crate::memory::globals::addr::TE_SCRP_HANDLE)
        .unwrap();
    assert_ne!(native_handle, 0);
    assert_eq!(
        classic_bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
        native_handle
    );
    assert_eq!(
        classic_bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
        0
    );

    native
        .scrap
        .desktop
        .replace_entries(vec![(*b"TEXT", b"Native".to_vec())]);
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::TETransferScrap { from_desktop: true },
    );
    let native_ptr = classic_bus.read_long(native_handle);
    assert_eq!(
        classic_bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
        6
    );
    assert_eq!(classic_bus.read_bytes(native_ptr, 6), b"Native");

    let (classic_handle, classic_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 7)
        .unwrap();
    classic_bus.write_bytes(classic_ptr, b"Classic");
    classic_bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, classic_handle);
    classic_bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 7);

    run_test_import(&mut native, PpcImportDispatcherTarget::TEScrapHandle);
    assert_eq!(native.cpu.gpr[3], classic_handle);
    run_test_import(
        &mut native,
        dispatcher_target_for_import("InterfaceLib", "TEGetScrapLength"),
    );
    assert_eq!(native.cpu.gpr[3], 7);
    assert_eq!(ppc_te_scrap_bytes(&mut native.memory), b"Classic");
}

#[test]
fn cloned_native_adapter_detaches_textedit_feature_state() {
    let original = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original
        .scrap
        .text_edit
        .set_feature_bit(0x0033_1000, 0, true);
    let detached = original.clone();

    detached
        .scrap
        .text_edit
        .set_feature_bit(0x0033_1000, 0, false);
    detached
        .scrap
        .text_edit
        .set_feature_bit(0x0033_2000, 2, true);

    assert!(original.scrap.text_edit.feature_bit(0x0033_1000, 0));
    assert!(!original.scrap.text_edit.feature_bit(0x0033_2000, 2));
    assert!(!detached.scrap.text_edit.feature_bit(0x0033_1000, 0));
}

#[test]
fn cloned_native_adapter_detaches_desktop_scrap() {
    let mut original = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    run_test_import(&mut original, PpcImportDispatcherTarget::ZeroScrap);
    let mut detached = original.clone();
    let source = PPC_DATA_BASE + 0x2600;
    detached.memory.add_region(source, b"detached".to_vec());
    detached.cpu.gpr[3] = 8;
    detached.cpu.gpr[4] = u32::from_be_bytes(*b"TEXT");
    detached.cpu.gpr[5] = source;
    run_test_import(&mut detached, PpcImportDispatcherTarget::PutScrap);

    assert!(original.scrap.desktop.entries.is_empty());
    assert_eq!(original.scrap.desktop.count, 1);
    assert_eq!(
        detached.scrap.desktop.entries,
        vec![(*b"TEXT", b"detached".to_vec())]
    );
    assert_eq!(detached.scrap.desktop.count, 1);
}
