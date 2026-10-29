use super::*;

const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;

fn data_file(path: &str, data: &[u8], creator: &[u8; 4]) -> PpcVfsFileRecord {
    PpcVfsFileRecord {
        path: path.to_string(),
        data: data.to_vec().into(),
        creator: u32::from_be_bytes(*creator),
        file_type: u32::from_be_bytes(*b"TEXT"),
        finder_flags: 0,
        dirty: false,
    }
}

fn resource_fork(path: &str, bytes: &[u8]) -> PpcVfsResourceFileRecord {
    PpcVfsResourceFileRecord {
        path: path.to_string(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        resource_len: bytes.len() as u32,
        raw_data: Some(bytes.to_vec().into()),
        map_attrs: 0,
        dirty: false,
    }
}

fn loaded_with(symbol: &[u8]) -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
    loaded.memory.add_region(SCRATCH, vec![0; 0x400]);
    loaded.seed_vfs_directories(initial_ppc_vfs_directories(), PPC_ROOT_DIR_ID, 0);
    loaded
}

fn run(loaded: &mut PpcLoadedApp) -> i16 {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    loaded.cpu.gpr[3] as u16 as i16
}

#[test]
fn file_manager_imports_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("CloseWD", PpcFileManagerOperation::CloseWd),
        ("CreateResFile", PpcFileManagerOperation::CreateResFile),
        ("CatMove", PpcFileManagerOperation::CatMove),
        ("FSpCatMove", PpcFileManagerOperation::FSpCatMove),
        (
            "FSpExchangeFiles",
            PpcFileManagerOperation::FSpExchangeFiles,
        ),
        ("HOpenRF", PpcFileManagerOperation::HOpenRf),
        (
            "PBOpenRFSync",
            PpcFileManagerOperation::PbOpenRf {
                hierarchical: false,
                deny_modes: false,
            },
        ),
        (
            "PBHOpenRFSync",
            PpcFileManagerOperation::PbOpenRf {
                hierarchical: true,
                deny_modes: false,
            },
        ),
        (
            "PBHOpenRFDenySync",
            PpcFileManagerOperation::PbOpenRf {
                hierarchical: true,
                deny_modes: true,
            },
        ),
        ("PBHCopyFileSync", PpcFileManagerOperation::PbHCopyFile),
        ("PBHSetFLockSync", PpcFileManagerOperation::PbHSetFLock),
        ("PBHRstFLockSync", PpcFileManagerOperation::PbHRstFLock),
        ("PBDTOpenInform", PpcFileManagerOperation::PbDtOpenInform),
        (
            "PBDTSetCommentSync",
            PpcFileManagerOperation::PbDtSetComment,
        ),
        ("PBEject", PpcFileManagerOperation::PbEject),
        ("PBMountVol", PpcFileManagerOperation::PbMountVol),
        ("fsdelete", PpcFileManagerOperation::FsDeleteCString),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::FileManager(operation),
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBOpenDFSync"),
        PpcImportDispatcherTarget::PBOpen
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "sfputfile"),
        PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::SfPutFileCString
        )
    );
}

#[test]
fn hopenrf_opens_resource_fork_by_directory_and_name() {
    let mut loaded = loaded_with(b"HOpenRF");
    let name = SCRATCH;
    let ref_out = SCRATCH + 0x80;
    write_ppc_pstring(&mut loaded.memory, name, b"Forked");
    loaded.push_test_vfs_file(data_file("System Folder/Forked", b"data", b"ttxt"));
    loaded.push_vfs_resource_file(resource_fork("System Folder/Forked", b"fork"));
    loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
    loaded.cpu.gpr[4] = PPC_SYSTEM_FOLDER_DIR_ID;
    loaded.cpu.gpr[5] = name;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = ref_out;

    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let ref_num = loaded.memory.read_u16_be(ref_out).unwrap() as i16;
    let open = loaded
        .files
        .iter()
        .find(|file| file.ref_num == ref_num)
        .unwrap();
    assert_eq!(
        open.path,
        format!("{PPC_OPEN_RESOURCE_FORK_PREFIX}System Folder/Forked")
    );
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == open.path && file.data == b"fork"));
}

