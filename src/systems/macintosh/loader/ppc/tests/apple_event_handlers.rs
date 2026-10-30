use super::*;

#[test]
fn ppc_coercion_handler_registers_queries_and_calls_descriptor_handler() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [0x90a6_0000, 0x9086_0004, 0x3860_0000, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    loaded.memory.write_u32_be(source, from).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[
        from,
        to,
        scratch + 0x110,
        scratch + 0x114,
        scratch + 0x118,
        0,
    ]);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEGetCoercionHandler);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x110), Some(handler));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x114),
        Some(0x1234_5678)
    );
    assert_eq!(loaded.memory.read_u8(scratch + 0x118), Some(1));
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(to));
    // AECoercePtr wraps its bytes in a temporary descriptor for this handler.
    for (offset, word) in [
        0x80e3_0000, // lwz r7, 0(r3): descriptor type
        0x8103_0004, // lwz r8, 4(r3): handle
        0x8108_0000, // lwz r8, 0(r8): data pointer
        0x8128_0000, // lwz r9, 0(r8): data
        0x90e6_0000, // stw r7, 0(r6)
        0x9126_0004, // stw r9, 4(r6)
        0x3860_0000,
        0x4e80_0020,
    ]
    .into_iter()
    .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded
        .memory
        .write_u32_be(scratch + 0x120, 0x4142_4344)
        .unwrap();
    let original_handles = loaded.handles();
    let original_ptrs = loaded.ptrs();
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(from));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0x4142_4344));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);

    // An error returned by the guest still releases the temporary descriptor.
    loaded.memory.write_u32_be(entry, 0x3860_ffce).unwrap(); // li r3, -50
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);

    // An invalid handler fails before entering guest code and also cleans up.
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, 0xdead_beec, 0, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    // A declined application coercion continues to the system handler.
    let system_handler = scratch + 0x140;
    let system_entry = scratch + 0x160;
    loaded
        .memory
        .write_u32_be(system_handler, system_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(system_handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [0x90a6_0000, 0x9086_0004, 0x3860_0000, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(system_entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(entry, 0x3860_f95c).unwrap(); // li r3, -1700
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, system_handler, 0x9876_5432, 1, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x9876_5432));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(to));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, handler, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AERemoveCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[
        from,
        to,
        scratch + 0x110,
        scratch + 0x114,
        scratch + 0x118,
        0,
    ]);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEGetCoercionHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1717));
}

#[test]
fn ppc_declined_coercion_falls_back_to_builtin_conversion() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let output = scratch + 0x100;
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    loaded.memory.write_u32_be(entry, 0x3860_f95c).unwrap(); // li r3, -1700
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    let from = u32::from_be_bytes(*b"long");
    let to = u32::from_be_bytes(*b"doub");
    loaded.memory.write_u32_be(scratch + 0x120, 42).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let wildcard_handler = scratch + 0x48;
    let wildcard_entry = scratch + 0x90;
    loaded
        .memory
        .write_u32_be(wildcard_handler, wildcard_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(wildcard_handler + 4, loaded.rtoc)
        .unwrap();
    loaded
        .memory
        .write_u32_be(wildcard_entry, 0x3860_ffce)
        .unwrap();
    loaded
        .memory
        .write_u32_be(wildcard_entry + 4, 0x4e80_0020)
        .unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[PPC_TYPE_WILDCARD, to, wildcard_handler, 0, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));
    let handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let data = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(data), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(data + 4), Some(0));
}

#[test]
fn ppc_descriptor_coercion_retries_after_failed_builtin_conversion() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let source = scratch + 0x100;
    let output = scratch + 0x110;
    let count = scratch + 0x120;
    let from = u32::from_be_bytes(*b"Z001");
    let to = u32::from_be_bytes(*b"Z002");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [
        0x80e5_0000,
        0x38e7_0001,
        0x90e5_0000,
        0x2c07_0002,
        0x4182_000c,
        0x3860_f95c,
        0x4e80_0020,
        0x9086_0000,
        0x3860_0000,
        0x9066_0004,
        0x4e80_0020,
    ]
    .into_iter()
    .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, 0, 0, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, count, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(count), Some(2));
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));

    // A handler that keeps declining is called at most twice and leaves a
    // null result, even when no built-in conversion exists.
    loaded.memory.write_u32_be(count, 0).unwrap();
    loaded.memory.write_u32_be(entry + 12, 0x2c07_0003).unwrap();
    let handles = loaded.handles();
    let ptrs = loaded.ptrs();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
    assert_eq!(loaded.memory.read_u32_be(count), Some(2));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), ptrs);

    // Errors other than errAECoercionFail stop without a second pass.
    loaded.memory.write_u32_be(count, 0).unwrap();
    loaded.memory.write_u32_be(entry + 20, 0x3860_ffce).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(count), Some(1));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), ptrs);

    // A successful built-in conversion ends the first pass.
    let numeric_source = scratch + 0x130;
    let numeric_data = scratch + 0x140;
    let numeric_from = u32::from_be_bytes(*b"long");
    let numeric_to = u32::from_be_bytes(*b"doub");
    loaded.memory.write_u32_be(count, 0).unwrap();
    loaded.memory.write_u32_be(entry + 20, 0x3860_f95c).unwrap();
    loaded.memory.write_u32_be(numeric_data, 42).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[numeric_from, numeric_data, 4, numeric_source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[numeric_from, numeric_to, handler, count, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[numeric_source, numeric_to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(count), Some(1));
    assert_eq!(loaded.memory.read_u32_be(output), Some(numeric_to));
}

#[test]
fn ppc_descriptor_second_pass_uses_replacement_handler() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let replacement = scratch + 0x50;
    let entry = scratch + 0x80;
    let replacement_entry = scratch + 0xc0;
    let source = scratch + 0x100;
    let output = scratch + 0x110;
    let from = u32::from_be_bytes(*b"Z001");
    let to = u32::from_be_bytes(*b"Z002");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    for (pointer, code) in [(handler, entry), (replacement, replacement_entry)] {
        loaded.memory.write_u32_be(pointer, code).unwrap();
        loaded
            .memory
            .write_u32_be(pointer + 4, loaded.rtoc)
            .unwrap();
    }
    for (offset, word) in [0x9086_0000, 0x3860_0000, 0x9066_0004, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(replacement_entry + offset as u32 * 4, word)
            .unwrap();
    }
    let mut install = loaded.imports[0].clone();
    install.symbol_index = 1;
    install.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(install.trap_pc, vec![0; 4]);
    loaded.imports.push(install);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(32, 3, 3, 0),
        d_form_u(14, 6, 0, 0),
        d_form_u(14, 7, 0, 1),
        d_form_u(14, 8, 0, 0),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, (-1700i16) as u16),
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, 0, 0, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, replacement, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoerceDesc,
    );
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
}

#[test]
fn ppc_declined_coercion_uses_current_handler_table() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let system_handler = scratch + 0x50;
    let entry = scratch + 0x80;
    let system_entry = scratch + 0x60;
    let output = scratch + 0x100;
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    for (pointer, code) in [(handler, entry), (system_handler, system_entry)] {
        loaded.memory.write_u32_be(pointer, code).unwrap();
        loaded
            .memory
            .write_u32_be(pointer + 4, loaded.rtoc)
            .unwrap();
    }
    loaded
        .memory
        .write_u32_be(system_entry, 0x3860_ffce)
        .unwrap(); // li r3, -50
    loaded
        .memory
        .write_u32_be(system_entry + 4, 0x4e80_0020)
        .unwrap();
    let mut removal = loaded.imports[0].clone();
    removal.symbol_index = 1;
    removal.symbol_name = "AERemoveCoercionHandler".into();
    removal.dispatcher_target = PpcImportDispatcherTarget::AERemoveCoercionHandler;
    removal.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(removal.trap_pc, vec![0; 4]);
    loaded.imports.push(removal);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6, // mflr r0
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 4, 6, 0), // to type
        d_form_u(14, 5, 7, 0), // system handler
        d_form_u(14, 6, 0, 1), // system table
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6, // mtlr r0
        d_form_u(14, 3, 0, (-1700i16) as u16),
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    let from = u32::from_be_bytes(*b"long");
    let to = u32::from_be_bytes(*b"doub");
    loaded.memory.write_u32_be(scratch + 0x120, 42).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, system_handler, 0, 0, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, system_handler, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch + 0x120, 4, to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoercePtr,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));
    let handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let data = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(data), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(data + 4), Some(0));
}

#[test]
fn ppc_descriptor_coercion_and_duplication_read_live_handle_data() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECreateDesc")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    let long = u32::from_be_bytes(*b"long");
    let double = u32::from_be_bytes(*b"doub");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(scratch + 0x120, 42).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[long, scratch + 0x120, 4, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let source_handle = loaded.memory.read_u32_be(source + 4).unwrap();
    let source_data = loaded.memory.read_u32_be(source_handle).unwrap();
    loaded.memory.write_u32_be(source_data, 100).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, double, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(double));
    let result_handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let result_data = loaded.memory.read_u32_be(result_handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(result_data), Some(0x4059_0000));
    assert_eq!(loaded.memory.read_u32_be(result_data + 4), Some(0));
    loaded.cpu.gpr[3..5].copy_from_slice(&[source, output + 8]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DuplicateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let duplicate_handle = loaded.memory.read_u32_be(output + 12).unwrap();
    let duplicate_data = loaded.memory.read_u32_be(duplicate_handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(duplicate_data), Some(100));

    // Replacing a descriptor's handle with nil must not reuse cached bytes or type.
    let changed_type = u32::from_be_bytes(*b"type");
    loaded.memory.write_u32_be(source, changed_type).unwrap();
    loaded.memory.write_u32_be(source + 4, 0).unwrap();
    loaded.cpu.gpr[3..5].copy_from_slice(&[source, output + 16]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DuplicateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output + 16), Some(changed_type));
    let empty_handle = loaded.memory.read_u32_be(output + 20).unwrap();
    assert_eq!(
        loaded
            .handles()
            .iter()
            .find(|record| record.handle == empty_handle)
            .unwrap()
            .size,
        0
    );
    loaded.memory.write_u32_be(source, long).unwrap();
    loaded
        .memory
        .write_u32_be(source + 4, source_handle)
        .unwrap();
}

#[test]
fn ppc_put_descriptor_copies_live_handle_data() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECreateList")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let list = scratch + 0x100;
    let source = scratch + 0x108;
    let long = u32::from_be_bytes(*b"long");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.cpu.gpr[3..7].copy_from_slice(&[0, 0, 0, list]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(scratch + 0x120, 42).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[long, scratch + 0x120, 4, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let handle = loaded.memory.read_u32_be(source + 4).unwrap();
    let data = loaded.memory.read_u32_be(handle).unwrap();
    loaded.memory.write_u32_be(data, 100).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[list, 0, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(data, 200).unwrap();
    loaded.cpu.gpr[3..11].copy_from_slice(&[
        list,
        1,
        PPC_TYPE_WILDCARD,
        0,
        scratch + 0x140,
        scratch + 0x144,
        4,
        scratch + 0x148,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x140), Some(long));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x144), Some(100));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x148), Some(4));

    let record = scratch + 0x150;
    let keyword = u32::from_be_bytes(*b"data");
    loaded.cpu.gpr[3..7].copy_from_slice(&[0, 0, 1, record]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[record, keyword, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(data, 300).unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        record,
        keyword,
        PPC_TYPE_WILDCARD,
        scratch + 0x160,
        scratch + 0x164,
        4,
        scratch + 0x168,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x160), Some(long));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x164), Some(200));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x168), Some(4));
}

#[test]
fn ppc_descriptor_getters_call_installed_coercion_handler() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let record = scratch + 0x100;
    let output = scratch + 0x108;
    let keyword_ptr = scratch + 0x110;
    let data = scratch + 0x120;
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    let keyword = u32::from_be_bytes(*b"data");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [0x90a6_0000, 0x9086_0004, 0x3860_0000, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..7].copy_from_slice(&[0, 0, 1, record]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(data, 0x4142_4344).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, keyword, from, data, 4]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let original_handles = loaded.handles();
    let original_ptrs = loaded.ptrs();
    loaded.cpu.gpr[3..7].copy_from_slice(&[record, keyword, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(to));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    // A failing guest handler must not leave its partial output as the result.
    loaded.memory.write_u32_be(entry + 4, 0x3860_ffce).unwrap();
    loaded.memory.write_u32_be(entry + 8, 0x4e80_0020).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[record, keyword, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    for error in [PPC_PARAM_ERR, PPC_ERR_AE_COERCION_FAIL] {
        let partial = scratch + 0x130;
        loaded.cpu.gpr[3..7].copy_from_slice(&[from, data, 4, partial]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CreateDesc,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let partial_handle = loaded.memory.read_u32_be(partial + 4).unwrap();
        loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, partial_handle, 1, 0]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallCoercionHandler,
        );
        for (offset, word) in [
            0x9086_0000,
            0x90a6_0004,
            0x3860_0000 | error as u16 as u32,
            0x4e80_0020,
        ]
        .into_iter()
        .enumerate()
        {
            loaded
                .memory
                .write_u32_be(entry + offset as u32 * 4, word)
                .unwrap();
        }
        loaded.cpu.gpr[3..7].copy_from_slice(&[record, keyword, to, output]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::GetParamDesc,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(error));
        assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
        assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
        assert_eq!(loaded.handles(), original_handles);
        assert_eq!(loaded.ptrs(), original_ptrs);
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.memory.write_u32_be(entry, 0x90a6_0000).unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x9086_0004).unwrap();
    loaded.memory.write_u32_be(entry + 8, 0x3860_0000).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, 1, to, keyword_ptr, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(keyword_ptr), Some(keyword));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);

    // Declined coercions retain the temporary source through built-in fallback.
    loaded.memory.write_u32_be(entry, 0x3860_f95c).unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded
        .memory
        .write_u32_be(keyword_ptr, 0xfeed_beef)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, 1, to, keyword_ptr, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
    assert_eq!(loaded.memory.read_u32_be(keyword_ptr), Some(0xfeed_beef));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);

    // A getter must retain its temporary source across both handler passes.
    let count = scratch + 0x150;
    loaded.memory.write_u32_be(count, 0).unwrap();
    for (offset, word) in [
        0x8103_0000,
        0x9105_0004,
        0x80e5_0000,
        0x38e7_0001,
        0x90e5_0000,
        0x2c07_0002,
        0x4182_000c,
        0x3860_f95c,
        0x4e80_0020,
        0x9086_0000,
        0x3860_0000,
        0x9066_0004,
        0x4e80_0020,
    ]
    .into_iter()
    .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, count, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[record, keyword, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(count), Some(2));
    assert_eq!(loaded.memory.read_u32_be(count + 4), Some(from));
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    loaded.memory.write_u32_be(count, 0).unwrap();
    loaded.memory.write_u32_be(count + 4, 0).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, 1, to, keyword_ptr, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(count), Some(2));
    assert_eq!(loaded.memory.read_u32_be(count + 4), Some(from));
    assert_eq!(loaded.memory.read_u32_be(keyword_ptr), Some(keyword));
    assert_eq!(loaded.memory.read_u32_be(output), Some(to));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
    loaded.memory.write_u32_be(entry, 0x3860_f95c).unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();

    let double = u32::from_be_bytes(*b"doub");
    loaded.cpu.gpr[3..9].copy_from_slice(&[to, double, handler, 0, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.memory.write_u32_be(data, 42).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, keyword, to, data, 4]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[record, keyword, double, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(double));
    let handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let converted = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(converted), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(converted + 4), Some(0));
    assert_eq!(loaded.ptrs(), original_ptrs);
}

#[test]
fn ppc_pointer_getters_copy_installed_handler_results() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let record = scratch + 0x100;
    let data = scratch + 0x120;
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    let keyword = u32::from_be_bytes(*b"data");
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    let mut create = loaded.imports[0].clone();
    create.symbol_index = 1;
    create.symbol_name = "AECreateDesc".into();
    create.dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CreateDesc,
    );
    create.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(create.trap_pc, vec![0; 4]);
    loaded.imports.push(create);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 4, 0), // desired type
        d_form_u(14, 4, 5, 0), // refcon points to output data
        d_form_u(14, 5, 0, 4),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(data, 0x4142_4344).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, data, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[0, 0, 1, record]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    loaded.cpu.gpr[3..8].copy_from_slice(&[record, keyword, from, data, 4]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    let original_handles = loaded.handles();
    let original_ptrs = loaded.ptrs();
    for (nth, maximum) in [(false, 4), (true, 2)] {
        let type_ptr = scratch + 0x140;
        let output = scratch + 0x144;
        let actual = scratch + 0x148;
        let keyword_ptr = scratch + 0x14c;
        if nth {
            loaded.cpu.gpr[3..11].copy_from_slice(&[
                record,
                1,
                to,
                keyword_ptr,
                type_ptr,
                output,
                maximum,
                actual,
            ]);
        } else {
            loaded.cpu.gpr[3..10]
                .copy_from_slice(&[record, keyword, to, type_ptr, output, maximum, actual]);
        }
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::AppleEventCompatibility(if nth {
                PpcAppleEventCompatibilityOperation::GetNthPtr
            } else {
                PpcAppleEventCompatibilityOperation::GetParamPtr
            });
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 2);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(if nth {
                PPC_AE_BUFFER_IS_SMALL
            } else {
                PPC_NO_ERR
            })
        );
        assert_eq!(loaded.memory.read_u32_be(type_ptr), Some(to));
        assert_eq!(loaded.memory.read_u32_be(actual), Some(4));
        assert_eq!(loaded.memory.read_u16_be(output), Some(0x4142));
        if nth {
            assert_eq!(loaded.memory.read_u32_be(keyword_ptr), Some(keyword));
        }
        assert_eq!(loaded.handles(), original_handles);
        assert_eq!(loaded.ptrs(), original_ptrs);
    }

    // A pointer getter retains the source across a decline and disposes the
    // descriptor created by the successful second callback.
    let mut code = vec![
        0x80e5_0004,
        0x38e7_0001,
        0x90e5_0004,
        0x2c07_0002,
        0x4182_000c,
        0x3860_f95c,
        0x4e80_0020,
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 4, 0),
        d_form_u(14, 4, 5, 0),
        d_form_u(14, 5, 0, 4),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    for nth in [false, true] {
        let type_ptr = scratch + 0x140;
        let output = scratch + 0x144;
        let actual = scratch + 0x148;
        let keyword_ptr = scratch + 0x14c;
        loaded.memory.write_u32_be(data + 4, 0).unwrap();
        if nth {
            loaded.cpu.gpr[3..11].copy_from_slice(&[
                record,
                1,
                to,
                keyword_ptr,
                type_ptr,
                output,
                4,
                actual,
            ]);
        } else {
            loaded.cpu.gpr[3..10]
                .copy_from_slice(&[record, keyword, to, type_ptr, output, 4, actual]);
        }
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::AppleEventCompatibility(if nth {
                PpcAppleEventCompatibilityOperation::GetNthPtr
            } else {
                PpcAppleEventCompatibilityOperation::GetParamPtr
            });
        let probe = loaded.run_with_hle_imports(128);
        assert_eq!(probe.handled_import_count, 2);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u32_be(data + 4), Some(2));
        assert_eq!(loaded.memory.read_u32_be(type_ptr), Some(to));
        assert_eq!(loaded.memory.read_u32_be(actual), Some(4));
        assert_eq!(loaded.memory.read_u32_be(output), Some(0x4142_4344));
        if nth {
            assert_eq!(loaded.memory.read_u32_be(keyword_ptr), Some(keyword));
        }
        assert_eq!(loaded.handles(), original_handles);
        assert_eq!(loaded.ptrs(), original_ptrs);
    }

    // A handler may also assemble its result from a process-owned handle.
    loaded.imports[1].dispatcher_target = PpcImportDispatcherTarget::NewHandle { clear: true };
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(36, 4, 1, 56),
        d_form_u(36, 5, 1, 60),
        d_form_u(36, 6, 1, 64),
        d_form_u(14, 3, 0, 4),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(32, 6, 1, 64),
        d_form_u(32, 4, 1, 56),
        d_form_u(36, 4, 6, 0),
        d_form_u(36, 3, 6, 4),
        d_form_u(32, 7, 3, 0),
        d_form_u(32, 8, 1, 60),
        d_form_u(32, 9, 8, 0),
        d_form_u(36, 9, 7, 0),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    let type_ptr = scratch + 0x140;
    let output = scratch + 0x144;
    let actual = scratch + 0x148;
    loaded.cpu.gpr[3..10].copy_from_slice(&[record, keyword, to, type_ptr, output, 4, actual]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::GetParamPtr,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(type_ptr), Some(to));
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x4142_4344));
    assert_eq!(loaded.memory.read_u32_be(actual), Some(4));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);

    // A handler error must preserve caller buffers and release temporary storage.
    loaded.memory.write_u32_be(entry, 0x3860_ffce).unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.memory.write_u32_be(type_ptr, 0x1111_1111).unwrap();
    loaded.memory.write_u32_be(output, 0x2222_2222).unwrap();
    loaded.memory.write_u32_be(actual, 0x3333_3333).unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[record, keyword, to, type_ptr, output, 4, actual]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(type_ptr), Some(0x1111_1111));
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x2222_2222));
    assert_eq!(loaded.memory.read_u32_be(actual), Some(0x3333_3333));
    assert_eq!(loaded.handles(), original_handles);
    assert_eq!(loaded.ptrs(), original_ptrs);
}

