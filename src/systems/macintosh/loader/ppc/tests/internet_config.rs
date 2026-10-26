use super::*;

#[test]
fn internet_config_weak_imports_bind_to_typed_lifecycle_operations() {
    let operations = [
        ("ICStart", PpcImportDispatcherTarget::ICStart),
        ("ICStop", PpcImportDispatcherTarget::ICStop),
        ("ICGetSeed", PpcImportDispatcherTarget::ICGetSeed),
    ];
    for (name, target) in operations {
        assert_eq!(dispatcher_target_for_import("CarbonLib", name), target);
        let bindings = PpcImportBindingPlan::prepare(
            vec![PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "CarbonLib".into(),
                symbol_name: name.into(),
                class: 2,
                weak: true,
            }],
            1,
            0,
            ppc_import_layout(),
            &SystemlessPpcImportBindingPolicy,
        )
        .unwrap()
        .into_initial_bindings();
        assert_eq!(bindings[0].address, PPC_IMPORT_TVECTOR_BASE);
        assert_eq!(bindings[0].dispatcher_target, target);
    }
}

#[test]
fn internet_config_instances_share_seed_and_dispose_only_owned_storage() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    let seed_out = out + 16;
    let creator = u32::from_be_bytes(*b"MSol");
    let mut instances = Vec::new();
    for output in [out, out + 4] {
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = creator;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let instance = loaded.memory.read_u32_be(output).unwrap();
        assert_ne!(instance, 0);
        assert!(loaded.ptrs().iter().any(|p| p.ptr == instance));
        assert_eq!(loaded.memory.read_u32_be(instance), Some(creator));
        instances.push(instance);
    }
    assert_ne!(instances[0], instances[1]);
    loaded.toolbox_startup.internet_config.preferences_seed = 0x8765_4321;
    loaded
        .memory
        .write_u32_be(seed_out - 4, 0xAABB_CCDD)
        .unwrap();
    loaded
        .memory
        .write_u32_be(seed_out + 4, 0x1122_3344)
        .unwrap();
    for instance in &instances {
        loaded.cpu.gpr[3] = *instance;
        loaded.cpu.gpr[4] = seed_out;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u32_be(seed_out), Some(0x8765_4321));
    }
    assert_eq!(loaded.memory.read_u32_be(seed_out - 4), Some(0xAABB_CCDD));
    assert_eq!(loaded.memory.read_u32_be(seed_out + 4), Some(0x1122_3344));
    loaded.cpu.gpr[3] = instances[0];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(!loaded.ptrs().iter().any(|p| p.ptr == instances[0]));
    loaded.cpu.gpr[3] = instances[0];
    loaded.cpu.gpr[4] = seed_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(seed_out), Some(0x8765_4321));
    loaded.cpu.gpr[3] = instances[0];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert!(loaded.ptrs().iter().any(|p| p.ptr == instances[1]));

    loaded.cpu.gpr[3] = 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    let unrelated = loaded.cpu.gpr[3];
    assert_ne!(unrelated, 0);
    loaded.cpu.gpr[3] = unrelated;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert!(loaded.ptrs().iter().any(|p| p.ptr == unrelated));
}

#[test]
fn internet_config_invalid_outputs_do_not_allocate_or_overwrite_memory() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.cpu.gpr[3] = out;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    let instance = loaded.memory.read_u32_be(out).unwrap();
    let cursor = loaded.heap_cursor();
    let ptr_count = loaded.ptrs().len();
    for output in [0, u32::MAX - 1] {
        loaded.cpu.gpr[3] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
        assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
        assert_eq!(loaded.heap_cursor(), cursor);
        assert_eq!(loaded.ptrs().len(), ptr_count);
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
        assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    }
    assert_eq!(loaded.memory.read_u32_be(out), Some(instance));
}

#[test]
fn internet_config_failed_allocation_clears_output_and_leaves_no_live_instance() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.cpu.gpr[3] = loaded.heap_cursor();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    let cursor = loaded.heap_cursor();
    let count = loaded.ptrs().len();
    loaded.memory.write_u32_be(out, 0xDEAD_BEEF).unwrap();
    loaded.cpu.gpr[3] = out;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.memory.read_u32_be(out), Some(0));
    assert_eq!(loaded.heap_cursor(), cursor);
    assert_eq!(loaded.ptrs().len(), count);
}