#[test]
fn pb_open_rf_variants_report_results_in_the_parameter_block() {
    let mut loaded = loaded_with(b"PBHOpenRFDenySync");
    let pb = SCRATCH;
    let name = SCRATCH + 0x100;
    write_ppc_pstring(&mut loaded.memory, name, b"Forked");
    loaded.push_vfs_resource_file(resource_fork("System Folder/Forked", b"fork"));
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 48, PPC_SYSTEM_FOLDER_DIR_ID)
        .unwrap();
    loaded.cpu.gpr[3] = pb;
    // ioDenyModes requesting neither read nor write access is invalid.
    assert_eq!(run(&mut loaded), PPC_PARAM_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(pb + 16),
        Some(PPC_PARAM_ERR as u16)
    );

    loaded.memory.write_u16_be(pb + 26, 0x0011).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let ref_num = loaded.memory.read_u16_be(pb + 24).unwrap() as i16;
    assert!(loaded.files.iter().any(|file| file.ref_num == ref_num));

    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "PBOpenRFSync");
    write_ppc_pstring(&mut loaded.memory, name, b"Absent");
    loaded.memory.write_u16_be(pb + 22, 0).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_FNF_ERR);
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_FNF_ERR as u16));
}

#[test]
fn fsp_cat_move_moves_files_and_rejects_bad_moves() {
    let mut loaded = loaded_with(b"FSpCatMove");
    let source = SCRATCH;
    let dest = SCRATCH + 0x80;
    loaded.push_test_vfs_file(data_file("Notes", b"notes", b"ttxt"));
    loaded.push_vfs_resource_file(resource_fork("Notes", b"fork"));
    write_ppc_fsspec(&mut loaded.memory, source, 0, PPC_ROOT_DIR_ID, b"Notes");
    write_ppc_fsspec(
        &mut loaded.memory,
        dest,
        0,
        PPC_SYSTEM_FOLDER_DIR_ID,
        b"Preferences",
    );
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = dest;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "System Folder/Preferences/Notes"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path == "System Folder/Preferences/Notes"));
    assert!(!loaded.vfs_files.iter().any(|file| file.path == "Notes"));

    // A directory cannot move into its own subtree.
    write_ppc_fsspec(
        &mut loaded.memory,
        source,
        0,
        PPC_ROOT_DIR_ID,
        b"System Folder",
    );
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = dest;
    assert_eq!(run(&mut loaded), -122);

    // A missing source is fnfErr.
    write_ppc_fsspec(&mut loaded.memory, source, 0, PPC_ROOT_DIR_ID, b"Absent");
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = dest;
    assert_eq!(run(&mut loaded), PPC_FNF_ERR);
}

#[test]
fn fsp_exchange_files_swaps_forks_but_keeps_catalog_information() {
    let mut loaded = loaded_with(b"FSpExchangeFiles");
    let first = SCRATCH;
    let second = SCRATCH + 0x80;
    loaded.push_test_vfs_file(data_file("First", b"one", b"AAAA"));
    loaded.push_vfs_resource_file(resource_fork("First", b"first fork"));
    loaded.push_test_vfs_file(data_file("Second", b"two", b"BBBB"));
    write_ppc_fsspec(&mut loaded.memory, first, 0, PPC_ROOT_DIR_ID, b"First");
    write_ppc_fsspec(&mut loaded.memory, second, 0, PPC_ROOT_DIR_ID, b"Second");
    loaded.cpu.gpr[3] = first;
    loaded.cpu.gpr[4] = second;

    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let file = |path: &str| {
        loaded
            .vfs_files
            .iter()
            .find(|file| file.path == path)
            .cloned()
            .unwrap()
    };
    assert_eq!(file("First").data, b"two");
    assert_eq!(file("First").creator, u32::from_be_bytes(*b"AAAA"));
    assert_eq!(file("Second").data, b"one");
    assert_eq!(file("Second").creator, u32::from_be_bytes(*b"BBBB"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|fork| fork.path == "Second"));
    assert!(!loaded
        .vfs_resource_files
        .iter()
        .any(|fork| fork.path == "First"));
    assert!(!loaded
        .take_deleted_vfs_file_paths()
        .iter()
        .any(|path| path.starts_with('\0')));

    write_ppc_fsspec(
        &mut loaded.memory,
        second,
        0,
        PPC_ROOT_DIR_ID,
        b"System Folder",
    );
    loaded.cpu.gpr[3] = first;
    loaded.cpu.gpr[4] = second;
    assert_eq!(run(&mut loaded), -1302);
}