#[test]
fn ppc_coercion_accepts_guest_constructed_descriptor_handles() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECoerceDesc")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let from = u32::from_be_bytes(*b"long");
    let to = u32::from_be_bytes(*b"doub");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.cpu.gpr[3] = 4;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    let data = loaded.memory.read_u32_be(handle).unwrap();
    loaded.memory.write_u32_be(data, 42).unwrap();
    loaded.memory.write_u32_be(source, from).unwrap();
    loaded.memory.write_u32_be(source + 4, handle).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let result_handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let result_data = loaded.memory.read_u32_be(result_handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(result_data), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(result_data + 4), Some(0));
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [0x90a6_0000, 0x9086_0004, 0x3860_0000, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(to));
}

#[test]
fn ppc_pointer_coercion_rejects_unflattened_empty_list() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let list = scratch + 0x100;
    let output = scratch + 0x108;
    let from = u32::from_be_bytes(*b"list");
    let to = u32::from_be_bytes(*b"long");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    loaded.memory.write_u32_be(entry, 0x3860_0000).unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[0, 0, 0, list]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[list, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
}

#[test]
fn ppc_coercion_invokes_m68k_descriptor_handler_with_classic_stack_layout() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let entry = PPC_HEAP_BASE + 0x1200;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(source, from).unwrap();
    super::mixed_mode::install_test_m68k_callback(
        &mut loaded,
        descriptor,
        entry,
        0x0000_3fe0,
        0,
        &[0x426f, 0x0014, 0x4e74, 0x0010, 0x4e71], // clr.w 20(sp); rtd #16
    );
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, descriptor, 0x1234_5678, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoerceDesc,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    let frame = pending.initial_sp;
    assert_eq!(loaded.memory.read_u32_be(frame + 4), Some(output));
    assert_eq!(loaded.memory.read_u32_be(frame + 8), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(frame + 12), Some(to));
    assert_eq!(loaded.memory.read_u32_be(frame + 16), Some(source));
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.apple_events.pending_coercion_retries.is_empty());
    for (offset, word) in [0x3f7c, 0xffce, 0x0014, 0x4e74, 0x0010]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u16_be(entry + offset as u32 * 2, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(loaded.apple_events.pending_coercion_retries.is_empty());
}

#[test]
fn ppc_m68k_pointer_coercion_restores_handle_and_resumes_fallback() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let entry = PPC_HEAP_BASE + 0x1200;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    let from = u32::from_be_bytes(*b"long");
    let to = u32::from_be_bytes(*b"doub");
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(scratch + 0x120, 42).unwrap();
    super::mixed_mode::install_test_m68k_callback(
        &mut loaded,
        descriptor,
        entry,
        0x0003_ffe0,
        0,
        &[0x3f7c, 0xf95c, 0x001c, 0x4e74, 0x0018], // move.w #-1700,28(sp); rtd #24
    );
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, descriptor, 0x1234_5678, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, scratch + 0x120, 4, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    let handle = loaded.memory.read_u32_be(source + 4).unwrap();
    let data = loaded.memory.read_u32_be(handle).unwrap();
    let original_states = loaded.handle_states();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoerceDesc,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    let frame = pending.initial_sp;
    assert_eq!(loaded.memory.read_u32_be(frame + 4), Some(output));
    assert_eq!(loaded.memory.read_u32_be(frame + 8), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(frame + 12), Some(to));
    assert_eq!(loaded.memory.read_u32_be(frame + 16), Some(4));
    assert_eq!(loaded.memory.read_u32_be(frame + 20), Some(data));
    assert_eq!(loaded.memory.read_u32_be(frame + 24), Some(from));
    assert!(loaded
        .handle_states()
        .iter()
        .any(|state| state.handle == handle && state.locked));
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.apple_events.pending_coercion_retries.is_empty());
    assert!(loaded.apple_events.pending_coercion_handles.is_empty());
    assert_eq!(
        loaded
            .handle_states()
            .iter()
            .find(|state| state.handle == handle),
        original_states.iter().find(|state| state.handle == handle)
    );
    let result_handle = loaded.memory.read_u32_be(output + 4).unwrap();
    let result_data = loaded.memory.read_u32_be(result_handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(result_data), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(result_data + 4), Some(0));
}

#[test]
fn nested_ppc_coercion_resumes_after_m68k_handler_declines() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let outer_handler = scratch + 0x40;
    let outer_entry = scratch + 0x200;
    let inner_handler = PPC_HEAP_BASE + 0x1000;
    let inner_entry = PPC_HEAP_BASE + 0x1200;
    let source = scratch + 0x100;
    let output = scratch + 0x108;
    let inner_data = scratch + 0x140;
    let inner_output = scratch + 0x148;
    let outer_from = u32::from_be_bytes(*b"TEXT");
    let outer_to = u32::from_be_bytes(*b"type");
    let inner_from = u32::from_be_bytes(*b"long");
    let inner_to = u32::from_be_bytes(*b"doub");
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    loaded.memory.write_u32_be(source, outer_from).unwrap();
    loaded.memory.write_u32_be(inner_data, 42).unwrap();
    loaded
        .memory
        .write_u32_be(outer_handler, outer_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(outer_handler + 4, loaded.rtoc)
        .unwrap();
    super::mixed_mode::install_test_m68k_callback(
        &mut loaded,
        inner_handler,
        inner_entry,
        0x0003_ffe0,
        0,
        &[0x3f7c, 0xf95c, 0x001c, 0x4e74, 0x0018],
    );
    let mut nested = loaded.imports[0].clone();
    nested.symbol_index = 1;
    nested.symbol_name = "AECoercePtr".into();
    nested.dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoercePtr,
    );
    nested.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(nested.trap_pc, vec![0; 4]);
    loaded.imports.push(nested);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(36, 6, 1, 56),
        d_form_u(14, 7, 5, 8),
        d_form_u(14, 4, 5, 0),
        d_form_u(15, 3, 0, 0x6c6f),
        d_form_u(24, 3, 3, 0x6e67),
        d_form_u(14, 5, 0, 4),
        d_form_u(15, 6, 0, 0x646f),
        d_form_u(24, 6, 6, 0x7562),
    ];
    let call_pc = outer_entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(32, 6, 1, 56),
        d_form_u(36, 3, 6, 0), // record nested result
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(outer_entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[inner_from, inner_to, inner_handler, 0, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..9].copy_from_slice(&[outer_from, outer_to, outer_handler, inner_data, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, outer_to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoerceDesc,
    );
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.guest_calls().activate_m68k().is_some());
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0));
    assert_eq!(loaded.memory.read_u32_be(inner_output), Some(inner_to));
    let result_handle = loaded.memory.read_u32_be(inner_output + 4).unwrap();
    let result_data = loaded.memory.read_u32_be(result_handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(result_data), Some(0x4045_0000));
    assert_eq!(loaded.memory.read_u32_be(result_data + 4), Some(0));
    assert!(loaded.apple_events.pending_coercion_retries.is_empty());
}

#[test]
fn ppc_apple_event_numeric_text_coercions() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECoercePtr")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let data = scratch + 0x100;
    let output = scratch + 0x180;
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    let cases = [
        (
            *b"TEXT",
            b"-0".to_vec(),
            *b"sing",
            (-0.0f32).to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"-00".to_vec(),
            *b"doub",
            (-0.0f64).to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"-0".to_vec(),
            *b"long",
            0i32.to_be_bytes().to_vec(),
        ),
        // Exact decimal values halfway between successive 64-bit
        // significands: ties round to the even significand.
        (
            *b"TEXT",
            b"1.0000000000000000000542101086242752217003726400434970855712890625".to_vec(),
            *b"exte",
            [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"TEXT",
            b"1.0000000000000000001626303258728256651011179201304912567138671875".to_vec(),
            *b"exte",
            [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 2].to_vec(),
        ),
        (
            *b"TEXT",
            b"-1.0000000000000000000542101086242752217003726400434970855712890625".to_vec(),
            *b"exte",
            [0xbf, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"TEXT",
            b"-1.0000000000000000001626303258728256651011179201304912567138671875".to_vec(),
            *b"exte",
            [0xbf, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 2].to_vec(),
        ),
        (
            *b"TEXT",
            b" +iNf ".to_vec(),
            *b"doub",
            f64::INFINITY.to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"-INF".to_vec(),
            *b"sing",
            f32::NEG_INFINITY.to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"-inf".to_vec(),
            *b"exte",
            [0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"doub",
            f64::INFINITY.to_be_bytes().to_vec(),
            *b"TEXT",
            b"INF".to_vec(),
        ),
        (
            *b"sing",
            f32::NEG_INFINITY.to_be_bytes().to_vec(),
            *b"TEXT",
            b"-INF".to_vec(),
        ),
        (
            *b"exte",
            [0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"TEXT",
            b"-INF".to_vec(),
        ),
        (
            *b"TEXT",
            b" -42 ".to_vec(),
            *b"long",
            (-42i32).to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"1.25e2".to_vec(),
            *b"sing",
            125f32.to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"2.5".to_vec(),
            *b"shor",
            2i16.to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"9223372036854775807".to_vec(),
            *b"comp",
            i64::MAX.to_be_bytes().to_vec(),
        ),
        (
            *b"comp",
            i64::MIN.to_be_bytes().to_vec(),
            *b"TEXT",
            b"-9223372036854775808".to_vec(),
        ),
        (
            *b"doub",
            (-1.5f64).to_be_bytes().to_vec(),
            *b"TEXT",
            b"-1.5".to_vec(),
        ),
    ];
    for (from, bytes, to, expected) in cases {
        loaded.memory.write_bytes(data, &bytes).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(from),
            data,
            bytes.len() as u32,
            u32::from_be_bytes(to),
            output,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "from {:?} to {:?}", from, to);
        assert_eq!(
            loaded.memory.read_u32_be(output),
            Some(u32::from_be_bytes(to))
        );
        let handle = loaded.memory.read_u32_be(output + 4).unwrap();
        let pointer = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pointer, expected.len() as u32),
            Some(expected)
        );
        loaded.cpu.gpr[3] = output;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::DisposeDesc,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    for bytes in [
        b"".as_slice(),
        b"12x",
        b"NaN",
        b"1e999",
        b"inf",
        b"-INF",
        &[0x80],
    ] {
        loaded.memory.write_bytes(data, bytes).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEXT"),
            data,
            bytes.len() as u32,
            u32::from_be_bytes(*b"long"),
            output,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
    }
}

#[test]
fn ppc_apple_event_extended_numeric_coercions() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECoercePtr")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let data = scratch + 0x100;
    let output = scratch + 0x180;
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    let extended_max = [0x40, 0x3d, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe];
    let extended_min = [0xc0, 0x3e, 0x80, 0, 0, 0, 0, 0, 0, 0];
    let extended_smallest_double = [0x3b, 0xcd, 0x80, 0, 0, 0, 0, 0, 0, 0];
    let halfway_one = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0x04, 0];
    let above_halfway_one = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0x04, 1];
    let two_and_half = [0x40, 0x00, 0xa0, 0, 0, 0, 0, 0, 0, 0];
    let halfway_single = [0x3f, 0xff, 0x80, 0, 0, 0x80, 0, 0, 0, 0];
    let above_halfway_single = [0x3f, 0xff, 0x80, 0, 0, 0x80, 0, 0, 0, 1];
    let smallest_single = [0x3f, 0x6a, 0x80, 0, 0, 0, 0, 0, 0, 0];
    let positive_infinity = [0x7f, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
    let negative_infinity = [0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
    let cases = [
        (
            *b"doub",
            0x7ff8_0000_0000_002au64.to_be_bytes().to_vec(),
            *b"exte",
            [0x7f, 0xff, 0x40, 0, 0, 0, 0, 1, 0x50, 0].to_vec(),
        ),
        (
            *b"sing",
            0xffc0_005au32.to_be_bytes().to_vec(),
            *b"exte",
            [0xff, 0xff, 0x40, 0, 0x5a, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"sing",
            0x7f80_005au32.to_be_bytes().to_vec(),
            *b"exte",
            [0x7f, 0xff, 0x40, 0, 0x5a, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"exte",
            [0x7f, 0xff, 0x40, 0x21, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"doub",
            0x7ff8_0420_0000_0000u64.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            [0x7f, 0xff, 0, 0x21, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"sing",
            0x7fc0_2100u32.to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"1e-4951".to_vec(),
            *b"exte",
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 1].to_vec(),
        ),
        (
            *b"TEXT",
            b"1e-4932".to_vec(),
            *b"exte",
            [0, 0, 0x4c, 0x24, 0x8f, 0x91, 0xe5, 0x26, 0xaf, 0xe0].to_vec(),
        ),
        (
            *b"exte",
            [0x40, 0x00, 0x40, 0, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"doub",
            1f64.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            [0x7f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"doub",
            f64::INFINITY.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            [0xff, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"TEXT",
            b"-INF".to_vec(),
        ),
        (
            *b"doub",
            f64::INFINITY.to_be_bytes().to_vec(),
            *b"exte",
            positive_infinity.to_vec(),
        ),
        (
            *b"exte",
            negative_infinity.to_vec(),
            *b"doub",
            f64::NEG_INFINITY.to_be_bytes().to_vec(),
        ),
        (
            *b"sing",
            f32::NEG_INFINITY.to_be_bytes().to_vec(),
            *b"exte",
            negative_infinity.to_vec(),
        ),
        (
            *b"exte",
            positive_infinity.to_vec(),
            *b"sing",
            f32::INFINITY.to_be_bytes().to_vec(),
        ),
        (
            *b"comp",
            i64::MAX.to_be_bytes().to_vec(),
            *b"exte",
            extended_max.to_vec(),
        ),
        (
            *b"exte",
            extended_max.to_vec(),
            *b"comp",
            i64::MAX.to_be_bytes().to_vec(),
        ),
        (
            *b"comp",
            i64::MIN.to_be_bytes().to_vec(),
            *b"exte",
            extended_min.to_vec(),
        ),
        (
            *b"exte",
            extended_min.to_vec(),
            *b"comp",
            i64::MIN.to_be_bytes().to_vec(),
        ),
        (
            *b"doub",
            1u64.to_be_bytes().to_vec(),
            *b"exte",
            extended_smallest_double.to_vec(),
        ),
        (
            *b"exte",
            extended_smallest_double.to_vec(),
            *b"doub",
            1u64.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            halfway_one.to_vec(),
            *b"doub",
            1f64.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            above_halfway_one.to_vec(),
            *b"doub",
            (1f64.to_bits() + 1).to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            two_and_half.to_vec(),
            *b"long",
            2i32.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            halfway_single.to_vec(),
            *b"sing",
            1f32.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            above_halfway_single.to_vec(),
            *b"sing",
            (1f32.to_bits() + 1).to_be_bytes().to_vec(),
        ),
        (
            *b"sing",
            1u32.to_be_bytes().to_vec(),
            *b"exte",
            smallest_single.to_vec(),
        ),
        (
            *b"exte",
            smallest_single.to_vec(),
            *b"sing",
            1u32.to_be_bytes().to_vec(),
        ),
        (
            *b"exte",
            [0xc0, 0, 0xa0, 0, 0, 0, 0, 0, 0, 0].to_vec(),
            *b"long",
            (-2i32).to_be_bytes().to_vec(),
        ),
        (
            *b"TEXT",
            b"1e-400".to_vec(),
            *b"exte",
            [0x3a, 0xce, 0x95, 0xfe, 0x7e, 0x07, 0xc9, 0x1e, 0xfa, 0xfa].to_vec(),
        ),
        (
            *b"TEXT",
            b"4e-4951".to_vec(),
            *b"exte",
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 2].to_vec(),
        ),
        (
            *b"TEXT",
            b"1e4932".to_vec(),
            *b"exte",
            [0x7f, 0xfe, 0xd7, 0x2c, 0xb2, 0xa9, 0x5c, 0x7e, 0xf6, 0xcd].to_vec(),
        ),
        (*b"TEXT", b"1e-5000".to_vec(), *b"exte", [0u8; 10].to_vec()),
        (
            *b"TEXT",
            b"-0.0".to_vec(),
            *b"exte",
            [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"TEXT",
            b"1.0000000000000000000542101086242752217003726400434970855712890625".to_vec(),
            *b"exte",
            [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
        (
            *b"TEXT",
            b"1.000000000000000000108420217248550443400745280086994171142578125".to_vec(),
            *b"exte",
            [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1].to_vec(),
        ),
        (
            *b"TEXT",
            b"42".to_vec(),
            *b"exte",
            [0x40, 0x04, 0xa8, 0, 0, 0, 0, 0, 0, 0].to_vec(),
        ),
    ];
    for (from, bytes, to, expected) in cases {
        loaded.memory.write_bytes(data, &bytes).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(from),
            data,
            bytes.len() as u32,
            u32::from_be_bytes(to),
            output,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "from {:?} to {:?}", from, to);
        let handle = loaded.memory.read_u32_be(output + 4).unwrap();
        let pointer = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            loaded
                .handles()
                .iter()
                .find(|record| record.handle == handle)
                .unwrap()
                .size,
            expected.len() as u32
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pointer, expected.len() as u32),
            Some(expected)
        );
        loaded.cpu.gpr[3] = output;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::DisposeDesc,
            ),
        );
    }
    for invalid in [
        [0x40, 0x3e, 0x80, 0, 0, 0, 0, 0, 0, 0], // 2^63
        positive_infinity,
        negative_infinity,
        [0x7f, 0xff, 0x40, 0x21, 0, 0, 0, 0, 0, 0], // quiet NaN
        [0x7f, 0xff, 0, 0x21, 0, 0, 0, 0, 0, 0],    // signaling NaN
    ] {
        loaded.memory.write_bytes(data, &invalid).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"exte"),
            data,
            10,
            u32::from_be_bytes(*b"comp"),
            output,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
    }
    loaded.memory.write_bytes(data, b"1e4933").unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEXT"),
        data,
        6,
        u32::from_be_bytes(*b"exte"),
        output,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
}

#[test]
fn ppc_extended_text_round_trips_full_precision_and_range() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AECoercePtr")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let data = scratch + 0x100;
    let text_desc = scratch + 0x180;
    let extended_desc = scratch + 0x188;
    let extended = u32::from_be_bytes(*b"exte");
    let text = u32::from_be_bytes(*b"TEXT");
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    let values = [
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1],
        [0x40, 0x3d, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe],
        [0xc0, 0x3e, 0x80, 0, 0, 0, 0, 0, 0, 0],
        [0x3a, 0xce, 0x95, 0xfe, 0x7e, 0x07, 0xc9, 0x1e, 0xfa, 0xfa],
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        [0, 1, 0x80, 0, 0, 0, 0, 0, 0, 0],
        [0x7f, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ];
    let original_handles = loaded.handles();
    for value in values {
        loaded.memory.write_bytes(data, &value).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[extended, data, 10, text, text_desc]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "extended bytes {value:02x?}");
        let text_handle = loaded.memory.read_u32_be(text_desc + 4).unwrap();
        let text_data = loaded.memory.read_u32_be(text_handle).unwrap();
        let text_size = loaded
            .handles()
            .iter()
            .find(|record| record.handle == text_handle)
            .unwrap()
            .size;
        assert!(text_size <= 32);
        if value == [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1] {
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, text_data, text_size),
                Some(b"1.00000000000000000011".to_vec())
            );
        }
        if value == [0x40, 0x3d, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe] {
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, text_data, text_size),
                Some(b"9223372036854775807".to_vec())
            );
        }
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            text,
            text_data,
            text_size,
            extended,
            extended_desc,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CoercePtr,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "extended bytes {value:02x?}");
        let result_handle = loaded.memory.read_u32_be(extended_desc + 4).unwrap();
        let result_data = loaded.memory.read_u32_be(result_handle).unwrap();
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, result_data, 10),
            Some(value.to_vec())
        );
        for descriptor in [text_desc, extended_desc] {
            loaded.cpu.gpr[3] = descriptor;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::DisposeDesc,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
        assert_eq!(loaded.handles(), original_handles);
    }
}

#[test]
fn ppc_coerce_ptr_calls_registered_pointer_handler() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"AEInstallCoercionHandler")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler = scratch + 0x40;
    let entry = scratch + 0x80;
    let output = scratch + 0x100;
    loaded.memory.add_region(scratch, vec![0; 0x180]);
    loaded.memory.write_u32_be(handler, entry).unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, loaded.rtoc)
        .unwrap();
    for (offset, word) in [0x90e8_0000, 0x90c8_0004, 0x3860_0000, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    loaded.cpu.gpr[3..9].copy_from_slice(&[from, to, handler, 0x1234_5678, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..8].copy_from_slice(&[from, scratch, 4, to, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoercePtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(to));

    // Inspect the handle state from inside the pointer handler.
    let mut binding = loaded.imports[0].clone();
    binding.symbol_index = 1;
    binding.symbol_name = "HGetState".into();
    binding.dispatcher_target = PpcImportDispatcherTarget::HGetState;
    binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
    loaded.imports.push(binding);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6, // mflr r0
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(36, 8, 1, 56),
        0x8924_0000, // lbz r9, 0(r4)
        d_form_u(36, 5, 8, 4),
        d_form_u(36, 9, 8, 0),
        d_form_u(32, 3, 8, 0x24),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(32, 8, 1, 56),
        d_form_u(36, 3, 8, 0x30),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6, // mtlr r0
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ]);
    for (offset, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + offset as u32 * 4, word)
            .unwrap();
    }
    loaded
        .memory
        .write_u32_be(scratch + 0x120, 0x4142_4344)
        .unwrap();
    let source = scratch + 0x108;
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, scratch + 0x120, 4, source]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let handle = loaded.memory.read_u32_be(source + 4).unwrap();
    let data = loaded.memory.read_u32_be(handle).unwrap();
    loaded.memory.write_u8(data, b'Z').unwrap();
    loaded.memory.write_u32_be(scratch + 0x124, handle).unwrap();
    let original_states = loaded.handle_states();
    loaded.cpu.gpr[3..6].copy_from_slice(&[source, to, output]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::CoerceDesc,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_ne!(loaded.memory.read_u32_be(output + 0x30).unwrap() & 0x80, 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(u32::from(b'Z')));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(4));
    assert_eq!(loaded.handle_states(), original_states);
}

