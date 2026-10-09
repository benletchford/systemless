use super::*;

#[test]
fn system_task_and_system_click_are_quiescent() {
    for symbol in [b"SystemTask".as_slice(), b"SystemClick".as_slice()] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0x51a7_0001;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x51a7_0001);
    }
}

#[test]
fn open_desk_acc_reports_no_open_accessory() {
    let pef = synthetic_pef_with_import(b"OpenDeskAcc");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE + 0x1000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn close_desk_acc_is_quiescent() {
    let pef = synthetic_pef_with_import(b"CloseDeskAcc");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x51a7_0002;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x51a7_0002);
}

#[test]
fn system_edit_reports_unhandled_by_desk_accessory() {
    let pef = synthetic_pef_with_import(b"SystemEdit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 3;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}
