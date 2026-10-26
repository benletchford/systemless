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