#[test]
fn coercion_dispatch_prefers_application_table_before_system() {
    use super::super::dispatch_apple_events::PpcCoercionHandler;
    let from = u32::from_be_bytes(*b"TEXT");
    let to = u32::from_be_bytes(*b"long");
    let handler = |pointer| PpcCoercionHandler {
        pointer,
        refcon: pointer + 1,
        from_type_is_desc: true,
    };
    let mut state = PpcAppleEventState::default();
    state
        .coercion_handlers
        .insert((true, from, to), handler(10));
    state
        .coercion_handlers
        .insert((false, PPC_TYPE_WILDCARD, PPC_TYPE_WILDCARD), handler(20));
    assert_eq!(state.coercion_handler_for(from, to), Some(handler(20)));
    assert_eq!(state.coercion_handlers.get(&(false, from, to)), None);
    state
        .coercion_handlers
        .insert((false, from, to), handler(30));
    assert_eq!(state.coercion_handler_for(from, to), Some(handler(30)));
    state.coercion_handlers.remove(&(false, from, to));
    state
        .coercion_handlers
        .remove(&(false, PPC_TYPE_WILDCARD, PPC_TYPE_WILDCARD));
    assert_eq!(state.coercion_handler_for(from, to), Some(handler(10)));
    state
        .coercion_handlers
        .insert((false, from, PPC_TYPE_WILDCARD), handler(40));
    state
        .coercion_handlers
        .insert((false, PPC_TYPE_WILDCARD, to), handler(50));
    assert_eq!(state.coercion_handler_for(from, to), Some(handler(50)));
}

#[test]
fn ae_interact_with_user_allows_foreground_application() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEInteractWithUser"),
        PpcImportDispatcherTarget::AEInteractWithUser,
    );
    let pef = synthetic_pef_with_import(b"AEInteractWithUser");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = u32::MAX; // Default timeout.
    loaded.cpu.gpr[4] = 0; // No Notification Manager record.
    loaded.cpu.gpr[5] = 0; // No idle callback.

    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEInteractWithUser);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

#[test]
fn ae_manager_info_returns_version_and_recorder_count() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEManagerInfo"),
        PpcImportDispatcherTarget::AEManagerInfo,
    );
    let pef = synthetic_pef_with_import(b"AEManagerInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xff; 4]);
    loaded.cpu.gpr[4] = output;

    for (keyword, expected) in [(b"vers", 0x0101_0000), (b"recr", 0)] {
        loaded.cpu.gpr[3] = u32::from_be_bytes(*keyword);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEManagerInfo);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(output), Some(expected));
    }

    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"vers");
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEManagerInfo);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn ppc_launch_size_resource_enables_open_application_apple_event() {
    let pef = synthetic_pef_with_import(b"WaitNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut size = Vec::new();
    size.extend_from_slice(&ApplicationSizeResource::HIGH_LEVEL_EVENT_AWARE.to_be_bytes());
    size.extend_from_slice(&(2 * 1024 * 1024u32).to_be_bytes());
    size.extend_from_slice(&(1024 * 1024u32).to_be_bytes());
    loaded.seed_vfs_files_and_resources(
        Vec::new(),
        Vec::new(),
        vec![PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"SIZE"),
            res_id: -1,
            name: Vec::new(),
            data: size.into(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        }],
    );
    loaded.set_launched_app_path("Test App");

    assert!(loaded
        .apple_events
        .apple_event_launch_state
        .is_high_level_event_aware());
}

#[test]
fn ppc_apple_event_manager_delivers_oapp_and_calls_registered_native_handler() {
    let pef = synthetic_pef_with_import(b"AEInstallEventHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler_tvector = scratch;
    let handler_entry = scratch + 0x100;
    let event_record = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    loaded
        .memory
        .write_u32_be(handler_tvector, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_tvector + 4, 0x0200_1234)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry, 0x3860_1234)
        .unwrap(); // li r3,$1234
    loaded
        .memory
        .write_u32_be(handler_entry + 4, 0x4e80_0020)
        .unwrap(); // blr
    loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
    loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
    loaded.cpu.gpr[5] = handler_tvector;
    loaded.cpu.gpr[6] = 0xfeed_beef;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.apple_events.handlers.len(), 1);

    loaded
        .apple_events
        .apple_event_launch_state
        .set_high_level_event_aware(true);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent);
    loaded.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    loaded.cpu.gpr[4] = event_record;
    loaded.cpu.gpr[5] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u16_be(event_record),
        Some(PPC_HIGH_LEVEL_EVENT)
    );
    assert_eq!(
        loaded.memory.read_u32_be(event_record + 2),
        Some(PPC_CORE_EVENT_CLASS)
    );
    assert_eq!(
        u32::from(loaded.memory.read_u16_be(event_record + 10).unwrap()) << 16
            | u32::from(loaded.memory.read_u16_be(event_record + 12).unwrap()),
        PPC_OPEN_APPLICATION_EVENT
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
    loaded.cpu.gpr[3] = event_record;
    let handles_before_dispatch = loaded.handles();
    let ptrs_before_dispatch = loaded.ptrs();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
    assert_eq!(loaded.cpu.gpr[2], loaded.rtoc);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.handles(), handles_before_dispatch);
    assert_eq!(loaded.ptrs(), ptrs_before_dispatch);
}

#[test]
fn ppc_launch_handler_receives_finder_address() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2600;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0xfeed_beef,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        current_launch_event: Some(PpcQueuedEvent {
            delivery_id: Some(1),
            what: PPC_HIGH_LEVEL_EVENT,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: (PPC_OPEN_APPLICATION_EVENT >> 16) as u16 as i16,
            where_h: PPC_OPEN_APPLICATION_EVENT as u16 as i16,
            modifiers: 0,
        }),
        ..PpcAppleEventState::default()
    };
    apple_events
        .apple_event_launch_state
        .note_open_application_event_delivered();
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(GuestCallEffect::from_ppc_import_action(action).is_some());
    let descriptor = ppc_ae_descriptor(
        &mut native.memory,
        &apple_events.descriptors,
        native.cpu.gpr[3],
    )
    .unwrap();
    let sender = &descriptor.attributes[&u32::from_be_bytes(*b"addr")];
    assert_eq!(sender.desc_type, u32::from_be_bytes(*b"sign"));
    assert_eq!(sender.data, b"MACS");
    assert!(apple_events.current_launch_event.is_none());
}

#[test]
fn ppc_launch_dispatch_failure_retains_sender_for_retry() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2600;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0xfeed_beef,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        current_launch_event: Some(PpcQueuedEvent {
            delivery_id: Some(1),
            what: PPC_HIGH_LEVEL_EVENT,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: (PPC_OPEN_APPLICATION_EVENT >> 16) as u16 as i16,
            where_h: PPC_OPEN_APPLICATION_EVENT as u16 as i16,
            modifiers: 0,
        }),
        ..PpcAppleEventState::default()
    };
    apple_events
        .apple_event_launch_state
        .note_open_application_event_delivered();
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;
    apple_events.fail_self_queue_reservation = 15;
    let failed = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(
        matches!(failed, PpcImportAction::Return(result) if result == ppc_i16_result(PPC_MEM_FULL_ERR))
    );
    assert!(apple_events.current_launch_event.is_some());
    apple_events.fail_self_queue_reservation = 0;
    native.cpu.gpr[3] = event_record;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(GuestCallEffect::from_ppc_import_action(action).is_some());
    let descriptor = ppc_ae_descriptor(
        &mut native.memory,
        &apple_events.descriptors,
        native.cpu.gpr[3],
    )
    .unwrap();
    let sender = &descriptor.attributes[&u32::from_be_bytes(*b"addr")];
    assert_eq!(sender.desc_type, u32::from_be_bytes(*b"sign"));
    assert_eq!(sender.data, b"MACS");
    assert!(apple_events.current_launch_event.is_none());
}

#[test]
fn suspended_direct_reply_reports_guest_heap_exhaustion() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let event_record = PPC_DATA_BASE + 0x2600;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0xfeed_beef,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        ..PpcAppleEventState::default()
    };
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(GuestCallEffect::from_ppc_import_action(action).is_some());
    assert_eq!(apple_events.pending_dispatches.len(), 1);
    let descriptors = native.cpu.gpr[3];
    assert_eq!(native.cpu.gpr[4], descriptors + 8);
    assert_eq!(native.cpu.gpr[5], 0xfeed_beef);
    let event_handle = native.memory.read_u32_be(descriptors + 4).unwrap();
    let reply_handle = native.memory.read_u32_be(descriptors + 12).unwrap();
    let (event_allocation, reply_allocation) = {
        let manager = context.memory_manager_mut();
        (
            manager.native_allocation(event_handle).unwrap(),
            manager.native_allocation(reply_handle).unwrap(),
        )
    };
    assert_eq!(event_allocation.size, 8);
    assert_eq!(reply_allocation.size, 0);
    assert_eq!(detached.native_allocation(event_handle), None);
    assert_eq!(detached.native_allocation(reply_handle), None);

    let output = event_record + 0x20;
    native.memory.add_region(output, vec![0; 8]);
    let dispatch = apple_events.pending_dispatches.last_mut().unwrap();
    dispatch.suspended = true;
    dispatch.send_reply = Some(output);
    dispatch.return_handler_result = false;
    native.set_heap_cursor(heap_limit);
    heap_cursor = heap_limit;
    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
    }
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(native.memory.read_u32_be(output + 4), Some(0));
    assert!(apple_events.pending_dispatches.is_empty());
    assert_eq!(apple_events.suspended_dispatches.len(), 1);
    assert!(apple_events.descriptors.pending_reply_handles.is_empty());
}

#[test]
fn apple_event_handler_bundle_is_process_owned_cross_isa_and_depth_disposed() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let event_record = PPC_DATA_BASE + 0x2600;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0xfeed_beef,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        ..PpcAppleEventState::default()
    };
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    let handle_states = native.handle_states();
    native.cpu.gpr[3] = event_record;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(GuestCallEffect::from_ppc_import_action(action).is_some());
    assert_eq!(apple_events.pending_dispatches.len(), 1);
    let descriptors = native.cpu.gpr[3];
    assert_eq!(native.cpu.gpr[4], descriptors + 8);
    assert_eq!(native.cpu.gpr[5], 0xfeed_beef);
    let event_handle = native.memory.read_u32_be(descriptors + 4).unwrap();
    let reply_handle = native.memory.read_u32_be(descriptors + 12).unwrap();
    let (event_allocation, reply_allocation) = {
        let manager = context.memory_manager_mut();
        (
            manager.native_allocation(event_handle).unwrap(),
            manager.native_allocation(reply_handle).unwrap(),
        )
    };
    assert_eq!(event_allocation.size, 8);
    assert_eq!(reply_allocation.size, 0);
    assert_eq!(detached.native_allocation(event_handle), None);
    assert_eq!(detached.native_allocation(reply_handle), None);

    let shared = native.memory.shared_view();
    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);
    assert_eq!(
        classic_bus.read_bytes(event_allocation.ptr, 8),
        [
            PPC_CORE_EVENT_CLASS.to_be_bytes(),
            PPC_OPEN_APPLICATION_EVENT.to_be_bytes(),
        ]
        .concat()
    );
    classic_bus.write_byte(event_allocation.ptr, b'P');
    assert_eq!(native.memory.read_u8(event_allocation.ptr), Some(b'P'));
    native
        .memory
        .write_u8(event_allocation.ptr + 7, b'!')
        .unwrap();
    assert_eq!(classic_bus.read_byte(event_allocation.ptr + 7), b'!');

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut apple_events,
            1,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
        assert!(manager.native_allocation(event_handle).is_some());
        assert!(manager.native_allocation(reply_handle).is_some());
    }
    assert_eq!(apple_events.pending_dispatches.len(), 1);

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
        assert_eq!(manager.native_allocation(event_handle), None);
        assert_eq!(manager.native_allocation(reply_handle), None);
        assert!(!manager
            .native_allocator_snapshot()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == descriptors));
    }
    assert!(apple_events.pending_dispatches.is_empty());
    assert_eq!(native.memory.read_u32_be(event_handle), Some(0));
    assert_eq!(native.memory.read_u32_be(reply_handle), Some(0));
    assert!(handles
        .iter()
        .all(|record| { record.handle != event_handle && record.handle != reply_handle }));
    assert!(handle_states
        .iter()
        .all(|record| { record.handle != event_handle && record.handle != reply_handle }));
}

#[test]
fn classic_apple_event_handler_bundle_is_process_owned_cross_isa_and_disposed() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let ram_end = classic_bus.ram_size();
    let low_memory_end = 0x0010_0000.min(ram_end);
    let low_memory = classic_bus
        .shared_ram_region(0, low_memory_end)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    for (base, end) in native.memory.mapping_holes(low_memory_end, ram_end) {
        let memory = classic_bus
            .shared_ram_region(base, end - base)
            .expect("classic adapter owns the native mapping hole");
        context.attach_memory(base, memory, &mut native.memory);
    }

    let sp = TEST_SP;
    let event_class = u32::from_be_bytes(*b"aevt");
    let event_id = u32::from_be_bytes(*b"oapp");
    let handler = 0x0040_8000;
    classic_cpu.write_reg(Register::D0, 0x091F);
    classic_bus.write_word(sp, 0);
    classic_bus.write_long(sp + 2, 0xfeed_beef);
    classic_bus.write_long(sp + 6, handler);
    classic_bus.write_long(sp + 10, event_id);
    classic_bus.write_long(sp + 14, event_class);
    classic_bus.write_word(sp + 18, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(
        native
            .apple_events
            .handlers
            .get(false, event_class, event_id)
            .map(|registered| (
                registered.procedure.isa,
                registered.procedure.original_pointer,
                registered.refcon,
            )),
        Some((GuestIsa::M68k, handler, 0xfeed_beef))
    );

    let native_event_class = u32::from_be_bytes(*b"misc");
    let native_event_id = u32::from_be_bytes(*b"slct");
    native.cpu.gpr[3] = native_event_class;
    native.cpu.gpr[4] = native_event_id;
    native.cpu.gpr[5] = PPC_CODE_BASE;
    native.cpu.gpr[6] = 0x1234_5678;
    native.cpu.gpr[7] = 1;
    assert_eq!(
        ppc_install_apple_event_handler(&native.cpu, &mut native.memory, &mut native.apple_events,),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    assert_eq!(
        classic
            .ae_handlers
            .get(true, native_event_class, native_event_id)
            .map(|registered| (registered.procedure.isa, registered.refcon)),
        Some((GuestIsa::PowerPc, 0x1234_5678))
    );

    classic_bus.write_word(0x0032_0000, PPC_HIGH_LEVEL_EVENT);
    classic_bus.write_long(0x0032_0002, event_class);
    classic_bus.write_long(0x0032_000a, event_id);
    classic_cpu.write_reg(Register::A7, sp);
    classic_cpu.write_reg(Register::PC, 0x00f0_1234);
    classic_cpu.write_reg(Register::D0, 0x021B);
    classic_bus.write_long(sp, 0x0032_0000);
    classic_bus.write_word(sp + 4, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());

    let state = classic
        .ae_call_state
        .clone()
        .expect("classic AppleEvent handler is in flight");
    let (event_desc, reply_desc) = state
        .owned_descriptors
        .expect("classic AppleEvent callback descriptors are process-owned");
    let event_handle = classic_bus.read_long(event_desc + 4);
    let event_data = classic_bus.read_long(event_handle);
    assert_eq!(native.memory.read_u32_be(event_desc), Some(event_class));
    assert_eq!(
        native.memory.read_u32_be(event_desc + 4),
        Some(event_handle)
    );
    assert_eq!(native.memory.read_u32_be(event_handle), Some(event_data));
    classic_bus.write_byte(event_data, b'C');
    assert_eq!(native.memory.read_u8(event_data), Some(b'C'));
    native.memory.write_u8(event_data + 7, b'!').unwrap();
    assert_eq!(classic_bus.read_byte(event_data + 7), b'!');

    classic_bus.write_word(state.expected_sp_after_rtd, 0);
    classic_cpu.write_reg(Register::A7, state.expected_sp_after_rtd);
    classic_cpu.write_reg(Register::D0, 0xFEFE);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.get_alloc_size(event_desc), None);
    assert_eq!(classic_bus.get_alloc_size(reply_desc), None);
    assert_eq!(classic_bus.get_alloc_size(event_handle), None);
    assert_eq!(classic_bus.get_alloc_size(event_data), None);
    assert!(classic.ae_call_state.is_none());

    native.cpu.gpr[3] = event_class;
    native.cpu.gpr[4] = event_id;
    native.cpu.gpr[5] = PPC_CODE_BASE;
    native.cpu.gpr[6] = 0xcafe_babe;
    native.cpu.gpr[7] = 0;
    assert_eq!(
        ppc_install_apple_event_handler(&native.cpu, &mut native.memory, &mut native.apple_events,),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    classic_cpu.write_reg(Register::A7, sp);
    classic_cpu.write_reg(Register::PC, 0x00f0_5678);
    classic_cpu.write_reg(Register::D0, 0x021B);
    classic_bus.write_long(sp, 0x0032_0000);
    classic_bus.write_word(sp + 4, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    let pending = classic.guest_calls.pending_powerpc_from_m68k().unwrap();
    assert_eq!(pending.target.isa, GuestIsa::PowerPc);
    assert_eq!(pending.target.entry, PPC_CODE_BASE);
    assert_eq!(pending.arguments.as_slice()[2], 0xcafe_babe);
}
#[test]
fn apple_event_handler_bundle_allocation_failure_is_atomic() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    native.set_heap_cursor(native.heap_limit().saturating_sub(8));
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2700;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        ..PpcAppleEventState::default()
    };
    let before_allocator = context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap();
    let before_handles = context
        .memory_manager_mut()
        .native_handle_records()
        .to_vec();
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;

    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert_eq!(
        action,
        PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR))
    );
    assert!(apple_events.pending_dispatches.is_empty());
    let manager = context.memory_manager_mut();
    let after_allocator = manager.native_allocator_snapshot().unwrap();
    assert_eq!(
        after_allocator.heap.heap_cursor,
        before_allocator.heap.heap_cursor
    );
    assert_eq!(
        after_allocator.heap.heap_limit,
        before_allocator.heap.heap_limit
    );
    assert_eq!(after_allocator.ptrs, before_allocator.ptrs);
    assert_eq!(
        after_allocator.free_ptr_blocks,
        before_allocator.free_ptr_blocks
    );
    assert_eq!(
        after_allocator.free_handle_blocks,
        before_allocator.free_handle_blocks
    );
    assert_eq!(manager.native_handle_records(), before_handles);
}

#[test]
fn native_apple_event_dispatch_enters_registered_classic_handler() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2800;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let classic_handler = 0x0000_4000;
    native.apple_events.handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: crate::guest_procedure::GuestProcedure::raw_m68k(classic_handler),
            refcon: 0xfeed_beef,
        },
    );
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;

    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert_eq!(action, PpcImportAction::Halt);
    assert!(native.guest_calls().has_m68k_execution());
    let pending = native.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.entry, classic_handler);
    assert_eq!(native.apple_events.pending_dispatches.len(), 1);
    let dispatch = native.apple_events.pending_dispatches[0];
    let mut classic_descriptors = SharedProcessAppleEventDescriptors::default();
    context.attach_apple_event_descriptors(&mut classic_descriptors);
    let event = classic_descriptors
        .events
        .get(&dispatch.descriptors)
        .expect("classic adapter sees native callback event semantics");
    assert_eq!(event.event_class, PPC_CORE_EVENT_CLASS);
    assert_eq!(event.event_id, PPC_OPEN_APPLICATION_EVENT);
    assert!(classic_descriptors
        .descriptors
        .contains_key(&(dispatch.descriptors + 8)));

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut native.apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
    }
    assert!(!classic_descriptors
        .events
        .contains_key(&dispatch.descriptors));
    assert!(!classic_descriptors
        .descriptors
        .contains_key(&dispatch.descriptors));
}

