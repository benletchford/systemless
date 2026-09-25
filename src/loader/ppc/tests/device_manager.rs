use super::*;

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
