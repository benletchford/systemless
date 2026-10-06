use super::*;

#[test]
fn auto_sleep_control_tracks_nested_disables() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "AutoSleepControl"),
        PpcImportDispatcherTarget::AutoSleepControl
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "IsAutoSlpControlDisabled"),
        PpcImportDispatcherTarget::IsAutoSlpControlDisabled
    );

    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"AutoSleepControl");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AutoSleepControl);
    assert_eq!(loaded.toolbox_startup.auto_sleep_disable_level, 1);
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AutoSleepControl);
    assert_eq!(loaded.toolbox_startup.auto_sleep_disable_level, 2);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::IsAutoSlpControlDisabled,
    );
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AutoSleepControl);
    assert_eq!(loaded.toolbox_startup.auto_sleep_disable_level, 1);
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AutoSleepControl);
    assert_eq!(loaded.toolbox_startup.auto_sleep_disable_level, 0);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::IsAutoSlpControlDisabled,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AutoSleepControl);
    assert_eq!(loaded.toolbox_startup.auto_sleep_disable_level, 0);
}

#[test]
fn get_dctl_entry_exposes_only_the_main_device() {
    let pef = synthetic_pef_with_import(b"GetDCtlEntry");
    let mut loaded = load_pef_application(&pef).unwrap();

    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_DCE_HANDLE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 1;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn open_driver_reports_open_err_and_clears_the_output_refnum() {
    let pef = synthetic_pef_with_import(b"OpenDriver");
    let mut loaded = load_pef_application(&pef).unwrap();
    let name = PPC_DATA_BASE + 0x1000;
    let refnum = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(name, vec![0; 0x42]);
    loaded.memory.write_u16_be(refnum, 0x7fff).unwrap();
    write_ppc_pstring(&mut loaded.memory, name, b".AIn");
    loaded.cpu.gpr[3] = name;
    loaded.cpu.gpr[4] = refnum;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_OPEN_ERR));
    assert_eq!(loaded.memory.read_u16_be(refnum), Some(0));
}

#[test]
fn import_bindings_classify_printing_imports() {
    for (symbol, operation) in [
        ("PrClose", PpcPrintingCompatibilityOperation::PrClose),
        ("PrCloseDoc", PpcPrintingCompatibilityOperation::PrCloseDoc),
        ("PrClosePage", PpcPrintingCompatibilityOperation::PrClosePage),
        ("PrError", PpcPrintingCompatibilityOperation::PrError),
        ("PrGeneral", PpcPrintingCompatibilityOperation::PrGeneral),
        ("PrSetError", PpcPrintingCompatibilityOperation::PrSetError),
        ("PrValidate", PpcPrintingCompatibilityOperation::PrValidate),
        ("PrJobDialog", PpcPrintingCompatibilityOperation::PrJobDialog),
        ("PrOpen", PpcPrintingCompatibilityOperation::PrOpen),
        ("PrOpenDoc", PpcPrintingCompatibilityOperation::PrOpenDoc),
        ("PrOpenPage", PpcPrintingCompatibilityOperation::PrOpenPage),
        ("PrPicFile", PpcPrintingCompatibilityOperation::PrPicFile),
        ("PrStlDialog", PpcPrintingCompatibilityOperation::PrStlDialog),
        ("PrintDefault", PpcPrintingCompatibilityOperation::PrintDefault),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::PrintingCompatibility(operation),
        );
    }
}

#[test]
fn printing_compatibility_dispatch_returns_expected_actions() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"PrError")).unwrap();
    let mut dispatch = |op| ppc_dispatch_printing_compatibility(
        op, &loaded.cpu, &mut loaded.memory, &mut loaded.toolbox_startup,
    );
    assert_eq!(
        dispatch(PpcPrintingCompatibilityOperation::PrJobDialog),
        PpcImportAction::Return(0)
    );
    assert_eq!(
        dispatch(PpcPrintingCompatibilityOperation::PrStlDialog),
        PpcImportAction::Return(0)
    );
    assert_eq!(
        dispatch(PpcPrintingCompatibilityOperation::PrOpenDoc),
        PpcImportAction::Return(0)
    );
    assert_eq!(
        dispatch(PpcPrintingCompatibilityOperation::PrError),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    assert_eq!(
        dispatch(PpcPrintingCompatibilityOperation::PrintDefault),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    for op in [
        PpcPrintingCompatibilityOperation::PrClose,
        PpcPrintingCompatibilityOperation::PrCloseDoc,
        PpcPrintingCompatibilityOperation::PrClosePage,
        PpcPrintingCompatibilityOperation::PrOpen,
        PpcPrintingCompatibilityOperation::PrOpenPage,
        PpcPrintingCompatibilityOperation::PrPicFile,
    ] {
        assert_eq!(
            dispatch(op),
            PpcImportAction::ReturnPreserve
        );
    }
    drop(dispatch);
    loaded.cpu.gpr[3] = ppc_i16_result(-128);
    assert_eq!(
        ppc_dispatch_printing_compatibility(
            PpcPrintingCompatibilityOperation::PrSetError,
            &loaded.cpu,
            &mut loaded.memory,
            &mut loaded.toolbox_startup,
        ),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(
        ppc_dispatch_printing_compatibility(
            PpcPrintingCompatibilityOperation::PrError,
            &loaded.cpu,
            &mut loaded.memory,
            &mut loaded.toolbox_startup,
        ),
        PpcImportAction::Return(ppc_i16_result(-128)),
    );
    let general_data = PPC_DATA_BASE + 0x1800;
    loaded.memory.add_region(general_data, vec![0; 8]);
    loaded.cpu.gpr[3] = general_data;
    assert_eq!(loaded.memory.write_u16_be(general_data, 8), Some(()));
    assert_eq!(
        ppc_dispatch_printing_compatibility(
            PpcPrintingCompatibilityOperation::PrGeneral,
            &loaded.cpu,
            &mut loaded.memory,
            &mut loaded.toolbox_startup,
        ),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(loaded.memory.read_u16_be(general_data + 2), Some(2));
    assert_eq!(loaded.toolbox_startup.printing_error, 2);
}

#[test]
fn hle_import_runner_get_adb_info_exposes_standard_devices() {
    for (address, expected) in [(2, [2, 2]), (3, [1, 3])] {
        let pef = synthetic_pef_with_import(b"GetADBInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let info_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(info_ptr, vec![0xaa; 12]);
        loaded.cpu.gpr[3] = info_ptr;
        loaded.cpu.gpr[4] = address;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u8(info_ptr), Some(expected[0]));
        assert_eq!(loaded.memory.read_u8(info_ptr + 1), Some(expected[1]));
        assert_eq!(loaded.memory.read_u32_be(info_ptr + 2), Some(0));
        assert_eq!(loaded.memory.read_u32_be(info_ptr + 6), Some(0));
        assert_eq!(loaded.memory.read_u16_be(info_ptr + 10), Some(0xaaaa));
    }

    let pef = synthetic_pef_with_import(b"GetADBInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
}