#[test]
fn classic_pack8_suspends_native_apple_event_dispatch() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2800;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let classic_handler = 0x0000_4000;
    native.apple_events.handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: crate::guest_procedure::GuestProcedure::raw_m68k(classic_handler),
            refcon: 0xfeed_beef,
        },
    );
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;

    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert_eq!(action, PpcImportAction::Halt);
    assert!(native.guest_calls().has_m68k_execution());
    let pending = native.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.entry, classic_handler);
    assert_eq!(native.apple_events.pending_dispatches.len(), 1);
    let dispatch = native.apple_events.pending_dispatches[0];
    let (mut classic, mut classic_cpu, mut classic_bus) = crate::trap::test_helpers::setup();
    context.attach_apple_event_descriptors(&mut classic.ae_descriptor_state);
    let event_copy = 0x20000;
    classic_bus.write_long(event_copy, PPC_CORE_EVENT_CLASS);
    classic_bus.write_long(event_copy + 4, dispatch.event_handle + 4);
    classic_bus.write_long(TEST_SP, event_copy);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x022b);
    classic.current_trap_word = 0xa816;
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(
        !classic
            .ae_descriptor_state
            .native_dispatches
            .last()
            .unwrap()
            .2
    );
    classic_bus.write_long(event_copy + 4, dispatch.event_handle);
    let duplicate = 0x21000;
    classic_bus.write_long(TEST_SP, duplicate);
    classic_bus.write_long(TEST_SP + 4, event_copy);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x0405);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 8), 0);
    let duplicate_handle = classic_bus.read_long(duplicate + 4);
    assert_ne!(duplicate_handle, 0);
    assert_ne!(duplicate_handle, dispatch.event_handle);
    assert_eq!(
        classic
            .ae_descriptor_state
            .native_descriptor_origins
            .get(&duplicate_handle),
        Some(&dispatch.event_handle)
    );
    let reply_copy = 0x20008;
    let reply_duplicate = 0x21008;
    classic_bus.write_long(reply_copy, PPC_CORE_EVENT_CLASS);
    classic_bus.write_long(reply_copy + 4, dispatch.reply_handle);
    classic_bus.write_long(TEST_SP, reply_duplicate);
    classic_bus.write_long(TEST_SP + 4, reply_copy);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x0405);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 8), 0);
    let reply_duplicate_handle = classic_bus.read_long(reply_duplicate + 4);
    assert_ne!(reply_duplicate_handle, 0);
    assert_ne!(reply_duplicate_handle, dispatch.reply_handle);
    assert_eq!(
        classic
            .ae_descriptor_state
            .native_descriptor_origins
            .get(&reply_duplicate_handle),
        Some(&dispatch.reply_handle)
    );
    let program = 0x22000;
    for (index, word) in [
        0x558fu16, 0x2f3c, 0x0002, 0x1000, 0x303c, 0x022b, 0xa816, 0x548f,
    ]
    .into_iter()
    .enumerate()
    {
        classic_bus.write_word(program + index as u32 * 2, word);
    }
    let mut guest_cpu = crate::cpu::M68kCpu::new();
    guest_cpu.write_reg(Register::PC, program);
    guest_cpu.write_reg(Register::A7, TEST_SP + 0x100);
    for _ in 0..3 {
        assert!(matches!(
            guest_cpu.step(&mut classic_bus),
            crate::cpu::StepResult::Ok
        ));
    }
    assert!(matches!(
        guest_cpu.step(&mut classic_bus),
        crate::cpu::StepResult::Aline(0xa816)
    ));
    assert_eq!(guest_cpu.read_reg(Register::D0) & 0xffff, 0x022b);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut guest_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 0xfe), 0);
    assert_eq!(guest_cpu.read_reg(Register::A7), TEST_SP + 0xfe);
    assert!(matches!(
        guest_cpu.step(&mut classic_bus),
        crate::cpu::StepResult::Ok
    ));
    assert_eq!(guest_cpu.read_reg(Register::A7), TEST_SP + 0x100);
    let mut classic_descriptors = SharedProcessAppleEventDescriptors::default();
    context.attach_apple_event_descriptors(&mut classic_descriptors);
    let event = classic_descriptors
        .events
        .get(&dispatch.descriptors)
        .expect("classic adapter sees native callback event semantics");
    assert_eq!(event.event_class, PPC_CORE_EVENT_CLASS);
    assert_eq!(event.event_id, PPC_OPEN_APPLICATION_EVENT);
    assert!(classic_descriptors
        .descriptors
        .contains_key(&(dispatch.descriptors + 8)));

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut native.apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
    }
    assert!(native.apple_events.pending_dispatches.is_empty());
    assert_eq!(native.apple_events.suspended_dispatches.len(), 1);
    assert!(classic_descriptors
        .events
        .contains_key(&dispatch.descriptors));
    assert!(classic_descriptors
        .descriptors
        .contains_key(&(dispatch.descriptors + 8)));
    assert!(classic_descriptors.native_dispatches.is_empty());
    let native_copy = PPC_DATA_BASE + 0x2900;
    native.memory.add_region(native_copy, vec![0; 24]);
    native
        .memory
        .write_u32_be(native_copy, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u32_be(native_copy + 4, duplicate_handle)
        .unwrap();
    native
        .memory
        .write_u32_be(native_copy + 8, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u32_be(native_copy + 12, reply_duplicate_handle)
        .unwrap();
    native.cpu.gpr[3] = native_copy;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::SetTheCurrentEvent,
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
        )
    };
    assert_eq!(action, PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
    let classic_current = 0x21020;
    classic_bus.write_long(TEST_SP, classic_current);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x021a);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_long(classic_current), PPC_CORE_EVENT_CLASS);
    assert_eq!(classic_bus.read_long(classic_current + 4), duplicate_handle);
    classic_bus.write_long(TEST_SP, event_copy);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x022c);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    native.cpu.gpr[3] = native_copy + 16;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::GetTheCurrentEvent,
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
        )
    };
    assert_eq!(action, PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
    assert_eq!(
        native.memory.read_u32_be(native_copy + 20),
        Some(dispatch.event_handle)
    );
    native.cpu.gpr[3..7].copy_from_slice(&[native_copy, native_copy + 8, 0, 0]);
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
        )
    };
    assert_eq!(action, PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
    assert!(native.apple_events.suspended_dispatches.is_empty());
    assert!(!classic_descriptors
        .native_descriptor_origins
        .contains_key(&duplicate_handle));
    assert!(!classic_descriptors
        .native_descriptor_origins
        .contains_key(&reply_duplicate_handle));
}

#[test]
fn ppc_special_handler_install_preserves_native_callbacks_and_system_scope() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEInstallSpecialHandler"),
        PpcImportDispatcherTarget::AEInstallSpecialHandler,
    );
    let pef = synthetic_pef_with_import(b"AEInstallSpecialHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let class = u32::from_be_bytes(*b"phac");
    for (system, handler) in [(false, 0x1000), (true, 0x2000), (false, 0x3000)] {
        loaded.cpu.gpr[3] = class;
        loaded.cpu.gpr[4] = handler;
        loaded.cpu.gpr[5] = u32::from(system);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallSpecialHandler,
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.apple_events.special_handlers.get(&(system, class)),
            Some(&handler),
        );
    }
    assert_eq!(
        loaded.apple_events.object_callbacks.get(&class),
        Some(&0x3000)
    );
    assert_eq!(
        loaded.apple_events.special_handlers.get(&(true, class)),
        Some(&0x2000)
    );
    assert_eq!(
        loaded.apple_events.special_handlers.get(&(false, class)),
        Some(&0x3000)
    );
}

#[test]
fn ae_remove_event_handler_requires_the_installed_handler() {
    let pef = synthetic_pef_with_import(b"AEInstallEventHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let (handler_tvector, handler_entry) = (scratch, scratch + 0x100);
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    loaded
        .memory
        .write_u32_be(handler_tvector, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry, 0x4e80_0020)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
    loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
    loaded.cpu.gpr[5] = handler_tvector;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    assert_eq!(loaded.apple_events.handlers.len(), 1);

    for (handler, expected) in [
        (scratch + 0x40, ppc_i16_result(-1717)),
        (handler_tvector, ppc_i16_result(PPC_NO_ERR)),
        (handler_tvector, ppc_i16_result(-1717)),
    ] {
        loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
        loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
        loaded.cpu.gpr[5] = handler;
        loaded.cpu.gpr[6] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AERemoveEventHandler);
        assert_eq!(loaded.cpu.gpr[3], expected);
    }
    assert_eq!(loaded.apple_events.handlers.len(), 0);
}

fn self_addressed_send_fixture() -> (PpcLoadedApp, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AESend")).unwrap();
    let scratch = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(scratch, vec![0; 0x1000]);
    loaded.memory.write_u32_be(scratch + 0x44, 2).unwrap();
    let ae = PpcImportDispatcherTarget::AppleEventCompatibility;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"psn ");
    loaded.cpu.gpr[4] = scratch + 0x40;
    loaded.cpu.gpr[5] = 8;
    loaded.cpu.gpr[6] = scratch;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateDesc),
    );
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 17;
    loaded.cpu.gpr[7] = 42;
    loaded.cpu.gpr[8] = scratch + 8;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateAppleEvent),
    );
    loaded
        .memory
        .write_bytes(scratch + 0x50, b"payload")
        .unwrap();
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"----");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[6] = scratch + 0x50;
    loaded.cpu.gpr[7] = 7;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::PutParamPtr),
    );
    (loaded, scratch)
}

#[test]
fn direct_suspended_event_survives_handler_return_and_times_out() {
    for (mode, classic, ignore_app, duplicate_suspend, abandon, keep, fail_stage) in [
        (1, false, false, false, false, false, 0),
        (1, true, false, false, false, false, 0),
        (1, false, true, false, false, false, 0),
        (1, false, false, false, false, true, 0),
        (1, false, false, false, true, false, 0),
        (2, false, false, false, true, false, 0),
        (2, false, false, false, false, false, 0),
        (3, false, false, false, false, false, 0),
        (3, false, false, true, false, false, 0),
        (3, false, false, false, false, false, 13),
        (3, false, false, false, false, false, 14),
    ] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let entry = scratch + 0x200;
        let observed = scratch + 0x100;
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = 1;
        binding.symbol_name = "AESuspendTheCurrentEvent".into();
        binding.dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
        loaded.import_count = 2;
        if duplicate_suspend {
            let mut duplicate_binding = loaded.imports[0].clone();
            duplicate_binding.symbol_index = 2;
            duplicate_binding.symbol_name = "AEDuplicateDesc".into();
            duplicate_binding.dispatcher_target =
                dispatcher_target_for_import("InterfaceLib", &duplicate_binding.symbol_name);
            duplicate_binding.trap_pc = PPC_IMPORT_TRAP_BASE + 8;
            loaded
                .memory
                .add_region(duplicate_binding.trap_pc, vec![0; 4]);
            loaded.imports.push(duplicate_binding);
            loaded.import_count = 3;
        }
        let mut code = vec![
            0x7c08_02a6,
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(32, 6, 3, 0),
            d_form_u(36, 6, 5, 0),
            d_form_u(32, 6, 3, 4),
            d_form_u(36, 6, 5, 4),
            d_form_u(32, 6, 4, 0),
            d_form_u(36, 6, 5, 8),
            d_form_u(32, 6, 4, 4),
            d_form_u(36, 6, 5, 12),
        ];
        if duplicate_suspend {
            code.push(d_form_u(36, 5, 1, 56));
            code.push(d_form_u(14, 4, 5, 0x30));
            let call_pc = entry + code.len() as u32 * 4;
            code.push(
                0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 8).wrapping_sub(call_pc) & 0x03ff_fffc),
            );
            code.push(d_form_u(32, 3, 1, 56));
            code.push(d_form_u(14, 3, 3, 0x30));
        }
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, 0),
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x80,
            observed,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        loaded.cpu.gpr[3..8].copy_from_slice(&[scratch + 8, scratch + 16, mode, 0, u32::MAX]);
        loaded.apple_events.fail_self_queue_reservation = fail_stage;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(if fail_stage == 0 {
                -1712
            } else {
                PPC_MEM_FULL_ERR
            })
        );
        assert!(loaded.apple_events.pending_dispatches.is_empty());
        if fail_stage != 0 {
            assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
            assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
            assert!(loaded
                .apple_events
                .descriptors
                .pending_reply_handles
                .is_empty());
            loaded.apple_events.fail_self_queue_reservation = 0;
            loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, 0, 0]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert!(loaded.apple_events.suspended_dispatches.is_empty());
            continue;
        }
        if abandon {
            let dispatch = loaded.apple_events.suspended_dispatches[0];
            loaded.cpu.gpr[3] = observed;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::SetTheCurrentEvent,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            let output = scratch + 16;
            for descriptor in if mode == 1 {
                vec![observed]
            } else {
                vec![observed, observed + 8]
            } {
                loaded.cpu.gpr[3] = descriptor;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AppleEventCompatibility(
                        PpcAppleEventCompatibilityOperation::DisposeDesc,
                    ),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
            assert!(loaded.apple_events.suspended_dispatches.is_empty());
            assert!(loaded
                .apple_events
                .descriptors
                .selected_current_event
                .is_none());
            assert!(!loaded
                .ptrs()
                .iter()
                .any(|record| record.ptr == dispatch.descriptors));
            assert!(!loaded
                .handles()
                .iter()
                .any(|record| record.handle == dispatch.event_handle
                    || record.handle == dispatch.reply_handle));
            if mode != 1 {
                loaded.cpu.gpr[3] = output;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AppleEventCompatibility(
                        PpcAppleEventCompatibilityOperation::CountItems,
                    ),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1718));
            }
            continue;
        }
        if duplicate_suspend {
            let duplicate = observed + 0x30;
            assert_ne!(
                loaded.memory.read_u32_be(duplicate + 4),
                loaded.memory.read_u32_be(observed + 4)
            );
            loaded.cpu.gpr[3] = duplicate;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::DisposeDesc,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        }
        loaded.cpu.gpr[3] = observed;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::SetTheCurrentEvent,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded.cpu.gpr[3] = scratch + 0x120;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::GetTheCurrentEvent,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(scratch + 0x124),
            loaded.memory.read_u32_be(observed + 4)
        );
        assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
        let event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            observed,
        )
        .unwrap();
        assert_eq!(event.fields[&u32::from_be_bytes(*b"----")].data, b"payload");
        assert_eq!(
            loaded.memory.read_u32_be(observed + 4),
            Some(loaded.apple_events.suspended_dispatches[0].event_handle)
        );
        if mode == 1 {
            let original_handle = loaded.apple_events.suspended_dispatches[0].event_handle;
            let duplicate = scratch + 0x130;
            loaded.cpu.gpr[3..5].copy_from_slice(&[observed, duplicate]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::DuplicateDesc,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            let duplicate_handle = loaded.memory.read_u32_be(duplicate + 4).unwrap();
            assert_ne!(duplicate_handle, original_handle);
            assert_eq!(
                loaded
                    .apple_events
                    .dispatch_descriptor_origins
                    .get(&duplicate_handle),
                Some(&original_handle)
            );
            let vector = scratch + 0x180;
            let resume_entry = scratch + 0x1a0;
            loaded.memory.write_u32_be(vector, resume_entry).unwrap();
            loaded.memory.write_u32_be(vector + 4, loaded.rtoc).unwrap();
            if classic {
                for (offset, word) in [0x3f7c, 0x0007, 0x0010, 0x4e74, 0x000c]
                    .into_iter()
                    .enumerate()
                {
                    loaded
                        .memory
                        .write_u16_be(resume_entry + offset as u32 * 2, word)
                        .unwrap();
                }
                loaded.apple_events.handlers.install(
                    false,
                    u32::from_be_bytes(*b"TEST"),
                    u32::from_be_bytes(*b"send"),
                    ProcessAppleEventHandler {
                        procedure: crate::guest_procedure::GuestProcedure::raw_m68k(resume_entry),
                        refcon: 0x1234,
                    },
                );
            } else {
                loaded
                    .memory
                    .write_u32_be(resume_entry, 0x3860_0007)
                    .unwrap();
                loaded
                    .memory
                    .write_u32_be(resume_entry + 4, 0x4e80_0020)
                    .unwrap();
                loaded.cpu.gpr[3..8].copy_from_slice(&[
                    u32::from_be_bytes(*b"TEST"),
                    u32::from_be_bytes(*b"send"),
                    vector,
                    0x1234,
                    0,
                ]);
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AEInstallEventHandler,
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
            if ignore_app {
                let system_entry = scratch + 0x1c0;
                let system_vector = scratch + 0x190;
                loaded
                    .memory
                    .write_u32_be(system_entry, 0x3860_0008)
                    .unwrap();
                loaded
                    .memory
                    .write_u32_be(system_entry + 4, 0x4e80_0020)
                    .unwrap();
                loaded
                    .memory
                    .write_u32_be(system_vector, system_entry)
                    .unwrap();
                loaded
                    .memory
                    .write_u32_be(system_vector + 4, loaded.rtoc)
                    .unwrap();
                loaded.cpu.gpr[3..8].copy_from_slice(&[
                    u32::from_be_bytes(*b"TEST"),
                    u32::from_be_bytes(*b"send"),
                    system_vector,
                    0,
                    1,
                ]);
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AEInstallEventHandler,
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
            loaded.cpu.gpr[3..7].copy_from_slice(&[
                duplicate,
                observed + 8,
                u32::MAX,
                if ignore_app {
                    2
                } else if keep {
                    0x8000_0000
                } else {
                    0
                },
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
                ),
            );
            assert_eq!(
                loaded.cpu.gpr[3],
                ppc_i16_result(if ignore_app { 8 } else { 7 })
            );
            assert!(loaded.apple_events.suspended_dispatches.is_empty());
            assert!(loaded
                .apple_events
                .descriptors
                .selected_current_event
                .is_none());
            assert_eq!(
                loaded
                    .handles()
                    .iter()
                    .any(|record| record.handle == original_handle),
                keep
            );
            if keep {
                assert!(ppc_ae_descriptor(
                    &mut loaded.memory,
                    &loaded.apple_events.descriptors,
                    observed
                )
                .is_some());
                loaded.cpu.gpr[3] = observed;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AppleEventCompatibility(
                        PpcAppleEventCompatibilityOperation::DisposeDesc,
                    ),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
                assert!(!loaded
                    .handles()
                    .iter()
                    .any(|record| record.handle == original_handle));
            }
            assert!(ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                duplicate
            )
            .is_some());
            loaded.cpu.gpr[3] = duplicate;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::DisposeDesc,
                ),
            );
            assert!(!loaded
                .apple_events
                .dispatch_descriptor_origins
                .contains_key(&duplicate_handle));
        } else {
            let output = scratch + 16;
            let key = u32::from_be_bytes(*b"----");
            loaded.cpu.gpr[3..7].copy_from_slice(&[output, key, PPC_TYPE_WILDCARD, 0]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::GetParamPtr,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1718));
            let (mut classic, mut classic_cpu, mut classic_bus) =
                crate::trap::test_helpers::setup();
            classic
                .ae_descriptor_state
                .attach_to(&loaded.apple_events.descriptors);
            let classic_original = 0x20000;
            let classic_duplicate = 0x20008;
            let count_ptr = 0x20020;
            classic_bus.write_long(classic_original, PPC_CORE_EVENT_CLASS);
            classic_bus.write_long(
                classic_original + 4,
                loaded.memory.read_u32_be(output + 4).unwrap(),
            );
            classic.current_trap_word = 0xa816;
            classic_bus.write_long(TEST_SP, classic_duplicate);
            classic_bus.write_long(TEST_SP + 4, classic_original);
            classic_cpu.write_reg(Register::A7, TEST_SP);
            classic_cpu.write_reg(Register::D0, 0x0405);
            assert!(classic
                .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
                .unwrap()
                .is_ok());
            assert_eq!(classic_bus.read_word(TEST_SP + 8), 0);
            classic_bus.write_long(TEST_SP, count_ptr);
            classic_bus.write_long(TEST_SP + 4, classic_duplicate);
            classic_cpu.write_reg(Register::A7, TEST_SP);
            classic_cpu.write_reg(Register::D0, 0x0407);
            assert!(classic
                .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
                .unwrap()
                .is_ok());
            assert_eq!(classic_bus.read_word(TEST_SP + 8), (-1718i16) as u16);
            let duplicate = scratch + 0x130;
            loaded.cpu.gpr[3..5].copy_from_slice(&[output, duplicate]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::DuplicateDesc,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            loaded.cpu.gpr[3] = duplicate;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::CountItems,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1718));
            if mode == 2 {
                loaded.cpu.gpr[3] = output;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AppleEventCompatibility(
                        PpcAppleEventCompatibilityOperation::DisposeDesc,
                    ),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
            let value = scratch + 0x150;
            loaded
                .memory
                .write_u32_be(value, u32::from_be_bytes(*b"done"))
                .unwrap();
            loaded.cpu.gpr[3..8].copy_from_slice(&[
                observed + 8,
                key,
                u32::from_be_bytes(*b"TEXT"),
                value,
                4,
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::PutParamPtr,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            let dispatcher = if mode == 3 {
                let vector = scratch + 0x180;
                let entry = scratch + 0x1a0;
                loaded.memory.write_u32_be(vector, entry).unwrap();
                loaded.memory.write_u32_be(vector + 4, loaded.rtoc).unwrap();
                loaded.memory.write_u32_be(entry, 0x3860_0007).unwrap();
                loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
                vector
            } else {
                0
            };
            loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, dispatcher, 0x1234]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
                ),
            );
            assert_eq!(
                loaded.cpu.gpr[3],
                ppc_i16_result(if mode == 3 { 7 } else { PPC_NO_ERR })
            );
            assert!(loaded.apple_events.suspended_dispatches.is_empty());
            assert!(loaded.apple_events.pending_dispatches.is_empty());
            let reply = ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                duplicate,
            )
            .unwrap();
            assert_eq!(reply.fields[&key].data, b"done");
            classic_bus.write_long(TEST_SP, count_ptr);
            classic_bus.write_long(TEST_SP + 4, classic_duplicate);
            classic_cpu.write_reg(Register::A7, TEST_SP);
            classic_cpu.write_reg(Register::D0, 0x0407);
            assert!(classic
                .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
                .unwrap()
                .is_ok());
            assert_eq!(classic_bus.read_word(TEST_SP + 8), 0);
            assert!(classic_bus.read_long(count_ptr) >= 1);
            classic_bus.write_long(TEST_SP, classic_duplicate);
            classic_cpu.write_reg(Register::A7, TEST_SP);
            classic_cpu.write_reg(Register::D0, 0x0204);
            assert!(classic
                .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
                .unwrap()
                .is_ok());
            if mode == 3 {
                assert_eq!(
                    reply.fields[&u32::from_be_bytes(*b"errn")].data,
                    7i32.to_be_bytes()
                );
            } else {
                assert!(!reply.fields.contains_key(&u32::from_be_bytes(*b"errn")));
            }
            loaded.cpu.gpr[3..10].copy_from_slice(&[
                duplicate,
                key,
                PPC_TYPE_WILDCARD,
                scratch + 0x160,
                scratch + 0x164,
                4,
                scratch + 0x168,
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::GetParamPtr,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 0x164),
                Some(u32::from_be_bytes(*b"done"))
            );
        }
    }
}