#[test]
fn original_internet_config_library_shares_owned_instances_and_reports_glue() {
    for library in [
        b"ICAp;InternetConfigLib".as_slice(),
        b"InternetConfigLib".as_slice(),
    ] {
        let pef = synthetic_pef_with_library_import(library, b"ICStart");
        let mut loaded = load_pef_application(&pef).unwrap();
        let out = PPC_STACK_TOP - 64;
        loaded.cpu.gpr[3] = out;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let instance = loaded.memory.read_u32_be(out).unwrap();
        assert_ne!(instance, 0);
        loaded.memory.write_u32_be(out + 4, 0xaaaa_aaaa).unwrap();
        loaded.memory.write_u32_be(out + 8, 0x1234_5678).unwrap();
        for symbol in ["ICGetComponentInstance", "ICGetSeed", "ICStop"] {
            let target =
                dispatcher_target_for_import(std::str::from_utf8(library).unwrap(), symbol);
            assert_ne!(target, PpcImportDispatcherTarget::Unsupported);
            loaded.cpu.gpr[3] = instance;
            loaded.cpu.gpr[4] = out + 4;
            run_test_import(&mut loaded, target);
            if symbol == "ICGetComponentInstance" {
                assert_eq!(loaded.cpu.gpr[3], 0x8000_8001);
                assert_eq!(loaded.memory.read_u32_be(out + 4), Some(0));
            } else {
                assert_eq!(loaded.cpu.gpr[3], 0);
            }
            assert_eq!(loaded.memory.read_u32_be(out + 8), Some(0x1234_5678));
        }
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = out + 4;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::ICGetComponentInstance,
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }
}

#[test]
fn internet_config_url_normalization_matches_classic_programmer_kit_cases() {
    // Cases from IC 2.0.2's ICeTEe/Test URLs and ICURLHandling.p.
    let cases: &[(&[u8], &[u8], usize, usize, &[u8])] = &[
        (b"[http://wibb\r\rle.com]", b"", 0, 21, b"http://wibble.com"),
        (
            b"See www.apple.com. end",
            b"",
            8,
            8,
            b"http://www.apple.com",
        ),
        (
            b"See www.apple.com. end",
            b"",
            4,
            18,
            b"http://www.apple.com.",
        ),
        (
            b"quinn@quinn.echidna.id.au",
            b"mailto",
            0,
            25,
            b"mailto:quinn@quinn.echidna.id.au",
        ),
        (
            b"<URL:HTTP://Example.test/A>",
            b"",
            8,
            8,
            b"HTTP://Example.test/A",
        ),
        (
            b"  <test://wibble.com  \r /wobble> ",
            b"",
            8,
            8,
            b"test://wibble.com/wobble",
        ),
        (
            b"\xd2http://example.test\xd3",
            b"",
            8,
            8,
            b"http://example.test",
        ),
    ];
    for &(data, hint, mut start, mut end, expected) in cases {
        assert_eq!(
            dispatch_internet_config::parse_url_bytes(data, hint, &mut start, &mut end).unwrap(),
            expected,
            "input={data:?}"
        );
    }
    for data in [b"<not a URL>".as_slice(), b"<http://missing", b" ", b"'"] {
        let mut start = 0;
        let mut end = 0;
        assert_eq!(
            dispatch_internet_config::parse_url_bytes(data, b"", &mut start, &mut end),
            Err(-673)
        );
    }
}

