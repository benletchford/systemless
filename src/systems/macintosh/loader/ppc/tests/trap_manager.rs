use super::*;

#[test]
fn hle_import_runner_handles_trap_address_queries() {
    for (symbol, target, toolbox) in [
        (
            b"NGetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NGetTrapAddress,
            true,
        ),
        (
            b"GetToolTrapAddress".as_slice(),
            PpcImportDispatcherTarget::GetToolTrapAddress,
            true,
        ),
        (
            b"GetOSTrapAddress".as_slice(),
            PpcImportDispatcherTarget::GetOSTrapAddress,
            false,
        ),
    ] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = 0xFFFF_A800;
        loaded.cpu.gpr[4] = u32::from(toolbox);
        let expected: u32 = if toolbox { 0x1234_5678 } else { 0x8765_4321 };
        loaded.memory.add_region(
            ppc_raw_trap_table_entry(0xA800, toolbox),
            expected.to_be_bytes().to_vec(),
        );

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "{symbol:?}");
        assert_eq!(probe.unsupported_import_index, None, "{symbol:?}");
        assert_eq!(loaded.cpu.gpr[3], expected, "{symbol:?}");
    }
}

#[test]
fn hle_import_runner_handles_supported_trap_address_setters() {
    for (symbol, target, toolbox, general) in [
        (
            b"SetToolTrapAddress".as_slice(),
            PpcImportDispatcherTarget::SetToolTrapAddress,
            true,
            false,
        ),
        (
            b"SetOSTrapAddress".as_slice(),
            PpcImportDispatcherTarget::SetOSTrapAddress,
            false,
            false,
        ),
        (
            b"NSetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NSetTrapAddress,
            true,
            true,
        ),
        (
            b"NSetTrapAddress".as_slice(),
            PpcImportDispatcherTarget::NSetTrapAddress,
            false,
            true,
        ),
    ] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.imports[0].dispatcher_target = target;
        let trap_word = 0xFFFF_A823u32;
        let entry = ppc_raw_trap_table_entry(trap_word as u16, toolbox);
        loaded
            .memory
            .add_region(entry, 0x1234_5678u32.to_be_bytes().to_vec());
        loaded.cpu.gpr[3] = PPC_CODE_BASE;
        loaded.cpu.gpr[4] = trap_word;
        loaded.cpu.gpr[5] = u32::from(general && toolbox);

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "{symbol:?}");
        assert_eq!(probe.unsupported_import_index, None, "{symbol:?}");
        assert_eq!(loaded.memory.read_u32_be(entry), Some(PPC_CODE_BASE));
    }
}

#[test]
fn native_trap_apis_do_not_promote_a_writable_signature_to_a_protected_head() {
    let trap_word = 0xA823;
    let table_entry = ppc_raw_trap_table_entry(trap_word, true);
    let writable_head: u32 = 0x0030_0000;
    let apparent_successor: u32 = 0x0040_0000;
    let replacement: u32 = 0x0050_0000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(table_entry, writable_head.to_be_bytes().to_vec());
    let mut head_bytes = Vec::from(
        crate::trap::manager::COME_FROM_PATCH_SIGNATURE.to_be_bytes(),
    );
    head_bytes.extend_from_slice(&apparent_successor.to_be_bytes());
    memory.add_region(writable_head, head_bytes);

    assert_eq!(
        ppc_logical_trap_address(&mut memory, trap_word, true),
        Some(writable_head)
    );
    assert!(ppc_set_logical_trap_address(
        &mut memory,
        trap_word,
        true,
        replacement,
    ));
    assert_eq!(memory.read_u32_be(table_entry), Some(replacement));
    assert_eq!(
        memory.read_u32_be(writable_head + 4),
        Some(apparent_successor)
    );
}