#[test]
fn guest_null_descriptor_does_not_reuse_stale_record_contents() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let descriptor = scratch + 8;
    loaded
        .memory
        .write_u32_be(descriptor, PPC_TYPE_NULL)
        .unwrap();
    loaded.memory.write_u32_be(descriptor + 4, 0).unwrap();
    let value = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        descriptor,
    )
    .unwrap();
    assert_eq!(value.desc_type, PPC_TYPE_NULL);
    assert!(value.data.is_empty());
    assert!(value.fields.is_empty());
    assert!(value.items.is_empty());
    assert!(value.attributes.is_empty());
    let output = scratch + 0x180;
    loaded.cpu.gpr[3..7].copy_from_slice(&[
        descriptor,
        u32::from_be_bytes(*b"----"),
        PPC_TYPE_WILDCARD,
        output,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1704));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        descriptor,
        u32::from_be_bytes(*b"----"),
        PPC_TYPE_WILDCARD,
        output,
        scratch + 0x190,
        16,
        scratch + 0x1a0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1704));
    loaded.cpu.gpr[3..8].copy_from_slice(&[descriptor, 1, PPC_TYPE_WILDCARD, 0, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1703));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    loaded.cpu.gpr[3..11].copy_from_slice(&[
        descriptor,
        1,
        PPC_TYPE_WILDCARD,
        0,
        output,
        scratch + 0x190,
        16,
        scratch + 0x1a0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1703));
    for (operation, error) in [
        (PpcAppleEventCompatibilityOperation::CountItems, -1703),
        (PpcAppleEventCompatibilityOperation::SizeOfNthItem, -1703),
        (PpcAppleEventCompatibilityOperation::SizeOfParam, -1704),
        (PpcAppleEventCompatibilityOperation::GetAttributeDesc, -50),
        (PpcAppleEventCompatibilityOperation::GetAttributePtr, -50),
    ] {
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            descriptor,
            u32::from_be_bytes(*b"evcl"),
            PPC_TYPE_WILDCARD,
            output,
            scratch + 0x190,
            16,
            scratch + 0x1a0,
        ]);
        match operation {
            PpcAppleEventCompatibilityOperation::CountItems => loaded.cpu.gpr[4] = output,
            PpcAppleEventCompatibilityOperation::SizeOfNthItem => {
                loaded.cpu.gpr[4..7].copy_from_slice(&[1, output, scratch + 0x190]);
            }
            PpcAppleEventCompatibilityOperation::SizeOfParam => {
                loaded.cpu.gpr[5..7].copy_from_slice(&[output, scratch + 0x190]);
            }
            _ => {}
        }
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(operation),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(error), "{operation:?}");
    }
}

#[test]
fn apple_event_descriptor_creation_invalid_data_returns_null_descriptor() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let output = scratch + 0x180;
    loaded.memory.write_u32_be(output, 0xfeed_beef).unwrap();
    loaded.memory.write_u32_be(output + 4, 0xdead_beef).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[u32::from_be_bytes(*b"TEXT"), 0xffff_ff00, 4, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
}

#[test]
fn null_descriptor_round_trips_through_record_without_handle() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let null_desc = scratch + 0x180;
    let result = scratch + 0x188;
    let key = u32::from_be_bytes(*b"null");
    let handles = loaded.handles();
    loaded.cpu.gpr[3..7].copy_from_slice(&[PPC_TYPE_NULL, 0, 0, null_desc]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(null_desc + 4), Some(0));
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, key, null_desc]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, key, PPC_TYPE_WILDCARD, result]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(result), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(result + 4), Some(0));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn apple_event_list_creation_failure_returns_null_descriptor() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let output = scratch + 0x180;
    loaded.memory.write_u32_be(output, 0xfeed_beef).unwrap();
    loaded.memory.write_u32_be(output + 4, 0xdead_beef).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 0x50, 3, 0, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
}

#[test]
fn disposing_apple_event_descriptor_produces_reusable_null_descriptor() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let descriptor = scratch + 8;
    for _ in 0..2 {
        loaded.cpu.gpr[3] = descriptor;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::DisposeDesc,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(descriptor), Some(PPC_TYPE_NULL));
        assert_eq!(loaded.memory.read_u32_be(descriptor + 4), Some(0));
    }
    let output = scratch + 0x180;
    let handles = loaded.handles();
    loaded.cpu.gpr[3..5].copy_from_slice(&[descriptor, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DuplicateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn missing_apple_event_descriptor_getters_return_null_descriptors() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let output = scratch + 0x180;
    for operation in [
        PpcAppleEventCompatibilityOperation::GetParamDesc,
        PpcAppleEventCompatibilityOperation::GetAttributeDesc,
    ] {
        loaded.memory.write_u32_be(output, 0xfeed_beef).unwrap();
        loaded.memory.write_u32_be(output + 4, 0xdead_beef).unwrap();
        loaded.cpu.gpr[3..7].copy_from_slice(&[
            scratch + 8,
            u32::from_be_bytes(*b"miss"),
            PPC_TYPE_WILDCARD,
            output,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(operation),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_DESC_NOT_FOUND));
        assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
        assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    }
    loaded.memory.write_u32_be(output, 0xfeed_beef).unwrap();
    loaded.memory.write_u32_be(output + 4, 0xdead_beef).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[scratch + 8, 99, PPC_TYPE_WILDCARD, 0, output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_DESC_NOT_FOUND));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));

    loaded.memory.write_u32_be(output, 0xfeed_beef).unwrap();
    loaded.memory.write_u32_be(output + 4, 0xdead_beef).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, u32::from_be_bytes(*b"nope"), output]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CoerceDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_COERCION_FAIL));
    assert_eq!(loaded.memory.read_u32_be(output), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
}

#[test]
fn aesend_event_handler_coerces_parameter_after_custom_handler_declines() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let observed = scratch + 0x100;
    let entry = scratch + 0x200;
    let coercion_entry = scratch + 0x300;
    loaded.memory.write_bytes(scratch + 0x50, b"42").unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        scratch + 8,
        u32::from_be_bytes(*b"----"),
        u32::from_be_bytes(*b"TEXT"),
        scratch + 0x50,
        2,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    loaded
        .memory
        .write_u32_be(scratch + 0x88, coercion_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    for (index, word) in [0x38e0_0001, 0x90e5_0000, 0x3860_f95c, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(coercion_entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[
        u32::from_be_bytes(*b"TEXT"),
        u32::from_be_bytes(*b"long"),
        scratch + 0x88,
        scratch + 0x120,
        1,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let mut binding = loaded.imports[0].clone();
    binding.symbol_index = 1;
    binding.symbol_name = "AEGetParamPtr".into();
    binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
    binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
    loaded.imports.push(binding);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 6, 5, 0),
        d_form_u(14, 7, 5, 8),
        d_form_u(14, 9, 5, 4),
        d_form_u(14, 8, 0, 4),
        d_form_u(15, 4, 0, 0x2d2d),
        d_form_u(24, 4, 4, 0x2d2d),
        d_form_u(15, 5, 0, 0x6c6f),
        d_form_u(24, 5, 5, 0x6e67),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    for (index, word) in code.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, *word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x80,
        observed,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(observed),
        Some(u32::from_be_bytes(*b"long"))
    );
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(4));
    assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(42));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(1));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert!(loaded.apple_events.pending_getter_coercions.is_empty());
}

#[test]
fn aesend_coercion_guest_allocation_failure_preserves_delivery() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let observed = scratch + 0x100;
    let entry = scratch + 0x200;
    let coercion_entry = scratch + 0x300;
    loaded.memory.write_bytes(scratch + 0x50, b"42").unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        scratch + 8,
        u32::from_be_bytes(*b"----"),
        u32::from_be_bytes(*b"TEXT"),
        scratch + 0x50,
        2,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    loaded
        .memory
        .write_u32_be(scratch + 0x88, coercion_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    for (index, word) in [0x38e0_0001, 0x90e5_0000, 0x3860_f95c, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(coercion_entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[
        u32::from_be_bytes(*b"TEXT"),
        u32::from_be_bytes(*b"long"),
        scratch + 0x88,
        scratch + 0x120,
        1,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let mut binding = loaded.imports[0].clone();
    binding.symbol_index = 1;
    binding.symbol_name = "AEGetParamPtr".into();
    binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
    binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
    loaded.imports.push(binding);
    loaded.import_count = 2;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(36, 5, 1, 56),
        d_form_u(14, 6, 5, 0),
        d_form_u(14, 7, 5, 8),
        d_form_u(14, 9, 5, 4),
        d_form_u(14, 8, 0, 4),
        d_form_u(15, 4, 0, 0x2d2d),
        d_form_u(24, 4, 4, 0x2d2d),
        d_form_u(15, 5, 0, 0x6c6f),
        d_form_u(24, 5, 5, 0x6e67),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(32, 5, 1, 56),
        d_form_u(36, 3, 5, 12),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    for (index, word) in code.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, *word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x80,
        observed,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    for _ in 0..32 {
        loaded.run_with_hle_imports(1);
        if !loaded.apple_events.pending_dispatches.is_empty() {
            break;
        }
    }
    assert_eq!(loaded.apple_events.pending_dispatches.len(), 1);
    loaded.set_heap_cursor(loaded.heap_limit());
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(observed + 12),
        Some(ppc_i16_result(PPC_MEM_FULL_ERR))
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(0));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert!(loaded.apple_events.pending_getter_coercions.is_empty());
}

#[test]
fn aesend_coercion_limited_heap_cleanup() {
    for space in [96, 128] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let observed = scratch + 0x100;
        let entry = scratch + 0x200;
        let coercion_entry = scratch + 0x300;
        loaded.memory.write_bytes(scratch + 0x50, b"42").unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            scratch + 8,
            u32::from_be_bytes(*b"----"),
            u32::from_be_bytes(*b"TEXT"),
            scratch + 0x50,
            2,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::PutParamPtr,
            ),
        );
        loaded
            .memory
            .write_u32_be(scratch + 0x88, coercion_entry)
            .unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x8c, loaded.rtoc)
            .unwrap();
        for (index, word) in [0x38e0_0001, 0x90e5_0000, 0x3860_f95c, 0x4e80_0020]
            .into_iter()
            .enumerate()
        {
            loaded
                .memory
                .write_u32_be(coercion_entry + index as u32 * 4, word)
                .unwrap();
        }
        loaded.cpu.gpr[3..9].copy_from_slice(&[
            u32::from_be_bytes(*b"TEXT"),
            u32::from_be_bytes(*b"long"),
            scratch + 0x88,
            scratch + 0x120,
            1,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallCoercionHandler,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = 1;
        binding.symbol_name = "AEGetParamPtr".into();
        binding.dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
        loaded.import_count = 2;
        let mut code = vec![
            0x7c08_02a6,
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(36, 5, 1, 56),
            d_form_u(14, 6, 5, 0),
            d_form_u(14, 7, 5, 8),
            d_form_u(14, 9, 5, 4),
            d_form_u(14, 8, 0, 4),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x6c6f),
            d_form_u(24, 5, 5, 0x6e67),
        ];
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(32, 5, 1, 56),
            d_form_u(36, 3, 5, 12),
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x80,
            observed,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        let handles = loaded.handles();
        let pointers = loaded.ptrs();
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        for _ in 0..32 {
            loaded.run_with_hle_imports(1);
            if !loaded.apple_events.pending_dispatches.is_empty() {
                break;
            }
        }
        assert_eq!(loaded.apple_events.pending_dispatches.len(), 1);
        loaded.set_heap_cursor(loaded.heap_limit() - space);
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let expected = if space == 96 {
            PPC_MEM_FULL_ERR
        } else {
            PPC_NO_ERR
        };
        assert_eq!(
            loaded.memory.read_u32_be(observed + 12),
            Some(ppc_i16_result(expected))
        );
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(1));
        if expected == PPC_NO_ERR {
            assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(42));
        }
        assert_eq!(loaded.handles(), handles);
        assert_eq!(loaded.ptrs(), pointers);
        assert!(loaded.apple_events.pending_dispatches.is_empty());
        assert!(loaded.apple_events.pending_getter_coercions.is_empty());
    }
}

#[test]
fn aesend_suspends_after_event_handler_coercion_and_resumes() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let observed = scratch + 0x100;
    let entry = scratch + 0x200;
    let coercion_entry = scratch + 0x300;
    loaded.memory.write_bytes(scratch + 0x50, b"42").unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        scratch + 8,
        u32::from_be_bytes(*b"----"),
        u32::from_be_bytes(*b"TEXT"),
        scratch + 0x50,
        2,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    loaded
        .memory
        .write_u32_be(scratch + 0x88, coercion_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    for (index, word) in [0x38e0_0001, 0x90e5_0000, 0x3860_f95c, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u32_be(coercion_entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..9].copy_from_slice(&[
        u32::from_be_bytes(*b"TEXT"),
        u32::from_be_bytes(*b"long"),
        scratch + 0x88,
        scratch + 0x120,
        1,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallCoercionHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let mut binding = loaded.imports[0].clone();
    binding.symbol_index = 1;
    binding.symbol_name = "AEGetParamPtr".into();
    binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
    binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
    loaded.imports.push(binding);
    let mut suspend_binding = loaded.imports[0].clone();
    suspend_binding.symbol_index = 2;
    suspend_binding.symbol_name = "AESuspendTheCurrentEvent".into();
    suspend_binding.dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", &suspend_binding.symbol_name);
    suspend_binding.trap_pc = PPC_IMPORT_TRAP_BASE + 8;
    loaded
        .memory
        .add_region(suspend_binding.trap_pc, vec![0; 4]);
    loaded.imports.push(suspend_binding);
    loaded.import_count = 3;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(36, 3, 1, 56),
        d_form_u(32, 6, 3, 0),
        d_form_u(36, 6, 5, 0x30),
        d_form_u(32, 6, 3, 4),
        d_form_u(36, 6, 5, 0x34),
        d_form_u(32, 6, 4, 0),
        d_form_u(36, 6, 5, 0x38),
        d_form_u(32, 6, 4, 4),
        d_form_u(36, 6, 5, 0x3c),
        d_form_u(14, 6, 5, 0),
        d_form_u(14, 7, 5, 8),
        d_form_u(14, 9, 5, 4),
        d_form_u(14, 8, 0, 4),
        d_form_u(15, 4, 0, 0x2d2d),
        d_form_u(24, 4, 4, 0x2d2d),
        d_form_u(15, 5, 0, 0x6c6f),
        d_form_u(24, 5, 5, 0x6e67),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.push(d_form_u(32, 3, 1, 56));
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 8).wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    for (index, word) in code.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, *word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x80,
        observed,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 3);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1712));
    assert_eq!(
        loaded.memory.read_u32_be(observed),
        Some(u32::from_be_bytes(*b"long"))
    );
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(4));
    assert_eq!(loaded.memory.read_u32_be(observed + 8), Some(42));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(1));
    assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
    loaded.cpu.gpr[3..7].copy_from_slice(&[observed + 0x30, observed + 0x38, 0, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.apple_events.suspended_dispatches.is_empty());
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert!(loaded.apple_events.pending_getter_coercions.is_empty());
}

#[test]
fn aesend_direct_queue_and_wait_preserve_payload_reply_and_handler_errors() {
    for mode in [2, 3] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let entry = scratch + 0x200;
        let observed = scratch + 0x100;
        for (index, symbol) in [(1, "AEGetParamPtr"), (2, "AEPutParamPtr")] {
            let mut binding = loaded.imports[0].clone();
            binding.symbol_index = index;
            binding.symbol_name = symbol.into();
            binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", symbol);
            binding.trap_pc = PPC_IMPORT_TRAP_BASE + index * 4;
            loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
            loaded.imports.push(binding);
        }
        loaded.import_count = 3;
        // Preserve the PPC linkage area and outgoing parameter home area.
        let mut code = vec![
            0x7c08_02a6, // mflr r0
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(36, 5, 1, 56), // refcon = output buffer
            d_form_u(36, 4, 1, 60), // reply descriptor
            d_form_u(14, 6, 5, 0),
            d_form_u(14, 7, 5, 8),
            d_form_u(14, 9, 5, 4),
            d_form_u(14, 8, 0, 16),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x5445),
            d_form_u(24, 5, 5, 0x5854),
        ];
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(32, 3, 1, 60),
            d_form_u(32, 6, 1, 56),
            d_form_u(32, 7, 6, 4),
            d_form_u(14, 6, 6, 8),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x5445),
            d_form_u(24, 5, 5, 0x5854),
        ]);
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 8).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, (-50i16) as u16),
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
        loaded.cpu.gpr[5] = scratch + 0x80;
        loaded.cpu.gpr[6] = observed;
        loaded.cpu.gpr[7] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        let before = loaded.handles().len();
        let pointers = loaded.ptrs();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        loaded.cpu.gpr[3] = scratch + 8;
        loaded.cpu.gpr[4] = scratch + 16;
        loaded.cpu.gpr[5] = mode;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = u32::MAX;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(probe.handled_import_count, 3);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, observed + 8, 7),
            Some(b"payload".to_vec())
        );
        assert!(loaded.apple_events.pending_dispatches.is_empty());
        assert_eq!(loaded.ptrs(), pointers);
        assert_eq!(loaded.handles().len(), before + 1);
        let reply = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 16,
        )
        .unwrap();
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(reply.fields[&u32::from_be_bytes(*b"----")].data, b"payload");
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            (-50i32).to_be_bytes()
        );
        assert_eq!(
            reply.attributes[&u32::from_be_bytes(*b"rtid")].data,
            17i16.to_be_bytes()
        );
    }
}

#[test]
fn aesend_no_reply_passes_a_null_descriptor_and_releases_temporaries() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let entry = scratch + 0x200;
    let observed = scratch + 0x100;
    let code = [
        d_form_u(32, 6, 4, 0),
        d_form_u(36, 6, 5, 0),
        d_form_u(32, 6, 4, 4),
        d_form_u(36, 6, 5, 4),
        d_form_u(14, 3, 0, 42),
        0x4e80_0020,
    ];
    for (index, word) in code.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, *word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
    loaded.cpu.gpr[5] = scratch + 0x80;
    loaded.cpu.gpr[6] = observed;
    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0));
    assert_eq!(loaded.memory.read_u32_be(scratch + 16), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    loaded.cpu.gpr[3] = scratch + 16;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"----");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[6] = scratch + 0x50;
    loaded.cpu.gpr[7] = 7;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1707));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn aesend_rejects_missing_handlers_targets_and_invalid_modes_without_allocating() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let handles = loaded.handles();
    for (mode, expected) in [(0, -1710i16), (3, PPC_ERR_AE_EVENT_NOT_HANDLED)] {
        loaded.cpu.gpr[3] = scratch + 8;
        loaded.cpu.gpr[4] = scratch + 16;
        loaded.cpu.gpr[5] = mode;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected));
        assert_eq!(loaded.handles(), handles);
    }
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data[7] = 9;
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 3;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn aesend_direct_nested_handler_calls_release_their_own_temporaries() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let ae = PpcImportDispatcherTarget::AppleEventCompatibility;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"innr");
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 18;
    loaded.cpu.gpr[7] = 42;
    loaded.cpu.gpr[8] = scratch + 24;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateAppleEvent),
    );
    let mut nested_send = loaded.imports[0].clone();
    nested_send.symbol_index = 1;
    nested_send.symbol_name = "AESend".into();
    nested_send.dispatcher_target = ae(PpcAppleEventCompatibilityOperation::Send);
    nested_send.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(nested_send.trap_pc, vec![0; 4]);
    loaded.imports.push(nested_send);
    loaded.import_count = 2;
    let outer = scratch + 0x200;
    let inner = scratch + 0x300;
    let mut outer_code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 5, 24),
        d_form_u(14, 4, 5, 32),
        d_form_u(14, 5, 0, 1),
        d_form_u(14, 6, 0, 0),
        d_form_u(14, 7, 0, u16::MAX),
    ];
    let call_pc = outer + outer_code.len() as u32 * 4;
    outer_code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    outer_code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    let inner_code = vec![
        d_form_u(14, 6, 0, 1),
        d_form_u(36, 6, 5, 0),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (entry, code) in [(outer, outer_code), (inner, inner_code)] {
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
    }
    for (tvector, entry, id, refcon) in [
        (scratch + 0x80, outer, *b"send", scratch),
        (scratch + 0x88, inner, *b"innr", scratch + 0x100),
    ] {
        loaded.memory.write_u32_be(tvector, entry).unwrap();
        loaded
            .memory
            .write_u32_be(tvector + 4, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
        loaded.cpu.gpr[4] = u32::from_be_bytes(id);
        loaded.cpu.gpr[5] = tvector;
        loaded.cpu.gpr[6] = refcon;
        loaded.cpu.gpr[7] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
    }
    let handles = loaded.handles().len();
    let pointers = loaded.ptrs();
    loaded.imports[0].dispatcher_target = ae(PpcAppleEventCompatibilityOperation::Send);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = u32::MAX;
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(1));
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.handles().len(), handles + 1);
    assert_eq!(loaded.ptrs(), pointers);
}