#[test]
fn internet_config_parse_url_updates_selection_and_resizes_owned_handle() {
    let pef = synthetic_pef_with_library_import(b"ICAp;InternetConfigLib", b"ICStart");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 512]);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    let instance = loaded.memory.read_u32_be(scratch).unwrap();
    loaded.cpu.gpr[3] = 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let handle = loaded.cpu.gpr[3];
    let text = b"See <URL:http://example.test/a> now";
    loaded.memory.write_bytes(scratch + 32, text).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 8, 0xaaaa_aaaa)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 20, 0xbbbb_bbbb)
        .unwrap();
    for (start, end, expected_error) in [(12, 12, 0), (40, 12, ppc_i16_result(PPC_PARAM_ERR))] {
        loaded.memory.write_u32_be(scratch + 12, start).unwrap();
        loaded.memory.write_u32_be(scratch + 16, end).unwrap();
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = scratch + 32;
        loaded.cpu.gpr[6] = text.len() as u32;
        loaded.cpu.gpr[7] = scratch + 12;
        loaded.cpu.gpr[8] = scratch + 16;
        loaded.cpu.gpr[9] = handle;
        let target = dispatcher_target_for_import("ICAp;InternetConfigLib", "ICParseURL");
        run_test_import(&mut loaded, target);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.memory.read_u32_be(scratch + 8), Some(0xaaaa_aaaa));
        assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0xbbbb_bbbb));
        if expected_error == 0 {
            assert_eq!(loaded.memory.read_u32_be(scratch + 12), Some(4));
            assert_eq!(
                loaded.memory.read_u32_be(scratch + 16),
                Some((text.len() - 4) as u32)
            );
        }
        let bytes = b"http://example.test/a";
        let pointer = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pointer, bytes.len() as u32).unwrap(),
            bytes
        );
        loaded.cpu.gpr[3] = handle;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::GetHandleSize);
        assert_eq!(loaded.cpu.gpr[3], bytes.len() as u32);
    }
}

#[test]
fn internet_config_reads_active_profile_from_guest_resources_with_truncation() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 512]);
    loaded.cpu.gpr[3] = scratch;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    let instance = loaded.memory.read_u32_be(scratch).unwrap();
    let mut entries = Vec::new();
    for (kind, id, name, data) in [
        (*b"CURP", 128, b"".as_slice(), b"ICRQ".to_vec()),
        (*b"ICRP", 128, b"Email", b"\0\0\0\0wrong-profile".to_vec()),
        (*b"ICRQ", 128, b"Email", b"\0\0\0\x02a@b.test".to_vec()),
        (*b"ICRQ", 129, b"Broken", vec![0, 0, 0]),
    ] {
        entries.push(crate::managers::resource::ResourceForkEntry {
            res_type: kind,
            id,
            name: name.to_vec(),
            data,
            attrs: 0,
        });
    }
    let fork = crate::managers::resource::serialize_resource_fork_with_attrs(&entries, 0).unwrap();
    loaded
        .process_file_system
        .push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Internet Preferences".into(),
            creator: u32::from_be_bytes(*b"ICAp"),
            file_type: u32::from_be_bytes(*b"ICAp"),
            finder_flags: 0,
            resource_len: fork.len() as u32,
            raw_data: Some(fork.into()),
            map_attrs: 0,
            dirty: false,
        });
    for (key, capacity, null_buffer, error, attr, length, copied) in [
        (b"email".as_slice(), 3, false, -670, 2, 8, b"a@b".as_slice()),
        (b"EMAIL", 16, false, 0, 2, 8, b"a@b.test"),
        (b"Email", -1, true, 0, 2, 8, b""),
        (b"Email", -1, false, -50, u32::MAX, 0, b""),
        (b"Missing", 16, false, -666, u32::MAX, 0, b""),
        (b"Broken", 16, false, -668, u32::MAX, 0, b""),
        (b"", 16, false, -50, u32::MAX, 0, b""),
    ] {
        loaded
            .memory
            .write_u8(scratch + 32, key.len() as u8)
            .unwrap();
        loaded.memory.write_bytes(scratch + 33, key).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 8, 0xaaaa_aaaa)
            .unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 12, capacity as u32)
            .unwrap();
        loaded
            .memory
            .write_bytes(scratch + 128, &[0xcc; 20])
            .unwrap();
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = scratch + 32;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = if null_buffer { 0 } else { scratch + 128 };
        loaded.cpu.gpr[7] = scratch + 12;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("ICAp;InternetConfigLib", "ICGetPref"),
        );
        assert_eq!(loaded.cpu.gpr[3], (error as i32) as u32);
        assert_eq!(loaded.memory.read_u32_be(scratch + 8), Some(attr));
        assert_eq!(loaded.memory.read_u32_be(scratch + 12), Some(length));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, scratch + 128, copied.len() as u32).unwrap(),
            copied
        );
        assert_eq!(
            loaded.memory.read_u8(scratch + 128 + copied.len() as u32),
            Some(0xcc)
        );
    }
}