#[test]
fn pbh_copy_file_duplicates_both_forks() {
    let mut loaded = loaded_with(b"PBHCopyFileSync");
    let pb = SCRATCH;
    let name = SCRATCH + 0x100;
    let copy_name = SCRATCH + 0x140;
    loaded.push_test_vfs_file(data_file("Original", b"payload", b"CCCC"));
    loaded.push_vfs_resource_file(resource_fork("Original", b"resources"));
    write_ppc_pstring(&mut loaded.memory, name, b"Original");
    write_ppc_pstring(&mut loaded.memory, copy_name, b"Copy");
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded.memory.write_u32_be(pb + 32, copy_name).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 36, PPC_SYSTEM_FOLDER_DIR_ID)
        .unwrap();
    loaded
        .memory
        .write_u32_be(pb + 48, PPC_ROOT_DIR_ID)
        .unwrap();
    loaded.cpu.gpr[3] = pb;

    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let copy = loaded
        .vfs_files
        .iter()
        .find(|file| file.path == "System Folder/Copy")
        .cloned()
        .unwrap();
    assert_eq!(copy.data, b"payload");
    assert_eq!(copy.creator, u32::from_be_bytes(*b"CCCC"));
    let fork = loaded
        .vfs_resource_files
        .iter()
        .find(|file| file.path == "System Folder/Copy")
        .cloned()
        .unwrap();
    assert_eq!(fork.raw_data.as_ref().unwrap(), b"resources");
    assert!(loaded.vfs_files.iter().any(|file| file.path == "Original"));

    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_DUP_FN_ERR);
    assert_eq!(
        loaded.memory.read_u16_be(pb + 16),
        Some(PPC_DUP_FN_ERR as u16)
    );
}

#[test]
fn working_directory_volume_and_lock_calls_follow_the_runtime_model() {
    let mut loaded = loaded_with(b"CloseWD");
    loaded.cpu.gpr[3] = 0x7123;
    assert_eq!(run(&mut loaded), PPC_RF_NUM_ERR);
    loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);

    let pb = SCRATCH;
    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "PBMountVol");
    loaded.memory.write_u16_be(pb + 22, 1).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), -55);
    loaded.memory.write_u16_be(pb + 22, 9).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), -56);

    loaded.imports[0].dispatcher_target = dispatcher_target_for_import("InterfaceLib", "PBEject");
    loaded
        .memory
        .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(0));

    let name = SCRATCH + 0x100;
    loaded.push_test_vfs_file(data_file("Locked", b"", b"LLLL"));
    write_ppc_pstring(&mut loaded.memory, name, b"Locked");
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 48, PPC_ROOT_DIR_ID)
        .unwrap();
    for symbol in ["PBHSetFLockSync", "PBHRstFLockSync"] {
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import("InterfaceLib", symbol);
        loaded.cpu.gpr[3] = pb;
        assert_eq!(run(&mut loaded), PPC_NO_ERR, "{symbol}");
    }
    write_ppc_pstring(&mut loaded.memory, name, b"Missing");
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_FNF_ERR);
}

#[test]
fn create_res_file_and_fsdelete_use_the_default_directory() {
    let mut loaded = loaded_with(b"CreateResFile");
    let name = SCRATCH;
    write_ppc_pstring(&mut loaded.memory, name, b"Fresh");
    loaded.cpu.gpr[3] = name;
    run(&mut loaded);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::RES_ERR),
        Some(0)
    );
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path.ends_with("Fresh")));

    loaded.imports[0].dispatcher_target = dispatcher_target_for_import("InterfaceLib", "fsdelete");
    let c_name = SCRATCH + 0x80;
    for (offset, byte) in b"Fresh\0".iter().enumerate() {
        loaded
            .memory
            .write_u8(c_name + offset as u32, *byte)
            .unwrap();
    }
    loaded.cpu.gpr[3] = c_name;
    loaded.cpu.gpr[4] = 0;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    assert!(!loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path.ends_with("Fresh")));
    assert_eq!(ppc_std_c_string(&mut loaded.memory, c_name, 64), b"Fresh");
}

