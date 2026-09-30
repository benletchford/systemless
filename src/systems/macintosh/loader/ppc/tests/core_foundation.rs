use super::*;
use crate::process_context::ProcessVfsFileRecord;

#[test]
fn classic_cfbundle_load_executable_rejects_mach_o_code() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFBundleLoadExecutable"),
        PpcImportDispatcherTarget::CfBundleLoadExecutable
    );
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFBundleLoadExecutable")).unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    loaded.seed_vfs_files_and_resources(
        vec![
            ProcessVfsFileRecord {
                path: "Demo.app/Contents/Frameworks/Input.bundle/Contents/Info.plist"
                    .to_string(),
                data: b"<plist><dict><key>CFBundleExecutable</key><string>libInput.dylib</string></dict></plist>"
                    .to_vec()
                    .into(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            },
            ProcessVfsFileRecord {
                path: "Demo.app/Contents/Frameworks/Input.bundle/Contents/MacOS/libInput.dylib"
                    .to_string(),
                data: vec![0xfe, 0xed, 0xfa, 0xce, 0, 0, 0, 18].into(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            },
        ],
        vec![],
        vec![],
    );

    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleGetMainBundle);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    let frameworks = loaded.cpu.gpr[3];
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded.memory.write_bytes(source, b"Input.bundle\0").unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let component = loaded.cpu.gpr[3];
    loaded.cpu.gpr[4] = frameworks;
    loaded.cpu.gpr[5] = component;
    loaded.cpu.gpr[6] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent,
    );
    let bundle_url = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bundle_url;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleCreate);
    let bundle = loaded.cpu.gpr[3];
    assert_ne!(bundle, 0);

    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleLoadExecutable);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_cfbundle_create_opens_nested_bundle_and_registers_identifier() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFBundleCreate"),
        PpcImportDispatcherTarget::CfBundleCreate
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CFBundleCreate")).unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    loaded.seed_vfs_files_and_resources(
        vec![ProcessVfsFileRecord {
            path: "Demo.app/Contents/Frameworks/Plugin.bundle/Contents/Info.plist".to_string(),
            data: b"<plist><dict><key>CFBundleIdentifier</key><string>org.example.plugin</string></dict></plist>".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }],
        vec![],
        vec![],
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    let frameworks = loaded.cpu.gpr[3];
    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded
        .memory
        .write_bytes(source, b"Plugin.bundle\0")
        .unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let component = loaded.cpu.gpr[3];
    loaded.cpu.gpr[4] = frameworks;
    loaded.cpu.gpr[5] = component;
    loaded.cpu.gpr[6] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent,
    );
    let bundle_url = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bundle_url;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleCreate);
    let bundle = loaded.cpu.gpr[3];
    assert_ne!(bundle, 0);
    loaded.cpu.gpr[4] = bundle_url;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleCreate);
    assert_eq!(loaded.cpu.gpr[3], bundle);
    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 2);

    loaded
        .memory
        .write_bytes(source, b"org.example.plugin\0")
        .unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier,
    );
    assert_eq!(loaded.cpu.gpr[3], bundle);
    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_cfbundle_create_rejects_invalid_url() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CFBundleCreate")).unwrap();
    loaded.cpu.gpr[4] = 0x1234;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfBundleCreate);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_cfurl_appends_independently_owned_file_and_directory_components() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFURLCreateCopyAppendingPathComponent"),
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(
        b"CFURLCreateCopyAppendingPathComponent",
    ))
    .unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    loaded.seed_vfs_files_and_resources(
        vec![ProcessVfsFileRecord {
            path: "Demo.app/Contents/Frameworks/Plugin.bundle/Contents/Info.plist".to_string(),
            data: b"<plist/>".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }],
        vec![],
        vec![],
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    let base = loaded.cpu.gpr[3];
    assert_ne!(base, 0);

    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded
        .memory
        .write_bytes(source, b"Plugin.bundle\0")
        .unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let component = loaded.cpu.gpr[3];

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = base;
    loaded.cpu.gpr[5] = component;
    loaded.cpu.gpr[6] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent,
    );
    let directory = loaded.cpu.gpr[3];
    assert_ne!(directory, base);
    assert_eq!(
        loaded.toolbox_startup.cf_strings.url_path(directory),
        Some("Demo.app/Contents/Frameworks/Plugin.bundle")
    );
    assert_eq!(
        loaded
            .toolbox_startup
            .cf_strings
            .url_is_directory(directory),
        Some(true)
    );

    loaded.memory.write_bytes(source, b"Info.plist\0").unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    let file_component = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = directory;
    loaded.cpu.gpr[5] = file_component;
    loaded.cpu.gpr[6] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent,
    );
    let file = loaded.cpu.gpr[3];
    assert_eq!(
        loaded.toolbox_startup.cf_strings.url_path(file),
        Some("Demo.app/Contents/Frameworks/Plugin.bundle/Info.plist")
    );
    assert_eq!(
        loaded.toolbox_startup.cf_strings.url_is_directory(file),
        Some(false)
    );
    loaded.cpu.gpr[3] = base;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    assert_eq!(
        loaded.toolbox_startup.cf_strings.url_path(file),
        Some("Demo.app/Contents/Frameworks/Plugin.bundle/Info.plist")
    );
}

