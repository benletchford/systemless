use super::*;

#[test]
fn hle_import_runner_handles_gestalt_powerpc_capabilities_and_rejects_sysa() {
    let pef = synthetic_pef_with_import(b"Gestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(response_ptr, vec![0; 16]);

    for (selector, expected_error, expected_response) in [
        (*b"cput", PPC_NO_ERR, 0x0104),
        (*b"proc", PPC_NO_ERR, 2),
        (*b"fpu ", PPC_NO_ERR, 3),
        (*b"mmu ", PPC_NO_ERR, 4),
        (*b"sysa", PPC_GESTALT_UNDEF_SELECTOR_ERR, 0),
    ] {
        loaded.cpu.gpr[3] = u32::from_be_bytes(selector);
        loaded.cpu.gpr[4] = response_ptr;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::Gestalt);

        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected_error));
        assert_eq!(
            loaded.memory.read_u32_be(response_ptr),
            Some(expected_response)
        );
    }
}

#[test]
fn hle_import_runner_handles_gestalt_cfm_present() {
    let pef = synthetic_pef_with_import(b"Gestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(response_ptr, vec![0; 16]);
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"cfrg");
    loaded.cpu.gpr[4] = response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(response_ptr), Some(1));
}

#[test]
fn hle_import_runner_reports_system7_color_quickdraw_13() {
    let pef = synthetic_pef_with_import(b"Gestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(response_ptr, vec![0; 16]);
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"qd  ");
    loaded.cpu.gpr[4] = response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(response_ptr), Some(0x0230));
}

#[test]
fn hle_import_runner_handles_gestalt_qd3d_present_and_version() {
    let pef = synthetic_pef_with_import(b"Gestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(response_ptr, vec![0; 16]);

    for (selector, expected_response) in [(*b"qd3d", 1), (*b"q3v ", PPC_QD3D_VERSION)] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.memory.write_u32_be(response_ptr, 0).unwrap();
        loaded.cpu.gpr[3] = u32::from_be_bytes(selector);
        loaded.cpu.gpr[4] = response_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(response_ptr),
            Some(expected_response)
        );
    }
}

#[test]
fn hle_import_runner_reports_unknown_gestalt_selector() {
    let pef = synthetic_pef_with_import(b"Gestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_HEAP_BASE;
    loaded.memory.add_region(response_ptr, vec![0; 16]);
    loaded
        .memory
        .write_u32_be(response_ptr, 0xfeed_face)
        .unwrap();
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"zzzz");
    loaded.cpu.gpr[4] = response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_GESTALT_UNDEF_SELECTOR_ERR)
    );
    assert_eq!(loaded.memory.read_u32_be(response_ptr), Some(0));
}