#[test]
fn desktop_database_open_inform_and_set_comment() {
    let mut loaded = loaded_with(b"PBDTOpenInform");
    let pb = SCRATCH;
    loaded.memory.write_u32_be(pb + 28, 0xffff_ffff).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let desktop_ref = loaded.memory.read_u16_be(pb + 24).unwrap();
    assert_eq!(loaded.memory.read_u32_be(pb + 28), Some(0));

    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "PBDTSetCommentSync");
    let name = SCRATCH + 0x100;
    loaded.push_test_vfs_file(data_file("Commented", b"", b"DDDD"));
    write_ppc_pstring(&mut loaded.memory, name, b"Commented");
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 48, PPC_ROOT_DIR_ID)
        .unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    loaded.memory.write_u16_be(pb + 24, 0x1234).unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_RF_NUM_ERR);
    loaded.memory.write_u16_be(pb + 24, desktop_ref).unwrap();
    write_ppc_pstring(&mut loaded.memory, name, b"Nowhere");
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_FNF_ERR);
}

#[test]
fn sfputfile_reads_c_strings_and_returns_a_c_string_file_name() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"sfputfile")).unwrap();
    let reply = PPC_DATA_BASE + 0x1400;
    let strings = PPC_DATA_BASE + 0x1500;
    loaded.memory.add_region(reply, vec![0xaa; 75]);
    loaded.memory.add_region(strings, vec![0; 0x100]);
    let where_ptr = strings;
    let prompt = strings + 0x10;
    let name = strings + 0x40;
    loaded.memory.write_u32_be(where_ptr, 0xffff_ffff).unwrap();
    for (base, text) in [(prompt, &b"Save game:\0"[..]), (name, &b"Game 1\0"[..])] {
        for (offset, byte) in text.iter().enumerate() {
            loaded.memory.write_u8(base + offset as u32, *byte).unwrap();
        }
    }
    loaded.cpu.gpr[3] = where_ptr;
    loaded.cpu.gpr[4] = prompt;
    loaded.cpu.gpr[5] = name;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = reply;
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let tracking = loaded
        .toolbox_startup
        .standard_file_put_tracking
        .clone()
        .unwrap();
    assert_eq!(tracking.name, b"Game 1");
    assert_eq!(tracking.prompt, b"Save game:");
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (u32::from(PPC_KEY_RETURN) << 8) | 0x0d,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    }]);
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.memory.read_u8(reply), Some(1));
    assert_eq!(
        ppc_std_c_string(&mut loaded.memory, reply + 10, 64),
        b"Game 1"
    );
}

#[test]
fn cat_move_resolves_named_and_empty_destinations_and_keeps_both_forks() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CatMove"),
        PpcImportDispatcherTarget::FileManager(PpcFileManagerOperation::CatMove),
    );
    let mut loaded = loaded_with(b"CatMove");
    loaded.push_test_vfs_file(data_file("Notes", b"notes", b"ttxt"));
    loaded.push_vfs_resource_file(resource_fork("Notes", b"fork"));
    write_ppc_pstring(&mut loaded.memory, SCRATCH, b"Notes");
    write_ppc_pstring(&mut loaded.memory, SCRATCH + 0x80, b"Preferences");
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        0,
        PPC_ROOT_DIR_ID,
        SCRATCH,
        PPC_SYSTEM_FOLDER_DIR_ID,
        SCRATCH + 0x80,
    ]);
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let moved = "System Folder/Preferences/Notes";
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == moved && file.data.as_ref() == b"notes"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path == moved));
    // Resolve a zero source directory ID through its working-directory ref.
    loaded.working_directories.with_mut(|directories| {
        directories.insert(
            77,
            ProcessWorkingDirectory {
                ref_num: 77,
                volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
                dir_id: PPC_PREFERENCES_DIR_ID,
                proc_id: 0,
            },
        );
    });
    // A destination named by ID takes an empty Pascal string.
    write_ppc_pstring(&mut loaded.memory, SCRATCH + 0x80, b"");
    loaded.cpu.gpr[3..8].copy_from_slice(&[77, 0, SCRATCH, PPC_ROOT_DIR_ID, SCRATCH + 0x80]);
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    assert!(loaded.vfs_files.iter().any(|file| file.path == "Notes"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path == "Notes"));
    assert!(!loaded.vfs_files.iter().any(|file| file.path == moved));
}