#[test]
fn internet_config_preferences_persist_in_resource_forks_and_enforce_sessions() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let p = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(p, vec![0; 512]);
    loaded.cpu.gpr[3] = p;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    let instance = loaded.memory.read_u32_be(p).unwrap();
    loaded.memory.write_bytes(p + 32, b"\x05Email").unwrap();
    loaded.memory.write_bytes(p + 64, b"guest@test").unwrap();
    for (permission, expected_begin, expected_write, attributes) in [
        (1, 0, -667, 0),
        (2, 0, 0, 1),
        (2, 0, -667, u32::MAX),
        (2, 0, 0, 0),
    ] {
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = permission;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICBegin);
        assert_eq!(loaded.cpu.gpr[3], expected_begin);
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = permission;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICBegin);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = p + 32;
        loaded.cpu.gpr[5] = attributes;
        loaded.cpu.gpr[6] = p + 64;
        loaded.cpu.gpr[7] = 10;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICSetPref);
        assert_eq!(loaded.cpu.gpr[3], (expected_write as i32) as u32);
        loaded.cpu.gpr[3] = instance;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICEnd);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    // Keep a loaded Resource Manager handle attached while IC rewrites the
    // preference. Both views must expose the same resized payload.
    loaded.cpu.gpr[3] = 4;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let resource_handle = loaded.cpu.gpr[3];
    loaded
        .process_file_system
        .with_resource_manager_mut(|manager| {
            manager
                .vfs_resources
                .iter_mut()
                .find(|record| record.name == b"Email")
                .unwrap()
                .handle = resource_handle;
        });
    loaded.cpu.gpr[3] = instance;
    loaded.cpu.gpr[4] = p + 32;
    loaded.cpu.gpr[5] = u32::MAX;
    loaded.cpu.gpr[6] = p + 64;
    loaded.cpu.gpr[7] = 10;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICSetPref);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let pointer = loaded.memory.read_u32_be(resource_handle).unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, pointer, 14).unwrap(),
        b"\0\0\0\0guest@test"
    );
    loaded.cpu.gpr[3] = resource_handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetHandleSize);
    assert_eq!(loaded.cpu.gpr[3], 14);
    let fork = loaded
        .process_file_system
        .with_resource_manager_mut(|manager| {
            let file = manager
                .vfs_resource_files
                .iter()
                .find(|file| file.path.ends_with("Internet Preferences"))
                .unwrap();
            assert!(file.dirty);
            ppc_serialized_resource_fork(file, &manager.vfs_resources).unwrap()
        });
    // Reload the saved fork into a different process, with no IC instance state
    // carried over. Reading must materialize the serialized resources.
    let mut fresh = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    fresh.memory.add_region(p, vec![0; 512]);
    fresh
        .process_file_system
        .push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Internet Preferences".into(),
            creator: u32::from_be_bytes(*b"ICAp"),
            file_type: u32::from_be_bytes(*b"ICAp"),
            finder_flags: 0,
            resource_len: fork.len() as u32,
            raw_data: Some(fork.into()),
            map_attrs: 0,
            dirty: false,
        });
    fresh.cpu.gpr[3] = p;
    run_test_import(&mut fresh, PpcImportDispatcherTarget::ICStart);
    let instance = fresh.memory.read_u32_be(p).unwrap();
    fresh.memory.write_bytes(p + 32, b"\x05Email").unwrap();
    fresh.memory.write_u32_be(p + 12, 32).unwrap();
    fresh.cpu.gpr[3] = instance;
    fresh.cpu.gpr[4] = p + 32;
    fresh.cpu.gpr[5] = p + 8;
    fresh.cpu.gpr[6] = p + 64;
    fresh.cpu.gpr[7] = p + 12;
    run_test_import(&mut fresh, PpcImportDispatcherTarget::ICGetPref);
    assert_eq!(fresh.cpu.gpr[3], 0);
    assert_eq!(fresh.memory.read_u32_be(p + 8), Some(0));
    assert_eq!(fresh.memory.read_u32_be(p + 12), Some(10));
    assert_eq!(
        ppc_memory_read_bytes(&mut fresh.memory, p + 64, 10).unwrap(),
        b"guest@test"
    );
    fresh.cpu.gpr[3] = 1;
    run_test_import(
        &mut fresh,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let handle = fresh.cpu.gpr[3];
    fresh.cpu.gpr[3] = handle;
    run_test_import(&mut fresh, PpcImportDispatcherTarget::HLock);
    for (key, error, size) in [(b"\x05Email".as_slice(), 0, 10), (b"\x07Missing", -666, 0)] {
        fresh.memory.write_bytes(p + 32, key).unwrap();
        fresh.cpu.gpr[3] = instance;
        fresh.cpu.gpr[4] = p + 32;
        fresh.cpu.gpr[5] = p + 8;
        fresh.cpu.gpr[6] = handle;
        run_test_import(
            &mut fresh,
            dispatcher_target_for_import("ICAp;InternetConfigLib", "ICFindPrefHandle"),
        );
        assert_eq!(fresh.cpu.gpr[3], (error as i32) as u32);
        fresh.cpu.gpr[3] = handle;
        run_test_import(&mut fresh, PpcImportDispatcherTarget::GetHandleSize);
        assert_eq!(fresh.cpu.gpr[3], size);
        if size != 0 {
            let pointer = fresh.memory.read_u32_be(handle).unwrap();
            assert_eq!(
                ppc_memory_read_bytes(&mut fresh.memory, pointer, size).unwrap(),
                b"guest@test"
            );
        }
        fresh.cpu.gpr[3] = handle;
        run_test_import(&mut fresh, PpcImportDispatcherTarget::HGetState);
        assert_eq!(fresh.cpu.gpr[3] & 0x80, 0x80);
    }
}

