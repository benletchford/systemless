use super::*;

const BASE: u32 = PPC_DATA_BASE + 0x1000;
const PREFS_FILE: &str = "System Folder/Preferences/Game Prefs";

fn fsref_app() -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FSGetCatalogInfo")).unwrap();
    loaded.memory.add_region(BASE, vec![0; 0x1000]);
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: PREFS_FILE.to_string(),
        data: b"0123456789".to_vec().into(),
        creator: u32::from_be_bytes(*b"GAME"),
        file_type: u32::from_be_bytes(*b"pref"),
        finder_flags: 0x0100,
        dirty: false,
    });
    loaded
}

fn call(loaded: &mut PpcLoadedApp, operation: PpcFsRefOperation, args: &[u32]) -> i16 {
    for (index, value) in args.iter().enumerate() {
        loaded.cpu.gpr[3 + index] = *value;
    }
    run_test_import(loaded, PpcImportDispatcherTarget::FsRef(operation));
    loaded.cpu.gpr[3] as u16 as i16
}

fn write_spec(loaded: &mut PpcLoadedApp, spec: u32, dir_id: u32, name: &[u8]) {
    loaded
        .memory
        .write_u16_be(spec, PPC_BOOT_VOLUME_REF_NUM as u16)
        .unwrap();
    loaded.memory.write_u32_be(spec + 2, dir_id).unwrap();
    write_ppc_pstring(&mut loaded.memory, spec + 6, name);
}

/// Writes UTF-16 BE UniChars and returns the UniCharCount.
fn write_unichars(loaded: &mut PpcLoadedApp, addr: u32, text: &str) -> u32 {
    let units: Vec<u16> = text.encode_utf16().collect();
    for (index, unit) in units.iter().enumerate() {
        loaded
            .memory
            .write_u16_be(addr + 2 * index as u32, *unit)
            .unwrap();
    }
    units.len() as u32
}

fn read_uni_str255(loaded: &mut PpcLoadedApp, addr: u32) -> String {
    let len = loaded.memory.read_u16_be(addr).unwrap();
    let units: Vec<u16> = (0..u32::from(len))
        .map(|index| loaded.memory.read_u16_be(addr + 2 + 2 * index).unwrap())
        .collect();
    String::from_utf16(&units).unwrap()
}

fn assert_fsref(loaded: &mut PpcLoadedApp, fsref: u32, parent_dir_id: u32, name: &[u8]) {
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, fsref, 4).as_deref(),
        Some(&b"SyFR"[..])
    );
    assert_eq!(
        loaded.memory.read_u16_be(fsref + 4),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(loaded.memory.read_u32_be(fsref + 6), Some(parent_dir_id));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, fsref + 10).as_deref(),
        Some(name)
    );
}

fn make_ref(loaded: &mut PpcLoadedApp, fsref: u32, dir_id: u32, name: &[u8]) {
    let spec = BASE + 0xf00;
    write_spec(loaded, spec, dir_id, name);
    assert_eq!(
        call(loaded, PpcFsRefOperation::FspMakeFsRef, &[spec, fsref]),
        PPC_NO_ERR
    );
}

#[test]
fn fsref_symbols_resolve_from_interface_lib_and_carbon_lib() {
    for (symbol, operation) in [
        ("FSpMakeFSRef", PpcFsRefOperation::FspMakeFsRef),
        ("FSMakeFSRefUnicode", PpcFsRefOperation::MakeFsRefUnicode),
        ("FSCompareFSRefs", PpcFsRefOperation::CompareFsRefs),
        ("FSGetCatalogInfo", PpcFsRefOperation::GetCatalogInfo),
        ("FSSetCatalogInfo", PpcFsRefOperation::SetCatalogInfo),
        ("FSOpenFork", PpcFsRefOperation::OpenFork),
        ("FSReadFork", PpcFsRefOperation::ReadFork),
        ("FSWriteFork", PpcFsRefOperation::WriteFork),
        ("FSCloseFork", PpcFsRefOperation::CloseFork),
        ("FSGetForkPosition", PpcFsRefOperation::GetForkPosition),
        ("FSSetForkPosition", PpcFsRefOperation::SetForkPosition),
        ("FSGetForkSize", PpcFsRefOperation::GetForkSize),
        ("FSSetForkSize", PpcFsRefOperation::SetForkSize),
        ("FSGetDataForkName", PpcFsRefOperation::GetDataForkName),
        (
            "FSGetResourceForkName",
            PpcFsRefOperation::GetResourceForkName,
        ),
        ("FSDeleteObject", PpcFsRefOperation::DeleteObject),
        ("FSCreateFileUnicode", PpcFsRefOperation::CreateFileUnicode),
        (
            "FSCreateDirectoryUnicode",
            PpcFsRefOperation::CreateDirectoryUnicode,
        ),
        ("FSRenameUnicode", PpcFsRefOperation::RenameUnicode),
        ("FSFindFolder", PpcFsRefOperation::FindFolder),
        ("FSResolveAliasFile", PpcFsRefOperation::ResolveAliasFile),
        (
            "FSResolveAliasFileWithMountFlags",
            PpcFsRefOperation::ResolveAliasFile,
        ),
    ] {
        for library in ["InterfaceLib", "CarbonLib"] {
            assert_eq!(
                dispatcher_target_for_import(library, symbol),
                PpcImportDispatcherTarget::FsRef(operation),
                "{library} {symbol}"
            );
        }
    }
}