#[test]
fn aesend_real_psn_queues_event_and_optional_reply() {
    for mode in [1, 2] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        loaded.memory.write_u32_be(scratch + 0x64, 256).unwrap();
        loaded.cpu.gpr[3..7].copy_from_slice(&[
            u32::from_be_bytes(*b"psn "),
            scratch + 0x60,
            8,
            scratch + 0x38,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CreateDesc,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        // Recreate the event so the address attribute contains the real PSN.
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
        loaded.cpu.gpr[5] = scratch + 0x38;
        loaded.cpu.gpr[6] = 17;
        loaded.cpu.gpr[7] = 42;
        loaded.cpu.gpr[8] = scratch + 0x28;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CreateAppleEvent,
            ),
        );
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            scratch + 0x28,
            u32::from_be_bytes(*b"----"),
            u32::from_be_bytes(*b"TEXT"),
            scratch + 0x50,
            7,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::PutParamPtr,
            ),
        );
        let entry = scratch + 0x200;
        let observed = scratch + 0x100;
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = 1;
        binding.symbol_name = "AEGetParamPtr".into();
        binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", "AEGetParamPtr");
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
        loaded.import_count = 2;
        let mut code = vec![
            0x7c08_02a6,
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(32, 6, 4, 0),
            d_form_u(36, 6, 5, 32),
            d_form_u(14, 6, 5, 0),
            d_form_u(14, 7, 5, 8),
            d_form_u(14, 9, 5, 4),
            d_form_u(14, 8, 0, 16),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x5445),
            d_form_u(24, 5, 5, 0x5854),
        ];
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, (-50i16) as u16),
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x80,
            observed,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 0x28, scratch + 16, mode]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let queue = loaded.event_queue();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.front().unwrap().what, PPC_HIGH_LEVEL_EVENT);
        assert_eq!(queue.front().unwrap().message, u32::from_be_bytes(*b"TEST"));
        let event = &loaded.apple_events.queued_self_events[0].1;
        assert_eq!(event.fields[&u32::from_be_bytes(*b"----")].data, b"payload");
        assert_eq!(
            event.attributes[&u32::from_be_bytes(*b"esrc")].data,
            2i16.to_be_bytes()
        );
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::EventAvail);
        assert_eq!(loaded.cpu.gpr[3], 1);
        assert!(loaded.apple_events.current_self_event.is_none());
        assert_eq!(loaded.apple_events.queued_self_events.len(), 1);
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        loaded.cpu.gpr[3] = scratch + 0x90;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        if mode == 2 {
            loaded.apple_events.fail_self_queue_reservation = 11;
        }
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(probe.handled_import_count, 2);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-50));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, observed + 8, 7),
            Some(b"payload".to_vec())
        );
        assert_eq!(
            loaded.memory.read_u32_be(observed + 32),
            Some(if mode == 1 {
                PPC_TYPE_NULL
            } else {
                PPC_CORE_EVENT_CLASS
            })
        );
        assert!(loaded.apple_events.current_self_event.is_none());
        if mode == 2 {
            assert_eq!(loaded.event_queue().len(), 0);
            assert_eq!(loaded.apple_events.completed_self_replies.len(), 1);
            loaded.apple_events.fail_self_queue_reservation = 0;
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::EventAvail);
            assert_eq!(loaded.cpu.gpr[3], 1);
            assert_eq!(loaded.event_queue().len(), 1);
            let reply = &loaded.apple_events.queued_self_events[0].1;
            assert_eq!(reply.data, b"aevtansr");
            assert_eq!(
                reply.fields[&u32::from_be_bytes(*b"errn")].data,
                (-50i32).to_be_bytes()
            );
            assert_eq!(
                reply.attributes[&u32::from_be_bytes(*b"rtid")].data,
                17i16.to_be_bytes()
            );
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
            );
            assert_eq!(loaded.cpu.gpr[3], 1);
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 0x92),
                Some(PPC_CORE_EVENT_CLASS)
            );
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 0x9a),
                Some(u32::from_be_bytes(*b"ansr"))
            );
            assert!(loaded.apple_events.current_self_event.is_some());
            // Reuse the handler code to read the queued reply's error parameter.
            code[9] = d_form_u(15, 4, 0, 0x6572);
            code[10] = d_form_u(24, 4, 4, 0x726e);
            code[11] = d_form_u(15, 5, 0, 0x6c6f);
            code[12] = d_form_u(24, 5, 5, 0x6e67);
            let return_index = code.len() - 2;
            code[return_index] = d_form_u(14, 3, 0, 0);
            for (index, word) in code.iter().enumerate() {
                loaded
                    .memory
                    .write_u32_be(entry + index as u32 * 4, *word)
                    .unwrap();
            }
            loaded.cpu.gpr[3..8].copy_from_slice(&[
                PPC_CORE_EVENT_CLASS,
                u32::from_be_bytes(*b"ansr"),
                scratch + 0x80,
                observed,
                0,
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AEInstallEventHandler,
            );
            loaded.cpu.gpr[3] = scratch + 0x90;
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
            let probe = loaded.run_with_hle_imports(256);
            assert_eq!(probe.handled_import_count, 2);
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(
                loaded.memory.read_u32_be(observed + 8),
                Some((-50i32) as u32)
            );
            assert!(loaded.event_queue().is_empty());
            continue;
        }
        assert!(loaded.apple_events.queued_self_events.is_empty());
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 0x28, 0, 1]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert!(loaded.apple_events.current_self_event.is_some());
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        loaded.cpu.gpr[5] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert!(loaded.apple_events.current_self_event.is_none());
    }
}

#[test]
fn aesend_signature_targets_launched_application_and_preserves_queue_order() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    for (path, creator) in [("Other", *b"OTHR"), ("Chosen", *b"APPL")] {
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: path.to_owned(),
            data: Vec::new().into(),
            creator: u32::from_be_bytes(creator),
            file_type: u32::from_be_bytes(*b"APPL"),
            finder_flags: 0,
            dirty: false,
        });
    }
    loaded.set_launched_app_path("Chosen");
    let input = PpcInputSnapshot::default();
    loaded.set_input_snapshot(input);
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    let address = event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap();
    address.desc_type = u32::from_be_bytes(*b"sign");
    address.data = b"OTHR".to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, 0, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
    assert!(loaded.event_queue().is_empty());
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = b"APPL".to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    for (payload, priority) in [
        (b"first".as_slice(), 0),
        (b"second".as_slice(), 0),
        (b"high".as_slice(), 1),
    ] {
        loaded.memory.write_bytes(scratch + 0x50, payload).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            scratch + 8,
            u32::from_be_bytes(*b"----"),
            u32::from_be_bytes(*b"TEXT"),
            scratch + 0x50,
            payload.len() as u32,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::PutParamPtr,
            ),
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, 0, 1]);
        loaded.cpu.gpr[6] = priority;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    // The sender may dispose its event before the recipient processes it.
    loaded.cpu.gpr[3] = scratch + 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.event_queue().len(), 3);
    assert_eq!(
        loaded.event_queue().front().unwrap().modifiers,
        ppc_current_event_modifiers(input)
    );
    // An unrelated event can have the same guest-visible EventRecord bytes.
    let mut collision = loaded.event_queue().front().unwrap();
    collision.delivery_id = None;
    loaded
        .event_queue()
        .with_mut(|queue| queue.push_front(collision));
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.apple_events.current_self_event.is_none());
    assert_eq!(loaded.apple_events.queued_self_events.len(), 3);
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED)
    );
    assert_eq!(loaded.apple_events.queued_self_events.len(), 3);
    for expected in [
        b"high".as_slice(),
        b"first".as_slice(),
        b"second".as_slice(),
    ] {
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        let event = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
        assert_eq!(event.fields[&u32::from_be_bytes(*b"----")].data, expected);
    }
    assert!(loaded.apple_events.queued_self_events.is_empty());
}

#[test]
fn aesend_prunes_removed_queue_payload_without_losing_other_events() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    for payload in [b"first".as_slice(), b"second".as_slice()] {
        loaded.memory.write_bytes(scratch + 0x50, payload).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            scratch + 8,
            u32::from_be_bytes(*b"----"),
            u32::from_be_bytes(*b"TEXT"),
            scratch + 0x50,
            payload.len() as u32,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::PutParamPtr,
            ),
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, 0, 1]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    loaded.event_queue().with_mut(|queue| {
        queue.pop_front();
    });
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.apple_events.queued_self_events.is_empty());
    let current = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
    assert_eq!(
        current.fields[&u32::from_be_bytes(*b"----")].data,
        b"second"
    );
}

#[test]
fn queued_apple_event_pruning_preserves_survivors_in_order() {
    let mut state = PpcAppleEventState::default();
    let mut queue = VecDeque::new();
    for id in 1..=1000u64 {
        let record = PpcQueuedEvent {
            delivery_id: Some(id),
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"TEST"),
            when: 42,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        };
        state.queued_self_events.push_back((
            record,
            ProcessAeDescriptor::default(),
            PpcSelfReplyRoute::None,
        ));
        if id % 2 == 0 {
            queue.push_back(record);
        }
    }
    state.prune_queued_self_events(&queue);
    let ids: Vec<_> = state
        .queued_self_events
        .iter()
        .map(|(event, _, _)| event.delivery_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        (1..=1000u64).filter(|id| id % 2 == 0).collect::<Vec<_>>()
    );
}

#[test]
fn aesend_source_attribute_reservation_failure_leaves_request_unchanged() {
    for direct in [false, true] {
        for stage in [9, 10, 12] {
            let (mut loaded, scratch) = self_addressed_send_fixture();
            if !direct {
                let mut event = ppc_ae_descriptor(
                    &mut loaded.memory,
                    &loaded.apple_events.descriptors,
                    scratch + 8,
                )
                .unwrap();
                event
                    .attributes
                    .get_mut(&u32::from_be_bytes(*b"addr"))
                    .unwrap()
                    .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
                ppc_store_ae_descriptor_semantics(
                    &mut loaded.memory,
                    &loaded.apple_events.descriptors,
                    scratch + 8,
                    event,
                );
            }
            let before = ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                scratch + 8,
            )
            .unwrap();
            loaded.apple_events.fail_self_queue_reservation = stage;
            loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, 0, 1]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::Send,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
            assert_eq!(loaded.event_queue().len(), 0);
            assert!(loaded.apple_events.pending_dispatches.is_empty());
            assert_eq!(
                ppc_ae_descriptor(
                    &mut loaded.memory,
                    &loaded.apple_events.descriptors,
                    scratch + 8,
                )
                .unwrap(),
                before
            );
        }
    }
}

#[test]
fn aesend_queued_requests_do_not_expose_timeout_or_modify_sender() {
    for reply_mode in [1, 2] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        event.attributes.insert(
            u32::from_be_bytes(*b"timo"),
            ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"long"),
                data: 120i32.to_be_bytes().to_vec(),
                ..Default::default()
            },
        );
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event.clone(),
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, reply_mode]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert!(!loaded.apple_events.queued_self_events[0]
            .1
            .attributes
            .contains_key(&u32::from_be_bytes(*b"timo")));
        assert_eq!(
            ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                scratch + 8
            ),
            Some(event)
        );
    }
}

#[test]
fn aesend_queue_reservation_failure_preserves_existing_event_and_retries() {
    for stage in 1..=3 {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, 0, 1]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let original = loaded.apple_events.queued_self_events[0].0;
        loaded.apple_events.fail_self_queue_reservation = stage;
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        assert_eq!(loaded.event_queue().len(), 1);
        assert_eq!(loaded.apple_events.queued_self_events.len(), 1);
        assert_eq!(loaded.apple_events.queued_self_events[0].0, original);
        loaded.apple_events.fail_self_queue_reservation = 0;
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.event_queue().len(), 2);
        assert_eq!(loaded.apple_events.queued_self_events.len(), 2);
        assert_eq!(loaded.apple_events.queued_self_events[0].0, original);
    }
}

#[test]
fn aesend_many_discarded_requests_each_produce_a_reply() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    for return_id in 0..32i16 {
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"rtid"))
            .unwrap()
            .data = return_id.to_be_bytes().to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    loaded.event_queue().with_mut(|queue| queue.clear());
    loaded.apple_events.fail_self_queue_reservation = 8;
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.apple_events.completed_self_replies.len(), 32);
    loaded.apple_events.fail_self_queue_reservation = 0;
    for return_id in 0..32i16 {
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        let reply = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(
            reply.attributes[&u32::from_be_bytes(*b"rtid")].data,
            return_id.to_be_bytes()
        );
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
        );
    }
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn aesend_completed_reply_survives_reservation_failure_until_retry() {
    for stage in 4..=8 {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        loaded.apple_events.fail_self_queue_reservation = stage;
        loaded.cpu.gpr[3] = scratch + 0x90;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED)
        );
        assert_eq!(loaded.apple_events.completed_self_replies.len(), 1);
        assert_eq!(loaded.event_queue().len(), 0);
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.apple_events.completed_self_replies.len(), 1);
        loaded.apple_events.fail_self_queue_reservation = 0;
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        let reply = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
        );
        assert!(loaded.apple_events.completed_self_replies.is_empty());
    }
}

#[test]
fn queued_dispatch_reservation_failure_retains_reply_route() {
    for (waiting, stage) in [false, true]
        .into_iter()
        .flat_map(|waiting| (15..=20).map(move |stage| (waiting, stage)))
    {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let entry = scratch + 0x200;
        loaded.memory.write_u32_be(entry, 0x4e80_0020).unwrap();
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x80,
            0,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        if waiting {
            loaded.apple_events.queued_self_events[0].2 = PpcSelfReplyRoute::Waiting(42);
            loaded.apple_events.completed_wait_replies.reserve(1);
        }
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        loaded.apple_events.fail_self_queue_reservation = stage;
        loaded.cpu.gpr[3] = scratch + 0x90;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        if waiting {
            assert!(matches!(
                loaded.apple_events.take_wait_reply(42),
                Some(PpcPendingSelfReply::Error(_, PPC_MEM_FULL_ERR))
            ));
        } else {
            assert_eq!(loaded.event_queue().len(), 1);
            let reply = &loaded.apple_events.queued_self_events[0].1;
            assert_eq!(reply.data, b"aevtansr");
            assert_eq!(
                reply.fields[&u32::from_be_bytes(*b"errn")].data,
                i32::from(PPC_MEM_FULL_ERR).to_be_bytes()
            );
        }
        assert!(loaded.apple_events.pending_dispatches.is_empty());
    }
}

#[test]
fn queued_wait_filter_retains_reply_until_suspended_handler_resumes() {
    for redispatch in 0..=3 {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = 1;
        binding.symbol_name = "AESuspendTheCurrentEvent".into();
        binding.dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
        loaded.import_count = 2;
        let observed = scratch + 0x100;
        let handler = scratch + 0x400;
        let mut code = vec![
            0x7c08_02a6,
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(32, 6, 3, 0),
            d_form_u(36, 6, 5, 0),
            d_form_u(32, 6, 3, 4),
            d_form_u(36, 6, 5, 4),
            d_form_u(32, 6, 4, 0),
            d_form_u(36, 6, 5, 8),
            d_form_u(32, 6, 4, 4),
            d_form_u(36, 6, 5, 12),
        ];
        let call_pc = handler + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, 0),
            0x4e80_0020,
        ]);
        for (index, word) in code.into_iter().enumerate() {
            loaded
                .memory
                .write_u32_be(handler + index as u32 * 4, word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x88, handler).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x8c, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x88,
            observed,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        let filter = scratch + 0x200;
        loaded
            .memory
            .write_u32_be(filter, d_form_u(14, 3, 0, 1))
            .unwrap();
        loaded.memory.write_u32_be(filter + 4, 0x4e80_0020).unwrap();
        loaded.memory.write_u32_be(scratch + 0x80, filter).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            u32::MAX - 1,
            0,
            scratch + 0x80,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        assert_eq!(
            loaded.run_with_hle_imports(512).unsupported_import_index,
            None
        );
        assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
        assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
        assert!(loaded.apple_events.completed_wait_replies.is_empty());
        assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
        let waiting_cpu = loaded.cpu.clone();
        let redispatch_entry = scratch + 0x600;
        loaded
            .memory
            .write_u32_be(redispatch_entry, d_form_u(14, 3, 0, 7))
            .unwrap();
        loaded
            .memory
            .write_u32_be(redispatch_entry + 4, 0x4e80_0020)
            .unwrap();
        if redispatch == 3 {
            let mut code = vec![
                0x7c08_02a6,
                d_form_u(36, 0, 1, 8),
                d_form_u(37, 1, 1, (-96i16) as u16),
            ];
            let call_pc = redispatch_entry + code.len() as u32 * 4;
            code.push(
                0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc),
            );
            code.extend([
                d_form_u(14, 1, 1, 96),
                d_form_u(32, 0, 1, 8),
                0x7c08_03a6,
                d_form_u(14, 3, 0, 7),
                0x4e80_0020,
            ]);
            for (index, word) in code.into_iter().enumerate() {
                loaded
                    .memory
                    .write_u32_be(redispatch_entry + index as u32 * 4, word)
                    .unwrap();
            }
        }
        loaded
            .memory
            .write_u32_be(scratch + 0x90, redispatch_entry)
            .unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x94, loaded.rtoc)
            .unwrap();
        let dispatcher = if redispatch == 2 {
            for (index, word) in [0x3f7cu16, 7, 0x0010, 0x4e74, 0x000c]
                .into_iter()
                .enumerate()
            {
                loaded
                    .memory
                    .write_u16_be(scratch + 0x700 + index as u32 * 2, word)
                    .unwrap();
            }
            loaded.cpu.gpr[3..6].copy_from_slice(&[
                scratch + 0x700,
                0x0000_0fe0,
                PPC_ROUTINE_RECORD_M68K_ISA as u32,
            ]);
            run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRoutineDescriptor);
            loaded.cpu.gpr[3]
        } else if redispatch == 1 || redispatch == 3 {
            scratch + 0x90
        } else {
            0
        };
        loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, dispatcher, 0]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
        );
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            probe.handled_import_count,
            if redispatch == 3 { 2 } else { 1 }
        );
        drain_test_m68k_guest_calls(&mut loaded);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(if redispatch != 0 { 7 } else { PPC_NO_ERR })
        );
        if redispatch == 3 {
            assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
            assert!(loaded.apple_events.completed_wait_replies.is_empty());
            loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, 0, 0]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        }
        assert!(loaded.apple_events.suspended_dispatches.is_empty());
        assert_eq!(loaded.apple_events.completed_wait_replies.len(), 1);
        loaded.cpu = waiting_cpu;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
        assert!(loaded.apple_events.waiting_sends.is_empty());
        let reply = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 16,
        )
        .unwrap();
        assert_eq!(reply.data, b"aevtansr");
        if redispatch == 1 || redispatch == 2 {
            assert_eq!(
                reply.fields[&u32::from_be_bytes(*b"errn")].data,
                7i32.to_be_bytes()
            );
        } else {
            assert!(!reply.fields.contains_key(&u32::from_be_bytes(*b"errn")));
        }
    }
}

#[test]
fn queued_wait_filter_can_send_nested_direct_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 0, 2].to_vec();
    loaded
        .memory
        .write_u32_be(scratch + 0x300, PPC_CORE_EVENT_CLASS)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x304, 0).unwrap();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x300,
        event,
    );
    let handler = scratch + 0x400;
    loaded
        .memory
        .write_u32_be(handler, d_form_u(32, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, d_form_u(14, 6, 6, 1))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 8, d_form_u(36, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 12, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 16, 0x4e80_0020)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x88, handler).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let entry = scratch + 0x200;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 15, 0),
        d_form_u(14, 4, 15, 16),
        d_form_u(14, 5, 0, 1),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | (PPC_IMPORT_TRAP_BASE.wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ]);
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x120;
    loaded.cpu.gpr[15] = scratch + 0x300;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(512).unsupported_import_index,
        None
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(2));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_can_complete_after_nested_wait_times_out() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    loaded
        .memory
        .write_u32_be(scratch + 0x300, PPC_CORE_EVENT_CLASS)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x304, 0).unwrap();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x300,
        event,
    );
    let handler = scratch + 0x400;
    loaded
        .memory
        .write_u32_be(handler, d_form_u(32, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, d_form_u(14, 6, 6, 1))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 8, d_form_u(36, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 12, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 16, 0x4e80_0020)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x88, handler).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let entry = scratch + 0x200;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 15, 0),
        d_form_u(14, 4, 15, 16),
        d_form_u(14, 5, 0, 3),
        d_form_u(14, 6, 0, 0),
        d_form_u(14, 7, 0, 0),
        d_form_u(14, 8, 0, 0),
        d_form_u(14, 9, 0, 0),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | (PPC_IMPORT_TRAP_BASE.wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(36, 3, 14, 4),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ]);
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x120;
    loaded.cpu.gpr[15] = scratch + 0x300;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(512).unsupported_import_index,
        None
    );
    for _ in 0..5 {
        if loaded.cpu.pc == PPC_HALT_PC {
            break;
        }
        assert_eq!(
            loaded.run_with_hle_imports(512).unsupported_import_index,
            None
        );
    }
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x124),
        Some(ppc_i16_result(-1712))
    );
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 1);
    assert_eq!(
        loaded.apple_events.queued_self_events[0].2,
        PpcSelfReplyRoute::Abandoned
    );
}