#[test]
fn internet_config_folder_selection_keeps_databases_and_seeds_separate() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let p = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(p, vec![0; 512]);
    let mut instances = Vec::new();
    for _ in 0..2 {
        loaded.cpu.gpr[3] = p;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
        instances.push(loaded.memory.read_u32_be(p).unwrap());
    }
    loaded
        .process_file_system
        .push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Internet Preferences".into(),
            creator: u32::from_be_bytes(*b"ICAp"),
            file_type: u32::from_be_bytes(*b"ICAp"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
    loaded.memory.write_u16_be(p + 32, 0).unwrap();
    loaded.memory.write_u32_be(p + 34, PPC_ROOT_DIR_ID).unwrap();
    loaded.cpu.gpr[3] = instances[1];
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = p + 32;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICFindConfigFile);
    assert_eq!(loaded.cpu.gpr[3], 0);
    // A failed configuration change must retain the selected database.
    loaded.memory.write_u32_be(p + 34, 0x12345678).unwrap();
    loaded.cpu.gpr[3] = instances[1];
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = p + 32;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICFindConfigFile);
    assert_eq!(loaded.cpu.gpr[3], (-120i32) as u32);
    loaded.memory.write_bytes(p + 64, b"\x04Test").unwrap();
    loaded.memory.write_bytes(p + 96, b"value").unwrap();
    for (attributes, buffer, expected_seed) in [(0, p + 96, 2), (2, 0, 2), (0, 0, 3)] {
        loaded.cpu.gpr[3] = instances[1];
        loaded.cpu.gpr[4] = p + 64;
        loaded.cpu.gpr[5] = attributes;
        loaded.cpu.gpr[6] = buffer;
        loaded.cpu.gpr[7] = if buffer == 0 { u32::MAX } else { 5 };
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICSetPref);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for (instance, seed) in [(instances[0], 1), (instances[1], expected_seed)] {
            loaded.cpu.gpr[3] = instance;
            loaded.cpu.gpr[4] = p + 8;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(loaded.memory.read_u32_be(p + 8), Some(seed));
        }
    }
    for (instance, error) in [(instances[0], -666), (instances[1], 0)] {
        loaded.memory.write_u32_be(p + 12, 8).unwrap();
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = p + 64;
        loaded.cpu.gpr[5] = p + 8;
        loaded.cpu.gpr[6] = p + 128;
        loaded.cpu.gpr[7] = p + 12;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetPref);
        assert_eq!(loaded.cpu.gpr[3], (error as i32) as u32);
        if error == 0 {
            assert_eq!(loaded.memory.read_u32_be(p + 12), Some(5));
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, p + 128, 5).unwrap(),
                b"value"
            );
        }
    }
}
