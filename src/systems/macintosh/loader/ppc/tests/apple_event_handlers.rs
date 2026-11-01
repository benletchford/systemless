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
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEInstallSpecialHandler);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.apple_events.special_handlers.get(&(system, class)),
            Some(&handler),
        );
    }
    assert_eq!(loaded.apple_events.object_callbacks.get(&class), Some(&0x3000));
    assert_eq!(loaded.apple_events.special_handlers.get(&(true, class)), Some(&0x2000));
    assert_eq!(loaded.apple_events.special_handlers.get(&(false, class)), Some(&0x3000));
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
