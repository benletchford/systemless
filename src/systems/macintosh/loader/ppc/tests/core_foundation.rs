use super::*;

#[test]
fn carbon_cfbundle_lookup_returns_null_when_no_bundle_is_loaded() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFBundleGetBundleWithIdentifier"),
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(
        b"CFBundleGetBundleWithIdentifier",
    ))
    .unwrap();
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded
        .memory
        .write_bytes(source, b"com.apple.Carbon\0")
        .unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let identifier = loaded.cpu.gpr[3];
    assert_ne!(identifier, 0);

    loaded.cpu.gpr[3] = identifier;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.gpr[3] = identifier;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_constant_cfstring_is_interned_and_survives_balanced_retain_release() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "__CFStringMakeConstantString"),
        PpcImportDispatcherTarget::CfStringMakeConstantString
    );
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"__CFStringMakeConstantString")).unwrap();
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded.memory.write_bytes(source, b"Caf\x8e\0").unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let reference = loaded.cpu.gpr[3];
    assert_ne!(reference, 0);

    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    assert_eq!(loaded.cpu.gpr[3], reference);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetLength);
    assert_eq!(loaded.cpu.gpr[3], 4);

    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRetain);
    assert_eq!(loaded.cpu.gpr[3], reference);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 2);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = source + 16;
    loaded.cpu.gpr[5] = 5;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetCString);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        (0..5)
            .map(|offset| loaded.memory.read_u8(source + 16 + offset))
            .collect::<Option<Vec<_>>>(),
        Some(b"Caf\x8e\0".to_vec())
    );
}

#[test]
fn carbon_owned_cfstring_handles_utf8_capacity_and_release() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFStringCreateWithCString")).unwrap();
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 128]);
    loaded
        .memory
        .write_bytes(source, "A😀\0".as_bytes())
        .unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = source;
    loaded.cpu.gpr[5] = 0x0800_0100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringCreateWithCString,
    );
    let reference = loaded.cpu.gpr[3];
    assert_ne!(reference, 0);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetLength);
    assert_eq!(loaded.cpu.gpr[3], 3);

    loaded.memory.write_u8(source + 32, 0xA5).unwrap();
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = source + 32;
    loaded.cpu.gpr[5] = 5;
    loaded.cpu.gpr[6] = 0x0800_0100;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetCString);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(source + 32), Some(0xA5));
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[5] = 6;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetCString);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u8(source + 37), Some(0));

    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_pascal_cfstring_uses_system_encoding() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFStringGetSystemEncoding")).unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringGetSystemEncoding,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 32]);
    loaded.memory.write_bytes(source, b"\x03Mac").unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = source;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringCreateWithPascalString,
    );
    let reference = loaded.cpu.gpr[3];
    assert_ne!(reference, 0);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetLength);
    assert_eq!(loaded.cpu.gpr[3], 3);
}

#[test]
fn carbon_cfstring_bytes_obey_range_encoding_and_output_capacity() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFStringCreateWithBytes"),
        PpcImportDispatcherTarget::CfStringCreateWithBytes
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFStringGetBytes"),
        PpcImportDispatcherTarget::CfStringGetBytes
    );
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFStringCreateWithBytes")).unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 128]);
    loaded
        .memory
        .write_bytes(base, b"\xEF\xBB\xBF\xC3\xA9")
        .unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = base;
    loaded.cpu.gpr[5] = 5;
    loaded.cpu.gpr[6] = 0x0800_0100;
    loaded.cpu.gpr[7] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringCreateWithBytes,
    );
    let reference = loaded.cpu.gpr[3];
    assert_ne!(reference, 0);
    loaded.cpu.gpr[3] = reference;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetLength);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let used_len = base + 64;
    let stack_arg = loaded.cpu.gpr[1] + PPC_PARAMETER_AREA_OFFSET + 8 * 4;
    loaded.memory.write_u32_be(stack_arg, used_len).unwrap();
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 0x0800_0100;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 1;
    loaded.cpu.gpr[9] = base + 32;
    loaded.cpu.gpr[10] = 5;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetBytes);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(used_len), Some(5));
    assert_eq!(
        (0..5)
            .map(|offset| loaded.memory.read_u8(base + 32 + offset))
            .collect::<Option<Vec<_>>>(),
        Some(b"\xEF\xBB\xBF\xC3\xA9".to_vec())
    );

    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfStringGetBytes);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(used_len), Some(5));
}