#[test]
fn fsp_make_fsref_and_get_catalog_info_describe_a_file() {
    let mut loaded = fsref_app();
    let (fsref, info, name, spec, parent) =
        (BASE, BASE + 0x80, BASE + 0x200, BASE + 0x400, BASE + 0x480);
    make_ref(&mut loaded, fsref, PPC_PREFERENCES_DIR_ID, b"game prefs");
    assert_fsref(&mut loaded, fsref, PPC_PREFERENCES_DIR_ID, b"Game Prefs");

    // NodeFlags | ParentDirID | NodeID | FinderInfo | DataSizes.
    let which = 0x0002 | 0x0008 | 0x0010 | 0x0800 | 0x4000;
    let result = call(
        &mut loaded,
        PpcFsRefOperation::GetCatalogInfo,
        &[fsref, which, info, name, spec, parent],
    );

    assert_eq!(result, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(info), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(info + 4),
        Some(PPC_PREFERENCES_DIR_ID)
    );
    assert_eq!(
        loaded.memory.read_u32_be(info + 72),
        Some(u32::from_be_bytes(*b"pref"))
    );
    assert_eq!(
        loaded.memory.read_u32_be(info + 76),
        Some(u32::from_be_bytes(*b"GAME"))
    );
    assert_eq!(loaded.memory.read_u16_be(info + 80), Some(0x0100));
    assert_eq!(loaded.memory.read_u64_be(info + 104), Some(10));
    assert_eq!(loaded.memory.read_u64_be(info + 112), Some(512));
    assert_eq!(read_uni_str255(&mut loaded, name), "Game Prefs");
    assert_eq!(
        loaded.memory.read_u32_be(spec + 2),
        Some(PPC_PREFERENCES_DIR_ID)
    );
    assert_fsref(
        &mut loaded,
        parent,
        PPC_SYSTEM_FOLDER_DIR_ID,
        b"Preferences",
    );

    // A directory reports kFSNodeIsDirectoryMask and its valence.
    make_ref(&mut loaded, fsref, PPC_SYSTEM_FOLDER_DIR_ID, b"Preferences");
    let result = call(
        &mut loaded,
        PpcFsRefOperation::GetCatalogInfo,
        &[fsref, 0x0002 | 0x0010 | 0x2000, info, 0, 0, 0],
    );
    assert_eq!(result, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u16_be(info), Some(0x0010));
    assert_eq!(
        loaded.memory.read_u32_be(info + 8),
        Some(PPC_PREFERENCES_DIR_ID)
    );
    assert_eq!(loaded.memory.read_u32_be(info + 136), Some(1));

    let spec_ptr = BASE + 0xf00;
    write_spec(&mut loaded, spec_ptr, PPC_PREFERENCES_DIR_ID, b"Missing");
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::FspMakeFsRef,
            &[spec_ptr, fsref]
        ),
        PPC_FNF_ERR
    );
    loaded.memory.write_bytes(fsref, b"XXXX").unwrap();
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::GetCatalogInfo,
            &[fsref, which, info, 0, 0, 0]
        ),
        PPC_PARAM_ERR
    );
}

