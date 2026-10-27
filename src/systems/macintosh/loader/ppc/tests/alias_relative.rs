use super::*;

const SOURCE: u32 = PPC_DATA_BASE + 0x1000;
const TARGET: u32 = SOURCE + 80;
const CHANGED: u32 = SOURCE + 160;
const FROM_DIR: u32 = 301;
const TO_DIR: u32 = 302;
const OTHER_DIR: u32 = 303;

// Independent classic AliasRecord layout: the parent catalog ID is at46,
// while the target's own catalog ID is at114. Do not use implementation offsets.
fn alias_bytes(parent: u32, from: i16, to: i16, full_path: &[u8]) -> Vec<u8> {
    let padded = (full_path.len() + 1) & !1;
    let size = 150 + 4 + padded + 4;
    let mut data = vec![0; size];
    data[4..6].copy_from_slice(&(size as u16).to_be_bytes());
    data[6..8].copy_from_slice(&2u16.to_be_bytes());
    data[46..50].copy_from_slice(&parent.to_be_bytes());
    data[50] = 9;
    data[51..60].copy_from_slice(b"Model.dat");
    data[114..118].copy_from_slice(&0x1234_5678u32.to_be_bytes());
    data[130..132].copy_from_slice(&from.to_be_bytes());
    data[132..134].copy_from_slice(&to.to_be_bytes());
    data[150..152].copy_from_slice(&2u16.to_be_bytes());
    data[152..154].copy_from_slice(&(full_path.len() as u16).to_be_bytes());
    data[154..154 + full_path.len()].copy_from_slice(full_path);
    data[size - 4..size - 2].copy_from_slice(&0xffffu16.to_be_bytes());
    data
}

fn fixture(data: &[u8], target_paths: &[&str]) -> (PpcLoadedApp, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ResolveAlias")).unwrap();
    let mut directories = initial_ppc_vfs_directories();
    for (dir_id, parent_dir_id, path) in [
        (300, PPC_ROOT_DIR_ID, "Moved"),
        (FROM_DIR, 300, "Moved/From"),
        (TO_DIR, 300, "Moved/To"),
        (OTHER_DIR, PPC_ROOT_DIR_ID, "Elsewhere"),
    ] {
        directories.push(PpcVfsDirectory {
            dir_id,
            parent_dir_id,
            path: path.to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
    }
    loaded.seed_vfs_directories(directories, FROM_DIR, 304);
    for path in std::iter::once("Moved/From/Source.dat").chain(target_paths.iter().copied()) {
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: path.to_string(),
            data: vec![1, 2, 3].into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
    }
    loaded.memory.add_region(SOURCE, vec![0xaa; 192]);
    write_ppc_fsspec(
        &mut loaded.memory,
        SOURCE,
        PPC_BOOT_VOLUME_REF_NUM,
        FROM_DIR,
        b"Source.dat",
    );
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        data,
    );
    assert_ne!(handle, 0);
    (loaded, handle)
}

fn resolve(loaded: &mut PpcLoadedApp, handle: u32, source: u32) {
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = handle;
    loaded.cpu.gpr[5] = TARGET;
    loaded.cpu.gpr[6] = CHANGED;
    run_test_import(loaded, PpcImportDispatcherTarget::ResolveAlias);
}

#[test]
fn classic_alias_parent_id_is_distinct_from_target_file_id() {
    let bytes = alias_bytes(OTHER_DIR, -1, -1, b"OldDisk:Unavailable:Model.dat");
    let (mut loaded, handle) = fixture(&bytes, &["Elsewhere/Model.dat"]);
    resolve(&mut loaded, handle, 0);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(TARGET + 2), Some(OTHER_DIR));
    loaded.cpu.gpr[3] = TARGET;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = CHANGED + 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FSpOpenDF);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn moved_sibling_alias_prefers_relative_file_over_absolute_or_duplicate_basename() {
    let bytes = alias_bytes(OTHER_DIR, 1, 1, b"OldDisk:Unavailable:Model.dat");
    let (mut loaded, handle) = fixture(&bytes, &["Moved/From/Model.dat", "Elsewhere/Model.dat"]);
    resolve(&mut loaded, handle, SOURCE);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(TARGET + 2), Some(FROM_DIR));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, TARGET + 6),
        Some(b"Model.dat".to_vec())
    );
}

#[test]
fn moved_nested_alias_uses_saved_relative_levels_and_target_path_suffix() {
    let bytes = alias_bytes(OTHER_DIR, 2, 2, b"OldDisk:Original:To:Model.dat");
    let (mut loaded, handle) = fixture(&bytes, &["Moved/To/Model.dat", "Elsewhere/Model.dat"]);
    resolve(&mut loaded, handle, SOURCE);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(TARGET + 2), Some(TO_DIR));
}

#[test]
fn missing_relative_candidate_falls_back_without_using_an_unrelated_basename() {
    let bytes = alias_bytes(OTHER_DIR, 2, 2, b"OldDisk:Original:Missing:Model.dat");
    let (mut loaded, handle) = fixture(&bytes, &["Moved/To/Model.dat", "Elsewhere/Model.dat"]);
    resolve(&mut loaded, handle, SOURCE);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(TARGET + 2), Some(OTHER_DIR));
}

#[test]
fn truncated_alias_does_not_publish_a_relative_target() {
    let mut bytes = alias_bytes(OTHER_DIR, 1, 1, b"OldDisk:Original:Model.dat");
    bytes.truncate(149);
    let (mut loaded, handle) = fixture(&bytes, &["Moved/From/Model.dat"]);
    resolve(&mut loaded, handle, SOURCE);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(TARGET + 2), Some(0xaaaa_aaaa));
    assert_eq!(loaded.memory.read_u8(CHANGED), Some(0xaa));
}