#[test]
fn carbon_cfurl_append_rejects_unknown_url_or_string() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(
        b"CFURLCreateCopyAppendingPathComponent",
    ))
    .unwrap();
    loaded.cpu.gpr[4] = 0x1234;
    loaded.cpu.gpr[5] = 0x5678;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_private_frameworks_url_is_owned_and_uses_bundle_directory() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFBundleCopyPrivateFrameworksURL"),
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(
        b"CFBundleCopyPrivateFrameworksURL",
    ))
    .unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    loaded.seed_vfs_files_and_resources(
        vec![ProcessVfsFileRecord {
            path: "Demo.app/Contents/Frameworks/Plugin.bundle/Contents/Info.plist".to_string(),
            data: b"<plist/>".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }],
        vec![],
        vec![],
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    let bundle = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = bundle;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    let url = loaded.cpu.gpr[3];
    assert_ne!(url, 0);
    assert_eq!(
        loaded.toolbox_startup.cf_strings.url_path(url),
        Some("Demo.app/Contents/Frameworks")
    );
    loaded.cpu.gpr[3] = url;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.cpu.gpr[3] = url;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    assert_eq!(loaded.toolbox_startup.cf_strings.url_path(url), None);
}

#[test]
fn carbon_private_frameworks_url_is_null_without_directory_or_bundle() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(
        b"CFBundleCopyPrivateFrameworksURL",
    ))
    .unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    let bundle = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = bundle;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let mut unbundled = load_pef_application(&synthetic_pef_with_import(
        b"CFBundleCopyPrivateFrameworksURL",
    ))
    .unwrap();
    unbundled.set_launched_app_path("Legacy App");
    run_test_import(
        &mut unbundled,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    let bundle = unbundled.cpu.gpr[3];
    unbundled.cpu.gpr[3] = bundle;
    run_test_import(
        &mut unbundled,
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl,
    );
    assert_eq!(unbundled.cpu.gpr[3], 0);
}

#[test]
fn carbon_main_bundle_is_stable_and_found_by_its_info_plist_identifier() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CFBundleGetMainBundle"),
        PpcImportDispatcherTarget::CfBundleGetMainBundle
    );
    let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"CarbonLib",
        b"CFBundleGetMainBundle",
        0x82,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut weak_loaded = load_pef_application(&pef).unwrap();
    assert_ne!(weak_loaded.imports[0].address, 0);
    assert_eq!(
        weak_loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(weak_loaded.imports[0].address)
    );

    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFBundleGetMainBundle")).unwrap();
    loaded.set_launched_app_path("Demo.app/Contents/MacOSClassic/Demo");
    loaded.seed_vfs_files_and_resources(
        vec![ProcessVfsFileRecord {
            path: "Demo.app/Contents/Info.plist".to_string(),
            data: b"<plist><dict><key>CFBundleIdentifier</key><string>org.example.demo</string></dict></plist>".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }],
        vec![],
        vec![],
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    let bundle = loaded.cpu.gpr[3];
    assert_ne!(bundle, 0);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    assert_eq!(loaded.cpu.gpr[3], bundle);

    let source = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(source, vec![0; 64]);
    loaded
        .memory
        .write_bytes(source, b"org.example.demo\0")
        .unwrap();
    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfStringMakeConstantString,
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier,
    );
    assert_eq!(loaded.cpu.gpr[3], bundle);

    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRetain);
    assert_eq!(loaded.cpu.gpr[3], bundle);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 2);
    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfRelease);
    loaded.cpu.gpr[3] = bundle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CfGetRetainCount);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn carbon_main_bundle_can_represent_an_unbundled_application() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CFBundleGetMainBundle")).unwrap();
    loaded.set_launched_app_path("Legacy App");
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::CfBundleGetMainBundle,
    );
    assert_ne!(loaded.cpu.gpr[3], 0);
}

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