#[test]
fn cat_move_moves_directory_descendants_and_rejects_collisions_without_mutation() {
    let mut loaded = loaded_with(b"CatMove");
    loaded.push_test_vfs_file(data_file(
        "System Folder/Preferences/Settings",
        b"settings",
        b"TEST",
    ));
    loaded.push_vfs_resource_file(resource_fork(
        "System Folder/Preferences/Settings",
        b"resource",
    ));
    // Empty source and destination names identify both directories by ID.
    write_ppc_pstring(&mut loaded.memory, SCRATCH, b"");
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        0,
        PPC_PREFERENCES_DIR_ID,
        SCRATCH,
        PPC_ROOT_DIR_ID,
        SCRATCH,
    ]);
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let directory = loaded
        .vfs_directories
        .iter()
        .find(|dir| dir.dir_id == PPC_PREFERENCES_DIR_ID)
        .unwrap();
    assert_eq!(directory.path, "Preferences");
    assert_eq!(directory.parent_dir_id, PPC_ROOT_DIR_ID);
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Preferences/Settings"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path == "Preferences/Settings"));

    loaded.push_test_vfs_file(data_file("Settings", b"original", b"TEST"));
    write_ppc_pstring(&mut loaded.memory, SCRATCH + 0x80, b"Settings");
    loaded.cpu.gpr[3..8].copy_from_slice(&[
        0,
        PPC_ROOT_DIR_ID,
        SCRATCH + 0x80,
        PPC_PREFERENCES_DIR_ID,
        SCRATCH,
    ]);
    assert_eq!(run(&mut loaded), PPC_DUP_FN_ERR);
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Settings" && file.data.as_ref() == b"original"));
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "Preferences/Settings" && file.data.as_ref() == b"settings"));

    loaded.cpu.gpr[3..8].copy_from_slice(&[
        0,
        PPC_PREFERENCES_DIR_ID,
        SCRATCH,
        PPC_PREFERENCES_DIR_ID,
        SCRATCH,
    ]);
    assert_eq!(run(&mut loaded), -122);
}

#[test]
fn pb_cat_move_uses_catalog_fields_and_reports_io_result() {
    let target = PpcImportDispatcherTarget::FileManager(PpcFileManagerOperation::PbCatMove);
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PBCatMoveSync"),
        target
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "PBCatMoveSync"),
        target
    );
    let mut loaded = loaded_with(b"PBCatMoveSync");
    let pb = SCRATCH;
    let name = SCRATCH + 0x80;
    let destination = SCRATCH + 0xc0;
    loaded.push_test_vfs_file(data_file("Notes", b"notes", b"ttxt"));
    loaded.push_vfs_resource_file(resource_fork("Notes", b"fork"));
    write_ppc_pstring(&mut loaded.memory, name, b"Notes");
    write_ppc_pstring(&mut loaded.memory, destination, b"Preferences");
    loaded.memory.write_u32_be(pb + 18, name).unwrap();
    loaded.memory.write_u16_be(pb + 22, 0).unwrap();
    loaded.memory.write_u32_be(pb + 28, destination).unwrap();
    loaded
        .memory
        .write_u32_be(pb + 36, PPC_SYSTEM_FOLDER_DIR_ID)
        .unwrap();
    loaded
        .memory
        .write_u32_be(pb + 48, PPC_ROOT_DIR_ID)
        .unwrap();
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(0));
    assert!(loaded
        .vfs_files
        .iter()
        .any(|file| file.path == "System Folder/Preferences/Notes"));
    assert!(loaded
        .vfs_resource_files
        .iter()
        .any(|file| file.path == "System Folder/Preferences/Notes"));
    loaded.cpu.gpr[3] = pb;
    assert_eq!(run(&mut loaded), PPC_FNF_ERR);
    assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_FNF_ERR as u16));
}

#[test]
fn new_alias_minimal_allocates_independent_records_and_clears_failed_output() {
    let target = PpcImportDispatcherTarget::NewAliasMinimal;
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewAliasMinimal"),
        target
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "NewAliasMinimal"),
        target
    );
    let mut loaded = loaded_with(b"NewAliasMinimal");
    let spec = SCRATCH;
    let out = SCRATCH + 0x80;
    write_ppc_fsspec(
        &mut loaded.memory,
        spec,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"Notes",
    );
    loaded.cpu.gpr[3] = spec;
    loaded.cpu.gpr[4] = out;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let first = loaded.memory.read_u32_be(out).unwrap();
    assert_ne!(first, 0);
    loaded.cpu.gpr[3] = spec;
    loaded.cpu.gpr[4] = out + 4;
    assert_eq!(run(&mut loaded), PPC_NO_ERR);
    let second = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_ne!(first, second);
    assert_eq!(loaded.aliases.len(), 2);
    assert_eq!(loaded.aliases[0].target_name, b"Notes");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = out;
    assert_eq!(run(&mut loaded), PPC_PARAM_ERR);
    assert_eq!(loaded.memory.read_u32_be(out), Some(0));
}