#[test]
fn native_trap_apis_share_permanent_come_from_topology() {
    let pef = synthetic_pef_with_import(b"NGetTrapAddress");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
    let mut dispatcher = TrapDispatcher::new();
    dispatcher
        .materialize_trap_tables(
            &mut bus,
            crate::trap::dispatch::TrapTableProfile::PowerPc604,
        )
        .expect("trap table construction requires writable cells and system storage");
    let trap_word = 0xA823u16;
    let table_entry = ppc_raw_trap_table_entry(trap_word, true);
    let head = bus.read_long(table_entry);
    let original = bus.read_long(head + 4);
    let second_head = bus.alloc_synthetic(10);
    bus.write_readonly_code_word(second_head, 0x6006);
    bus.write_readonly_code_word(second_head + 2, 0x4ef9);
    bus.write_readonly_code_word(second_head + 4, (original >> 16) as u16);
    bus.write_readonly_code_word(second_head + 6, original as u16);
    bus.write_readonly_code_word(second_head + 8, 0x60f8);
    bus.protect_readonly_code(second_head, 10);
    bus.write_readonly_code_word(head + 4, (second_head >> 16) as u16);
    bus.write_readonly_code_word(head + 6, second_head as u16);
    for (base, len) in [
        (
            crate::trap::dispatch::OS_TRAP_TABLE_BASE,
            u32::from(crate::trap::dispatch::OS_TRAP_TABLE_SLOTS) * 4,
        ),
        (
            crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE,
            u32::from(crate::trap::dispatch::TOOLBOX_TRAP_TABLE_SLOTS) * 4,
        ),
    ] {
        let region = bus.shared_ram_region(base, len).unwrap();
        // SAFETY: this focused fixture serializes the two adapters.
        unsafe { loaded.memory.add_shared_region(base, region) };
    }
    let (synthetic_base, synthetic) = bus.shared_synthetic_reservation().unwrap();
    // SAFETY: this focused fixture serializes the two adapters.
    unsafe {
        loaded.memory.add_shared_readonly_region(
            Some(GuestIsa::M68k),
            synthetic_base,
            synthetic,
        )
    };
    let ds_err = bus
        .shared_ram_region(crate::memory::globals::addr::DS_ERR_CODE, 2)
        .unwrap();
    // SAFETY: this focused fixture serializes the two adapters.
    unsafe {
        loaded
            .memory
            .add_shared_region(crate::memory::globals::addr::DS_ERR_CODE, ds_err)
    };

    assert_eq!(bus.read_long(head), 0x6006_4ef9);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head), 0x6006_4ef9);
    assert_eq!(bus.read_long(second_head + 4), original);

    // The 68K Trap Manager and the PPC import adapter must observe one
    // raw guest table, including the hidden successor of a permanent
    // come-from chain.  Mutate the chain through the 68K-side service and
    // read it through the PPC side before exercising the inverse route.
    assert_eq!(
        dispatcher.trap_table_address(&bus, trap_word),
        Some(original)
    );
    let m68k_patch = 0x0022_0000;
    dispatcher
        .install_trap_address(&mut bus, trap_word, m68k_patch)
        .expect("68K patch must install into the materialized table");
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), m68k_patch);

    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], m68k_patch);

    loaded.cpu.gpr[3] = 0xAA6E;
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    let unimplemented = loaded.cpu.gpr[3];

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0xAA57;
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_ne!(loaded.cpu.gpr[3], unimplemented);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], m68k_patch);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NSetTrapAddress;
    loaded.cpu.gpr[3] = head;
    loaded.cpu.gpr[4] = u32::from(trap_word);
    loaded.cpu.gpr[5] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), m68k_patch);

    let replacement = PPC_CODE_BASE;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = replacement;
    loaded.cpu.gpr[4] = u32::from(trap_word);
    loaded.cpu.gpr[5] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(bus.read_long(table_entry), head);
    assert_eq!(bus.read_long(head + 4), second_head);
    assert_eq!(bus.read_long(second_head + 4), replacement);
    assert_eq!(loaded.memory.write_u32_be(second_head + 4, original), None);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NGetTrapAddress;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], replacement);

    loaded
        .memory
        .write_u32_be(table_entry, original)
        .expect("raw trap table remains guest-writable");
    assert_eq!(bus.read_long(table_entry), original);
    assert_eq!(
        dispatcher.trap_table_address(&bus, trap_word),
        Some(original),
        "68K getter must observe a PPC-side raw table overwrite"
    );
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from(trap_word);
    loaded.cpu.gpr[4] = 1;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], original);
}