#[test]
fn fs_make_fsref_unicode_names_a_child_of_a_folder_ref() {
    let mut loaded = fsref_app();
    let (folder, child, name) = (BASE, BASE + 0x80, BASE + 0x100);
    make_ref(
        &mut loaded,
        folder,
        PPC_SYSTEM_FOLDER_DIR_ID,
        b"Preferences",
    );
    let len = write_unichars(&mut loaded, name, "Game Prefs");

    let result = call(
        &mut loaded,
        PpcFsRefOperation::MakeFsRefUnicode,
        &[folder, len, name, 0, child],
    );

    assert_eq!(result, PPC_NO_ERR);
    assert_fsref(&mut loaded, child, PPC_PREFERENCES_DIR_ID, b"Game Prefs");

    let len = write_unichars(&mut loaded, name, "Nothing");
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::MakeFsRefUnicode,
            &[folder, len, name, 0, child + 0x80]
        ),
        PPC_FNF_ERR
    );
    // errFSNotAFolder: a file cannot be the parent.
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::MakeFsRefUnicode,
            &[child, len, name, 0, child + 0x80]
        ),
        -1407
    );
}

#[test]
fn fs_open_fork_reads_the_data_fork_through_fs_read_fork() {
    let mut loaded = fsref_app();
    let (fsref, ref_num, buffer, actual, size) =
        (BASE, BASE + 0x80, BASE + 0x100, BASE + 0x180, BASE + 0x190);
    make_ref(&mut loaded, fsref, PPC_PREFERENCES_DIR_ID, b"Game Prefs");

    // FSOpenFork(ref, 0, NULL, fsRdPerm, &forkRefNum) opens the data fork.
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::OpenFork,
            &[fsref, 0, 0, 1, ref_num]
        ),
        PPC_NO_ERR
    );
    let fork = u32::from(loaded.memory.read_u16_be(ref_num).unwrap());

    // fsFromStart with the SInt64 offset 3 in r5:r6.
    let result = call(
        &mut loaded,
        PpcFsRefOperation::ReadFork,
        &[fork, 1, 0, 3, 4, buffer, actual],
    );
    assert_eq!(result, PPC_NO_ERR);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, buffer, 4).as_deref(),
        Some(&b"3456"[..])
    );
    assert_eq!(loaded.memory.read_u32_be(actual), Some(4));

    assert_eq!(
        call(&mut loaded, PpcFsRefOperation::GetForkSize, &[fork, size]),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u64_be(size), Some(10));

    // A short read at the mark returns eofErr with the bytes it read.
    let result = call(
        &mut loaded,
        PpcFsRefOperation::ReadFork,
        &[fork, 0, 0, 0, 8, buffer, actual],
    );
    assert_eq!(result, PPC_EOF_ERR);
    assert_eq!(loaded.memory.read_u32_be(actual), Some(3));
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::GetForkPosition,
            &[fork, size]
        ),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u64_be(size), Some(10));

    let name = BASE + 0x200;
    let len = write_unichars(&mut loaded, name, "OTHER_FORK");
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::OpenFork,
            &[fsref, len, name, 1, ref_num]
        ),
        -1409
    );
}