#[test]
fn queued_wait_filter_can_complete_successful_nested_wait() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    loaded
        .memory
        .write_u32_be(scratch + 0x300, PPC_CORE_EVENT_CLASS)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x304, 0).unwrap();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x300,
        event,
    );
    let handler = scratch + 0x400;
    loaded
        .memory
        .write_u32_be(handler, d_form_u(32, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, d_form_u(14, 6, 6, 1))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 8, d_form_u(36, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 12, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 16, 0x4e80_0020)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x88, handler).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let entry = scratch + 0x200;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 15, 0),
        d_form_u(14, 4, 15, 16),
        d_form_u(14, 5, 0, 3),
        d_form_u(14, 6, 0, 1),
        d_form_u(14, 7, 0, (-2i16) as u16),
        d_form_u(14, 8, 0, 0),
        d_form_u(14, 9, 16, 0),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | (PPC_IMPORT_TRAP_BASE.wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(36, 3, 14, 4),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ]);
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x500, d_form_u(14, 3, 0, 1))
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x504, 0x4e80_0020)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x90, scratch + 0x500)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x94, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[16] = scratch + 0x90;
    loaded.cpu.gpr[14] = scratch + 0x120;
    loaded.cpu.gpr[15] = scratch + 0x300;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(512).unsupported_import_index,
        None
    );
    for _ in 0..5 {
        if loaded.cpu.pc == PPC_HALT_PC {
            break;
        }
        assert_eq!(
            loaded.run_with_hle_imports(512).unsupported_import_index,
            None
        );
    }
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(2));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x124),
        Some(ppc_i16_result(PPC_NO_ERR))
    );
    let inner_reply = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x310,
    )
    .unwrap();
    assert_eq!(inner_reply.data, b"aevtansr");
    assert_ne!(loaded.memory.read_u32_be(scratch + 0x314), Some(0));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_can_complete_nested_wait_with_68k_filter() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    loaded
        .memory
        .write_u32_be(scratch + 0x300, PPC_CORE_EVENT_CLASS)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x304, 0).unwrap();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x300,
        event,
    );
    let handler = scratch + 0x400;
    loaded
        .memory
        .write_u32_be(handler, d_form_u(32, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 4, d_form_u(14, 6, 6, 1))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 8, d_form_u(36, 6, 14, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 12, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler + 16, 0x4e80_0020)
        .unwrap();
    loaded.memory.write_u32_be(scratch + 0x88, handler).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let entry = scratch + 0x200;
    let mut code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 15, 0),
        d_form_u(14, 4, 15, 16),
        d_form_u(14, 5, 0, 3),
        d_form_u(14, 6, 0, 1),
        d_form_u(14, 7, 0, (-2i16) as u16),
        d_form_u(14, 8, 0, 0),
        d_form_u(14, 9, 16, 0),
    ];
    let call_pc = entry + code.len() as u32 * 4;
    code.push(0x4800_0001 | (PPC_IMPORT_TRAP_BASE.wrapping_sub(call_pc) & 0x03ff_fffc));
    code.extend([
        d_form_u(36, 3, 14, 4),
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ]);
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    for (index, word) in [0x1f7cu16, 0x0001, 0x0014, 0x4e74, 0x0010]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u16_be(scratch + 0x500 + index as u32 * 2, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..6].copy_from_slice(&[
        scratch + 0x500,
        0x0000_3fd0,
        PPC_ROUTINE_RECORD_M68K_ISA as u32,
    ]);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRoutineDescriptor);
    loaded.cpu.gpr[16] = loaded.cpu.gpr[3];
    loaded.cpu.gpr[14] = scratch + 0x120;
    loaded.cpu.gpr[15] = scratch + 0x300;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(512).unsupported_import_index,
        None
    );
    for _ in 0..10 {
        drain_test_m68k_guest_calls(&mut loaded);
        if loaded.cpu.pc == PPC_HALT_PC {
            break;
        }
        assert_eq!(
            loaded.run_with_hle_imports(512).unsupported_import_index,
            None
        );
    }
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x120), Some(2));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x124),
        Some(ppc_i16_result(PPC_NO_ERR))
    );
    let inner_reply = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 0x310,
    )
    .unwrap();
    assert_eq!(inner_reply.data, b"aevtansr");
    assert_ne!(loaded.memory.read_u32_be(scratch + 0x314), Some(0));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_handles_candidate_removed_during_callback() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    loaded
        .memory
        .write_u32_be(entry, d_form_u(14, 3, 0, 1))
        .unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(1).unsupported_import_index,
        None
    );
    assert_eq!(loaded.cpu.pc, entry);
    loaded.event_queue().with_mut(|queue| queue.clear());
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.queued_self_events.is_empty());
    let reply = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 16,
    )
    .unwrap();
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
    );
}

#[test]
fn queued_wait_filter_guest_allocation_failure_preserves_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    loaded.memory.write_u32_be(entry, 0x4e80_0020).unwrap();
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    loaded.set_heap_cursor(loaded.heap_limit());
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert_eq!(loaded.event_queue().len(), 1);
    assert_eq!(
        loaded.apple_events.queued_self_events[0].2,
        PpcSelfReplyRoute::Abandoned
    );
    assert!(loaded.apple_events.completed_wait_replies.is_empty());
    assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
}

#[test]
fn queued_wait_filter_scan_reservation_failure_is_atomic() {
    for stage in [21, 22] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let entry = scratch + 0x200;
        loaded.memory.write_u32_be(entry, 0x4e80_0020).unwrap();
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            u32::MAX - 1,
            0,
            scratch + 0x80,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        if stage == 21 {
            loaded.apple_events.fail_self_queue_reservation = stage;
        }
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        if stage == 22 {
            assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
            loaded.apple_events.fail_self_queue_reservation = stage;
        }

        if stage == 22 {
            assert_eq!(
                loaded.run_with_hle_imports(256).unsupported_import_index,
                None
            );
        }
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        assert!(loaded.apple_events.waiting_sends.is_empty());
        assert_eq!(loaded.event_queue().len(), usize::from(stage == 22));
        if stage == 22 {
            assert_eq!(
                loaded.apple_events.queued_self_events[0].2,
                PpcSelfReplyRoute::Abandoned
            );
        } else {
            assert!(loaded.apple_events.queued_self_events.is_empty());
        }
        assert!(loaded.apple_events.filter_seen.is_empty());
        assert!(loaded.apple_events.completed_wait_replies.is_empty());
        assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
    }
}

#[test]
fn queued_wait_filter_visits_unretained_high_level_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.event_queue().with_mut(|queue| {
        queue.push_back(PpcQueuedEvent {
            delivery_id: None,
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"EXTN"),
            when: 42,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        })
    });
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(u32::from_be_bytes(*b"EXTN"))
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x104),
        Some(u32::from_be_bytes(*b"TEST"))
    );
    assert_eq!(loaded.event_queue().len(), 2);
}

#[test]
fn queued_wait_filter_receives_pending_launch_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded
        .apple_events
        .apple_event_launch_state
        .set_high_level_event_aware(true);
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 7, 6, 0),
        d_form_u(36, 7, 15, 0),
        d_form_u(32, 7, 6, 4),
        d_form_u(32, 7, 7, 0),
        d_form_u(32, 7, 7, 0),
        d_form_u(36, 7, 15, 4),
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[15] = scratch + 0x110;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(PPC_CORE_EVENT_CLASS)
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x110),
        Some(u32::from_be_bytes(*b"sign"))
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x114),
        Some(u32::from_be_bytes(*b"MACS"))
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x104),
        Some(u32::from_be_bytes(*b"TEST"))
    );
    assert_eq!(loaded.event_queue().len(), 2);
}

#[test]
fn queued_wait_filter_dispatches_accepted_unretained_high_level_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.event_queue().with_mut(|queue| {
        queue.push_back(PpcQueuedEvent {
            delivery_id: None,
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"EXTN"),
            when: 42,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        })
    });
    let handler_entry = scratch + 0x300;
    loaded
        .memory
        .write_u32_be(handler_entry, d_form_u(14, 6, 0, 77))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 4, d_form_u(36, 6, 15, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 8, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 12, 0x4e80_0020)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x88, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"EXTN"),
        0x0001_0002,
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[15] = scratch + 0x110;
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(u32::from_be_bytes(*b"EXTN"))
    );
    for _ in 0..8 {
        loaded.run_with_hle_imports(256);
        if loaded.apple_events.waiting_sends.is_empty() {
            break;
        }
    }
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x110), Some(77));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_external_dispatch_failure_restores_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.event_queue().with_mut(|queue| {
        queue.push_back(PpcQueuedEvent {
            delivery_id: None,
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"EXTN"),
            when: 42,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        })
    });
    let handler_entry = scratch + 0x300;
    loaded
        .memory
        .write_u32_be(handler_entry, d_form_u(14, 6, 0, 77))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 4, d_form_u(36, 6, 15, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 8, d_form_u(14, 3, 0, 0))
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry + 12, 0x4e80_0020)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x88, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x8c, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"EXTN"),
        0x0001_0002,
        scratch + 0x88,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[15] = scratch + 0x110;
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    loaded.apple_events.fail_self_queue_reservation = 15;
    for _ in 0..8 {
        loaded.run_with_hle_imports(256);
        if loaded.apple_events.waiting_sends.is_empty() {
            break;
        }
    }
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.filter_seen.is_empty());
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 2);
    assert_eq!(
        loaded.event_queue().with_mut(|queue| queue[0].message),
        u32::from_be_bytes(*b"EXTN")
    );
    assert_eq!(
        loaded.apple_events.queued_self_events[0].2,
        PpcSelfReplyRoute::Abandoned
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x110), Some(0));
}

#[test]
fn queued_wait_filter_ignores_unhandled_external_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.event_queue().with_mut(|queue| {
        queue.push_back(PpcQueuedEvent {
            delivery_id: None,
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"EXTN"),
            when: 42,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        })
    });
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(u32::from_be_bytes(*b"EXTN"))
    );
    for _ in 0..8 {
        loaded.run_with_hle_imports(256);
        if loaded.apple_events.waiting_sends.is_empty() {
            break;
        }
    }
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let reply = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 16,
    )
    .unwrap();
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
    );
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_external_allocation_failure_preserves_event() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.event_queue().with_mut(|queue| {
        queue.push_back(PpcQueuedEvent {
            delivery_id: None,
            what: PPC_HIGH_LEVEL_EVENT,
            message: u32::from_be_bytes(*b"EXTN"),
            when: 42,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        })
    });
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 1),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    loaded.apple_events.fail_self_queue_reservation = 23;
    for _ in 0..8 {
        loaded.run_with_hle_imports(256);
        if loaded.apple_events.waiting_sends.is_empty() {
            break;
        }
    }
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.filter_seen.is_empty());
    assert_eq!(loaded.event_queue().len(), 2);
    assert_eq!(
        loaded.event_queue().with_mut(|queue| queue[0].message),
        u32::from_be_bytes(*b"EXTN")
    );
    assert_eq!(
        loaded.apple_events.queued_self_events[0].2,
        PpcSelfReplyRoute::Abandoned
    );
}

#[test]
fn queued_wait_filter_scans_high_priority_events_in_queue_order() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    event.data[..4].copy_from_slice(b"PREV");
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    event.data[..4].copy_from_slice(b"TEST");
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        1,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(loaded.event_queue().len(), 2);
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(u32::from_be_bytes(*b"TEST"))
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x104),
        Some(u32::from_be_bytes(*b"PREV"))
    );
    assert_eq!(loaded.event_queue().len(), 2);
    assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
}

#[test]
fn queued_wait_filter_continues_scan_after_candidate_is_removed() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    event.data[..4].copy_from_slice(b"PREV");
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    event.data[..4].copy_from_slice(b"NEXT");
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event.clone(),
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 1]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    event.data[..4].copy_from_slice(b"TEST");
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    let code = [
        d_form_u(32, 6, 3, 2),
        d_form_u(36, 6, 14, 0),
        d_form_u(14, 14, 14, 4),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[14] = scratch + 0x100;
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        1,
        u32::MAX - 1,
        0,
        scratch + 0x80,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(loaded.event_queue().len(), 3);
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x100),
        Some(u32::from_be_bytes(*b"TEST"))
    );
    assert_eq!(
        loaded.run_with_hle_imports(1).unsupported_import_index,
        None
    );
    assert_eq!(loaded.cpu.pc, entry);
    loaded.event_queue().with_mut(|queue| {
        let position = queue
            .iter()
            .position(|event| event.message == u32::from_be_bytes(*b"PREV"))
            .unwrap();
        queue.remove(position);
    });
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x104),
        Some(u32::from_be_bytes(*b"PREV"))
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 0x108),
        Some(u32::from_be_bytes(*b"NEXT"))
    );
    assert_eq!(loaded.event_queue().len(), 2);
    assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
}

#[test]
fn queued_wait_68k_filter_accepts_event_and_completes_send() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    for (index, word) in [0x1f7cu16, 0x0001, 0x0014, 0x4e74, 0x0010]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u16_be(entry + index as u32 * 2, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..6].copy_from_slice(&[entry, 0x0000_3fd0, PPC_ROUTINE_RECORD_M68K_ISA as u32]);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRoutineDescriptor);
    let filter = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        0,
        filter,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert_eq!(loaded.event_queue().len(), 0);
}

#[test]
fn queued_wait_filter_inspects_event_and_dispatches_only_when_accepted() {
    for (accepted, handler) in [(false, false), (true, false), (true, true)] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        if handler {
            let entry = scratch + 0x400;
            loaded
                .memory
                .write_u32_be(entry, d_form_u(14, 3, 0, 0))
                .unwrap();
            loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
            loaded.memory.write_u32_be(scratch + 0x88, entry).unwrap();
            loaded
                .memory
                .write_u32_be(scratch + 0x8c, loaded.rtoc)
                .unwrap();
            loaded.cpu.gpr[3..8].copy_from_slice(&[
                u32::from_be_bytes(*b"TEST"),
                u32::from_be_bytes(*b"send"),
                scratch + 0x88,
                0,
                0,
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AEInstallEventHandler,
            );
        }
        let entry = scratch + 0x200;
        let code = [
            d_form_u(36, 4, 14, 0),
            d_form_u(36, 5, 14, 4),
            d_form_u(32, 7, 6, 0),
            d_form_u(36, 7, 14, 8),
            d_form_u(14, 3, 0, u16::from(accepted)),
            0x4e80_0020,
        ];
        for (index, word) in code.into_iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[14] = scratch + 0x100;
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            u32::MAX - 1,
            0,
            scratch + 0x80,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        assert_eq!(
            loaded.run_with_hle_imports(256).unsupported_import_index,
            None
        );
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(17));
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x104), Some(42));
        assert_eq!(
            loaded.memory.read_u32_be(scratch + 0x108),
            Some(u32::from_be_bytes(*b"psn "))
        );
        if accepted {
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert!(loaded.apple_events.waiting_sends.is_empty());
            assert!(loaded.apple_events.filter_seen.is_empty());
            assert_eq!(loaded.event_queue().len(), 0);
            let reply = ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                scratch + 16,
            )
            .unwrap();
            if handler {
                assert!(!reply.fields.contains_key(&u32::from_be_bytes(*b"errn")));
            } else {
                assert_eq!(
                    reply.fields[&u32::from_be_bytes(*b"errn")].data,
                    i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
                );
            }
        } else {
            assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
            assert_eq!(loaded.event_queue().len(), 1);
            assert_eq!(loaded.apple_events.queued_self_events.len(), 1);
        }
    }
}

#[test]
fn queued_wait_idle_mouse_region_wakes_on_outside_position() {
    for region_kind in 0..=2 {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        loaded.set_clock_cycle_timing(100_000, 0);
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let region = scratch + 0x700;
        loaded.memory.write_u32_be(region, scratch + 0x710).unwrap();
        loaded.memory.write_u16_be(scratch + 0x710, 10).unwrap();
        loaded.memory.write_u16_be(scratch + 0x712, 0).unwrap();
        loaded.memory.write_u16_be(scratch + 0x714, 0).unwrap();
        loaded
            .memory
            .write_u16_be(scratch + 0x716, if region_kind == 2 { 0 } else { 10 })
            .unwrap();
        loaded
            .memory
            .write_u16_be(scratch + 0x718, if region_kind == 2 { 0 } else { 10 })
            .unwrap();
        let entry = scratch + 0x200;
        let code = [
            d_form_u(40, 6, 3, 0),
            d_form_u(36, 6, 14, 0),
            d_form_u(32, 6, 3, 2),
            d_form_u(36, 6, 14, 4),
            d_form_u(32, 6, 3, 10),
            d_form_u(36, 6, 14, 8),
            d_form_u(36, 15, 5, 0),
            d_form_u(14, 6, 0, 10_000),
            d_form_u(36, 6, 4, 0),
            d_form_u(14, 3, 0, 0),
            0x4e80_0020,
        ];
        for (index, word) in code.into_iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[14] = scratch + 0x100;
        loaded.cpu.gpr[15] = if region_kind == 0 { 0 } else { region };
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_v: 5,
            mouse_h: 5,
            ..Default::default()
        });
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            u32::MAX - 1,
            scratch + 0x80,
            0,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        assert_eq!(
            loaded.run_with_hle_imports(256).unsupported_import_index,
            None
        );
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(0));
        loaded.memory.write_u32_be(scratch + 0x100, 99).unwrap();
        assert_eq!(
            loaded.run_with_hle_imports(64).unsupported_import_index,
            None
        );
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(99));
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_v: 20,
            mouse_h: 30,
            ..Default::default()
        });
        assert_eq!(
            loaded.run_with_hle_imports(256).unsupported_import_index,
            None
        );
        if region_kind == 1 {
            assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(15));
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 0x104),
                Some(0xfa00_0000)
            );
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 0x108),
                Some((20 << 16) | 30)
            );
        } else {
            assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(99));
        }
        assert_eq!(loaded.event_queue().len(), 1);
        assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
    }
}

#[test]
fn queued_wait_accepts_68k_idle_callback_and_boolean_result() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    for (index, word) in [0x1f7cu16, 0x0001, 0x0010, 0x4e74, 0x000c]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u16_be(entry + index as u32 * 2, word)
            .unwrap();
    }
    loaded.cpu.gpr[3..6].copy_from_slice(&[entry, 0x0000_0fd0, PPC_ROUTINE_RECORD_M68K_ISA as u32]);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRoutineDescriptor);
    let idle = loaded.cpu.gpr[3];
    assert_ne!(idle, 0);
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        scratch + 8,
        scratch + 16,
        3,
        0,
        u32::MAX - 1,
        idle,
        0,
    ]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    assert_eq!(
        loaded.run_with_hle_imports(64).unsupported_import_index,
        None
    );
    assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
    assert_eq!(
        loaded.run_with_hle_imports(256).unsupported_import_index,
        None
    );
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1711));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert_eq!(
        loaded.apple_events.queued_self_events[0].2,
        PpcSelfReplyRoute::Abandoned
    );
}

