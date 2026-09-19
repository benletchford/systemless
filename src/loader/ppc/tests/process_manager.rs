use super::*;

#[test]
fn hle_import_runner_enumerates_the_single_process() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let psn_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(psn_ptr, vec![0; 8]);
    loaded.cpu.gpr[3] = psn_ptr;

    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetNextProcess,
        ),
    );

    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr),
        Some(ProcessSerialNumber::CURRENT.high)
    );
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr + 4),
        Some(ProcessSerialNumber::CURRENT.low)
    );

    loaded.cpu.gpr[3] = psn_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetNextProcess,
        ),
    );

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr + 4),
        Some(ProcessSerialNumber::CURRENT.low)
    );

    loaded
        .memory
        .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low + 1)
        .unwrap();
    loaded.cpu.gpr[3] = psn_ptr;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetNextProcess,
        ),
    );

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr + 4),
        Some(ProcessSerialNumber::CURRENT.low + 1)
    );
}

#[test]
fn hle_import_runner_handles_get_current_process() {
    let pef = synthetic_pef_with_import(b"GetCurrentProcess");
    let mut loaded = load_pef_application(&pef).unwrap();
    let psn_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(psn_ptr, vec![0xaa; 8]);
    loaded.cpu.gpr[3] = psn_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr),
        Some(ProcessSerialNumber::CURRENT.high)
    );
    assert_eq!(
        loaded.memory.read_u32_be(psn_ptr + 4),
        Some(ProcessSerialNumber::CURRENT.low)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn hle_import_runner_handles_wake_up_process() {
    let pef = synthetic_pef_with_import(b"WakeUpProcess");
    let mut loaded = load_pef_application(&pef).unwrap();
    let psn_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(psn_ptr, vec![0; 8]);
    loaded
        .memory
        .write_u32_be(psn_ptr, ProcessSerialNumber::CURRENT.high)
        .unwrap();
    loaded
        .memory
        .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low)
        .unwrap();
    loaded.cpu.gpr[3] = psn_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = psn_ptr;
    loaded
        .memory
        .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low + 1)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0x0600_0000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn hle_import_runner_handles_same_process() {
    let pef = synthetic_pef_with_import(b"SameProcess");
    let mut loaded = load_pef_application(&pef).unwrap();
    let first_psn_ptr = PPC_DATA_BASE + 0x1000;
    let second_psn_ptr = PPC_DATA_BASE + 0x1008;
    let result_ptr = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(first_psn_ptr, vec![0; 17]);
    for psn_ptr in [first_psn_ptr, second_psn_ptr] {
        loaded
            .memory
            .write_u32_be(psn_ptr, ProcessSerialNumber::CURRENT.high)
            .unwrap();
        loaded
            .memory
            .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low)
            .unwrap();
    }
    loaded.cpu.gpr[3] = first_psn_ptr;
    loaded.cpu.gpr[4] = second_psn_ptr;
    loaded.cpu.gpr[5] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u8(result_ptr), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = first_psn_ptr;
    loaded.cpu.gpr[4] = second_psn_ptr;
    loaded.cpu.gpr[5] = result_ptr;
    loaded
        .memory
        .write_u32_be(second_psn_ptr + 4, ProcessSerialNumber::CURRENT.low + 1)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u8(result_ptr), Some(0));
}

#[test]
fn hle_import_runner_handles_get_process_information() {
    let pef = synthetic_pef_with_import(b"GetProcessInformation");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.vfs_directories.push(PpcVfsDirectory {
        dir_id: 100,
        path: "Gridz Demo ƒ".to_string(),
        parent_dir_id: PPC_ROOT_DIR_ID,
        finder_flags: 0,
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        dirty: false,
    });
    loaded.vfs_directories.push(PpcVfsDirectory {
        dir_id: 101,
        path: "Source Folder".to_string(),
        parent_dir_id: PPC_ROOT_DIR_ID,
        finder_flags: 0,
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        dirty: false,
    });
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Source Folder/Installer".to_string(),
        data: (Vec::new()).into(),
        creator: u32::from_be_bytes(*b"SRC!"),
        file_type: u32::from_be_bytes(*b"APPL"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Gridz Demo ƒ/Gridz™ Demo".to_string(),
        data: (Vec::new()).into(),
        creator: u32::from_be_bytes(*b"Grid"),
        file_type: u32::from_be_bytes(*b"APPL"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.set_launched_app_path("Gridz Demo ƒ/Gridz™ Demo");
    let psn_ptr = PPC_DATA_BASE + 0x1000;
    let info_ptr = PPC_DATA_BASE + 0x1100;
    let name_ptr = PPC_DATA_BASE + 0x1200;
    let spec_ptr = PPC_DATA_BASE + 0x1300;
    loaded.memory.add_region(psn_ptr, vec![0; 8]);
    loaded.memory.add_region(info_ptr, vec![0; 60]);
    loaded.memory.add_region(name_ptr, vec![0xaa; 64]);
    loaded.memory.add_region(spec_ptr, vec![0xbb; 70]);
    loaded
        .memory
        .write_u32_be(psn_ptr, ProcessSerialNumber::CURRENT.high)
        .unwrap();
    loaded
        .memory
        .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low)
        .unwrap();
    loaded.memory.write_u16_be(info_ptr, 60).unwrap();
    loaded.memory.write_u32_be(info_ptr + 4, name_ptr).unwrap();
    loaded.memory.write_u32_be(info_ptr + 56, spec_ptr).unwrap();
    loaded.cpu.gpr[3] = psn_ptr;
    loaded.cpu.gpr[4] = info_ptr;
    let gridz_demo_name = [
        b'G', b'r', b'i', b'd', b'z', 0xAA, b' ', b'D', b'e', b'm', b'o',
    ];

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr + 8),
        Some(ProcessSerialNumber::CURRENT.high)
    );
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr + 12),
        Some(ProcessSerialNumber::CURRENT.low)
    );
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr + 16),
        Some(u32::from_be_bytes(*b"APPL"))
    );
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr + 20),
        Some(u32::from_be_bytes(*b"Grid"))
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
        Some(&gridz_demo_name[..])
    );
    assert_eq!(
        loaded.memory.read_u16_be(spec_ptr),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(loaded.memory.read_u32_be(spec_ptr + 2), Some(100));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, spec_ptr + 6).as_deref(),
        Some(&gridz_demo_name[..])
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = psn_ptr;
    loaded.cpu.gpr[4] = info_ptr;
    loaded
        .memory
        .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low + 1)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
}