#[test]
fn fs_create_file_unicode_round_trips_through_fs_write_fork() {
    let mut loaded = fsref_app();
    let (folder, info, name, new_ref, new_spec) =
        (BASE, BASE + 0x80, BASE + 0x180, BASE + 0x200, BASE + 0x280);
    let (ref_num, data, actual) = (BASE + 0x300, BASE + 0x310, BASE + 0x320);
    make_ref(
        &mut loaded,
        folder,
        PPC_SYSTEM_FOLDER_DIR_ID,
        b"Preferences",
    );
    loaded
        .memory
        .write_u32_be(info + 72, u32::from_be_bytes(*b"SAVE"))
        .unwrap();
    loaded
        .memory
        .write_u32_be(info + 76, u32::from_be_bytes(*b"GAME"))
        .unwrap();
    let len = write_unichars(&mut loaded, name, "Saved Game");

    let result = call(
        &mut loaded,
        PpcFsRefOperation::CreateFileUnicode,
        &[folder, len, name, 0x0800, info, new_ref, new_spec],
    );

    assert_eq!(result, PPC_NO_ERR);
    assert_fsref(&mut loaded, new_ref, PPC_PREFERENCES_DIR_ID, b"Saved Game");
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, new_spec + 6).as_deref(),
        Some(&b"Saved Game"[..])
    );
    let created = loaded
        .vfs_files
        .iter()
        .find(|file| file.path == "System Folder/Preferences/Saved Game")
        .unwrap();
    assert_eq!(created.file_type, u32::from_be_bytes(*b"SAVE"));
    assert_eq!(created.creator, u32::from_be_bytes(*b"GAME"));

    // fsRdWrPerm, then write five bytes from the start and close.
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::OpenFork,
            &[new_ref, 0, 0, 3, ref_num]
        ),
        PPC_NO_ERR
    );
    let fork = u32::from(loaded.memory.read_u16_be(ref_num).unwrap());
    loaded.memory.write_bytes(data, b"hello").unwrap();
    let result = call(
        &mut loaded,
        PpcFsRefOperation::WriteFork,
        &[fork, 1, 0, 0, 5, data, actual],
    );
    assert_eq!(result, PPC_NO_ERR);
    assert_eq!(loaded.memory.read_u32_be(actual), Some(5));
    assert_eq!(
        call(&mut loaded, PpcFsRefOperation::CloseFork, &[fork]),
        PPC_NO_ERR
    );
    assert!(loaded.files.is_empty());
    let written = loaded
        .vfs_files
        .iter()
        .find(|file| file.path == "System Folder/Preferences/Saved Game")
        .unwrap();
    assert_eq!(&written.data[..], b"hello");
    assert_eq!(
        call(&mut loaded, PpcFsRefOperation::CloseFork, &[fork]),
        PPC_RF_NUM_ERR
    );
}

#[test]
fn fs_compare_fsrefs_matches_names_case_insensitively() {
    let mut loaded = fsref_app();
    let (first, second, third) = (BASE, BASE + 0x80, BASE + 0x100);
    make_ref(&mut loaded, first, PPC_PREFERENCES_DIR_ID, b"Game Prefs");
    make_ref(&mut loaded, third, PPC_SYSTEM_FOLDER_DIR_ID, b"Preferences");
    let bytes = ppc_memory_read_bytes(&mut loaded.memory, first, 80).unwrap();
    loaded.memory.write_bytes(second, &bytes).unwrap();
    write_ppc_pstring(&mut loaded.memory, second + 10, b"GAME PREFS");

    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::CompareFsRefs,
            &[first, second]
        ),
        PPC_NO_ERR
    );
    // errFSRefsDifferent.
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::CompareFsRefs,
            &[first, third]
        ),
        -1420
    );
    loaded.memory.write_u8(second, 0).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            PpcFsRefOperation::CompareFsRefs,
            &[first, second]
        ),
        PPC_PARAM_ERR
    );
}

#[test]
fn fs_fork_names_name_the_data_and_resource_forks() {
    let mut loaded = fsref_app();
    let name = BASE;
    loaded.memory.write_u16_be(name, 99).unwrap();

    assert_eq!(
        call(&mut loaded, PpcFsRefOperation::GetResourceForkName, &[name]),
        PPC_NO_ERR
    );
    assert_eq!(read_uni_str255(&mut loaded, name), "RESOURCE_FORK");
    assert_eq!(
        call(&mut loaded, PpcFsRefOperation::GetDataForkName, &[name]),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u16_be(name), Some(0));
}

#[test]
fn fs_find_folder_returns_the_preferences_folder_ref() {
    let mut loaded = fsref_app();
    let found = BASE;

    let result = call(
        &mut loaded,
        PpcFsRefOperation::FindFolder,
        &[
            PPC_BOOT_VOLUME_REF_NUM as u16 as u32,
            u32::from_be_bytes(*b"pref"),
            0,
            found,
        ],
    );

    assert_eq!(result, PPC_NO_ERR);
    assert_fsref(&mut loaded, found, PPC_SYSTEM_FOLDER_DIR_ID, b"Preferences");

    // The root directory names the volume inside fsRtParID.
    let result = call(
        &mut loaded,
        PpcFsRefOperation::FindFolder,
        &[
            PPC_BOOT_VOLUME_REF_NUM as u16 as u32,
            u32::from_be_bytes(*b"desk"),
            0,
            found,
        ],
    );
    assert_eq!(result, PPC_NO_ERR);
    let volume_name = crate::trap::TrapDispatcher::boot_volume_name();
    assert_fsref(&mut loaded, found, 1, volume_name.as_bytes());
}