#[test]
fn queued_wait_idle_callback_receives_event_and_can_cancel() {
    for cancel in [false, true] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        loaded.set_clock_cycle_timing(100_000, 0);
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let entry = scratch + 0x200;
        loaded
            .memory
            .write_u32_be(entry, d_form_u(40, 6, 3, 0))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 4, d_form_u(36, 6, 14, 0))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 8, d_form_u(14, 7, 0, 10_000))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 12, d_form_u(36, 7, 4, 0))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 16, d_form_u(14, 7, 0, 42))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 20, d_form_u(36, 7, 5, 0))
            .unwrap();
        loaded
            .memory
            .write_u32_be(entry + 24, d_form_u(14, 3, 0, u16::from(cancel)))
            .unwrap();
        loaded.memory.write_u32_be(entry + 28, 0x4e80_0020).unwrap();
        loaded.cpu.gpr[14] = scratch + 0x100;
        loaded.event_queue().with_mut(|queue| {
            queue.push_back(PpcQueuedEvent {
                delivery_id: None,
                what: 3,
                message: 0x1234,
                when: 1,
                where_v: 2,
                where_h: 3,
                modifiers: 4,
            })
        });
        loaded.event_queue().with_mut(|queue| {
            queue.push_back(PpcQueuedEvent {
                delivery_id: None,
                what: 6,
                message: 0,
                when: 2,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });
        });
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            u32::MAX - 1,
            scratch + 0x80,
            0,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        let first = loaded.run_with_hle_imports(64);
        assert_eq!(first.unsupported_import_index, None);
        assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
        let second = loaded.run_with_hle_imports(256);
        assert_eq!(second.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(0));
        assert_eq!(loaded.event_queue().len(), 3);
        if cancel {
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1711));
            assert!(loaded.apple_events.waiting_sends.is_empty());
            assert_eq!(
                loaded.apple_events.queued_self_events[0].2,
                PpcSelfReplyRoute::Abandoned
            );
        } else {
            let wait = *loaded.apple_events.waiting_sends.values().next().unwrap();
            assert_eq!(wait.idle_sleep, 10_000);
            assert_eq!(wait.idle_mouse_region, 42);
            assert_eq!(
                loaded.run_with_hle_imports(256).unsupported_import_index,
                None
            );
            assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(6));
            assert_eq!(loaded.event_queue().len(), 2);
            loaded.memory.write_u32_be(scratch + 0x100, 99).unwrap();
            assert_eq!(
                loaded.run_with_hle_imports(64).unsupported_import_index,
                None
            );
            assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(99));
            assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
            assert!(loaded
                .apple_events
                .waiting_sends
                .values()
                .all(|wait| wait.idle_scratch == 0));
        }
    }
}

#[test]
fn queued_wait_send_switches_to_worker_and_resumes_with_reply() {
    use crate::guest_call::{ExecutionTaskId, NativeThreadContext, ThreadStorage};
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    for (index, target) in [
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        PpcImportDispatcherTarget::AEProcessAppleEvent,
        PpcImportDispatcherTarget::YieldToThread,
    ]
    .into_iter()
    .enumerate()
    {
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = index as u32 + 1;
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4 * (index as u32 + 1);
        binding.dispatcher_target = target;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
    }
    loaded.import_count = 4;
    let entry = scratch + 0x200;
    let mut code = Vec::new();
    code.push(d_form_u(14, 3, 0, PPC_HIGH_LEVEL_EVENT_MASK));
    code.push(d_form_u(14, 4, 14, 0));
    let call = |index: u32, instruction: usize| {
        0x4800_0001
            | ((PPC_IMPORT_TRAP_BASE + index * 4).wrapping_sub(entry + instruction as u32 * 4)
                & 0x03ff_fffc)
    };
    code.push(call(1, code.len()));
    code.push(d_form_u(14, 3, 14, 0));
    code.push(call(2, code.len()));
    code.push(d_form_u(
        14,
        3,
        0,
        ExecutionTaskId::APPLICATION.thread_id() as u16,
    ));
    code.push(call(3, code.len()));
    code.push(0x4e80_0020);
    for (index, word) in code.into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, word)
            .unwrap();
    }
    let mut worker_cpu = loaded.cpu.clone();
    worker_cpu.pc = entry;
    worker_cpu.gpr[14] = scratch + 0x90;
    worker_cpu.lr = PPC_HALT_PC;
    let calls = loaded.guest_calls().shared_handle();
    let worker = calls
        .create_native_thread(
            NativeThreadContext {
                context: worker_cpu.capture_execution_context(),
            },
            ThreadStorage::default(),
            false,
            |_| true,
        )
        .unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[scratch + 8, scratch + 16, 3, 0, u32::MAX - 1, 0, 0]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
        PpcAppleEventCompatibilityOperation::Send,
    );
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(calls.current_task(), worker);
    assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.apple_events.waiting_sends.is_empty());
    assert!(loaded.apple_events.completed_wait_replies.is_empty());
    let reply = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 16,
    )
    .unwrap();
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
    );
}

#[test]
fn queued_wait_send_reenters_without_reposting_and_completes_or_times_out() {
    for timeout in [false, true] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            scratch + 8,
            scratch + 16,
            3,
            0,
            if timeout { 0 } else { u32::MAX - 1 },
            0,
            0,
        ]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        let first = loaded.run_with_hle_imports(64);
        assert_eq!(first.unsupported_import_index, None);
        assert_eq!(loaded.event_queue().len(), 1);
        assert_eq!(loaded.apple_events.waiting_sends.len(), 1);
        let id = loaded.apple_events.queued_self_events[0]
            .0
            .delivery_id
            .unwrap();
        if !timeout {
            let waiting_cpu = loaded.cpu.clone();
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
            );
            loaded.cpu.gpr[3] = scratch + 0x90;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
            assert_eq!(
                loaded.cpu.gpr[3],
                ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED)
            );
            assert_eq!(
                loaded
                    .apple_events
                    .completed_wait_replies
                    .front()
                    .unwrap()
                    .0,
                id
            );
            loaded.cpu = waiting_cpu;
            loaded.imports[0].dispatcher_target =
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::Send,
                );
        }
        let second = loaded.run_with_hle_imports(64);
        assert_eq!(second.unsupported_import_index, None);
        assert_eq!(loaded.event_queue().len(), usize::from(timeout));
        assert!(loaded.apple_events.waiting_sends.is_empty());
        if timeout {
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1712));
            assert_eq!(
                loaded.apple_events.queued_self_events[0].2,
                PpcSelfReplyRoute::Abandoned
            );
            let entry = scratch + 0x200;
            loaded
                .memory
                .write_u32_be(entry, d_form_u(14, 3, 0, 0))
                .unwrap();
            loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
            loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
            loaded
                .memory
                .write_u32_be(scratch + 0x84, loaded.rtoc)
                .unwrap();
            loaded.cpu.gpr[3..8].copy_from_slice(&[
                u32::from_be_bytes(*b"TEST"),
                u32::from_be_bytes(*b"send"),
                scratch + 0x80,
                0,
                0,
            ]);
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::AEInstallEventHandler,
            );
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
            );
            loaded.cpu.gpr[3] = scratch + 0x90;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert!(loaded.apple_events.completed_wait_replies.is_empty());
            assert!(loaded.apple_events.completed_self_replies.is_empty());
            assert!(loaded.apple_events.queued_self_events.is_empty());
            assert_eq!(loaded.event_queue().len(), 0);
            assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
        } else {
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            let reply = ppc_ae_descriptor(
                &mut loaded.memory,
                &loaded.apple_events.descriptors,
                scratch + 16,
            )
            .unwrap();
            assert_eq!(reply.data, b"aevtansr");
        }
    }
}

#[test]
fn waiting_reply_materializes_sender_owned_event_and_handler_error() {
    for dispatch_error in [false, true] {
        let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
        let mut context = ProcessContext::default();
        native.attach_unconverted_process_services(&mut context);
        let output = PPC_DATA_BASE + 0x2800;
        native.memory.add_region(output, vec![0; 8]);
        let return_id = u32::from_be_bytes(*b"rtid");
        let mut descriptor = ProcessAeDescriptor {
            desc_type: PPC_CORE_EVENT_CLASS,
            data: b"aevtansr".to_vec(),
            ..Default::default()
        };
        descriptor.attributes.insert(
            return_id,
            ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"shor"),
                data: 17i16.to_be_bytes().to_vec(),
                ..Default::default()
            },
        );
        let pending = if dispatch_error {
            PpcPendingSelfReply::Error(descriptor, PPC_MEM_FULL_ERR)
        } else {
            PpcPendingSelfReply::Completed(descriptor, PPC_MEM_FULL_ERR)
        };
        let mut state = PpcAppleEventState::default();
        let mut heap_cursor = native.heap_cursor();
        let mut last_mem_error = native.last_mem_error();
        let mut handles = native.handles();
        let result = {
            let mut manager = context.memory_manager_mut();
            dispatch_apple_events::send::complete_wait_reply(
                pending,
                output,
                &mut manager,
                &mut native.memory,
                &mut heap_cursor,
                &mut last_mem_error,
                &mut handles,
                &mut state,
            )
        };
        assert_eq!(result, PPC_NO_ERR);
        let handle = native.memory.read_u32_be(output + 4).unwrap();
        assert_ne!(handle, 0);
        assert!(context
            .memory_manager_mut()
            .native_allocation(handle)
            .is_some());
        let reply = ppc_ae_descriptor(&mut native.memory, &state.descriptors, output).unwrap();
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(reply.attributes[&return_id].data, 17i16.to_be_bytes());
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            i32::from(PPC_MEM_FULL_ERR).to_be_bytes()
        );
    }
}

#[test]
fn waiting_self_reply_routes_handler_error_to_its_waiter() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.apple_events.queued_self_events[0].2 = PpcSelfReplyRoute::Waiting(42);
    loaded.apple_events.completed_wait_replies.reserve(1);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED)
    );
    assert_eq!(loaded.event_queue().len(), 0);
    assert!(loaded.apple_events.completed_self_replies.is_empty());
    assert!(loaded.apple_events.take_wait_reply(99).is_none());
    let reply = loaded.apple_events.take_wait_reply(42).unwrap();
    let PpcPendingSelfReply::Error(request, error) = reply else {
        panic!("expected an error reply");
    };
    assert_eq!(error, PPC_ERR_AE_EVENT_NOT_HANDLED);
    assert_eq!(
        request.attributes[&u32::from_be_bytes(*b"rtid")].data,
        17i16.to_be_bytes()
    );
}

#[test]
fn waiting_self_reply_retrieves_handler_result_by_identity() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    loaded
        .memory
        .write_u32_be(entry, d_form_u(14, 3, 0, (-50i16) as u16))
        .unwrap();
    loaded.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap();
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        scratch + 0x80,
        0,
        0,
    ]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.apple_events.queued_self_events[0].2 = PpcSelfReplyRoute::Waiting(42);
    loaded.apple_events.completed_wait_replies.reserve(1);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    loaded.cpu.gpr[3] = scratch + 0x90;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-50));
    assert_eq!(loaded.event_queue().len(), 0);
    assert!(loaded.apple_events.take_wait_reply(99).is_none());
    let PpcPendingSelfReply::Completed(reply, error) =
        loaded.apple_events.take_wait_reply(42).unwrap()
    else {
        panic!("expected the handler reply");
    };
    assert_eq!(error, -50);
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.attributes[&u32::from_be_bytes(*b"rtid")].data,
        17i16.to_be_bytes()
    );
    assert!(loaded.apple_events.take_wait_reply(42).is_none());
}

#[test]
fn aesend_queued_reply_reports_missing_handler() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED)
    );
    assert_eq!(loaded.event_queue().len(), 1);
    let reply = &loaded.apple_events.queued_self_events[0].1;
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
    );
}

#[test]
fn aesend_queued_reply_reports_discarded_or_unprocessed_request() {
    for discard_before_retrieval in [false, true] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        loaded.cpu.gpr[3..6].copy_from_slice(&[scratch + 8, scratch + 16, 2]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        if discard_before_retrieval {
            loaded.event_queue().with_mut(|queue| {
                queue.pop_front();
            });
        } else {
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
            );
            assert_eq!(loaded.cpu.gpr[3], 1);
            assert!(loaded.apple_events.current_self_event.is_some());
            loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
            loaded.cpu.gpr[4] = scratch + 0x90;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::EventAvail);
            assert!(loaded.apple_events.current_self_event.is_some());
        }
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        loaded.cpu.gpr[5] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        let reply = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
        );
        assert!(loaded.apple_events.queued_self_events.is_empty());
    }
}

#[test]
fn aesend_queued_reply_survives_classic_handler_return() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let entry = scratch + 0x200;
    for (index, word) in [0x3f7cu16, 0xffce, 0x0010, 0x4e74, 0x000c]
        .into_iter()
        .enumerate()
    {
        loaded
            .memory
            .write_u16_be(entry + index as u32 * 2, word)
            .unwrap();
    }
    loaded.apple_events.handlers.install(
        false,
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        ProcessAppleEventHandler {
            procedure: crate::guest_procedure::GuestProcedure::raw_m68k(entry),
            refcon: 0,
        },
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, scratch + 16, 2, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert!(
        loaded.apple_events.pending_dispatches.is_empty(),
        "pc={:08x}, result={:08x}",
        loaded.cpu.pc,
        loaded.cpu.gpr[3]
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-50));
    assert_eq!(loaded.event_queue().len(), 1);
    let reply = &loaded.apple_events.queued_self_events[0].1;
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        (-50i32).to_be_bytes()
    );
}

#[test]
fn aesend_queued_reply_reports_guest_heap_exhaustion() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let procedure = resolve_guest_procedure(
        &mut loaded.memory,
        PPC_CODE_BASE,
        loaded.rtoc,
        None,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    )
    .unwrap();
    loaded.apple_events.handlers.install(
        false,
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        ProcessAppleEventHandler {
            procedure,
            refcon: 0,
        },
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, scratch + 16, 2, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    loaded.set_heap_cursor(loaded.heap_limit().saturating_sub(8));
    let handles = loaded.handles();
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.handles(), handles);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 1);
    let reply = &loaded.apple_events.queued_self_events[0].1;
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_MEM_FULL_ERR).to_be_bytes()
    );
}

#[test]
fn aesend_queued_reply_reports_failed_classic_handler_setup() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    let mut procedure = crate::guest_procedure::GuestProcedure::raw_m68k(scratch + 0x200);
    procedure.proc_info = 0xffff_ffff;
    loaded.apple_events.handlers.install(
        false,
        u32::from_be_bytes(*b"TEST"),
        u32::from_be_bytes(*b"send"),
        ProcessAppleEventHandler {
            procedure,
            refcon: 0,
        },
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, scratch + 16, 2, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
    loaded.cpu.gpr[4] = scratch + 0x90;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    let mixed_mode = loaded.toolbox_startup.mixed_mode_m68k.snapshot();
    loaded.cpu.gpr[3] = scratch + 0x90;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEProcessAppleEvent);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert_eq!(
        loaded.toolbox_startup.mixed_mode_m68k.snapshot(),
        mixed_mode
    );
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.event_queue().len(), 1);
    let reply = &loaded.apple_events.queued_self_events[0].1;
    assert_eq!(reply.data, b"aevtansr");
    assert_eq!(
        reply.fields[&u32::from_be_bytes(*b"errn")].data,
        i32::from(PPC_MEM_FULL_ERR).to_be_bytes()
    );
}

#[test]
fn aesend_new_request_has_distinct_identity_from_flushed_error_reply() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, scratch + 16, 2, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.event_queue().with_mut(|queue| {
        queue.pop_front();
    });
    loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, 0, 1, 0]);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.event_queue().len(), 2);
    let identities: Vec<_> = loaded.event_queue().with_ref(|queue| {
        queue
            .iter()
            .map(|event| event.delivery_id.unwrap())
            .collect()
    });
    assert_ne!(identities[0], identities[1]);
    for expected in [b"aevtansr".as_slice(), b"TESTsend".as_slice()] {
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        assert_eq!(
            loaded
                .apple_events
                .current_self_event
                .as_ref()
                .unwrap()
                .1
                .data,
            expected
        );
    }
    assert!(loaded.apple_events.queued_self_events.is_empty());
}

#[test]
fn queued_suspended_event_emits_reply_only_after_resume() {
    for (waiting, redispatch, abandon, classic, resuspend) in [
        (false, false, false, false, false),
        (true, false, false, false, false),
        (false, true, false, false, false),
        (true, true, false, false, false),
        (false, true, false, true, false),
        (true, true, false, true, false),
        (false, true, false, false, true),
        (true, true, false, false, true),
        (false, false, true, false, false),
        (true, false, true, false, false),
    ] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let mut event = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
        )
        .unwrap();
        event
            .attributes
            .get_mut(&u32::from_be_bytes(*b"addr"))
            .unwrap()
            .data = [0, 0, 0, 0, 0, 0, 1, 0].to_vec();
        ppc_store_ae_descriptor_semantics(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 8,
            event,
        );
        let entry = scratch + 0x200;
        let observed = scratch + 0x100;
        let mut binding = loaded.imports[0].clone();
        binding.symbol_index = 1;
        binding.symbol_name = "AESuspendTheCurrentEvent".into();
        binding.dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", &binding.symbol_name);
        binding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
        loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
        loaded.imports.push(binding);
        loaded.import_count = 2;
        let mut code = vec![
            0x7c08_02a6,
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(32, 6, 3, 0),
            d_form_u(36, 6, 5, 0),
            d_form_u(32, 6, 3, 4),
            d_form_u(36, 6, 5, 4),
            d_form_u(32, 6, 4, 0),
            d_form_u(36, 6, 5, 8),
            d_form_u(32, 6, 4, 4),
            d_form_u(36, 6, 5, 12),
        ];
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, 0),
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[
            u32::from_be_bytes(*b"TEST"),
            u32::from_be_bytes(*b"send"),
            scratch + 0x80,
            observed,
            0,
        ]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        loaded.cpu.gpr[3..7].copy_from_slice(&[scratch + 8, scratch + 16, 2, 0]);
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        if waiting {
            loaded.apple_events.queued_self_events[0].2 = PpcSelfReplyRoute::Waiting(42);
            loaded.apple_events.completed_wait_replies.reserve(1);
        }
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        loaded.cpu.gpr[3] = scratch + 0x90;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(probe.handled_import_count, 2);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
        assert!(loaded.apple_events.completed_wait_replies.is_empty());
        assert!(loaded.apple_events.completed_self_replies.is_empty());
        assert!(loaded.event_queue().is_empty());
        if abandon {
            for descriptor in [observed + 8, observed] {
                loaded.cpu.gpr[3] = descriptor;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::AppleEventCompatibility(
                        PpcAppleEventCompatibilityOperation::DisposeDesc,
                    ),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            }
            assert!(loaded.apple_events.suspended_dispatches.is_empty());
            if waiting {
                let Some(PpcPendingSelfReply::Error(_, error)) =
                    loaded.apple_events.take_wait_reply(42)
                else {
                    panic!("missing abandoned waiting reply");
                };
                assert_eq!(error, PPC_ERR_AE_EVENT_NOT_HANDLED);
            } else {
                loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
                loaded.cpu.gpr[4] = scratch + 0x90;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
                );
                assert_eq!(loaded.cpu.gpr[3], 1);
                let answer = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
                assert_eq!(
                    answer.fields[&u32::from_be_bytes(*b"errn")].data,
                    i32::from(PPC_ERR_AE_EVENT_NOT_HANDLED).to_be_bytes()
                );
            }
            continue;
        }
        let dispatcher = if resuspend {
            u32::MAX
        } else if classic {
            let callback = scratch + 0x1c0;
            for (offset, word) in [0x3f7cu16, 0xffce, 0x0010, 0x4e74, 0x000c]
                .into_iter()
                .enumerate()
            {
                loaded
                    .memory
                    .write_u16_be(callback + offset as u32 * 2, word)
                    .unwrap();
            }
            loaded.apple_events.handlers.install(
                false,
                u32::from_be_bytes(*b"TEST"),
                u32::from_be_bytes(*b"send"),
                ProcessAppleEventHandler {
                    procedure: crate::guest_procedure::GuestProcedure::raw_m68k(callback),
                    refcon: 0,
                },
            );
            u32::MAX
        } else if redispatch {
            let vector = scratch + 0x180;
            let callback = scratch + 0x1c0;
            loaded.memory.write_u32_be(vector, callback).unwrap();
            loaded.memory.write_u32_be(vector + 4, loaded.rtoc).unwrap();
            loaded.memory.write_u32_be(callback, 0x3860_ffce).unwrap();
            loaded
                .memory
                .write_u32_be(callback + 4, 0x4e80_0020)
                .unwrap();
            vector
        } else {
            0
        };
        loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, dispatcher, 0]);
        if resuspend {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.imports[0].dispatcher_target =
                PpcImportDispatcherTarget::AppleEventCompatibility(
                    PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
                );
            let probe = loaded.run_with_hle_imports(256);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(probe.handled_import_count, 2);
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
            assert_eq!(loaded.apple_events.suspended_dispatches.len(), 1);
            assert!(loaded.apple_events.completed_self_replies.is_empty());
            assert!(loaded.apple_events.completed_wait_replies.is_empty());
            loaded.cpu.gpr[3..7].copy_from_slice(&[observed, observed + 8, 0, 0]);
        }
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::ResumeTheCurrentEvent,
            ),
        );
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(if redispatch && !resuspend {
                -50
            } else {
                PPC_NO_ERR
            })
        );
        assert!(loaded.apple_events.suspended_dispatches.is_empty());
        if waiting {
            let Some(PpcPendingSelfReply::Completed(reply, error)) =
                loaded.apple_events.take_wait_reply(42)
            else {
                panic!("missing waiting reply");
            };
            assert_eq!(
                error,
                if redispatch && !resuspend {
                    -50
                } else {
                    PPC_NO_ERR
                }
            );
            assert_eq!(reply.data, b"aevtansr");
            assert!(loaded.event_queue().is_empty());
            continue;
        }
        loaded.cpu.gpr[3] = PPC_HIGH_LEVEL_EVENT_MASK as u32;
        loaded.cpu.gpr[4] = scratch + 0x90;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        assert_eq!(
            loaded
                .apple_events
                .current_self_event
                .as_ref()
                .unwrap()
                .1
                .data,
            b"aevtansr"
        );
        if redispatch && !resuspend {
            let answer = &loaded.apple_events.current_self_event.as_ref().unwrap().1;
            assert_eq!(
                answer.fields[&u32::from_be_bytes(*b"errn")].data,
                (-50i32).to_be_bytes()
            );
        }
    }
}
